//! Interpreter, optimizer, exact-result, alias, and Type E4 fault coverage.

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

/// Independent polynomial-basis multiplication in GF(2^8), reduced modulo
/// x^8 + x^4 + x^3 + x + 1 (0x11B). Runtime is exactly eight iterations and
/// O(1) auxiliary space.
fn gf2p8_multiply(mut left: u8, mut right: u8) -> u8 {
    let mut result = 0u8;
    for _ in 0..8 {
        if right & 1 != 0 {
            result ^= left;
        }
        let carry = left & 0x80 != 0;
        left <<= 1;
        if carry {
            left ^= 0x1B;
        }
        right >>= 1;
    }
    result
}

pub(super) fn initial_state(case: GfniMultiplyCase, ordinal: usize) -> SemanticState {
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
        let value = match index % 13 {
            0 => 0,
            1 => 1,
            2 => u8::MAX,
            3 => 0x80,
            _ => (index as u8)
                .wrapping_mul(37)
                .wrapping_add(ordinal as u8)
                .rotate_left((index % 7) as u32),
        };
        set_byte(&mut state.vectors[usize::from(case.source1)], index, value);
    }
    state
}

pub(super) fn memory_bytes(case: GfniMultiplyCase, ordinal: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (index, value) in bytes[..case.width.bytes() as usize].iter_mut().enumerate() {
        *value = match index % 11 {
            0 => u8::MAX,
            1 => 0,
            2 => 1,
            3 => 0x80,
            _ => (index as u8)
                .wrapping_mul(53)
                .wrapping_add((ordinal as u8).rotate_left((index % 5) as u32)),
        };
    }
    bytes
}

pub(super) fn manual(
    case: GfniMultiplyCase,
    initial: &SemanticState,
    memory: &[u8; 64],
) -> SemanticState {
    let mut expected = initial.clone();
    let bytes = case.width.bytes() as usize;
    let source1 = initial.vectors[usize::from(case.source1)];
    let old_destination = initial.vectors[usize::from(case.destination)];
    let mask = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(case.mask())]
    };
    let destination = &mut expected.vectors[usize::from(case.destination)];
    for lane in 0..bytes {
        let value = if mask & (1u64 << lane) != 0 {
            gf2p8_multiply(get_byte(&source1, lane), memory[lane])
        } else if case.zeroing() {
            0
        } else {
            get_byte(&old_destination, lane)
        };
        set_byte(destination, lane, value);
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
    case: GfniMultiplyCase,
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
fn independent_gf_oracle_exhausts_all_65_536_byte_pairs_and_known_products() {
    assert_eq!(gf2p8_multiply(0x57, 0x13), 0xFE);
    assert_eq!(gf2p8_multiply(0x53, 0xCA), 0x01);
    assert_eq!(gf2p8_multiply(0x80, 0x02), 0x1B);
    let mut pairs = 0usize;
    for left in u8::MIN..=u8::MAX {
        for right in u8::MIN..=u8::MAX {
            let product = gf2p8_multiply(left, right);
            assert_eq!(product, gf2p8_multiply(right, left));
            assert_eq!(gf2p8_multiply(left, 0), 0);
            assert_eq!(gf2p8_multiply(left, 1), left);
            pairs += 1;
        }
    }
    assert_eq!(pairs, 65_536);
}

#[test]
fn all_27_scanner_cells_match_manual_results_at_o0_o1_o2() {
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
fn high_registers_and_destination_source_aliases_match_manual_results() {
    let cases = [
        GfniMultiplyCase {
            width: VecWidth::V128,
            destination: 17,
            source1: 17,
            control: MaskControl::None,
        },
        GfniMultiplyCase {
            width: VecWidth::V256,
            destination: 25,
            source1: 25,
            control: MaskControl::Merge,
        },
        GfniMultiplyCase {
            width: VecWidth::V512,
            destination: 31,
            source1: 31,
            control: MaskControl::Zero,
        },
        GfniMultiplyCase {
            width: VecWidth::V512,
            destination: 17,
            source1: 30,
            control: MaskControl::Merge,
        },
    ];
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_state(case, ordinal + 0x40);
        let bytes = memory_bytes(case, ordinal + 0x40);
        let expected = manual(case, &initial, &bytes);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            sequence(&function, true).unwrap_or_else(|| panic!("{level:?} {case:?}"));
            assert_eq!(
                interpret(&function, &initial, &bytes, case),
                expected,
                "{level:?} {case:?}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, cases.len() * LEVELS.len());
}

#[test]
fn type_e4_masks_suppress_inactive_byte_faults_and_active_faults_do_not_commit() {
    let mut suppressed = 0usize;
    let mut active_faults = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = GfniMultiplyCase {
                width,
                destination: 17,
                source1: 17,
                control,
            };
            let bytes = memory_bytes(case, width.bytes() as usize);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);

                let mut empty_initial = initial_state(case, width.bytes() as usize);
                empty_initial.masks[usize::from(case.mask())] = 0;
                let expected = manual(case, &empty_initial, &bytes);
                let mut empty_context = context(&empty_initial);
                let mut inaccessible = FlatMemory::new(0x2000);
                let result = SmirInterpreter::new().execute_block(
                    &mut empty_context,
                    &mut inaccessible,
                    &function.blocks[0],
                );
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::Return { .. })
                ));
                assert_eq!(state(&empty_context), expected, "{level:?} {case:?}");
                suppressed += 1;

                let mut low_initial = initial_state(case, width.bytes() as usize + 1);
                low_initial.masks[usize::from(case.mask())] = 0b11;
                let expected = manual(case, &low_initial, &bytes);
                let mut low_context = context(&low_initial);
                let mut low_memory = FlatMemory::new(0x2002);
                low_memory.load(0x2000, &bytes[..2]);
                let result = SmirInterpreter::new().execute_block(
                    &mut low_context,
                    &mut low_memory,
                    &function.blocks[0],
                );
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::Return { .. })
                ));
                assert_eq!(state(&low_context), expected, "{level:?} {case:?}");
                suppressed += 1;

                let mut fault_initial = initial_state(case, width.bytes() as usize + 2);
                fault_initial.masks[usize::from(case.mask())] = 0b101;
                let mut fault_context = context(&fault_initial);
                let mut partial = FlatMemory::new(0x2002);
                partial.load(0x2000, &bytes[..2]);
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
                assert_eq!(state(&fault_context), fault_initial, "{level:?} {case:?}");
                active_faults += 1;
            }
        }
    }
    assert_eq!(suppressed, 3 * 2 * LEVELS.len() * 2);
    assert_eq!(active_faults, 3 * 2 * LEVELS.len());
}

#[test]
fn unmasked_full_tuple_faults_precisely_before_destination_commit() {
    let mut faults = 0usize;
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        let case = GfniMultiplyCase {
            width,
            destination: 17,
            source1: 17,
            control: MaskControl::None,
        };
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let initial = initial_state(case, width.bytes() as usize);
            let bytes = memory_bytes(case, width.bytes() as usize);
            let size = width.bytes() as usize;
            let mut fault_context = context(&initial);
            let mut partial = FlatMemory::new(0x2000 + size - 1);
            partial.load(0x2000, &bytes[..size - 1]);
            let result = SmirInterpreter::new().execute_block(
                &mut fault_context,
                &mut partial,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ));
            assert_eq!(state(&fault_context), initial, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(faults, 3 * LEVELS.len());
}
