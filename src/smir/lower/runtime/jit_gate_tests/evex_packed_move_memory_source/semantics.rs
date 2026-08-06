//! Interpreter semantic, suppression, alignment, and partial-fault coverage.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticState {
    pub(super) gpr: [u64; 32],
    pub(super) vectors: [[u64; 16]; 32],
    pub(super) masks: [u64; 8],
    pub(super) rflags: u64,
    pub(super) mxcsr: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticOutcome {
    pub(super) state: SemanticState,
    pub(super) memory: [u8; 64],
}

fn lane_mask(lanes: usize) -> u64 {
    if lanes == 64 {
        u64::MAX
    } else {
        (1u64 << lanes) - 1
    }
}

pub(super) fn initial_state(
    case: PackedMoveMemoryCase,
    ordinal: usize,
    address: u64,
    writemask: u64,
) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64
                    .rotate_left(((register * 11 + word * 17 + ordinal) & 63) as u32)
                    ^ (register as u64).wrapping_mul(0x8040_2010_0804_0201)
                    ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
            })
        }),
        masks: std::array::from_fn(|index| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((index * 7 + ordinal) as u32)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x195)) & 0x8D5),
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F) | (((ordinal as u32) & 3) << 13),
    };
    state.gpr[usize::from(case.base)] = address;
    state.masks[usize::from(case.mask)] = writemask;
    state
}

pub(super) fn memory_bytes(ordinal: usize) -> [u8; 64] {
    std::array::from_fn(|index| {
        (index as u8)
            .wrapping_mul(0x3D)
            .wrapping_add((ordinal as u8).wrapping_mul(0x17))
            .wrapping_add(0x29)
    })
}

fn vector_bytes(words: &[u64; 16]) -> [u8; 128] {
    let mut bytes = [0u8; 128];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn vector_words(bytes: &[u8; 128]) -> [u64; 16] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

fn manual(
    case: PackedMoveMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticOutcome {
    let mut state = initial.clone();
    let mut result_memory = *memory;
    let mask = initial.masks[usize::from(case.mask)];
    let lane_bytes = case.lane_bytes();
    match case.direction {
        Direction::Load => {
            let old = vector_bytes(&initial.vectors[usize::from(case.vector)]);
            let mut destination = old;
            for lane in 0..case.lanes() {
                let range = lane * lane_bytes..(lane + 1) * lane_bytes;
                if mask & (1u64 << lane) != 0 {
                    destination[range.clone()].copy_from_slice(&memory[range]);
                } else if case.zeroing() {
                    destination[range].fill(0);
                }
            }
            destination[case.width.bytes() as usize..].fill(0);
            state.vectors[usize::from(case.vector)] = vector_words(&destination);
        }
        Direction::Store => {
            let source = vector_bytes(&initial.vectors[usize::from(case.vector)]);
            for lane in 0..case.lanes() {
                if mask & (1u64 << lane) != 0 {
                    let range = lane * lane_bytes..(lane + 1) * lane_bytes;
                    result_memory[range.clone()].copy_from_slice(&source[range]);
                }
            }
        }
    }
    SemanticOutcome {
        state,
        memory: result_memory,
    }
}

fn context(initial: &SemanticState) -> SmirContext {
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

fn state(context: &SmirContext) -> SemanticState {
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

fn execute(
    function: &SmirFunction,
    initial: &SemanticState,
    memory: &mut FlatMemory,
) -> (BlockResult, SemanticState) {
    let mut context = context(initial);
    let result = SmirInterpreter::new().execute_block(&mut context, memory, &function.blocks[0]);
    (result, state(&context))
}

pub(super) fn interpret_success(
    function: &SmirFunction,
    initial: &SemanticState,
    bytes: &[u8; 64],
) -> SemanticOutcome {
    let mut memory = FlatMemory::new(0x5000);
    memory.load(MEMORY_ADDRESS as usize, bytes);
    let (result, state) = execute(function, initial, &mut memory);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    let mut observed = [0u8; 64];
    memory.read(MEMORY_ADDRESS, &mut observed).unwrap();
    SemanticOutcome {
        state,
        memory: observed,
    }
}

#[test]
fn all_90_packed_move_cells_match_bit_exact_manual_semantics_at_o0_o1_o2() {
    let mut comparisons = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        let lanes = case.lanes();
        let mut mask =
            0xA55A_6996_F00F_3CC3u64.rotate_left((ordinal & 63) as u32) & lane_mask(lanes);
        if mask == 0 {
            mask = 1;
        }
        let initial = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
        let memory = memory_bytes(ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret_success(&function, &initial, &memory);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 90 * LEVELS.len());
}

#[test]
fn every_k1_to_k7_observes_exactly_the_architectural_lane_bits() {
    let mut comparisons = 0usize;
    for spec in SPECS {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for direction in Direction::ALL {
                for control in [MaskControl::Merge, MaskControl::Zero] {
                    if !control.valid_for(direction) {
                        continue;
                    }
                    for mask in 1..=7u8 {
                        let case = PackedMoveMemoryCase {
                            spec,
                            direction,
                            width,
                            vector: 16 + mask,
                            base: 2,
                            mask,
                            control,
                        };
                        let lanes = case.lanes();
                        let active = (0xD6A5_3C69_F00F_5AA5u64
                            .rotate_left(u32::from(mask * 7) + case.ll() as u32))
                            & lane_mask(lanes);
                        let initial = initial_state(
                            case,
                            usize::from(mask),
                            MEMORY_ADDRESS,
                            active | !lane_mask(lanes),
                        );
                        let memory = memory_bytes(usize::from(mask));
                        let function = optimize(lift_case(case), OptLevel::O2);
                        assert_eq!(
                            interpret_success(&function, &initial, &memory),
                            manual(case, &initial, &memory),
                            "{case:?}"
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 10 * 3 * 3 * 7);
}

#[test]
fn empty_and_out_of_range_masks_suppress_every_memory_access() {
    let mut empty_suppressions = 0usize;
    let mut high_suppressions = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, ordinal, MEMORY_ADDRESS, 0);
            let mut unmapped = FlatMemory::with_base(MEMORY_ADDRESS, 0);
            let (result, actual) = execute(&function, &initial, &mut unmapped);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(
                actual,
                manual(case, &initial, &[0; 64]).state,
                "{level:?} {case:?}: empty mask"
            );
            empty_suppressions += 1;

            let high = !lane_mask(case.lanes());
            if high != 0 {
                let initial = initial_state(case, ordinal ^ 0x55, MEMORY_ADDRESS, high);
                let mut unmapped = FlatMemory::with_base(MEMORY_ADDRESS, 0);
                let (result, actual) = execute(&function, &initial, &mut unmapped);
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                assert_eq!(
                    actual,
                    manual(case, &initial, &[0; 64]).state,
                    "{level:?} {case:?}: out-of-range mask"
                );
                high_suppressions += 1;
            }
        }
    }
    assert_eq!(empty_suppressions, 90 * LEVELS.len());
    assert!(high_suppressions >= 240);
}

#[test]
fn type_e1_alignment_faults_precede_mask_suppression_and_memory_access() {
    let aligned_cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| case.spec.aligned)
        .collect();
    assert_eq!(aligned_cases.len(), 36);
    let mut faults = 0usize;
    for (ordinal, case) in aligned_cases.into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, ordinal, MEMORY_ADDRESS + 1, 0);
            let mut unmapped = FlatMemory::with_base(MEMORY_ADDRESS + 1, 0);
            let (result, actual) = execute(&function, &initial, &mut unmapped);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::GeneralProtection {
                        addr: PC,
                        error_code: 0
                    })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, initial, "{level:?} {case:?}: #GP committed state");
            faults += 1;
        }
    }
    assert_eq!(faults, 36 * LEVELS.len());
}

#[test]
fn load_faults_never_commit_destination_flags_masks_or_mxcsr() {
    let load_cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| case.direction == Direction::Load)
        .collect();
    assert_eq!(load_cases.len(), 60);
    let mut faults = 0usize;
    for (ordinal, case) in load_cases.into_iter().enumerate() {
        let bytes = memory_bytes(ordinal);
        let size = case.width.bytes() as usize - 1;
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, ordinal, MEMORY_ADDRESS, lane_mask(case.lanes()));
            let mut partial = FlatMemory::with_base(MEMORY_ADDRESS, size);
            partial.load(0, &bytes[..size]);
            let (result, actual) = execute(&function, &initial, &mut partial);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, initial, "{level:?} {case:?}: load fault commit");
            let mut observed = vec![0u8; size];
            partial.read(MEMORY_ADDRESS, &mut observed).unwrap();
            assert_eq!(observed, bytes[..size], "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(faults, 60 * LEVELS.len());
}

#[test]
fn stores_commit_active_lanes_in_ascending_order_before_a_later_fault() {
    let store_cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| case.direction == Direction::Store)
        .collect();
    assert_eq!(store_cases.len(), 30);
    let mut faults = 0usize;
    for (ordinal, case) in store_cases.into_iter().enumerate() {
        let original = memory_bytes(ordinal);
        let size = case.width.bytes() as usize - 1;
        let fault_lane = case.lanes() - 1;
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, ordinal, MEMORY_ADDRESS, lane_mask(case.lanes()));
            let mut partial = FlatMemory::with_base(MEMORY_ADDRESS, size);
            partial.load(0, &original[..size]);
            let (result, actual) = execute(&function, &initial, &mut partial);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, initial, "{level:?} {case:?}: store fault state");

            let source = vector_bytes(&initial.vectors[usize::from(case.vector)]);
            let mut expected = original[..size].to_vec();
            for lane in 0..fault_lane {
                let range = lane * case.lane_bytes()..(lane + 1) * case.lane_bytes();
                expected[range.clone()].copy_from_slice(&source[range]);
            }
            let mut observed = vec![0u8; size];
            partial.read(MEMORY_ADDRESS, &mut observed).unwrap();
            assert_eq!(observed, expected, "{level:?} {case:?}: partial store");
            faults += 1;
        }
    }
    assert_eq!(faults, 30 * LEVELS.len());
}
