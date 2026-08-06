//! Intel-row inventory for helper-backed EVEX `VCVTPS2PH` memory destinations.

use super::*;

fn assert_memory_destination_lifts_admits_and_lowers(
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
    assert_eq!(
        result
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86PackedFpConvertStore { .. }))
            .count(),
        1,
        "{} {level:?}: {:#?}",
        row.cell,
        result.ops
    );

    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), 0x1000),
        X86InstructionBytes::new(bytes).expect("Intel VCVTPS2PH memory provenance"),
    );
    optimize_function(&mut function, level);

    let excluded = std::collections::HashMap::new();
    assert!(
        is_native_clobber_safe_excluding(&function, &excluded, true),
        "{} {level:?} ({bytes:02X?})",
        row.cell
    );
    assert!(
        !is_native_clobber_safe_excluding(&function, &excluded, false),
        "{} {level:?}: memory-disabled gate",
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
    let code = lowerer
        .finalize()
        .expect("finalize Intel EVEX VCVTPS2PH memory destination");

    let p0 = bytes[1];
    let p1 = bytes[2];
    let p2 = bytes[3];
    let modrm = bytes[5];
    let immediate = *bytes.last().unwrap();
    let source =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    let scratch = u8::from(source == 0);
    let rewritten = [
        0x62,
        (p0 & 0x97) | 0x60,
        p1 | 0x04,
        p2,
        0x1D,
        0xC0 | (modrm & 0x38) | scratch,
        immediate,
    ];
    assert!(
        code.windows(rewritten.len())
            .any(|window| window == rewritten),
        "{} {level:?}: missing {rewritten:02X?}",
        row.cell
    );
}

#[test]
fn all_six_intel_evex_vcvtps2ph_memory_cells_lift_admit_and_lower_at_o0_o1_o2() {
    let mut rows = 0usize;
    let mut cells = 0usize;
    let mut lowerings = 0usize;
    let mut widths = BTreeSet::new();

    for row in avx512_spec_evex_rows()
        .into_iter()
        .filter(|row| row.key.mnemonic == "vcvtps2ph")
    {
        let variant = evex_case_variants_for_row(&row)
            .into_iter()
            .find(|variant| variant.mode == EvexAsmMode::Memory)
            .unwrap_or_else(|| panic!("missing Intel memory variant: {}", row.cell));
        let masked = raw_evex_spec_bytes_for_variant(&row, variant);
        assert_eq!(masked[3] & 7, 1, "{}", row.cell);
        widths.insert(row.key.vl);
        rows += 1;

        for mask in [0, 1] {
            let mut bytes = masked.clone();
            bytes[3] = (bytes[3] & !7) | mask;
            cells += 1;
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_memory_destination_lifts_admits_and_lowers(&row, &bytes, level);
                lowerings += 1;
            }
        }
    }

    assert_eq!(rows, 3, "one Intel row per 128/256/512-bit source width");
    assert_eq!(
        widths,
        BTreeSet::from([EvexVl::Vl128, EvexVl::Vl256, EvexVl::Vl512])
    );
    assert_eq!(cells, 6, "three widths times unmasked/masked");
    assert_eq!(lowerings, 6 * 3, "all cells at O0/O1/O2");
}
