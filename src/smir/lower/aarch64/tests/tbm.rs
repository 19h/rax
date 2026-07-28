//! Native AArch64 synthesis of AMD TBM scalar operations.

use super::*;
use crate::smir::ir::ops::X86TbmKind;

const DEFINED: FlagSet = FlagSet::CF
    .union(FlagSet::ZF)
    .union(FlagSet::SF)
    .union(FlagSet::OF);

fn tbm_op(kind: X86TbmKind, width: OpWidth, flags: FlagUpdate) -> OpKind {
    OpKind::X86Tbm {
        dst: x86(X86Reg::Rax),
        src: x86(X86Reg::Rcx),
        width,
        kind,
        flags,
    }
}

fn reference(kind: X86TbmKind, src: u64, mask: u64) -> (u64, bool) {
    let src = src & mask;
    let incremented = src.wrapping_add(1) & mask;
    let decremented = src.wrapping_sub(1) & mask;
    let result = match kind {
        X86TbmKind::Blcfill => src & incremented,
        X86TbmKind::Blci => src | !incremented,
        X86TbmKind::Blcic => !src & incremented,
        X86TbmKind::Blcmsk => src ^ incremented,
        X86TbmKind::Blcs => src | incremented,
        X86TbmKind::Blsfill => src | decremented,
        X86TbmKind::Blsic => !src | decremented,
        X86TbmKind::T1mskc => !src | incremented,
        X86TbmKind::Tzmsk => !src & decremented,
    } & mask;
    let carry = if matches!(
        kind,
        X86TbmKind::Blsfill | X86TbmKind::Blsic | X86TbmKind::Tzmsk
    ) {
        src == 0
    } else {
        src == mask
    };
    (result, carry)
}

fn bextr_reference(src: u64, control: u64, width: OpWidth) -> u64 {
    let bits = width.bits();
    let start = (control & 0xff) as u32;
    let length = ((control >> 8) & 0xff) as u32;
    if start >= bits || length == 0 {
        return 0;
    }
    let shifted = (src & width.mask()) >> start;
    let field_bits = length.min(bits - start);
    if field_bits == 64 {
        shifted
    } else {
        shifted & ((1_u64 << field_bits) - 1)
    }
}

#[test]
fn lowers_every_tbm_kind_without_host_xop_dependency() {
    for kind in [
        X86TbmKind::Blcfill,
        X86TbmKind::Blci,
        X86TbmKind::Blcic,
        X86TbmKind::Blcmsk,
        X86TbmKind::Blcs,
        X86TbmKind::Blsfill,
        X86TbmKind::Blsic,
        X86TbmKind::T1mskc,
        X86TbmKind::Tzmsk,
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            let code = lower_single_op(tbm_op(kind, width, FlagUpdate::Specific(DEFINED)));
            assert!(!code.is_empty(), "{kind:?}, {width:?}");
        }
    }
}

#[test]
fn rejects_unmodeled_tbm_widths_and_flag_contracts() {
    for op in [
        tbm_op(
            X86TbmKind::Blcfill,
            OpWidth::W16,
            FlagUpdate::Specific(DEFINED),
        ),
        tbm_op(
            X86TbmKind::Blcfill,
            OpWidth::W64,
            FlagUpdate::Specific(FlagSet::CF),
        ),
    ] {
        assert!(matches!(
            try_lower_single_op(op),
            Err(LowerError::InvalidOperand { .. } | LowerError::UnsupportedOp { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
#[test]
fn native_tbm_matches_reference_for_every_kind_width_and_boundary() {
    let dst = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rax)).unwrap();
    let src = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rcx)).unwrap();

    for kind in [
        X86TbmKind::Blcfill,
        X86TbmKind::Blci,
        X86TbmKind::Blcic,
        X86TbmKind::Blcmsk,
        X86TbmKind::Blcs,
        X86TbmKind::Blsfill,
        X86TbmKind::Blsic,
        X86TbmKind::T1mskc,
        X86TbmKind::Tzmsk,
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            let code = lower_single_op(tbm_op(kind, width, FlagUpdate::Specific(DEFINED)));
            let mask = width.mask();
            for source in [0, 1, 2, 0x7E, 0x7F, mask - 1, mask] {
                let (regs, nzcv, sp) =
                    run_aarch64_code(&code, &[(dst, 0xA5A5_5A5A_DEAD_BEEF), (src, source)], 0xF);
                let (expected, carry) = reference(kind, source, mask);
                assert_eq!(
                    regs[dst as usize], expected,
                    "{kind:?}, {width:?}, {source:#x}"
                );
                assert_eq!(regs[src as usize], source, "{kind:?}: source");
                let expected_nzcv = (u8::from(expected & (1 << (width.bits() - 1)) != 0) << 3)
                    | (u8::from(expected == 0) << 2)
                    | (u8::from(carry) << 1);
                assert_eq!(nzcv, expected_nzcv, "{kind:?}, {width:?}, {source:#x}");
                assert_eq!(sp, 0x8000);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
#[test]
fn native_tbm_flag_suppression_preserves_nzcv() {
    let dst = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rax)).unwrap();
    let src = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rcx)).unwrap();
    let code = lower_single_op(tbm_op(X86TbmKind::Blcmsk, OpWidth::W64, FlagUpdate::None));
    let (regs, nzcv, sp) = run_aarch64_code(
        &code,
        &[(dst, 0xA5A5_5A5A_DEAD_BEEF), (src, 0xFFFF_FFFD)],
        0xB,
    );
    assert_eq!(regs[dst as usize], 3);
    assert_eq!(nzcv, 0xB);
    assert_eq!(sp, 0x8000);
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
#[test]
fn native_tbm_immediate_bextr_preserves_undefined_n_and_matches_boundaries() {
    let dst = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rax)).unwrap();
    let src = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rcx)).unwrap();
    let source = 0xFEDC_BA98_7654_3210;

    for width in [OpWidth::W32, OpWidth::W64] {
        for control in [0, 0x0804, 0x0840, 0x4004] {
            let code = lower_single_op(OpKind::Bextr {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                control: VReg::Imm(control),
                width,
                flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
            });
            let (regs, nzcv, sp) =
                run_aarch64_code(&code, &[(dst, 0xA5A5_5A5A_DEAD_BEEF), (src, source)], 0xB);
            let expected = bextr_reference(source, control as u64, width);
            assert_eq!(regs[dst as usize], expected, "{width:?}, {control:#06x}");
            assert_eq!(regs[src as usize], source, "{width:?}, source");
            assert_eq!(
                nzcv,
                0b1000 | (u8::from(expected == 0) << 2),
                "{width:?}, {control:#06x}: N preserved, Z produced, C/V cleared"
            );
            assert_eq!(sp, 0x8000);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
#[test]
fn native_tbm_subset_supports_rsp_rbp_identity_slots() {
    let rsp = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rsp)).unwrap();
    let rbp = Aarch64Lowerer::gpr_arm_or_x86(x86(X86Reg::Rbp)).unwrap();
    let source = 0xFEDC_BA98_7654_3210;

    let tbm_code = lower_single_op(OpKind::X86Tbm {
        dst: x86(X86Reg::Rsp),
        src: x86(X86Reg::Rbp),
        width: OpWidth::W64,
        kind: X86TbmKind::Blcfill,
        flags: FlagUpdate::Specific(DEFINED),
    });
    let (regs, nzcv, sp) = run_aarch64_code(
        &tbm_code,
        &[(rsp, 0xA5A5_5A5A_DEAD_BEEF), (rbp, source)],
        0xF,
    );
    let (expected_tbm, carry) = reference(X86TbmKind::Blcfill, source, u64::MAX);
    assert_eq!(regs[rsp as usize], expected_tbm);
    assert_eq!(regs[rbp as usize], source);
    assert_eq!(
        nzcv,
        (u8::from(expected_tbm & (1 << 63) != 0) << 3)
            | (u8::from(expected_tbm == 0) << 2)
            | (u8::from(carry) << 1)
    );
    assert_eq!(sp, 0x8000);

    let control = 0x0804;
    let bextr_code = lower_single_op(OpKind::Bextr {
        dst: x86(X86Reg::Rbp),
        src: x86(X86Reg::Rbp),
        control: VReg::Imm(control),
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
    });
    let (regs, nzcv, sp) = run_aarch64_code(&bextr_code, &[(rbp, source)], 0xB);
    let expected_bextr = bextr_reference(source, control as u64, OpWidth::W64);
    assert_eq!(regs[rbp as usize], expected_bextr);
    assert_eq!(nzcv, 0b1000 | (u8::from(expected_bextr == 0) << 2));
    assert_eq!(sp, 0x8000);
}
