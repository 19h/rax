//! Legacy and APX ADX lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_adx_legacy_prefixes_like_llvm() {
    for (bytes, name, kind, width) in [
        (
            &[0x66, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adcxl",
            X86AdxKind::Adcx,
            OpWidth::W32,
        ),
        (
            &[0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adcxq",
            X86AdxKind::Adcx,
            OpWidth::W64,
        ),
        (
            &[0xF3, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adoxl",
            X86AdxKind::Adox,
            OpWidth::W32,
        ),
        (
            &[0xF3, 0x48, 0x0F, 0x38, 0xF6, 0xC3][..],
            "adoxq",
            X86AdxKind::Adox,
            OpWidth::W64,
        ),
        (
            &[0x66, 0xF3, 0x0F, 0x38, 0xF6, 0xC3][..],
            "66+f3 adoxl",
            X86AdxKind::Adox,
            OpWidth::W32,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_adx_sequence(&result, 0, kind, x86_gpr(0), x86_gpr(0), x86_gpr(3), width);
    }
}

#[test]
fn lift_adx_apx_nd_uses_vvvv_destination_like_llvm() {
    // LLVM 20: `adcxq %rbx, %rax, %r8` => 62 f4 bd 18 66 c3.
    let result = lift_single(&[0x62, 0xF4, 0xBD, 0x18, 0x66, 0xC3]).unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_adx_sequence(
        &result,
        0,
        X86AdxKind::Adcx,
        x86_gpr(8),
        x86_gpr(0),
        x86_gpr(3),
        OpWidth::W64,
    );

    // LLVM accepts nonzero EVEX aaa bits for this opcode and ignores them.
    let result = lift_single(&[0x62, 0xF4, 0xBD, 0x19, 0x66, 0xC3]).unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_adx_sequence(
        &result,
        0,
        X86AdxKind::Adcx,
        x86_gpr(8),
        x86_gpr(0),
        x86_gpr(3),
        OpWidth::W64,
    );

    // LLVM 20: `adoxq 32(%r17,%r18,4), %r19, %r20`
    // => 62 ec da 10 66 5c 91 20.
    let result = lift_single(&[0x62, 0xEC, 0xDA, 0x10, 0x66, 0x5C, 0x91, 0x20]).unwrap();
    assert_eq!(result.bytes_consumed, 8);
    let mem_src = match &result.ops[0].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    ..
                },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(17));
            assert_eq!(*index, x86_gpr(18));
            *dst
        }
        other => panic!("expected APX ADOX memory load, got {other:?}"),
    };
    assert_adx_sequence(
        &result,
        1,
        X86AdxKind::Adox,
        x86_gpr(20),
        x86_gpr(19),
        mem_src,
        OpWidth::W64,
    );
}

#[test]
fn lift_adx_rejects_invalid_forms_like_llvm() {
    for (bytes, name) in [
        (&[0xF2, 0x0F, 0x38, 0xF6, 0xC3][..], "legacy f2 prefix"),
        (
            &[0xF2, 0x66, 0x0F, 0x38, 0xF6, 0xC3][..],
            "legacy f2 plus 66 prefixes",
        ),
    ] {
        let err = lift_single(bytes).expect_err(name);
        assert!(
            matches!(err, LiftError::InvalidEncoding { .. }),
            "{name}: {err:?}"
        );
    }

    for (bytes, name) in [
        (&[0x62, 0xF4, 0xBD, 0x08, 0x66, 0xC3][..], "APX missing ND"),
        (&[0x62, 0xF4, 0xBD, 0x1C, 0x66, 0xC3][..], "APX NF reserved"),
        (&[0x62, 0xF4, 0xBC, 0x18, 0x66, 0xC3][..], "APX pp none"),
        (&[0x62, 0xF4, 0xBF, 0x18, 0x66, 0xC3][..], "APX pp 3"),
        (
            &[0x62, 0xF4, 0xBD, 0x98, 0x66, 0xC3][..],
            "APX z bit reserved",
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| {
            panic!("{name}: reserved APX ADX form must strictly lift to #UD: {error:?}")
        });
        assert_eq!(result.bytes_consumed, 5, "{name}");
        assert!(result.ops.is_empty(), "{name}");
        assert!(
            matches!(
                result.control_flow,
                ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode
                }
            ),
            "{name}: {:?}",
            result.control_flow
        );
    }
}
