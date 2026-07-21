//! Strict-lift coverage for disabled and reserved Group 7 (`0F 01`) forms.

use super::*;

const RESIDUAL_FIXED_MODRM: [u8; 17] = [
    0xC7, 0xCC, 0xCD, 0xCE, 0xD2, 0xD3, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE,
    0xFF,
];

const OPCODE_LEADERS: [&[u8]; 18] = [
    &[0x0F, 0x01],
    &[0x26, 0x0F, 0x01],
    &[0x2E, 0x0F, 0x01],
    &[0x36, 0x0F, 0x01],
    &[0x3E, 0x0F, 0x01],
    &[0x64, 0x0F, 0x01],
    &[0x65, 0x0F, 0x01],
    &[0x66, 0x0F, 0x01],
    &[0x67, 0x0F, 0x01],
    &[0xF2, 0x0F, 0x01],
    &[0xF3, 0x0F, 0x01],
    &[0x40, 0x0F, 0x01],
    &[0x48, 0x0F, 0x01],
    &[0x4F, 0x0F, 0x01],
    &[0x66, 0xF2, 0x0F, 0x01],
    &[0x66, 0xF3, 0x0F, 0x01],
    &[0x64, 0xF3, 0x0F, 0x01],
    &[0xD5, 0x80, 0x01],
];

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("disabled or reserved Group 7 form must strictly lift");
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

fn complete_modrm_form(leader: &[u8], modrm: u8) -> Vec<u8> {
    let mode = modrm >> 6;
    let rm = modrm & 7;
    let mut bytes = leader.to_vec();
    bytes.push(modrm);
    if mode != 3 && rm == 4 {
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

fn complete_memory_form(leader: &[u8], mode: u8, rm: u8) -> Vec<u8> {
    debug_assert!(mode < 3);
    debug_assert!(rm < 8);
    complete_modrm_form(leader, (mode << 6) | (5 << 3) | rm)
}

#[test]
fn profile_disabled_fixed_group7_aliases_are_precise_invalid_opcode_traps() {
    for (name, bytes) in [
        ("XRESLDTRK", &[0xF2, 0x0F, 0x01, 0xE9][..]),
        ("SAVEPREVSSP", &[0xF3, 0x0F, 0x01, 0xEA][..]),
        ("UIRET", &[0xF3, 0x0F, 0x01, 0xEC][..]),
        ("TESTUI", &[0xF3, 0x0F, 0x01, 0xED][..]),
        ("MONITORX", &[0x0F, 0x01, 0xFA][..]),
        ("MCOMMIT", &[0xF3, 0x0F, 0x01, 0xFA][..]),
        ("MWAITX", &[0x0F, 0x01, 0xFB][..]),
        ("CLZERO", &[0x0F, 0x01, 0xFC][..]),
        ("RDPRU", &[0x0F, 0x01, 0xFD][..]),
        ("INVLPGB", &[0x0F, 0x01, 0xFE][..]),
        ("TLBSYNC", &[0x0F, 0x01, 0xFF][..]),
    ] {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("disabled {name} must strictly lift: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert!(result.ops.is_empty(), "{name}");
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}

#[test]
fn every_residual_fixed_group7_encoding_and_prefix_class_is_terminal_ud() {
    for leader in OPCODE_LEADERS {
        for modrm in RESIDUAL_FIXED_MODRM {
            let mut bytes = leader.to_vec();
            bytes.push(modrm);
            assert_ud(&bytes);
        }
    }
}

#[test]
fn every_group7_memory_slash5_form_consumes_its_complete_address_before_ud() {
    for leader in OPCODE_LEADERS {
        for mode in 0..3 {
            for rm in 0..8 {
                assert_ud(&complete_memory_form(leader, mode, rm));
            }
        }
    }
}

#[test]
fn every_complete_group7_modrm_and_prefix_class_avoids_interpreter_fallback() {
    for leader in OPCODE_LEADERS {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = complete_modrm_form(leader, modrm);
            match lift_single(&bytes) {
                Ok(result) => assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}"),
                Err(LiftError::InvalidEncoding { .. }) => {}
                other => panic!("complete Group 7 form entered fallback: {bytes:02X?}: {other:?}"),
            }
        }
    }
}

#[test]
fn group7_memory_slash5_truncation_remains_incomplete() {
    for bytes in [
        &[0x0F, 0x01, 0x2C][..],                   // missing SIB
        &[0xF3, 0x0F, 0x01, 0x2D, 0x00, 0x00][..], // short disp32
        &[0x0F, 0x01, 0x6C, 0x24][..],             // missing disp8
        &[0x0F, 0x01, 0xAC, 0x24, 0x00][..],       // short disp32 after SIB
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn lock_group7_residual_forms_remain_invalid_encodings() {
    for modrm in RESIDUAL_FIXED_MODRM {
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x01, modrm]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    for mode in 0..3 {
        for rm in 0..8 {
            let bytes = complete_memory_form(&[0xF0, 0x0F, 0x01], mode, rm);
            assert!(
                matches!(lift_single(&bytes), Err(LiftError::InvalidEncoding { .. })),
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn disabled_group7_forms_terminate_strict_blocks_without_fallthrough() {
    for bytes in [
        &[0x90, 0xF2, 0x0F, 0x01, 0xE9, 0x90][..],
        &[0x90, 0xF3, 0x0F, 0x01, 0x6C, 0x24, 0x7F, 0x90][..],
    ] {
        let mem = TestMemory::new(0x1000, bytes.to_vec());
        let mut lifter = X86_64Lifter::strict();
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let block = lifter
            .lift_block(0x1000, &mem, &mut ctx)
            .expect("disabled Group 7 form must terminate a strict block");

        assert!(block.ops.is_empty(), "{bytes:02X?}");
        assert!(matches!(
            block.terminator,
            Terminator::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}
