//! Intel-inventory closure for Type-E9NF EVEX scalar insertion from memory.

use super::*;

fn assert_scalar_insert_lifts_admits_and_lowers(
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
        "{} {control} {level:?}: scalar-insert memory replay was not admitted ({bytes:02X?})",
        row.cell
    );
    assert!(
        !is_native_clobber_safe_excluding(&function, &excluded, false),
        "{} {control} {level:?}: scalar-insert replay bypassed the memory gate",
        row.cell
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_narrow_vector_opmask_helpers(false);
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
fn scalar_insert_memory_replay_closes_all_seven_intel_w_cells_at_o0_o1_o2() {
    let expected_mnemonics =
        set_from_slice(&["vinsertps", "vpinsrb", "vpinsrd", "vpinsrq", "vpinsrw"]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut rows = 0usize;
    let mut cells = 0usize;
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
        rows += 1;
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let base = raw_evex_spec_bytes_for_variant(&row, variant);
        assert_eq!(base[0], 0x62, "{} ({base:02X?})", row.cell);
        assert!(matches!(base[1] & 7, 1 | 3), "{} ({base:02X?})", row.cell);
        assert_eq!(base[2] & 3, 1, "{} ({base:02X?})", row.cell);
        assert!(row.key.imm, "{} must carry imm8", row.cell);

        let widths: &[bool] = match row.key.w {
            EvexW::W0 => &[false],
            EvexW::W1 => &[true],
            EvexW::WIg => &[false, true],
        };
        for &w in widths {
            let mut bytes = base.clone();
            bytes[2] = (bytes[2] & !0x80) | (u8::from(w) << 7);
            bytes[3] &= 0x08; // fixed EVEX.128, unmasked, no broadcast
            let control = if w { "W1" } else { "W0" };
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_scalar_insert_lifts_admits_and_lowers(&row, &bytes, level, control);
                lowerings += 1;
            }
            cells += 1;
        }
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(rows, 5);
    assert_eq!(cells, 7);
    assert_eq!(lowerings, 7 * 3);
}
