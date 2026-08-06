//! Intel-inventory coverage for register-only EVEX FP16 widening conversions.

use super::*;

#[cfg(feature = "smir-jit")]
#[path = "fp16_widen_memory_replay.rs"]
mod fp16_widen_memory_replay;

#[test]
fn register_evex_fp16_widen_replay_closes_36_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&["vcvtph2pd", "vcvtph2ps", "vcvtph2psx"]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut expected_shapes = BTreeSet::new();
    let mut register_forms = 0usize;
    let mut memory_forms = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let widths: &[bool] = match row.key.w {
            EvexW::W0 => &[false],
            EvexW::W1 => &[true],
            EvexW::WIg => &[false, true],
        };
        for &w in widths {
            expected_shapes.insert((
                row.key.map,
                row.key.opcode,
                row.key.pp,
                w,
                avx512_spec::evex_vl_bits(row.key.vl),
                row.key.mnemonic != "vcvtph2ps",
            ));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_fp16_widen_requirements();
            match variant.mode {
                EvexAsmMode::Register => {
                    let expected =
                        Some((row.key.vl != EvexVl::Vl512, row.key.mnemonic != "vcvtph2ps"));
                    assert_eq!(
                        classified,
                        expected,
                        "{} ({bytes:02X?})",
                        spec_case_variant_id(&row, variant)
                    );

                    let mut lifter = X86_64Lifter::strict();
                    let mut context = LiftContext::new(SourceArch::X86_64);
                    let result = lifter
                        .lift_insn(0x1000, &bytes, &mut context)
                        .unwrap_or_else(|error| {
                            panic!(
                                "{}: {error:?} ({bytes:02X?})",
                                spec_case_variant_id(&row, variant)
                            )
                        });
                    assert_eq!(result.bytes_consumed, bytes.len());

                    let mut block = SmirBlock::new(BlockId(0), 0x1000);
                    block.ops = result.ops;
                    block.set_terminator(Terminator::Return { values: vec![] });
                    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
                    function.add_block(block);
                    function
                        .x86_instruction_bytes
                        .insert((BlockId(0), 0x1000), instruction);

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer.lower_function(&function).unwrap_or_else(|error| {
                        panic!(
                            "{}: {error:?} ({bytes:02X?})",
                            spec_case_variant_id(&row, variant)
                        )
                    });
                    let code = lowerer
                        .finalize()
                        .expect("finalize replay-eligible EVEX FP16 widening conversion");
                    assert!(code.windows(bytes.len()).any(|window| window == bytes));
                    register_forms += 1;
                }
                EvexAsmMode::Memory => {
                    assert_eq!(
                        classified,
                        None,
                        "memory replay must fail closed: {} ({bytes:02X?})",
                        spec_case_variant_id(&row, variant)
                    );
                    memory_forms += 1;
                }
            }
        }
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(expected_shapes.len(), 9);
    // Nine Intel rows each expand over four vector r/m extension buckets and
    // one memory form. Only the 36 register forms are source-replay safe.
    assert_eq!(register_forms, 36);
    assert_eq!(memory_forms, 9);

    // Exhaust map/opcode/pp/W/L'L/SAE/length and every R/X/B/R' combination
    // against the independently parsed Intel rows. The source inventory has
    // one row per VL; register SAE implies VL=512 and ignores L'L.
    for extensions in 0u8..=15 {
        for map in 0u8..=7 {
            for opcode in u8::MIN..=u8::MAX {
                for pp in 0u8..=3 {
                    for w in [false, true] {
                        for ll in 0u8..=3 {
                            for suppress_exceptions in [false, true] {
                                for trailing in [false, true] {
                                    let mut bytes = vec![
                                        0x62,
                                        (extensions << 4) | map,
                                        0x7C | pp | if w { 0x80 } else { 0 },
                                        (ll << 5)
                                            | if suppress_exceptions { 0x10 } else { 0 }
                                            | 0x09,
                                        opcode,
                                        0xC8,
                                    ];
                                    if trailing {
                                        bytes.push(0xA5);
                                    }
                                    let shape = expected_shapes.iter().find(
                                        |(
                                            shape_map,
                                            shape_opcode,
                                            shape_pp,
                                            shape_w,
                                            shape_ll,
                                            _,
                                        )| {
                                            (*shape_map, *shape_opcode, *shape_pp, *shape_w)
                                                == (map, opcode, pp, w)
                                                && (suppress_exceptions || *shape_ll == ll)
                                        },
                                    );
                                    let expected = shape.and_then(|(_, _, _, _, _, needs_fp16)| {
                                        (!trailing && (suppress_exceptions || ll != 3)).then_some((
                                            !suppress_exceptions && ll != 2,
                                            *needs_fp16,
                                        ))
                                    });
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_fp16_widen_requirements(),
                                        expected,
                                        "{bytes:02X?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let register = [0x62, 0xF6, 0x7D, 0x09, 0x13, 0xC8];
    let mut memory = register;
    memory[5] &= 0x3F;
    let mut reserved_vvvv = register;
    reserved_vvvv[2] &= !0x08;
    let mut reserved_v_prime = register;
    reserved_v_prime[3] &= !0x08;
    let mut zeroing_k0 = register;
    zeroing_k0[3] = 0x88;
    let mut reserved_ll = register;
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    for bytes in [
        memory,
        reserved_vvvv,
        reserved_v_prime,
        zeroing_k0,
        reserved_ll,
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_widen_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn widening_sae_ignored_ll_closes_27_generated_lift_gaps_at_all_optimization_levels() {
    use rax::smir::{FpRoundMode, VecElementType, VecWidth};

    let mut lifted = 0usize;
    let mut lowered = 0usize;
    for (map, pp, opcode, to, lanes, needs_fp16, report_fp16_denormal) in [
        (5u8, 0u8, 0x5Au8, VecElementType::F64, 8u8, true, true),
        (2, 1, 0x13, VecElementType::F32, 16, false, false),
        (6, 1, 0x13, VecElementType::F32, 16, true, false),
    ] {
        for ll in 1u8..=3 {
            for (aaa, zeroing) in [(0u8, false), (1, false), (1, true)] {
                let bytes = [
                    0x62,
                    0xF0 | map,
                    0x7C | pp,
                    (u8::from(zeroing) << 7) | (ll << 5) | 0x18 | aaa,
                    opcode,
                    0xC2,
                ];
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                assert_eq!(
                    instruction.evex_register_fp16_widen_requirements(),
                    Some((false, needs_fp16)),
                    "{bytes:02X?}"
                );

                let mut lifter = X86_64Lifter::strict();
                let mut context = LiftContext::new(SourceArch::X86_64);
                let result = lifter
                    .lift_insn(0x1000, &bytes, &mut context)
                    .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                assert!(matches!(
                    result.ops.last().unwrap().kind,
                    OpKind::X86PackedFpConvert {
                        from: VecElementType::F16,
                        to: actual_to,
                        lanes: actual_lanes,
                        dst_width: VecWidth::V512,
                        round: FpRoundMode::Dynamic,
                        suppress_exceptions: true,
                        report_fp16_denormal: actual_report,
                        ..
                    } if actual_to == to
                        && actual_lanes == lanes
                        && actual_report == report_fp16_denormal
                ));

                let mut block = SmirBlock::new(BlockId(0), 0x1000);
                block.ops = result.ops;
                block.set_terminator(Terminator::Return { values: vec![] });
                let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
                function.add_block(block);
                function
                    .x86_instruction_bytes
                    .insert((BlockId(0), 0x1000), instruction);

                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let mut optimized = function.clone();
                    optimize_function(&mut optimized, level);
                    let mut lowerer = X86_64Lowerer::new();
                    lowerer
                        .lower_function(&optimized)
                        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                    let code = lowerer
                        .finalize()
                        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                    assert!(
                        code.windows(bytes.len()).any(|window| window == bytes),
                        "{level:?} {bytes:02X?}"
                    );
                    lowered += 1;
                }
                lifted += 1;
            }
        }
    }

    assert_eq!(lifted, 27);
    assert_eq!(lowered, 81);
}
