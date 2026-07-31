//! Intel-inventory coverage for register-only EVEX FP16 narrowing conversions.

use super::*;

#[test]
fn register_evex_fp16_narrow_replay_closes_36_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&["vcvtpd2ph", "vcvtps2ph", "vcvtps2phx"]);
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
                row.key.imm,
                row.key.mnemonic != "vcvtps2ph",
            ));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_fp16_narrow_requirements();
            match variant.mode {
                EvexAsmMode::Register => {
                    let expected =
                        Some((row.key.vl != EvexVl::Vl512, row.key.mnemonic != "vcvtps2ph"));
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
                        .expect("finalize replay-eligible EVEX FP16 narrowing conversion");
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

    // Exhaust map/opcode/pp/W/L'L/b/length and every R/X/B/R' combination
    // against the independently parsed Intel rows. VCVTPD2PH and VCVTPS2PHX
    // consume all L'L values as ER when b=1; immediate-controlled VCVTPS2PH
    // retains imm8 rounding while b=1 makes all four L'L bit images defined.
    for extensions in 0u8..=15 {
        for map in 0u8..=7 {
            for opcode in u8::MIN..=u8::MAX {
                for pp in 0u8..=3 {
                    for w in [false, true] {
                        for ll in 0u8..=3 {
                            for embedded_control in [false, true] {
                                for suffix_len in 0usize..=2 {
                                    let mut bytes = vec![
                                        0x62,
                                        (extensions << 4) | map,
                                        0x7C | pp | if w { 0x80 } else { 0 },
                                        (ll << 5) | if embedded_control { 0x10 } else { 0 } | 0x09,
                                        opcode,
                                        0xC8,
                                    ];
                                    bytes.extend(std::iter::repeat_n(0xA5, suffix_len));
                                    let shape = expected_shapes.iter().find(
                                        |(
                                            shape_map,
                                            shape_opcode,
                                            shape_pp,
                                            shape_w,
                                            shape_ll,
                                            _,
                                            _,
                                        )| {
                                            (*shape_map, *shape_opcode, *shape_pp, *shape_w)
                                                == (map, opcode, pp, w)
                                                && (embedded_control || *shape_ll == ll)
                                        },
                                    );
                                    let expected = shape.and_then(
                                        |(_, _, _, _, _, has_immediate, needs_fp16)| {
                                            let control_valid =
                                                if embedded_control { true } else { ll != 3 };
                                            let length_valid =
                                                suffix_len == if *has_immediate { 1 } else { 0 };
                                            (control_valid && length_valid).then_some((
                                                !embedded_control && ll != 2,
                                                *needs_fp16,
                                            ))
                                        },
                                    );
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_fp16_narrow_requirements(),
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

    // All 256 immediate values are structural encodings; only bits 2:0 have
    // architectural rounding meaning and bits 7:3 are ignored. Each is also
    // admitted through all four register-source SAE L'L aliases.
    for immediate in u8::MIN..=u8::MAX {
        for (ll, embedded_control, expected) in [
            (0, false, (true, false)),
            (0, true, (false, false)),
            (1, true, (false, false)),
            (2, true, (false, false)),
            (3, true, (false, false)),
        ] {
            let bytes = [
                0x62,
                0xF3,
                0x7D,
                (ll << 5) | if embedded_control { 0x19 } else { 0x09 },
                0x1D,
                0xC8,
                immediate,
            ];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_fp16_narrow_requirements(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    let register = [0x62, 0xF5, 0x7D, 0x09, 0x1D, 0xC8];
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
                .evex_register_fp16_narrow_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}
