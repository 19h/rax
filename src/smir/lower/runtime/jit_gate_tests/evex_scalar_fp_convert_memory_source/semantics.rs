use super::*;

fn element_mask(element: VecElementType) -> u64 {
    match element {
        VecElementType::F16 => u64::from(u16::MAX),
        VecElementType::F32 => u64::from(u32::MAX),
        VecElementType::F64 => u64::MAX,
        _ => unreachable!(),
    }
}

fn exact_inactive_destination(case: ScalarConvertMemoryCase, initial: &GuestRegs) -> [u64; 8] {
    let mut expected = initial.zmm[usize::from(case.merge)];
    let mask = element_mask(case.conversion.to());
    let low = match case.control {
        MaskControl::Merge => initial.zmm[usize::from(case.destination())][0] & mask,
        MaskControl::Zero => 0,
        MaskControl::None => unreachable!("unmasked conversion is always active"),
    };
    expected[0] = (expected[0] & !mask) | low;
    expected[2..].fill(0);
    expected
}

#[test]
fn all_162_scalar_convert_cells_have_o0_o1_o2_equivalence_and_exact_masks() {
    let cases = all_cases();
    assert_eq!(cases.len(), 162);
    let mut active_executions = 0usize;
    let mut inactive_executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let source = source_bits(case.conversion, ordinal);
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
    assert_eq!(active_executions, 162 * LEVELS.len());
    assert_eq!(inactive_executions, 6 * 3 * 3 * 2 * LEVELS.len());
}

#[test]
fn finite_scalar_results_match_six_independent_exact_bit_oracles() {
    for (ordinal, (conversion, source, expected_low)) in [
        (Conversion::F64ToF32, 0x3FF8_0000_0000_0000, 0x3FC0_0000),
        (Conversion::F32ToF64, 0x3FC0_0000, 0x3FF8_0000_0000_0000),
        (Conversion::F64ToF16, 0x3FF8_0000_0000_0000, 0x3E00),
        (Conversion::F16ToF64, 0x3E00, 0x3FF8_0000_0000_0000),
        (Conversion::F32ToF16, 0x3FC0_0000, 0x3E00),
        (Conversion::F16ToF32, 0x3E00, 0x3FC0_0000),
    ]
    .into_iter()
    .enumerate()
    {
        let case = ScalarConvertMemoryCase {
            conversion,
            merge: [1, 17, 30][ordinal % 3],
            ll: (ordinal % 3) as u8,
            control: MaskControl::ALL[ordinal % 3],
        };
        let initial = initial_registers(case, ordinal, true);
        let merge = initial.zmm[usize::from(case.merge)];
        let actual = interpreter_success(&lift_case(case), &initial, source, case);
        let mask = element_mask(conversion.to());
        assert_eq!(
            actual.zmm[usize::from(case.destination())][0] & mask,
            expected_low,
            "{case:?}"
        );
        assert_eq!(
            actual.zmm[usize::from(case.destination())][0] & !mask,
            merge[0] & !mask,
            "{case:?}: upper bits within word 0"
        );
        assert_eq!(actual.zmm[usize::from(case.destination())][1], merge[1]);
        assert_eq!(actual.zmm[usize::from(case.destination())][2..], [0; 6]);
    }
}

#[test]
fn narrowing_memory_conversions_obey_all_four_dynamic_rounding_modes() {
    let cases = [
        (
            Conversion::F64ToF32,
            (1.0f64 + 2.0f64.powi(-24)).to_bits(),
            [0x3F80_0000, 0x3F80_0000, 0x3F80_0001, 0x3F80_0000],
        ),
        (
            Conversion::F64ToF16,
            (1.0f64 + 2.0f64.powi(-11)).to_bits(),
            [0x3C00, 0x3C00, 0x3C01, 0x3C00],
        ),
        (
            Conversion::F32ToF16,
            u64::from((1.0f32 + 2.0f32.powi(-11)).to_bits()),
            [0x3C00, 0x3C00, 0x3C01, 0x3C00],
        ),
    ];
    let mut checks = 0usize;
    for (conversion, source, expected) in cases {
        for (round, expected_low) in expected.into_iter().enumerate() {
            let case = ScalarConvertMemoryCase {
                conversion,
                merge: 17,
                ll: (round % 3) as u8,
                control: MaskControl::Merge,
            };
            let mut initial = initial_registers(case, round, true);
            initial.mxcsr = (0x1F80 & !(3 << 13)) | ((round as u32) << 13);
            let actual = interpreter_success(&lift_case(case), &initial, source, case);
            assert_eq!(
                actual.zmm[usize::from(case.destination())][0] & element_mask(conversion.to()),
                expected_low,
                "{case:?} round={round}"
            );
            assert_ne!(actual.mxcsr & (1 << 5), 0, "{case:?} round={round}");
            checks += 1;
        }
    }
    assert_eq!(checks, 3 * 4);
}

fn special_values(element: VecElementType) -> [u64; 10] {
    match element {
        VecElementType::F16 => [
            0x0000, 0x8000, 0x0001, 0x03FF, 0x0400, 0x7BFF, 0x7C00, 0xFC00, 0x7E11, 0x7C22,
        ],
        VecElementType::F32 => [
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
        VecElementType::F64 => [
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
        _ => unreachable!(),
    }
}

#[test]
fn special_values_rounding_daz_and_ftz_preserve_raw_o0_o1_o2_equivalence() {
    let mxcsr_modes = [0x1F80, 0x3FC0, 0x5F80 | 0x8000, 0x7FC0 | 0x8000];
    let mut comparisons = 0usize;
    for conversion in Conversion::ALL {
        for (value_index, source) in special_values(conversion.from()).into_iter().enumerate() {
            for (mode_index, mxcsr) in mxcsr_modes.into_iter().enumerate() {
                let case = ScalarConvertMemoryCase {
                    conversion,
                    merge: [1, 17, 30][value_index % 3],
                    ll: (mode_index % 3) as u8,
                    control: MaskControl::ALL[(value_index + mode_index) % 3],
                };
                let mut initial = initial_registers(case, value_index, true);
                initial.mxcsr = mxcsr;
                let expected = interpreter_success(&lift_case(case), &initial, source, case);
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
    }
    assert_eq!(comparisons, 6 * 10 * 4 * LEVELS.len());
}

#[test]
fn type_e3_mask_bit_zero_suppresses_access_and_all_source_faults_are_noncommitting() {
    let mut suppressions = 0usize;
    let mut active_faults = 0usize;
    for conversion in Conversion::ALL {
        for control in MaskControl::ALL {
            let case = ScalarConvertMemoryCase {
                conversion,
                merge: 17,
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
                    assert_eq!(interpreter_registers(&context, &inactive), expected);
                    suppressions += 1;
                }

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
                    "{level:?} {case:?}: source fault committed architectural state"
                );
                active_faults += 1;
            }
        }
    }
    assert_eq!(suppressions, 6 * 2 * LEVELS.len());
    assert_eq!(active_faults, 6 * 3 * LEVELS.len());
}

#[test]
fn unmasked_overflow_exception_preserves_destination_and_records_status() {
    let case = ScalarConvertMemoryCase {
        conversion: Conversion::F32ToF16,
        merge: 17,
        ll: 0,
        control: MaskControl::Merge,
    };
    for level in LEVELS {
        let function = optimize(lift_case(case), level);
        let mut initial = initial_registers(case, 0x55, true);
        initial.mxcsr = 0x1F80 & !(1 << 10); // unmask overflow
        let mut context = interpreter_context(&initial);
        let mut memory = FlatMemory::new(0x3000);
        memory.load(MEMORY_ADDRESS as usize, &f32::MAX.to_bits().to_le_bytes());
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
        ));
        let actual = interpreter_registers(&context, &initial);
        assert_eq!(
            actual.zmm[usize::from(case.destination())],
            initial.zmm[usize::from(case.destination())],
            "{level:?}: trapped conversion committed destination"
        );
        assert_ne!(actual.mxcsr & (1 << 3), 0, "{level:?}: overflow status");
        assert_eq!(actual.rflags, initial.rflags, "{level:?}");
    }
}
