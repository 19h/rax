//! Strict-lift coverage for Intel AMX in RAX's AMX-disabled guest profile.

use super::*;

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

fn assert_ud(bytes: &[u8]) {
    for result in [lift_single(bytes), lift_nonstrict(bytes)] {
        let result =
            result.unwrap_or_else(|error| panic!("assigned AMX form {bytes:02X?}: {error:?}"));
        assert_invalid_opcode_trap(&result, bytes.len());
    }
}

fn assert_incomplete(bytes: &[u8], need: usize) {
    let result = lift_single(bytes);
    assert!(
        matches!(
            result,
            Err(LiftError::Incomplete {
                addr: 0x1000,
                have,
                need: actual_need,
            }) if have == bytes.len() && actual_need == need
        ),
        "{bytes:02X?}: {result:?}"
    );
}

#[test]
fn every_assigned_vex_amx_0f38_cell_is_an_exact_terminal_ud() {
    // Intel SDM 325383-092 and Intel ISE 319433-059 assign 17 distinct
    // opcode/mandatory-prefix cells in the VEX.128.0F38.W0 tile space.
    let cases: &[(&str, &[u8])] = &[
        ("TMMULTF32PS", &[0xC4, 0xE2, 0x69, 0x48, 0xC1]),
        (
            "LDTILECFG disp32",
            &[0xC4, 0xE2, 0x78, 0x49, 0x80, 0x00, 0x04, 0x00, 0x00],
        ),
        ("STTILECFG disp8", &[0xC4, 0xE2, 0x79, 0x49, 0x40, 0x20]),
        ("TILEZERO", &[0xC4, 0xE2, 0x7B, 0x49, 0xC0]),
        ("TILELOADDRS", &[0xC4, 0xE2, 0x7B, 0x4A, 0x44, 0x88, 0x20]),
        ("TILELOADDRST1", &[0xC4, 0xE2, 0x79, 0x4A, 0x44, 0x88, 0x20]),
        ("TILELOADD", &[0xC4, 0xE2, 0x7B, 0x4B, 0x44, 0x88, 0x20]),
        ("TILELOADDT1", &[0xC4, 0xE2, 0x79, 0x4B, 0x44, 0x88, 0x20]),
        ("TILESTORED", &[0xC4, 0xE2, 0x7A, 0x4B, 0x44, 0x88, 0x20]),
        ("TDPBF16PS", &[0xC4, 0xE2, 0x6A, 0x5C, 0xC1]),
        ("TDPFP16PS", &[0xC4, 0xE2, 0x6B, 0x5C, 0xC1]),
        ("TDPBUUD", &[0xC4, 0xE2, 0x68, 0x5E, 0xC1]),
        ("TDPBUSD", &[0xC4, 0xE2, 0x69, 0x5E, 0xC1]),
        ("TDPBSUD", &[0xC4, 0xE2, 0x6A, 0x5E, 0xC1]),
        ("TDPBSSD", &[0xC4, 0xE2, 0x6B, 0x5E, 0xC1]),
        ("TCMMRLFP16PS", &[0xC4, 0xE2, 0x68, 0x6C, 0xC1]),
        ("TCMMIMFP16PS", &[0xC4, 0xE2, 0x69, 0x6C, 0xC1]),
    ];

    assert_eq!(cases.len(), 17);
    for &(name, bytes) in cases {
        let result =
            lift_single(bytes).unwrap_or_else(|error| panic!("{name} {bytes:02X?}: {error:?}"));
        assert_invalid_opcode_trap(&result, bytes.len());
        assert_ud(bytes);
    }

    // TILERELEASE shares the NP 49 cell with LDTILECFG.
    assert_ud(&[0xC4, 0xE2, 0x78, 0x49, 0xC0]);
}

#[test]
fn every_assigned_evex_amx_avx512_cell_is_an_exact_terminal_ud() {
    let mut cases = Vec::new();

    // Register-selected rows: EVEX.vvvv names the 32-bit row selector.
    for (opcode, pp_values) in [(0x4A, &[1_u8, 2_u8][..]), (0x6D, &[0, 1, 2, 3][..])] {
        for &pp in pp_values {
            cases.push(vec![0x62, 0xF2, 0x6C | pp, 0x48, opcode, 0xC1]);
        }
    }

    // Immediate-selected rows: the mandatory-prefix variants are distinct
    // AMX-AVX512 conversion or row-move instructions.
    for (opcode, pp_values) in [(0x07, &[0_u8, 1, 2, 3][..]), (0x77, &[2, 3][..])] {
        for &pp in pp_values {
            cases.push(vec![0x62, 0xF3, 0x7C | pp, 0x48, opcode, 0xC1, 0xA5]);
        }
    }

    assert_eq!(cases.len(), 12);
    for bytes in cases {
        assert_ud(&bytes);
    }
}

#[test]
fn disabled_amx_preserves_outer_prefix_and_complete_operand_boundaries() {
    // Address-size and FS overrides are valid before VEX. The decoded SIB and
    // displacement determine the length, but produce no address or memory op.
    let prefixed = [0x64, 0x67, 0xC4, 0xE2, 0x7B, 0x4B, 0x44, 0x88, 0x20];
    let result = lift_single(&prefixed).expect("prefixed TILELOADD");
    assert_invalid_opcode_trap(&result, prefixed.len());

    // A terminal profile #UD must not be rewritten into a dynamic APX guard,
    // even when B4/X4 would name R16/R17 for a semantic memory instruction.
    let extended_evex = [0x62, 0xFA, 0x69, 0x48, 0x4A, 0x04, 0x08];
    let result = lift_single(&extended_evex).expect("disabled EVEX AMX with B4/X4");
    assert_invalid_opcode_trap(&result, extended_evex.len());
    assert!(result.ops.is_empty(), "{:#?}", result.ops);

    assert_incomplete(&[0xC4, 0xE2, 0x79, 0x48], 5);
    assert_incomplete(&[0xC4, 0xE2, 0x7B, 0x4B, 0x04], 6);
    assert_incomplete(&[0xC4, 0xE2, 0x7B, 0x4B, 0x44, 0x20], 7);
    assert_incomplete(&[0xC4, 0xE2, 0x78, 0x49, 0x80, 0x11, 0x22, 0x33], 9);
    assert_incomplete(&[0x62, 0xF2, 0x7D, 0x48, 0x4A], 6);
    assert_incomplete(&[0x62, 0xF3, 0x7C, 0x48, 0x07, 0xC1], 7);
}

#[test]
fn disabled_amx_terminates_strict_blocks_without_lifting_fallthrough() {
    let disabled = [0x62, 0xF3, 0x7C, 0x48, 0x07, 0xC1, 0xA5];
    let mut code = vec![0x90];
    code.extend_from_slice(&disabled);
    code.extend_from_slice(&[0xB8, 0xEF, 0xBE, 0xAD, 0xDE]);

    let memory = TestMemory::new(0x1000, code);
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let block = lifter
        .lift_block(0x1000, &memory, &mut ctx)
        .expect("NOP followed by disabled AMX-AVX512 must lift");

    assert!(block.ops.is_empty());
    assert!(matches!(
        block.terminator,
        Terminator::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}
