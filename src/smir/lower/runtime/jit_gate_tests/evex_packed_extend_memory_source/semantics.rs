//! Interpreter oracles, mask suppression, and precise-frontier checks.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64};

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

pub(super) fn memory_bytes(case: ExtendCase, seed: usize) -> [u8; 64] {
    const I8_VALUES: [u64; 8] = [0x00, 0x01, 0x7F, 0x80, 0xFF, 0x55, 0xAA, 0x81];
    const I16_VALUES: [u64; 8] = [
        0x0000, 0x0001, 0x7FFF, 0x8000, 0xFFFF, 0x5555, 0xAAAA, 0x8001,
    ];
    const I32_VALUES: [u64; 8] = [
        0x0000_0000,
        0x0000_0001,
        0x7FFF_FFFF,
        0x8000_0000,
        0xFFFF_FFFF,
        0x5555_5555,
        0xAAAA_AAAA,
        0x8000_0001,
    ];

    let values = match case.spec.source_elem {
        VecElementType::I8 => I8_VALUES,
        VecElementType::I16 => I16_VALUES,
        VecElementType::I32 => I32_VALUES,
        other => unreachable!("packed-extension source {other:?}"),
    };
    let mut bytes = [0u8; 64];
    for lane in 0..usize::from(case.lanes()) {
        put_element(
            &mut bytes,
            case.spec.source_elem,
            lane,
            values[(seed + lane) % values.len()],
        );
    }
    bytes
}

pub(super) fn initial_registers(case: ExtendCase, seed: usize, mask: u64) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x10)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr: 0x1F80 | (((seed as u32) & 3) << 13),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    registers.gpr[2] = MEMORY_ADDRESS;
    registers.k[1] = mask;
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
        });
    }
    assert!(usize::from(case.destination) < registers.zmm.len());
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
    case: ExtendCase,
) -> GuestRegs {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x3000);
    memory.load(
        MEMORY_ADDRESS as usize,
        &bytes[..case.memory_size() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    interpreter_registers(&context, initial)
}

fn extended_value(case: ExtendCase, source: u64) -> u64 {
    let value = if case.spec.signed {
        match case.spec.source_elem {
            VecElementType::I8 => i64::from(source as u8 as i8) as u64,
            VecElementType::I16 => i64::from(source as u16 as i16) as u64,
            VecElementType::I32 => i64::from(source as u32 as i32) as u64,
            other => unreachable!("signed source {other:?}"),
        }
    } else {
        match case.spec.source_elem {
            VecElementType::I8 => u64::from(source as u8),
            VecElementType::I16 => u64::from(source as u16),
            VecElementType::I32 => u64::from(source as u32),
            other => unreachable!("zero-extended source {other:?}"),
        }
    };
    match case.spec.destination_elem {
        VecElementType::I16 => value & u64::from(u16::MAX),
        VecElementType::I32 => value & u64::from(u32::MAX),
        VecElementType::I64 => value,
        other => unreachable!("extension destination {other:?}"),
    }
}

fn expected_destination(case: ExtendCase, initial: &GuestRegs, memory: &[u8; 64]) -> [u64; 8] {
    let initial_bytes = words_to_bytes(initial.zmm[usize::from(case.destination)]);
    let mut expected = [0u8; 64];
    let mask = initial.k[1];
    for lane in 0..usize::from(case.lanes()) {
        let active = case.mask() == 0 || mask & (1u64 << lane) != 0;
        let value = if active {
            extended_value(case, get_element(memory, case.spec.source_elem, lane))
        } else if case.control == MaskControl::Merge {
            get_element(&initial_bytes, case.spec.destination_elem, lane)
        } else {
            0
        };
        put_element(&mut expected, case.spec.destination_elem, lane, value);
    }
    bytes_to_words(expected)
}

fn assert_only_destination_changed(case: ExtendCase, initial: &GuestRegs, actual: &GuestRegs) {
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
fn all_198_encodings_match_independent_integer_oracles_at_o0_o1_o2() {
    let mut encodings = 0usize;
    let mut comparisons = 0usize;
    for mut case in all_cases() {
        case.destination = [0, 8, 17, 31][encodings & 3];
        let lane_mask = (1u64 << case.lanes()) - 1;
        let mask = if case.mask() == 0 {
            u64::MAX
        } else {
            (0xA5A5_A5A5_A5A5_A5A5u64 ^ encodings as u64) & lane_mask
        };
        let initial = initial_registers(case, encodings, mask);
        let memory = memory_bytes(case, encodings);
        let expected_destination = expected_destination(case, &initial, &memory);
        let baseline = optimize(lift_case(case), OptLevel::O0);
        let baseline_result = interpret_success(&baseline, &initial, &memory, case);
        assert_eq!(
            baseline_result.zmm[usize::from(case.destination)],
            expected_destination,
            "O0 {case:?}"
        );
        assert_only_destination_changed(case, &initial, &baseline_result);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret_success(&function, &initial, &memory, case);
            assert_eq!(actual, baseline_result, "{level:?} {case:?}");
            assert_eq!(
                actual.zmm[usize::from(case.destination)],
                expected_destination,
                "{level:?} {case:?}"
            );
            comparisons += 1;
        }
        encodings += 1;
    }
    assert_eq!(encodings, 198);
    assert_eq!(comparisons, encodings * LEVELS.len());
}

#[test]
fn wig_encodings_are_semantically_identical_for_all_ten_families() {
    let mut comparisons = 0usize;
    for spec in SPECS
        .into_iter()
        .filter(|spec| !matches!(spec.opcode, 0x25 | 0x35))
    {
        for ll in 0..=2 {
            for control in MaskControl::ALL {
                let w0 = ExtendCase {
                    spec,
                    w: false,
                    ll,
                    destination: 17,
                    control,
                };
                let w1 = ExtendCase { w: true, ..w0 };
                let mask = 0xA5A5_A5A5 & ((1u64 << w0.lanes()) - 1);
                let initial = initial_registers(w0, comparisons, mask);
                let memory = memory_bytes(w0, comparisons);
                for level in LEVELS {
                    let left =
                        interpret_success(&optimize(lift_case(w0), level), &initial, &memory, w0);
                    let right =
                        interpret_success(&optimize(lift_case(w1), level), &initial, &memory, w1);
                    assert_eq!(left, right, "{} LL={ll} {control:?} {level:?}", spec.name);
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 10 * 3 * 3 * LEVELS.len());
}

#[test]
fn empty_masks_suppress_unmapped_memory_and_apply_merge_or_zero() {
    for spec in SPECS {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ExtendCase {
                spec,
                w: false,
                ll: 2,
                destination: 17,
                control,
            };
            let initial = initial_registers(case, usize::from(spec.opcode), 0);
            let blank = [0u8; 64];
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
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::Return { .. })
                ));
                let actual = interpreter_registers(&context, &initial);
                assert_eq!(
                    actual.zmm[17], expected,
                    "{} {control:?} {level:?}",
                    spec.name
                );
                assert_only_destination_changed(case, &initial, &actual);
            }
        }
    }
}

#[test]
fn inactive_final_lanes_suppress_partial_tuple_faults() {
    let cases = [
        ExtendCase {
            spec: SPECS[0],
            w: true,
            ll: 2,
            destination: 17,
            control: MaskControl::Merge,
        },
        ExtendCase {
            spec: SPECS[4],
            w: false,
            ll: 2,
            destination: 17,
            control: MaskControl::Zero,
        },
        ExtendCase {
            spec: SPECS[5],
            w: false,
            ll: 2,
            destination: 17,
            control: MaskControl::Merge,
        },
    ];
    for case in cases {
        let final_lane = case.lanes() - 1;
        let mask = (1u64 << final_lane) - 1;
        let initial = initial_registers(case, usize::from(case.spec.opcode), mask);
        let bytes = memory_bytes(case, 3);
        let available = case.memory_size() - case.spec.source_elem.bytes();
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut context = interpreter_context(&initial);
            let mut partial = FlatMemory::new(MEMORY_ADDRESS as usize + available as usize);
            partial.load(MEMORY_ADDRESS as usize, &bytes[..available as usize]);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut partial,
                &function.blocks[0],
            );
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{case:?} {level:?}: {result:?}"
            );
            let actual = interpreter_registers(&context, &initial);
            assert_eq!(
                actual.zmm[17],
                expected_destination(case, &initial, &bytes),
                "{case:?} {level:?}"
            );
        }
    }
}

#[test]
fn every_tuple_width_and_replay_form_faults_before_architectural_commit() {
    let cases = [
        // Exact unmasked source tuple sizes: 2, 4, 8, 16, and 32 bytes.
        ExtendCase {
            spec: SPECS[2],
            w: false,
            ll: 0,
            destination: 17,
            control: MaskControl::None,
        },
        ExtendCase {
            spec: SPECS[4],
            w: true,
            ll: 0,
            destination: 17,
            control: MaskControl::None,
        },
        ExtendCase {
            spec: SPECS[5],
            w: false,
            ll: 0,
            destination: 17,
            control: MaskControl::None,
        },
        ExtendCase {
            spec: SPECS[1],
            w: true,
            ll: 2,
            destination: 17,
            control: MaskControl::None,
        },
        ExtendCase {
            spec: SPECS[0],
            w: false,
            ll: 2,
            destination: 17,
            control: MaskControl::None,
        },
        // Masked B1/B2/B4 lane-helper shapes.
        ExtendCase {
            spec: SPECS[8],
            w: true,
            ll: 2,
            destination: 17,
            control: MaskControl::Merge,
        },
        ExtendCase {
            spec: SPECS[10],
            w: false,
            ll: 2,
            destination: 17,
            control: MaskControl::Zero,
        },
        ExtendCase {
            spec: SPECS[11],
            w: false,
            ll: 2,
            destination: 17,
            control: MaskControl::Merge,
        },
    ];
    let tuple_sizes: [u32; 5] = std::array::from_fn(|index| cases[index].memory_size());
    assert_eq!(tuple_sizes, [2, 4, 8, 16, 32]);

    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_registers(case, ordinal, u64::MAX);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut context = interpreter_context(&initial);
            let partial_size = MEMORY_ADDRESS as usize + case.memory_size() as usize - 1;
            let mut partial = FlatMemory::new(partial_size);
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
                "{level:?} {case:?}: fault committed state"
            );
        }
    }
}
