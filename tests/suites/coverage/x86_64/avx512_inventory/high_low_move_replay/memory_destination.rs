//! Intel-inventory coverage for EVEX VMOVLPS/LPD/HPS/HPD memory stores.

use super::*;

fn stack_replay(bytes: &[u8]) -> Vec<u8> {
    let start = bytes
        .iter()
        .position(|byte| *byte == 0x62)
        .expect("EVEX prefix");
    let p0 = bytes[start + 1];
    let p1 = bytes[start + 2];
    let p2 = bytes[start + 3];
    let opcode = bytes[start + 4];
    let modrm = bytes[start + 5];
    vec![
        0x62,
        (p0 & 0x97) | 0x60,
        p1 | 0x04,
        p2,
        opcode,
        (modrm & 0x38) | 0x04,
        0x24,
    ]
}

fn assert_memory_half_move_store_lifts_admits_and_lowers(
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
            .filter(|op| matches!(
                op.kind,
                OpKind::Store {
                    width: rax::smir::MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1,
        "{} {level:?}: exact 8-byte Type-E9NF destination",
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
        "{} {level:?}: memory half-move store was not admitted ({bytes:02X?})",
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
        .unwrap_or_else(|error| panic!("{} {level:?}: {error:?}", row.cell));
    let expected = stack_replay(bytes);
    assert!(
        code.windows(expected.len())
            .any(|window| window == expected),
        "{} {level:?}: missing stack store replay {expected:02X?}",
        row.cell
    );
}

#[test]
fn memory_evex_half_move_store_replay_closes_all_4_intel_rows_at_o0_o1_o2() {
    let expected_mnemonics = set_from_slice(&["vmovhpd", "vmovhps", "vmovlpd", "vmovlps"]);
    let expected_shapes = BTreeSet::from([
        (1, 0x13, 0, false, 0),
        (1, 0x13, 1, true, 0),
        (1, 0x17, 0, false, 0),
        (1, 0x17, 1, true, 0),
    ]);
    let mut seen_mnemonics = BTreeSet::new();
    let mut seen_shapes = BTreeSet::new();
    let mut cells = 0usize;
    let mut lowerings = 0usize;

    for row in avx512_spec_evex_rows().into_iter().filter(|row| {
        expected_mnemonics.contains(&row.key.mnemonic) && matches!(row.key.opcode, 0x13 | 0x17)
    }) {
        seen_mnemonics.insert(row.key.mnemonic.clone());
        let widths: &[bool] = match row.key.w {
            EvexW::W0 => &[false],
            EvexW::W1 => &[true],
            EvexW::WIg => &[false, true],
        };
        for &w in widths {
            seen_shapes.insert((
                row.key.map,
                row.key.opcode,
                row.key.pp,
                w,
                avx512_spec::evex_vl_bits(row.key.vl),
            ));
        }

        for variant in evex_case_variants_for_row(&row) {
            assert_eq!(variant.mode, EvexAsmMode::Memory, "{}", row.cell);
            let bytes = raw_evex_spec_bytes_for_variant(&row, variant);
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                assert_memory_half_move_store_lifts_admits_and_lowers(&row, &bytes, level);
                lowerings += 1;
            }
            cells += 1;
        }
    }

    assert_eq!(seen_mnemonics, expected_mnemonics);
    assert_eq!(seen_shapes, expected_shapes);
    assert_eq!(cells, 4);
    assert_eq!(lowerings, 4 * 3);
}
