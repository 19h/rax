//! Exhaustive legacy `0F 3A` opcode-frontier coverage.

use super::*;

const LIFTED_LEGACY_0F3A_OPCODES: &[u8] = &[
    0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x14, 0x15, 0x16, 0x17, 0x20, 0x21, 0x22, 0x40,
    0x41, 0x42, 0x44, 0x60, 0x61, 0x62, 0x63, 0xCC, 0xCE, 0xCF, 0xDF,
];

fn lift_nonstrict(bytes: &[u8]) -> Result<LiftResult, LiftError> {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter.lift_insn(0x1000, bytes, &mut ctx)
}

#[test]
fn every_unhandled_legacy_0f3a_opcode_is_terminal_at_the_opcode_frontier() {
    for opcode in 0_u8..=u8::MAX {
        if LIFTED_LEGACY_0F3A_OPCODES.contains(&opcode) {
            continue;
        }

        // Every legacy encoding assigned by Intel SDM Vol. 2 is dispatched
        // above. Remaining assigned map cells require VEX or EVEX; their
        // legacy spellings and all unassigned cells raise #UD without fetching
        // a ModR/M or immediate byte.
        let bytes = [0x0F, 0x3A, opcode, 0x84, 0x88, 0, 0, 0, 0xA5];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("legacy 0F 3A {opcode:02X}: {error:?}"));
        assert_invalid_opcode_trap(&result, 3);
    }
}

#[test]
fn legacy_0f3a_terminal_is_mode_independent_and_preserves_prefix_frontiers() {
    for (bytes, expected_len) in [
        (&[0x0F, 0x3A, 0x00, 0xC0, 0xA5][..], 3),
        (&[0x64, 0x67, 0x48, 0x0F, 0x3A, 0x00, 0xC0][..], 6),
        (&[0xF0, 0x66, 0x0F, 0x3A, 0x00, 0xC0][..], 5),
    ] {
        let strict = lift_single(bytes).expect("strict terminal #UD");
        assert_invalid_opcode_trap(&strict, expected_len);

        let nonstrict = lift_nonstrict(bytes).expect("non-strict terminal #UD");
        assert_invalid_opcode_trap(&nonstrict, expected_len);
    }

    assert!(matches!(
        lift_single(&[0x0F, 0x3A]),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 2,
            need: 3,
        })
    ));
    assert!(matches!(
        lift_single(&[0x64, 0x67, 0x0F, 0x3A]),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 4,
            need: 5,
        })
    ));
}
