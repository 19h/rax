//! Intel-inventory coverage for register-only EVEX FP32/FP64 conversions.

use super::*;

#[test]
fn register_evex_fp32_fp64_convert_replay_closes_24_runtime_gate_gaps() {
    let expected_mnemonics = set_from_slice(&["vcvtpd2ps", "vcvtps2pd"]);
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
            ));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_fp32_fp64_convert_needs_vl();
            match variant.mode {
                EvexAsmMode::Register => {
                    assert_eq!(
                        classified,
                        Some(row.key.vl != EvexVl::Vl512),
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
                    optimize_function(&mut function, OptLevel::O2);
                    #[cfg(feature = "smir-jit")]
                    {
                        assert!(
                            is_native_clobber_safe_excluding(
                                &function,
                                &std::collections::HashMap::new(),
                                true,
                            ),
                            "{} ({bytes:02X?})",
                            spec_case_variant_id(&row, variant)
                        );
                    }

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer.lower_function(&function).unwrap_or_else(|error| {
                        panic!(
                            "{}: {error:?} ({bytes:02X?})",
                            spec_case_variant_id(&row, variant)
                        )
                    });
                    let code = lowerer
                        .finalize()
                        .expect("finalize replay-eligible EVEX FP32/FP64 conversion");
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
    assert_eq!(expected_shapes.len(), 6);
    // Six Intel rows each expand over four vector r/m extension buckets and
    // one memory form. Only the 24 register forms are source-replay safe.
    assert_eq!(register_forms, 24);
    assert_eq!(memory_forms, 6);

    // Exhaust map/opcode/pp/W/L'L/b/length and every R/X/B/R' combination
    // against the independently parsed Intel rows. With b=0, L'L selects the
    // row vector length and 11b is reserved. With register-source b=1, both
    // instructions imply 512 bits: VCVTPD2PS consumes L'L as ER, while exact
    // widening VCVTPS2PD ignores L'L and uses SAE.
    for extensions in 0u8..=15 {
        for map in 0u8..=7 {
            for opcode in u8::MIN..=u8::MAX {
                for pp in 0u8..=3 {
                    for w in [false, true] {
                        for ll in 0u8..=3 {
                            for embedded_control in [false, true] {
                                for trailing in [false, true] {
                                    let mut bytes = vec![
                                        0x62,
                                        (extensions << 4) | map,
                                        0x7C | pp | if w { 0x80 } else { 0 },
                                        (ll << 5) | if embedded_control { 0x10 } else { 0 } | 0x09,
                                        opcode,
                                        0xC8,
                                    ];
                                    if trailing {
                                        bytes.push(0xA5);
                                    }
                                    let shape = expected_shapes.iter().find(
                                        |(shape_map, shape_opcode, shape_pp, shape_w, shape_ll)| {
                                            (*shape_map, *shape_opcode, *shape_pp, *shape_w)
                                                == (map, opcode, pp, w)
                                                && (embedded_control || *shape_ll == ll)
                                        },
                                    );
                                    let expected = shape.and_then(|_| {
                                        (!trailing && (embedded_control || ll != 3))
                                            .then_some(!embedded_control && ll != 2)
                                    });
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_fp32_fp64_convert_needs_vl(),
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

    let register = [0x62, 0xF1, 0x7C, 0x09, 0x5A, 0xC8];
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
                .evex_register_fp32_fp64_convert_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }

    for p2 in [0x19, 0x39, 0x59, 0x79] {
        let bytes = [0x62, 0xF1, 0x7C, p2, 0x5A, 0xC8];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp32_fp64_convert_needs_vl(),
            Some(false),
            "{bytes:02X?}"
        );
    }
}
