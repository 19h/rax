//! Interpreter, optimizer, exact-bit, imm8, WIG, and E4NF fault coverage.

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
        VecElementType::I16 => 0xFFFF,
        VecElementType::I32 => 0xFFFF_FFFF,
        _ => unreachable!("EVEX packed lane-shuffle element"),
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

fn lane_bits(elem: VecElementType, index: usize) -> u64 {
    const I16: [u16; 12] = [
        0x0000, 0xFFFF, 0x8000, 0x7FFF, 0x0001, 0xFFFE, 0x5555, 0xAAAA, 0x00FF, 0xFF00, 0x1234,
        0xFEDC,
    ];
    const I32: [u32; 12] = [
        0x0000_0000,
        0xFFFF_FFFF,
        0x8000_0000,
        0x7FFF_FFFF,
        0x0000_0001,
        0xFFFF_FFFE,
        0x5555_5555,
        0xAAAA_AAAA,
        0x0000_FFFF,
        0xFFFF_0000,
        0x1234_5678,
        0xFEDC_BA98,
    ];
    match elem {
        VecElementType::I16 => u64::from(I16[index % I16.len()]),
        VecElementType::I32 => u64::from(I32[index % I32.len()]),
        _ => unreachable!("EVEX packed lane-shuffle element"),
    }
}

pub(super) fn initial_state(case: LaneShuffleMemoryCase, ordinal: usize) -> SemanticState {
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
    let elem = case.kind.elem();
    let lanes = case.width.lanes(elem) as usize;
    let destination = &mut state.vectors[usize::from(case.destination)];
    for lane in 0..lanes {
        set_lane(destination, lane, elem, lane_bits(elem, lane + ordinal + 7));
    }
    state
}

pub(super) fn memory_bytes(case: LaneShuffleMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let elem = case.kind.elem();
    let lanes = case.width.lanes(elem) as usize;
    let lane_bytes = elem.bytes() as usize;
    for lane in 0..lanes {
        let value = lane_bits(elem, lane + ordinal);
        let offset = lane * lane_bytes;
        bytes[offset..offset + lane_bytes].copy_from_slice(&value.to_le_bytes()[..lane_bytes]);
    }
    bytes
}

fn memory_lane(case: LaneShuffleMemoryCase, bytes: &[u8; 64], lane: usize) -> u64 {
    let lane = if case.tuple.is_broadcast() { 0 } else { lane };
    let lane_bytes = case.kind.elem().bytes() as usize;
    let offset = lane * lane_bytes;
    let mut value = [0u8; 8];
    value[..lane_bytes].copy_from_slice(&bytes[offset..offset + lane_bytes]);
    u64::from_le_bytes(value)
}

pub(super) fn manual(
    case: LaneShuffleMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let elem = case.kind.elem();
    let lanes = case.width.lanes(elem) as usize;
    let block_lanes = if elem == VecElementType::I32 { 4 } else { 8 };
    let old_destination = initial.vectors[usize::from(case.destination)];
    let mut raw = [0u64; 16];

    for lane in 0..lanes {
        let within = lane % block_lanes;
        let block = lane - within;
        let shuffled = match case.kind {
            ShuffleKind::Dword => true,
            ShuffleKind::HighWord => within >= 4,
            ShuffleKind::LowWord => within < 4,
        };
        let selected_lane = if shuffled {
            let output = within % 4;
            block
                + if case.kind == ShuffleKind::HighWord {
                    4
                } else {
                    0
                }
                + usize::from((case.immediate >> (output * 2)) & 3)
        } else {
            lane
        };
        set_lane(
            &mut raw,
            lane,
            elem,
            memory_lane(case, memory, selected_lane),
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
    case: LaneShuffleMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = case.memory_size() as usize;
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
fn all_54_lane_shuffle_cells_match_manual_exact_bits_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
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
    assert_eq!(comparisons, 54 * LEVELS.len());
}

#[test]
fn every_imm8_value_matches_manual_selection_for_all_18_forms_and_opt_levels() {
    let mut comparisons = 0usize;
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (w, tuple) in valid_forms(kind) {
                for immediate in u8::MIN..=u8::MAX {
                    let control = MaskControl::ALL[usize::from(immediate % 3)];
                    let case = LaneShuffleMemoryCase {
                        kind,
                        width,
                        w,
                        destination: if immediate & 1 == 0 { 17 } else { 25 },
                        control,
                        tuple,
                        immediate,
                    };
                    let ordinal = usize::from(immediate)
                        + usize::from(kind.pp()) * 257
                        + usize::from(w) * 769;
                    let initial = initial_state(case, ordinal);
                    let memory = memory_bytes(case, ordinal);
                    let expected = manual(case, &initial, &memory);
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {case:?}"));
                        let actual = interpret(&function, &initial, &memory, case);
                        assert_eq!(actual, expected, "{level:?} {case:?}");
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 3 * 2 * 256 * LEVELS.len());
}

#[test]
fn wig_word_encodings_are_semantically_identical_and_byte_provenance_remains_distinct() {
    let mut comparisons = 0usize;
    for kind in [ShuffleKind::HighWord, ShuffleKind::LowWord] {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for control in MaskControl::ALL {
                for immediate in [0x00, 0x1B, 0x5A, 0x93, 0xA5, 0xFF] {
                    let w0 = LaneShuffleMemoryCase {
                        kind,
                        width,
                        w: false,
                        destination: 17,
                        control,
                        tuple: TupleKind::Full,
                        immediate,
                    };
                    let w1 = LaneShuffleMemoryCase { w: true, ..w0 };
                    let initial = initial_state(w0, usize::from(immediate));
                    let memory = memory_bytes(w0, usize::from(immediate));
                    let expected = manual(w0, &initial, &memory);
                    for level in LEVELS {
                        let f0 = optimize(lift_case(w0), level);
                        let f1 = optimize(lift_case(w1), level);
                        assert_ne!(w0.expected_replay(), w1.expected_replay());
                        assert_eq!(interpret(&f0, &initial, &memory, w0), expected);
                        assert_eq!(interpret(&f1, &initial, &memory, w1), expected);
                        comparisons += 2;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 2 * 3 * 3 * 6 * LEVELS.len() * 2);
}

#[test]
fn integer_boundary_patterns_rearrange_bitwise_without_flags_or_mxcsr_changes() {
    let mut comparisons = 0usize;
    for (ordinal, kind) in ShuffleKind::ALL.into_iter().enumerate() {
        for (w, tuple) in valid_forms(kind) {
            for immediate in [0x00, 0x1B, 0x5A, 0xA5, 0xE4, 0xFF] {
                let case = LaneShuffleMemoryCase {
                    kind,
                    width: VecWidth::V512,
                    w,
                    destination: 17,
                    control: if ordinal & 1 == 0 {
                        MaskControl::Merge
                    } else {
                        MaskControl::Zero
                    },
                    tuple,
                    immediate,
                };
                let initial = initial_state(case, ordinal);
                let memory = memory_bytes(case, ordinal);
                let expected = manual(case, &initial, &memory);
                for level in LEVELS {
                    let actual =
                        interpret(&optimize(lift_case(case), level), &initial, &memory, case);
                    assert_eq!(actual, expected, "{level:?} {case:?}");
                    assert_eq!(actual.rflags, initial.rflags, "{level:?} {case:?}");
                    assert_eq!(actual.mxcsr, initial.mxcsr, "{level:?} {case:?}");
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 2 * 6 * LEVELS.len());
}

#[test]
fn empty_masks_do_not_suppress_full_or_broadcast_e4nf_faults_or_commit_state() {
    let mut faults = 0usize;
    let mut ordinal = 0usize;
    for kind in ShuffleKind::ALL {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for (w, tuple) in valid_forms(kind) {
                let case = LaneShuffleMemoryCase {
                    kind,
                    width,
                    w,
                    destination: [1, 9, 17][ordinal % 3],
                    control: if ordinal & 1 == 0 {
                        MaskControl::Merge
                    } else {
                        MaskControl::Zero
                    },
                    tuple,
                    immediate: [0x00, 0x5A, 0xFF][ordinal % 3],
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
                    assert_eq!(
                        state(&fault_context),
                        initial,
                        "{level:?} {case:?}: partial E4NF fault committed state"
                    );
                    faults += 1;
                }
                ordinal += 1;
            }
        }
    }
    assert_eq!(faults, 3 * 3 * 2 * LEVELS.len());
}
