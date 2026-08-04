//! Interpreter, optimizer, exact-result, imm8, and E4NF.nb fault coverage.

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

fn get_byte(vector: &[u64; 16], index: usize) -> u8 {
    (vector[index / 8] >> ((index % 8) * 8)) as u8
}

fn set_byte(vector: &mut [u64; 16], index: usize, value: u8) {
    let shift = (index % 8) * 8;
    let mask = 0xFFu64 << shift;
    vector[index / 8] = (vector[index / 8] & !mask) | (u64::from(value) << shift);
}

fn get_word(vector: &[u64; 16], index: usize) -> u16 {
    u16::from_le_bytes([get_byte(vector, index * 2), get_byte(vector, index * 2 + 1)])
}

fn set_word(vector: &mut [u64; 16], index: usize, value: u16) {
    for (byte, value) in value.to_le_bytes().into_iter().enumerate() {
        set_byte(vector, index * 2 + byte, value);
    }
}

fn sad4(left: &[u8], right: &[u8]) -> u16 {
    left.iter()
        .zip(right)
        .map(|(left, right)| u16::from(left.abs_diff(*right)))
        .sum()
}

pub(super) fn initial_state(case: DbpsadbwMemoryCase, ordinal: usize) -> SemanticState {
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
    let bytes = case.width.bytes() as usize;
    for index in 0..bytes {
        let value = match index % 11 {
            0 => 0,
            1 => u8::MAX,
            _ => (index as u8)
                .wrapping_mul(37)
                .wrapping_add(ordinal as u8)
                .rotate_left((index % 7) as u32),
        };
        set_byte(&mut state.vectors[usize::from(case.source1)], index, value);
    }
    state
}

pub(super) fn memory_bytes(case: DbpsadbwMemoryCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (index, value) in bytes[..case.width.bytes() as usize].iter_mut().enumerate() {
        *value = match index % 13 {
            0 => u8::MAX,
            1 => 0,
            _ => (index as u8)
                .wrapping_mul(53)
                .wrapping_add((ordinal as u8).rotate_left((index % 5) as u32)),
        };
    }
    bytes
}

pub(super) fn manual(
    case: DbpsadbwMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let bytes = case.width.bytes() as usize;
    let words = bytes / 2;
    let source1 = initial.vectors[usize::from(case.source1)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let mut shuffled = [0u8; 64];
    for block in (0..bytes).step_by(16) {
        for dword in 0..4 {
            let selector = usize::from((case.immediate >> (dword * 2)) & 3);
            let source = block + selector * 4;
            let destination = block + dword * 4;
            shuffled[destination..destination + 4].copy_from_slice(&memory[source..source + 4]);
        }
    }

    let mut raw = [0u16; 32];
    for block in (0..bytes).step_by(8) {
        let first = [
            get_byte(&source1, block),
            get_byte(&source1, block + 1),
            get_byte(&source1, block + 2),
            get_byte(&source1, block + 3),
        ];
        let second = [
            get_byte(&source1, block + 4),
            get_byte(&source1, block + 5),
            get_byte(&source1, block + 6),
            get_byte(&source1, block + 7),
        ];
        let word = block / 2;
        raw[word] = sad4(&first, &shuffled[block..block + 4]);
        raw[word + 1] = sad4(&first, &shuffled[block + 1..block + 5]);
        raw[word + 2] = sad4(&second, &shuffled[block + 2..block + 6]);
        raw[word + 3] = sad4(&second, &shuffled[block + 3..block + 7]);
    }

    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for (lane, raw) in raw[..words].iter().copied().enumerate() {
        let value = if mask & (1u64 << lane) != 0 {
            raw
        } else if case.zeroing() {
            0
        } else {
            get_word(&old_destination, lane)
        };
        set_word(destination, lane, value);
    }
    destination[bytes / 8..].fill(0);
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
    case: DbpsadbwMemoryCase,
) -> SemanticState {
    let mut context = context(initial);
    let mut memory = FlatMemory::new(0x4000);
    memory.load(0x2000, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    state(&context)
}

#[test]
fn all_27_scanner_cells_match_manual_exact_results_at_o0_o1_o2() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 27);
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
    assert_eq!(comparisons, 27 * LEVELS.len());
}

#[test]
fn every_imm8_matches_manual_at_every_width_and_opt_level() {
    let mut comparisons = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for immediate in u8::MIN..=u8::MAX {
            let case = DbpsadbwMemoryCase {
                width,
                destination: if immediate & 1 == 0 { 17 } else { 25 },
                source1: if immediate & 2 == 0 { 17 } else { 26 },
                control: MaskControl::ALL[usize::from(immediate % 3)],
                immediate,
            };
            let ordinal = usize::from(immediate) + width.bytes() as usize;
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
    assert_eq!(comparisons, 3 * 256 * LEVELS.len());
}

#[test]
fn maximum_sad_is_1020_for_every_width() {
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        let case = DbpsadbwMemoryCase {
            width,
            destination: 1,
            source1: 2,
            control: MaskControl::None,
            immediate: 0xE4,
        };
        let mut initial = initial_state(case, 0);
        initial.vectors[2] = [0; 16];
        let bytes = [u8::MAX; 64];
        let expected = manual(case, &initial, &bytes);
        for lane in 0..width.lanes(VecElementType::I16) as usize {
            assert_eq!(get_word(&expected.vectors[1], lane), 1020);
        }
        for level in LEVELS {
            assert_eq!(
                interpret(&optimize(lift_case(case), level), &initial, &bytes, case),
                expected,
                "{level:?} {case:?}"
            );
        }
    }
}

#[test]
fn empty_masks_do_not_suppress_e4nf_faults_or_commit_state() {
    let mut faults = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = DbpsadbwMemoryCase {
                width,
                destination: 17,
                source1: 17,
                control,
                immediate: 0xA5,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let mut initial = initial_state(case, width.bytes() as usize);
                initial.masks[usize::from(case.mask())] = 0;
                let mut fault_context = context(&initial);
                let size = case.width.bytes() as usize;
                let mut partial = FlatMemory::new(0x2000 + size - 1);
                let bytes = memory_bytes(case, size);
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
    assert_eq!(faults, 3 * 2 * LEVELS.len());
}

#[test]
fn empty_masks_still_perform_the_full_successful_access_before_merge_or_zero() {
    let mut comparisons = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = DbpsadbwMemoryCase {
                width,
                destination: 25,
                source1: 26,
                control,
                immediate: 0x1B,
            };
            let mut initial = initial_state(case, width.bytes() as usize);
            initial.masks[usize::from(case.mask())] = 0;
            let bytes = memory_bytes(case, width.bytes() as usize);
            let expected = manual(case, &initial, &bytes);
            for level in LEVELS {
                let actual = interpret(&optimize(lift_case(case), level), &initial, &bytes, case);
                assert_eq!(actual, expected, "{level:?} {case:?}");
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 3 * 2 * LEVELS.len());
}
