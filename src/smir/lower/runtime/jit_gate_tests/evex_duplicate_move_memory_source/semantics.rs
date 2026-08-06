//! Interpreter semantics and precise E4NF/E5NF tuple-fault coverage.

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
    (1u64 << lanes) - 1
}

pub(super) fn initial_state(
    case: DuplicateMemoryCase,
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
    if case.mask() != 0 {
        state.masks[usize::from(case.mask())] = writemask;
    }
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

fn manual_explicit(
    case: DuplicateMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
    writemask: Option<u8>,
    zeroing: bool,
) -> SemanticOutcome {
    let mut state = initial.clone();
    let mut destination = vector_bytes(&initial.vectors[usize::from(case.destination)]);
    let element_bytes = case.kind.elem().bytes() as usize;
    let active = writemask
        .map(|mask| initial.masks[usize::from(mask)])
        .unwrap_or(u64::MAX);

    for lane in 0..case.lanes() {
        let range = lane * element_bytes..(lane + 1) * element_bytes;
        if active & (1u64 << lane) != 0 {
            let selector = lane / 2 * 2 + usize::from(case.kind.high());
            let source = selector * element_bytes..(selector + 1) * element_bytes;
            destination[range].copy_from_slice(&memory[source]);
        } else if zeroing {
            destination[range].fill(0);
        }
    }
    destination[case.width.bytes() as usize..].fill(0);
    state.vectors[usize::from(case.destination)] = vector_words(&destination);
    SemanticOutcome {
        state,
        memory: *memory,
    }
}

fn manual(
    case: DuplicateMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticOutcome {
    manual_explicit(
        case,
        initial,
        memory,
        (case.mask() != 0).then_some(case.mask()),
        case.zeroing(),
    )
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
fn all_27_duplicate_move_cells_match_bit_exact_manual_semantics_at_o0_o1_o2() {
    let mut comparisons = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        let mut mask =
            0xA55A_6996_F00F_3CC3u64.rotate_left((ordinal & 63) as u32) & lane_mask(case.lanes());
        if case.mask() != 0 && mask == 0 {
            mask = 1;
        }
        let initial = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
        let memory = memory_bytes(ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_eq!(
                interpret_success(&function, &initial, &memory),
                expected,
                "{level:?} {case:?}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 27 * LEVELS.len());
}

#[test]
fn every_k1_to_k7_observes_exactly_the_architectural_lane_bits() {
    let mut comparisons = 0usize;
    for kind in DuplicateKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for zeroing in [false, true] {
                for mask in 1..=7u8 {
                    let case = DuplicateMemoryCase {
                        kind,
                        width,
                        destination: 17 + (mask & 7),
                        base: 2,
                        control: if zeroing {
                            MaskControl::Zero
                        } else {
                            MaskControl::Merge
                        },
                    };
                    let mut bytes = case.bytes();
                    bytes[3] = (bytes[3] & !0x87) | mask | (u8::from(zeroing) << 7);
                    let function = optimize(function_from_bytes(&bytes, case), OptLevel::O2);
                    let active = (0xD6A5_3C69_F00F_5AA5u64
                        .rotate_left(u32::from(mask * 7) + case.ll() as u32))
                        & lane_mask(case.lanes());
                    let mut initial =
                        initial_state(case, usize::from(mask), MEMORY_ADDRESS, active);
                    initial.masks[usize::from(mask)] = active | !lane_mask(case.lanes());
                    let memory = memory_bytes(usize::from(mask));
                    assert_eq!(
                        interpret_success(&function, &initial, &memory),
                        manual_explicit(case, &initial, &memory, Some(mask), zeroing),
                        "K{mask} {case:?}"
                    );
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 3 * 2 * 7);
}

#[test]
fn empty_and_out_of_range_masks_do_not_suppress_e4nf_or_e5nf_tuple_faults() {
    let masked: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| case.mask() != 0)
        .collect();
    assert_eq!(masked.len(), 18);
    let mut faults = 0usize;
    for (ordinal, case) in masked.into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            for mask in [0, !lane_mask(case.lanes())] {
                let initial = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
                let mut unmapped = FlatMemory::with_base(MEMORY_ADDRESS, 0);
                let (result, actual) = execute(&function, &initial, &mut unmapped);
                assert!(
                    matches!(
                        result,
                        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                    ),
                    "{level:?} {case:?} mask={mask:#018X}: {result:?}"
                );
                assert_eq!(actual, initial, "{level:?} {case:?}: fault committed state");
                faults += 1;
            }
        }
    }
    assert_eq!(faults, 18 * LEVELS.len() * 2);
}

#[test]
fn every_partial_tuple_read_faults_without_committing_architectural_state() {
    let mut faults = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        let bytes = memory_bytes(ordinal);
        let size = case.memory_size() as usize - 1;
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
            assert_eq!(actual, initial, "{level:?} {case:?}: partial tuple commit");
            faults += 1;
        }
    }
    assert_eq!(faults, 27 * LEVELS.len());
}

#[test]
fn exact_tuple_size_is_sufficient_at_the_mapping_boundary() {
    let mut successes = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        let bytes = memory_bytes(ordinal);
        let size = case.memory_size() as usize;
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, ordinal, MEMORY_ADDRESS, lane_mask(case.lanes()));
            let mut exact = FlatMemory::with_base(MEMORY_ADDRESS, size);
            exact.load(0, &bytes[..size]);
            let (result, actual) = execute(&function, &initial, &mut exact);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(
                actual,
                manual(case, &initial, &bytes).state,
                "{level:?} {case:?}"
            );
            successes += 1;
        }
    }
    assert_eq!(successes, 27 * LEVELS.len());
}
