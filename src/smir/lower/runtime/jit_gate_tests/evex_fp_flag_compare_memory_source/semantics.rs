use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relation {
    Less,
    Equal,
    Greater,
    Unordered,
}

fn finite_values(format: Format, relation: Relation) -> (u64, u64) {
    let (one, two, three) = match format {
        Format::F16 => (0x3C00, 0x4000, 0x4200),
        Format::F32 => (0x3F80_0000, 0x4000_0000, 0x4040_0000),
        Format::F64 => (
            0x3FF0_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x4008_0000_0000_0000,
        ),
    };
    match relation {
        Relation::Less => (one, two),
        Relation::Equal => (two, two),
        Relation::Greater => (three, two),
        Relation::Unordered => unreachable!("finite unordered comparison"),
    }
}

fn relation_flags(relation: Relation) -> u64 {
    match relation {
        Relation::Less => 1,
        Relation::Equal => 1 << 6,
        Relation::Greater => 0,
        Relation::Unordered => (1 << 6) | (1 << 2) | 1,
    }
}

fn expected_success(mut initial: GuestRegs, relation: Relation, status: u32) -> GuestRegs {
    initial.rflags = (initial.rflags & !STATUS_FLAGS) | relation_flags(relation);
    initial.mxcsr |= status;
    initial
}

fn qnan(format: Format) -> u64 {
    match format {
        Format::F16 => 0x7E11,
        Format::F32 => 0x7FC0_0011,
        Format::F64 => 0x7FF8_0000_0000_0011,
    }
}

fn snan(format: Format) -> u64 {
    match format {
        Format::F16 => 0x7C11,
        Format::F32 => 0x7F80_0011,
        Format::F64 => 0x7FF0_0000_0000_0011,
    }
}

fn positive_subnormal(format: Format) -> u64 {
    1 & format.bit_mask()
}

fn positive_zero(_format: Format) -> u64 {
    0
}

fn negative_zero(format: Format) -> u64 {
    format.sign_mask()
}

fn positive_infinity(format: Format) -> u64 {
    match format {
        Format::F16 => 0x7C00,
        Format::F32 => 0x7F80_0000,
        Format::F64 => 0x7FF0_0000_0000_0000,
    }
}

fn maximum_finite(format: Format) -> u64 {
    match format {
        Format::F16 => 0x7BFF,
        Format::F32 => 0x7F7F_FFFF,
        Format::F64 => 0x7FEF_FFFF_FFFF_FFFF,
    }
}

#[test]
fn all_162_finite_relation_encoding_and_optimization_cells_match_intel_flags() {
    let mut comparisons = 0usize;
    for format in Format::ALL {
        for signaling in [false, true] {
            for ll in 0..=2 {
                for (ordinal, relation) in [Relation::Less, Relation::Equal, Relation::Greater]
                    .into_iter()
                    .enumerate()
                {
                    let case = Case {
                        format,
                        signaling,
                        source1: [1, 17, 30][ordinal],
                        ll,
                    };
                    let (first, second) = finite_values(format, relation);
                    let mut initial = initial_registers(case, comparisons);
                    set_source1(&mut initial, case, first);
                    let expected = expected_success(initial, relation, 0);
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let actual = interpreter_success(&function, &initial, second, case);
                        assert_eq!(actual, expected, "{level:?} {case:?} {relation:?}");
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 2 * 3 * 3 * LEVELS.len());
}

#[test]
fn nan_zero_subnormal_and_infinity_semantics_are_optimizer_invariant() {
    let mut comparisons = 0usize;
    for format in Format::ALL {
        for signaling in [false, true] {
            for daz in [false, true] {
                let mxcsr = 0x1F80 | (u32::from(daz) << 6);
                let cases = [
                    (
                        qnan(format),
                        finite_values(format, Relation::Equal).0,
                        Relation::Unordered,
                        u32::from(signaling),
                    ),
                    (
                        snan(format),
                        finite_values(format, Relation::Equal).0,
                        Relation::Unordered,
                        1,
                    ),
                    (
                        positive_zero(format),
                        negative_zero(format),
                        Relation::Equal,
                        0,
                    ),
                    (
                        positive_subnormal(format),
                        positive_zero(format),
                        if daz && format != Format::F16 {
                            Relation::Equal
                        } else {
                            Relation::Greater
                        },
                        if daz && format != Format::F16 {
                            0
                        } else {
                            1 << 1
                        },
                    ),
                    (
                        positive_infinity(format),
                        maximum_finite(format),
                        Relation::Greater,
                        0,
                    ),
                ];
                for (index, (first, second, relation, status)) in cases.into_iter().enumerate() {
                    let case = Case {
                        format,
                        signaling,
                        source1: [1, 17, 30][index % 3],
                        ll: (index % 3) as u8,
                    };
                    let mut initial = initial_registers(case, comparisons ^ index);
                    initial.mxcsr = mxcsr;
                    set_source1(&mut initial, case, first);
                    let expected = expected_success(initial, relation, status);
                    for level in LEVELS {
                        let function = optimize(lift_case(case), level);
                        let actual = interpreter_success(&function, &initial, second, case);
                        assert_eq!(
                            actual, expected,
                            "{level:?} {case:?} index={index} daz={daz}"
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 3 * 2 * 2 * 5 * LEVELS.len());
}

fn execute_with_source(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: u64,
    case: Case,
) -> (BlockResult, GuestRegs) {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x3000);
    memory.load(
        MEMORY_ADDRESS as usize,
        &source.to_le_bytes()[..case.format.memory_size()],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    let registers = interpreter_registers(&mut context, initial);
    (result, registers)
}

#[test]
fn unmasked_invalid_and_denormal_exceptions_leave_flags_noncommitting() {
    let mut traps = 0usize;
    let mut quiet_successes = 0usize;
    for format in Format::ALL {
        for signaling in [false, true] {
            for (kind, first, status) in [
                ("qNaN", qnan(format), 1u32),
                ("sNaN", snan(format), 1u32),
                ("denormal", positive_subnormal(format), 1u32 << 1),
            ] {
                let case = Case {
                    format,
                    signaling,
                    source1: 17,
                    ll: 2,
                };
                let second = finite_values(format, Relation::Equal).0;
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_registers(case, traps + quiet_successes);
                    initial.mxcsr = if kind == "denormal" {
                        0x1F80 & !(1 << 8)
                    } else {
                        0x1F80 & !(1 << 7)
                    };
                    set_source1(&mut initial, case, first);
                    let (result, actual) = execute_with_source(&function, &initial, second, case);
                    let should_trap = kind != "qNaN" || signaling;
                    if should_trap {
                        assert!(
                            matches!(
                                result,
                                BlockResult::Exit(ExitReason::SimdFloatingPoint { addr: PC })
                            ),
                            "{level:?} {case:?} {kind}: {result:?}"
                        );
                        let mut expected = initial;
                        expected.mxcsr |= status;
                        assert_eq!(actual, expected, "{level:?} {case:?} {kind}");
                        traps += 1;
                    } else {
                        assert!(matches!(
                            result,
                            BlockResult::Exit(ExitReason::Return { .. })
                        ));
                        assert_eq!(
                            actual,
                            expected_success(initial, Relation::Unordered, 0),
                            "{level:?} {case:?} quiet qNaN"
                        );
                        quiet_successes += 1;
                    }
                }
            }
        }
    }
    assert_eq!(traps, (3 * 2 * 3 - 3) * LEVELS.len());
    assert_eq!(quiet_successes, 3 * LEVELS.len());
}

#[test]
fn all_54_memory_fault_cells_are_precise_and_commit_no_architectural_state() {
    let mut faults = 0usize;
    for format in Format::ALL {
        for signaling in [false, true] {
            for ll in 0..=2 {
                let case = Case {
                    format,
                    signaling,
                    source1: 17,
                    ll,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let mut initial = initial_registers(case, faults);
                    // If reached, this operand would mutate MXCSR and flags.
                    initial.mxcsr &= !(1 << 7);
                    set_source1(&mut initial, case, snan(format));
                    let mut context = interpreter_context(&initial);
                    let mut inaccessible = FlatMemory::new(MEMORY_ADDRESS as usize);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut inaccessible,
                        &function.blocks[0],
                    );
                    assert!(
                        matches!(
                            result,
                            BlockResult::Exit(ExitReason::MemoryFault {
                                addr: MEMORY_ADDRESS,
                                write: false,
                            })
                        ),
                        "{level:?} {case:?}: {result:?}"
                    );
                    assert_eq!(
                        interpreter_registers(&mut context, &initial),
                        initial,
                        "{level:?} {case:?}: fault committed state"
                    );
                    faults += 1;
                }
            }
        }
    }
    assert_eq!(faults, 3 * 2 * 3 * LEVELS.len());
}

#[test]
fn memory_bytes_above_the_type_e3nf_footprint_are_unobservable() {
    for format in Format::ALL {
        let case = Case {
            format,
            signaling: true,
            source1: 17,
            ll: 1,
        };
        let (first, second) = finite_values(format, Relation::Less);
        let mut initial = initial_registers(case, format.memory_size());
        set_source1(&mut initial, case, first);
        let function = lift_case(case);
        let baseline = interpreter_success(&function, &initial, second, case);

        let mut context = interpreter_context(&initial);
        let mut memory = FlatMemory::new(0x3000);
        memory.load(MEMORY_ADDRESS as usize, &second.to_le_bytes());
        memory.load(
            MEMORY_ADDRESS as usize + format.memory_size(),
            &[0xFF; 8][..8 - format.memory_size()],
        );
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Return { .. })
        ));
        assert_eq!(interpreter_registers(&mut context, &initial), baseline);
    }
}
