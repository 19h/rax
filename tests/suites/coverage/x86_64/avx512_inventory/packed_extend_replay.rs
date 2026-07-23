//! Generated EVEX inventory coverage for register-only packed widening moves.

use super::*;

#[test]
fn register_evex_packed_extend_replay_closes_144_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&[
        "vpmovsxbq",
        "vpmovsxbd",
        "vpmovsxbw",
        "vpmovsxdq",
        "vpmovsxwd",
        "vpmovsxwq",
        "vpmovzxbq",
        "vpmovzxbd",
        "vpmovzxbw",
        "vpmovzxdq",
        "vpmovzxwd",
        "vpmovzxwq",
    ]);
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
                row.key.opcode,
                row.key.pp,
                w,
                avx512_spec::evex_vl_bits(row.key.vl),
            ));
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let classified = X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_packed_extend_needs_vl();
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
                    function.x86_instruction_bytes.insert(
                        (BlockId(0), 0x1000),
                        X86InstructionBytes::new(&bytes).unwrap(),
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer.lower_function(&function).unwrap_or_else(|error| {
                        panic!(
                            "{}: {error:?} ({bytes:02X?})",
                            spec_case_variant_id(&row, variant)
                        )
                    });
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
    assert_eq!(expected_shapes.len(), 66);
    assert_eq!(register_forms, 144);
    assert_eq!(memory_forms, 36);

    // Exhaust the complete map-2 opcode/pp/W/L'L classifier space against the
    // independently parsed Intel specification rows, including both W values
    // for every architecturally WIG encoding.
    for opcode in u8::MIN..=u8::MAX {
        for pp in 0u8..=3 {
            for w in [false, true] {
                for ll in 0u8..=3 {
                    let bytes = [
                        0x62,
                        0xF2,
                        0x7C | pp | if w { 0x80 } else { 0 },
                        (ll << 5) | 0x09,
                        opcode,
                        0xC8,
                    ];
                    let expected = expected_shapes
                        .contains(&(opcode, pp, w, ll))
                        .then_some(ll != 2);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_packed_extend_needs_vl(),
                        expected,
                        "{bytes:02X?}"
                    );
                }
            }
        }
    }
}
