//! Intel APX Map 4 dispatch-frontier regression tests.

use super::*;

fn assert_map4_needs_modrm(bytes: &[u8]) {
    assert!(matches!(
        lift_single(bytes),
        Err(LiftError::Incomplete {
            addr: 0x1000,
            have: 5,
            need: 6
        })
    ));
}

fn assert_map4_opcode_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("profile-disabled APX CET form must strictly lift");
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
fn non_target_map4_families_preserve_their_modrm_frontier() {
    // Valid ADC without NF still needs /r.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0xBC, 0x18, 0x11]);

    // A valid APX ADCX still needs /r.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0xBD, 0x18, 0x66]);

    // F2/F3 F8 families remain independent implementation gaps.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0x7F, 0x08, 0xF8]);
    assert_map4_needs_modrm(&[0x62, 0xF4, 0x7E, 0x08, 0xF8]);
}

#[test]
fn profile_disabled_apx_cet_forms_are_terminal_at_the_opcode_frontier() {
    for bytes in [
        &[0x62, 0xF4, 0x7C, 0x08, 0x66][..], // WRSSD
        &[0x62, 0xF4, 0xFC, 0x08, 0x66][..], // WRSSQ
        &[0x62, 0xF4, 0x7D, 0x08, 0x65][..], // WRUSSD
        &[0x62, 0xF4, 0xFD, 0x08, 0x65][..], // WRUSSQ
    ] {
        assert_map4_opcode_ud(bytes);
    }
}

#[test]
fn valid_unimplemented_f8_families_remain_explicit_fallbacks() {
    for bytes in [
        &[0x62, 0xF4, 0x7F, 0x08, 0xF8, 0xC0][..], // URDMSR rax,rax
        &[0x62, 0xF4, 0x7E, 0x08, 0xF8, 0xC0][..], // UWRMSR rax,rax
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Unsupported { .. })
        ));
    }
}
