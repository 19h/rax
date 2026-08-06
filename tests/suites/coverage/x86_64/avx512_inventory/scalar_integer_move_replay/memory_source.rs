//! Intel-inventory closure for EVEX scalar-integer memory moves.

use super::*;

fn assert_lifts_admits_and_lowers(row: &EvexSpecRow, bytes: &[u8], level: OptLevel) {
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
        "{} {level:?}: scalar-integer memory replay was not admitted ({bytes:02X?})",
        row.cell
    );
    let evex = bytes.iter().position(|byte| *byte == 0x62).unwrap();
    let stack = [
        0x62,
        (bytes[evex + 1] & 0x97) | 0x60,
        bytes[evex + 2] | 0x04,
        bytes[evex + 3],
        bytes[evex + 4],
        (bytes[evex + 5] & 0x38) | 0x04,
        0x24,
    ];
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
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?}", row.cell));
    assert!(
        code.windows(stack.len()).any(|window| window == stack),
        "{} {level:?}: exact stack replay missing",
        row.cell
    );
}

#[test]
fn scalar_integer_memory_replay_closes_all_10_intel_selector_cells_at_o0_o1_o2() {
    let expected_mnemonics = set_from_slice(&["vmovd", "vmovq", "vmovw"]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut shapes = BTreeSet::new();
    let mut rows = 0usize;
    let mut cells = 0usize;
    let mut lowerings = 0usize;
    let mut widths = BTreeMap::<u32, usize>::new();

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
        let w_values: &[bool] = match row.key.w {
            EvexW::W0 => &[false],
            EvexW::W1 => &[true],
            EvexW::WIg => &[false, true],
        };
        for &w in w_values {
            let mut bytes = base.clone();
            let evex = bytes.iter().position(|byte| *byte == 0x62).unwrap();
            bytes[evex + 2] = (bytes[evex + 2] & !0x80) | (u8::from(w) << 7);
            assert_eq!(bytes[evex + 1] & 7, row.key.map, "{}", row.cell);
            assert_eq!(bytes[evex + 2] & 3, row.key.pp, "{}", row.cell);
            assert_eq!(bytes[evex + 2] & 0x80 != 0, w, "{}", row.cell);
            assert_eq!((bytes[evex + 3] >> 5) & 3, 0, "{}", row.cell);
            assert_eq!(bytes[evex + 3] & 0x9F, 0x08, "{}", row.cell);
            assert_eq!(bytes[evex + 4], row.key.opcode, "{}", row.cell);
            shapes.insert((
                row.key.mnemonic.clone(),
                row.key.map,
                row.key.pp,
                w,
                row.key.opcode,
            ));
            let width = match row.key.mnemonic.as_str() {
                "vmovw" => 2,
                "vmovd" => 4,
                "vmovq" => 8,
                _ => unreachable!(),
            };
            *widths.entry(width).or_default() += 1;
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_lifts_admits_and_lowers(&row, &bytes, level);
                lowerings += 1;
            }
            cells += 1;
        }
        rows += 1;
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(rows, 8);
    assert_eq!(cells, 10);
    assert_eq!(shapes.len(), 10);
    assert_eq!(widths, BTreeMap::from([(2, 4), (4, 2), (8, 4)]));
    assert_eq!(lowerings, 10 * 3);
}
