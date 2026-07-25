//! Intel-inventory coverage for EVEX scalar floating-point-to-integer replay.

use super::*;

#[test]
fn register_evex_scalar_fp_to_int_replay_closes_80_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&[
        "vcvtsd2si",
        "vcvtsd2usi",
        "vcvtsh2si",
        "vcvtsh2usi",
        "vcvtss2si",
        "vcvtss2usi",
        "vcvttsd2si",
        "vcvttsd2usi",
        "vcvttsh2si",
        "vcvttsh2usi",
        "vcvttss2si",
        "vcvttss2usi",
    ]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut expected_shapes = BTreeSet::new();
    let mut rows = 0usize;
    let mut register_forms = 0usize;
    let mut memory_forms = 0usize;
    let mut preexisting_direct_register_forms = 0usize;
    let mut preexisting_register_gaps = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        rows += 1;
        seen_mnemonics.insert(row.key.mnemonic.clone());
        assert_eq!(row.key.vl, EvexVl::LlIg, "{}", row.source);
        assert!(!row.key.imm, "{}", row.source);
        assert_eq!(row.key.opcode_ext, None, "{}", row.source);
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
                row.key.map == 5,
                matches!(row.key.opcode, 0x2C | 0x78),
            ));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_scalar_fp_to_int_requires_fp16();
            let expected = (variant.mode == EvexAsmMode::Register).then_some(row.key.map == 5);
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

            // Establish the exact overlap with the pre-existing semantic
            // lowerer before source replay metadata is supplied.
            let mut direct_lowerer = X86_64Lowerer::new();
            let directly_lowered = direct_lowerer.lower_function(&function).is_ok();

            match variant.mode {
                EvexAsmMode::Register => {
                    if directly_lowered {
                        preexisting_direct_register_forms += 1;
                    } else {
                        preexisting_register_gaps += 1;
                    }
                    function
                        .x86_instruction_bytes
                        .insert((BlockId(0), 0x1000), instruction);
                    let mut replay_lowerer = X86_64Lowerer::new();
                    replay_lowerer
                        .lower_function(&function)
                        .unwrap_or_else(|error| {
                            panic!(
                                "{}: {error:?} ({bytes:02X?})",
                                spec_case_variant_id(&row, variant)
                            )
                        });
                    let code = replay_lowerer
                        .finalize()
                        .expect("finalize replay-eligible EVEX scalar FP-to-int conversion");
                    assert!(code.windows(bytes.len()).any(|window| window == bytes));
                    register_forms += 1;
                }
                EvexAsmMode::Memory => {
                    assert!(
                        !directly_lowered,
                        "memory form unexpectedly lowered: {} ({bytes:02X?})",
                        spec_case_variant_id(&row, variant)
                    );
                    function
                        .x86_instruction_bytes
                        .insert((BlockId(0), 0x1000), instruction);
                    let mut replay_lowerer = X86_64Lowerer::new();
                    assert!(
                        replay_lowerer.lower_function(&function).is_err(),
                        "memory replay must fail closed: {} ({bytes:02X?})",
                        spec_case_variant_id(&row, variant)
                    );
                    memory_forms += 1;
                }
            }
        }
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    // Twelve mnemonics each have W0/W1 rows. Each row expands over four XMM
    // r/m extension buckets and one memory form.
    assert_eq!(rows, 24);
    assert_eq!(expected_shapes.len(), 24);
    assert_eq!(register_forms, 96);
    assert_eq!(memory_forms, 24);
    // Signed binary32/binary64 forms with XMM0-XMM15 were already directly
    // lowerable: 4 mnemonics x 2 widths x 2 low-XMM buckets = 16 forms.
    assert_eq!(preexisting_direct_register_forms, 16);
    assert_eq!(preexisting_register_gaps, 80);

    // Exhaust map/opcode/pp/W/L'L/b/length and all four P0 extension channels
    // against the independently parsed Intel rows. L'L is ignored with b=0,
    // selects ER for non-truncating b=1 forms, and remains ignored by the
    // truncating SAE forms. EVEX.R' must remain encoded one because the GPR
    // destination has no architectural bit 4.
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
                                        (ll << 5) | if embedded_control { 0x10 } else { 0 } | 0x08,
                                        opcode,
                                        0xC8,
                                    ];
                                    if trailing {
                                        bytes.push(0xA5);
                                    }
                                    let expected = expected_shapes.iter().find_map(
                                        |(
                                            shape_map,
                                            shape_opcode,
                                            shape_pp,
                                            shape_w,
                                            fp16,
                                            truncating,
                                        )| {
                                            (!trailing
                                                && extensions & 1 != 0
                                                && (ll != 3 || (embedded_control && !*truncating))
                                                && (*shape_map, *shape_opcode, *shape_pp, *shape_w)
                                                    == (map, opcode, pp, w))
                                                .then_some(*fp16)
                                        },
                                    );
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_fp_to_int_requires_fp16(),
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

    for bytes in [
        [0x62, 0xF1, 0x7E, 0x08, 0x2D, 0xE0], // RSP destination.
        [0x62, 0xF1, 0x7E, 0x08, 0x2D, 0xE8], // RBP destination.
        [0x62, 0xE1, 0x7E, 0x08, 0x2D, 0xC8], // Fabricated GPR bit 4.
        [0x62, 0xF1, 0x7E, 0x08, 0x2D, 0x08], // Memory source.
        [0x62, 0xF1, 0x76, 0x08, 0x2D, 0xC8], // Reserved vvvv.
        [0x62, 0xF1, 0x7E, 0x00, 0x2D, 0xC8], // Reserved V'.
        [0x62, 0xF1, 0x7E, 0x09, 0x2D, 0xC8], // Reserved opmask.
        [0x62, 0xF1, 0x7E, 0x88, 0x2D, 0xC8], // Reserved zeroing.
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_fp_to_int_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }
}
