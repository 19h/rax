//! Intel-inventory coverage for EVEX scalar integer-to-floating-point replay.

use super::*;

fn scalar_int_to_fp_encoding(
    map: u8,
    pp: u8,
    opcode: u8,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
) -> [u8; 6] {
    assert!(map < 8 && pp < 4 && ll < 4);
    assert!(destination < 32 && merge < 32 && source < 16);
    let mut p0 = 0xF0 | map;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        (if w { 0x80 } else { 0 }) | ((!merge & 0x0F) << 3) | 0x04 | pp,
        (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if merge & 0x10 == 0 { 0x08 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn register_evex_scalar_int_to_fp_replay_closes_24_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&[
        "vcvtsi2sd",
        "vcvtsi2sh",
        "vcvtsi2ss",
        "vcvtusi2sd",
        "vcvtusi2sh",
        "vcvtusi2ss",
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
            expected_shapes.insert((row.key.map, row.key.opcode, row.key.pp, w, row.key.map == 5));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_scalar_int_to_fp_requires_fp16();
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

            // Establish the exact overlap with the semantic lowerer before
            // source replay metadata is supplied.
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
                        .expect("finalize replay-eligible EVEX scalar integer-to-FP conversion");
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
    // Six mnemonics each have W0/W1 rows. Each row expands over two GPR r/m
    // extension buckets and one memory form.
    assert_eq!(rows, 12);
    assert_eq!(expected_shapes.len(), 12);
    assert_eq!(register_forms, 24);
    assert_eq!(memory_forms, 12);
    assert_eq!(preexisting_direct_register_forms, 0);
    assert_eq!(preexisting_register_gaps, 24);

    // Exhaust map/opcode/pp/W/L'L/b/length and all four P0 extension channels
    // against the independently parsed Intel rows. EVEX.X must remain encoded
    // one because architectural GPR sources have no bit 4.
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
                                        (if w { 0x80 } else { 0 }) | 0x7C | pp,
                                        (ll << 5) | if embedded_control { 0x10 } else { 0 } | 0x08,
                                        opcode,
                                        0xC0,
                                    ];
                                    if trailing {
                                        bytes.push(0xA5);
                                    }
                                    let expected = expected_shapes.iter().find_map(
                                        |(shape_map, shape_opcode, shape_pp, shape_w, fp16)| {
                                            (!trailing
                                                && extensions & 0x04 != 0
                                                && (*shape_map, *shape_opcode, *shape_pp, *shape_w)
                                                    == (map, opcode, pp, w))
                                                .then_some(*fp16)
                                        },
                                    );
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_int_to_fp_requires_fp16(),
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

    // Exhaust every architectural destination, merge, and source register for
    // all 12 Intel shapes. Only low-bank RSP/RBP remain unsafe; R12/R13 are
    // admitted because EVEX.B selects the ordinary high GPR bank.
    let mut register_encodings = 0usize;
    for &(map, opcode, pp, w, fp16) in &expected_shapes {
        for destination in 0u8..32 {
            for merge in 0u8..32 {
                for source in 0u8..16 {
                    let bytes = scalar_int_to_fp_encoding(
                        map,
                        pp,
                        opcode,
                        w,
                        3,
                        true,
                        destination,
                        merge,
                        source,
                    );
                    let expected = (!matches!(source, 4 | 5)).then_some(fp16);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_scalar_int_to_fp_requires_fp16(),
                        expected,
                        "{bytes:02X?}"
                    );
                    register_encodings += 1;
                }
            }
        }
    }
    assert_eq!(register_encodings, 12 * 32 * 32 * 16);

    for bytes in [
        [0x62, 0xF1, 0x6E, 0x08, 0x2A, 0xC4], // RSP source.
        [0x62, 0xF1, 0x6E, 0x08, 0x2A, 0xC5], // RBP source.
        [0x62, 0xB1, 0x6E, 0x08, 0x2A, 0xC0], // Fabricated GPR bit 4.
        [0x62, 0xF1, 0x6E, 0x08, 0x2A, 0x00], // Memory source.
        [0x62, 0xF1, 0x6E, 0x09, 0x2A, 0xC0], // Reserved opmask.
        [0x62, 0xF1, 0x6E, 0x88, 0x2A, 0xC0], // Reserved zeroing.
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }
}
