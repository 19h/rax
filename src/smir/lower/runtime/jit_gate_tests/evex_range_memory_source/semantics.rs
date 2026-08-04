//! Interpreter, optimizer, MXCSR, and fault semantics for VRANGE memory replay.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::GuestRegs;

fn set_lane(words: &mut [u64; 8], elem: VecElementType, lane: usize, value: u64) {
    match elem {
        VecElementType::F32 => {
            let word = lane / 2;
            let shift = (lane % 2) * 32;
            words[word] = (words[word] & !(u64::from(u32::MAX) << shift))
                | (u64::from(value as u32) << shift);
        }
        VecElementType::F64 => words[lane] = value,
        _ => unreachable!("VRANGE binary32/binary64 element"),
    }
}

fn get_lane(words: &[u64; 8], elem: VecElementType, lane: usize) -> u64 {
    match elem {
        VecElementType::F32 => (words[lane / 2] >> ((lane % 2) * 32)) & u64::from(u32::MAX),
        VecElementType::F64 => words[lane],
        _ => unreachable!("VRANGE binary32/binary64 element"),
    }
}

fn source_bits(elem: VecElementType, lane: usize) -> u64 {
    const F32: [u32; 16] = [
        0x7FC0_1234,
        0x7F80_1234,
        0x0000_0000,
        0x8000_0000,
        0x3FC0_0000,
        0xBFC0_0000,
        0x4020_0000,
        0xC020_0000,
        0x0000_0001,
        0x8000_0001,
        0x7F80_0000,
        0xFF80_0000,
        0x3F80_0000,
        0xBF80_0000,
        0x4040_0000,
        0xC040_0000,
    ];
    const F64: [u64; 16] = [
        0x7FF8_0000_0000_1234,
        0x7FF0_0000_0000_1234,
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3FF8_0000_0000_0000,
        0xBFF8_0000_0000_0000,
        0x4004_0000_0000_0000,
        0xC004_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x3FF0_0000_0000_0000,
        0xBFF0_0000_0000_0000,
        0x4008_0000_0000_0000,
        0xC008_0000_0000_0000,
    ];
    match elem {
        VecElementType::F32 => u64::from(F32[lane % F32.len()]),
        VecElementType::F64 => F64[lane % F64.len()],
        _ => unreachable!("VRANGE binary32/binary64 element"),
    }
}

pub(super) fn initial_registers(case: RangeMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0xA5A5_0000_0000_0000u64
                ^ ((ordinal as u64) << 12)
                ^ (index as u64 * 0x0101_0101_0101_0101)
        }),
        zmm: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x1111_0000_0000_0000u64 ^ ((register as u64) << 24) ^ (word as u64 * 0x0101_0101)
            })
        }),
        k: std::array::from_fn(|index| {
            if index == 1 {
                0xA55A_A55A
            } else {
                0xF0F0_0000 ^ index as u64
            }
        }),
        rflags: 0x8D5,
        mxcsr: 0x1F80,
        ..GuestRegs::default()
    };
    registers.gpr[3] = 0x2000;
    let lanes = VecWidth::V512.lanes(case.elem) as usize;
    for lane in 0..lanes {
        set_lane(
            &mut registers.zmm[usize::from(case.source1)],
            case.elem,
            lane,
            source_bits(case.elem, lane + ordinal),
        );
    }
    registers
}

pub(super) fn memory_value(case: RangeMemoryCase, ordinal: usize) -> [u64; 8] {
    let mut value = [0u64; 8];
    let lanes = VecWidth::V512.lanes(case.elem) as usize;
    for lane in 0..lanes {
        set_lane(
            &mut value,
            case.elem,
            lane,
            source_bits(case.elem, lane + ordinal + 5),
        );
    }
    value
}

pub(super) fn memory_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn fp_masks(elem: VecElementType) -> (u64, u64, u64, u64) {
    match elem {
        VecElementType::F32 => (0x8000_0000, 0x7F80_0000, 0x007F_FFFF, 0x0040_0000),
        VecElementType::F64 => (
            0x8000_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0x000F_FFFF_FFFF_FFFF,
            0x0008_0000_0000_0000,
        ),
        _ => unreachable!("VRANGE binary32/binary64 element"),
    }
}

fn is_qnan(bits: u64, elem: VecElementType) -> bool {
    let (_, exponent, fraction, quiet) = fp_masks(elem);
    bits & exponent == exponent && bits & fraction != 0 && bits & quiet != 0
}

fn is_snan(bits: u64, elem: VecElementType) -> bool {
    let (_, exponent, fraction, quiet) = fp_masks(elem);
    bits & exponent == exponent && bits & fraction != 0 && bits & quiet == 0
}

fn is_subnormal(bits: u64, elem: VecElementType) -> bool {
    let (_, exponent, fraction, _) = fp_masks(elem);
    bits & exponent == 0 && bits & fraction != 0
}

fn less_equal(a: u64, b: u64, elem: VecElementType, absolute: bool) -> bool {
    let sign = fp_masks(elem).0;
    let a = if absolute { a & !sign } else { a };
    let b = if absolute { b & !sign } else { b };
    match elem {
        VecElementType::F32 => f32::from_bits(a as u32) <= f32::from_bits(b as u32),
        VecElementType::F64 => f64::from_bits(a) <= f64::from_bits(b),
        _ => unreachable!("VRANGE binary32/binary64 element"),
    }
}

fn manual_lane(mut a: u64, mut b: u64, elem: VecElementType, mxcsr: u32, imm: u8) -> (u64, u32) {
    let (sign, _, fraction, quiet) = fp_masks(elem);
    if is_snan(a, elem) {
        return (a | quiet, 1);
    }
    if is_snan(b, elem) {
        return (b | quiet, 1);
    }

    let mut status = 0u32;
    let daz = mxcsr & (1 << 6) != 0;
    if is_subnormal(a, elem) {
        if daz {
            a &= sign;
        } else if !is_qnan(b, elem) {
            status |= 1 << 1;
        }
    }
    if is_subnormal(b, elem) {
        if daz {
            b &= sign;
        } else if !is_qnan(a, elem) {
            status |= 1 << 1;
        }
    }

    let compare = imm & 3;
    let a_magnitude = a & !sign;
    let b_magnitude = b & !sign;
    let opposite_sign = (a ^ b) & sign != 0;
    let temporary = if is_qnan(b, elem) {
        a
    } else if is_qnan(a, elem) {
        b
    } else if a_magnitude == 0 && b_magnitude == 0 && opposite_sign {
        if compare & 1 == 0 { sign } else { 0 }
    } else if compare >= 2 && a_magnitude == b_magnitude && opposite_sign {
        if compare == 2 {
            sign | a_magnitude
        } else {
            a_magnitude
        }
    } else {
        match compare {
            0 => {
                if less_equal(a, b, elem, false) {
                    a
                } else {
                    b
                }
            }
            1 => {
                if less_equal(a, b, elem, false) {
                    b
                } else {
                    a
                }
            }
            2 => {
                if less_equal(a, b, elem, true) {
                    a
                } else {
                    b
                }
            }
            3 => {
                if less_equal(a, b, elem, true) {
                    b
                } else {
                    a
                }
            }
            _ => unreachable!(),
        }
    };
    let magnitude = temporary & (sign - 1);
    let result = match (imm >> 2) & 3 {
        0 => (a & sign) | magnitude,
        1 => temporary,
        2 => magnitude,
        3 => sign | magnitude,
        _ => unreachable!(),
    };
    let result = match elem {
        VecElementType::F32 => u64::from(result as u32),
        VecElementType::F64 => result,
        _ => unreachable!("VRANGE binary32/binary64 element"),
    };
    (result, status)
}

fn manual_success(initial: &GuestRegs, memory: [u64; 8], case: RangeMemoryCase) -> GuestRegs {
    let mut expected = *initial;
    let first = initial.zmm[usize::from(case.source1)];
    let old = initial.zmm[usize::from(case.destination)];
    let mut result = if case.scalar() { first } else { old };
    if case.scalar() {
        result[2..].fill(0);
    } else {
        result[usize::try_from(case.width.bytes() / 8).unwrap()..].fill(0);
    }
    let lanes = if case.scalar() {
        1
    } else {
        case.width.lanes(case.elem) as usize
    };
    let active = if case.mask() == 0 {
        u64::MAX
    } else {
        initial.k[usize::from(case.mask())]
    };
    let mut status = 0u32;
    for lane in 0..lanes {
        if active & (1u64 << lane) == 0 {
            if case.zeroing() {
                set_lane(&mut result, case.elem, lane, 0);
            } else {
                set_lane(
                    &mut result,
                    case.elem,
                    lane,
                    get_lane(&old, case.elem, lane),
                );
            }
            continue;
        }
        let source_lane = if case.broadcast() { 0 } else { lane };
        let (bits, lane_status) = manual_lane(
            get_lane(&first, case.elem, lane),
            get_lane(&memory, case.elem, source_lane),
            case.elem,
            initial.mxcsr,
            case.immediate,
        );
        status |= lane_status;
        set_lane(&mut result, case.elem, lane, bits);
    }
    expected.mxcsr |= status;
    expected.zmm[usize::from(case.destination)] = result;
    expected
}

fn context_from(initial: &GuestRegs) -> SmirContext {
    let mut context = SmirContext::new_x86_64();
    context.pc = PC;
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

pub(super) fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory_value: [u64; 8],
    case: RangeMemoryCase,
) -> GuestRegs {
    let mut context = context_from(initial);
    let mut memory = FlatMemory::new(0x10000);
    let bytes = memory_bytes(memory_value);
    memory.load(0x2000, &bytes[..case.memory_size() as usize]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{result:?}"
    );

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

#[test]
fn every_control_matches_intel_range_model_across_all_shapes_daz_and_opt_levels() {
    let mut comparisons = 0usize;
    for (ordinal, base) in all_cases().into_iter().enumerate() {
        for immediate in 0..=0x0Fu8 {
            let case = RangeMemoryCase { immediate, ..base };
            let memory = memory_value(case, ordinal);
            for daz in [false, true] {
                let mut initial = initial_registers(case, ordinal);
                initial.mxcsr = (initial.mxcsr & !(1 << 6)) | (u32::from(daz) << 6);
                let expected = manual_success(&initial, memory, case);
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let actual = interpreter_success(&function, &initial, memory, case);
                    assert_eq!(actual, expected, "{level:?} DAZ={daz} {case:?}");
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 180 * 16 * 2 * LEVELS.len());
}

#[test]
fn inactive_memory_suppresses_faults_and_active_faults_do_not_commit() {
    for elem in [VecElementType::F32, VecElementType::F64] {
        for form in [
            SourceForm::Vector,
            SourceForm::Broadcast,
            SourceForm::Scalar { ll: 3 },
        ] {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = RangeMemoryCase {
                    elem,
                    width: VecWidth::V512,
                    destination: 17,
                    source1: 18,
                    form,
                    control,
                    immediate: 0x0D,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_registers(case, 0);
                    initial.gpr[3] = 0x20_000;
                    initial.k[usize::from(case.mask())] = 0;
                    let expected = manual_success(&initial, [0; 8], case);
                    let mut context = context_from(&initial);
                    let mut memory = FlatMemory::new(0x1000);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(matches!(
                        result,
                        BlockResult::Exit(ExitReason::Return { .. })
                    ));
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(x86.xmm[17][..8], expected.zmm[17]);
                    assert_eq!(x86.mxcsr, expected.mxcsr);

                    let mut active = initial;
                    active.k[usize::from(case.mask())] = 1;
                    let old = active.zmm[17];
                    let mut context = context_from(&active);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(matches!(
                        result,
                        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                    ));
                    let ArchRegState::X86_64(x86) = &context.arch_regs else {
                        unreachable!()
                    };
                    assert_eq!(x86.xmm[17][..8], old);
                    assert_eq!(x86.mxcsr, active.mxcsr);
                }
            }
        }
    }
}

#[test]
fn unmasked_invalid_exception_sets_ie_and_preserves_destination_atomically() {
    for form in [
        SourceForm::Vector,
        SourceForm::Broadcast,
        SourceForm::Scalar { ll: 3 },
    ] {
        let case = RangeMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V512,
            destination: 17,
            source1: 18,
            form,
            control: MaskControl::None,
            immediate: 0x0F,
        };
        let mut memory_value = memory_value(case, 0);
        set_lane(&mut memory_value, case.elem, 0, 0x7F80_1234);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut initial = initial_registers(case, 0);
            initial.mxcsr &= !(1 << 7);
            let old = initial.zmm[17];
            let mut context = context_from(&initial);
            let mut memory = FlatMemory::new(0x10000);
            let bytes = memory_bytes(memory_value);
            memory.load(0x2000, &bytes[..case.memory_size() as usize]);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: PC })
            ));
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            assert_eq!(x86.xmm[17][..8], old);
            assert_eq!(x86.mxcsr & 1, 1);
        }
    }
}
