use super::*;

fn source_patterns(case: ScalarIntMemoryCase) -> Vec<u64> {
    let mask = if case.w { u64::MAX } else { u32::MAX as u64 };
    let sign_bit = if case.w { 1u64 << 63 } else { 1u64 << 31 };
    let mut patterns = vec![0, 1, 2, mask, sign_bit - 1, sign_bit];
    let boundary = 1u64 << case.format.precision();
    if boundary <= mask {
        patterns.extend([boundary - 1, boundary, boundary + 1, boundary + 3]);
        if case.signed {
            patterns.extend([
                mask.wrapping_sub(boundary).wrapping_add(1),
                mask.wrapping_sub(boundary),
            ]);
        }
    }
    if case.format == DestinationFormat::F16 {
        patterns.extend([2047, 2048, 2049, 65_503, 65_504, 65_519, 65_520, 70_000]);
        if case.signed {
            patterns.extend([
                mask.wrapping_sub(2049).wrapping_add(1),
                mask.wrapping_sub(70_000).wrapping_add(1),
            ]);
        }
    }
    patterns.iter_mut().for_each(|value| *value &= mask);
    patterns.sort_unstable();
    patterns.dedup();
    patterns
}

fn expected_destination_layout(
    case: ScalarIntMemoryCase,
    initial: &GuestRegs,
    low: u64,
) -> [u64; 8] {
    let mut expected = initial.zmm[usize::from(case.merge)];
    let mask = case.format.element_mask();
    expected[0] = (expected[0] & !mask) | (low & mask);
    expected[2..].fill(0);
    expected
}

#[test]
fn all_108_scanner_cells_cover_integer_boundaries_and_o0_o1_o2_raw_equivalence() {
    let cases = scanner_cases();
    assert_eq!(cases.len(), 108);
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let baseline = optimize(lift_case(case), OptLevel::O0);
        for (sample, source) in source_patterns(case).into_iter().enumerate() {
            let round = (ordinal + sample) & 3;
            let mxcsr = (0x1F80 & !(3 << 13)) | ((round as u32) << 13);
            let initial = initial_registers(case, ordinal ^ sample, mxcsr);
            let expected = interpreter_success(&baseline, &initial, source, case);
            assert_eq!(expected.gpr, initial.gpr, "{case:?} source={source:#018X}");
            assert_eq!(expected.rflags, initial.rflags, "{case:?}");
            assert_eq!(expected.k, initial.k, "{case:?}");
            let destination = usize::from(case.destination);
            let low = expected.zmm[destination][0] & case.format.element_mask();
            assert_eq!(
                expected.zmm[destination],
                expected_destination_layout(case, &initial, low),
                "{case:?} source={source:#018X}: scalar merge/zero-upper layout"
            );
            for index in 0..32usize {
                if index != destination {
                    assert_eq!(
                        expected.zmm[index], initial.zmm[index],
                        "{case:?}: ZMM{index}"
                    );
                }
            }

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
    assert!(comparisons >= 108 * 8 * LEVELS.len());
}

#[test]
fn exact_integer_cases_match_independent_binary16_binary32_binary64_bit_oracles() {
    for (ordinal, (format, signed, w, source, expected_low)) in [
        (DestinationFormat::F32, false, false, 1, 0x3F80_0000),
        (
            DestinationFormat::F32,
            true,
            false,
            u32::MAX as u64,
            0xBF80_0000,
        ),
        (
            DestinationFormat::F64,
            false,
            true,
            1,
            0x3FF0_0000_0000_0000,
        ),
        (
            DestinationFormat::F64,
            true,
            true,
            u64::MAX,
            0xBFF0_0000_0000_0000,
        ),
        (DestinationFormat::F16, false, false, 1, 0x3C00),
        (DestinationFormat::F16, true, true, u64::MAX, 0xBC00),
        (DestinationFormat::F16, false, false, 2048, 0x6800),
    ]
    .into_iter()
    .enumerate()
    {
        let case = ScalarIntMemoryCase {
            format,
            signed,
            w,
            ll: (ordinal % 3) as u8,
            destination: [0, 17, 31][ordinal % 3],
            merge: [1, 30, 16][ordinal % 3],
            base: 2,
        };
        let initial = initial_registers(case, ordinal, 0x1F80);
        let expected_layout = expected_destination_layout(case, &initial, expected_low);
        for level in LEVELS {
            let actual =
                interpreter_success(&optimize(lift_case(case), level), &initial, source, case);
            assert_eq!(
                actual.zmm[usize::from(case.destination)],
                expected_layout,
                "{level:?} {case:?}"
            );
            assert_eq!(actual.mxcsr, initial.mxcsr, "{level:?} {case:?}: exact");
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RoundingCase {
    format: DestinationFormat,
    signed: bool,
    w: bool,
    source: u64,
    expected: [u64; 4],
}

#[test]
fn dynamic_mxcsr_rounding_matches_independent_half_ulp_oracles_for_all_formats_and_signs() {
    let cases = [
        RoundingCase {
            format: DestinationFormat::F32,
            signed: false,
            w: false,
            source: 16_777_217,
            expected: [0x4B80_0000, 0x4B80_0000, 0x4B80_0001, 0x4B80_0000],
        },
        RoundingCase {
            format: DestinationFormat::F32,
            signed: true,
            w: false,
            source: (-16_777_217i32) as u32 as u64,
            expected: [0xCB80_0000, 0xCB80_0001, 0xCB80_0000, 0xCB80_0000],
        },
        RoundingCase {
            format: DestinationFormat::F64,
            signed: false,
            w: true,
            source: 9_007_199_254_740_993,
            expected: [
                0x4340_0000_0000_0000,
                0x4340_0000_0000_0000,
                0x4340_0000_0000_0001,
                0x4340_0000_0000_0000,
            ],
        },
        RoundingCase {
            format: DestinationFormat::F64,
            signed: true,
            w: true,
            source: (-9_007_199_254_740_993i64) as u64,
            expected: [
                0xC340_0000_0000_0000,
                0xC340_0000_0000_0001,
                0xC340_0000_0000_0000,
                0xC340_0000_0000_0000,
            ],
        },
        RoundingCase {
            format: DestinationFormat::F16,
            signed: false,
            w: false,
            source: 2049,
            expected: [0x6800, 0x6800, 0x6801, 0x6800],
        },
        RoundingCase {
            format: DestinationFormat::F16,
            signed: true,
            w: false,
            source: (-2049i32) as u32 as u64,
            expected: [0xE800, 0xE801, 0xE800, 0xE800],
        },
    ];
    let mut checks = 0usize;
    for (ordinal, oracle) in cases.into_iter().enumerate() {
        for (round, expected_low) in oracle.expected.into_iter().enumerate() {
            let case = ScalarIntMemoryCase {
                format: oracle.format,
                signed: oracle.signed,
                w: oracle.w,
                ll: (round % 3) as u8,
                destination: [0, 17, 31][ordinal % 3],
                merge: [1, 30, 16][ordinal % 3],
                base: 2,
            };
            let mxcsr = (0x1F80 & !(3 << 13)) | ((round as u32) << 13);
            let initial = initial_registers(case, ordinal ^ round, mxcsr);
            for level in LEVELS {
                let actual = interpreter_success(
                    &optimize(lift_case(case), level),
                    &initial,
                    oracle.source,
                    case,
                );
                assert_eq!(
                    actual.zmm[usize::from(case.destination)][0] & case.format.element_mask(),
                    expected_low,
                    "{level:?} {case:?} round={round}"
                );
                assert_ne!(
                    actual.mxcsr & (1 << 5),
                    0,
                    "{level:?} {case:?} round={round}: precision status"
                );
                checks += 1;
            }
        }
    }
    assert_eq!(checks, cases.len() * 4 * LEVELS.len());
}

#[test]
fn every_source_width_faults_before_destination_mxcsr_or_other_state_commits() {
    let mut faults = 0usize;
    for format in DestinationFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                let case = ScalarIntMemoryCase {
                    format,
                    signed,
                    w,
                    ll: 2,
                    destination: 17,
                    merge: 30,
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
    assert_eq!(faults, 3 * 2 * 2 * LEVELS.len());
}

#[test]
fn unmasked_binary16_overflow_is_precise_noncommitting_and_records_status() {
    for signed in [false, true] {
        let case = ScalarIntMemoryCase {
            format: DestinationFormat::F16,
            signed,
            w: true,
            ll: 0,
            destination: 17,
            merge: 30,
            base: 2,
        };
        let source = if signed { i64::MAX as u64 } else { u64::MAX };
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let mut initial = initial_registers(case, usize::from(signed), 0x1F80);
            initial.mxcsr &= !(1 << 10); // unmask overflow
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
                "{level:?} {case:?}: {result:?}"
            );
            let actual = interpreter_registers(&context, &initial);
            assert_eq!(
                actual.zmm[usize::from(case.destination)],
                initial.zmm[usize::from(case.destination)],
                "{level:?} {case:?}: trapped conversion committed destination"
            );
            assert_ne!(actual.mxcsr & (1 << 3), 0, "{level:?} {case:?}: overflow");
            assert_eq!(actual.gpr, initial.gpr, "{level:?} {case:?}");
            assert_eq!(actual.rflags, initial.rflags, "{level:?} {case:?}");
            assert_eq!(actual.k, initial.k, "{level:?} {case:?}");
        }
    }
}
