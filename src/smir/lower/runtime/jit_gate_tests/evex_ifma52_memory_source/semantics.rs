//! Interpreter, optimizer, and Type E4 fault-suppression coverage.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;

const MASK52: u64 = (1u64 << 52) - 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SemanticState {
    pub(super) gpr: [u64; 32],
    pub(super) vectors: [[u64; 16]; 32],
    pub(super) masks: [u64; 8],
    pub(super) rflags: u64,
    pub(super) mxcsr: u32,
}

pub(super) fn initial_state(case: Ifma52MemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xFFF0_0000_0000_0001u64.rotate_left((register * 11 + word * 17 + ordinal) as u32)
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
    state.gpr[2] = 0x2000;
    for lane in 0..case.lanes() {
        state.vectors[usize::from(case.source1)][lane] =
            boundary_operand(lane, ordinal, 0x1357_9BDF_2468_ACE0);
        if case.source1 != case.destination {
            state.vectors[usize::from(case.destination)][lane] =
                boundary_accumulator(lane, ordinal);
        }
    }
    state
}

pub(super) fn memory_bytes(case: Ifma52MemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..case.lanes() {
        let value = boundary_operand(lane, ordinal, 0xFEDC_BA98_7654_3210);
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn boundary_operand(lane: usize, ordinal: usize, salt: u64) -> u64 {
    match lane % 8 {
        0 => 0,
        1 => 1,
        2 => MASK52,
        3 => MASK52 + 1,
        4 => u64::MAX,
        5 => 1u64 << 51,
        6 => 0x000A_BCDE_F012_3456,
        _ => salt
            .wrapping_add((ordinal as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .rotate_left((lane * 7) as u32),
    }
}

fn boundary_accumulator(lane: usize, ordinal: usize) -> u64 {
    match lane % 8 {
        0 => 0,
        1 => u64::MAX,
        2 => u64::MAX - MASK52 + 1,
        3 => 1u64 << 63,
        4 => MASK52,
        5 => MASK52 + 1,
        6 => 0xFFFF_FFFF_FFFF_0000,
        _ => 0x6A09_E667_F3BC_C909u64
            .wrapping_add(ordinal as u64)
            .rotate_left((lane * 9) as u32),
    }
}

fn memory_lane(memory: &[u8; 64], lane: usize) -> u64 {
    u64::from_le_bytes(memory[lane * 8..lane * 8 + 8].try_into().unwrap())
}

fn ifma52_lane(accumulator: u64, source1: u64, source2: u64, high: bool) -> u64 {
    let product = u128::from(source1 & MASK52) * u128::from(source2 & MASK52);
    let addend = if high {
        ((product >> 52) as u64) & MASK52
    } else {
        (product as u64) & MASK52
    };
    accumulator.wrapping_add(addend)
}

pub(super) fn manual(
    case: Ifma52MemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let source1 = initial.vectors[usize::from(case.source1)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..case.lanes() {
        destination[lane] = if mask & (1u64 << lane) != 0 {
            ifma52_lane(
                old_destination[lane],
                source1[lane],
                memory_lane(memory, if case.broadcast() { 0 } else { lane }),
                case.kind == Ifma52Kind::High,
            )
        } else if case.zeroing() {
            0
        } else {
            old_destination[lane]
        };
    }
    for word in case.lanes()..destination.len() {
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
    case: Ifma52MemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        8
    } else {
        case.width.bytes() as usize
    };
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
fn all_108_ifma52_cells_match_manual_104_bit_product_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 108);
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
    assert_eq!(comparisons, 108 * LEVELS.len());
}

#[test]
fn ifma52_low_high_masking_and_u64_wrap_boundaries_match_exact_bits() {
    for (ordinal, case) in [
        Ifma52MemoryCase {
            kind: Ifma52Kind::Low,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        Ifma52MemoryCase {
            kind: Ifma52Kind::High,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut initial = initial_state(case, ordinal);
        let mut memory = [0u8; 64];
        for lane in 0..case.lanes() {
            initial.vectors[usize::from(case.destination)][lane] = u64::MAX - lane as u64;
            initial.vectors[usize::from(case.source1)][lane] =
                if lane & 1 == 0 { u64::MAX } else { MASK52 };
            let source2 = if lane & 1 == 0 { MASK52 } else { MASK52 + 1 };
            memory[lane * 8..lane * 8 + 8].copy_from_slice(&source2.to_le_bytes());
        }
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert_eq!(
                interpret(&function, &initial, &memory, case),
                expected,
                "{level:?} {case:?}"
            );
        }
    }
}

#[test]
fn empty_masks_suppress_unmapped_ifma52_accesses_and_partial_faults_do_not_commit() {
    for case in [
        Ifma52MemoryCase {
            kind: Ifma52Kind::Low,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Ifma52MemoryCase {
            kind: Ifma52Kind::High,
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
            empty.masks[usize::from(case.mask())] = 0;
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
            active.masks[usize::from(case.mask())] = 1;
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

    let case = Ifma52MemoryCase {
        kind: Ifma52Kind::High,
        width: VecWidth::V512,
        destination: 17,
        source1: 18,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    for level in [OptLevel::O0, OptLevel::O2] {
        let function = optimize(lift_case(case), level);
        let mut initial = initial_state(case, 1);
        initial.masks[usize::from(case.mask())] = 0b0101;
        let bytes = memory_bytes(case, 1);
        let mut memory = FlatMemory::new(0x2010);
        memory.load(0x2000, &bytes[..16]);
        let mut partial_context = context(&initial);
        let result = SmirInterpreter::new().execute_block(
            &mut partial_context,
            &mut memory,
            &function.blocks[0],
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr: 0x2010,
                    write: false,
                    ..
                })
            ),
            "{level:?}: {result:?}"
        );
        assert_eq!(
            state(&partial_context),
            initial,
            "{level:?}: lane-2 fault committed lane-0 result"
        );
    }
}
