//! Independent interpreter, optimizer, mask, bit-domain, and fault semantics.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::GuestRegs;

fn bytes_from_words(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn words_from_bytes(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

pub(super) fn initial_registers(case: VpshufbitqmbMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0xA55A_0000_0000_0000u64
                ^ (ordinal as u64).rotate_left((index * 7) as u32)
                ^ (index as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        zmm: std::array::from_fn(|register| {
            std::array::from_fn(|qword| {
                0x0123_4567_89AB_CDEFu64
                    .rotate_left(((register * 11 + qword * 17 + ordinal) & 63) as u32)
                    ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
                    ^ (qword as u64).wrapping_mul(0x1111_2222_4444_8888)
            })
        }),
        k: std::array::from_fn(|index| {
            0xF0F0_0F0F_A5A5_5A5Au64.rotate_left((index * 9 + ordinal) as u32) ^ index as u64
        }),
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80 | (((ordinal & 3) as u32) << 13),
        vector_active: 1,
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        ..GuestRegs::default()
    };
    registers.gpr[3] = 0x2000;
    // Preserve nonzero bits above every architectural result width so tests
    // prove the destination K register is zero-extended.
    registers.k[usize::from(case.destination)] |= 0xFFFF_FFFF_0000_0000;
    registers
}

pub(super) fn memory_value(ordinal: usize) -> [u64; 8] {
    const CONTROLS: [u8; 16] = [
        0x00, 0x01, 0x1F, 0x20, 0x3E, 0x3F, 0x40, 0x7F, 0x80, 0xC1, 0x5F, 0xA0, 0xFE, 0xFF, 0x15,
        0xEA,
    ];
    let mut bytes = [0u8; 64];
    for (lane, byte) in bytes.iter_mut().enumerate() {
        *byte = CONTROLS[(lane * 5 + ordinal * 3) % CONTROLS.len()];
    }
    words_from_bytes(bytes)
}

fn context_from(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

fn registers_from_context(initial: &GuestRegs, context: &SmirContext) -> GuestRegs {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut actual = *initial;
    actual.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        actual.zmm[index].copy_from_slice(&value[..8]);
    }
    actual.k = x86.k;
    actual.rflags = x86.rflags;
    actual.mxcsr = x86.mxcsr;
    actual
}

pub(super) fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    value: [u64; 8],
    case: VpshufbitqmbMemoryCase,
) -> GuestRegs {
    let mut context = context_from(initial);
    let mut memory = FlatMemory::new(0x10000);
    let bytes = bytes_from_words(value);
    memory.load(0x2000, &bytes[..case.width.bytes() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{result:?}"
    );
    registers_from_context(initial, &context)
}

pub(super) fn expected_mask(
    case: VpshufbitqmbMemoryCase,
    initial: &GuestRegs,
    memory: [u64; 8],
) -> u64 {
    let controls = bytes_from_words(memory);
    let active = if case.mask == 0 {
        u64::MAX
    } else {
        initial.k[usize::from(case.mask)]
    };
    let mut result = 0u64;
    for lane in 0..case.width.bytes() as usize {
        if active & (1u64 << lane) == 0 {
            continue;
        }
        let qword = initial.zmm[usize::from(case.source1)][lane / 8];
        let selector = controls[lane] & 0x3F;
        result |= ((qword >> selector) & 1) << lane;
    }
    result
}

fn semantic_cases() -> Vec<VpshufbitqmbMemoryCase> {
    let mut cases = Vec::new();
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for destination in [0, 1, 7] {
            for source1 in [0, 15, 16, 31] {
                for mask in [0, 1, 7] {
                    cases.push(VpshufbitqmbMemoryCase {
                        width,
                        destination,
                        source1,
                        mask,
                    });
                }
            }
        }
    }
    cases
}

#[test]
fn all_108_semantic_shapes_match_independent_oracle_at_o0_o1_o2() {
    let cases = semantic_cases();
    assert_eq!(cases.len(), 108);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_registers(case, ordinal);
        let value = memory_value(ordinal);
        let mut expected = initial;
        expected.k[usize::from(case.destination)] = expected_mask(case, &initial, value);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, value, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 108 * LEVELS.len());
}

#[test]
fn selectors_are_qword_local_and_ignore_both_high_control_bits() {
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        let case = VpshufbitqmbMemoryCase {
            width,
            destination: 5,
            source1: 31,
            mask: 0,
        };
        let mut initial = initial_registers(case, width.bytes() as usize);
        let mut controls = [0u8; 64];
        for lane in 0..width.bytes() as usize {
            let qword = lane / 8;
            let selected = (qword * 7) as u8;
            initial.zmm[31][qword] = 1u64 << selected;
            let selector = if lane & 1 == 0 {
                selected
            } else {
                selected.wrapping_add(1) & 0x3F
            };
            controls[lane] = selector | (((lane / 2) & 3) as u8 * 0x40);
        }
        let value = words_from_bytes(controls);
        let lanes = width.bytes();
        let width_mask = if lanes == 64 {
            u64::MAX
        } else {
            (1u64 << lanes) - 1
        };
        let expected = 0x5555_5555_5555_5555 & width_mask;
        assert_eq!(expected_mask(case, &initial, value), expected);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, value, case);
            assert_eq!(actual.k[5], expected, "{level:?} {case:?}");
        }
    }
}

#[test]
fn destination_writemask_alias_reads_old_k_and_zeroes_high_destination_bits() {
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        let case = VpshufbitqmbMemoryCase {
            width,
            destination: 1,
            source1: 17,
            mask: 1,
        };
        let mut initial = initial_registers(case, width.bytes() as usize + 101);
        initial.k[1] = 0xFFFF_0000_A5A5_6969;
        let value = memory_value(width.bytes() as usize + 101);
        let expected = expected_mask(case, &initial, value);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, value, case);
            assert_eq!(actual.k[1], expected, "{level:?} {case:?}");
            if width != VecWidth::V512 {
                assert_eq!(actual.k[1] >> width.bytes(), 0, "{level:?} {case:?}");
            }
        }
    }
}

#[test]
fn inactive_memory_is_suppressed_and_active_faults_are_precise_and_noncommitting() {
    for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
        for level in LEVELS {
            let masked = VpshufbitqmbMemoryCase {
                width,
                destination: 1,
                source1: 31,
                mask: 1,
            };
            let function = optimize(lift_case(masked), level);

            let mut inactive = initial_registers(masked, 211);
            inactive.gpr[3] = 0x20_000;
            inactive.k[1] = 0;
            let mut context = context_from(&inactive);
            let mut memory = FlatMemory::new(0x40);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &function.blocks[0],
            );
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                "{level:?} {masked:?}: {result:?}"
            );
            let actual = registers_from_context(&inactive, &context);
            let mut expected = inactive;
            expected.k[1] = 0;
            assert_eq!(actual, expected, "{level:?} {masked:?}: suppression");

            let mut active = initial_registers(masked, 307);
            active.gpr[3] = 0x3F;
            active.k[1] = 0b10;
            let old_destination = active.k[1];
            let mut context = context_from(&active);
            let mut memory = FlatMemory::new(0x40);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &function.blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr: 0x40,
                        write: false,
                        ..
                    })
                ),
                "{level:?} {masked:?}: {result:?}"
            );
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.k[1], old_destination, "{level:?} {masked:?}");
            assert_eq!(x86.rflags, active.rflags, "{level:?} {masked:?}");
            assert_eq!(x86.mxcsr, active.mxcsr, "{level:?} {masked:?}");

            let unmasked = VpshufbitqmbMemoryCase { mask: 0, ..masked };
            let function = optimize(lift_case(unmasked), level);
            let mut active = initial_registers(unmasked, 401);
            active.gpr[3] = 0x20_000;
            let old_destination = active.k[1];
            let mut context = context_from(&active);
            let mut memory = FlatMemory::new(0x40);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &function.blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr: 0x20_000,
                        write: false,
                        ..
                    })
                ),
                "{level:?} {unmasked:?}: {result:?}"
            );
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.k[1], old_destination, "{level:?} {unmasked:?}");
        }
    }
}
