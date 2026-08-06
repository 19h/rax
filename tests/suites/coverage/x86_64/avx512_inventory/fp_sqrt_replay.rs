//! Intel-inventory coverage for register-only EVEX floating-point square root.

use super::*;

#[cfg(feature = "smir-jit")]
#[path = "packed_er_memory_source.rs"]
mod packed_er_memory_source;

#[test]
fn register_evex_fp_sqrt_replay_closes_44_generated_lift_lower_gaps() {
    let expected_mnemonics =
        set_from_slice(&["vsqrtpd", "vsqrtph", "vsqrtps", "vsqrtsd", "vsqrtss"]);
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
        let ll_values: &[u8] = match row.key.vl {
            EvexVl::LlIg => &[0, 1, 2],
            _ => &[avx512_spec::evex_vl_bits(row.key.vl)],
        };
        for &w in widths {
            for &ll in ll_values {
                expected_shapes.insert((
                    row.key.map,
                    row.key.opcode,
                    row.key.pp,
                    w,
                    ll,
                    row.key.vl == EvexVl::LlIg,
                    row.key.mnemonic == "vsqrtph",
                ));
            }
        }

        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let classified = instruction.evex_register_fp_sqrt_requirements();
            match variant.mode {
                EvexAsmMode::Register => {
                    let expected = Some((
                        row.key.vl != EvexVl::LlIg && row.key.vl != EvexVl::Vl512,
                        row.key.mnemonic == "vsqrtph",
                    ));
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
                        .expect("finalize replay-eligible EVEX square-root operation");
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
    // Nine packed width rows plus three defined LLIG encodings for each scalar
    // row. L'L=11 is reserved when EVEX.b does not select an embedded control.
    assert_eq!(expected_shapes.len(), 15);
    assert_eq!(register_forms, 44);
    assert_eq!(memory_forms, 11);

    // Exhaust map/opcode/pp/W/L'L/length and every R/X/B/R' combination
    // against independently parsed Intel rows. The loop probes all four L'L
    // values and requires the reserved L'L=11 encoding to fail closed; EVEX.b
    // controls are exhaustively checked by the IR classifier test because the
    // source inventory records one opcode row.
    for extensions in 0u8..=15 {
        for map in 0u8..=7 {
            for opcode in u8::MIN..=u8::MAX {
                for pp in 0u8..=3 {
                    for w in [false, true] {
                        for ll in 0u8..=3 {
                            for trailing in [false, true] {
                                let mut bytes = vec![
                                    0x62,
                                    (extensions << 4) | map,
                                    0x7C | pp | if w { 0x80 } else { 0 },
                                    (ll << 5) | 0x09,
                                    opcode,
                                    0xC8,
                                ];
                                if trailing {
                                    bytes.push(0xA5);
                                }
                                let expected = expected_shapes
                                    .iter()
                                    .find(
                                        |(
                                            shape_map,
                                            shape_opcode,
                                            shape_pp,
                                            shape_w,
                                            shape_ll,
                                            _,
                                            _,
                                        )| {
                                            !trailing
                                                && (
                                                    *shape_map,
                                                    *shape_opcode,
                                                    *shape_pp,
                                                    *shape_w,
                                                    *shape_ll,
                                                ) == (map, opcode, pp, w, ll)
                                        },
                                    )
                                    .map(|(_, _, _, _, _, scalar, fp16)| {
                                        (!*scalar && ll != 2, *fp16)
                                    });
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_fp_sqrt_requirements(),
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

    let register = [0x62, 0xF1, 0x7C, 0x49, 0x51, 0xC8];
    let mut memory = register;
    memory[5] &= 0x3F;
    let mut reserved_vvvv = register;
    reserved_vvvv[2] &= !0x08;
    let mut reserved_v_prime = register;
    reserved_v_prime[3] &= !0x08;
    let mut reserved_ll = register;
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    let mut zeroing_k0 = register;
    zeroing_k0[3] = 0xC8;
    for bytes in [
        memory,
        reserved_vvvv,
        reserved_v_prime,
        reserved_ll,
        zeroing_k0,
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp_sqrt_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}
