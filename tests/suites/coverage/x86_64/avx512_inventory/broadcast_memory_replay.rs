//! Intel-inventory coverage for helper-backed EVEX memory broadcasts.

use super::*;

fn is_memory_broadcast_row(row: &EvexSpecRow) -> bool {
    row.key.map == 2
        && row.key.pp == 1
        && matches!(
            (row.key.opcode, row.key.w),
            (0x18, EvexW::W0)
                | (0x19, EvexW::W0 | EvexW::W1)
                | (0x1A, EvexW::W0 | EvexW::W1)
                | (0x1B, EvexW::W0 | EvexW::W1)
                | (0x58, EvexW::W0)
                | (0x59, EvexW::W0 | EvexW::W1)
                | (0x5A, EvexW::W0 | EvexW::W1)
                | (0x5B, EvexW::W0 | EvexW::W1)
                | (0x78, EvexW::W0)
                | (0x79, EvexW::W0)
        )
}

#[test]
fn memory_evex_broadcast_replay_covers_all_34_intel_encoding_rows() {
    let expected_mnemonics = set_from_slice(&[
        "vbroadcastf32x2",
        "vbroadcastf32x4",
        "vbroadcastf32x8",
        "vbroadcastf64x2",
        "vbroadcastf64x4",
        "vbroadcasti32x2",
        "vbroadcasti32x4",
        "vbroadcasti32x8",
        "vbroadcasti64x2",
        "vbroadcasti64x4",
        "vbroadcastsd",
        "vbroadcastss",
        "vpbroadcastb",
        "vpbroadcastd",
        "vpbroadcastq",
        "vpbroadcastw",
    ]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut expected_shapes = BTreeSet::new();
    let mut register_forms = 0usize;
    let mut memory_forms = 0usize;
    let mut optimized_lowerings = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(is_memory_broadcast_row)
    {
        assert!(
            expected_mnemonics.contains(&row.key.mnemonic),
            "unexpected Intel broadcast row: {row:?}"
        );
        assert!(!row.key.imm, "broadcast row has immediate: {row:?}");
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let w = match row.key.w {
            EvexW::W0 => false,
            EvexW::W1 => true,
            EvexW::WIg => panic!("memory broadcast unexpectedly WIG: {}", row.cell),
        };
        let inserted =
            expected_shapes.insert((row.key.opcode, w, avx512_spec::evex_vl_bits(row.key.vl)));
        assert!(inserted, "duplicate Intel broadcast shape: {row:?}");

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            match variant.mode {
                EvexAsmMode::Register => {
                    register_forms += 1;
                }
                EvexAsmMode::Memory => {
                    assert_eq!(bytes.len(), 6, "{}", spec_case_variant_id(&row, variant));
                    let stack_instruction = [
                        0x62,
                        (bytes[1] & 0x97) | 0x60,
                        bytes[2] | 0x04,
                        bytes[3],
                        bytes[4],
                        (bytes[5] & 0x38) | 0x04,
                        0x24,
                    ];

                    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                        let mut lifter = X86_64Lifter::strict();
                        let mut context = LiftContext::new(SourceArch::X86_64);
                        let result = lifter
                            .lift_insn(0x1000, &bytes, &mut context)
                            .unwrap_or_else(|error| {
                                panic!(
                                    "{} {level:?}: {error:?} ({bytes:02X?})",
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
                        optimize_function(&mut function, level);

                        let excluded = std::collections::HashMap::new();
                        assert!(
                            is_native_clobber_safe_excluding(&function, &excluded, true),
                            "{} {level:?} ({bytes:02X?})",
                            spec_case_variant_id(&row, variant)
                        );
                        assert!(
                            !is_native_clobber_safe_excluding(&function, &excluded, false),
                            "{} {level:?}: admitted without memory helpers",
                            spec_case_variant_id(&row, variant)
                        );
                        let narrow =
                            rax::smir::lower::runtime::x86_native_vector_uses_k16_opmasks_excluding(
                                &function, &excluded,
                            );
                        let mut lowerer = X86_64Lowerer::new();
                        lowerer.set_mem_helpers(true);
                        lowerer.set_preserve_vector_mem_helpers(true);
                        lowerer.set_native_vector_state_active(true);
                        lowerer.set_narrow_vector_opmask_helpers(narrow);
                        lowerer.set_avx_ymm16_vector_state(false);
                        lowerer.set_jit_fault_deopt_guards(true);
                        lowerer.lower_function(&function).unwrap_or_else(|error| {
                            panic!(
                                "{} {level:?}: {error:?} ({bytes:02X?})",
                                spec_case_variant_id(&row, variant)
                            )
                        });
                        let code = lowerer
                            .finalize()
                            .expect("finalize Intel EVEX broadcast memory replay");
                        assert!(
                            code.windows(stack_instruction.len())
                                .any(|window| window == stack_instruction)
                        );
                        optimized_lowerings += 1;
                    }
                    memory_forms += 1;
                }
            }
        }
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(expected_shapes.len(), 34);
    assert_eq!(register_forms, 88);
    assert_eq!(memory_forms, 34);
    assert_eq!(optimized_lowerings, memory_forms * 3);
}
