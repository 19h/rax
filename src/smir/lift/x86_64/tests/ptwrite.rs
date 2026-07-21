//! Strict-lift coverage for PTWRITE in the deterministic non-Intel-PT profile.

use super::*;

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("profile-disabled PTWRITE must strictly lift to #UD");
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(result.ops.is_empty(), "{bytes:02X?}");
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
    assert!(matches!(
        result.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}

fn complete_memory_form(leader: &[u8], mode: u8, rm: u8) -> Vec<u8> {
    debug_assert!(mode < 3);
    debug_assert!(rm < 8);
    let mut bytes = leader.to_vec();
    bytes.push(mode << 6 | 4 << 3 | rm);
    if rm == 4 {
        bytes.push(0x24); // scale=1, no index, base=RSP/R12
    }
    match mode {
        0 if rm == 5 => bytes.extend_from_slice(&0x1234_5678_u32.to_le_bytes()),
        1 => bytes.push(0x80),
        2 => bytes.extend_from_slice(&0x89AB_CDEF_u32.to_le_bytes()),
        _ => {}
    }
    bytes
}

#[test]
fn every_legacy_ptwrite_register_selector_is_an_exact_invalid_opcode_trap() {
    for rm in 0..8 {
        for leader in [
            &[0xF3, 0x0F, 0xAE][..],
            &[0xF3, 0x48, 0x0F, 0xAE],
            &[0xF3, 0x41, 0x0F, 0xAE],
            &[0xF3, 0x49, 0x0F, 0xAE],
            &[0x66, 0xF3, 0x0F, 0xAE],
        ] {
            let mut bytes = leader.to_vec();
            bytes.push(0xE0 | rm);
            assert_ud(&bytes);
        }
    }
}

#[test]
fn every_complete_legacy_ptwrite_memory_shape_is_terminal_ud() {
    for leader in [
        &[0xF3, 0x0F, 0xAE][..],
        &[0xF3, 0x48, 0x0F, 0xAE],
        &[0x67, 0xF3, 0x0F, 0xAE],
        &[0x64, 0xF3, 0x0F, 0xAE],
        &[0x36, 0xF3, 0x0F, 0xAE],
        &[0x66, 0xF3, 0x0F, 0xAE],
    ] {
        for mode in 0..3 {
            for rm in 0..8 {
                assert_ud(&complete_memory_form(leader, mode, rm));
            }
        }
    }
}

#[test]
fn rex2_ptwrite_forms_converge_on_the_same_profile_disabled_ud() {
    for bank in 0..2 {
        for rm in 0..8 {
            assert_ud(&[0xF3, 0xD5, 0x90 | bank, 0xAE, 0xE0 | rm]);
        }
    }

    // Memory XSAVE-family slots are independently reserved under REX2. The
    // reservation and absent PTWRITE feature both resolve to the same #UD.
    assert_ud(&[0xF3, 0xD5, 0x80, 0xAE, 0x20]);
}

#[test]
fn lock_ptwrite_forms_are_terminal_ud_without_operand_observation() {
    assert_ud(&[0xF0, 0xF3, 0x0F, 0xAE, 0xE0]);
    assert_ud(&[0xF0, 0xF3, 0x0F, 0xAE, 0x64, 0x24, 0x7F]);
}

#[test]
fn ptwrite_address_truncation_remains_incomplete_not_ud() {
    for bytes in [
        &[0xF3, 0x0F, 0xAE, 0x24][..],
        &[0xF3, 0x0F, 0xAE, 0x25, 0x00, 0x00],
        &[0xF3, 0x0F, 0xAE, 0x64, 0x24],
        &[0xF3, 0x0F, 0xAE, 0xA4, 0x24, 0x00],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn disabled_ptwrite_terminates_a_strict_block_at_the_faulting_instruction() {
    let bytes = [
        0x90, // NOP
        0xF3, 0x48, 0x0F, 0xAE, 0x64, 0x24, 0x7F, // PTWRITE qword ptr [RSP+7Fh]
        0x90, // unreachable
    ];
    let mem = TestMemory::new(0x1000, bytes.to_vec());
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let block = lifter
        .lift_block(0x1000, &mem, &mut ctx)
        .expect("profile-disabled PTWRITE must terminate a strict block");

    assert!(block.ops.is_empty());
    assert!(matches!(
        block.terminator,
        Terminator::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}

#[test]
fn unprefixed_group15_slash4_remains_xsave() {
    let result = lift_single(&[0x0F, 0xAE, 0x20]).expect("unprefixed /4 is XSAVE");
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86XSave { .. },
            ..
        }]
    ));
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
}
