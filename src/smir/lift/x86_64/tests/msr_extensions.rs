//! Strict-lift coverage for disabled WRMSRNS, RDMSRLIST, and WRMSRLIST.

use super::*;

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("disabled MSR extension must strictly lift to #UD");
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
fn disabled_msr_extensions_strictly_lift_all_legacy_aliases_as_ud() {
    for bytes in [
        &[0x0F, 0x01, 0xC6][..],       // WRMSRNS
        &[0xF2, 0x0F, 0x01, 0xC6][..], // RDMSRLIST
        &[0xF3, 0x0F, 0x01, 0xC6][..], // WRMSRLIST
        &[0x66, 0x0F, 0x01, 0xC6][..],
        &[0x66, 0xF2, 0x0F, 0x01, 0xC6][..],
        &[0x66, 0xF3, 0x0F, 0x01, 0xC6][..],
    ] {
        assert_ud(bytes);
    }
}

#[test]
fn disabled_msr_extensions_preserve_prefix_apx_and_vector_fault_equivalence() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // ignored segment overrides
        0x67, // ignored address-size override
        0x40, 0x48, 0x4F, // ignored ordinary REX fields
    ] {
        assert_ud(&[prefix, 0x0F, 0x01, 0xC6]);
    }

    for bytes in [
        &[0xC5, 0xF8, 0x01, 0xC6][..],             // two-byte VEX
        &[0xC4, 0xE1, 0x78, 0x01, 0xC6][..],       // three-byte VEX
        &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC6][..], // EVEX
        &[0xD5, 0x80, 0x01, 0xC6][..],             // REX2 / APX
    ] {
        assert_ud(bytes);
    }

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0xC6]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn disabled_msr_extension_terminates_strict_blocks_without_fallthrough() {
    let mem = TestMemory::new(0x1000, vec![0x90, 0xF2, 0x0F, 0x01, 0xC6]);
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let block = lifter
        .lift_block(0x1000, &mem, &mut ctx)
        .expect("NOP followed by disabled RDMSRLIST must lift");

    assert!(block.ops.is_empty());
    assert!(matches!(
        block.terminator,
        Terminator::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}
