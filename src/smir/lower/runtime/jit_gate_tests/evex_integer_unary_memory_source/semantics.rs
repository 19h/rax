//! Independent semantics, optimizer parity, and Type E4 fault coverage.

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

fn lane_mask(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I8 => 0xFF,
        VecElementType::I16 => 0xFFFF,
        VecElementType::I32 => 0xFFFF_FFFF,
        VecElementType::I64 => u64::MAX,
        _ => unreachable!("integer-unary element"),
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

fn source_value(elem: VecElementType, lane: usize, ordinal: usize) -> u64 {
    let mask = lane_mask(elem);
    let high = 1u64 << (u32::from(elem.bytes()) * 8 - 1);
    // Adjacent pairs deliberately repeat so every vector width exercises
    // conflict bits while the pair groups span leading-zero/popcount edges.
    match (lane / 2 + ordinal) % 8 {
        0 => 0,
        1 => 1,
        2 => high,
        3 => high - 1,
        4 => mask,
        5 => mask >> 1,
        6 => 0xA55A_A55A_A55A_A55A & mask,
        _ => (0x0102_0408_1020_4081u64.rotate_left((ordinal * 7) as u32)) & mask,
    }
}

pub(super) fn initial_state(_case: IntegerUnaryMemoryCase, ordinal: usize) -> SemanticState {
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
    state.gpr[3] = MEMORY_ADDRESS;
    state
}

pub(super) fn memory_bytes(case: IntegerUnaryMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let lanes = case.width.lanes(case.elem()) as usize;
    for lane in 0..lanes {
        let value = source_value(case.elem(), lane, ordinal);
        let lane_bytes = case.elem().bytes() as usize;
        let offset = lane * lane_bytes;
        bytes[offset..offset + lane_bytes].copy_from_slice(&value.to_le_bytes()[..lane_bytes]);
    }
    bytes
}

fn operation_result(case: IntegerUnaryMemoryCase, source: &[u64], lane: usize) -> u64 {
    match case.operation {
        IntegerUnaryOperation::ConflictD | IntegerUnaryOperation::ConflictQ => {
            let mut conflicts = 0u64;
            for previous in 0..lane {
                if source[previous] == source[lane] {
                    conflicts |= 1u64 << previous;
                }
            }
            conflicts
        }
        IntegerUnaryOperation::LeadingZerosD => u64::from((source[lane] as u32).leading_zeros()),
        IntegerUnaryOperation::LeadingZerosQ => u64::from(source[lane].leading_zeros()),
        IntegerUnaryOperation::PopcntB
        | IntegerUnaryOperation::PopcntW
        | IntegerUnaryOperation::PopcntD
        | IntegerUnaryOperation::PopcntQ => u64::from(source[lane].count_ones()),
    }
}

fn manual(
    case: IntegerUnaryMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(case.elem()) as usize;
    let source: Vec<_> = (0..lanes)
        .map(|lane| memory_lane(memory, if case.broadcast() { 0 } else { lane }, case.elem()))
        .collect();
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        let value = if mask & (1u64 << lane) != 0 {
            operation_result(case, &source, lane)
        } else if case.zeroing() {
            0
        } else {
            get_lane(&old_destination, lane, case.elem())
        };
        set_lane(destination, lane, case.elem(), value);
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
    case: IntegerUnaryMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        case.memory_width().bytes()
    } else {
        case.width.bytes()
    } as usize;
    memory.load(
        usize::try_from(MEMORY_ADDRESS).unwrap(),
        &memory_bytes[..size],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_126_integer_unary_cells_match_independent_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 126);
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
    assert_eq!(comparisons, 126 * LEVELS.len());
}

#[test]
fn empty_masks_suppress_type_e4_accesses_and_active_faults_do_not_commit() {
    for case in [
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::ConflictD,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::LeadingZerosQ,
            width: VecWidth::V256,
            destination: 9,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::PopcntB,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        IntegerUnaryMemoryCase {
            operation: IntegerUnaryOperation::PopcntQ,
            width: VecWidth::V512,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
    ] {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let bytes = memory_bytes(case, 17);
            let mut unmapped = FlatMemory::new(0x1000);

            let mut empty = initial_state(case, 17);
            empty.masks[usize::from(case.mask())] = 0;
            let expected = manual(case, &empty, &bytes);
            let mut empty_context = context(&empty);
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

            let mut active = initial_state(case, 17);
            let highest_lane = case.width.lanes(case.elem()) - 1;
            active.masks[usize::from(case.mask())] = 1u64 << highest_lane;
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
