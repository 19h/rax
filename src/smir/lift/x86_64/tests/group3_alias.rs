//! Intel compatibility aliases for legacy Group 3 `/1` TEST encodings.

use super::*;

#[test]
fn lift_legacy_group3_slash1_test_alias_matches_slash0_exactly() {
    for (name, canonical, alias) in [
        (
            "TEST AL,imm8",
            &[0xF6, 0xC0, 0x81][..],
            &[0xF6, 0xC8, 0x81][..],
        ),
        (
            "TEST AH,imm8",
            &[0xF6, 0xC4, 0x7F][..],
            &[0xF6, 0xCC, 0x7F][..],
        ),
        (
            "TEST CX,imm16",
            &[0x66, 0xF7, 0xC1, 0x34, 0x80][..],
            &[0x66, 0xF7, 0xC9, 0x34, 0x80][..],
        ),
        (
            "TEST EDX,imm32",
            &[0xF7, 0xC2, 0x78, 0x56, 0x34, 0x80][..],
            &[0xF7, 0xCA, 0x78, 0x56, 0x34, 0x80][..],
        ),
        (
            "TEST RBX,sign-extended imm32",
            &[0x48, 0xF7, 0xC3, 0x78, 0x56, 0x34, 0x80][..],
            &[0x48, 0xF7, 0xCB, 0x78, 0x56, 0x34, 0x80][..],
        ),
        (
            "TEST qword [rbx],imm32",
            &[0x48, 0xF7, 0x03, 0x78, 0x56, 0x34, 0x80][..],
            &[0x48, 0xF7, 0x0B, 0x78, 0x56, 0x34, 0x80][..],
        ),
        (
            "TEST qword [rip+disp32],imm32",
            &[
                0x48, 0xF7, 0x05, 0x20, 0x00, 0x00, 0x00, 0x78, 0x56, 0x34, 0x80,
            ][..],
            &[
                0x48, 0xF7, 0x0D, 0x20, 0x00, 0x00, 0x00, 0x78, 0x56, 0x34, 0x80,
            ][..],
        ),
    ] {
        let canonical = lift_single(canonical).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let alias = lift_single(alias).unwrap_or_else(|error| panic!("{name} /1: {error:?}"));

        assert_eq!(alias.bytes_consumed, canonical.bytes_consumed, "{name}");
        assert_eq!(alias.ops.len(), canonical.ops.len(), "{name}");
        assert_eq!(
            format!("{:?}", alias.ops),
            format!("{:?}", canonical.ops),
            "{name}: /1 must produce exactly the /0 TEST semantics"
        );
    }

    for bytes in [
        &[0xF6, 0xC8][..],
        &[0x66, 0xF7, 0xC8, 0x00][..],
        &[0xF7, 0xC8, 0x00, 0x00, 0x00][..],
        &[0x48, 0xF7, 0xC8, 0x00, 0x00, 0x00][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "truncated Group 3 /1 TEST did not retain its immediate: {bytes:02X?}"
        );
    }
}
