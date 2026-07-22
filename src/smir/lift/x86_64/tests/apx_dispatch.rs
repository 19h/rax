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

#[test]
fn non_target_map4_families_preserve_their_modrm_frontier() {
    // Valid ADC without NF still needs /r.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0xBC, 0x18, 0x11]);

    // WRUSS (65 /r) and the F2/F3 F8 families are architecturally valid but
    // remain independent implementation gaps. Removing the global ModR/M
    // check for NF-invalid ADC/SBB must not reclassify their frontiers.
    assert_map4_needs_modrm(&[0x62, 0xF4, 0x7D, 0x08, 0x65]);
    assert_map4_needs_modrm(&[0x62, 0xF4, 0x7F, 0x08, 0xF8]);
    assert_map4_needs_modrm(&[0x62, 0xF4, 0x7E, 0x08, 0xF8]);
}

#[test]
fn valid_unimplemented_map4_families_remain_explicit_fallbacks() {
    for bytes in [
        &[0x62, 0xF4, 0x7D, 0x08, 0x65, 0x00][..], // WRUSSD [rax],eax
        &[0x62, 0xF4, 0x7F, 0x08, 0xF8, 0xC0][..], // URDMSR rax,rax
        &[0x62, 0xF4, 0x7E, 0x08, 0xF8, 0xC0][..], // UWRMSR rax,rax
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Unsupported { .. })
        ));
    }
}
