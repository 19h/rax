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
