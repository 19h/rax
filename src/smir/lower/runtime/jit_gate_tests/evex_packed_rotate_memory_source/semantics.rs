//! Interpreter and optimizer equivalence for packed rotate memory sources.

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

pub(super) fn initial_state(case: RotateMemoryCase, ordinal: usize) -> SemanticState {
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
            0,
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    state.gpr[3] = 0x2000;
    if case.kind.variable() && case.source == Some(case.destination) {
        state.vectors[usize::from(case.destination)] =
            std::array::from_fn(|word| 0xFFFF_FFFF_0000_0001u64.rotate_left((word * 9) as u32));
    }
    state
}

pub(super) fn memory_bytes(case: RotateMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    match case.elem {
        VecElementType::I32 => {
            for lane in 0..16 {
                let value = [
                    0u32,
                    1,
                    31,
                    32,
                    33,
                    63,
                    64,
                    0xFFFF_FFFF,
                    0x8000_0001,
                    0x7FFF_FFFF,
                    7,
                    15,
                    16,
                    127,
                    128,
                    255,
                ][lane]
                    ^ (ordinal as u32).rotate_left((lane * 3) as u32);
                bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        VecElementType::I64 => {
            for lane in 0..8 {
                let value = [0u64, 1, 63, 64, 65, 127, 128, u64::MAX][lane]
                    ^ (ordinal as u64).rotate_left((lane * 7) as u32);
                bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
            }
        }
        _ => unreachable!("packed rotate element"),
    }
    bytes
}

fn get_lane(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I32 => {
            let word = vector[lane / 2];
            (word >> ((lane & 1) * 32)) & 0xFFFF_FFFF
        }
        VecElementType::I64 => vector[lane],
        _ => unreachable!("packed rotate element"),
    }
}

fn set_lane(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    match elem {
        VecElementType::I32 => {
            let word = &mut vector[lane / 2];
            let shift = (lane & 1) * 32;
            *word = (*word & !(0xFFFF_FFFFu64 << shift)) | ((value & 0xFFFF_FFFF) << shift);
        }
        VecElementType::I64 => vector[lane] = value,
        _ => unreachable!("packed rotate element"),
    }
}

fn memory_lane(bytes: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I32 => {
            u32::from_le_bytes(bytes[lane * 4..lane * 4 + 4].try_into().unwrap()) as u64
        }
        VecElementType::I64 => {
            u64::from_le_bytes(bytes[lane * 8..lane * 8 + 8].try_into().unwrap())
        }
        _ => unreachable!("packed rotate element"),
    }
}

fn manual(case: RotateMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(case.elem) as usize;
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let source_snapshot = case
        .source
        .map(|source| initial.vectors[usize::from(source)]);
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        if mask & (1u64 << lane) == 0 {
            if case.zeroing() {
                set_lane(destination, lane, case.elem, 0);
            } else {
                set_lane(
                    destination,
                    lane,
                    case.elem,
                    get_lane(&old_destination, lane, case.elem),
                );
            }
            continue;
        }

        let memory_index = if case.broadcast() { 0 } else { lane };
        let memory_value = memory_lane(memory, memory_index, case.elem);
        let (value, count) = if let Some(source) = source_snapshot {
            (get_lane(&source, lane, case.elem), memory_value)
        } else {
            (memory_value, u64::from(case.amount))
        };
        let rotated = match case.elem {
            VecElementType::I32 => {
                let value = value as u32;
                (if case.kind.left() {
                    value.rotate_left(count as u32)
                } else {
                    value.rotate_right(count as u32)
                }) as u64
            }
            VecElementType::I64 => {
                if case.kind.left() {
                    value.rotate_left(count as u32)
                } else {
                    value.rotate_right(count as u32)
                }
            }
            _ => unreachable!("packed rotate element"),
        };
        set_lane(destination, lane, case.elem, rotated);
    }
    for word in usize::try_from(case.width.bytes() / 8).unwrap()..destination.len() {
        destination[word] = 0;
    }
    expected
}

pub(super) fn interpret(
    function: &SmirFunction,
    initial: &SemanticState,
    memory_bytes: &[u8; 64],
    case: RotateMemoryCase,
) -> SemanticState {
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

#[test]
fn all_864_memory_cells_match_manual_integer_rotates_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 864);
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
    assert_eq!(comparisons, 864 * LEVELS.len());
}

#[test]
fn masked_broadcasts_use_one_aggregate_predicate_and_at_most_one_load() {
    for kind in RotateKind::ALL {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                let case = RotateMemoryCase {
                    kind,
                    elem,
                    width,
                    destination: 17,
                    source: kind.variable().then_some(18),
                    form: SourceForm::Broadcast,
                    control: MaskControl::Zero,
                    amount: 0xFF,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    assert_eq!(
                        function.blocks[0]
                            .ops
                            .iter()
                            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                            .count(),
                        1,
                        "{level:?} {case:?}: {:#?}",
                        function.blocks[0].ops
                    );
                    assert!(
                        function.blocks[0].ops.iter().any(|op| matches!(
                            op.kind,
                            OpKind::PredLoad {
                                width: MemWidth::B4 | MemWidth::B8,
                                ..
                            }
                        )),
                        "{level:?} {case:?}"
                    );
                }
            }
        }
    }
}
