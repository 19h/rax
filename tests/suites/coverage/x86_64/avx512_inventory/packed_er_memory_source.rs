//! Intel-inventory closure for packed AVX-512ER Type-E2 memory sources.

use super::*;

fn assert_packed_er_memory_lifts_admits_and_lowers(
    row: &EvexSpecRow,
    bytes: &[u8],
    level: OptLevel,
    control: &str,
) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut context)
        .unwrap_or_else(|error| {
            panic!("{} {control} {level:?}: {error:?} ({bytes:02X?})", row.cell)
        });
    assert_eq!(result.bytes_consumed, bytes.len(), "{} {control}", row.cell);
    assert!(
        result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86Exp2 { .. } | OpKind::X86Recip28 { .. } | OpKind::X86Rsqrt28 { .. }
        )),
        "{} {control}: missing packed AVX-512ER semantic",
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
        "{} {control} {level:?}: packed ER memory replay was not admitted ({bytes:02X?})",
        row.cell
    );
    assert!(
        !is_native_clobber_safe_excluding(&function, &excluded, false),
        "{} {control} {level:?}: packed ER replay bypassed the memory gate",
        row.cell
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_narrow_vector_opmask_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.lower_function(&function).unwrap_or_else(|error| {
        panic!("{} {control} {level:?}: {error:?} ({bytes:02X?})", row.cell)
    });
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{} {control} {level:?}: {error:?}", row.cell));
    assert!(!code.is_empty(), "{} {control} {level:?}", row.cell);
}

#[test]
fn packed_er_memory_replay_closes_all_36_form_mask_cells_at_o0_o1_o2() {
    let expected_mnemonics = set_from_slice(&[
        "vexp2pd",
        "vexp2ps",
        "vrcp28pd",
        "vrcp28ps",
        "vrsqrt28pd",
        "vrsqrt28ps",
    ]);
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
        (false, 0u8, false, "vector-unmasked"),
        (false, 3, false, "vector-merge"),
        (false, 3, true, "vector-zero"),
        (true, 0, false, "broadcast-unmasked"),
        (true, 3, false, "broadcast-merge"),
        (true, 3, true, "broadcast-zero"),
    ];
    let mut cells = 0usize;
    let mut lowerings = 0usize;
    for row in rows.values() {
        let variant = evex_case_variants_for_row(row)
            .into_iter()
            .find(|variant| variant.mode == EvexAsmMode::Memory)
            .expect("packed ER specification row has a memory form");
        let base = raw_evex_spec_bytes_for_variant(row, variant);
        assert_eq!(base.len(), 6, "{} ({base:02X?})", row.cell);
        assert_eq!(base[0], 0x62, "{} ({base:02X?})", row.cell);
        assert_eq!(base[1] & 7, 2, "{} ({base:02X?})", row.cell);
        assert_eq!(base[2] & 3, 1, "{} ({base:02X?})", row.cell);

        for (broadcast, mask, zeroing, control) in controls {
            let mut bytes = base.clone();
            // AVX-512ER packed forms are fixed EVEX.512; memory EVEX.b is a
            // scalar broadcast and never SAE. EVEX.V' and vvvv stay reserved.
            bytes[3] = 0x48 | (u8::from(broadcast) << 4) | mask | (u8::from(zeroing) << 7);
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_packed_er_memory_lifts_admits_and_lowers(row, &bytes, level, control);
                lowerings += 1;
            }
            cells += 1;
        }
    }

    assert_eq!(rows.len(), 6);
    assert_eq!(cells, 36);
    assert_eq!(lowerings, 36 * 3);
}
