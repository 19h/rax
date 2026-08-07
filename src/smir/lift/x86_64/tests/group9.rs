//! Group 9 (`0F C7`) semantic lifting and prefix-partition tests.

use super::*;

#[test]
fn lift_compacted_xsave_family_group9_encodings_and_legality() {
    for (bytes, expected_kind, rex_w) in [
        (&[0x0F, 0xC7, 0x23][..], X86XSaveKind::XSaveC, false),
        (&[0x48, 0x0F, 0xC7, 0x23][..], X86XSaveKind::XSaveC, true),
        (&[0x0F, 0xC7, 0x2B][..], X86XSaveKind::XSaveS, false),
        (&[0x48, 0x0F, 0xC7, 0x2B][..], X86XSaveKind::XSaveS, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86XSave {
                    rex_w: got_rex,
                    kind,
                    ..
                },
                ..
            }] if *got_rex == rex_w && *kind == expected_kind
        ));
    }
    for (bytes, rex_w) in [
        (&[0x0F, 0xC7, 0x1B][..], false),
        (&[0x48, 0x0F, 0xC7, 0x1B][..], true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86XRstor {
                    rex_w: got_rex,
                    supervisor: true,
                    ..
                },
                ..
            }] if *got_rex == rex_w
        ));
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0xC7, 0x64, 0x4B, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 6);
    let [
        SmirOp {
            kind:
                OpKind::X86XSave {
                    addr,
                    kind: X86XSaveKind::XSaveC,
                    ..
                },
            ..
        },
    ] = addr32.ops.as_slice()
    else {
        panic!("expected one addr32 XSAVEC operation")
    };
    super::addr32_assertions::sib(addr, Some(X86Reg::Rbx), X86Reg::Rcx, 2, 0x20);

    for bytes in [
        &[0xF0, 0x0F, 0xC7, 0x23][..],
        &[0x66, 0x0F, 0xC7, 0x23][..],
        &[0xF2, 0x0F, 0xC7, 0x1B][..],
        &[0xF3, 0x0F, 0xC7, 0x2B][..],
        &[0x0F, 0xC7, 0xD8][..],
        &[0x0F, 0xC7, 0xE0][..],
        &[0x0F, 0xC7, 0xE8][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn lift_group9_cmpxchg_random_seed_rdpid_and_disabled_vmx_partition() {
    for (bytes, wide, locked) in [
        (&[0x0F, 0xC7, 0x0B][..], false, false),
        (&[0x66, 0x0F, 0xC7, 0x0B][..], false, false),
        (&[0x48, 0x0F, 0xC7, 0x0B][..], true, false),
        (&[0xF0, 0x48, 0x0F, 0xC7, 0x0B][..], true, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Cmpxchg8b16b {
                    wide: got_wide,
                    locked: got_locked,
                    ..
                },
                ..
            }] if *got_wide == wide && *got_locked == locked
        ));
    }

    for (bytes, width, seed, register) in [
        (&[0x0F, 0xC7, 0xF0][..], OpWidth::W32, false, X86Reg::Rax),
        (
            &[0x66, 0x0F, 0xC7, 0xF0][..],
            OpWidth::W16,
            false,
            X86Reg::Rax,
        ),
        (
            &[0x48, 0x0F, 0xC7, 0xF0][..],
            OpWidth::W64,
            false,
            X86Reg::Rax,
        ),
        (
            &[0x41, 0x0F, 0xC7, 0xF0][..],
            OpWidth::W32,
            false,
            X86Reg::R8,
        ),
        (&[0x0F, 0xC7, 0xF8][..], OpWidth::W32, true, X86Reg::Rax),
        (
            &[0x66, 0x0F, 0xC7, 0xF8][..],
            OpWidth::W16,
            true,
            X86Reg::Rax,
        ),
        (
            &[0x48, 0x0F, 0xC7, 0xF8][..],
            OpWidth::W64,
            true,
            X86Reg::Rax,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Random {
                    dst: VReg::Arch(ArchReg::X86(got_register)),
                    width: got_width,
                    seed: got_seed,
                },
                ..
            }] if *got_width == width && *got_seed == seed && *got_register == register
        ));
    }

    for bytes in [
        &[0xF3, 0x0F, 0xC7, 0xF8][..],
        &[0x66, 0xF3, 0x0F, 0xC7, 0xF8][..],
        &[0xF3, 0x48, 0x0F, 0xC7, 0xF8][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86ReadPid {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rax))
                },
                ..
            }]
        ));
    }

    for bytes in [
        &[0x0F, 0xC7, 0xC8][..],
        &[0xF2, 0x0F, 0xC7, 0x30][..],
        &[0x66, 0x0F, 0xC7, 0x38][..],
        &[0xF3, 0x0F, 0xC7, 0x38][..],
        &[0xF0, 0x0F, 0xC7, 0xF0][..],
        &[0xF0, 0x0F, 0xC7, 0xF8][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }

    for bytes in [
        &[0x0F, 0xC7, 0x30][..],
        &[0x0F, 0xC7, 0x38][..],
        &[0x66, 0x0F, 0xC7, 0x30][..],
        &[0xF3, 0x0F, 0xC7, 0x30][..],
        &[0x66, 0xF3, 0x0F, 0xC7, 0x30][..],
    ] {
        let result = lift_single(bytes).expect("profile-disabled VMX must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, bytes.len());
    }

    let senduipi = lift_single(&[0xF3, 0x0F, 0xC7, 0xF0]).unwrap();
    assert_eq!(senduipi.bytes_consumed, 4);
    assert!(senduipi.ops.is_empty());
    assert!(matches!(
        senduipi.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));
}
