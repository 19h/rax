//! Interpreter, optimizer, exact-bit, and Type E4 fault coverage.

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

fn element_bits(elem: VecElementType) -> u32 {
    elem.bytes() * 8
}

fn bit_mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn vector_lane(vector: &[u64; 16], elem: VecElementType, lane: usize) -> u64 {
    let bits = element_bits(elem);
    let bit = lane * bits as usize;
    (vector[bit / 64] >> (bit % 64)) & bit_mask(bits)
}

fn set_vector_lane(vector: &mut [u64; 16], elem: VecElementType, lane: usize, value: u64) {
    let bits = element_bits(elem);
    let bit = lane * bits as usize;
    let shift = bit % 64;
    let mask = bit_mask(bits);
    vector[bit / 64] = (vector[bit / 64] & !(mask << shift)) | ((value & mask) << shift);
}

fn boundary_operand(bits: u32, lane: usize, ordinal: usize, salt: u64) -> u64 {
    let mask = bit_mask(bits);
    let sign = 1u64 << (bits - 1);
    match lane % 8 {
        0 => 0,
        1 => 1,
        2 => mask,
        3 => sign,
        4 => sign - 1,
        5 => (sign + 1) & mask,
        6 => 0xAAAA_AAAA_AAAA_AAAAu64 & mask,
        _ => {
            salt.wrapping_add((ordinal as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                .rotate_left((lane * 7) as u32)
                & mask
        }
    }
}

pub(super) fn initial_state(case: MultiplyMemoryCase, ordinal: usize) -> SemanticState {
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
    let source = &mut state.vectors[usize::from(case.source1)];
    if case.kind.is_widening() {
        for lane in 0..case.lanes() {
            set_vector_lane(
                source,
                VecElementType::I32,
                lane * 2,
                boundary_operand(32, lane, ordinal, 0x1357_9BDF),
            );
            set_vector_lane(
                source,
                VecElementType::I32,
                lane * 2 + 1,
                boundary_operand(32, lane + 3, ordinal, 0xDEAD_BEEF),
            );
        }
    } else {
        for lane in 0..case.lanes() {
            set_vector_lane(
                source,
                case.kind.elem(),
                lane,
                boundary_operand(
                    element_bits(case.kind.elem()),
                    lane,
                    ordinal,
                    0x1357_9BDF_2468_ACE0,
                ),
            );
        }
    }
    if case.source1 != case.destination {
        let destination = &mut state.vectors[usize::from(case.destination)];
        for lane in 0..case.lanes() {
            set_vector_lane(
                destination,
                case.kind.elem(),
                lane,
                boundary_operand(
                    element_bits(case.kind.elem()),
                    lane + 5,
                    ordinal,
                    0x6A09_E667_F3BC_C909,
                ),
            );
        }
    }
    state
}

pub(super) fn memory_bytes(case: MultiplyMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    if case.kind.is_widening() {
        for lane in 0..case.lanes() {
            let low = boundary_operand(32, lane + 1, ordinal, 0xFEDC_BA98) as u32;
            let high = boundary_operand(32, lane + 4, ordinal, 0xCAFE_BABE) as u32;
            let value = u64::from(low) | (u64::from(high) << 32);
            bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
    } else {
        let size = case.memory_width().bytes() as usize;
        for lane in 0..case.lanes() {
            let value = boundary_operand(
                element_bits(case.kind.elem()),
                lane + 1,
                ordinal,
                0xFEDC_BA98_7654_3210,
            );
            bytes[lane * size..lane * size + size].copy_from_slice(&value.to_le_bytes()[..size]);
        }
    }
    bytes
}

fn memory_lane(case: MultiplyMemoryCase, memory: &[u8; 64], lane: usize) -> u64 {
    let size = case.memory_width().bytes() as usize;
    let source_lane = if case.broadcast() { 0 } else { lane };
    let offset = source_lane * size;
    let mut raw = [0u8; 8];
    raw[..size].copy_from_slice(&memory[offset..offset + size]);
    u64::from_le_bytes(raw)
}

fn multiply_lane(kind: MultiplyKind, source1: u64, source2: u64) -> u64 {
    match kind {
        MultiplyKind::SignedDwordToQword => {
            (source1 as u32 as i32 as i64).wrapping_mul(source2 as u32 as i32 as i64) as u64
        }
        MultiplyKind::UnsignedDwordToQword => {
            u64::from(source1 as u32).wrapping_mul(u64::from(source2 as u32))
        }
        MultiplyKind::RoundedHighSignedWord => {
            let product = i64::from(source1 as u16 as i16) * i64::from(source2 as u16 as i16);
            ((product + (1 << 14)) >> 15) as u16 as u64
        }
        MultiplyKind::HighUnsignedWord => {
            (u64::from(source1 as u16) * u64::from(source2 as u16)) >> 16
        }
        MultiplyKind::HighSignedWord => {
            let product = i32::from(source1 as u16 as i16) * i32::from(source2 as u16 as i16);
            (product >> 16) as u16 as u64
        }
        MultiplyKind::LowWord => source1.wrapping_mul(source2) & 0xFFFF,
        MultiplyKind::LowDword => source1.wrapping_mul(source2) & 0xFFFF_FFFF,
        MultiplyKind::LowQword => source1.wrapping_mul(source2),
    }
}

pub(super) fn manual(
    case: MultiplyMemoryCase,
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
        let result = if mask & (1u64 << lane) != 0 {
            let lhs = if case.kind.is_widening() {
                vector_lane(&source1, VecElementType::I32, lane * 2)
            } else {
                vector_lane(&source1, case.kind.elem(), lane)
            };
            let rhs = memory_lane(case, memory, lane);
            multiply_lane(case.kind, lhs, rhs)
        } else if case.zeroing() {
            0
        } else {
            vector_lane(&old_destination, case.kind.elem(), lane)
        };
        set_vector_lane(destination, case.kind.elem(), lane, result);
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
    memory_bytes: &[u8; 64],
    case: MultiplyMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast() {
        case.memory_width().bytes() as usize
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
fn all_432_multiply_cells_match_manual_exact_bits_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 432);
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
    assert_eq!(comparisons, 432 * LEVELS.len());
}

#[test]
fn multiply_signed_unsigned_high_rounding_and_wrap_boundaries_match_exact_bits() {
    for (ordinal, kind) in MultiplyKind::ALL.into_iter().enumerate() {
        let case = MultiplyMemoryCase {
            kind,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: if kind.allows_broadcast() && ordinal & 1 != 0 {
                SourceForm::Broadcast
            } else {
                SourceForm::Vector
            },
            control: MaskControl::None,
            w: if kind.is_wig() {
                ordinal & 1 != 0
            } else {
                kind.fixed_w()
            },
        };
        let initial = initial_state(case, ordinal);
        let memory = memory_bytes(case, ordinal);
        let expected = manual(case, &initial, &memory);
        for level in LEVELS {
            assert_eq!(
                interpret(&optimize(lift_case(case), level), &initial, &memory, case),
                expected,
                "{level:?} {case:?}"
            );
        }
    }
}

#[test]
fn multiply_architectural_extrema_produce_the_specified_lane_bits() {
    let extrema = [
        (
            MultiplyKind::SignedDwordToQword,
            0x8000_0000,
            0x8000_0000,
            0x4000_0000_0000_0000,
        ),
        (
            MultiplyKind::UnsignedDwordToQword,
            0xFFFF_FFFF,
            0xFFFF_FFFF,
            0xFFFF_FFFE_0000_0001,
        ),
        (MultiplyKind::RoundedHighSignedWord, 0x8000, 0x8000, 0x8000),
        (MultiplyKind::HighUnsignedWord, 0xFFFF, 0xFFFF, 0xFFFE),
        (MultiplyKind::HighSignedWord, 0x8000, 0x7FFF, 0xC000),
        (MultiplyKind::LowWord, 0xFFFF, 0xFFFF, 1),
        (MultiplyKind::LowDword, 0xFFFF_FFFF, 0xFFFF_FFFF, 1),
        (MultiplyKind::LowQword, u64::MAX, u64::MAX, 1),
    ];
    for (kind, lhs, rhs, expected_lane) in extrema {
        let case = MultiplyMemoryCase {
            kind,
            width: VecWidth::V128,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::None,
            w: kind.fixed_w(),
        };
        let mut initial = initial_state(case, 0);
        set_vector_lane(
            &mut initial.vectors[usize::from(case.source1)],
            if kind.is_widening() {
                VecElementType::I32
            } else {
                kind.elem()
            },
            0,
            lhs,
        );
        let mut memory = [0u8; 64];
        let memory_value = if kind.is_widening() {
            (rhs & 0xFFFF_FFFF) | 0xDEAD_BEEF_0000_0000
        } else {
            rhs
        };
        let size = case.memory_width().bytes() as usize;
        memory[..size].copy_from_slice(&memory_value.to_le_bytes()[..size]);
        let expected = manual(case, &initial, &memory);
        assert_eq!(
            vector_lane(
                &expected.vectors[usize::from(case.destination)],
                kind.elem(),
                0
            ),
            expected_lane,
            "{case:?}"
        );
        for level in [OptLevel::O0, OptLevel::O2] {
            assert_eq!(
                interpret(&optimize(lift_case(case), level), &initial, &memory, case),
                expected,
                "{level:?} {case:?}"
            );
        }
    }
}

#[test]
fn empty_masks_suppress_unmapped_multiply_accesses_and_faults_do_not_commit() {
    let cases = [
        MultiplyMemoryCase {
            kind: MultiplyKind::LowWord,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            w: true,
        },
        MultiplyMemoryCase {
            kind: MultiplyKind::SignedDwordToQword,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            w: true,
        },
        MultiplyMemoryCase {
            kind: MultiplyKind::LowQword,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            w: true,
        },
    ];
    for case in cases {
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

    for (case, mask, mapped) in [
        (
            MultiplyMemoryCase {
                kind: MultiplyKind::LowWord,
                width: VecWidth::V512,
                destination: 17,
                source1: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
                w: false,
            },
            (1u64 << 0) | (1u64 << 8),
            16usize,
        ),
        (
            MultiplyMemoryCase {
                kind: MultiplyKind::UnsignedDwordToQword,
                width: VecWidth::V512,
                destination: 17,
                source1: 18,
                form: SourceForm::Vector,
                control: MaskControl::Merge,
                w: true,
            },
            0b0101,
            16,
        ),
    ] {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let mut initial = initial_state(case, 1);
            initial.masks[usize::from(case.mask())] = mask;
            let bytes = memory_bytes(case, 1);
            let mut memory = FlatMemory::new(0x2010);
            memory.load(0x2000, &bytes[..mapped]);
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
                "{level:?} {case:?}: {result:?}"
            );
            assert_eq!(
                state(&partial_context),
                initial,
                "{level:?} {case:?}: a later-lane fault committed an earlier lane"
            );
        }
    }
}
