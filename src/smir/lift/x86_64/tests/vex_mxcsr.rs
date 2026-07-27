//! Strict-lift coverage for LDMXCSR/STMXCSR and VEX WIG variants.

use super::*;

#[test]
fn lift_legacy_and_vex_mxcsr_memory_operations() {
    for (bytes, load, vex, vex_w) in [
        (&[0x0F, 0xAE, 0x10][..], true, false, false),
        (&[0x0F, 0xAE, 0x58, 0x04][..], false, false, false),
        (&[0x66, 0x0F, 0xAE, 0x10][..], true, false, false),
        (&[0xF3, 0x0F, 0xAE, 0x58, 0x04][..], false, false, false),
        (&[0xC5, 0xF8, 0xAE, 0x10][..], true, true, false),
        (&[0xC5, 0xF8, 0xAE, 0x58, 0x04][..], false, true, false),
        (&[0xC4, 0xE1, 0x78, 0xAE, 0x10][..], true, true, false),
        (
            &[0xC4, 0xE1, 0x78, 0xAE, 0x58, 0x04][..],
            false,
            true,
            false,
        ),
        (&[0xC4, 0xE1, 0xF8, 0xAE, 0x10][..], true, true, true),
        (&[0xC4, 0xE1, 0xF8, 0xAE, 0x58, 0x04][..], false, true, true),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(
            result.ops.iter().any(|op| {
                (load && matches!(op.kind, OpKind::X86LoadMxcsr { .. }))
                    || (!load && matches!(op.kind, OpKind::X86StoreMxcsr { .. }))
            }),
            "{bytes:02X?}"
        );
        if vex {
            assert!(matches!(
                result.ops.last().unwrap().x86_hint,
                Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::None,
                    opcode: 0xAE,
                    width: VecWidth::V128,
                    w,
                    ..
                }) if w == vex_w
            ));
        }
    }

    let reserved_register = lift_single(&[0x0F, 0xAE, 0xD0])
        .expect("reserved legacy register /2 must strictly lift to #UD");
    assert_invalid_opcode_trap(&reserved_register, 3);

    for bytes in [
        &[0xC5, 0xFC, 0xAE, 0x10][..],       // VEX.L=1
        &[0xC5, 0xE8, 0xAE, 0x10][..],       // reserved VEX.vvvv
        &[0xC4, 0xE1, 0xFC, 0xAE, 0x10][..], // VEX.W=1, VEX.L=1
        &[0xC4, 0xE1, 0xE8, 0xAE, 0x10][..], // VEX.W=1, reserved VEX.vvvv
        &[0xC4, 0xE1, 0x68, 0xAE, 0x10][..], // VEX.W=0, reserved VEX.vvvv
        &[0xC4, 0xE1, 0xE8, 0xAE, 0x18][..], // VEX.W=1, reserved VEX.vvvv
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted reserved VEX MXCSR encoding {bytes:02X?}"
        );
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0xAE, 0x14, 0x77]).unwrap();
    assert_eq!(addr32.bytes_consumed, 5);
    assert!(addr32.ops[..addr32.ops.len() - 1].iter().all(|op| matches!(
        op.kind,
        OpKind::Shl {
            width: OpWidth::W32,
            flags: FlagUpdate::None,
            ..
        } | OpKind::Add {
            width: OpWidth::W32,
            flags: FlagUpdate::None,
            ..
        }
    )));
    assert!(matches!(
        addr32.ops.last().map(|op| &op.kind),
        Some(OpKind::X86LoadMxcsr { .. })
    ));
}
