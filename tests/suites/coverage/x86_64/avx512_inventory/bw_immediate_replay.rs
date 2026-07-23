//! Intel-inventory coverage for register-only EVEX VPALIGNR/VDBPSADBW.

use super::*;

#[test]
fn register_evex_bw_immediate_replay_closes_24_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&["vdbpsadbw", "vpalignr"]);
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
            ));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_bw_immediate_needs_vl();
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

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer.lower_function(&function).unwrap_or_else(|error| {
                        panic!(
                            "{}: {error:?} ({bytes:02X?})",
                            spec_case_variant_id(&row, variant)
                        )
                    });
                    let code = lowerer
                        .finalize()
                        .expect("finalize replay-eligible AVX-512BW immediate operation");
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
    assert_eq!(register_forms, 24);
    assert_eq!(memory_forms, 6);

    // Exhaust map/opcode/pp/W/L'L/length and every R/X/B/R' combination
    // against independently parsed Intel rows. WIG rows contribute both
    // architectural W values.
    for extensions in 0u8..=15 {
        for map in 0u8..=7 {
            for opcode in u8::MIN..=u8::MAX {
                for pp in 0u8..=3 {
                    for w in [false, true] {
                        for ll in 0u8..=3 {
                            for immediate in [false, true] {
                                let mut bytes = vec![
                                    0x62,
                                    (extensions << 4) | map,
                                    0x7C | pp | if w { 0x80 } else { 0 },
                                    (ll << 5) | 0x09,
                                    opcode,
                                    0xC8,
                                ];
                                if immediate {
                                    bytes.push(0xFF);
                                }
                                let expected = expected_shapes
                                    .contains(&(map, opcode, pp, w, ll, immediate))
                                    .then_some(ll != 2);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_bw_immediate_needs_vl(),
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

    let register = [0x62, 0xF3, 0x6D, 0x08, 0x0F, 0xC8, 0x1F];
    let mut memory = register;
    memory[5] &= 0x3F;
    let mut embedded_broadcast = register;
    embedded_broadcast[3] |= 0x10;
    for bytes in [memory, embedded_broadcast] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_bw_immediate_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}
