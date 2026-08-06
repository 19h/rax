//! Independent Intel-contract semantics, optimizer parity, and fault precision.

use super::*;
use crate::smir::TrapKind;
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

pub(super) fn initial_state(case: HalfMoveCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0202_0404_0808)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3u64
                    .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            })
        }),
        masks: std::array::from_fn(|register| {
            0x0102_0304_0506_0708u64.rotate_left((register * 7) as u32)
        }),
        rflags: 0x2 | ((ordinal as u64).wrapping_mul(0x145) & 0x8D5),
        mxcsr: 0x1F80 | (ordinal as u32 & 0x3F),
    };
    state.gpr[2] = MEMORY_ADDRESS;
    assert!(case.destination < 32 && case.source1 < 32);
    state
}

pub(super) fn manual_destination(
    case: HalfMoveCase,
    source1: &[u64; 16],
    memory: u64,
) -> [u64; 16] {
    let mut result = [0u64; 16];
    result[usize::from(case.lane.index())] = memory;
    result[usize::from(1 - case.lane.index())] = source1[usize::from(1 - case.lane.index())];
    result
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
    let mut function = function.clone();
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut context = context(initial);
    let result = SmirInterpreter::new().execute_block(&mut context, memory, &function.blocks[0]);
    (result, state(&context))
}

pub(super) fn interpret_success(
    function: &SmirFunction,
    initial: &SemanticState,
    memory_value: u64,
) -> SemanticState {
    let mut memory = FlatMemory::new(0x5000);
    memory.load(MEMORY_ADDRESS as usize, &memory_value.to_le_bytes());
    let (result, state) = execute(function, initial, &mut memory);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    state
}

#[test]
fn all_8192_operand_format_lane_optimizer_cells_match_intel_bit_transfer_contract() {
    let mut comparisons = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for destination in 0..32u8 {
                for source1 in 0..32u8 {
                    let case = HalfMoveCase {
                        lane,
                        format,
                        destination,
                        source1,
                    };
                    let ordinal = comparisons;
                    let memory_value = 0xFEDC_BA98_7654_3210u64
                        ^ (ordinal as u64).wrapping_mul(0x0101_0202_0404_0808);
                    let initial = initial_state(case, ordinal);
                    let mut expected = initial.clone();
                    expected.vectors[usize::from(destination)] = manual_destination(
                        case,
                        &initial.vectors[usize::from(source1)],
                        memory_value,
                    );
                    for level in [OptLevel::O0, OptLevel::O2] {
                        let function = optimize(lift_case(case), level);
                        let actual = interpret_success(&function, &initial, memory_value);
                        assert_eq!(actual, expected, "{level:?} {case:?}");
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 2 * 2 * 32 * 32 * 2);
}

#[test]
fn unconditional_source_fault_precedes_destination_commit_at_every_level() {
    for (ordinal, case) in representative_cases().into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, ordinal);
            let mut memory = FlatMemory::new(0);
            let (result, actual) = execute(&function, &initial, &mut memory);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr: MEMORY_ADDRESS,
                        write: false,
                    })
                ),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(actual, initial, "{level:?} {case:?}");
        }
    }
}
