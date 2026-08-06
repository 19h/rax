//! Intel-inventory closure for AVX512_4VNNIW Tuple1_4X memory sources.

use super::*;

fn assert_four_dot_memory_lifts_admits_and_lowers(
    row: &EvexSpecRow,
    bytes: &[u8],
    level: OptLevel,
    source_index: u8,
    control: &str,
) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut context)
        .unwrap_or_else(|error| {
            panic!(
                "{} src={source_index} {control} {level:?}: {error:?} ({bytes:02X?})",
                row.cell
            )
        });
    assert_eq!(result.bytes_consumed, bytes.len(), "{} {control}", row.cell);
    assert!(
        result
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86FourDotProduct { .. })),
        "{} src={source_index} {control}: missing 4VNNIW semantic",
        row.cell
    );

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
        "{} src={source_index} {control} {level:?}: 4VNNIW replay was not admitted",
        row.cell
    );
    assert!(
        !is_native_clobber_safe_excluding(&function, &excluded, false),
        "{} src={source_index} {control} {level:?}: bypassed memory gate",
        row.cell
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.lower_function(&function).unwrap_or_else(|error| {
        panic!(
            "{} src={source_index} {control} {level:?}: {error:?} ({bytes:02X?})",
            row.cell
        )
    });
    let code = lowerer.finalize().unwrap_or_else(|error| {
        panic!(
            "{} src={source_index} {control} {level:?}: {error:?}",
            row.cell
        )
    });
    assert!(!code.is_empty());
}

#[test]
fn four_dot_product_memory_replay_closes_all_18_scanner_cells_at_o0_o1_o2() {
    let expected_mnemonics = set_from_slice(&["vp4dpwssd", "vp4dpwssds"]);
    let mut rows = BTreeMap::new();
    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        rows.entry(row.key.mnemonic.clone()).or_insert(row);
    }
    assert_eq!(
        rows.keys().cloned().collect::<BTreeSet<_>>(),
        expected_mnemonics
    );

    let controls = [
        (0u8, false, "unmasked"),
        (1, false, "merge"),
        (1, true, "zero"),
    ];
    let mut cells = 0usize;
    let mut lowerings = 0usize;
    for row in rows.values() {
        let variant = evex_case_variants_for_row(row)
            .into_iter()
            .find(|variant| variant.mode == EvexAsmMode::Memory)
            .expect("4VNNIW specification row has a memory form");
        let base = raw_evex_spec_bytes_for_variant(row, variant);
        assert_eq!(base.len(), 6, "{} ({base:02X?})", row.cell);
        assert_eq!(base[0], 0x62, "{} ({base:02X?})", row.cell);
        assert_eq!(base[1] & 7, 2, "{} ({base:02X?})", row.cell);
        assert_eq!(base[2] & 3, 3, "{} ({base:02X?})", row.cell);

        for source_index in [0u8, 1, 15] {
            for (mask, zeroing, control) in controls {
                let mut bytes = base.clone();
                bytes[2] = (bytes[2] & 0x87) | (((!source_index) & 0x0F) << 3);
                bytes[3] = (u8::from(zeroing) << 7)
                    | 0x40
                    | (u8::from(source_index & 16 == 0) << 3)
                    | mask;
                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    assert_four_dot_memory_lifts_admits_and_lowers(
                        row,
                        &bytes,
                        level,
                        source_index,
                        control,
                    );
                    lowerings += 1;
                }
                cells += 1;
            }
        }
    }

    assert_eq!(rows.len(), 2);
    assert_eq!(cells, 18);
    assert_eq!(lowerings, 18 * 3);
}
