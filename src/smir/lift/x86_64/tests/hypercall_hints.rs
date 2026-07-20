//! Strict-lift coverage for the deterministic VMCALL/VMMCALL hint profile.

use super::*;

fn assert_hint_noop(bytes: &[u8]) {
    let result = lift_single(bytes).expect("configured hypercall hint must strictly lift");
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(result.ops.is_empty(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
}

fn assert_ud(bytes: &[u8]) {
    let result = lift_single(bytes).expect("unsupported virtualization alias must lift to #UD");
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
fn vmcall_and_vmmcall_strictly_lift_as_exact_no_operations() {
    assert_hint_noop(&[0x0F, 0x01, 0xC1]);
    assert_hint_noop(&[0x0F, 0x01, 0xD9]);
}

#[test]
fn hypercall_hints_accept_only_semantically_ignored_legacy_and_rex_prefixes() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, 0x4F, // representative ordinary REX forms
    ] {
        assert_hint_noop(&[prefix, 0x0F, 0x01, 0xC1]);
        assert_hint_noop(&[prefix, 0x0F, 0x01, 0xD9]);
    }

    // Intel assigns no F2/F3 aliases to VMCALL, so repeat prefixes remain
    // ignored. AMD assigns both encodings to VMGEXIT instead of VMMCALL.
    assert_hint_noop(&[0xF2, 0x0F, 0x01, 0xC1]);
    assert_hint_noop(&[0xF3, 0x0F, 0x01, 0xC1]);
    assert_ud(&[0xF2, 0x0F, 0x01, 0xD9]);
    assert_ud(&[0xF3, 0x0F, 0x01, 0xD9]);

    for modrm in [0xC1, 0xD9] {
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x01, modrm]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn hypercall_hints_keep_rex2_feature_and_vendor_boundaries_fail_closed() {
    // Intel APX permits a REX2-compressed VMCALL only when APX is enabled.
    // Empty-op SMIR has no dynamic APX feature guard, so this form must remain
    // an interpreter frontier rather than bypassing the direct decoder's #UD.
    assert!(matches!(
        lift_single(&[0xD5, 0x80, 0x01, 0xC1]),
        Err(LiftError::Unsupported { .. })
    ));

    // VMMCALL is an AMD instruction, while REX2 is Intel APX. The compressed
    // D9 form is undefined on both vendor profiles and can be modeled exactly.
    assert_ud(&[0xD5, 0x80, 0x01, 0xD9]);
}

#[test]
fn hypercall_hints_do_not_split_a_strict_lifted_block() {
    let ops = lift_one(&[
        0x0F, 0x01, 0xC1, // VMCALL
        0x0F, 0x01, 0xD9, // VMMCALL
        0x90, // NOP
    ])
    .expect("configured hypercall hints must not force interpreter fallback");
    assert!(ops.is_empty());
}
