//! Independent Intel-contract semantics, optimizer parity, and store faults.

use super::super::semantics::{SemanticState, initial_state as load_initial_state};
use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};

pub(super) fn initial_state(case: HalfMoveStoreCase, ordinal: usize) -> SemanticState {
    load_initial_state(
        HalfMoveCase {
            lane: case.lane,
            format: case.format,
            destination: 0,
            source1: case.source,
        },
        ordinal,
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

pub(super) fn expected_store_value(initial: &SemanticState, case: HalfMoveStoreCase) -> u64 {
    initial.vectors[usize::from(case.source)][usize::from(case.lane.index())]
}

#[test]
fn all_384_source_format_lane_optimizer_cells_match_single_qword_store_contract() {
    let mut comparisons = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for source in 0..32u8 {
                let case = HalfMoveStoreCase {
                    lane,
                    format,
                    source,
                };
                for level in LEVELS {
                    let function = optimize(lift_store_case(case), level);
                    let initial = initial_state(case, comparisons);
                    let stored = expected_store_value(&initial, case);
                    let mut memory = FlatMemory::new(0x5000);
                    let before = std::array::from_fn::<u8, 24, _>(|index| {
                        0xA5u8 ^ (index as u8).wrapping_mul(0x13)
                    });
                    memory.load(MEMORY_ADDRESS as usize - 8, &before);

                    let (result, actual_state) = execute(&function, &initial, &mut memory);
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                        "{level:?} {case:?}: {result:?}"
                    );
                    assert_eq!(actual_state, initial, "{level:?} {case:?}: state");

                    let mut actual_memory = [0u8; 24];
                    memory.read(MEMORY_ADDRESS - 8, &mut actual_memory).unwrap();
                    let mut expected_memory = before;
                    expected_memory[8..16].copy_from_slice(&stored.to_le_bytes());
                    assert_eq!(
                        actual_memory, expected_memory,
                        "{level:?} {case:?}: exact 8-byte memory footprint"
                    );
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 2 * 2 * 32 * LEVELS.len());
}

#[test]
fn unconditional_store_fault_is_precise_and_noncommitting_at_every_level() {
    for (ordinal, case) in representative_store_cases().into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_store_case(case), level);
            let initial = initial_state(case, ordinal);
            let mut memory = FlatMemory::new(0);
            let (result, actual) = execute(&function, &initial, &mut memory);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr: MEMORY_ADDRESS,
                        write: true,
                    })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, initial, "{level:?} {case:?}: state commit");
        }
    }
}
