use super::*;

fn scalar_register_success(function: &SmirFunction, initial: &GuestRegs) -> GuestRegs {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));
    interpreter_registers(&context, initial)
}

#[test]
fn scalar_fp16_complex_memory_matches_register_source_for_all_controls_llig_and_fp_classes() {
    let mut cases = 0usize;
    let mut comparisons = 0usize;
    for operation in ComplexOperation::ALL {
        for source1 in [1, 15] {
            for ll in 0..=3 {
                for control in MaskControl::ALL {
                    let case = ScalarComplexMemoryCase {
                        operation,
                        source1,
                        ll,
                        control,
                    };
                    let register_bytes = scalar_register_encoding(
                        operation,
                        case.destination(),
                        source1,
                        2,
                        ll,
                        case.mask(),
                        case.zeroing(),
                    );
                    let register_function = lift_scalar_bytes(&register_bytes);
                    for (value_index, pair) in PAIR_CORPUS.into_iter().enumerate() {
                        let ordinal = cases * PAIR_CORPUS.len() + value_index;
                        let mut initial = scalar_initial_registers(case, ordinal);
                        initial.zmm[2][0] =
                            (initial.zmm[2][0] & 0xFFFF_FFFF_0000_0000) | u64::from(pair);
                        let value = [u64::from(pair), 0, 0, 0, 0, 0, 0, 0];
                        let expected = scalar_register_success(&register_function, &initial);
                        let unoptimized = scalar_interpreter_success(
                            &lift_scalar_case(case),
                            &initial,
                            value,
                            case,
                        );
                        assert_eq!(
                            unoptimized, expected,
                            "register/memory mismatch {case:?} pair={pair:#010X}"
                        );

                        let destination = case.destination() as usize;
                        let source1 = case.source1 as usize;
                        assert_eq!(
                            expected.zmm[destination][0] >> 32,
                            initial.zmm[source1][0] >> 32,
                            "scalar bits 63:32 {case:?} pair={pair:#010X}"
                        );
                        assert_eq!(
                            expected.zmm[destination][1], initial.zmm[source1][1],
                            "scalar bits 127:64 {case:?} pair={pair:#010X}"
                        );
                        assert_eq!(
                            &expected.zmm[destination][2..],
                            &[0; 6],
                            "scalar zero-upper {case:?} pair={pair:#010X}"
                        );

                        for level in LEVELS {
                            let function = optimize(lift_scalar_case(case), level);
                            let actual =
                                scalar_interpreter_success(&function, &initial, value, case);
                            assert_eq!(actual, expected, "{level:?} {case:?} pair={pair:#010X}");
                            comparisons += 1;
                        }
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 4 * 2 * 4 * 3);
    assert_eq!(comparisons, cases * PAIR_CORPUS.len() * LEVELS.len());
}

#[test]
fn type_e10_bit_zero_masks_suppress_faults_and_active_faults_do_not_commit() {
    let mut suppressions = 0usize;
    let mut masked_faults = 0usize;
    let mut unmasked_faults = 0usize;
    for operation in ComplexOperation::ALL {
        for ll in 0..=3 {
            for control in [MaskControl::Merge, MaskControl::Zero] {
                let case = ScalarComplexMemoryCase {
                    operation,
                    source1: 17,
                    ll,
                    control,
                };
                for level in LEVELS {
                    let function = optimize(lift_scalar_case(case), level);
                    let value =
                        [u64::from(PAIR_CORPUS[(usize::from(ll) + 1) % PAIR_CORPUS.len()]); 8];

                    // Only k1[0] participates. A set high bit with bit 0 clear
                    // suppresses the complete 4-byte access.
                    let mut inactive = scalar_initial_registers(case, suppressions);
                    inactive.k[usize::from(case.mask())] = 1 << 42;
                    let expected = scalar_interpreter_success(&function, &inactive, value, case);
                    let mut context = interpreter_context(&inactive);
                    let mut inaccessible = FlatMemory::new(0x2000);
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
                        "suppressed {level:?} {case:?}"
                    );
                    suppressions += 1;

                    let mut active = scalar_initial_registers(case, masked_faults ^ 0x55);
                    active.k[usize::from(case.mask())] = (1 << 42) | 1;
                    let mut context = interpreter_context(&active);
                    let mut inaccessible = FlatMemory::new(0x2000);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut inaccessible,
                        &function.blocks[0],
                    );
                    assert!(matches!(
                        result,
                        BlockResult::Exit(ExitReason::MemoryFault {
                            addr: 0x2000,
                            write: false,
                        })
                    ));
                    assert_eq!(
                        interpreter_registers(&context, &active),
                        active,
                        "active fault committed state {level:?} {case:?}"
                    );
                    masked_faults += 1;
                }
            }

            let case = ScalarComplexMemoryCase {
                operation,
                source1: 17,
                ll,
                control: MaskControl::None,
            };
            for level in LEVELS {
                let function = optimize(lift_scalar_case(case), level);
                let initial = scalar_initial_registers(case, unmasked_faults ^ 0xAA);
                let mut context = interpreter_context(&initial);
                let mut inaccessible = FlatMemory::new(0x2000);
                let result = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut inaccessible,
                    &function.blocks[0],
                );
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr: 0x2000,
                        write: false,
                    })
                ));
                assert_eq!(
                    interpreter_registers(&context, &initial),
                    initial,
                    "unmasked fault committed state {level:?} {case:?}"
                );
                unmasked_faults += 1;
            }
        }
    }
    assert_eq!(suppressions, 4 * 4 * 2 * LEVELS.len());
    assert_eq!(masked_faults, suppressions);
    assert_eq!(unmasked_faults, 4 * 4 * LEVELS.len());
}
