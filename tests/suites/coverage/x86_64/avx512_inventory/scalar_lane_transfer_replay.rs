//! Intel-inventory coverage for register-only EVEX scalar lane transfers.

use super::*;

#[cfg(feature = "smir-jit")]
#[path = "scalar_insert_memory_source.rs"]
mod scalar_insert_memory_source;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum GprField {
    None,
    Reg,
    Rm,
}

#[test]
fn register_evex_scalar_lane_transfer_replay_closes_26_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&[
        "vextractps",
        "vinsertps",
        "vpextrb",
        "vpextrd",
        "vpextrq",
        "vpextrw",
        "vpinsrb",
        "vpinsrd",
        "vpinsrq",
        "vpinsrw",
    ]);
    let dq_mnemonics = set_from_slice(&["vpextrd", "vpextrq", "vpinsrd", "vpinsrq"]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut expected_shapes = BTreeMap::new();
    let mut register_forms = 0usize;
    let mut memory_forms = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let needs_dq = dq_mnemonics.contains(&row.key.mnemonic);
        let reserved_vvvv =
            row.key.mnemonic == "vextractps" || row.key.mnemonic.starts_with("vpextr");
        let gpr_field = if row.key.mnemonic == "vinsertps" {
            GprField::None
        } else if row.key.mnemonic == "vpextrw" && row.key.map == 1 {
            GprField::Reg
        } else {
            GprField::Rm
        };
        let widths: &[bool] = match row.key.w {
            EvexW::W0 => &[false],
            EvexW::W1 => &[true],
            EvexW::WIg => &[false, true],
        };
        for &w in widths {
            let previous = expected_shapes.insert(
                (
                    row.key.map,
                    row.key.opcode,
                    row.key.pp,
                    w,
                    avx512_spec::evex_vl_bits(row.key.vl),
                    row.key.imm,
                ),
                (needs_dq, reserved_vvvv, gpr_field),
            );
            assert!(
                previous.is_none(),
                "duplicate Intel lane-transfer shape: {row:?}"
            );
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_scalar_lane_transfer_requires_dq();
            match variant.mode {
                EvexAsmMode::Register => {
                    assert_eq!(
                        classified,
                        Some(needs_dq),
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
                        .expect("finalize replay-eligible EVEX scalar lane transfer");
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
    assert_eq!(expected_shapes.len(), 17);
    assert_eq!(register_forms, 26);
    assert_eq!(memory_forms, 10);

    // Exhaust map/opcode/pp/W/L'L/EVEX.b/length and every R/X/B/R'
    // combination against independently parsed Intel rows. GPRs use only one
    // extension channel; the second channel must remain inverted-one because
    // the architecture has no GPR16-GPR31.
    for extensions in 0u8..=15 {
        for map in 0u8..=7 {
            for opcode in u8::MIN..=u8::MAX {
                for pp in 0u8..=3 {
                    for w in [false, true] {
                        for ll in 0u8..=3 {
                            for embedded_control in [false, true] {
                                for immediate in [false, true] {
                                    let mut bytes = vec![
                                        0x62,
                                        (extensions << 4) | map,
                                        0x7C | pp | if w { 0x80 } else { 0 },
                                        (ll << 5) | if embedded_control { 0x10 } else { 0 } | 0x08,
                                        opcode,
                                        0xC8,
                                    ];
                                    if immediate {
                                        bytes.push(0xFF);
                                    }
                                    let expected = expected_shapes
                                        .get(&(map, opcode, pp, w, ll, immediate))
                                        .and_then(|&(needs_dq, _, gpr_field)| {
                                            let extension_valid = match gpr_field {
                                                GprField::None => true,
                                                GprField::Reg => extensions & 0x01 != 0,
                                                GprField::Rm => extensions & 0x04 != 0,
                                            };
                                            (!embedded_control && extension_valid)
                                                .then_some(needs_dq)
                                        });
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_lane_transfer_requires_dq(),
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
        [0x62, 0xF1, 0x7D, 0x08, 0xC5, 0xE0, 0x03], // RSP destination in reg
        [0x62, 0xF3, 0x7D, 0x08, 0x17, 0xCD, 0x03], // RBP destination in r/m
        [0x62, 0xF3, 0x6D, 0x08, 0x20, 0xCC, 0x03], // RSP source in r/m
        [0x62, 0xE1, 0x7D, 0x08, 0xC5, 0xC8, 0x03], // fabricated reg GPR bit 4
        [0x62, 0xB3, 0x7D, 0x08, 0x17, 0xC8, 0x03], // fabricated r/m GPR bit 4
        [0x62, 0xF3, 0x75, 0x08, 0x17, 0xC8, 0x03], // reserved vvvv
        [0x62, 0xF3, 0x7D, 0x00, 0x17, 0xC8, 0x03], // reserved V'
        [0x62, 0xF3, 0x7D, 0x08, 0x17, 0x08, 0x03], // memory destination
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_lane_transfer_requires_dq(),
            None,
            "{bytes:02X?}"
        );
    }
}
