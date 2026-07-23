//! Intel-inventory coverage for register-only EVEX VEXTRACTF*/VEXTRACTI* chunks.

use super::*;

#[test]
fn register_evex_chunk_extract_replay_closes_48_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&[
        "vextractf32x4",
        "vextractf32x8",
        "vextractf64x2",
        "vextractf64x4",
        "vextracti32x4",
        "vextracti32x8",
        "vextracti64x2",
        "vextracti64x4",
    ]);
    let dq_mnemonics = set_from_slice(&[
        "vextractf32x8",
        "vextractf64x2",
        "vextracti32x8",
        "vextracti64x2",
    ]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut expected_shapes = BTreeMap::new();
    let mut register_forms = 0usize;
    let mut memory_forms = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let requirements = (
            row.key.vl != EvexVl::Vl512,
            dq_mnemonics.contains(&row.key.mnemonic),
        );
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
                requirements,
            );
            assert!(previous.is_none(), "duplicate Intel extract shape: {row:?}");
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_chunk_extract_requirements();
            match variant.mode {
                EvexAsmMode::Register => {
                    assert_eq!(
                        classified,
                        Some(requirements),
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
                        .expect("finalize replay-eligible EVEX chunk extract");
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
    assert_eq!(expected_shapes.len(), 12);
    assert_eq!(register_forms, 48);
    assert_eq!(memory_forms, 12);

    // Exhaust map/opcode/pp/W/L'L/length and every R/X/B/R' combination
    // against independently parsed Intel rows.
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
                                    .get(&(map, opcode, pp, w, ll, immediate))
                                    .copied();
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_chunk_extract_requirements(),
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

    let register = [0x62, 0xF3, 0x7D, 0x29, 0x19, 0xC8, 0xFF];
    let mut memory = register;
    memory[5] &= 0x3F;
    let mut embedded_broadcast = register;
    embedded_broadcast[3] |= 0x10;
    let mut reserved_vvvv = register;
    reserved_vvvv[2] &= !0x08;
    let mut reserved_v_prime = register;
    reserved_v_prime[3] &= !0x08;
    for bytes in [memory, embedded_broadcast, reserved_vvvv, reserved_v_prime] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_chunk_extract_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}
