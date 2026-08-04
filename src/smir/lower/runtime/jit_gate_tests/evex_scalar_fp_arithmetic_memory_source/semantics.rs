use super::*;

fn scalar_mask(format: ScalarFormat) -> u64 {
    match format {
        ScalarFormat::F16 => u64::from(u16::MAX),
        ScalarFormat::F32 => u64::from(u32::MAX),
        ScalarFormat::F64 => u64::MAX,
    }
}

fn exact_inactive_destination(case: ScalarFpMemoryCase, initial: &GuestRegs) -> [u64; 8] {
    let mut expected = initial.zmm[usize::from(case.source1)];
    let mask = scalar_mask(case.format);
    let low = match case.control {
        MaskControl::Merge => initial.zmm[usize::from(case.destination())][0] & mask,
        MaskControl::Zero => 0,
        MaskControl::None => unreachable!("unmasked scalar operation is always active"),
    };
    expected[0] = (expected[0] & !mask) | low;
    expected[2..].fill(0);
    expected
}

#[test]
fn all_567_scalar_fp_cells_have_o0_o1_o2_interpreter_equivalence_and_exact_masks() {
    let cases = all_cases();
    assert_eq!(cases.len(), 567);
    let mut active_executions = 0usize;
    let mut inactive_executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let source = scalar_bits(case.format, case.operation, ordinal);
        let active_initial = initial_registers(case, ordinal, true);
        let baseline = optimize(lift_case(case), OptLevel::O0);
        let active_expected = interpreter_success(&baseline, &active_initial, source, case);
        assert_eq!(active_expected.rflags, active_initial.rflags, "{case:?}");
        assert_eq!(active_expected.k, active_initial.k, "{case:?}");
        assert_eq!(
            active_expected.zmm[usize::from(case.destination())][2..],
            [0; 6],
            "{case:?}: bits above XMM must be zero"
        );

        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let active = interpreter_success(&function, &active_initial, source, case);
            assert_eq!(active, active_expected, "{level:?} {case:?}: active");
            active_executions += 1;
        }

        if case.control != MaskControl::None {
            let inactive_initial = initial_registers(case, ordinal, false);
            let inactive_expected = interpreter_success(&baseline, &inactive_initial, source, case);
            assert_eq!(
                inactive_expected.zmm[usize::from(case.destination())],
                exact_inactive_destination(case, &inactive_initial),
                "{case:?}: inactive merge/zero and source-1 upper lanes"
            );
            assert_eq!(inactive_expected.mxcsr, inactive_initial.mxcsr, "{case:?}");
            assert_eq!(
                inactive_expected.rflags, inactive_initial.rflags,
                "{case:?}"
            );
            assert_eq!(inactive_expected.k, inactive_initial.k, "{case:?}");
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let inactive = interpreter_success(&function, &inactive_initial, source, case);
                assert_eq!(inactive, inactive_expected, "{level:?} {case:?}: inactive");
                inactive_executions += 1;
            }
        }
    }
    assert_eq!(active_executions, 567 * LEVELS.len());
    assert_eq!(inactive_executions, 7 * 3 * 3 * 3 * 2 * LEVELS.len());
}

fn set_low_scalar(registers: &mut GuestRegs, index: u8, format: ScalarFormat, value: u64) {
    let mask = scalar_mask(format);
    let word = &mut registers.zmm[usize::from(index)][0];
    *word = (*word & !mask) | (value & mask);
}

#[test]
fn finite_scalar_results_match_independent_binary16_binary32_binary64_oracles() {
    let cases = [
        (
            ScalarFpMemoryCase {
                operation: ArithmeticOperation::Mul,
                format: ScalarFormat::F16,
                source1: 1,
                ll: 0,
                control: MaskControl::None,
            },
            0x3E00,
            0x4000,
            0x4200,
        ), // 1.5 * 2.0 = 3.0
        (
            ScalarFpMemoryCase {
                operation: ArithmeticOperation::Add,
                format: ScalarFormat::F32,
                source1: 17,
                ll: 1,
                control: MaskControl::Merge,
            },
            0x3FC0_0000,
            0x4000_0000,
            0x4060_0000,
        ), // 1.5 + 2.0 = 3.5
        (
            ScalarFpMemoryCase {
                operation: ArithmeticOperation::Sqrt,
                format: ScalarFormat::F64,
                source1: 30,
                ll: 2,
                control: MaskControl::Zero,
            },
            0xBFF0_0000_0000_0000,
            0x4010_0000_0000_0000,
            0x4000_0000_0000_0000,
        ), // sqrt(4.0) = 2.0; source 1 supplies only upper XMM bits.
    ];
    for (ordinal, (case, source1, memory, expected_low)) in cases.into_iter().enumerate() {
        let mut initial = initial_registers(case, ordinal, true);
        set_low_scalar(&mut initial, case.source1, case.format, source1);
        let source1_xmm = initial.zmm[usize::from(case.source1)];
        let actual = interpreter_success(&lift_case(case), &initial, memory, case);
        let mask = scalar_mask(case.format);
        assert_eq!(
            actual.zmm[usize::from(case.destination())][0] & mask,
            expected_low,
            "{case:?}"
        );
        assert_eq!(
            actual.zmm[usize::from(case.destination())][0] & !mask,
            source1_xmm[0] & !mask,
            "{case:?}: upper bits within lane 0"
        );
        assert_eq!(
            actual.zmm[usize::from(case.destination())][1],
            source1_xmm[1],
            "{case:?}: upper 64 XMM bits"
        );
        assert_eq!(actual.zmm[usize::from(case.destination())][2..], [0; 6]);
    }
}

fn special_values(format: ScalarFormat) -> [u64; 10] {
    match format {
        ScalarFormat::F16 => [
            0x0000, 0x8000, 0x0001, 0x03FF, 0x0400, 0x7BFF, 0x7C00, 0xFC00, 0x7E11, 0x7C22,
        ],
        ScalarFormat::F32 => [
            0x0000_0000,
            0x8000_0000,
            0x0000_0001,
            0x007F_FFFF,
            0x0080_0000,
            0x7F7F_FFFF,
            0x7F80_0000,
            0xFF80_0000,
            0x7FC0_0011,
            0x7F80_0022,
        ],
        ScalarFormat::F64 => [
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x000F_FFFF_FFFF_FFFF,
            0x0010_0000_0000_0000,
            0x7FEF_FFFF_FFFF_FFFF,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
            0x7FF8_0000_0000_0011,
            0x7FF0_0000_0000_0022,
        ],
    }
}

#[test]
fn special_values_rounding_daz_and_ftz_preserve_raw_o0_o1_o2_equivalence() {
    // The corpus covers +/-0, minimum/maximum subnormal, minimum normal,
    // maximum finite, +/-infinity, quiet NaN with payload, and signaling NaN
    // with payload. The four MXCSR images cover RN/RD/RU/RZ and clear,
    // DAZ-only, FTZ-only, and DAZ+FTZ modes with all exceptions masked.
    let mxcsr_modes = [0x1F80, 0x3FC0, 0x5F80 | 0x8000, 0x7FC0 | 0x8000];
    let mut comparisons = 0usize;
    for operation in ArithmeticOperation::ALL {
        for format in ScalarFormat::ALL {
            let values = special_values(format);
            for (value_index, memory) in values.into_iter().enumerate() {
                for (mode_index, mxcsr) in mxcsr_modes.into_iter().enumerate() {
                    let case = ScalarFpMemoryCase {
                        operation,
                        format,
                        source1: [1, 17, 30][value_index % 3],
                        ll: (mode_index % 3) as u8,
                        control: MaskControl::ALL[(value_index + mode_index) % 3],
                    };
                    let mut initial = initial_registers(case, value_index, true);
                    initial.mxcsr = mxcsr;
                    set_low_scalar(
                        &mut initial,
                        case.source1,
                        format,
                        values[(value_index + 3) % values.len()],
                    );
                    let expected = interpreter_success(&lift_case(case), &initial, memory, case);
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let actual = interpreter_success(&function, &initial, memory, case);
                        assert_eq!(
                            actual, expected,
                            "{level:?} {case:?} memory={memory:#018X} mxcsr={mxcsr:#06X}"
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 7 * 3 * 10 * 4 * LEVELS.len());
}

#[test]
fn type_e3_mask_bit_zero_suppresses_access_and_all_source_faults_are_noncommitting() {
    let mut suppressions = 0usize;
    let mut masked_faults = 0usize;
    let mut unmasked_faults = 0usize;
    for operation in ArithmeticOperation::ALL {
        for format in ScalarFormat::ALL {
            for control in MaskControl::ALL {
                let case = ScalarFpMemoryCase {
                    operation,
                    format,
                    source1: 17,
                    ll: 2,
                    control,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    if control != MaskControl::None {
                        let inactive = initial_registers(case, 0x10, false);
                        let expected =
                            interpreter_success(&function, &inactive, 0xDEAD_BEEF_CAFE_BABE, case);
                        let mut context = interpreter_context(&inactive);
                        let mut inaccessible = FlatMemory::new(MEMORY_ADDRESS as usize);
                        let result = SmirInterpreter::new().execute_block(
                            &mut context,
                            &mut inaccessible,
                            &function.blocks[0],
                        );
                        assert!(matches!(
                            result,
                            BlockResult::Exit(ExitReason::Return { .. })
                        ));
                        assert_eq!(
                            interpreter_registers(&context, &inactive),
                            expected,
                            "{level:?} {case:?}: inactive source access was not suppressed"
                        );
                        suppressions += 1;

                        let active = initial_registers(case, 0x20, true);
                        let mut context = interpreter_context(&active);
                        let mut inaccessible = FlatMemory::new(MEMORY_ADDRESS as usize);
                        let result = SmirInterpreter::new().execute_block(
                            &mut context,
                            &mut inaccessible,
                            &function.blocks[0],
                        );
                        assert!(matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ));
                        assert_eq!(
                            interpreter_registers(&context, &active),
                            active,
                            "{level:?} {case:?}: active fault committed architectural state"
                        );
                        masked_faults += 1;
                    } else {
                        let initial = initial_registers(case, 0x30, true);
                        let mut context = interpreter_context(&initial);
                        let mut inaccessible = FlatMemory::new(MEMORY_ADDRESS as usize);
                        let result = SmirInterpreter::new().execute_block(
                            &mut context,
                            &mut inaccessible,
                            &function.blocks[0],
                        );
                        assert!(matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                        ));
                        assert_eq!(
                            interpreter_registers(&context, &initial),
                            initial,
                            "{level:?} {case:?}: unmasked fault committed architectural state"
                        );
                        unmasked_faults += 1;
                    }
                }
            }
        }
    }
    assert_eq!(suppressions, 7 * 3 * 2 * LEVELS.len());
    assert_eq!(masked_faults, suppressions);
    assert_eq!(unmasked_faults, 7 * 3 * LEVELS.len());
}
