//! Strict-lift coverage for PCONFIG in the non-PCONFIG guest profile.

use super::*;

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("disabled PCONFIG must strictly lift to #UD");
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
fn disabled_pconfig_strictly_lifts_as_an_exact_invalid_opcode_trap() {
    assert_ud(&[0x0F, 0x01, 0xC5]);
}

#[test]
fn disabled_pconfig_preserves_prefix_and_apx_fault_equivalence() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // ignored segment overrides
        0x67, // ignored address-size override
        0x40, 0x48, 0x4F, // ignored ordinary REX fields
        0x66, 0xF2, 0xF3, // explicitly invalid PCONFIG prefixes
    ] {
        assert_ud(&[prefix, 0x0F, 0x01, 0xC5]);
    }

    for bytes in [
        &[0xC5, 0xF8, 0x01, 0xC5][..],             // two-byte VEX
        &[0xC4, 0xE1, 0x78, 0x01, 0xC5][..],       // three-byte VEX
        &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC5][..], // EVEX
    ] {
        assert_ud(bytes);
    }

    // PCONFIG remains absent whether APX is disabled or enabled. A REX2
    // decode failure and the subsequent feature check therefore converge on
    // the same precise #UD without exposing a dynamic architectural input.
    assert_ud(&[0xD5, 0x80, 0x01, 0xC5]);

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0xC5]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn disabled_pconfig_terminates_strict_blocks_without_fallthrough() {
    let mem = TestMemory::new(0x1000, vec![0x90, 0x0F, 0x01, 0xC5]);
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let block = lifter
        .lift_block(0x1000, &mem, &mut ctx)
        .expect("NOP followed by disabled PCONFIG must lift");

    assert!(block.ops.is_empty());
    assert!(matches!(
        block.terminator,
        Terminator::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}
