//! Independent interpreter, optimizer, pair-mask, and aliasing coverage.

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

fn get_lane(vector: &[u64; 16], elem: VecElementType, lane: usize) -> u64 {
    match elem {
        VecElementType::I32 => (vector[lane / 2] >> ((lane & 1) * 32)) & 0xFFFF_FFFF,
        VecElementType::I64 => vector[lane],
        _ => unreachable!("VP2INTERSECT D/Q element"),
    }
}

fn set_lane(vector: &mut [u64; 16], elem: VecElementType, lane: usize, value: u64) {
    match elem {
        VecElementType::I32 => {
            let shift = (lane & 1) * 32;
            vector[lane / 2] =
                (vector[lane / 2] & !(0xFFFF_FFFFu64 << shift)) | ((value & 0xFFFF_FFFF) << shift);
        }
        VecElementType::I64 => vector[lane] = value,
        _ => unreachable!("VP2INTERSECT D/Q element"),
    }
}

fn memory_lane(bytes: &[u8; 64], elem: VecElementType, lane: usize) -> u64 {
    match elem {
        VecElementType::I32 => {
            let offset = lane * 4;
            u64::from(u32::from_le_bytes(
                bytes[offset..offset + 4].try_into().unwrap(),
            ))
        }
        VecElementType::I64 => {
            let offset = lane * 8;
            u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
        }
        _ => unreachable!("VP2INTERSECT D/Q element"),
    }
}

fn set_memory_lane(bytes: &mut [u8; 64], elem: VecElementType, lane: usize, value: u64) {
    match elem {
        VecElementType::I32 => {
            let offset = lane * 4;
            bytes[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes());
        }
        VecElementType::I64 => {
            let offset = lane * 8;
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        _ => unreachable!("VP2INTERSECT D/Q element"),
    }
}

pub(super) fn initial_state(case: Vp2IntersectMemoryCase, ordinal: usize) -> SemanticState {
    let mut state = SemanticState {
        gpr: std::array::from_fn(|register| {
            0xA55A_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register * 5 + word * 11 + ordinal) as u32)
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
    let lanes = case.width.lanes(case.elem) as usize;
    for lane in 0..lanes {
        let value = match lane % 6 {
            0 => 0,
            1 | 4 => 0xFFFF_FFFF_FFFF_FFFF,
            2 => 0x8000_0000_8000_0000,
            _ => (ordinal as u64)
                .wrapping_mul(0x1_0000_0001)
                .wrapping_add((lane as u64).wrapping_mul(0x0101_0101_0101_0101)),
        };
        set_lane(
            &mut state.vectors[usize::from(case.source1)],
            case.elem,
            lane,
            value,
        );
    }
    state
}

pub(super) fn memory_bytes(
    case: Vp2IntersectMemoryCase,
    initial: &SemanticState,
    ordinal: usize,
) -> [u8; 64] {
    let mut bytes = [0xA5u8; 64];
    let lanes = case.width.lanes(case.elem) as usize;
    for lane in 0..lanes {
        let value = if lane & 1 == 0 || case.broadcast {
            let source_lane = (lane * 5 + ordinal + 1) % lanes;
            get_lane(
                &initial.vectors[usize::from(case.source1)],
                case.elem,
                source_lane,
            )
        } else {
            0xDEAD_0000_0000_0000u64 ^ (ordinal as u64).rotate_left(lane as u32) ^ lane as u64
        };
        set_memory_lane(&mut bytes, case.elem, lane, value);
    }
    bytes
}

pub(super) fn manual(
    case: Vp2IntersectMemoryCase,
    initial: &SemanticState,
    bytes: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let lanes = case.width.lanes(case.elem) as usize;
    let source1 = &initial.vectors[usize::from(case.source1)];
    let mut first = 0u64;
    let mut second = 0u64;
    for lane1 in 0..lanes {
        let value1 = get_lane(source1, case.elem, lane1);
        for lane2 in 0..lanes {
            let source2_lane = if case.broadcast { 0 } else { lane2 };
            if value1 == memory_lane(bytes, case.elem, source2_lane) {
                first |= 1 << lane1;
                second |= 1 << lane2;
            }
        }
    }
    expected.masks[usize::from(case.destination_base())] = first;
    expected.masks[usize::from(case.destination_base() + 1)] = second;
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
    case: Vp2IntersectMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    let size = if case.broadcast {
        case.elem.bytes() as usize
    } else {
        case.width.bytes() as usize
    };
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
fn all_36_scanner_cells_match_manual_pair_masks_at_o0_o1_o2() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 36);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal);
        let bytes = memory_bytes(case, &initial, ordinal);
        let expected = manual(case, &initial, &bytes);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpret(&function, &initial, &bytes, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 36 * LEVELS.len());
}

#[test]
fn odd_pair_selectors_high_sources_and_all_widths_match_manual() {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for broadcast in [false, true] {
                let ordinal = cases.len();
                cases.push(Vp2IntersectMemoryCase {
                    width,
                    elem,
                    destination: if ordinal & 1 == 0 { 1 } else { 7 },
                    source1: if ordinal & 1 == 0 { 17 } else { 31 },
                    broadcast,
                });
            }
        }
    }
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal + 101);
        let bytes = memory_bytes(case, &initial, ordinal + 101);
        let expected = manual(case, &initial, &bytes);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let exact = sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {case:?}"));
            assert_eq!(exact.encoding.destination_base, case.destination_base());
            let actual = interpret(&function, &initial, &bytes, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 12 * LEVELS.len());
}

#[test]
fn duplicate_matches_set_both_masks_with_exact_lane_polarity() {
    for broadcast in [false, true] {
        let case = Vp2IntersectMemoryCase {
            width: VecWidth::V128,
            elem: VecElementType::I32,
            destination: 7,
            source1: 31,
            broadcast,
        };
        let mut initial = initial_state(case, 211);
        for (lane, value) in [5, 7, 5, 9].into_iter().enumerate() {
            set_lane(&mut initial.vectors[31], case.elem, lane, value);
        }
        let mut bytes = [0u8; 64];
        let memory = if broadcast {
            [5, 5, 5, 5]
        } else {
            [9, 5, 5, 12]
        };
        for (lane, value) in memory.into_iter().enumerate() {
            set_memory_lane(&mut bytes, case.elem, lane, value);
        }
        let expected = manual(case, &initial, &bytes);
        assert_eq!(
            expected.masks[6],
            0b0101 | if broadcast { 0 } else { 0b1000 }
        );
        assert_eq!(expected.masks[7], if broadcast { 0b1111 } else { 0b0111 });
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            assert_eq!(interpret(&function, &initial, &bytes, case), expected);
        }
    }
}

#[test]
fn full_and_broadcast_memory_faults_precede_both_pair_commits_at_all_levels() {
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for elem in [VecElementType::I32, VecElementType::I64] {
            for broadcast in [false, true] {
                let case = Vp2IntersectMemoryCase {
                    width,
                    elem,
                    destination: 7,
                    source1: 31,
                    broadcast,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_state(case, 307);
                    let access_size = if broadcast {
                        elem.bytes() as u64
                    } else {
                        u64::from(width.bytes())
                    };
                    initial.gpr[2] = 0x40 - access_size + 1;
                    let mut context = context(&initial);
                    let mut memory = FlatMemory::new(0x40);
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
                    assert_eq!(state(&context), initial, "{level:?} {case:?}");
                }
            }
        }
    }
}
