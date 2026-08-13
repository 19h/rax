//! AArch64-host lowering coverage for x86 register `XCHG` shapes that use APX
//! extended GPRs.

use super::*;

#[test]
fn lowers_xchg_apx_egpr_operands_runtime() {
    let code = lower_ops(vec![
        OpKind::Xchg {
            reg1: x86(X86Reg::R16),
            reg2: x86(X86Reg::R17),
            width: OpWidth::W64,
        },
        OpKind::Xchg {
            reg1: x86(X86Reg::R18),
            reg2: x86(X86Reg::R19),
            width: OpWidth::W16,
        },
        OpKind::Xchg {
            reg1: x86(X86Reg::R20),
            reg2: x86(X86Reg::R21),
            width: OpWidth::W8,
        },
        OpKind::Xchg {
            reg1: x86(X86Reg::R22),
            reg2: x86(X86Reg::R22),
            width: OpWidth::W8,
        },
    ]);
    let regs = [
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
        (18, 0x1111_2222_3333_abcd),
        (19, 0x9999_8888_7777_1234),
        (20, 0xffff_ffff_ffff_00f0),
        (21, 0x2121_2121_2121_2121),
        (22, 0x2222_2222_2222_22a5),
    ];
    let old_nzcv = 0b0101;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[16], 0x1717_1717_1717_1717);
    assert_eq!(out[17], 0x1616_1616_1616_1616);
    assert_eq!(out[18], 0x1111_2222_3333_1234);
    assert_eq!(out[19], 0x9999_8888_7777_abcd);
    assert_eq!(out[20], 0xffff_ffff_ffff_0021);
    assert_eq!(out[21], 0x2121_2121_2121_21f0);
    assert_eq!(out[22], 0x2222_2222_2222_22a5);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}

#[test]
fn rejects_xchg_apx_r31_identity_mapping() {
    for kind in [
        OpKind::Xchg {
            reg1: x86(X86Reg::R31),
            reg2: x86(X86Reg::R16),
            width: OpWidth::W64,
        },
        OpKind::Xchg {
            reg1: x86(X86Reg::R16),
            reg2: x86(X86Reg::R31),
            width: OpWidth::W16,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
