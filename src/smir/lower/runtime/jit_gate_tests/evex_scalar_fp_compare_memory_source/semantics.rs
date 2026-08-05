use super::*;

#[test]
fn all_54_scanner_cells_match_manual_finite_semantics_at_o0_o1_o2() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
    let relations = [Relation::Less, Relation::Equal, Relation::Greater];
    let mut active_comparisons = 0usize;
    let mut inactive_comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let relation = relations[ordinal % relations.len()];
        let (source1, memory) = finite_values(case.format, relation);
        let mut initial = initial_registers(case, ordinal, true);
        set_source1(&mut initial, case, source1);
        let expected = manual_result(&initial, case, relation, true);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, memory, case);
            assert_eq!(actual, expected, "{level:?} {case:?} {relation:?}");
            active_comparisons += 1;
        }

        if case.control == MaskControl::Masked {
            let mut inactive = initial_registers(case, ordinal ^ 0x55, false);
            set_source1(&mut inactive, case, source1);
            let expected = manual_result(&inactive, case, relation, false);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let actual = interpreter_success(&function, &inactive, memory, case);
                assert_eq!(
                    actual, expected,
                    "{level:?} {case:?} {relation:?}: inactive"
                );
                inactive_comparisons += 1;
            }
        }
    }
    assert_eq!(active_comparisons, 54 * LEVELS.len());
    assert_eq!(inactive_comparisons, 27 * LEVELS.len());
}

#[test]
fn every_five_bit_predicate_matches_the_intel_finite_truth_table() {
    let relations = [Relation::Less, Relation::Equal, Relation::Greater];
    let mut comparisons = 0usize;
    for format in ScalarFormat::ALL {
        for predicate in 0..32u8 {
            for (relation_index, relation) in relations.into_iter().enumerate() {
                let case = ScalarCompareMemoryCase {
                    format,
                    source1: [1, 17, 30][relation_index],
                    ll: (predicate % 3) as u8,
                    control: MaskControl::None,
                    predicate,
                };
                let (source1, memory) = finite_values(format, relation);
                let mut initial = initial_registers(case, predicate as usize, true);
                set_source1(&mut initial, case, source1);
                let expected = manual_result(&initial, case, relation, true);
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let actual = interpreter_success(&function, &initial, memory, case);
                    assert_eq!(
                        actual, expected,
                        "{level:?} {format:?} predicate={predicate} {relation:?}"
                    );
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 32 * 3 * LEVELS.len());
}

fn qnan(format: ScalarFormat) -> u64 {
    match format {
        ScalarFormat::F16 => 0x7E11,
        ScalarFormat::F32 => 0x7FC0_0011,
        ScalarFormat::F64 => 0x7FF8_0000_0000_0011,
    }
}

fn snan(format: ScalarFormat) -> u64 {
    match format {
        ScalarFormat::F16 => 0x7C11,
        ScalarFormat::F32 => 0x7F80_0011,
        ScalarFormat::F64 => 0x7FF0_0000_0000_0011,
    }
}

fn one(format: ScalarFormat) -> u64 {
    finite_values(format, Relation::Less).0
}

fn predicate_signals_qnan(predicate: u8) -> bool {
    matches!(
        predicate & 0x1F,
        1 | 2 | 5 | 6 | 9 | 10 | 13 | 14 | 16 | 19 | 20 | 23 | 24 | 27 | 28 | 31
    )
}

#[test]
fn quiet_and_signaling_nan_results_and_mxcsr_invalid_follow_intel_predicates() {
    let mut comparisons = 0usize;
    for format in ScalarFormat::ALL {
        for predicate in 0..32u8 {
            for (is_signaling_nan, source1) in [(false, qnan(format)), (true, snan(format))] {
                let case = ScalarCompareMemoryCase {
                    format,
                    source1: 17,
                    ll: (predicate % 3) as u8,
                    control: MaskControl::None,
                    predicate,
                };
                let mut initial = initial_registers(case, predicate as usize, true);
                set_source1(&mut initial, case, source1);
                let mut expected = manual_result(&initial, case, Relation::Unordered, true);
                if is_signaling_nan || predicate_signals_qnan(predicate) {
                    expected.mxcsr |= 1;
                }
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let actual = interpreter_success(&function, &initial, one(format), case);
                    assert_eq!(
                        actual, expected,
                        "{level:?} {format:?} predicate={predicate} sNaN={is_signaling_nan}"
                    );
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 32 * 2 * LEVELS.len());
}

fn special_pairs(format: ScalarFormat) -> [(u64, u64, Relation); 7] {
    match format {
        ScalarFormat::F16 => [
            (0x0000, 0x8000, Relation::Equal),
            (0x0001, 0x0000, Relation::Greater),
            (0x03FF, 0x0400, Relation::Less),
            (0x7C00, 0x7BFF, Relation::Greater),
            (0xFC00, 0xFBFF, Relation::Less),
            (0x7E11, 0x3C00, Relation::Unordered),
            (0x7C11, 0x3C00, Relation::Unordered),
        ],
        ScalarFormat::F32 => [
            (0x0000_0000, 0x8000_0000, Relation::Equal),
            (0x0000_0001, 0x0000_0000, Relation::Greater),
            (0x007F_FFFF, 0x0080_0000, Relation::Less),
            (0x7F80_0000, 0x7F7F_FFFF, Relation::Greater),
            (0xFF80_0000, 0xFF7F_FFFF, Relation::Less),
            (0x7FC0_0011, 0x3F80_0000, Relation::Unordered),
            (0x7F80_0011, 0x3F80_0000, Relation::Unordered),
        ],
        ScalarFormat::F64 => [
            (
                0x0000_0000_0000_0000,
                0x8000_0000_0000_0000,
                Relation::Equal,
            ),
            (
                0x0000_0000_0000_0001,
                0x0000_0000_0000_0000,
                Relation::Greater,
            ),
            (0x000F_FFFF_FFFF_FFFF, 0x0010_0000_0000_0000, Relation::Less),
            (
                0x7FF0_0000_0000_0000,
                0x7FEF_FFFF_FFFF_FFFF,
                Relation::Greater,
            ),
            (0xFFF0_0000_0000_0000, 0xFFEF_FFFF_FFFF_FFFF, Relation::Less),
            (
                0x7FF8_0000_0000_0011,
                0x3FF0_0000_0000_0000,
                Relation::Unordered,
            ),
            (
                0x7FF0_0000_0000_0011,
                0x3FF0_0000_0000_0000,
                Relation::Unordered,
            ),
        ],
    }
}

#[test]
fn zeros_subnormals_normals_infinities_and_nans_are_optimizer_invariant() {
    // Comparison has no rounding result, but MXCSR.DAZ changes binary32/64
    // subnormal classification. Verify raw architectural parity with DAZ both
    // clear and set while independently checking the Intel predicate truth.
    let mut comparisons = 0usize;
    for format in ScalarFormat::ALL {
        for (pair_index, (source1, memory, relation)) in
            special_pairs(format).into_iter().enumerate()
        {
            for (mode_index, mxcsr) in [0x1F80u32, 0x1FC0].into_iter().enumerate() {
                let predicate = ((pair_index * 7 + mode_index * 13) & 0x1F) as u8;
                let case = ScalarCompareMemoryCase {
                    format,
                    source1: [1, 17, 30][pair_index % 3],
                    ll: (pair_index % 3) as u8,
                    control: MaskControl::ALL[mode_index],
                    predicate,
                };
                let mut initial = initial_registers(case, pair_index, true);
                initial.mxcsr = mxcsr;
                set_source1(&mut initial, case, source1);
                let baseline = interpreter_success(&lift_case(case), &initial, memory, case);
                let effective_relation =
                    if mode_index == 1 && format != ScalarFormat::F16 && pair_index == 1 {
                        Relation::Equal
                    } else {
                        relation
                    };
                assert_eq!(
                    baseline.k[usize::from(case.destination())],
                    u64::from(predicate_result(predicate, effective_relation)),
                    "{format:?} predicate={predicate} {effective_relation:?} mxcsr={mxcsr:#06X}"
                );
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let actual = interpreter_success(&function, &initial, memory, case);
                    assert_eq!(
                        actual, baseline,
                        "{level:?} {format:?} predicate={predicate} pair={pair_index} mxcsr={mxcsr:#06X}"
                    );
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 7 * 2 * LEVELS.len());
}

#[test]
fn type_e3_mask_bit_zero_suppresses_access_and_all_faults_are_noncommitting() {
    let mut suppressions = 0usize;
    let mut masked_faults = 0usize;
    let mut unmasked_faults = 0usize;
    for format in ScalarFormat::ALL {
        for control in MaskControl::ALL {
            let case = ScalarCompareMemoryCase {
                format,
                source1: 17,
                ll: 2,
                control,
                predicate: 19,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                if control == MaskControl::Masked {
                    let mut inactive = initial_registers(case, 0x10, false);
                    set_source1(&mut inactive, case, qnan(format));
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
                        manual_result(&inactive, case, Relation::Unordered, false),
                        "{level:?} {case:?}: inactive source access was not suppressed"
                    );
                    suppressions += 1;

                    let mut active = initial_registers(case, 0x20, true);
                    set_source1(&mut active, case, qnan(format));
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
                        "{level:?} {case:?}: active fault committed K or MXCSR"
                    );
                    masked_faults += 1;
                } else {
                    let mut initial = initial_registers(case, 0x30, true);
                    set_source1(&mut initial, case, qnan(format));
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
                        "{level:?} {case:?}: unmasked fault committed K or MXCSR"
                    );
                    unmasked_faults += 1;
                }
            }
        }
    }
    assert_eq!(suppressions, 3 * LEVELS.len());
    assert_eq!(masked_faults, suppressions);
    assert_eq!(unmasked_faults, 3 * LEVELS.len());
}
