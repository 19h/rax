//! Interpreter, optimizer, MXCSR, and fault semantics for VSCALEF memory replay.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::GuestRegs;

const REGISTER_SOURCE2: u8 = 23;

fn set_lane(words: &mut [u64; 8], elem: VecElementType, lane: usize, value: u64) {
    let (lanes_per_word, bits) = match elem {
        VecElementType::F16 => (4, 16),
        VecElementType::F32 => (2, 32),
        VecElementType::F64 => (1, 64),
        _ => unreachable!("VSCALEF binary16/binary32/binary64 element"),
    };
    let word = lane / lanes_per_word;
    let shift = (lane % lanes_per_word) * bits;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    words[word] = (words[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn get_lane(words: &[u64; 8], elem: VecElementType, lane: usize) -> u64 {
    let (lanes_per_word, bits) = match elem {
        VecElementType::F16 => (4, 16),
        VecElementType::F32 => (2, 32),
        VecElementType::F64 => (1, 64),
        _ => unreachable!("VSCALEF binary16/binary32/binary64 element"),
    };
    let shift = (lane % lanes_per_word) * bits;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    (words[lane / lanes_per_word] >> shift) & mask
}

fn source_bits(elem: VecElementType, lane: usize) -> u64 {
    const F16: [u16; 16] = [
        0x3E00, 0xBE00, 0x4000, 0xC000, 0x0000, 0x8000, 0x0001, 0x8001, 0x7BFF, 0xFBFF, 0x7C00,
        0xFC00, 0x7E34, 0x7C34, 0x3C00, 0xBC00,
    ];
    const F32: [u32; 16] = [
        0x3FC0_0000,
        0xBFC0_0000,
        0x4000_0000,
        0xC000_0000,
        0x0000_0000,
        0x8000_0000,
        0x0000_0001,
        0x8000_0001,
        0x7F7F_FFFF,
        0xFF7F_FFFF,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC0_1234,
        0x7F80_1234,
        0x3F80_0000,
        0xBF80_0000,
    ];
    const F64: [u64; 16] = [
        0x3FF8_0000_0000_0000,
        0xBFF8_0000_0000_0000,
        0x4000_0000_0000_0000,
        0xC000_0000_0000_0000,
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x7FEF_FFFF_FFFF_FFFF,
        0xFFEF_FFFF_FFFF_FFFF,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_1234,
        0x7FF0_0000_0000_1234,
        0x3FF0_0000_0000_0000,
        0xBFF0_0000_0000_0000,
    ];
    match elem {
        VecElementType::F16 => u64::from(F16[lane % F16.len()]),
        VecElementType::F32 => u64::from(F32[lane % F32.len()]),
        VecElementType::F64 => F64[lane % F64.len()],
        _ => unreachable!("VSCALEF binary16/binary32/binary64 element"),
    }
}

fn one_bits(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::F16 => 0x3C00,
        VecElementType::F32 => 0x3F80_0000,
        VecElementType::F64 => 0x3FF0_0000_0000_0000,
        _ => unreachable!("VSCALEF binary16/binary32/binary64 element"),
    }
}

fn signaling_nan_bits(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::F16 => 0x7C01,
        VecElementType::F32 => 0x7F80_0001,
        VecElementType::F64 => 0x7FF0_0000_0000_0001,
        _ => unreachable!("VSCALEF binary16/binary32/binary64 element"),
    }
}

fn max_finite_bits(elem: VecElementType) -> u64 {
    match elem {
        VecElementType::F16 => 0x7BFF,
        VecElementType::F32 => 0x7F7F_FFFF,
        VecElementType::F64 => 0x7FEF_FFFF_FFFF_FFFF,
        _ => unreachable!("VSCALEF binary16/binary32/binary64 element"),
    }
}

pub(super) fn initial_registers(case: ScaleFMemoryCase, ordinal: usize) -> GuestRegs {
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

pub(super) fn memory_value(case: ScaleFMemoryCase, ordinal: usize) -> [u64; 8] {
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

fn register_source(case: ScaleFMemoryCase, memory: [u64; 8]) -> [u64; 8] {
    if !case.broadcast() {
        return memory;
    }
    let scalar = get_lane(&memory, case.elem, 0);
    let mut result = [0; 8];
    for lane in 0..VecWidth::V512.lanes(case.elem) as usize {
        set_lane(&mut result, case.elem, lane, scalar);
    }
    result
}

fn register_function(case: ScaleFMemoryCase) -> SmirFunction {
    let register_case = if case.broadcast() {
        ScaleFMemoryCase {
            form: SourceForm::Vector,
            ..case
        }
    } else {
        case
    };
    lift_bytes(&register_encoding(register_case, REGISTER_SOURCE2))
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

fn architectural_state(context: &SmirContext, initial: &GuestRegs) -> GuestRegs {
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
    memory_value: [u64; 8],
    case: ScaleFMemoryCase,
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
    architectural_state(&context, initial)
}

#[test]
fn every_memory_shape_matches_register_semantics_across_mxcsr_and_opt_levels() {
    let mut comparisons = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        let memory = memory_value(case, ordinal);
        for rounding_control in 0..4u32 {
            for daz in [false, true] {
                for ftz in [false, true] {
                    let mut initial = initial_registers(case, ordinal);
                    initial.mxcsr = (initial.mxcsr & !((3 << 13) | (1 << 6) | (1 << 15)))
                        | (rounding_control << 13)
                        | (u32::from(daz) << 6)
                        | (u32::from(ftz) << 15);
                    initial.zmm[usize::from(REGISTER_SOURCE2)] = register_source(case, memory);
                    for level in LEVELS {
                        let memory_function = optimize(lift_case(case), level);
                        let register_function = optimize(register_function(case), level);
                        let expected =
                            interpreter_success(&register_function, &initial, memory, case);
                        let actual = interpreter_success(&memory_function, &initial, memory, case);
                        assert_eq!(
                            actual, expected,
                            "{level:?} RC={rounding_control} DAZ={daz} FTZ={ftz} {case:?}"
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 360 * 4 * 2 * 2 * LEVELS.len());
}

#[test]
fn inactive_memory_suppresses_faults_and_active_faults_do_not_commit() {
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for form in [
            SourceForm::Vector,
            SourceForm::Broadcast,
            SourceForm::Scalar { ll: 3 },
        ] {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = ScaleFMemoryCase {
                    elem,
                    width: VecWidth::V512,
                    destination: 17,
                    source1: 18,
                    form,
                    control,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_registers(case, 0);
                    initial.gpr[3] = 0x20_000;
                    initial.k[usize::from(case.mask())] = 0;
                    initial.zmm[usize::from(REGISTER_SOURCE2)] = [0; 8];
                    let expected = interpreter_success(
                        &optimize(register_function(case), level),
                        &initial,
                        [0; 8],
                        case,
                    );
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
                    assert_eq!(architectural_state(&context, &initial), expected);

                    let mut active = initial;
                    active.k[usize::from(case.mask())] = 1;
                    let mut context = context_from(&active);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(matches!(
                        result,
                        BlockResult::Exit(ExitReason::MemoryFault {
                            addr: 0x20_000,
                            write: false
                        })
                    ));
                    assert_eq!(context.pc, PC);
                    assert_eq!(architectural_state(&context, &active), active);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ExceptionCase {
    Invalid,
    Overflow,
}

fn exception_inputs(case: ScaleFMemoryCase, kind: ExceptionCase) -> (GuestRegs, [u64; 8], u32) {
    let mut initial = initial_registers(case, 0);
    let lanes = VecWidth::V512.lanes(case.elem) as usize;
    for lane in 0..lanes {
        set_lane(
            &mut initial.zmm[usize::from(case.source1)],
            case.elem,
            lane,
            match kind {
                ExceptionCase::Invalid => one_bits(case.elem),
                ExceptionCase::Overflow => max_finite_bits(case.elem),
            },
        );
    }
    let mut memory = [0; 8];
    for lane in 0..lanes {
        set_lane(&mut memory, case.elem, lane, one_bits(case.elem));
    }
    let (mask_bit, status_bit) = match kind {
        ExceptionCase::Invalid => {
            set_lane(&mut memory, case.elem, 0, signaling_nan_bits(case.elem));
            (7, 0)
        }
        ExceptionCase::Overflow => (10, 3),
    };
    initial.mxcsr &= !(1 << mask_bit);
    (initial, memory, status_bit)
}

#[test]
fn unmasked_invalid_and_overflow_exceptions_are_atomic_for_every_precision_and_form() {
    let mut traps = 0usize;
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        for form in [
            SourceForm::Vector,
            SourceForm::Broadcast,
            SourceForm::Scalar { ll: 3 },
        ] {
            for kind in [ExceptionCase::Invalid, ExceptionCase::Overflow] {
                let case = ScaleFMemoryCase {
                    elem,
                    width: VecWidth::V512,
                    destination: 17,
                    source1: 18,
                    form,
                    control: MaskControl::None,
                };
                let (initial, memory_value, status_bit) = exception_inputs(case, kind);
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
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
                    let actual = architectural_state(&context, &initial);
                    assert_eq!(
                        actual.zmm[usize::from(case.destination)],
                        initial.zmm[usize::from(case.destination)],
                        "{level:?} {case:?}"
                    );
                    assert_eq!(actual.mxcsr & (1 << status_bit), 1 << status_bit);
                    assert_eq!(actual.gpr, initial.gpr);
                    assert_eq!(actual.k, initial.k);
                    assert_eq!(actual.rflags, initial.rflags);
                    traps += 1;
                }
            }
        }
    }
    assert_eq!(traps, 3 * 3 * 2 * LEVELS.len());
}
