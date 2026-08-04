//! Interpreter, optimizer, truth-table, and E4 fault-suppression coverage.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticState {
    pub(super) gpr: [u64; 32],
    pub(super) vectors: [[u64; 16]; 32],
    pub(super) masks: [u64; 8],
    pub(super) rflags: u64,
    pub(super) mxcsr: u32,
}

pub(super) fn initial_state(case: TernaryMemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x8000_0001_7FFF_FFFFu64.rotate_left((register * 11 + word * 17 + ordinal) as u32)
                    ^ ((register as u64) << 56)
                    ^ (word as u64).wrapping_mul(0x1020_4081_0204_0810)
            })
        }),
        masks: [
            u64::MAX,
            [0, u64::MAX, 0xA55A_3CC3_F00F_9696, 0x8000_0000_0000_0001][ordinal & 3],
            0x5AA5_C33C_0FF0_6969,
            0xA55A_3CC3_F00F_9696,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    state.gpr[0] = 0x1000;
    state.gpr[1] = (0x2000 - state.gpr[0] - case.compressed_displacement() as u64) / 2;
    if case.source2 == case.destination {
        state.vectors[usize::from(case.destination)] =
            std::array::from_fn(|word| 0xFFFF_0001_8000_7FFFu64.rotate_left((word * 9) as u32));
    }
    state
}

pub(super) fn memory_bytes(ordinal: usize) -> [u8; 64] {
    let words: [u64; 8] = std::array::from_fn(|word| {
        0x0123_4567_89AB_CDEFu64.rotate_left((word * 13 + ordinal) as u32)
            ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
    });
    let mut bytes = [0u8; 64];
    for (word, value) in words.into_iter().enumerate() {
        bytes[word * 8..word * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn get_lane(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I32 => (vector[lane / 2] >> ((lane & 1) * 32)) & 0xFFFF_FFFF,
        VecElementType::I64 => vector[lane],
        _ => unreachable!("VPTERNLOG element"),
    }
}

fn set_lane(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    match elem {
        VecElementType::I32 => {
            let shift = (lane & 1) * 32;
            vector[lane / 2] =
                (vector[lane / 2] & !(0xFFFF_FFFFu64 << shift)) | ((value & 0xFFFF_FFFF) << shift);
        }
        VecElementType::I64 => vector[lane] = value,
        _ => unreachable!("VPTERNLOG element"),
    }
}

fn memory_vector(case: TernaryMemoryCase, bytes: &[u8; 64]) -> [u64; 16] {
    let mut vector = [0u64; 16];
    if case.broadcast() {
        let scalar = match case.elem {
            VecElementType::I32 => u32::from_le_bytes(bytes[..4].try_into().unwrap()) as u64,
            VecElementType::I64 => u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            _ => unreachable!("VPTERNLOG element"),
        };
        for lane in 0..case.width.lanes(case.elem) as usize {
            set_lane(&mut vector, lane, case.elem, scalar);
        }
    } else {
        for word in 0..case.width.bytes() as usize / 8 {
            vector[word] = u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap());
        }
    }
    vector
}

fn ternary_word(a: u64, b: u64, c: u64, immediate: u8) -> u64 {
    let mut result = 0u64;
    for index in 0..8u8 {
        if immediate & (1 << index) != 0 {
            result |= if index & 4 != 0 { a } else { !a }
                & if index & 2 != 0 { b } else { !b }
                & if index & 1 != 0 { c } else { !c };
        }
    }
    result
}

pub(super) fn manual(
    case: TernaryMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let source1 = initial.vectors[usize::from(case.destination)];
    let source2 = initial.vectors[usize::from(case.source2)];
    let source3 = memory_vector(case, memory);
    let mut raw = [0u64; 16];
    for word in 0..case.width.bytes() as usize / 8 {
        raw[word] = ternary_word(source1[word], source2[word], source3[word], case.immediate);
    }
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..case.width.lanes(case.elem) as usize {
        let value = if mask & (1u64 << lane) != 0 {
            get_lane(&raw, lane, case.elem)
        } else if case.zeroing() {
            0
        } else {
            get_lane(&source1, lane, case.elem)
        };
        set_lane(destination, lane, case.elem, value);
    }
    for word in case.width.bytes() as usize / 8..destination.len() {
        destination[word] = 0;
    }
    expected
}

fn context_from_state(initial: &SemanticState) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        x86.xmm = initial.vectors;
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

pub(super) fn interpret(
    function: &SmirFunction,
    initial: &SemanticState,
    memory_bytes: &[u8; 64],
    case: TernaryMemoryCase,
) -> SemanticState {
    let mut context = context_from_state(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        case.elem.bytes()
    } else {
        case.width.bytes()
    } as usize;
    memory.load(0x2000, &memory_bytes[..size]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    SemanticState {
        gpr: x86.gpr,
        vectors: x86.xmm,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn all_108_memory_cells_match_manual_ternary_model_at_o0_o1_o2() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 108);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret(&function, &initial, &memory, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 108 * LEVELS.len());
}

#[test]
fn every_truth_table_matches_manual_model_across_all_shapes() {
    let memory = memory_bytes(7);
    let mut comparisons = 0usize;
    for elem in [VecElementType::I32, VecElementType::I64] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for control in MaskControl::ALL {
                    for immediate in u8::MIN..=u8::MAX {
                        let case = TernaryMemoryCase {
                            elem,
                            width,
                            destination: 17,
                            source2: 18,
                            form,
                            control,
                            immediate,
                        };
                        let initial = initial_state(case, usize::from(immediate));
                        let expected = manual(case, &initial, &memory);
                        for level in LEVELS {
                            let function = optimize(lift_case(case), level);
                            let actual = interpret(&function, &initial, &memory, case);
                            assert_eq!(actual, expected, "{level:?} {case:?}");
                            comparisons += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 2 * 3 * 2 * 3 * 256 * LEVELS.len());
}

#[test]
fn inactive_e4_memory_suppresses_faults_and_faults_do_not_commit_destination() {
    for elem in [VecElementType::I32, VecElementType::I64] {
        for form in [SourceForm::Vector, SourceForm::Broadcast] {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = TernaryMemoryCase {
                    elem,
                    width: VecWidth::V512,
                    destination: 17,
                    source2: 18,
                    form,
                    control,
                    immediate: 0x96,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_state(case, 0);
                    initial.gpr[0] = 0x8000;
                    initial.gpr[1] = 0;
                    initial.masks[usize::from(case.mask())] = 0;
                    let expected = manual(case, &initial, &[0; 64]);
                    let mut context = context_from_state(&initial);
                    let mut memory = FlatMemory::new(0x100);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                        "{level:?} {case:?}: {result:?}"
                    );
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(
                        x86.xmm[usize::from(case.destination)],
                        expected.vectors[usize::from(case.destination)],
                        "{level:?} {case:?}"
                    );

                    let mut active = initial.clone();
                    active.masks[usize::from(case.mask())] = 1;
                    let old_destination = active.vectors[usize::from(case.destination)];
                    let mut context = context_from_state(&active);
                    let mut memory = FlatMemory::new(0x100);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ),
                        "{level:?} {case:?}: {result:?}"
                    );
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(
                        x86.xmm[usize::from(case.destination)],
                        old_destination,
                        "{level:?} {case:?}"
                    );
                }
            }
        }
    }
}
