//! Exhaustive terminal coverage for the legacy two-byte opcode map.

use super::*;

// These are the only cells that reach `lift_0f_opcode`'s profile-terminal
// default after all assigned RAX-profile encodings are dispatched. 0F 1D is a
// ModR/M-consuming Reserved NOP, 0F 37 is GETSEC behind disabled SMX, and the
// other facility-assigned VMX cells 0F 78/79 and their mandatory-prefix SSE4A
// forms have their own exact dispatcher.
const PROFILE_TERMINAL_0F_OPCODES: &[u8] = &[
    0x24, 0x25, 0x26, 0x27, 0x36, 0x37, 0x39, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x7A, 0x7B, 0xA6, 0xA7,
];

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

#[test]
fn every_profile_terminal_0f_cell_is_an_exact_invalid_opcode_frontier() {
    for &opcode in PROFILE_TERMINAL_0F_OPCODES {
        let bytes = [0x0F, opcode, 0x84, 0x88, 0, 0, 0, 0, 0xA5];
        let strict =
            lift_single(&bytes).unwrap_or_else(|error| panic!("strict 0F {opcode:02X}: {error:?}"));
        assert_invalid_opcode_trap(&strict, 2);

        let nonstrict = lift_nonstrict(&bytes)
            .unwrap_or_else(|error| panic!("nonstrict 0F {opcode:02X}: {error:?}"));
        assert_invalid_opcode_trap(&nonstrict, 2);
    }
}

#[test]
fn legacy_0f_terminal_preserves_prefix_frontiers() {
    for (bytes, expected_len) in [
        (&[0x0F, 0x24, 0xC0][..], 2),
        (&[0x64, 0x67, 0x48, 0x0F, 0x24, 0xC0][..], 5),
        (&[0xF0, 0x66, 0x0F, 0x37, 0xC0][..], 4),
    ] {
        let strict = lift_single(bytes).expect("strict terminal #UD");
        assert_invalid_opcode_trap(&strict, expected_len);

        let nonstrict = lift_nonstrict(bytes).expect("nonstrict terminal #UD");
        assert_invalid_opcode_trap(&nonstrict, expected_len);
    }
}

#[test]
fn no_two_byte_opcode_reaches_a_generic_unimplemented_diagnostic() {
    for opcode in 0_u8..=u8::MAX {
        let mut bytes = [0_u8; 15];
        bytes[0] = 0x0F;
        bytes[1] = opcode;

        if let Err(LiftError::Unsupported { mnemonic, .. }) = lift_single(&bytes) {
            assert_ne!(
                mnemonic,
                format!("0x0F 0x{opcode:02X}"),
                "legacy two-byte opcode {opcode:#04x} reached the generic fallback"
            );
        }
    }
}
