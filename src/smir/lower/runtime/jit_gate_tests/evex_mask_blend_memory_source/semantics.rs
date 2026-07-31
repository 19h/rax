//! Interpreter, optimizer, and Type E4 fault-suppression coverage.

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

pub(super) fn initial_state(case: MaskBlendMemoryCase, ordinal: usize) -> SemanticState {
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
            0x8000_0000_0000_0001,
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
    if case.destination == case.source1 {
        state.vectors[usize::from(case.source1)] =
            std::array::from_fn(|word| 0xFFFF_FFFF_0000_0001u64.rotate_left((word * 9) as u32));
    }
    state
}

pub(super) fn memory_bytes(case: MaskBlendMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let lanes = case.width.lanes(case.kind.elem()) as usize;
    for lane in 0..lanes {
        let value = 0xFEDC_BA98_7654_3210u64.rotate_left((lane * 7 + ordinal * 3) as u32)
            ^ (lane as u64).wrapping_mul(0x1020_4081_0204_0810)
            ^ (ordinal as u64).wrapping_mul(0x0101_0101_0101_0101);
        let lane_bytes = case.kind.elem().bytes() as usize;
        let offset = lane * lane_bytes;
        bytes[offset..offset + lane_bytes].copy_from_slice(&value.to_le_bytes()[..lane_bytes]);
    }
    bytes
}

fn lane_mask(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I8 => 0xFF,
        VecElementType::I16 => 0xFFFF,
        VecElementType::I32 => 0xFFFF_FFFF,
        VecElementType::I64 => u64::MAX,
        _ => unreachable!("EVEX mask-blend element"),
    }
}

fn get_lane(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    let bits = usize::try_from(elem.bytes()).unwrap() * 8;
    let lanes_per_word = 64 / bits;
    let shift = (lane % lanes_per_word) * bits;
    (vector[lane / lanes_per_word] >> shift) & lane_mask(elem)
}

fn set_lane(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    let bits = usize::try_from(elem.bytes()).unwrap() * 8;
    let lanes_per_word = 64 / bits;
    let shift = (lane % lanes_per_word) * bits;
    let mask = lane_mask(elem) << shift;
    let word = &mut vector[lane / lanes_per_word];
    *word = (*word & !mask) | ((value & lane_mask(elem)) << shift);
}

fn memory_lane(bytes: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    let lane_bytes = elem.bytes() as usize;
    let offset = lane * lane_bytes;
    let mut value = [0u8; 8];
    value[..lane_bytes].copy_from_slice(&bytes[offset..offset + lane_bytes]);
    u64::from_le_bytes(value)
}

fn manual(case: MaskBlendMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let elem = case.kind.elem();
    let lanes = case.width.lanes(elem) as usize;
    let selector = if case.selector() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.selector())]
    };
    let source1 = initial.vectors[usize::from(case.source1)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        let value = if selector & (1u64 << lane) != 0 {
            memory_lane(memory, if case.broadcast() { 0 } else { lane }, elem)
        } else if case.zeroing() {
            0
        } else {
            get_lane(&source1, lane, elem)
        };
        set_lane(destination, lane, elem, value);
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
    memory_bytes: &[u8; 64],
    case: MaskBlendMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        case.memory_width().bytes()
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
    state(&context)
}

#[test]
fn all_270_mask_blend_cells_match_manual_selector_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 270);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(case, ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret(&function, &initial, &memory, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 270 * LEVELS.len());
}

#[test]
fn empty_selectors_suppress_type_e4_accesses_and_faults_do_not_commit() {
    for case in [
        MaskBlendMemoryCase {
            kind: BlendKind::PackedByte,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        MaskBlendMemoryCase {
            kind: BlendKind::PackedDouble,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
    ] {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let bytes = memory_bytes(case, 0);
            let mut empty = initial_state(case, 0);
            empty.masks[usize::from(case.selector())] = 0;
            let expected = manual(case, &empty, &bytes);
            let mut empty_context = context(&empty);
            let mut unmapped = FlatMemory::new(0x1000);
            let result = SmirInterpreter::new().execute_block(
                &mut empty_context,
                &mut unmapped,
                &function.blocks[0],
            );
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(state(&empty_context), expected, "{level:?} {case:?}");

            let mut active = initial_state(case, 0);
            active.masks[usize::from(case.selector())] = 1;
            let mut fault_context = context(&active);
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
                active,
                "{level:?} {case:?}: fault committed architectural state"
            );
        }
    }
}
