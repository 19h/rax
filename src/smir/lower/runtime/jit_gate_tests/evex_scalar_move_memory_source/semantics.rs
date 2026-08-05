use super::*;

#[test]
fn all_45_cells_have_o0_o1_o2_bit_exact_load_store_and_mask_semantics() {
    let cases = all_cases();
    assert_eq!(cases.len(), 45);
    let mut active_executions = 0usize;
    let mut inactive_executions = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let memory_before = (0xE7D6_C5B4_A392_8170u64
            ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081))
        .to_le_bytes();
        let active_initial = initial_registers(case, ordinal, true);
        let active_expected =
            independent_success_oracle(&active_initial, memory_before, case, true);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let active = interpreter_success(&function, &active_initial, memory_before);
            assert_eq!(active, active_expected, "{level:?} {case:?}: active");
            active_executions += 1;
        }

        if case.control != MaskControl::None {
            let inactive_initial = initial_registers(case, ordinal ^ 0x55, false);
            let inactive_expected =
                independent_success_oracle(&inactive_initial, memory_before, case, false);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let inactive = interpreter_success(&function, &inactive_initial, memory_before);
                assert_eq!(inactive, inactive_expected, "{level:?} {case:?}: inactive");
                inactive_executions += 1;
            }
        }
    }
    assert_eq!(active_executions, 45 * LEVELS.len());
    assert_eq!(inactive_executions, 27 * LEVELS.len());
}

#[test]
fn every_k1_to_k7_controls_only_bit_zero_for_all_formats_directions_and_llig_images() {
    let mut executions = 0usize;
    for format in ScalarFormat::ALL {
        for direction in Direction::ALL {
            for ll in 0..=2 {
                for control in [MaskControl::Merge, MaskControl::Zero] {
                    if !control.valid_for(direction) {
                        continue;
                    }
                    for mask in 1..=7u8 {
                        let case = ScalarMoveCase {
                            format,
                            direction,
                            vector: 16 + ((mask + ll) & 7),
                            ll,
                            control,
                        };
                        let bytes = memory_encoding(
                            format,
                            direction,
                            case.vector,
                            ll,
                            mask,
                            control == MaskControl::Zero,
                            2,
                        );
                        let function = optimize(function_from_bytes(&bytes, case), OptLevel::O2);
                        let exact = sequence(&function).expect("all-mask scalar move sequence");
                        assert_eq!(exact.encoding.writemask, Some(mask));
                        let memory_before = 0x1122_3344_5566_7788u64
                            .rotate_left(u32::from(mask * 5 + ll))
                            .to_le_bytes();
                        for active in [false, true] {
                            let mut initial = initial_registers(case, usize::from(mask), true);
                            initial.k[usize::from(mask)] =
                                (0xFEDC_BA98_7654_3210 & !1) | u64::from(active);
                            let expected =
                                independent_success_oracle(&initial, memory_before, case, active);
                            let actual = interpreter_success(&function, &initial, memory_before);
                            assert_eq!(
                                actual, expected,
                                "{format:?} {direction:?} ll={ll} k{mask} {control:?} active={active}"
                            );
                            executions += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(executions, 3 * 3 * (2 + 1) * 7 * 2);
}

#[test]
fn inactive_accesses_are_suppressed_and_active_boundary_faults_are_noncommitting() {
    let mut suppressions = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in all_cases().into_iter().enumerate() {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let active = initial_registers(case, ordinal, true);
            let mut active_context = interpreter_context(&active);
            let size = MEMORY_ADDRESS as usize + case.format.memory_size() - 1;
            let mut boundary = FlatMemory::new(size);
            let result = SmirInterpreter::new().execute_block(
                &mut active_context,
                &mut boundary,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault {
                    write,
                    ..
                }) if write == (case.direction == Direction::Store)
            ));
            assert_eq!(
                interpreter_registers(&active_context, &active),
                active,
                "{level:?} {case:?}: boundary fault committed state"
            );
            faults += 1;

            if case.control != MaskControl::None {
                let inactive = initial_registers(case, ordinal ^ 0xAA, false);
                let mut inactive_context = interpreter_context(&inactive);
                let mut inaccessible = FlatMemory::new(MEMORY_ADDRESS as usize);
                let result = SmirInterpreter::new().execute_block(
                    &mut inactive_context,
                    &mut inaccessible,
                    &function.blocks[0],
                );
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::Return { .. })
                ));
                let expected = independent_success_oracle(&inactive, [0; 8], case, false).0;
                assert_eq!(
                    interpreter_registers(&inactive_context, &inactive),
                    expected,
                    "{level:?} {case:?}: inactive access was not suppressed"
                );
                suppressions += 1;
            }
        }
    }
    assert_eq!(faults, 45 * LEVELS.len());
    assert_eq!(suppressions, 27 * LEVELS.len());
}
