//! Generated EVEX inventory coverage for register-only packed-move replay.

use super::*;

#[test]
fn register_evex_packed_move_replay_closes_240_generated_lift_lower_gaps() {
    let expected_mnemonics = set_from_slice(&[
        "vmovapd",
        "vmovaps",
        "vmovdqa32",
        "vmovdqa64",
        "vmovdqu16",
        "vmovdqu32",
        "vmovdqu64",
        "vmovdqu8",
        "vmovupd",
        "vmovups",
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
        let w = match row.key.w {
            EvexW::W0 => false,
            EvexW::W1 => true,
            EvexW::WIg => panic!("packed EVEX move unexpectedly uses WIG: {}", row.cell),
        };
        expected_shapes.insert((
            row.key.opcode,
            row.key.pp,
            w,
            avx512_spec::evex_vl_bits(row.key.vl),
        ));
        for variant in evex_case_variants_for_row(&row) {
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            let classified = X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_packed_move_needs_vl();
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
    assert_eq!(expected_shapes.len(), 60);
    assert_eq!(register_forms, 240);
    assert_eq!(memory_forms, 60);

    // Exhaust the complete map-1 opcode/pp/W/L'L classifier space against the
    // independently parsed Intel specification rows.
    for opcode in u8::MIN..=u8::MAX {
        for pp in 0u8..=3 {
            for w in [false, true] {
                for ll in 0u8..=3 {
                    let bytes = [
                        0x62,
                        0xF1,
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
                            .evex_register_packed_move_needs_vl(),
                        expected,
                        "{bytes:02X?}"
                    );
                }
            }
        }
    }
}

#[cfg(feature = "smir-jit")]
fn assert_masked_memory_case_lifts_admits_and_lowers(
    row: &EvexSpecRow,
    bytes: &[u8],
    level: OptLevel,
) {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?} ({bytes:02X?})", row.cell));
    assert_eq!(result.bytes_consumed, bytes.len(), "{}", row.cell);

    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), 0x1000),
        X86InstructionBytes::new(bytes).unwrap(),
    );
    optimize_function(&mut function, level);

    assert!(
        is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true),
        "{} {level:?}: masked memory replay was not admitted ({bytes:02X?})",
        row.cell
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(false);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?} ({bytes:02X?})", row.cell));
    lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?}", row.cell));
}

#[cfg(feature = "smir-jit")]
#[test]
fn masked_memory_packed_move_replay_closes_all_90_intel_inventory_cells_at_o0_o1_o2() {
    let expected_mnemonics = set_from_slice(&[
        "vmovapd",
        "vmovaps",
        "vmovdqa32",
        "vmovdqa64",
        "vmovdqu16",
        "vmovdqu32",
        "vmovdqu64",
        "vmovdqu8",
        "vmovupd",
        "vmovups",
    ]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut rows = 0usize;
    let mut control_cells = 0usize;
    let mut lowerings = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        let Some(variant) = evex_case_variants_for_row(&row)
            .into_iter()
            .find(|variant| variant.mode == EvexAsmMode::Memory)
        else {
            continue;
        };
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
        assert_eq!(bytes[3] & 7, 1, "{} ({bytes:02X?})", row.cell);
        assert_eq!(bytes[3] & 0x80, 0, "{} ({bytes:02X?})", row.cell);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            assert_masked_memory_case_lifts_admits_and_lowers(&row, &bytes, level);
            lowerings += 1;
        }
        control_cells += 1;

        if matches!(row.key.opcode, 0x10 | 0x28 | 0x6F) {
            let mut zeroing = bytes.clone();
            zeroing[3] |= 0x80;
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_masked_memory_case_lifts_admits_and_lowers(&row, &zeroing, level);
                lowerings += 1;
            }
            control_cells += 1;
        } else {
            assert!(matches!(row.key.opcode, 0x11 | 0x29 | 0x7F));
        }
        rows += 1;
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(rows, 60);
    assert_eq!(control_cells, 90);
    assert_eq!(lowerings, 90 * 3);
}
