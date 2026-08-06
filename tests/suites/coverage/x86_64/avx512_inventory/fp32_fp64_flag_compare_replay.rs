//! Intel-inventory coverage for EVEX FP32/FP64 flag compares.

use super::*;

#[cfg(feature = "smir-jit")]
#[path = "fp_flag_compare_replay/memory_source.rs"]
mod memory_source;

#[test]
fn register_evex_fp32_fp64_flag_compare_replay_closes_16_runtime_gate_gaps() {
    let expected_mnemonics = set_from_slice(&["vcomisd", "vcomiss", "vucomisd", "vucomiss"]);
    let expected_shapes = BTreeSet::from([
        (1, 0x2E, 0, false),
        (1, 0x2E, 1, true),
        (1, 0x2F, 0, false),
        (1, 0x2F, 1, true),
    ]);
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
            let classified = instruction.evex_register_fp32_fp64_flag_compare_requirements();
            match variant.mode {
                EvexAsmMode::Register => {
                    assert_eq!(
                        classified,
                        Some((false, false)),
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
                        .expect("finalize replay-eligible FP32/FP64 flag compare");
                    assert!(code.windows(bytes.len()).any(|window| window == bytes));
                    register_forms += 1;
                }
                EvexAsmMode::Memory => {
                    assert_eq!(
                        classified,
                        None,
                        "register classifier must exclude the separately inventoried memory form: {} ({bytes:02X?})",
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
    // memory form. This test owns the 16 register-classifier forms; the child
    // inventory owns memory admission and lowering.
    assert_eq!(register_forms, 16);
    assert_eq!(memory_forms, 4);

    for p1 in [0x7C, 0xFD] {
        let reserved = [0x62, 0xF1, p1, 0x68, 0x2E, 0xC8];
        assert_eq!(
            X86InstructionBytes::new(&reserved)
                .unwrap()
                .evex_register_fp32_fp64_flag_compare_requirements(),
            None,
            "{reserved:02X?}"
        );

        let sae = [0x62, 0xF1, p1, 0x78, 0x2E, 0xC8];
        assert_eq!(
            X86InstructionBytes::new(&sae)
                .unwrap()
                .evex_register_fp32_fp64_flag_compare_requirements(),
            Some((false, false)),
            "{sae:02X?}"
        );
    }
}
