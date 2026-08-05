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
        0x0000, 0x8000, 0x0001, 0x3E00, 0xC100, 0x7BFF, 0x7E01, 0x7C00,
    ];
    const F32_VALUES: [u64; 8] = [
        0x0000_0000,
        0x8000_0000,
        0x3F00_0000,
        0x3FC0_0000,
        0x4020_0000,
        0x477F_E000,
        0x7FC1_2345,
        0x3380_0000,
    ];
    const F64_VALUES: [u64; 8] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3FE0_0000_0000_0000,
        0x3FF8_0000_0000_0000,
        0x4004_0000_0000_0000,
        0x40EF_FFC0_0000_0000,
        0x7FF8_1234_5678_9ABC,
        0x3E70_0000_0000_0000,
    ];
    const I16_VALUES: [u64; 8] = [
        0,
        1,
        u16::MAX as u64,
        i16::MAX as u16 as u64,
        i16::MIN as u16 as u64,
        2049,
        0x5555,
        0xAAAA,
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
        VecElementType::I16 => I16_VALUES,
        VecElementType::I32 => I32_VALUES,
        VecElementType::I64 => I64_VALUES,
        other => unreachable!("packed FP16 conversion source {other:?}"),
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

#[test]
fn all_396_encodings_preserve_raw_o0_o1_o2_interpreter_equivalence() {
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
    assert_eq!(encodings, 396);
    assert_eq!(comparisons, encodings * LEVELS.len());
}

#[test]
fn precision_and_integer_to_fp16_match_independent_exact_bit_oracles() {
    let precision = [
        (SPECS[0], 0x3FF8_0000_0000_0000u64, 0x3E00u64),
        (SPECS[1], 0x3E00, 0x3FF8_0000_0000_0000),
        (SPECS[2], 0x3FC0_0000, 0x3E00),
        (SPECS[3], 0x3E00, 0x3FC0_0000),
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
        .filter(|spec| matches!(spec.kind, IntToFp16 { .. }))
    {
        let IntToFp16 { int_elem, signed } = spec.kind else {
            unreachable!()
        };
        let case = ConvertCase {
            spec,
            ll: 0,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        };
        let source = if signed {
            match int_elem {
                VecElementType::I16 => (-2i16) as u16 as u64,
                VecElementType::I32 => (-2i32) as u32 as u64,
                VecElementType::I64 => (-2i64) as u64,
                _ => unreachable!(),
            }
        } else {
            2
        };
        let expected = if signed { 0xC000 } else { 0x4000 };
        let mut memory = [0u8; 64];
        put_element(&mut memory, int_elem, 0, source);
        let initial = initial_registers(case, 2, 0x1F80, u64::MAX);
        for level in LEVELS {
            let actual =
                interpret_success(&optimize(lift_case(case), level), &initial, &memory, case);
            let destination = words_to_bytes(actual.zmm[usize::from(case.destination)]);
            for lane in 0..usize::from(case.lanes()) {
                assert_eq!(
                    get_element(&destination, VecElementType::F16, lane),
                    expected,
                    "{level:?} {} lane={lane}",
                    spec.name
                );
            }
        }
    }
}

#[test]
fn all_fp16_to_integer_rounding_modes_and_truncation_are_exact() {
    let mut checks = 0usize;
    for spec in SPECS
        .into_iter()
        .filter(|spec| matches!(spec.kind, Fp16ToInt { .. }))
    {
        let Fp16ToInt {
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
        let mut memory = [0u8; 64];
        put_element(
            &mut memory,
            VecElementType::F16,
            0,
            if signed { 0xC100 } else { 0x4100 },
        );
        let width_mask = match int_elem {
            VecElementType::I16 => u16::MAX as u64,
            VecElementType::I32 => u32::MAX as u64,
            VecElementType::I64 => u64::MAX,
            _ => unreachable!(),
        };
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
            } & width_mask;
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
    }
    assert_eq!(checks, 12 * 4 * LEVELS.len());
}

#[test]
fn masks_merge_zero_and_suppress_inactive_memory_and_fp_exceptions() {
    let spec = SPECS[10]; // VCVTPH2DQ
    for control in [MaskControl::Merge, MaskControl::Zero] {
        let case = ConvertCase {
            spec,
            ll: 0,
            destination: 17,
            form: SourceForm::Vector,
            control,
        };
        let mut memory = [0u8; 64];
        for (lane, value) in [0x3C00, 0x7E01, 0x4100, 0x7C00].into_iter().enumerate() {
            put_element(&mut memory, VecElementType::F16, lane, value);
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

    for control in [MaskControl::Merge, MaskControl::Zero] {
        let case = ConvertCase {
            spec: SPECS[10],
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
                assert_eq!(actual.zmm[17], initial.zmm[17], "{level:?}");
            } else {
                assert_eq!(actual.zmm[17], [0; 8], "{level:?}");
            }
            assert_eq!(actual.mxcsr, initial.mxcsr, "{level:?}");
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
    assert_eq!(semantic_checks, 22 * 3 * 2 * LEVELS.len());
    assert_eq!(fault_checks, semantic_checks);
}

#[test]
fn every_replay_source_shape_faults_before_destination_mxcsr_mask_or_flags_commit() {
    let cases = [
        ConvertCase {
            spec: SPECS[0],
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[1],
            ll: 0,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::None,
        },
        ConvertCase {
            spec: SPECS[4],
            ll: 2,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: SPECS[0],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: SPECS[4],
            ll: 2,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::Merge,
        },
        ConvertCase {
            spec: SPECS[12],
            ll: 0,
            destination: 17,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
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
fn unmasked_invalid_overflow_and_precision_exceptions_are_precise_and_noncommitting() {
    let cases = [
        (SPECS[10], 0x7E01u64, 0u32, 7u32),
        (SPECS[5], i64::MAX as u64, 3u32, 10u32),
        (SPECS[0], 0x3FF1_9999_9999_999Au64, 5u32, 12u32),
    ];
    for (spec, source, status_bit, mask_bit) in cases {
        let case = ConvertCase {
            spec,
            ll: 0,
            destination: 17,
            form: SourceForm::Broadcast,
            control: MaskControl::None,
        };
        let mut memory_bytes = [0u8; 64];
        put_element(&mut memory_bytes, spec.source_elem(), 0, source);
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
                "{} {level:?}: {result:?}",
                spec.name
            );
            let actual = interpreter_registers(&context, &initial);
            assert_eq!(actual.gpr, initial.gpr, "{} {level:?}", spec.name);
            assert_eq!(actual.zmm, initial.zmm, "{} {level:?}", spec.name);
            assert_eq!(actual.k, initial.k, "{} {level:?}", spec.name);
            assert_eq!(actual.rflags, initial.rflags, "{} {level:?}", spec.name);
            assert_ne!(
                actual.mxcsr & (1 << status_bit),
                0,
                "{} {level:?}",
                spec.name
            );
        }
    }
}
