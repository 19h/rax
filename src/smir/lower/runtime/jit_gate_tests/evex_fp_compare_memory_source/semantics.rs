//! Interpreter, optimizer, MXCSR, masking, and fault semantics for packed comparisons.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64};

fn set_lane(words: &mut [u64; 8], elem: VecElementType, lane: usize, value: u64) {
    let bytes = elem.bytes() as usize;
    let mut raw = words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    let start = lane * bytes;
    raw[start..start + bytes].copy_from_slice(&value.to_le_bytes()[..bytes]);
    for (word, chunk) in words.iter_mut().zip(raw.chunks_exact(8)) {
        *word = u64::from_le_bytes(chunk.try_into().unwrap());
    }
}

fn get_lane(words: &[u64; 8], elem: VecElementType, lane: usize) -> u64 {
    let bytes = elem.bytes() as usize;
    let raw = words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    let start = lane * bytes;
    let mut value = [0u8; 8];
    value[..bytes].copy_from_slice(&raw[start..start + bytes]);
    u64::from_le_bytes(value)
}

fn patterns(elem: VecElementType) -> &'static [u64] {
    const F16: [u64; 14] = [
        0x0000, 0x8000, 0x3C00, 0x4000, 0x4400, 0x0001, 0x8001, 0x0400, 0x7C00, 0xFC00, 0x7E01,
        0x7C01, 0xBC00, 0x3555,
    ];
    const F32: [u64; 14] = [
        0x0000_0000,
        0x8000_0000,
        0x3F80_0000,
        0x4000_0000,
        0x4080_0000,
        0x0000_0001,
        0x8000_0001,
        0x0080_0000,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC0_0001,
        0x7F80_0001,
        0xBF80_0000,
        0x3F00_0001,
    ];
    const F64: [u64; 14] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3FF0_0000_0000_0000,
        0x4000_0000_0000_0000,
        0x4010_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x0010_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0001,
        0x7FF0_0000_0000_0001,
        0xBFF0_0000_0000_0000,
        0x3FE0_0000_0000_0001,
    ];
    match elem {
        VecElementType::F16 => &F16,
        VecElementType::F32 => &F32,
        VecElementType::F64 => &F64,
        _ => unreachable!("packed comparison element"),
    }
}

pub(super) fn initial_registers(case: FpCompareMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0xA5A5_0000_0000_0000u64
                ^ ((ordinal as u64) << 12)
                ^ (index as u64 * 0x0101_0101_0101_0101)
        }),
        zmm: std::array::from_fn(|register| {
            let mut value = [0u64; 8];
            for lane in 0..case.width.lanes(case.elem) as usize {
                let source = patterns(case.elem);
                set_lane(
                    &mut value,
                    case.elem,
                    lane,
                    source[(lane + register * 3 + ordinal) % source.len()],
                );
            }
            value
        }),
        k: std::array::from_fn(|index| {
            if index == 1 {
                0xA5A5_A5A5_A5A5_A5A5
            } else {
                0xF0F0_0000_0000_0000 ^ index as u64
            }
        }),
        rflags: 0x8D5,
        mxcsr: 0x1F80 | (((ordinal & 3) as u32) << 13),
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        vector_active: X86_VECTOR_STATE_K64,
        ..GuestRegs::default()
    };
    registers.gpr[3] = 0x2000;
    registers
}

pub(super) fn memory_value(case: FpCompareMemoryCase, ordinal: usize) -> [u64; 8] {
    let mut value = [0u64; 8];
    let source = patterns(case.elem);
    for lane in 0..case.width.lanes(case.elem) as usize {
        set_lane(
            &mut value,
            case.elem,
            lane,
            source[(lane * 5 + ordinal + 1) % source.len()],
        );
    }
    value
}

pub(super) fn memory_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn context_from(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
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

pub(super) fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    value: [u64; 8],
    case: FpCompareMemoryCase,
) -> GuestRegs {
    let mut context = context_from(initial);
    let mut memory = FlatMemory::new(0x10000);
    let bytes = memory_bytes(value);
    memory.load(0x2000, &bytes[..case.memory_size() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut actual = *initial;
    actual.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        actual.zmm[index].copy_from_slice(&value[..8]);
    }
    actual.k = x86.k;
    actual.rflags = x86.rflags;
    actual.mxcsr = x86.mxcsr;
    actual
}

#[test]
fn all_108_memory_shapes_preserve_o0_o1_o2_interpreter_equivalence() {
    let cases = all_cases();
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_registers(case, ordinal);
        let value = memory_value(case, ordinal);
        let expected = interpreter_success(&lift_case(case), &initial, value, case);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, value, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 108 * LEVELS.len());
}

fn relation_values(elem: VecElementType, relation: usize) -> (u64, u64) {
    match (elem, relation) {
        (VecElementType::F16, 0) => (0x4000, 0x3C00),
        (VecElementType::F16, 1) => (0x3C00, 0x4000),
        (VecElementType::F16, 2) => (0x8000, 0x0000),
        (VecElementType::F16, 3) => (0x7E01, 0),
        (VecElementType::F32, 0) => (0x4000_0000, 0x3F80_0000),
        (VecElementType::F32, 1) => (0x3F80_0000, 0x4000_0000),
        (VecElementType::F32, 2) => (0x8000_0000, 0x0000_0000),
        (VecElementType::F32, 3) => (0x7FC0_0001, 0),
        (VecElementType::F64, 0) => (0x4000_0000_0000_0000, 0x3FF0_0000_0000_0000),
        (VecElementType::F64, 1) => (0x3FF0_0000_0000_0000, 0x4000_0000_0000_0000),
        (VecElementType::F64, 2) => (0x8000_0000_0000_0000, 0x0000_0000_0000_0000),
        (VecElementType::F64, 3) => (0x7FF8_0000_0000_0001, 0),
        _ => unreachable!("four comparison relations"),
    }
}

#[test]
fn packed_memory_comparisons_match_all_32_intel_predicate_truth_tables() {
    const TABLES: [u8; 16] = [
        0b0100, 0b0010, 0b0110, 0b1000, 0b1011, 0b1101, 0b1001, 0b0111, 0b1100, 0b1010, 0b1110,
        0b0000, 0b0011, 0b0101, 0b0001, 0b1111,
    ];
    const SIGNALING: [u8; 16] = [1, 2, 5, 6, 9, 10, 13, 14, 16, 19, 20, 23, 24, 27, 28, 31];

    let mut checked = 0usize;
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for control in MaskControl::ALL {
            for predicate in 0..32u8 {
                let case = FpCompareMemoryCase {
                    elem,
                    width: VecWidth::V512,
                    destination: 7,
                    source1: 17,
                    form: SourceForm::Vector,
                    control,
                    predicate,
                };
                let mut initial = initial_registers(case, usize::from(predicate));
                initial.zmm[17] = [0; 8];
                initial.k[1] = 0xA5A5_A5A5;
                initial.k[7] = u64::MAX;
                initial.mxcsr = 0x1F80 | (1 << 5);
                let mut memory = [0u64; 8];
                let lanes = case.width.lanes(elem) as usize;
                for lane in 0..lanes {
                    let (first, second) = relation_values(elem, lane & 3);
                    set_lane(&mut initial.zmm[17], elem, lane, first);
                    set_lane(&mut memory, elem, lane, second);
                }
                let actual = interpreter_success(&lift_case(case), &initial, memory, case);
                let active = if case.mask() == 0 {
                    u64::MAX
                } else {
                    initial.k[1]
                };
                let table = TABLES[usize::from(predicate & 0x0F)];
                let mut expected = 0u64;
                let mut active_unordered = false;
                for lane in 0..lanes {
                    if active & (1u64 << lane) != 0 {
                        let relation = lane & 3;
                        expected |= u64::from(table & (1 << relation) != 0) << lane;
                        active_unordered |= relation == 3;
                    }
                }
                assert_eq!(actual.k[7], expected, "{case:?}");
                let expected_status =
                    (1 << 5) | u32::from(active_unordered && SIGNALING.contains(&predicate));
                assert_eq!(actual.mxcsr & 0x3F, expected_status, "{case:?}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3 * 2 * 32);
}

#[test]
fn destination_writemask_alias_reads_old_k_value_and_clears_high_result_bits() {
    let case = FpCompareMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        destination: 1,
        source1: 17,
        form: SourceForm::Broadcast,
        control: MaskControl::Masked,
        predicate: 0,
    };
    let mut initial = initial_registers(case, 0);
    initial.k[1] = 0b1101 | (u64::MAX << 8);
    initial.zmm[17] = [0; 8];
    for lane in 0..4 {
        set_lane(&mut initial.zmm[17], case.elem, lane, 0x3F80_0000);
    }
    let mut memory = [0u64; 8];
    set_lane(&mut memory, case.elem, 0, 0x3F80_0000);
    let actual = interpreter_success(&lift_case(case), &initial, memory, case);
    assert_eq!(actual.k[1], 0b1101);
}

#[test]
fn inactive_memory_suppresses_faults_and_active_faults_are_noncommitting() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for form in [SourceForm::Vector, SourceForm::Broadcast] {
            let case = FpCompareMemoryCase {
                elem,
                width: VecWidth::V512,
                destination: 7,
                source1: 17,
                form,
                control: MaskControl::Masked,
                predicate: 19,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let mut initial = initial_registers(case, 0);
                initial.gpr[3] = 0x20_000;
                initial.k[1] = 0;
                initial.k[7] = u64::MAX;
                let mut context = context_from(&initial);
                let mut memory = FlatMemory::new(0x1000);
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut memory,
                    &function.blocks[0],
                );
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::Return { .. })
                ));
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!()
                };
                assert_eq!(x86.k[7], 0, "{level:?} {case:?}");
                assert_eq!(x86.mxcsr, initial.mxcsr, "{level:?} {case:?}");

                let mut active = initial;
                active.k[1] = 1;
                active.k[7] = 0x0123_4567_89AB_CDEF;
                let mut context = context_from(&active);
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut memory,
                    &function.blocks[0],
                );
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr: 0x20_000,
                        write: false,
                        ..
                    })
                ));
                let ArchRegState::X86_64(x86) = &context.arch_regs else {
                    unreachable!()
                };
                assert_eq!(x86.k[7], active.k[7], "{level:?} {case:?}");
                assert_eq!(x86.mxcsr, active.mxcsr, "{level:?} {case:?}");
            }
        }
    }
}

#[test]
fn unmasked_invalid_exception_sets_ie_without_committing_destination_mask() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for form in [SourceForm::Vector, SourceForm::Broadcast] {
            let case = FpCompareMemoryCase {
                elem,
                width: VecWidth::V512,
                destination: 7,
                source1: 17,
                form,
                control: MaskControl::None,
                predicate: 19,
            };
            let mut initial = initial_registers(case, 0);
            initial.mxcsr = 0x1F80 & !(1 << 7);
            initial.k[7] = 0x0123_4567_89AB_CDEF;
            let mut value = [0u64; 8];
            let qnan = match elem {
                VecElementType::F16 => 0x7E01,
                VecElementType::F32 => 0x7FC0_0001,
                VecElementType::F64 => 0x7FF8_0000_0000_0001,
                _ => unreachable!(),
            };
            set_lane(&mut value, elem, 0, qnan);
            let mut context = context_from(&initial);
            let mut memory = FlatMemory::new(0x10000);
            let bytes = memory_bytes(value);
            memory.load(0x2000, &bytes[..case.memory_size() as usize]);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &lift_case(case).blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
                ),
                "{case:?}: {result:?}"
            );
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.k[7], initial.k[7], "{case:?}");
            assert_ne!(x86.mxcsr & 1, 0, "{case:?}");
        }
    }
}

#[test]
fn raw_lane_helpers_cover_every_element_boundary_without_cross_lane_aliasing() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        let mut words = [u64::MAX; 8];
        let lanes = VecWidth::V512.lanes(elem) as usize;
        for lane in 0..lanes {
            let bits = (lane as u64 + 1) & ((1u64 << (elem.bytes() * 8).min(63)) - 1);
            set_lane(&mut words, elem, lane, bits);
        }
        for lane in 0..lanes {
            assert_eq!(
                get_lane(&words, elem, lane),
                lane as u64 + 1,
                "{elem:?} {lane}"
            );
        }
    }
}
