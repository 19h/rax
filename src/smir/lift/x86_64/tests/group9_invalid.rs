//! Strict-lift coverage for reserved Group 9 (`0F C7 /0` and `/2`) forms.

use super::*;

const OPCODE_LEADERS: [&[u8]; 19] = [
    &[0x0F, 0xC7],
    &[0x26, 0x0F, 0xC7],
    &[0x2E, 0x0F, 0xC7],
    &[0x36, 0x0F, 0xC7],
    &[0x3E, 0x0F, 0xC7],
    &[0x64, 0x0F, 0xC7],
    &[0x65, 0x0F, 0xC7],
    &[0x66, 0x0F, 0xC7],
    &[0x67, 0x0F, 0xC7],
    &[0xF0, 0x0F, 0xC7],
    &[0xF2, 0x0F, 0xC7],
    &[0xF3, 0x0F, 0xC7],
    &[0x40, 0x0F, 0xC7],
    &[0x48, 0x0F, 0xC7],
    &[0x4F, 0x0F, 0xC7],
    &[0x66, 0xF2, 0x0F, 0xC7],
    &[0x66, 0xF3, 0x0F, 0xC7],
    &[0x64, 0xF3, 0x0F, 0xC7],
    &[0xD5, 0x80, 0xC7],
];

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

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("reserved Group 9 form must strictly lift");
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

#[test]
fn every_reserved_group9_form_and_prefix_class_is_terminal_ud() {
    for leader in OPCODE_LEADERS {
        for mode in 0..=3 {
            for group in [0, 2] {
                for rm in 0..8 {
                    let modrm = (mode << 6) | (group << 3) | rm;
                    assert_ud(&complete_modrm_form(leader, modrm));
                }
            }
        }
    }
}

#[test]
fn every_scanner_profile_disabled_vmx_form_is_terminal_ud() {
    const FORMS: [(&str, &[u8], u8); 20] = [
        ("VMCLEAR 66", &[0x66, 0x0F, 0xC7], 6),
        ("VMCLEAR 66+REX.W", &[0x66, 0x48, 0x0F, 0xC7], 6),
        ("VMPTRLD NP", &[0x0F, 0xC7], 6),
        ("VMPTRLD 67", &[0x67, 0x0F, 0xC7], 6),
        ("VMPTRLD FS", &[0x64, 0x0F, 0xC7], 6),
        ("VMPTRLD GS", &[0x65, 0x0F, 0xC7], 6),
        ("VMPTRLD REX.W", &[0x48, 0x0F, 0xC7], 6),
        ("VMPTRLD REX.R", &[0x44, 0x0F, 0xC7], 6),
        ("VMPTRLD REX.B", &[0x41, 0x0F, 0xC7], 6),
        ("VMPTRLD REX.WRB", &[0x4D, 0x0F, 0xC7], 6),
        ("VMPTRST NP", &[0x0F, 0xC7], 7),
        ("VMPTRST 67", &[0x67, 0x0F, 0xC7], 7),
        ("VMPTRST FS", &[0x64, 0x0F, 0xC7], 7),
        ("VMPTRST GS", &[0x65, 0x0F, 0xC7], 7),
        ("VMPTRST REX.W", &[0x48, 0x0F, 0xC7], 7),
        ("VMPTRST REX.R", &[0x44, 0x0F, 0xC7], 7),
        ("VMPTRST REX.B", &[0x41, 0x0F, 0xC7], 7),
        ("VMPTRST REX.WRB", &[0x4D, 0x0F, 0xC7], 7),
        ("VMXON F3", &[0xF3, 0x0F, 0xC7], 6),
        ("VMXON F3+REX.W", &[0xF3, 0x48, 0x0F, 0xC7], 6),
    ];

    let mut checks = 0usize;
    for (name, leader, group) in FORMS {
        for mode in 0_u8..=2 {
            for rm in 0_u8..8 {
                let modrm = (mode << 6) | (group << 3) | rm;
                let bytes = complete_modrm_form(leader, modrm);
                let result = lift_single(&bytes)
                    .unwrap_or_else(|error| panic!("{name} {bytes:02X?}: {error:?}"));
                assert_invalid_opcode_trap(&result, bytes.len());
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 20 * 3 * 8);
}

#[test]
fn every_rex2_profile_disabled_vmx_memory_address_form_is_terminal_ud() {
    let mut checks = 0usize;
    for payload in 0x80_u8..=0xFF {
        for (mandatory_prefix, group) in [
            (&[][..], 6_u8),
            (&[0x66][..], 6),
            (&[0xF3][..], 6),
            (&[][..], 7),
        ] {
            let mut leader = mandatory_prefix.to_vec();
            leader.extend_from_slice(&[0xD5, payload, 0xC7]);
            for mode in 0_u8..=2 {
                for rm in 0_u8..8 {
                    let modrm = (mode << 6) | (group << 3) | rm;
                    assert_ud(&complete_modrm_form(&leader, modrm));
                    checks += 1;
                }
            }
        }
    }
    assert_eq!(checks, 128 * 4 * 3 * 8);
}

#[test]
fn redundant_66_prefix_does_not_override_profile_disabled_vmxon() {
    for leader in [
        &[0x66, 0xF3, 0x0F, 0xC7][..],
        &[0xF3, 0x66, 0x0F, 0xC7],
        &[0x66, 0xF3, 0xD5, 0x90, 0xC7],
        &[0xF3, 0x66, 0xD5, 0x90, 0xC7],
    ] {
        for mode in 0_u8..=2 {
            for rm in 0_u8..8 {
                assert_ud(&complete_modrm_form(leader, (mode << 6) | (6 << 3) | rm));
            }
        }
    }
}

#[test]
fn every_complete_group9_modrm_and_prefix_class_avoids_interpreter_fallback() {
    for leader in OPCODE_LEADERS {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = complete_modrm_form(leader, modrm);
            match lift_single(&bytes) {
                Ok(result)
                    if leader == [0xD5, 0x80, 0xC7]
                        && modrm >> 6 != 3
                        && matches!((modrm >> 3) & 7, 3..=5) =>
                {
                    // APX reserves the memory XSAVE*/XRSTOR* encodings at
                    // the opcode-plus-ModR/M boundary, before address decode.
                    assert_invalid_opcode_trap(&result, 4);
                }
                Ok(result) => assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}"),
                Err(LiftError::InvalidEncoding { .. }) => {}
                other => panic!("complete Group 9 form entered fallback: {bytes:02X?}: {other:?}"),
            }
        }
    }
}

#[test]
fn reserved_group9_memory_truncation_remains_incomplete() {
    for bytes in [
        &[0x0F, 0xC7, 0x04][..],
        &[0xF3, 0x0F, 0xC7, 0x15, 0x00, 0x00][..],
        &[0x0F, 0xC7, 0x44, 0x24][..],
        &[0xD5, 0x80, 0xC7, 0x94, 0x24, 0x00][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn reserved_group9_forms_terminate_strict_blocks_without_fallthrough() {
    for bytes in [
        &[0x90, 0x0F, 0xC7, 0xC0, 0x90][..],
        &[0x90, 0xF3, 0x0F, 0xC7, 0x54, 0x24, 0x7F, 0x90][..],
    ] {
        let mem = TestMemory::new(0x1000, bytes.to_vec());
        let mut lifter = X86_64Lifter::strict();
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let block = lifter
            .lift_block(0x1000, &mem, &mut ctx)
            .expect("reserved Group 9 form must terminate a strict block");

        assert!(block.ops.is_empty(), "{bytes:02X?}");
        assert!(matches!(
            block.terminator,
            Terminator::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}
