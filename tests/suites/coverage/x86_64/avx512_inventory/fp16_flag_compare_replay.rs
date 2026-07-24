//! Intel-inventory coverage for register-only EVEX VCOMISH/VUCOMISH.

use super::*;

#[test]
fn register_evex_fp16_flag_compare_replay_closes_8_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&["vcomish", "vucomish"]);
    let expected_shapes = BTreeSet::from([(5, 0x2E, 0, false), (5, 0x2F, 0, false)]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut seen_shapes = BTreeSet::new();
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
            seen_shapes.insert((row.key.map, row.key.opcode, row.key.pp, w));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_fp16_flag_compare_requirements();
            match variant.mode {
                EvexAsmMode::Register => {
                    assert_eq!(
                        classified,
                        Some((false, true)),
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
                        .expect("finalize replay-eligible FP16 flag compare");
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
    assert_eq!(seen_shapes, expected_shapes);
    // Each Intel row expands across four vector r/m extension buckets and one
    // memory form. Only the eight register forms are replay-safe.
    assert_eq!(register_forms, 8);
    assert_eq!(memory_forms, 2);

    // Exhaust every structural prefix/opcode/control/length frontier. All
    // four R/X/B/R' extension bits encode vector register operands here.
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
                                            | 0x08,
                                        opcode,
                                        0xC8,
                                    ];
                                    if trailing {
                                        bytes.push(0xA5);
                                    }
                                    let expected = (!trailing
                                        && expected_shapes.contains(&(map, opcode, pp, w)))
                                    .then_some((false, true));
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_fp16_flag_compare_requirements(),
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

    let register = [0x62, 0xF5, 0x7C, 0x08, 0x2F, 0xC8];
    let mut memory = register;
    memory[5] &= 0x3F;
    let mut reserved_vvvv = register;
    reserved_vvvv[2] &= !0x08;
    let mut reserved_v_prime = register;
    reserved_v_prime[3] &= !0x08;
    let mut zeroing = register;
    zeroing[3] |= 0x80;
    let mut opmask = register;
    opmask[3] |= 1;
    for bytes in [memory, reserved_vvvv, reserved_v_prime, zeroing, opmask] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_flag_compare_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}
