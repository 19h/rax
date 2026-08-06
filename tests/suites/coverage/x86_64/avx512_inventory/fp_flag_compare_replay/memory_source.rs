//! Intel-inventory coverage for helper-backed EVEX VCOMI/VUCOMI memory replay.

use super::*;
use rax::smir::MemWidth;

fn expected_memory_width(mnemonic: &str) -> MemWidth {
    if mnemonic.ends_with("ish") {
        MemWidth::B2
    } else if mnemonic.ends_with("iss") {
        MemWidth::B4
    } else if mnemonic.ends_with("isd") {
        MemWidth::B8
    } else {
        panic!("unexpected scalar flag-compare mnemonic: {mnemonic}")
    }
}

fn assert_lifts_admits_and_lowers(row: &EvexSpecRow, bytes: &[u8], level: OptLevel) {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?} ({bytes:02X?})", row.cell));
    assert_eq!(result.bytes_consumed, bytes.len(), "{}", row.cell);
    let width = expected_memory_width(&row.key.mnemonic);
    assert_eq!(
        result
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Load { width: actual, .. } if actual == width))
            .count(),
        1,
        "{} {level:?}: exact scalar load ({bytes:02X?})",
        row.cell
    );
    assert_eq!(
        result
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::VBroadcast { lanes: 1, .. }))
            .count(),
        1,
        "{} {level:?}: scalar broadcast ({bytes:02X?})",
        row.cell
    );
    assert_eq!(
        result
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86FpCompare { .. }))
            .count(),
        1,
        "{} {level:?}: flag compare ({bytes:02X?})",
        row.cell
    );

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
        "{} {level:?}: memory replay was not admitted ({bytes:02X?})",
        row.cell
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?} ({bytes:02X?})", row.cell));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?}", row.cell));
    let expected = [
        0x62,
        (bytes[1] & 0x97) | 0x60,
        bytes[2] | 0x04,
        bytes[3],
        bytes[4],
        (bytes[5] & 0x38) | 0x04,
        0x24,
    ];
    assert_eq!(
        code.windows(expected.len())
            .filter(|window| *window == expected)
            .count(),
        1,
        "{} {level:?}: missing exact stack replay {expected:02X?} ({bytes:02X?})",
        row.cell
    );
}

#[test]
fn memory_evex_fp_flag_compare_replay_closes_all_18_inventory_cells_at_o0_o1_o2() {
    let expected_mnemonics = set_from_slice(&[
        "vcomisd", "vcomish", "vcomiss", "vucomisd", "vucomish", "vucomiss",
    ]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut rows = 0usize;
    let mut control_cells = 0usize;
    let mut lowerings = 0usize;

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| expected_mnemonics.contains(&row.key.mnemonic))
    {
        let variants = evex_case_variants_for_row(&row);
        let memory_variants = variants
            .iter()
            .filter(|variant| variant.mode == EvexAsmMode::Memory)
            .collect::<Vec<_>>();
        assert_eq!(memory_variants.len(), 1, "{}", row.cell);
        let base = raw_evex_spec_bytes_for_variant(&row, *memory_variants[0]);
        assert_eq!(base[0], 0x62, "{} ({base:02X?})", row.cell);
        assert_eq!(base[3] & 0x9F, 0x08, "{} ({base:02X?})", row.cell);
        seen_mnemonics.insert(row.key.mnemonic.clone());

        for ll in 0u8..=2 {
            let mut bytes = base.clone();
            bytes[3] = (bytes[3] & !0x60) | (ll << 5);
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_lifts_admits_and_lowers(&row, &bytes, level);
                lowerings += 1;
            }
            control_cells += 1;
        }
        rows += 1;
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(rows, 6);
    assert_eq!(control_cells, 18);
    assert_eq!(lowerings, 18 * 3);
}
