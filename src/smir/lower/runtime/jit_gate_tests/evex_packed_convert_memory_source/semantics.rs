//! Interpreter oracles, mask suppression, and precise-frontier checks.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64};

pub(super) fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (index, word) in words.into_iter().enumerate() {
        bytes[index * 8..(index + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub(super) fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..(index + 1) * 8].try_into().unwrap())
    })
}

fn put_element(bytes: &mut [u8; 64], elem: VecElementType, lane: usize, value: u64) {
    let size = elem.bytes() as usize;
    bytes[lane * size..(lane + 1) * size].copy_from_slice(&value.to_le_bytes()[..size]);
}

fn get_element(bytes: &[u8; 64], elem: VecElementType, lane: usize) -> u64 {
    let size = elem.bytes() as usize;
    let mut value = [0u8; 8];
    value[..size].copy_from_slice(&bytes[lane * size..(lane + 1) * size]);
    u64::from_le_bytes(value)
}

pub(super) fn memory_bytes(case: ConvertCase, seed: usize) -> [u8; 64] {
    const F16_VALUES: [u64; 8] = [
        0x0000, 0x8000, 0x3C00, 0xC000, 0x0001, 0x03FF, 0x0400, 0x7BFF,
    ];
    const F32_VALUES: [u64; 8] = [
        0x0000_0000,
        0x8000_0000,
        0x3F00_0000,
        0x3FC0_0000,
        0x4020_0000,
        0x4120_0000,
        0x4B80_0001,
        0x0080_0000,
    ];
    const F64_VALUES: [u64; 8] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3FE0_0000_0000_0000,
        0x3FF8_0000_0000_0000,
        0x4004_0000_0000_0000,
        0x4024_0000_0000_0000,
        0x4330_0000_0000_0001,
        0x0010_0000_0000_0000,
    ];
    const I32_VALUES: [u64; 8] = [
        0,
        1,
        u32::MAX as u64,
        i32::MAX as u32 as u64,
        i32::MIN as u32 as u64,
        16_777_217,
        0x5555_5555,
        0xAAAA_AAAA,
    ];
    const I64_VALUES: [u64; 8] = [
        0,
        1,
        u64::MAX,
        i64::MAX as u64,
        i64::MIN as u64,
        9_007_199_254_740_993,
        0x5555_5555_5555_5555,
        0xAAAA_AAAA_AAAA_AAAA,
    ];

    let values = match case.spec.source_elem() {
        VecElementType::F16 => F16_VALUES,
        VecElementType::F32 => F32_VALUES,
        VecElementType::F64 => F64_VALUES,
        VecElementType::I32 => I32_VALUES,
        VecElementType::I64 => I64_VALUES,
        other => unreachable!("packed conversion source {other:?}"),
    };
    let mut bytes = [0u8; 64];
    let lanes = if case.broadcast() {
        1
    } else {
        usize::from(case.lanes())
    };
    for lane in 0..lanes {
        put_element(
            &mut bytes,
            case.spec.source_elem(),
            lane,
            values[(seed + lane) % values.len()],
        );
    }
    bytes
}

pub(super) fn initial_registers(
    case: ConvertCase,
    seed: usize,
    mxcsr: u32,
    mask: u64,
) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x10)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K64,
        mxcsr,
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    registers.gpr[2] = MEMORY_ADDRESS;
    registers.k[1] = mask;
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
        });
    }
    // Ensure this helper remains coupled to the requested destination even
    // when tests select high EVEX register indices.
    assert!(usize::from(case.destination) < registers.zmm.len());
    registers
}

fn interpreter_context(initial: &GuestRegs) -> SmirContext {
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
        x86.apx_enabled = true;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    context
}

fn interpreter_registers(context: &SmirContext, initial: &GuestRegs) -> GuestRegs {
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut result = *initial;
    result.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        result.zmm[index].copy_from_slice(&value[..8]);
    }
    result.k = x86.k;
    result.rflags = x86.rflags;
    result.mxcsr = x86.mxcsr;
    result
}

pub(super) fn interpret_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    bytes: &[u8; 64],
    case: ConvertCase,
) -> GuestRegs {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x3000);
    memory.load(
        MEMORY_ADDRESS as usize,
        &bytes[..case.memory_size() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    interpreter_registers(&context, initial)
}

fn assert_only_destination_and_mxcsr_changed(
    case: ConvertCase,
    initial: &GuestRegs,
    actual: &GuestRegs,
) {
    assert_eq!(actual.gpr, initial.gpr, "{case:?}: GPRs");
    for index in 0..32 {
        if index != usize::from(case.destination) {
            assert_eq!(
                actual.zmm[index], initial.zmm[index],
                "{case:?}: ZMM{index}"
            );
        }
    }
    assert_eq!(actual.k, initial.k, "{case:?}: opmasks");
    assert_eq!(actual.rflags, initial.rflags, "{case:?}: RFLAGS");
}

const FP16_EDGE_VALUES: [u16; 16] = [
    0x0000, // +0
    0x8000, // -0
    0x0001, // minimum subnormal
    0x7D55, // signaling NaN
    0x03FF, // maximum subnormal
    0x0400, // minimum normal
    0x7E55, // quiet NaN with payload
    0x7C00, // +infinity
    0xFC00, // -infinity
    0x7BFF, // maximum finite
    0x3C00, // +1
    0xC000, // -2
    0x3555, 0xB555, 0x0401, 0xFBFF,
];

/// Exact IEEE 754 binary16-to-binary32 widening, including payload placement
/// and signaling-NaN quieting. Every finite binary16 value is exactly
/// representable as binary32, so no rounding operation is required.
fn f16_to_f32_bits_oracle(bits: u16) -> u32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1F;
    let fraction = u32::from(bits & 0x03FF);
    match exponent {
        0 if fraction == 0 => sign,
        0 => {
            let mut significand = fraction;
            let mut unbiased_exponent = -14i32;
            while significand & 0x0400 == 0 {
                significand <<= 1;
                unbiased_exponent -= 1;
            }
            significand &= 0x03FF;
            sign | (((unbiased_exponent + 127) as u32) << 23) | (significand << 13)
        }
        0x1F if fraction == 0 => sign | 0x7F80_0000,
        0x1F => sign | 0x7FC0_0000 | (fraction << 13),
        _ => sign | (u32::from(exponent + 112) << 23) | (fraction << 13),
    }
}

fn f16_is_signaling_nan(bits: u16) -> bool {
    bits & 0x7C00 == 0x7C00 && bits & 0x03FF != 0 && bits & 0x0200 == 0
}

#[test]
fn all_468_encodings_preserve_raw_o0_o1_o2_interpreter_equivalence() {
    let mut encodings = 0usize;
    let mut comparisons = 0usize;
    for spec in SPECS {
        for ll in 0..=2 {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for control in MaskControl::ALL {
                    let case = ConvertCase {
                        spec,
                        ll,
                        destination: [0, 8, 17, 31][encodings & 3],
                        form,
                        control,
                    };
                    let lanes = case.lanes();
                    let lane_mask = (1u64 << lanes) - 1;
                    let mask = if case.mask() == 0 {
                        u64::MAX
                    } else {
                        (0xAAAA_AAAA_AAAA_AAAAu64 ^ (encodings as u64)) & lane_mask
                    };
                    let mxcsr = (0x1F80 & !(3 << 13)) | (((encodings & 3) as u32) << 13);
                    let initial = initial_registers(case, encodings, mxcsr, mask);
                    let memory = memory_bytes(case, encodings);
                    let baseline = optimize(lift_case(case), OptLevel::O0);
                    let expected = interpret_success(&baseline, &initial, &memory, case);
                    assert_only_destination_and_mxcsr_changed(case, &initial, &expected);
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let actual = interpret_success(&function, &initial, &memory, case);
                        assert_eq!(actual, expected, "{level:?} {case:?}");
                        comparisons += 1;
                    }
                    encodings += 1;
                }
            }
        }
    }
    assert_eq!(encodings, 468);
    assert_eq!(comparisons, encodings * LEVELS.len());
}

#[test]
fn all_9_fp16_widen_memory_encodings_are_exact_and_ignore_daz_ftz_and_rounding() {
    let controls = [
        (0u32, false, false),
        (1, true, false),
        (2, false, true),
        (3, true, true),
    ];
    let mut comparisons = 0usize;
    for ll in 0..=2 {
        for control in MaskControl::ALL {
            let case = ConvertCase {
                spec: FP16_WIDEN_SPEC,
                ll,
                destination: 17,
                form: SourceForm::Vector,
                control,
            };
            let mut memory = [0u8; 64];
            for (lane, value) in FP16_EDGE_VALUES
                .into_iter()
                .take(usize::from(case.lanes()))
                .enumerate()
            {
                put_element(&mut memory, VecElementType::F16, lane, u64::from(value));
            }

            for (config_index, (round, daz, ftz)) in controls.into_iter().enumerate() {
                let lane_mask = (1u64 << case.lanes()) - 1;
                let mask = if config_index & 1 == 0 {
                    0xA5A5 & lane_mask
                } else {
                    0x5A5A & lane_mask
                };
                let mxcsr = (0x1F80 & !(3 << 13))
                    | (round << 13)
                    | (u32::from(daz) << 6)
                    | (u32::from(ftz) << 15);
                let initial = initial_registers(case, config_index, mxcsr, mask);
                let initial_destination = words_to_bytes(initial.zmm[17]);
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let actual = interpret_success(&function, &initial, &memory, case);
                    assert_only_destination_and_mxcsr_changed(case, &initial, &actual);
                    let destination = words_to_bytes(actual.zmm[17]);
                    let mut invalid = false;
                    for lane in 0..usize::from(case.lanes()) {
                        let active = control == MaskControl::None || mask & (1u64 << lane) != 0;
                        let expected = if active {
                            let source = FP16_EDGE_VALUES[lane];
                            invalid |= f16_is_signaling_nan(source);
                            u64::from(f16_to_f32_bits_oracle(source))
                        } else if control == MaskControl::Zero {
                            0
                        } else {
                            get_element(&initial_destination, VecElementType::F32, lane)
                        };
                        assert_eq!(
                            get_element(&destination, VecElementType::F32, lane),
                            expected,
                            "{level:?} {case:?} RC={round} DAZ={daz} FTZ={ftz} lane={lane}"
                        );
                    }
                    assert!(
                        destination[case.operation_width().bytes() as usize..]
                            .iter()
                            .all(|byte| *byte == 0),
                        "{level:?} {case:?}: EVEX upper-lane clearing"
                    );
                    assert_eq!(actual.mxcsr & 0x3F, u32::from(invalid));
                    assert_eq!(actual.mxcsr & !0x3F, initial.mxcsr & !0x3F);
                    assert_eq!(actual.mxcsr & (1 << 1), 0, "FP16 denormal status");
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 9 * controls.len() * LEVELS.len());
}

#[test]
fn fp16_type_e11_masks_suppress_each_inactive_two_byte_lane_fault() {
    for ll in [0, 2] {
        for control in [MaskControl::Merge, MaskControl::Zero] {
            let case = ConvertCase {
                spec: FP16_WIDEN_SPEC,
                ll,
                destination: 17,
                form: SourceForm::Vector,
                control,
            };
            for mask in [0, 1u64 << 63] {
                let initial = initial_registers(case, usize::from(ll), 0x1F80, mask);
                let initial_destination = words_to_bytes(initial.zmm[17]);
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut context = interpreter_context(&initial);
                    let mut unmapped = FlatMemory::new(MEMORY_ADDRESS as usize);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut unmapped,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                        "{level:?} {case:?} mask={mask:#018X}: {result:?}"
                    );
                    let actual = interpreter_registers(&context, &initial);
                    let destination = words_to_bytes(actual.zmm[17]);
                    let active_bytes = case.operation_width().bytes() as usize;
                    if control == MaskControl::Merge {
                        assert_eq!(
                            &destination[..active_bytes],
                            &initial_destination[..active_bytes]
                        );
                    } else {
                        assert!(destination[..active_bytes].iter().all(|byte| *byte == 0));
                    }
                    assert!(destination[active_bytes..].iter().all(|byte| *byte == 0));
                    assert_eq!(actual.mxcsr, initial.mxcsr);
                }
            }
        }
    }

    let case = ConvertCase {
        spec: FP16_WIDEN_SPEC,
        ll: 2,
        destination: 17,
        form: SourceForm::Vector,
        control: MaskControl::Merge,
    };
    let bytes = memory_bytes(case, 2);
    for level in LEVELS {
        let function = optimize(lift_case(case), level);

        // Lane zero alone is mapped and active; all later unmapped lanes are
        // inactive and therefore cannot fault.
        let initial = initial_registers(case, 2, 0x1F80, 1);
        let mut context = interpreter_context(&initial);
        let mut lane_zero_only = FlatMemory::new(MEMORY_ADDRESS as usize + 2);
        lane_zero_only.load(MEMORY_ADDRESS as usize, &bytes[..2]);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut lane_zero_only,
            &function.blocks[0],
        );
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
            "{level:?}: inactive later lane faulted: {result:?}"
        );

        // Lanes zero and three are active, but memory ends immediately before
        // lane three's two-byte element. Earlier helper reads cannot commit.
        let initial = initial_registers(case, 3, 0x1F80, 0b1001);
        let mut context = interpreter_context(&initial);
        let mut partial = FlatMemory::new(MEMORY_ADDRESS as usize + 6);
        partial.load(MEMORY_ADDRESS as usize, &bytes[..6]);
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut partial, &function.blocks[0]);
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
            ),
            "{level:?}: active lane three did not fault: {result:?}"
        );
        assert_eq!(interpreter_registers(&context, &initial), initial);

        // A tuple ending exactly at the memory boundary is valid.
        let initial = initial_registers(case, 4, 0x1F80, 0xFFFF);
        let mut context = interpreter_context(&initial);
        let size = case.memory_size() as usize;
        let mut exact = FlatMemory::new(MEMORY_ADDRESS as usize + size);
        exact.load(MEMORY_ADDRESS as usize, &bytes[..size]);
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut exact, &function.blocks[0]);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
            "{level:?}: exact tuple boundary: {result:?}"
        );
    }
}

#[test]
fn fp16_unmasked_signaling_nan_exception_is_precise_and_noncommitting() {
    let case = ConvertCase {
        spec: FP16_WIDEN_SPEC,
        ll: 0,
        destination: 17,
        form: SourceForm::Vector,
        control: MaskControl::None,
    };
    let mut bytes = [0u8; 64];
    for (lane, value) in [0x7D55u16, 0x3C00, 0x0001, 0x7E55].into_iter().enumerate() {
        put_element(&mut bytes, VecElementType::F16, lane, u64::from(value));
    }
    for level in LEVELS {
        let function = optimize(lift_case(case), level);
        let mut initial = initial_registers(case, level as usize, 0x1F80, u64::MAX);
        initial.mxcsr &= !(1 << 7);
        let mut context = interpreter_context(&initial);
        let mut memory = FlatMemory::new(0x3000);
        memory.load(
            MEMORY_ADDRESS as usize,
            &bytes[..case.memory_size() as usize],
        );
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: PC })
            ),
            "{level:?}: {result:?}"
        );
        let actual = interpreter_registers(&context, &initial);
        assert_eq!(actual.gpr, initial.gpr, "{level:?}");
        assert_eq!(actual.zmm, initial.zmm, "{level:?}");
        assert_eq!(actual.k, initial.k, "{level:?}");
        assert_eq!(actual.rflags, initial.rflags, "{level:?}");
        assert_eq!(actual.mxcsr, initial.mxcsr | 1, "{level:?}");
        assert_eq!(actual.mxcsr & (1 << 1), 0, "{level:?}: no FP16 denormal")
    }
}

#[test]
fn fp_precision_and_integer_to_fp_match_independent_language_oracles() {
    let precision = [
        (SPECS[0], 0xBFC0_0000u64, (f64::from(-1.5f32)).to_bits()),
        (
            SPECS[1],
            0x3FF8_0000_0000_0000u64,
            (1.5f64 as f32).to_bits() as u64,
        ),
    ];
    for (spec, source, expected) in precision {
        let case = ConvertCase {
            spec,
            ll: 0,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        };
        let mut memory = [0u8; 64];
        put_element(&mut memory, spec.source_elem(), 0, source);
        let initial = initial_registers(case, 1, 0x1F80, u64::MAX);
        for level in LEVELS {
            let actual =
                interpret_success(&optimize(lift_case(case), level), &initial, &memory, case);
            let destination = words_to_bytes(actual.zmm[usize::from(case.destination)]);
            for lane in 0..usize::from(case.lanes()) {
                assert_eq!(
                    get_element(&destination, spec.destination_elem(), lane),
                    expected,
                    "{level:?} {} lane={lane}",
                    spec.name
                );
            }
        }
    }

    for spec in SPECS
        .into_iter()
        .filter(|spec| matches!(spec.kind, IntToFp { .. }))
    {
        let case = ConvertCase {
            spec,
            ll: 0,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        };
        let raw = match spec.kind {
            IntToFp {
                int_elem: VecElementType::I32,
                signed: true,
                ..
            } => (-16_777_217i32) as u32 as u64,
            IntToFp {
                int_elem: VecElementType::I32,
                signed: false,
                ..
            } => u32::MAX as u64,
            IntToFp {
                int_elem: VecElementType::I64,
                signed: true,
                ..
            } => (-9_007_199_254_740_993i64) as u64,
            IntToFp {
                int_elem: VecElementType::I64,
                signed: false,
                ..
            } => u64::MAX,
            _ => unreachable!(),
        };
        let expected = match spec.kind {
            IntToFp {
                int_elem: VecElementType::I32,
                fp_elem: VecElementType::F32,
                signed: true,
            } => (raw as u32 as i32 as f32).to_bits() as u64,
            IntToFp {
                int_elem: VecElementType::I32,
                fp_elem: VecElementType::F32,
                signed: false,
            } => (raw as u32 as f32).to_bits() as u64,
            IntToFp {
                int_elem: VecElementType::I32,
                fp_elem: VecElementType::F64,
                signed: true,
            } => (raw as u32 as i32 as f64).to_bits(),
            IntToFp {
                int_elem: VecElementType::I32,
                fp_elem: VecElementType::F64,
                signed: false,
            } => (raw as u32 as f64).to_bits(),
            IntToFp {
                int_elem: VecElementType::I64,
                fp_elem: VecElementType::F32,
                signed: true,
            } => (raw as i64 as f32).to_bits() as u64,
            IntToFp {
                int_elem: VecElementType::I64,
                fp_elem: VecElementType::F32,
                signed: false,
            } => (raw as f32).to_bits() as u64,
            IntToFp {
                int_elem: VecElementType::I64,
                fp_elem: VecElementType::F64,
                signed: true,
            } => (raw as i64 as f64).to_bits(),
            IntToFp {
                int_elem: VecElementType::I64,
                fp_elem: VecElementType::F64,
                signed: false,
            } => (raw as f64).to_bits(),
            _ => unreachable!(),
        };
        let mut memory = [0u8; 64];
        put_element(&mut memory, spec.source_elem(), 0, raw);
        let initial = initial_registers(case, 2, 0x1F80, u64::MAX);
        for level in LEVELS {
            let actual =
                interpret_success(&optimize(lift_case(case), level), &initial, &memory, case);
            let destination = words_to_bytes(actual.zmm[usize::from(case.destination)]);
            for lane in 0..usize::from(case.lanes()) {
                assert_eq!(
                    get_element(&destination, spec.destination_elem(), lane),
                    expected,
                    "{level:?} {} lane={lane}",
                    spec.name
                );
            }
        }
    }
}

#[test]
fn all_fp_to_integer_rounding_modes_truncation_and_indefinite_values_are_exact() {
    let mut checks = 0usize;
    for spec in SPECS
        .into_iter()
        .filter(|spec| matches!(spec.kind, FpToInt { .. }))
    {
        let FpToInt {
            fp_elem,
            int_elem,
            signed,
            truncate,
        } = spec.kind
        else {
            unreachable!()
        };
        let case = ConvertCase {
            spec,
            ll: 0,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        };
        let source = match (fp_elem, signed) {
            (VecElementType::F32, true) => 0xC020_0000,
            (VecElementType::F32, false) => 0x4020_0000,
            (VecElementType::F64, true) => 0xC004_0000_0000_0000,
            (VecElementType::F64, false) => 0x4004_0000_0000_0000,
            _ => unreachable!(),
        };
        let mut memory = [0u8; 64];
        put_element(&mut memory, fp_elem, 0, source);
        for rc in 0u32..4 {
            let expected = if truncate {
                if signed { (-2i64) as u64 } else { 2 }
            } else if signed {
                [
                    (-2i64) as u64,
                    (-3i64) as u64,
                    (-2i64) as u64,
                    (-2i64) as u64,
                ][rc as usize]
            } else {
                [2, 2, 3, 2][rc as usize]
            } & if int_elem == VecElementType::I32 {
                u32::MAX as u64
            } else {
                u64::MAX
            };
            let mxcsr = (0x1F80 & !(3 << 13)) | (rc << 13);
            let initial = initial_registers(case, rc as usize, mxcsr, u64::MAX);
            for level in LEVELS {
                let actual =
                    interpret_success(&optimize(lift_case(case), level), &initial, &memory, case);
                let destination = words_to_bytes(actual.zmm[usize::from(case.destination)]);
                for lane in 0..usize::from(case.lanes()) {
                    assert_eq!(
                        get_element(&destination, int_elem, lane),
                        expected,
                        "{level:?} {} rc={rc} lane={lane}",
                        spec.name
                    );
                }
                assert_ne!(actual.mxcsr & (1 << 5), 0, "{level:?} {}", spec.name);
                checks += 1;
            }
        }

        let nan = if fp_elem == VecElementType::F32 {
            0x7FC1_2345
        } else {
            0x7FF8_1234_5678_9ABC
        };
        put_element(&mut memory, fp_elem, 0, nan);
        let initial = initial_registers(case, 9, 0x1F80, u64::MAX);
        let indefinite = match (signed, int_elem) {
            (true, VecElementType::I32) => 0x8000_0000,
            (true, VecElementType::I64) => 0x8000_0000_0000_0000,
            (false, VecElementType::I32) => u32::MAX as u64,
            (false, VecElementType::I64) => u64::MAX,
            _ => unreachable!(),
        };
        for level in LEVELS {
            let actual =
                interpret_success(&optimize(lift_case(case), level), &initial, &memory, case);
            let destination = words_to_bytes(actual.zmm[usize::from(case.destination)]);
            for lane in 0..usize::from(case.lanes()) {
                assert_eq!(get_element(&destination, int_elem, lane), indefinite);
            }
            assert_ne!(actual.mxcsr & 1, 0, "{level:?} {}", spec.name);
        }
    }
    assert_eq!(checks, 16 * 4 * LEVELS.len());
}

#[test]
fn masks_merge_zero_and_suppress_inactive_memory_and_fp_exceptions() {
    let spec = SPECS[10]; // VCVTPS2DQ
    for control in [MaskControl::Merge, MaskControl::Zero] {
        let case = ConvertCase {
            spec,
            ll: 0,
            destination: 17,
            form: SourceForm::Vector,
            control,
        };
        let mut memory = [0u8; 64];
        for (lane, value) in [0x3F80_0000, 0x7FC1_2345, 0x4020_0000, 0x7F80_0000]
            .into_iter()
            .enumerate()
        {
            put_element(&mut memory, VecElementType::F32, lane, value);
        }
        let initial = initial_registers(case, 4, 0x1F80, 0b0101);
        let initial_destination = words_to_bytes(initial.zmm[17]);
        for level in LEVELS {
            let actual =
                interpret_success(&optimize(lift_case(case), level), &initial, &memory, case);
            let destination = words_to_bytes(actual.zmm[17]);
            assert_eq!(get_element(&destination, VecElementType::I32, 0), 1);
            assert_eq!(get_element(&destination, VecElementType::I32, 2), 2);
            for lane in [1usize, 3] {
                assert_eq!(
                    get_element(&destination, VecElementType::I32, lane),
                    if control == MaskControl::Zero {
                        0
                    } else {
                        get_element(&initial_destination, VecElementType::I32, lane)
                    },
                    "{level:?} {control:?} lane={lane}"
                );
            }
            assert_eq!(actual.mxcsr & 1, 0, "{level:?}: inactive invalid lane");
            assert_ne!(actual.mxcsr & (1 << 5), 0, "{level:?}: active 2.5 lane");
        }
    }

    for (spec, control) in [
        (SPECS[0], MaskControl::Merge),
        (SPECS[2], MaskControl::Zero),
    ] {
        let case = ConvertCase {
            spec,
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control,
        };
        let initial = initial_registers(case, 7, 0x1F80, 0);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut context = interpreter_context(&initial);
            let mut unmapped = FlatMemory::new(MEMORY_ADDRESS as usize);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut unmapped,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::Return { .. })
            ));
            let actual = interpreter_registers(&context, &initial);
            if control == MaskControl::Merge {
                assert_eq!(actual.zmm[17], initial.zmm[17], "{level:?} {}", spec.name);
            } else {
                assert_eq!(actual.zmm[17], [0; 8], "{level:?} {}", spec.name);
            }
            assert_eq!(actual.mxcsr, initial.mxcsr, "{level:?} {}", spec.name);
        }
    }
}

#[test]
fn every_masked_broadcast_reads_when_an_applicable_bit_above_bit_zero_is_set() {
    let mut semantic_checks = 0usize;
    let mut fault_checks = 0usize;
    for spec in SPECS {
        for ll in 0..=2 {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = ConvertCase {
                    spec,
                    ll,
                    destination: 17,
                    form: SourceForm::Broadcast,
                    control,
                };
                let active_lane = usize::from(case.lanes() - 1);
                let active_mask = 1u64 << active_lane;
                let memory = memory_bytes(case, 3);
                let initial = initial_registers(case, 3, 0x1F80, active_mask);
                let lane_zero_initial = initial_registers(case, 3, 0x1F80, 1);

                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let actual = interpret_success(&function, &initial, &memory, case);
                    let lane_zero = interpret_success(&function, &lane_zero_initial, &memory, case);
                    let actual_destination =
                        words_to_bytes(actual.zmm[usize::from(case.destination)]);
                    let lane_zero_destination =
                        words_to_bytes(lane_zero.zmm[usize::from(case.destination)]);
                    assert_eq!(
                        get_element(&actual_destination, spec.destination_elem(), active_lane,),
                        get_element(&lane_zero_destination, spec.destination_elem(), 0),
                        "{level:?} {case:?}: broadcast source was not read for k1[{active_lane}]",
                    );
                    assert_eq!(
                        actual.mxcsr, lane_zero.mxcsr,
                        "{level:?} {case:?}: one active broadcast lane must have identical FP status",
                    );
                    semantic_checks += 1;

                    let mut context = interpreter_context(&initial);
                    let mut unmapped = FlatMemory::new(MEMORY_ADDRESS as usize);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut unmapped,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ),
                        "{level:?} {case:?}: k1[{active_lane}] did not enable the broadcast read: {result:?}",
                    );
                    assert_eq!(
                        interpreter_registers(&context, &initial),
                        initial,
                        "{level:?} {case:?}: enabled broadcast fault committed state",
                    );
                    fault_checks += 1;
                }
            }
        }
    }
    assert_eq!(semantic_checks, 26 * 3 * 2 * LEVELS.len());
    assert_eq!(fault_checks, semantic_checks);
}

#[test]
fn every_replay_form_faults_before_destination_mxcsr_mask_or_flags_commit() {
    let cases = [
        ConvertCase {
            spec: SPECS[0],
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[17],
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: SPECS[1],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[0],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: SPECS[2],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: FP16_WIDEN_SPEC,
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
    ];
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_registers(case, ordinal, 0x1F80 | (2 << 13), u64::MAX);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut context = interpreter_context(&initial);
            let partial_size = MEMORY_ADDRESS as usize + case.memory_size() as usize - 1;
            let mut partial = FlatMemory::new(partial_size);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
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
                interpreter_registers(&context, &initial),
                initial,
                "{level:?} {case:?}: fault committed state"
            );
        }
    }
}

#[test]
fn unmasked_invalid_and_precision_exceptions_are_precise_and_noncommitting() {
    let case = ConvertCase {
        spec: SPECS[10],
        ll: 0,
        destination: 17,
        form: SourceForm::Broadcast,
        control: MaskControl::None,
    };
    for (name, source, status_bit, mask_bit) in [
        ("invalid", 0x7FC1_2345u64, 0u32, 7u32),
        ("precision", 0x4020_0000u64, 5u32, 12u32),
    ] {
        let mut memory_bytes = [0u8; 64];
        put_element(&mut memory_bytes, VecElementType::F32, 0, source);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut initial = initial_registers(case, level as usize, 0x1F80, u64::MAX);
            initial.mxcsr &= !(1 << mask_bit);
            let mut context = interpreter_context(&initial);
            let mut memory = FlatMemory::new(0x3000);
            memory.load(
                MEMORY_ADDRESS as usize,
                &memory_bytes[..case.memory_size() as usize],
            );
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &function.blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: PC })
                ),
                "{name} {level:?}: {result:?}"
            );
            let actual = interpreter_registers(&context, &initial);
            assert_eq!(actual.gpr, initial.gpr, "{name} {level:?}");
            assert_eq!(actual.zmm, initial.zmm, "{name} {level:?}");
            assert_eq!(actual.k, initial.k, "{name} {level:?}");
            assert_eq!(actual.rflags, initial.rflags, "{name} {level:?}");
            assert_ne!(actual.mxcsr & (1 << status_bit), 0, "{name} {level:?}");
        }
    }
}
