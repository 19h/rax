//! Intel-inventory coverage for register-only EVEX scalar-integer moves.

use super::*;

#[cfg(feature = "smir-jit")]
#[path = "scalar_integer_move_replay/memory_source.rs"]
mod memory_source;

#[test]
fn register_evex_scalar_integer_move_replay_closes_12_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&["vmovq", "vmovw"]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut expected_shapes = BTreeSet::new();
    let mut replay_register_forms = 0usize;
    let mut existing_direct_register_forms = 0usize;
    let mut memory_forms = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let replay_family = row.key.mnemonic == "vmovw" || row.source.starts_with("movq.txt:");
        if replay_family {
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
                    row.key.mnemonic == "vmovw",
                ));
            }
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_scalar_integer_move_requires_fp16();
            let expected = match (replay_family, variant.mode) {
                (true, EvexAsmMode::Register) => Some(row.key.mnemonic == "vmovw"),
                _ => None,
            };
            assert_eq!(
                classified,
                expected,
                "{} ({bytes:02X?})",
                spec_case_variant_id(&row, variant)
            );

            match variant.mode {
                EvexAsmMode::Register => {
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
                    let code = lowerer.finalize().unwrap_or_else(|error| {
                        panic!(
                            "{}: {error:?} ({bytes:02X?})",
                            spec_case_variant_id(&row, variant)
                        )
                    });
                    assert!(code.windows(bytes.len()).any(|window| window == bytes));
                    if replay_family {
                        replay_register_forms += 1;
                    } else {
                        existing_direct_register_forms += 1;
                    }
                }
                EvexAsmMode::Memory => memory_forms += 1,
            }
        }
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    // VMOVQ has two vector-register aliases with four r/m extension buckets;
    // VMOVW has two GPR aliases with two r/m extension buckets.
    assert_eq!(replay_register_forms, 12);
    // VMOVQ's GPR aliases were already directly lowerable.
    assert_eq!(existing_direct_register_forms, 4);
    assert_eq!(memory_forms, 6);
    assert_eq!(expected_shapes.len(), 6);

    // Exhaust map/opcode/pp/W/L'L/EVEX.b/length and all four P0 extension
    // channels against independently parsed Intel rows. VMOVW requires raw
    // EVEX.X'=1 because its r/m operand is a 16-register GPR; VMOVQ consumes
    // that channel as XMM bit 4.
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
                                        |(shape_map, shape_opcode, shape_pp, shape_w, fp16)| {
                                            let vmovw_x_valid = !*fp16 || extensions & 0x04 != 0;
                                            (!embedded_control
                                                && !trailing
                                                && ll == 0
                                                && vmovw_x_valid
                                                && (*shape_map, *shape_opcode, *shape_pp, *shape_w)
                                                    == (map, opcode, pp, w))
                                                .then_some(*fp16)
                                        },
                                    );
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_integer_move_requires_fp16(),
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
        [0x62, 0xF5, 0x7D, 0x08, 0x6E, 0xC4], // RSP source
        [0x62, 0xF5, 0x7D, 0x08, 0x7E, 0xC5], // RBP destination
        [0x62, 0xB5, 0x7D, 0x08, 0x6E, 0xC0], // fabricated GPR bit 4
        [0x62, 0xF1, 0xFE, 0x08, 0x7E, 0x08], // VMOVQ memory source
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_integer_move_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }
}
