//! Independent bit-vector oracles, mask suppression, and precise faults.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K16, X86_VECTOR_STATE_K64};

pub(super) fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (index, word) in words.into_iter().enumerate() {
        bytes[index * 8..(index + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub(super) fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..(index + 1) * 8].try_into().unwrap())
    })
}

fn put_element(bytes: &mut [u8; 64], elem: VecElementType, lane: usize, value: u64) {
    let size = elem.bytes() as usize;
    bytes[lane * size..(lane + 1) * size].copy_from_slice(&value.to_le_bytes()[..size]);
}

fn get_element(bytes: &[u8; 64], elem: VecElementType, lane: usize) -> u64 {
    let size = elem.bytes() as usize;
    let mut value = [0u8; 8];
    value[..size].copy_from_slice(&bytes[lane * size..(lane + 1) * size]);
    u64::from_le_bytes(value)
}

pub(super) fn memory_bytes(case: BroadcastMemoryCase, seed: usize) -> [u8; 64] {
    // Includes signed zeros, infinities, NaN-like payloads, and alternating
    // integer patterns after truncation to each element width. Broadcasts are
    // raw bit transfers, so none of these patterns may update MXCSR.
    const PATTERNS: [u64; 8] = [
        0x0000_0000_0000_0000,
        0x8000_0000_8000_0000,
        0x7FF0_0000_7F80_0000,
        0xFFF0_0000_FF80_0000,
        0x7FF8_1234_7FC1_2345,
        0x7FF0_0001_7F80_0001,
        0x55AA_33CC_F00F_9669,
        0xFEDC_BA98_7654_3210,
    ];
    let mut bytes = [0xA5; 64];
    for lane in 0..usize::from(case.shape.source_lanes) {
        let value =
            PATTERNS[(seed + lane) % PATTERNS.len()] ^ (seed as u64).rotate_left((lane * 7) as u32);
        put_element(&mut bytes, case.shape.elem, lane, value);
    }
    bytes
}

fn lane_mask(case: BroadcastMemoryCase) -> u64 {
    let lanes = case.shape.destination_lanes();
    if lanes == 64 {
        u64::MAX
    } else {
        (1u64 << lanes) - 1
    }
}

pub(super) fn initial_registers(case: BroadcastMemoryCase, seed: usize, mask: u64) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x10)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: if !case.shape.needs_avx512bw && case.shape.destination_lanes() <= 16 {
            X86_VECTOR_STATE_K16
        } else {
            X86_VECTOR_STATE_K64
        },
        mxcsr: 0x1F80 | (((seed as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    registers.gpr[usize::from(case.base)] = MEMORY_ADDRESS;
    registers.k[usize::from(case.mask())] = mask;
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
        });
    }
    registers
}

fn interpreter_context(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    context.pc = PC;
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

fn interpreter_registers(context: &SmirContext, initial: &GuestRegs) -> GuestRegs {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut result = *initial;
    result.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        result.zmm[index].copy_from_slice(&value[..8]);
    }
    result.k = x86.k;
    result.rflags = x86.rflags;
    result.mxcsr = x86.mxcsr;
    result
}

pub(super) fn interpret_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    bytes: &[u8; 64],
    case: BroadcastMemoryCase,
) -> GuestRegs {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x5000);
    memory.load(
        MEMORY_ADDRESS as usize,
        &bytes[..case.shape.memory_size() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    interpreter_registers(&context, initial)
}

pub(super) fn expected_destination(
    case: BroadcastMemoryCase,
    initial: &GuestRegs,
    memory: &[u8; 64],
) -> [u64; 8] {
    let old = words_to_bytes(initial.zmm[usize::from(case.destination)]);
    // EVEX.128 and EVEX.256 writes clear every bit above the active vector
    // length, including when all destination lanes merge from the old value.
    let mut expected = [0u8; 64];
    let mask = initial.k[usize::from(case.mask())];
    let lanes = case.shape.destination_lanes() as usize;
    for lane in 0..lanes {
        let active = case.mask() == 0 || mask & (1u64 << lane) != 0;
        let value = if active {
            get_element(
                memory,
                case.shape.elem,
                lane % usize::from(case.shape.source_lanes),
            )
        } else if case.control == MaskControl::Merge {
            get_element(&old, case.shape.elem, lane)
        } else {
            0
        };
        put_element(&mut expected, case.shape.elem, lane, value);
    }
    bytes_to_words(expected)
}

fn assert_only_destination_changed(
    case: BroadcastMemoryCase,
    initial: &GuestRegs,
    actual: &GuestRegs,
) {
    assert_eq!(actual.gpr, initial.gpr, "{case:?}: GPRs");
    for index in 0..32 {
        if index != usize::from(case.destination) {
            assert_eq!(
                actual.zmm[index], initial.zmm[index],
                "{case:?}: ZMM{index}"
            );
        }
    }
    assert_eq!(actual.k, initial.k, "{case:?}: opmasks");
    assert_eq!(actual.rflags, initial.rflags, "{case:?}: RFLAGS");
    assert_eq!(actual.mxcsr, initial.mxcsr, "{case:?}: MXCSR");
}

#[test]
fn all_102_shapes_match_independent_bit_broadcast_oracles_at_o0_o1_o2() {
    let mut encodings = 0usize;
    let mut comparisons = 0usize;
    for mut case in all_cases() {
        case.destination = [0, 8, 17, 31][encodings & 3];
        let mask = if case.mask() == 0 {
            u64::MAX
        } else {
            ((0xA5A5_A5A5_A5A5_A5A5u64.rotate_left(encodings as u32)) | 1) & lane_mask(case)
        };
        let initial = initial_registers(case, encodings, mask);
        assert_eq!(
            initial.vector_active,
            if !case.shape.needs_avx512bw && case.shape.destination_lanes() <= 16 {
                X86_VECTOR_STATE_K16
            } else {
                X86_VECTOR_STATE_K64
            },
            "native opmask bridge mode for {case:?}"
        );
        let memory = memory_bytes(case, encodings);
        let expected = expected_destination(case, &initial, &memory);
        let baseline = interpret_success(
            &optimize(lift_case(case), OptLevel::O0),
            &initial,
            &memory,
            case,
        );
        assert_eq!(
            baseline.zmm[usize::from(case.destination)],
            expected,
            "O0 {case:?}"
        );
        assert_only_destination_changed(case, &initial, &baseline);
        for level in LEVELS {
            let actual =
                interpret_success(&optimize(lift_case(case), level), &initial, &memory, case);
            assert_eq!(actual, baseline, "{level:?} {case:?}");
            assert_eq!(
                actual.zmm[usize::from(case.destination)],
                expected,
                "{level:?} {case:?}"
            );
            comparisons += 1;
        }
        encodings += 1;
    }
    assert_eq!(encodings, 34 * 3);
    assert_eq!(comparisons, encodings * LEVELS.len());
}

#[test]
fn empty_and_out_of_range_masks_suppress_the_complete_unmapped_tuple() {
    let blank = [0u8; 64];
    let mut suppressions = 0usize;
    for shape in SHAPES {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = BroadcastMemoryCase {
                shape,
                destination: 17,
                base: 2,
                control,
            };
            let masks = if shape.destination_lanes() == 64 {
                vec![0]
            } else {
                vec![0, 1u64 << shape.destination_lanes()]
            };
            for mask in masks {
                let initial = initial_registers(case, suppressions, mask);
                let expected = expected_destination(case, &initial, &blank);
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut context = interpreter_context(&initial);
                    let mut unmapped = FlatMemory::new(MEMORY_ADDRESS as usize);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut unmapped,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                        "{level:?} {case:?} mask={mask:#x}: {result:?}"
                    );
                    let actual = interpreter_registers(&context, &initial);
                    assert_eq!(
                        actual.zmm[usize::from(case.destination)],
                        expected,
                        "{level:?} {case:?} mask={mask:#x}"
                    );
                    assert_only_destination_changed(case, &initial, &actual);
                    suppressions += 1;
                }
            }
        }
    }
    assert!(suppressions > 34 * 2 * LEVELS.len());
}

#[test]
fn any_applicable_mask_bit_requires_the_complete_tuple_before_commit() {
    let mut faults = 0usize;
    let mut tuple_sizes = std::collections::BTreeSet::new();
    for mut case in all_cases() {
        case.destination = 17;
        tuple_sizes.insert(case.shape.memory_size());
        let mask = if case.mask() == 0 { u64::MAX } else { 1 };
        let initial = initial_registers(case, faults, mask);
        let memory = memory_bytes(case, faults);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut context = interpreter_context(&initial);
            let available = case.shape.memory_size() as usize - 1;
            let mut partial = FlatMemory::new(MEMORY_ADDRESS as usize + available);
            partial.load(MEMORY_ADDRESS as usize, &memory[..available]);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut partial,
                &function.blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(
                interpreter_registers(&context, &initial),
                initial,
                "{level:?} {case:?}: partial tuple committed state"
            );
            faults += 1;
        }
    }
    assert_eq!(tuple_sizes, [1, 2, 4, 8, 16, 32].into_iter().collect());
    assert_eq!(faults, 34 * 3 * LEVELS.len());
}
