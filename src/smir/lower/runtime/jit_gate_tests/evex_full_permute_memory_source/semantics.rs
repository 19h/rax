//! Raw-bit interpreter, optimizer, and Type-E4NF fault coverage.

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

fn element_mask(elem: VecElementType) -> u64 {
    match elem.bytes() {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        8 => u64::MAX,
        _ => unreachable!("full-permute element width"),
    }
}

fn get_element(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    let bits = elem.bytes() as usize * 8;
    (vector[lane * bits / 64] >> (lane * bits % 64)) & element_mask(elem)
}

fn set_element(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    let bits = elem.bytes() as usize * 8;
    let word = &mut vector[lane * bits / 64];
    let shift = lane * bits % 64;
    let mask = element_mask(elem);
    *word = (*word & !(mask << shift)) | ((value & mask) << shift);
}

pub(super) fn initial_state(case: PermuteMemoryCase, ordinal: usize) -> SemanticState {
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

    if case.kind.variable() {
        let lanes = case.width.lanes(case.elem()) as usize;
        let indices = &mut state.vectors[usize::from(case.indices)];
        for lane in 0..lanes {
            let selected = (lane * 5 + 3) & (lanes - 1);
            let ignored_high = (lane as u64).wrapping_mul(0x9E37) & !(lanes - 1) as u64;
            set_element(indices, lane, case.elem(), selected as u64 | ignored_high);
        }
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

fn memory_element(memory: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    let bytes = elem.bytes() as usize;
    let start = lane * bytes;
    let mut value = 0u64;
    for byte in 0..bytes {
        value |= u64::from(memory[start + byte]) << (byte * 8);
    }
    value
}

fn source_lane(case: PermuteMemoryCase, initial: &SemanticState, lane: usize) -> usize {
    if case.kind.variable() {
        let lanes = case.width.lanes(case.elem()) as usize;
        get_element(
            &initial.vectors[usize::from(case.indices)],
            lane,
            case.elem(),
        ) as usize
            & (lanes - 1)
    } else {
        usize::from(
            case.expected_control()
                .source_lane(lane as u8)
                .expect("immediate selector"),
        )
    }
}

fn manual(case: PermuteMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(case.elem()) as usize;
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        let value = if mask & (1u64 << lane) == 0 {
            if case.zeroing() {
                0
            } else {
                get_element(&old_destination, lane, case.elem())
            }
        } else {
            let source_lane = if case.form == SourceForm::Broadcast {
                0
            } else {
                source_lane(case, initial, lane)
            };
            memory_element(memory, source_lane, case.elem())
        };
        set_element(destination, lane, case.elem(), value);
    }
    for word in case.width.bytes() as usize / 8..destination.len() {
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
    case: PermuteMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let tuple_bytes = if case.form == SourceForm::Broadcast {
        case.elem().bytes()
    } else {
        case.width.bytes()
    } as usize;
    let mut memory = FlatMemory::with_base(0x2000, tuple_bytes);
    memory.load(0, &bytes[..tuple_bytes]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

fn semantic_cases() -> Vec<PermuteMemoryCase> {
    let destinations = [0u8, 9, 17, 31];
    scanner_cases()
        .into_iter()
        .enumerate()
        .map(|(ordinal, mut case)| {
            case.destination = destinations[ordinal & 3];
            case
        })
        .collect()
}

#[test]
fn all_258_raw_bit_cells_match_manual_selectors_aliases_masks_and_tuples_at_all_levels() {
    let cases = semantic_cases();
    assert_eq!(cases.len(), 258);
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
    assert_eq!(comparisons, 258 * LEVELS.len());
}

#[test]
fn empty_masks_still_fault_before_any_architectural_commit_for_every_operation_and_tuple() {
    let mut faults = 0usize;
    for kind in PermuteKind::ALL {
        for &width in widths(kind) {
            for &form in forms(kind) {
                for control in [MaskControl::Merge, MaskControl::Zero] {
                    let case = PermuteMemoryCase {
                        kind,
                        width,
                        destination: 17,
                        indices: 18,
                        form,
                        control,
                        immediate: 0xA5,
                    };
                    for level in [OptLevel::O0, OptLevel::O2] {
                        let function = optimize(lift_case(case), level);
                        assert!(
                            !function.blocks[0]
                                .ops
                                .iter()
                                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                        );
                        let mut initial = initial_state(case, faults);
                        initial.masks[usize::from(case.mask())] = 0;
                        let mut fault_context = context(&initial);
                        let mut unmapped = FlatMemory::with_base(0x2000, 0);
                        let result = SmirInterpreter::new().execute_block(
                            &mut fault_context,
                            &mut unmapped,
                            &function.blocks[0],
                        );
                        assert!(
                            matches!(
                                result,
                                BlockResult::Exit(ExitReason::MemoryFault {
                                    addr: 0x2000,
                                    write: false
                                })
                            ),
                            "{level:?} {case:?}: {result:?}"
                        );
                        assert_eq!(
                            state(&fault_context),
                            initial,
                            "{level:?} {case:?}: fault committed state"
                        );
                        faults += 1;
                    }
                }
            }
        }
    }
    assert_eq!(faults, 168);
}

#[test]
fn full_vector_tuples_fault_on_partial_mapping_even_when_every_selector_uses_lane_zero() {
    let mut faults = 0usize;
    for kind in PermuteKind::ALL {
        for &width in widths(kind) {
            let case = PermuteMemoryCase {
                kind,
                width,
                destination: 17,
                indices: 18,
                form: SourceForm::Vector,
                control: MaskControl::None,
                immediate: 0,
            };
            for level in [OptLevel::O0, OptLevel::O2] {
                let function = optimize(lift_case(case), level);
                let mut initial = initial_state(case, faults);
                if kind.variable() {
                    initial.vectors[usize::from(case.indices)] = [0; 16];
                }
                let bytes = memory_bytes(faults);
                let mapped = case.elem().bytes() as usize;
                let mut partial = FlatMemory::with_base(0x2000, mapped);
                partial.load(0, &bytes[..mapped]);
                let mut fault_context = context(&initial);
                let result = SmirInterpreter::new().execute_block(
                    &mut fault_context,
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
                assert_eq!(state(&fault_context), initial, "{level:?} {case:?}");
                faults += 1;
            }
        }
    }
    assert_eq!(faults, 48);
}

#[test]
fn broadcast_tuples_read_exactly_one_scalar_at_the_mapping_boundary() {
    let mut successes = 0usize;
    for kind in PermuteKind::ALL
        .into_iter()
        .filter(|kind| kind.allows_broadcast())
    {
        for &width in widths(kind) {
            let case = PermuteMemoryCase {
                kind,
                width,
                destination: 17,
                indices: 18,
                form: SourceForm::Broadcast,
                control: MaskControl::None,
                immediate: 0xA5,
            };
            let initial = initial_state(case, successes);
            let bytes = memory_bytes(successes);
            let expected = manual(case, &initial, &bytes);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let actual = interpret(&function, &initial, &bytes, case);
                assert_eq!(actual, expected, "{level:?} {case:?}");
                successes += 1;
            }
        }
    }
    assert_eq!(successes, 54);
}
