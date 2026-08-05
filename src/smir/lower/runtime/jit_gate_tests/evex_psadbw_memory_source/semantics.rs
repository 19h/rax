//! Independent interpreter, optimizer, aliasing, and result coverage.

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

pub(super) fn initial_state(case: PsadbwMemoryCase, ordinal: usize) -> SemanticState {
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
        let shift = (index % 8) * 8;
        let value = match index % 11 {
            0 => 0,
            1 => u8::MAX,
            _ => (index as u8)
                .wrapping_mul(37)
                .wrapping_add(ordinal as u8)
                .rotate_left((index % 7) as u32),
        };
        let word = &mut state.vectors[usize::from(case.source1)][index / 8];
        *word = (*word & !(0xFFu64 << shift)) | (u64::from(value) << shift);
    }
    state
}

pub(super) fn memory_bytes(case: PsadbwMemoryCase, ordinal: usize) -> [u8; 64] {
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
    case: PsadbwMemoryCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let source1 = initial.vectors[usize::from(case.source1)];
    let blocks = case.width.bytes() as usize / 8;
    let destination = &mut expected.vectors[usize::from(case.destination)];
    destination.fill(0);
    for block in 0..blocks {
        let mut sum = 0u16;
        for byte in 0..8 {
            let index = block * 8 + byte;
            sum += u16::from(get_byte(&source1, index).abs_diff(memory[index]));
        }
        destination[block] = u64::from(sum);
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
    case: PsadbwMemoryCase,
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
fn all_18_scanner_cells_match_manual_exact_results_at_o0_o1_o2() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 18);
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
    assert_eq!(comparisons, 18 * LEVELS.len());
}

#[test]
fn aliasing_high_registers_wig_and_extreme_differences_match_manual() {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for (destination, source1) in [(0, 0), (9, 10), (17, 17), (25, 26)] {
            for w in [false, true] {
                cases.push(PsadbwMemoryCase {
                    width,
                    destination,
                    source1,
                    w,
                });
            }
        }
    }
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let mut initial = initial_state(case, ordinal + 97);
        let mut bytes = [0u8; 64];
        for index in 0..case.width.bytes() as usize {
            let value = if (index / 8) & 1 == 0 { 0 } else { u8::MAX };
            let shift = (index % 8) * 8;
            let word = &mut initial.vectors[usize::from(case.source1)][index / 8];
            *word = (*word & !(0xFFu64 << shift)) | (u64::from(value) << shift);
            bytes[index] = u8::MAX - value;
        }
        let expected = manual(case, &initial, &bytes);
        for block in 0..case.width.bytes() as usize / 8 {
            assert_eq!(
                expected.vectors[usize::from(case.destination)][block],
                8 * u64::from(u8::MAX),
                "{case:?} block {block}"
            );
        }
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            assert!(sequence(&function, true).is_some(), "{level:?} {case:?}");
            let actual = interpret(&function, &initial, &bytes, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 3 * 4 * 2 * LEVELS.len());
}

#[test]
fn wig_images_are_semantically_identical_and_zero_upper_destination_state() {
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        let w0 = PsadbwMemoryCase {
            width,
            destination: 25,
            source1: 25,
            w: false,
        };
        let w1 = PsadbwMemoryCase { w: true, ..w0 };
        let initial = initial_state(w0, width.bytes() as usize);
        let bytes = memory_bytes(w0, 211);
        let expected = manual(w0, &initial, &bytes);
        for level in LEVELS {
            let actual_w0 = interpret(&optimize(lift_case(w0), level), &initial, &bytes, w0);
            let actual_w1 = interpret(&optimize(lift_case(w1), level), &initial, &bytes, w1);
            assert_eq!(actual_w0, expected, "{level:?} {width:?} W0");
            assert_eq!(actual_w1, expected, "{level:?} {width:?} W1");
            assert!(
                actual_w0.vectors[25][width.bytes() as usize / 8..]
                    .iter()
                    .all(|word| *word == 0),
                "{level:?} {width:?}"
            );
        }
    }
}
