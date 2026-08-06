//! Intel-inventory coverage for EVEX `VMOVNTDQA` memory replay.

use super::*;

fn assert_vmovntdqa_memory_lifts_admits_and_lowers(
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
        "{} {level:?}: VMOVNTDQA memory replay was not admitted ({bytes:02X?})",
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
fn vmovntdqa_memory_replay_closes_all_three_intel_inventory_cells_at_o0_o1_o2() {
    let rows: Vec<_> = avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| row.key.mnemonic == "vmovntdqa")
        .collect();
    assert_eq!(rows.len(), 3);

    let mut widths = BTreeSet::new();
    let mut lowerings = 0usize;
    for row in rows {
        assert_eq!(row.key.map, 2, "{}", row.cell);
        assert_eq!(row.key.opcode, 0x2A, "{}", row.cell);
        assert_eq!(row.key.pp, 1, "{}", row.cell);
        assert_eq!(row.key.w, EvexW::W0, "{}", row.cell);
        assert_eq!(row.key.form, avx512_spec::EvexOperandForm::MemoryOnly);
        widths.insert(row.key.vl);

        let variants = evex_case_variants_for_row(&row);
        assert_eq!(variants.len(), 1, "{}", row.cell);
        assert_eq!(variants[0].mode, EvexAsmMode::Memory, "{}", row.cell);
        let bytes = raw_evex_spec_bytes_for_variant(&row, variants[0]);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            assert_vmovntdqa_memory_lifts_admits_and_lowers(&row, &bytes, level);
            lowerings += 1;
        }
    }
    assert_eq!(
        widths,
        BTreeSet::from([EvexVl::Vl128, EvexVl::Vl256, EvexVl::Vl512])
    );
    assert_eq!(lowerings, 3 * 3);
}
