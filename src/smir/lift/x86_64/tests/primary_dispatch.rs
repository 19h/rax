//! Exhaustive legacy primary-opcode ingress coverage.

use super::*;

#[test]
fn every_primary_opcode_is_dispatched_or_intercepted_before_legacy_match() {
    for opcode in 0_u8..=u8::MAX {
        // Fifteen bytes make every immediate/displacement decoder complete
        // while retaining the architectural maximum instruction length.
        let mut bytes = [0_u8; 15];
        bytes[0] = opcode;

        if let Err(LiftError::Unsupported { mnemonic, .. }) = lift_single(&bytes) {
            assert_ne!(
                mnemonic,
                format!("0x{opcode:02X}"),
                "primary opcode {opcode:#04x} reached the generic legacy fallback"
            );
        }
    }
}
