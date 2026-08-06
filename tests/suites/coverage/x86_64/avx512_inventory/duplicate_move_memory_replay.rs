//! Intel-inventory coverage for EVEX duplicate-move memory replay.

use super::*;

fn assert_duplicate_move_memory_lifts_admits_and_lowers(
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
        "{} {level:?}: duplicate-move memory replay was not admitted ({bytes:02X?})",
        row.cell
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?} ({bytes:02X?})", row.cell));
    lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?}", row.cell));
}

#[test]
fn duplicate_move_memory_replay_closes_all_27_intel_inventory_cells_at_o0_o1_o2() {
    let expected_mnemonics = set_from_slice(&["vmovddup", "vmovshdup", "vmovsldup"]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut rows = 0usize;
    let mut control_cells = 0usize;
    let mut lowerings = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        let variant = evex_case_variants_for_row(&row)
            .into_iter()
            .find(|variant| variant.mode == EvexAsmMode::Memory)
            .unwrap_or_else(|| panic!("missing memory form: {}", row.cell));
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let base = raw_evex_spec_bytes_for_variant(&row, variant);
        assert_eq!(base[3] & 7, 1, "{} ({base:02X?})", row.cell);
        assert_eq!(base[3] & 0x80, 0, "{} ({base:02X?})", row.cell);

        let mut unmasked = base.clone();
        unmasked[3] &= !0x87;
        let merge = base;
        let mut zero = merge.clone();
        zero[3] |= 0x80;
        for bytes in [&unmasked, &merge, &zero] {
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_duplicate_move_memory_lifts_admits_and_lowers(&row, bytes, level);
                lowerings += 1;
            }
            control_cells += 1;
        }
        rows += 1;
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(rows, 9);
    assert_eq!(control_cells, 27);
    assert_eq!(lowerings, 27 * 3);
}
