//! Interpreter, optimizer, and Type E4/E4NF memory semantics.

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

fn bf16(value: f32) -> u16 {
    let bits = value.to_bits();
    assert_eq!(bits & 0xFFFF, 0, "test input must be exactly BF16");
    (bits >> 16) as u16
}

fn set_u32_lane(vector: &mut [u64; 16], lane: usize, value: u32) {
    let word = &mut vector[lane / 2];
    let shift = (lane % 2) * 32;
    *word = (*word & !(0xFFFF_FFFFu64 << shift)) | (u64::from(value) << shift);
}

fn get_u32_lane(vector: &[u64; 16], lane: usize) -> u32 {
    (vector[lane / 2] >> ((lane % 2) * 32)) as u32
}

fn set_u16_lane(vector: &mut [u64; 16], lane: usize, value: u16) {
    let word = &mut vector[lane / 4];
    let shift = (lane % 4) * 16;
    *word = (*word & !(0xFFFFu64 << shift)) | (u64::from(value) << shift);
}

fn get_u16_lane(vector: &[u64; 16], lane: usize) -> u16 {
    (vector[lane / 4] >> ((lane % 4) * 16)) as u16
}

pub(super) fn initial_state(case: Bf16MemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA5A5_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 3) as u32)
                ^ (register as u64).wrapping_mul(0x0101_0204_0810_2040)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 11 + word * 7 + ordinal) as u32)
            })
        }),
        masks: [
            u64::MAX,
            0x8000_0001,
            0x5AA5_C33C,
            0x9696_6996,
            u64::MAX,
            0x5555_AAAA,
            0x8000_0001,
            0xF0F0_0F0F,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80,
    };
    state.gpr[3] = 0x2000;

    let lanes = case.width.lanes(VecElementType::F32) as usize;
    match case.kind {
        Bf16Kind::ConvertOne => {}
        Bf16Kind::ConvertTwo => {
            let source = &mut state.vectors[usize::from(case.source1)];
            for lane in 0..lanes {
                let value = ((lane + ordinal) % 9) as f32 * 0.5 - 2.0;
                set_u32_lane(source, lane, value.to_bits());
            }
        }
        Bf16Kind::DotProduct => {
            let source = &mut state.vectors[usize::from(case.source1)];
            for lane in 0..lanes {
                // The packed pair is also a finite FP32 accumulator when
                // destination and source1 alias: low BF16 = +0, high BF16 is
                // an exact finite value.
                let high = bf16(((lane + ordinal) % 5) as f32 * 0.5 + 0.5);
                set_u32_lane(source, lane, u32::from(high) << 16);
            }
            if case.destination != case.source1 {
                let destination = &mut state.vectors[usize::from(case.destination)];
                for lane in 0..lanes {
                    let value = ((lane + ordinal) % 7) as f32 - 3.0;
                    set_u32_lane(destination, lane, value.to_bits());
                }
            }
        }
    }
    state
}

pub(super) fn memory_bytes(case: Bf16MemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let lanes = case.width.lanes(VecElementType::F32) as usize;
    for lane in 0..lanes {
        let value = match case.kind {
            Bf16Kind::ConvertOne | Bf16Kind::ConvertTwo => {
                let value = ((lane * 3 + ordinal) % 11) as f32 * 0.25 - 1.0;
                value.to_bits()
            }
            Bf16Kind::DotProduct => {
                let low = bf16(((lane * 2 + ordinal) % 5) as f32 - 1.0);
                let high = bf16(((lane * 3 + ordinal) % 7) as f32 * 0.5 - 1.5);
                u32::from(low) | (u32::from(high) << 16)
            }
        };
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn memory_u32(bytes: &[u8; 64], lane: usize) -> u32 {
    u32::from_le_bytes(bytes[lane * 4..lane * 4 + 4].try_into().unwrap())
}

fn bf16_to_f32(value: u16) -> f32 {
    f32::from_bits(u32::from(value) << 16)
}

fn manual(case: Bf16MemoryCase, initial: &SemanticState, memory: &[u8; 64]) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(VecElementType::F32) as usize;
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let source1 = initial.vectors[usize::from(case.source1)];
    let original = initial.vectors[usize::from(case.destination)];
    let destination = &mut expected.vectors[usize::from(case.destination)];

    match case.kind {
        Bf16Kind::ConvertOne => {
            for lane in 0..lanes {
                let active = mask & (1u64 << lane) != 0;
                let value = if active {
                    let memory_lane = if case.broadcast() { 0 } else { lane };
                    bf16(f32::from_bits(memory_u32(memory, memory_lane)))
                } else if case.zeroing() {
                    0
                } else {
                    get_u16_lane(&original, lane)
                };
                set_u16_lane(destination, lane, value);
            }
        }
        Bf16Kind::ConvertTwo => {
            for lane in 0..lanes * 2 {
                let active = mask & (1u64 << lane) != 0;
                let value = if active {
                    if lane < lanes {
                        let memory_lane = if case.broadcast() { 0 } else { lane };
                        bf16(f32::from_bits(memory_u32(memory, memory_lane)))
                    } else {
                        bf16(f32::from_bits(get_u32_lane(&source1, lane - lanes)))
                    }
                } else if case.zeroing() {
                    0
                } else {
                    get_u16_lane(&original, lane)
                };
                set_u16_lane(destination, lane, value);
            }
        }
        Bf16Kind::DotProduct => {
            for lane in 0..lanes {
                let value = if mask & (1u64 << lane) != 0 {
                    let memory_lane = if case.broadcast() { 0 } else { lane };
                    let lhs = get_u32_lane(&source1, lane);
                    let rhs = memory_u32(memory, memory_lane);
                    let accumulator = f32::from_bits(get_u32_lane(&original, lane));
                    let high = bf16_to_f32((lhs >> 16) as u16)
                        .mul_add(bf16_to_f32((rhs >> 16) as u16), accumulator);
                    bf16_to_f32(lhs as u16)
                        .mul_add(bf16_to_f32(rhs as u16), high)
                        .to_bits()
                } else if case.zeroing() {
                    0
                } else {
                    get_u32_lane(&original, lane)
                };
                set_u32_lane(destination, lane, value);
            }
        }
    }

    let written_bytes = if case.kind == Bf16Kind::ConvertOne {
        case.width.bytes() / 2
    } else {
        case.width.bytes()
    };
    for word in usize::try_from(written_bytes / 8).unwrap()..destination.len() {
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
    case: Bf16MemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        MemWidth::B4.bytes()
    } else {
        case.width.bytes()
    } as usize;
    memory.load(0x2000, &bytes[..size]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_162_bf16_cells_match_manual_finite_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 162);
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
    assert_eq!(comparisons, 162 * LEVELS.len());
}

#[test]
fn e4_empty_masks_suppress_single_convert_and_dot_but_e4nf_pair_convert_faults() {
    for case in [
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertOne,
            width: VecWidth::V512,
            destination: 17,
            source1: 0,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertOne,
            width: VecWidth::V512,
            destination: 17,
            source1: 0,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::ConvertTwo,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        Bf16MemoryCase {
            kind: Bf16Kind::DotProduct,
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
            let mut empty_context = context(&empty);
            let mut unmapped = FlatMemory::new(0x1000);
            let result = SmirInterpreter::new().execute_block(
                &mut empty_context,
                &mut unmapped,
                &function.blocks[0],
            );
            if case.kind == Bf16Kind::ConvertTwo {
                assert!(
                    matches!(
                        result,
                        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                    ),
                    "{level:?} {case:?}: {result:?}"
                );
                assert_eq!(state(&empty_context), empty, "{level:?} {case:?}");
            } else {
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                assert_eq!(
                    state(&empty_context),
                    manual(case, &empty, &bytes),
                    "{level:?} {case:?}"
                );

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
                assert_eq!(state(&fault_context), active, "{level:?} {case:?}");
            }
        }
    }
}
