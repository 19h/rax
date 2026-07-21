//! Dynamic APX provenance for REX2 instructions modeled as no-effect hints.

use super::*;

fn assert_only_apx_guard(bytes: &[u8]) {
    let result = lift_single(bytes)
        .unwrap_or_else(|error| panic!("REX2 no-effect form {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::X86RequireApx,
            x86_hint: None,
        }]
    ));
}

#[test]
fn every_rex2_no_effect_family_retains_one_dynamic_apx_guard() {
    for bytes in [
        &[0xD5, 0x00, 0x90][..],         // NOP
        &[0xF3, 0xD5, 0x00, 0x90],       // PAUSE
        &[0xD5, 0x00, 0x9B],             // FWAIT
        &[0xD5, 0x80, 0x08],             // INVD
        &[0xD5, 0x80, 0x09],             // WBINVD
        &[0xD5, 0x80, 0x1C, 0xC0],       // CLDEMOTE register hint
        &[0xD5, 0x00, 0xC6, 0xF8, 0x42], // XABORT outside RTM
        &[0xD5, 0x80, 0x01, 0xC1],       // VMCALL hint
        &[0xD5, 0x00, 0xD9, 0xD0],       // FNOP
        &[0xD5, 0x00, 0xDB, 0xE0],       // FENI8087_NOP
        &[0xD5, 0x00, 0xDB, 0xE1],       // FDISI8087_NOP
        &[0xD5, 0x00, 0xDB, 0xE4],       // FSETPM287_NOP
        &[0x66, 0x67, 0xF2, 0x2E, 0xD5, 0x00, 0x9B],
    ] {
        assert_only_apx_guard(bytes);
    }
}

#[test]
fn ignored_rex2_payload_space_retains_exactly_one_apx_guard() {
    for payload in 0x00_u8..=0x7F {
        assert_only_apx_guard(&[0xD5, payload, 0x9B]);
        assert_only_apx_guard(&[0xD5, payload, 0xC6, 0xF8, 0x42]);
        for (opcode, modrm) in [(0xD9, 0xD0), (0xDB, 0xE0), (0xDB, 0xE1), (0xDB, 0xE4)] {
            assert_only_apx_guard(&[0xD5, payload, opcode, modrm]);
        }

        // B4/B3 extend the opcode-register field and turn 90 into XCHG.
        // Every remaining REX2 payload bit is ignored by NOP/PAUSE.
        if payload & 0x11 == 0 {
            assert_only_apx_guard(&[0xD5, payload, 0x90]);
            assert_only_apx_guard(&[0xF3, 0xD5, payload, 0x90]);
        }
    }

    for payload in 0x80_u8..=0xFF {
        assert_only_apx_guard(&[0xD5, payload, 0x08]);
        assert_only_apx_guard(&[0xD5, payload, 0x09]);
        assert_only_apx_guard(&[0xD5, payload, 0x01, 0xC1]);
        assert_only_apx_guard(&[0xD5, payload, 0x1C, 0xC0]);
    }
}

#[test]
fn rex2_cldemote_memory_guard_precedes_address_and_hint_ops() {
    let result = lift_single(&[0x64, 0xD5, 0x80, 0x1C, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12])
        .expect("REX2 CLDEMOTE memory form");
    assert_eq!(result.bytes_consumed, 10);
    assert!(matches!(
        result.ops.first(),
        Some(SmirOp {
            id: OpId(0),
            kind: OpKind::X86RequireApx,
            ..
        })
    ));
    assert!(matches!(
        result.ops.last(),
        Some(SmirOp {
            kind: OpKind::X86CacheControl {
                kind: X86CacheControlKind::Cldemote,
                ..
            },
            ..
        })
    ));
    for (index, op) in result.ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16));
        assert_eq!(op.guest_pc, 0x1000);
    }
}

#[test]
fn legacy_no_effect_forms_remain_operation_free() {
    for bytes in [
        &[0x90][..],
        &[0xF3, 0x90],
        &[0x9B],
        &[0x0F, 0x08],
        &[0x0F, 0x09],
        &[0x0F, 0x1C, 0xC0],
        &[0xC6, 0xF8, 0x42],
        &[0x0F, 0x01, 0xC1],
        &[0xD9, 0xD0],
        &[0xDB, 0xE0],
        &[0xDB, 0xE1],
        &[0xDB, 0xE4],
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(result.ops.is_empty(), "{bytes:02X?}: {:?}", result.ops);
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    }
}

#[test]
fn lock_no_effect_forms_fail_before_any_operand_or_immediate_fetch() {
    for bytes in [
        &[0xF0, 0x90][..],
        &[0xF0, 0xD5, 0x00, 0x90],
        &[0xF0, 0x9B],
        &[0xF0, 0xD5, 0x00, 0x9B],
        &[0xF0, 0x0F, 0x08],
        &[0xF0, 0xD5, 0x80, 0x08],
        &[0xF0, 0x0F, 0x09],
        &[0xF0, 0xD5, 0x80, 0x09],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn rex2_b_extension_keeps_xchg_precedence_over_pause_aliasing() {
    let result = lift_single(&[0xF3, 0xD5, 0x10, 0x90]).expect("REX2 XCHG RAX,R16");
    assert_eq!(result.bytes_consumed, 4);
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Xchg {
                reg1: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                reg2: VReg::Arch(ArchReg::X86(X86Reg::R16)),
                ..
            },
            ..
        }]
    ));
}
