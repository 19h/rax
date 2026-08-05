//! Interpreter, optimizer, exact-bit, imm8, and Type E6NF fault coverage.

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
        VecElementType::F32 | VecElementType::I32 => 0xFFFF_FFFF,
        VecElementType::F64 | VecElementType::I64 => u64::MAX,
        _ => unreachable!("chunk-insert element width"),
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

fn special_bits(elem: VecElementType, index: usize) -> u64 {
    const BITS32: [u32; 12] = [
        0x0000_0000,
        0x8000_0000,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC1_2345,
        0x7F81_2345,
        0x0000_0001,
        0x8000_0001,
        0x0080_0000,
        0x7F7F_FFFF,
        0x1357_9BDF,
        0xECA8_6420,
    ];
    const BITS64: [u64; 12] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_1234_5678_9ABC,
        0x7FF0_1234_5678_9ABC,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x0010_0000_0000_0000,
        0x7FEF_FFFF_FFFF_FFFF,
        0x1357_9BDF_2468_ACE0,
        0xECA8_6420_DB97_531F,
    ];
    match elem {
        VecElementType::F32 | VecElementType::I32 => u64::from(BITS32[index % BITS32.len()]),
        VecElementType::F64 | VecElementType::I64 => BITS64[index % BITS64.len()],
        _ => unreachable!("chunk-insert element width"),
    }
}

pub(super) fn initial_state(case: ChunkInsertMemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA55A_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 5) as u32)
                ^ (register as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 7 + word * 13 + ordinal) as u32)
                    ^ ((register as u64) << 57)
                    ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
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
        mxcsr: 0xDFC5,
    };
    state.gpr[2] = 0x2000;
    let lanes = case.width.lanes(case.kind.elem) as usize;
    for lane in 0..lanes {
        set_lane(
            &mut state.vectors[usize::from(case.source1)],
            lane,
            case.kind.elem,
            special_bits(case.kind.elem, lane + ordinal),
        );
    }
    state
}

pub(super) fn memory_bytes(case: ChunkInsertMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let lanes = case.kind.chunk_width.lanes(case.kind.elem) as usize;
    let lane_bytes = case.kind.elem.bytes() as usize;
    for lane in 0..lanes {
        let value = special_bits(case.kind.elem, lane + ordinal + 5);
        let offset = lane * lane_bytes;
        bytes[offset..offset + lane_bytes].copy_from_slice(&value.to_le_bytes()[..lane_bytes]);
    }
    bytes
}

fn memory_lane(case: ChunkInsertMemoryCase, bytes: &[u8; 64], lane: usize) -> u64 {
    let lane_bytes = case.kind.elem.bytes() as usize;
    let offset = lane * lane_bytes;
    let mut value = [0u8; 8];
    value[..lane_bytes].copy_from_slice(&bytes[offset..offset + lane_bytes]);
    u64::from_le_bytes(value)
}

pub(super) fn manual(
    case: ChunkInsertMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let elem = case.kind.elem;
    let lanes = case.width.lanes(elem) as usize;
    let chunk_lanes = case.kind.chunk_width.lanes(elem) as usize;
    let chunks = case.width.bytes() / case.kind.chunk_width.bytes();
    let first_lane = usize::from(case.immediate & (chunks as u8 - 1)) * chunk_lanes;
    let source1 = initial.vectors[usize::from(case.source1)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let mut raw = source1;
    for lane in 0..chunk_lanes {
        set_lane(
            &mut raw,
            first_lane + lane,
            elem,
            memory_lane(case, memory, lane),
        );
    }

    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        let value = if mask & (1u64 << lane) != 0 {
            get_lane(&raw, lane, elem)
        } else if case.zeroing() {
            0
        } else {
            get_lane(&old_destination, lane, elem)
        };
        set_lane(destination, lane, elem, value);
    }
    destination[usize::try_from(case.width.bytes() / 8).unwrap()..].fill(0);
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
    case: ChunkInsertMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    memory.load(0x2000, &bytes[..case.memory_size() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_108_cells_match_manual_exact_bits_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 108);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let bytes = memory_bytes(case, ordinal);
        let expected = manual(case, &initial, &bytes);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret(&function, &initial, &bytes, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 108 * LEVELS.len());
}

#[test]
fn every_imm8_matches_manual_for_all_12_shapes_controls_and_opt_levels() {
    let mut comparisons = 0usize;
    for (shape_index, (kind, width)) in shape_cases().into_iter().enumerate() {
        for control in MaskControl::ALL {
            for immediate in u8::MIN..=u8::MAX {
                let case = ChunkInsertMemoryCase {
                    kind,
                    width,
                    destination: if immediate & 1 == 0 { 17 } else { 25 },
                    source1: if immediate & 2 == 0 { 17 } else { 26 },
                    control,
                    immediate,
                };
                let ordinal = usize::from(immediate) + shape_index * 257;
                let initial = initial_state(case, ordinal);
                let bytes = memory_bytes(case, ordinal);
                let expected = manual(case, &initial, &bytes);
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {case:?}"));
                    let actual = interpret(&function, &initial, &bytes, case);
                    assert_eq!(actual, expected, "{level:?} {case:?}");
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 12 * 3 * 256 * LEVELS.len());
}

#[test]
fn imm8_high_bits_are_ignored_after_exact_byte_provenance_is_preserved() {
    let mut comparisons = 0usize;
    for (kind, width) in shape_cases() {
        let chunks = width.bytes() / kind.chunk_width.bytes();
        for selector in 0u8..chunks as u8 {
            let low = selector;
            let high = selector | 0xFC;
            let low_case = ChunkInsertMemoryCase {
                kind,
                width,
                destination: 17,
                source1: 26,
                control: MaskControl::Merge,
                immediate: low,
            };
            let high_case = ChunkInsertMemoryCase {
                immediate: high,
                ..low_case
            };
            let initial = initial_state(low_case, usize::from(selector));
            let bytes = memory_bytes(low_case, usize::from(selector));
            let low_expected = manual(low_case, &initial, &bytes);
            let high_expected = manual(high_case, &initial, &bytes);
            assert_eq!(low_expected, high_expected, "{kind:?} {width:?}");
            for level in LEVELS {
                let low_actual = interpret(
                    &optimize(lift_case(low_case), level),
                    &initial,
                    &bytes,
                    low_case,
                );
                let high_actual = interpret(
                    &optimize(lift_case(high_case), level),
                    &initial,
                    &bytes,
                    high_case,
                );
                assert_eq!(low_actual, high_actual, "{level:?} {kind:?} {width:?}");
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 32 * LEVELS.len());
}

#[test]
fn empty_masks_do_not_suppress_e6nf_faults_or_commit_state() {
    let mut faults = 0usize;
    for (ordinal, (kind, width)) in shape_cases().into_iter().enumerate() {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ChunkInsertMemoryCase {
                kind,
                width,
                destination: [1, 9, 17, 25][ordinal % 4],
                source1: [2, 10, 17, 26][ordinal % 4],
                control,
                immediate: [0x00, 0x4E, 0xA5, 0xFF][ordinal % 4],
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let mut initial = initial_state(case, ordinal);
                initial.masks[usize::from(case.mask())] = 0;
                let mut fault_context = context(&initial);
                let size = case.memory_size() as usize;
                let mut partial = FlatMemory::new(0x2000 + size - 1);
                let bytes = memory_bytes(case, ordinal);
                partial.load(0x2000, &bytes[..size - 1]);
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
    assert_eq!(faults, 12 * 2 * LEVELS.len());
}

#[test]
fn empty_masks_still_perform_one_successful_access_before_merge_or_zero() {
    let mut comparisons = 0usize;
    for (ordinal, (kind, width)) in shape_cases().into_iter().enumerate() {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ChunkInsertMemoryCase {
                kind,
                width,
                destination: 17,
                source1: if ordinal & 1 == 0 { 17 } else { 18 },
                control,
                immediate: [0x00, 0x4E, 0xA5, 0xFF][ordinal % 4],
            };
            let mut initial = initial_state(case, ordinal);
            initial.masks[usize::from(case.mask())] = 0;
            let bytes = memory_bytes(case, ordinal);
            let expected = manual(case, &initial, &bytes);
            for level in LEVELS {
                let actual = interpret(&optimize(lift_case(case), level), &initial, &bytes, case);
                assert_eq!(actual, expected, "{level:?} {case:?}");
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 12 * 2 * LEVELS.len());
}
