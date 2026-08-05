use super::*;

fn source_patterns(format: SourceFormat) -> &'static [u64] {
    match format {
        SourceFormat::F16 => &[
            0x0000, 0x8000, 0x0001, 0x03FF, 0x3800, 0xB800, 0x3E00, 0xBE00, 0x4100, 0xC100, 0x7BFF,
            0xFBFF, 0x7C00, 0xFC00, 0x7E55, 0x7D55,
        ],
        SourceFormat::F32 => &[
            0x0000_0000,
            0x8000_0000,
            0x0000_0001,
            0x007F_FFFF,
            0x3F00_0000,
            0xBF00_0000,
            0x3FC0_0000,
            0xBFC0_0000,
            0x4020_0000,
            0xC020_0000,
            0x4EFF_FFFF,
            0x4F00_0000,
            0x5F80_0000,
            0x7F80_0000,
            0xFF80_0000,
            0x7FC1_2345,
            0x7F81_2345,
        ],
        SourceFormat::F64 => &[
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x000F_FFFF_FFFF_FFFF,
            0x3FE0_0000_0000_0000,
            0xBFE0_0000_0000_0000,
            0x3FF8_0000_0000_0000,
            0xBFF8_0000_0000_0000,
            0x4004_0000_0000_0000,
            0xC004_0000_0000_0000,
            0x41DF_FFFF_FFC0_0000,
            0x41E0_0000_0000_0000,
            0x43DF_FFFF_FFFF_FFFF,
            0x43E0_0000_0000_0000,
            0x43F0_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
            0x7FF8_1234_5678_9ABC,
            0x7FF0_1234_5678_9ABC,
        ],
    }
}

fn width_result(case: ScalarFpToIntMemoryCase, signed_value: i64) -> u64 {
    if case.w {
        signed_value as u64
    } else {
        signed_value as i32 as u32 as u64
    }
}

fn indefinite(case: ScalarFpToIntMemoryCase) -> u64 {
    match (case.signed, case.w) {
        (true, false) => 0x8000_0000,
        (true, true) => 0x8000_0000_0000_0000,
        (false, false) => u32::MAX as u64,
        (false, true) => u64::MAX,
    }
}

fn assert_only_destination_and_mxcsr_changed(
    case: ScalarFpToIntMemoryCase,
    initial: &GuestRegs,
    actual: &GuestRegs,
) {
    for index in 0..32 {
        if index != usize::from(case.destination) {
            assert_eq!(
                actual.gpr[index], initial.gpr[index],
                "{case:?}: GPR{index}"
            );
        }
    }
    if !case.w {
        assert_eq!(
            actual.gpr[usize::from(case.destination)] >> 32,
            0,
            "{case:?}"
        );
    }
    assert_eq!(actual.zmm, initial.zmm, "{case:?}: vector state");
    assert_eq!(actual.k, initial.k, "{case:?}: mask state");
    assert_eq!(actual.rflags, initial.rflags, "{case:?}: flags");
}

#[test]
fn all_72_scanner_cells_cover_special_values_and_o0_o1_o2_raw_equivalence() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 72);
    let mut comparisons = 0usize;
    for (ordinal, scanner_case) in cases.into_iter().enumerate() {
        let case = ScalarFpToIntMemoryCase {
            destination: [0, 1, 8, 15][ordinal & 3],
            ..scanner_case
        };
        let baseline = optimize(lift_case(case), OptLevel::O0);
        for (sample, &source) in source_patterns(case.format).iter().enumerate() {
            let rc = ((ordinal + sample) & 3) as u32;
            let daz = if (ordinal + sample) & 1 == 0 {
                0
            } else {
                1 << 6
            };
            let mxcsr = (0x1F80 & !(3 << 13)) | (rc << 13) | daz;
            let initial = initial_registers(case, ordinal ^ sample, mxcsr);
            let expected = interpreter_success(&baseline, &initial, source, case);
            assert_only_destination_and_mxcsr_changed(case, &initial, &expected);

            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let actual = interpreter_success(&function, &initial, source, case);
                assert_eq!(
                    actual, expected,
                    "{level:?} {case:?} source={source:#018X} mxcsr={mxcsr:#06X}"
                );
                comparisons += 1;
            }
        }
    }
    assert!(comparisons >= 72 * 16 * LEVELS.len());
}

#[test]
fn dynamic_rounding_and_truncation_match_independent_two_and_half_oracles() {
    let mut checks = 0usize;
    for format in SourceFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                for truncate in [false, true] {
                    let case = ScalarFpToIntMemoryCase {
                        format,
                        signed,
                        truncate,
                        w,
                        ll: ((usize::from(signed) + usize::from(w)) % 3) as u8,
                        destination: if w { 8 } else { 0 },
                        base: 2,
                    };
                    let source = if signed {
                        format.negative_two_and_half()
                    } else {
                        format.positive_two_and_half()
                    };
                    for rc in 0u32..4 {
                        let expected_signed = if truncate {
                            if signed { -2 } else { 2 }
                        } else if signed {
                            [-2, -3, -2, -2][rc as usize]
                        } else {
                            [2, 2, 3, 2][rc as usize]
                        };
                        let mxcsr = (0x1F80 & !(3 << 13)) | (rc << 13);
                        let initial = initial_registers(case, rc as usize, mxcsr);
                        for level in LEVELS {
                            let actual = interpreter_success(
                                &optimize(lift_case(case), level),
                                &initial,
                                source,
                                case,
                            );
                            assert_eq!(
                                actual.gpr[usize::from(case.destination)],
                                width_result(case, expected_signed),
                                "{level:?} {case:?} rc={rc}"
                            );
                            assert_ne!(
                                actual.mxcsr & (1 << 5),
                                0,
                                "{level:?} {case:?} rc={rc}: precision"
                            );
                            assert_only_destination_and_mxcsr_changed(case, &initial, &actual);
                            checks += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(checks, 3 * 2 * 2 * 2 * 4 * LEVELS.len());
}

#[test]
fn masked_invalid_nan_infinity_and_negative_unsigned_return_width_specific_indefinite() {
    let mut checks = 0usize;
    for format in SourceFormat::ALL {
        for signed in [false, true] {
            for truncate in [false, true] {
                for w in [false, true] {
                    let case = ScalarFpToIntMemoryCase {
                        format,
                        signed,
                        truncate,
                        w,
                        ll: (checks % 3) as u8,
                        destination: if w { 15 } else { 1 },
                        base: 2,
                    };
                    let mut sources = vec![format.quiet_nan(), format.positive_infinity()];
                    if !signed {
                        sources.push(format.negative_two_and_half());
                    }
                    for source in sources {
                        let initial = initial_registers(case, checks, 0x1F80);
                        for level in LEVELS {
                            let actual = interpreter_success(
                                &optimize(lift_case(case), level),
                                &initial,
                                source,
                                case,
                            );
                            assert_eq!(
                                actual.gpr[usize::from(case.destination)],
                                indefinite(case),
                                "{level:?} {case:?} source={source:#018X}"
                            );
                            assert_ne!(actual.mxcsr & 1, 0, "{level:?} {case:?}: invalid");
                            assert_only_destination_and_mxcsr_changed(case, &initial, &actual);
                            checks += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        checks,
        3 * 2 * 2 * 2 * 2 * LEVELS.len() + 3 * 1 * 2 * 2 * LEVELS.len()
    );
}

#[test]
fn daz_applies_to_binary32_binary64_but_binary16_subnormals_remain_precision_inexact() {
    for format in SourceFormat::ALL {
        let case = ScalarFpToIntMemoryCase {
            format,
            signed: true,
            truncate: false,
            w: false,
            ll: 0,
            destination: 0,
            base: 2,
        };
        for daz in [false, true] {
            let mxcsr = 0x1F80 | (u32::from(daz) << 6);
            let initial = initial_registers(case, usize::from(daz), mxcsr);
            for level in LEVELS {
                let actual = interpreter_success(
                    &optimize(lift_case(case), level),
                    &initial,
                    format.positive_subnormal(),
                    case,
                );
                assert_eq!(actual.gpr[0], 0, "{level:?} {format:?} daz={daz}");
                assert_eq!(
                    actual.mxcsr & (1 << 5) != 0,
                    format == SourceFormat::F16 || !daz,
                    "{level:?} {format:?} daz={daz}: precision"
                );
            }
        }
    }
}

#[test]
fn every_source_width_faults_before_gpr_mxcsr_vector_mask_or_flags_commit() {
    let mut faults = 0usize;
    for format in SourceFormat::ALL {
        for signed in [false, true] {
            for truncate in [false, true] {
                for w in [false, true] {
                    let case = ScalarFpToIntMemoryCase {
                        format,
                        signed,
                        truncate,
                        w,
                        ll: 2,
                        destination: 9,
                        base: 2,
                    };
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let initial = initial_registers(case, faults, 0x1F80 | (2 << 13));
                        let mut context = interpreter_context(&initial);
                        let mut partial = FlatMemory::new(
                            MEMORY_ADDRESS as usize + case.memory_size().saturating_sub(1),
                        );
                        let result = SmirInterpreter::new().execute_block(
                            &mut context,
                            &mut partial,
                            &function.blocks[0],
                        );
                        assert!(matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ));
                        assert_eq!(
                            interpreter_registers(&context, &initial),
                            initial,
                            "{level:?} {case:?}: partial source fault committed state"
                        );
                        faults += 1;
                    }
                }
            }
        }
    }
    assert_eq!(faults, 3 * 2 * 2 * 2 * LEVELS.len());
}

#[test]
fn unmasked_invalid_and_precision_exceptions_are_precise_and_noncommitting() {
    for (name, source, exception_bit, mask_bit) in [
        ("invalid", SourceFormat::F64.quiet_nan(), 0u32, 7u32),
        (
            "precision",
            SourceFormat::F64.positive_two_and_half(),
            5u32,
            12u32,
        ),
    ] {
        for truncate in [false, true] {
            let case = ScalarFpToIntMemoryCase {
                format: SourceFormat::F64,
                signed: true,
                truncate,
                w: true,
                ll: 1,
                destination: 9,
                base: 2,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let mut initial = initial_registers(case, usize::from(truncate), 0x1F80);
                initial.mxcsr &= !(1 << mask_bit);
                let mut context = interpreter_context(&initial);
                let mut memory = FlatMemory::new(0x3000);
                memory.load(MEMORY_ADDRESS as usize, &source.to_le_bytes());
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
                    "{name} {level:?} {case:?}: {result:?}"
                );
                let actual = interpreter_registers(&context, &initial);
                assert_eq!(
                    actual.gpr, initial.gpr,
                    "{name} {level:?} {case:?}: trapped conversion committed GPR"
                );
                assert_eq!(actual.zmm, initial.zmm, "{name} {level:?} {case:?}");
                assert_eq!(actual.k, initial.k, "{name} {level:?} {case:?}");
                assert_eq!(actual.rflags, initial.rflags, "{name} {level:?} {case:?}");
                assert_ne!(
                    actual.mxcsr & (1 << exception_bit),
                    0,
                    "{name} {level:?} {case:?}: status"
                );
            }
        }
    }
}
