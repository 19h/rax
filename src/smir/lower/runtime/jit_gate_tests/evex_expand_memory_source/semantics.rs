//! Independent semantics, optimizer parity, and Type-E4 fault coverage.

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
        VecElementType::I32 | VecElementType::F32 => 0xFFFF_FFFF,
        VecElementType::I64 | VecElementType::F64 => u64::MAX,
        _ => unreachable!("packed expand element"),
    }
}

fn get_lane(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    let bits = elem.bytes() as usize * 8;
    let lanes_per_word = 64 / bits;
    let shift = (lane % lanes_per_word) * bits;
    (vector[lane / lanes_per_word] >> shift) & lane_mask(elem)
}

fn set_lane(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    let bits = elem.bytes() as usize * 8;
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

pub(super) fn initial_state(case: ExpandMemoryCase, ordinal: usize) -> SemanticState {
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
    state.gpr[2] = MEMORY_ADDRESS;
    // Vary the active density while retaining at least one low active lane.
    state.masks[usize::from(case.mask())] = (0xA5A5_9696_3CC3_6996u64.rotate_left(ordinal as u32)
        | 1)
        & if case.lanes() == 64 {
            u64::MAX
        } else {
            (1u64 << case.lanes()) - 1
        };
    state
}

pub(super) fn memory_bytes(case: ExpandMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..case.lanes() {
        let value = (0x0102_0408_1020_4081u64.rotate_left((lane * 7 + ordinal * 11) as u32)
            ^ (lane as u64).wrapping_mul(0x1111_0101_0011_1001)
            ^ ((ordinal as u64) << 52))
            & lane_mask(case.elem());
        let lane_bytes = case.elem().bytes() as usize;
        let offset = lane * lane_bytes;
        bytes[offset..offset + lane_bytes].copy_from_slice(&value.to_le_bytes()[..lane_bytes]);
    }
    bytes
}

fn manual(case: ExpandMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let control = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    let mut input = 0usize;
    for lane in 0..case.lanes() {
        let value = if control & (1u64 << lane) != 0 {
            let value = memory_lane(memory, input, case.elem());
            input += 1;
            value
        } else if case.zeroing() {
            0
        } else {
            get_lane(&old_destination, lane, case.elem())
        };
        set_lane(destination, lane, case.elem(), value);
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

fn assert_state_eq(actual: &SemanticState, expected: &SemanticState, label: &str) {
    assert_eq!(actual.gpr, expected.gpr, "{label}: GPR state");
    for register in 0..actual.vectors.len() {
        assert_eq!(
            actual.vectors[register], expected.vectors[register],
            "{label}: ZMM{register}"
        );
    }
    assert_eq!(actual.masks, expected.masks, "{label}: opmask state");
    assert_eq!(actual.rflags, expected.rflags, "{label}: RFLAGS");
    assert_eq!(actual.mxcsr, expected.mxcsr, "{label}: MXCSR");
}

pub(super) fn interpret(
    function: &SmirFunction,
    initial: &SemanticState,
    bytes: &[u8; 64],
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    memory.load(MEMORY_ADDRESS as usize, bytes);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_54_expand_cells_match_independent_dense_mapping_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let bytes = memory_bytes(case, ordinal);
        let expected = manual(case, &initial, &bytes);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret(&function, &initial, &bytes);
            assert_state_eq(&actual, &expected, &format!("{level:?} {case:?}"));
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 54 * LEVELS.len());
}

#[test]
fn empty_masks_suppress_all_accesses_and_apply_merge_or_zero_policy() {
    for operation in ExpandOperation::ALL {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ExpandMemoryCase {
                operation,
                width: VecWidth::V512,
                destination: 17,
                control,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let bytes = memory_bytes(case, 91);
                let mut initial = initial_state(case, 91);
                initial.masks[usize::from(case.mask())] = 0;
                let expected = manual(case, &initial, &bytes);
                let mut context = context(&initial);
                let mut unmapped = FlatMemory::new(0x1000);
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut unmapped,
                    &function.blocks[0],
                );
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                assert_state_eq(&state(&context), &expected, &format!("{level:?} {case:?}"));
            }
        }
    }
}

#[test]
fn active_faults_after_prior_dense_reads_do_not_commit_destination_or_state() {
    let case = ExpandMemoryCase {
        operation: ExpandOperation::ExpandD,
        width: VecWidth::V512,
        destination: 17,
        control: MaskControl::Merge,
    };
    let bytes = memory_bytes(case, 113);
    for level in LEVELS {
        let function = optimize(lift_case(case), level);

        let mut initial = initial_state(case, 113);
        initial.masks[usize::from(case.mask())] = (1 << 0) | (1 << 15);
        let mut fault_context = context(&initial);
        // Dense element zero succeeds at 0x2000; dense element one faults at
        // the exact end of this 0x2004-byte memory.
        let mut partial = FlatMemory::new((MEMORY_ADDRESS + 4) as usize);
        partial.load(MEMORY_ADDRESS as usize, &bytes[..4]);
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
            "{level:?}: {result:?}"
        );
        assert_eq!(
            state(&fault_context),
            initial,
            "{level:?}: a later dense read fault committed architectural state"
        );
    }
}
