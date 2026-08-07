//! Exhaustive legacy `0F 38` opcode-frontier coverage.

use super::*;

const DISPATCHED_LEGACY_0F38_OPCODES: &[u8] = &[
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x10, 0x14, 0x15, 0x17,
    0x1C, 0x1D, 0x1E, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x28, 0x29, 0x2A, 0x2B, 0x30, 0x31, 0x32,
    0x33, 0x34, 0x35, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x82, 0x8A,
    0x8B, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCF, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xF0, 0xF1, 0xF6,
    0xF8, 0xF9, 0xFC,
];

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

#[test]
fn every_undispatched_legacy_0f38_opcode_is_terminal_at_the_opcode_frontier() {
    for opcode in 0_u8..=u8::MAX {
        if DISPATCHED_LEGACY_0F38_OPCODES.contains(&opcode) {
            continue;
        }

        let bytes = [0x0F, 0x38, opcode, 0x84, 0x88, 0, 0, 0, 0xA5];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("legacy 0F 38 {opcode:02X}: {error:?}"));
        assert_invalid_opcode_trap(&result, 3);
    }
}

#[test]
fn legacy_0f38_terminal_matches_the_fixed_profile_and_absolute_frontiers() {
    for (name, bytes, expected_len) in [
        ("unassigned map cell", &[0x0F, 0x38, 0x0C, 0xC0][..], 3),
        (
            "disabled VMX INVEPT",
            &[0x66, 0x0F, 0x38, 0x80, 0xC0][..],
            4,
        ),
        ("disabled CET WRUSS", &[0x66, 0x0F, 0x38, 0xF5, 0xC0][..], 4),
        (
            "disabled Key Locker ENCODEKEY128",
            &[0xF3, 0x0F, 0x38, 0xFA, 0xC0][..],
            4,
        ),
    ] {
        let strict = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&strict, expected_len);

        let nonstrict = lift_nonstrict(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&nonstrict, expected_len);
    }

    assert!(matches!(
        lift_single(&[0x0F, 0x38]),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 2,
            need: 3,
        })
    ));
    assert!(matches!(
        lift_single(&[0x64, 0x67, 0x0F, 0x38]),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 4,
            need: 5,
        })
    ));
}

#[test]
fn every_profile_disabled_key_locker_opcode_and_modrm_byte_is_terminal() {
    const KEY_LOCKER_OPCODES: [u8; 7] = [0xD8, 0xDC, 0xDD, 0xDE, 0xDF, 0xFA, 0xFB];
    const PREFIXES: [&[u8]; 2] = [&[0xF3, 0x0F, 0x38], &[0xF3, 0x48, 0x0F, 0x38]];

    let mut checks = 0usize;
    for prefix in PREFIXES {
        for opcode in KEY_LOCKER_OPCODES {
            for modrm in u8::MIN..=u8::MAX {
                let mut bytes = prefix.to_vec();
                bytes.extend_from_slice(&[opcode, modrm]);
                let result = lift_single(&bytes).unwrap_or_else(|error| {
                    panic!("profile-disabled Key Locker {bytes:02X?}: {error:?}")
                });
                assert_invalid_opcode_trap(&result, prefix.len() + 1);
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 2 * 7 * 256);
}

#[test]
fn key_locker_mandatory_f3_dominates_redundant_66_without_hiding_aes() {
    for bytes in [
        &[0x66, 0xF3, 0x0F, 0x38, 0xDC, 0xC0][..],
        &[0xF3, 0x66, 0x0F, 0x38, 0xDC, 0xC0],
        &[0x66, 0xF3, 0x0F, 0x38, 0xDD, 0x00],
        &[0xF3, 0x66, 0x0F, 0x38, 0xDE, 0x00],
    ] {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("redundant-prefix Key Locker {bytes:02X?}: {error:?}"));
        assert_invalid_opcode_trap(&result, 5);
    }

    let aesenc = lift_single(&[0x66, 0x0F, 0x38, 0xDC, 0xC0]).expect("ordinary AESENC");
    assert_eq!(aesenc.bytes_consumed, 5);
    assert!(matches!(aesenc.control_flow, ControlFlow::Fallthrough));
    assert!(
        aesenc
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86Aes { .. }))
    );
}
