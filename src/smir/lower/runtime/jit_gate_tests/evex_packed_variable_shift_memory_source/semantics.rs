//! Interpreter, optimizer, and E4 fault-suppression coverage.

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

pub(super) fn initial_state(case: ShiftMemoryCase, ordinal: usize) -> SemanticState {
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
            [0, u64::MAX, 0xA55A_3CC3_F00F_9696, 0x8000_0000_0000_0001][ordinal & 3],
            u64::MAX,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    // EVEX disp8 is scaled by the full-vector or broadcast tuple.
    state.gpr[0] = 0x1000;
    state.gpr[1] = (0x2000 - state.gpr[0] - case.compressed_displacement() as u64) / 2;
    if case.source == case.destination {
        state.vectors[usize::from(case.destination)] =
            std::array::from_fn(|word| 0xFFFF_0001_8000_7FFFu64.rotate_left((word * 9) as u32));
    }
    state
}

pub(super) fn memory_bytes(case: ShiftMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    match case.kind.elem {
        VecElementType::I16 => {
            let counts = [0u16, 1, 15, 16, 17, 31, 32, u16::MAX];
            for lane in 0..32 {
                let value = counts[(lane + ordinal) % counts.len()];
                bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        VecElementType::I32 => {
            let counts = [0u32, 1, 31, 32, 33, 63, 64, u32::MAX];
            for lane in 0..16 {
                let value = counts[(lane + ordinal) % counts.len()];
                bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        VecElementType::I64 => {
            let counts = [0u64, 1, 63, 64, 65, 127, 128, u64::MAX];
            for lane in 0..8 {
                let value = counts[(lane + ordinal) % counts.len()];
                bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
            }
        }
        _ => unreachable!("packed variable-shift integer element"),
    }
    bytes
}

fn get_lane(vector: &[u64; 16], lane: usize, elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I16 => (vector[lane / 4] >> ((lane & 3) * 16)) & 0xFFFF,
        VecElementType::I32 => (vector[lane / 2] >> ((lane & 1) * 32)) & 0xFFFF_FFFF,
        VecElementType::I64 => vector[lane],
        _ => unreachable!("packed variable-shift integer element"),
    }
}

fn set_lane(vector: &mut [u64; 16], lane: usize, elem: VecElementType, value: u64) {
    let (word, shift, mask) = match elem {
        VecElementType::I16 => (lane / 4, (lane & 3) * 16, 0xFFFFu64),
        VecElementType::I32 => (lane / 2, (lane & 1) * 32, 0xFFFF_FFFFu64),
        VecElementType::I64 => (lane, 0, u64::MAX),
        _ => unreachable!("packed variable-shift integer element"),
    };
    vector[word] = (vector[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn memory_lane(bytes: &[u8; 64], lane: usize, elem: VecElementType) -> u64 {
    match elem {
        VecElementType::I16 => {
            u16::from_le_bytes(bytes[lane * 2..lane * 2 + 2].try_into().unwrap()) as u64
        }
        VecElementType::I32 => {
            u32::from_le_bytes(bytes[lane * 4..lane * 4 + 4].try_into().unwrap()) as u64
        }
        VecElementType::I64 => {
            u64::from_le_bytes(bytes[lane * 8..lane * 8 + 8].try_into().unwrap())
        }
        _ => unreachable!("packed variable-shift integer element"),
    }
}

fn shifted(value: u64, count: u64, elem: VecElementType, shift: ShiftOp) -> u64 {
    let bits = u64::from(elem.bytes()) * 8;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let value = value & mask;
    if count >= bits {
        return match shift {
            ShiftOp::Lsl | ShiftOp::Lsr => 0,
            ShiftOp::Asr => {
                if value & (1u64 << (bits - 1)) != 0 {
                    mask
                } else {
                    0
                }
            }
            _ => unreachable!("packed variable-shift operation"),
        };
    }
    let count = count as u32;
    match (elem, shift) {
        (_, ShiftOp::Lsl) => (value << count) & mask,
        (_, ShiftOp::Lsr) => value >> count,
        (VecElementType::I16, ShiftOp::Asr) => ((value as u16 as i16) >> count) as u16 as u64,
        (VecElementType::I32, ShiftOp::Asr) => ((value as u32 as i32) >> count) as u32 as u64,
        (VecElementType::I64, ShiftOp::Asr) => ((value as i64) >> count) as u64,
        _ => unreachable!("packed variable-shift operation"),
    }
}

fn manual(case: ShiftMemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(case.kind.elem) as usize;
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let source = initial.vectors[usize::from(case.source)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        if mask & (1u64 << lane) == 0 {
            set_lane(
                destination,
                lane,
                case.kind.elem,
                if case.zeroing() {
                    0
                } else {
                    get_lane(&old_destination, lane, case.kind.elem)
                },
            );
            continue;
        }
        let count_lane = if case.broadcast() { 0 } else { lane };
        let count = memory_lane(memory, count_lane, case.kind.elem);
        let value = get_lane(&source, lane, case.kind.elem);
        set_lane(
            destination,
            lane,
            case.kind.elem,
            shifted(value, count, case.kind.elem, case.kind.shift),
        );
    }
    for word in usize::try_from(case.width.bytes() / 8).unwrap()..destination.len() {
        destination[word] = 0;
    }
    expected
}

fn context_from_state(initial: &SemanticState) -> SmirContext {
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

pub(super) fn interpret(
    function: &SmirFunction,
    initial: &SemanticState,
    memory_bytes: &[u8; 64],
    case: ShiftMemoryCase,
) -> SemanticState {
    let mut context = context_from_state(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        case.kind.elem.bytes()
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
fn all_405_memory_cells_match_manual_shift_model_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 405);
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
    assert_eq!(comparisons, 405 * LEVELS.len());
}

#[test]
fn inactive_e4_lanes_suppress_faults_and_faults_do_not_commit_destination() {
    for kind in ShiftKind::ALL {
        for form in [SourceForm::Vector, SourceForm::Broadcast] {
            if kind.elem == VecElementType::I16 && form == SourceForm::Broadcast {
                continue;
            }
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = ShiftMemoryCase {
                    kind,
                    width: VecWidth::V512,
                    destination: 17,
                    source: 18,
                    form,
                    control,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_state(case, 0);
                    initial.gpr[0] = 0x8000;
                    initial.gpr[1] = 0;
                    initial.masks[usize::from(case.mask())] = 0;
                    let mut context = context_from_state(&initial);
                    let mut memory = FlatMemory::new(0x100);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                        "{level:?} {case:?}: {result:?}"
                    );

                    let mut active = initial.clone();
                    active.masks[usize::from(case.mask())] = 1;
                    let old_destination = active.vectors[usize::from(case.destination)];
                    let mut context = context_from_state(&active);
                    let mut memory = FlatMemory::new(0x100);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ),
                        "{level:?} {case:?}: {result:?}"
                    );
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(
                        x86.xmm[usize::from(case.destination)],
                        old_destination,
                        "{level:?} {case:?}"
                    );
                }
            }
        }
    }
}
