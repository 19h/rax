//! Interpreter, optimizer, and Type E4NF.nb coverage for EVEX `VPALIGNR`.

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

pub(super) fn initial_state(case: AlignrMemoryCase, ordinal: usize) -> SemanticState {
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
            0xA55A_3CC3_F00F_9696,
            0x5AA5_C33C_0FF0_6969,
            0x9696_6996_A55A_3CC3,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    state.gpr[3] = 0x2000;
    if case.high == case.destination {
        state.vectors[usize::from(case.high)] =
            std::array::from_fn(|word| 0xFFFF_FFFF_0000_0001u64.rotate_left((word * 9) as u32));
    }
    state
}

pub(super) fn memory_bytes(ordinal: usize) -> [u8; 64] {
    std::array::from_fn(|byte| {
        (byte as u8)
            .wrapping_mul(29)
            .wrapping_add((ordinal as u8).rotate_left((byte & 7) as u32))
            ^ 0xA5
    })
}

fn get_byte(vector: &[u64; 16], lane: usize) -> u8 {
    ((vector[lane / 8] >> ((lane & 7) * 8)) & 0xFF) as u8
}

fn set_byte(vector: &mut [u64; 16], lane: usize, value: u8) {
    let word = &mut vector[lane / 8];
    let shift = (lane & 7) * 8;
    *word = (*word & !(0xFFu64 << shift)) | (u64::from(value) << shift);
}

fn manual(case: AlignrMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(VecElementType::I8) as usize;
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let high = initial.vectors[usize::from(case.high)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        if mask & (1u64 << lane) == 0 {
            set_byte(
                destination,
                lane,
                if case.zeroing() {
                    0
                } else {
                    get_byte(&old_destination, lane)
                },
            );
            continue;
        }
        let block_base = lane / 16 * 16;
        let selected = usize::from(case.immediate) + (lane & 15);
        let value = if selected < 16 {
            memory[block_base + selected]
        } else if selected < 32 {
            get_byte(&high, block_base + selected - 16)
        } else {
            0
        };
        set_byte(destination, lane, value);
    }
    for word in usize::try_from(case.width.bytes() / 8).unwrap()..destination.len() {
        destination[word] = 0;
    }
    expected
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

pub(super) fn interpret(
    function: &SmirFunction,
    initial: &SemanticState,
    bytes: &[u8; 64],
    case: AlignrMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    memory.load(0x2000, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

fn semantic_cases() -> Vec<AlignrMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for (destination, high) in [(0, 0), (9, 10), (17, 18), (31, 31)] {
            for control in MaskControl::ALL {
                for w in [false, true] {
                    for immediate in [0, 1, 15, 16, 17, 31, 32, u8::MAX] {
                        cases.push(AlignrMemoryCase {
                            width,
                            destination,
                            high,
                            control,
                            immediate,
                            w,
                        });
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn all_576_cases_match_manual_per_128_bit_concatenation_at_o0_o1_o2() {
    let cases = semantic_cases();
    assert_eq!(cases.len(), 576);
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
    assert_eq!(comparisons, 576 * LEVELS.len());
}

#[test]
fn empty_masks_and_register_only_selectors_still_fault_without_commit() {
    for immediate in [16, 32, u8::MAX] {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = AlignrMemoryCase {
                width: VecWidth::V512,
                destination: 17,
                high: 18,
                control,
                immediate,
                w: false,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                assert!(matches!(
                    function.blocks[0].ops[sequence_index(&function)].kind,
                    OpKind::VLoad {
                        width: VecWidth::V512,
                        ..
                    }
                ));
                assert!(
                    !function.blocks[0]
                        .ops
                        .iter()
                        .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                );

                let mut empty = initial_state(case, usize::from(immediate));
                empty.masks[usize::from(case.mask())] = 0;
                let mut fault_context = context(&empty);
                let mut unmapped = FlatMemory::new(0x1000);
                let result = SmirInterpreter::new().execute_block(
                    &mut fault_context,
                    &mut unmapped,
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
                    state(&fault_context),
                    empty,
                    "{level:?} {case:?}: fault committed state"
                );
            }
        }
    }
}
