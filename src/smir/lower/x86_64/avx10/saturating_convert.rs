//! AVX10.2 MAP5 saturating-conversion lowering.

use super::*;
use crate::smir::ir::ops::{X86SatFpFormat, x86_sat_fp_to_int_controls, x86_sat_fp_to_int_widths};

impl Avx10Lowerer {
    pub(super) fn lower_x86_scalar_fp_to_int_sat(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        elem: VecElementType,
        int_width: OpWidth,
        signed: bool,
        suppress_exceptions: bool,
    ) -> Avx10LowerResult<()> {
        if !matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))) if *index < 32) {
            return Err(LowerError::UnsupportedOperation(
                "Scalar saturation conversion: source must be XMM0-XMM31".to_string(),
            ));
        }
        let dst_reg = self.vreg_to_gpr(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let pp = match elem {
            VecElementType::F32 => 2,
            VecElementType::F64 => 3,
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "Scalar saturation conversion: source must be binary32 or binary64".to_string(),
                ));
            }
        };
        let w = match int_width {
            OpWidth::W32 => false,
            OpWidth::W64 => true,
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "Scalar saturation conversion: result must be 32 or 64 bits".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex_with_b(
            5,
            pp,
            w,
            VecWidth::V128,
            dst_reg,
            0,
            src_reg,
            0,
            false,
            suppress_exceptions,
            Some(0),
        );
        enc.emit_opcode(if signed { 0x6D } else { 0x6C });
        enc.emit_modrm_rr(dst_reg, src_reg);
        Ok(())
    }

    pub(super) fn lower_vcvt_fp_to_int_sat(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        mask: Option<&VReg>,
        fp_format: X86SatFpFormat,
        int_elem: VecElementType,
        width: VecWidth,
        signed: bool,
        truncate: bool,
        round: FpRoundMode,
        zeroing: bool,
        suppress_exceptions: bool,
    ) -> Avx10LowerResult<()> {
        let vector_matches_width = |reg: &VReg, reg_width: VecWidth| {
            matches!(
                (reg, reg_width),
                (VReg::Arch(ArchReg::X86(X86Reg::Xmm(_))), VecWidth::V64)
                    | (VReg::Arch(ArchReg::X86(X86Reg::Xmm(_))), VecWidth::V128)
                    | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(_))), VecWidth::V256)
                    | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(_))), VecWidth::V512)
            )
        };
        let Some((src_width, encoded_width)) =
            x86_sat_fp_to_int_widths(fp_format, int_elem, width, truncate)
        else {
            return Err(LowerError::UnsupportedOperation(
                "Saturation conversion: invalid types or payload width".to_string(),
            ));
        };
        if !vector_matches_width(dst, width)
            || !vector_matches_width(src, src_width)
            || (zeroing && mask.is_none())
            || !x86_sat_fp_to_int_controls(
                fp_format,
                truncate,
                round,
                suppress_exceptions,
                encoded_width,
            )
        {
            return Err(LowerError::UnsupportedOperation(
                "Saturation conversion: invalid vector, mask, or rounding shape".to_string(),
            ));
        }
        let dst_reg = self.vreg_to_zmm(dst)?;
        let src_reg = self.vreg_to_zmm(src)?;
        let mask_reg = match mask {
            Some(mask) => {
                let reg = self.vreg_to_k(mask)?;
                if reg == 0 {
                    return Err(LowerError::UnsupportedOperation(
                        "Saturation conversion: k0 cannot encode a writemask".to_string(),
                    ));
                }
                reg
            }
            None => 0,
        };

        let (pp, opcode, w) = match (fp_format, int_elem, signed, truncate) {
            (X86SatFpFormat::F16, VecElementType::I8, true, true) => (0, 0x68, false),
            (X86SatFpFormat::F16, VecElementType::I8, true, false) => (0, 0x69, false),
            (X86SatFpFormat::F16, VecElementType::I8, false, true) => (0, 0x6A, false),
            (X86SatFpFormat::F16, VecElementType::I8, false, false) => (0, 0x6B, false),
            (X86SatFpFormat::BF16, VecElementType::I8, true, true) => (3, 0x68, false),
            (X86SatFpFormat::BF16, VecElementType::I8, true, false) => (3, 0x69, false),
            (X86SatFpFormat::BF16, VecElementType::I8, false, true) => (3, 0x6A, false),
            (X86SatFpFormat::BF16, VecElementType::I8, false, false) => (3, 0x6B, false),
            (X86SatFpFormat::F32, VecElementType::I8, true, true) => (1, 0x68, false),
            (X86SatFpFormat::F32, VecElementType::I8, true, false) => (1, 0x69, false),
            (X86SatFpFormat::F32, VecElementType::I8, false, true) => (1, 0x6A, false),
            (X86SatFpFormat::F32, VecElementType::I8, false, false) => (1, 0x6B, false),
            (X86SatFpFormat::F32, VecElementType::I32, true, true) => (0, 0x6D, false),
            (X86SatFpFormat::F32, VecElementType::I32, false, true) => (0, 0x6C, false),
            (X86SatFpFormat::F32, VecElementType::I64, true, true) => (1, 0x6D, false),
            (X86SatFpFormat::F32, VecElementType::I64, false, true) => (1, 0x6C, false),
            (X86SatFpFormat::F64, VecElementType::I32, true, true) => (0, 0x6D, true),
            (X86SatFpFormat::F64, VecElementType::I32, false, true) => (0, 0x6C, true),
            (X86SatFpFormat::F64, VecElementType::I64, true, true) => (1, 0x6D, true),
            (X86SatFpFormat::F64, VecElementType::I64, false, true) => (1, 0x6C, true),
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "Saturation conversion: invalid types".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        let ll_override = if suppress_exceptions {
            Some(if truncate {
                0
            } else {
                match round {
                    FpRoundMode::RoundNearest => 0,
                    FpRoundMode::RoundDown => 1,
                    FpRoundMode::RoundUp => 2,
                    FpRoundMode::RoundTowardZero => 3,
                    _ => unreachable!("rounding shape validated above"),
                }
            })
        } else {
            None
        };
        enc.emit_evex_with_b(
            5, // MAP5
            pp,
            w,
            encoded_width,
            dst_reg,
            0, // EVEX.vvvv is reserved and encodes 1111b.
            src_reg,
            mask_reg,
            zeroing,
            suppress_exceptions,
            ll_override,
        );
        enc.emit_opcode(opcode);
        enc.emit_modrm_rr(dst_reg, src_reg);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_map5_masks_sae_and_rejects_bad_shapes() {
        let lowerer = Avx10Lowerer::new();
        let xmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Xmm(n)));
        let ymm = |n| VReg::Arch(ArchReg::X86(X86Reg::Ymm(n)));
        let zmm = |n| VReg::Arch(ArchReg::X86(X86Reg::Zmm(n)));
        let k = |n| VReg::Arch(ArchReg::X86(X86Reg::K(n)));
        let op =
            |dst, src, mask, fp_format, int_elem, width, signed, zeroing, suppress_exceptions| {
                OpKind::VCvtFpToIntSat {
                    dst,
                    src,
                    mask,
                    fp_elem: fp_format,
                    int_elem,
                    width,
                    signed,
                    truncate: true,
                    round: FpRoundMode::RoundTowardZero,
                    zeroing,
                    suppress_exceptions,
                }
            };
        let rounded = |dst, src, mask, width, signed, round, zeroing, suppress_exceptions| {
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask,
                fp_elem: X86SatFpFormat::F32,
                int_elem: VecElementType::I8,
                width,
                signed,
                truncate: false,
                round,
                zeroing,
                suppress_exceptions,
            }
        };
        let shaped = |dst, src, fp_format, int_elem, width, signed, suppress_exceptions| {
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask: None,
                fp_elem: fp_format,
                int_elem,
                width,
                signed,
                truncate: true,
                round: FpRoundMode::RoundTowardZero,
                zeroing: false,
                suppress_exceptions,
            }
        };

        for (kind, expected) in [
            (
                op(
                    xmm(1),
                    xmm(2),
                    None,
                    X86SatFpFormat::F32,
                    VecElementType::I8,
                    VecWidth::V128,
                    true,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0x7D, 0x08, 0x68, 0xCA][..],
            ),
            (
                op(
                    ymm(1),
                    ymm(2),
                    None,
                    X86SatFpFormat::F32,
                    VecElementType::I8,
                    VecWidth::V256,
                    false,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0x7D, 0x28, 0x6A, 0xCA][..],
            ),
            (
                op(
                    xmm(1),
                    xmm(2),
                    None,
                    X86SatFpFormat::F16,
                    VecElementType::I8,
                    VecWidth::V128,
                    true,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0x7C, 0x08, 0x68, 0xCA][..],
            ),
            (
                OpKind::VCvtFpToIntSat {
                    dst: zmm(17),
                    src: zmm(18),
                    mask: Some(k(3)),
                    fp_elem: X86SatFpFormat::F16,
                    int_elem: VecElementType::I8,
                    width: VecWidth::V512,
                    signed: true,
                    truncate: false,
                    round: FpRoundMode::RoundUp,
                    zeroing: true,
                    suppress_exceptions: true,
                },
                &[0x62, 0xA5, 0x7C, 0xDB, 0x69, 0xCA][..],
            ),
            (
                OpKind::VCvtFpToIntSat {
                    dst: ymm(1),
                    src: ymm(2),
                    mask: None,
                    fp_elem: X86SatFpFormat::BF16,
                    int_elem: VecElementType::I8,
                    width: VecWidth::V256,
                    signed: false,
                    truncate: false,
                    round: FpRoundMode::RoundNearest,
                    zeroing: false,
                    suppress_exceptions: false,
                },
                &[0x62, 0xF5, 0x7F, 0x28, 0x6B, 0xCA][..],
            ),
            (
                op(
                    zmm(17),
                    zmm(18),
                    Some(k(3)),
                    X86SatFpFormat::BF16,
                    VecElementType::I8,
                    VecWidth::V512,
                    false,
                    true,
                    false,
                ),
                &[0x62, 0xA5, 0x7F, 0xCB, 0x6A, 0xCA][..],
            ),
            (
                op(
                    zmm(1),
                    zmm(2),
                    None,
                    X86SatFpFormat::F64,
                    VecElementType::I64,
                    VecWidth::V512,
                    true,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0xFD, 0x48, 0x6D, 0xCA][..],
            ),
            (
                op(
                    zmm(17),
                    zmm(18),
                    Some(k(3)),
                    X86SatFpFormat::F64,
                    VecElementType::I64,
                    VecWidth::V512,
                    false,
                    true,
                    false,
                ),
                &[0x62, 0xA5, 0xFD, 0xCB, 0x6C, 0xCA][..],
            ),
            (
                op(
                    zmm(1),
                    zmm(2),
                    None,
                    X86SatFpFormat::F32,
                    VecElementType::I8,
                    VecWidth::V512,
                    true,
                    false,
                    true,
                ),
                &[0x62, 0xF5, 0x7D, 0x18, 0x68, 0xCA][..],
            ),
            (
                shaped(
                    xmm(1),
                    xmm(2),
                    X86SatFpFormat::F64,
                    VecElementType::I32,
                    VecWidth::V64,
                    true,
                    false,
                ),
                &[0x62, 0xF5, 0xFC, 0x08, 0x6D, 0xCA][..],
            ),
            (
                shaped(
                    xmm(1),
                    ymm(2),
                    X86SatFpFormat::F64,
                    VecElementType::I32,
                    VecWidth::V128,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0xFC, 0x28, 0x6C, 0xCA][..],
            ),
            (
                shaped(
                    ymm(1),
                    zmm(2),
                    X86SatFpFormat::F64,
                    VecElementType::I32,
                    VecWidth::V256,
                    true,
                    true,
                ),
                &[0x62, 0xF5, 0xFC, 0x18, 0x6D, 0xCA][..],
            ),
            (
                shaped(
                    zmm(1),
                    zmm(2),
                    X86SatFpFormat::F32,
                    VecElementType::I32,
                    VecWidth::V512,
                    true,
                    false,
                ),
                &[0x62, 0xF5, 0x7C, 0x48, 0x6D, 0xCA][..],
            ),
            (
                shaped(
                    xmm(1),
                    xmm(2),
                    X86SatFpFormat::F32,
                    VecElementType::I64,
                    VecWidth::V128,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0x7D, 0x08, 0x6C, 0xCA][..],
            ),
            (
                shaped(
                    ymm(1),
                    xmm(2),
                    X86SatFpFormat::F32,
                    VecElementType::I64,
                    VecWidth::V256,
                    true,
                    false,
                ),
                &[0x62, 0xF5, 0x7D, 0x28, 0x6D, 0xCA][..],
            ),
            (
                shaped(
                    zmm(1),
                    ymm(2),
                    X86SatFpFormat::F32,
                    VecElementType::I64,
                    VecWidth::V512,
                    false,
                    true,
                ),
                &[0x62, 0xF5, 0x7D, 0x18, 0x6C, 0xCA][..],
            ),
            (
                rounded(
                    xmm(1),
                    xmm(2),
                    None,
                    VecWidth::V128,
                    true,
                    FpRoundMode::Dynamic,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0x7D, 0x08, 0x69, 0xCA][..],
            ),
            (
                rounded(
                    ymm(1),
                    ymm(2),
                    None,
                    VecWidth::V256,
                    false,
                    FpRoundMode::Dynamic,
                    false,
                    false,
                ),
                &[0x62, 0xF5, 0x7D, 0x28, 0x6B, 0xCA][..],
            ),
            (
                rounded(
                    zmm(1),
                    zmm(2),
                    None,
                    VecWidth::V512,
                    true,
                    FpRoundMode::RoundDown,
                    false,
                    true,
                ),
                &[0x62, 0xF5, 0x7D, 0x38, 0x69, 0xCA][..],
            ),
            (
                rounded(
                    zmm(1),
                    zmm(2),
                    None,
                    VecWidth::V512,
                    true,
                    FpRoundMode::RoundNearest,
                    false,
                    true,
                ),
                &[0x62, 0xF5, 0x7D, 0x18, 0x69, 0xCA][..],
            ),
            (
                rounded(
                    zmm(1),
                    zmm(2),
                    None,
                    VecWidth::V512,
                    true,
                    FpRoundMode::RoundUp,
                    false,
                    true,
                ),
                &[0x62, 0xF5, 0x7D, 0x58, 0x69, 0xCA][..],
            ),
            (
                rounded(
                    zmm(17),
                    zmm(18),
                    Some(k(3)),
                    VecWidth::V512,
                    false,
                    FpRoundMode::RoundTowardZero,
                    true,
                    true,
                ),
                &[0x62, 0xA5, 0x7D, 0xFB, 0x6B, 0xCA][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer
                .try_lower(&kind, &mut code)
                .expect("saturating conversion must be recognized")
                .unwrap();
            assert_eq!(code.as_slice(), expected, "{kind:?}");
        }

        for malformed in [
            op(
                zmm(1),
                zmm(2),
                None,
                X86SatFpFormat::BF16,
                VecElementType::I8,
                VecWidth::V512,
                true,
                false,
                true,
            ),
            OpKind::VCvtFpToIntSat {
                dst: xmm(1),
                src: xmm(2),
                mask: None,
                fp_elem: X86SatFpFormat::BF16,
                int_elem: VecElementType::I8,
                width: VecWidth::V128,
                signed: true,
                truncate: false,
                round: FpRoundMode::Dynamic,
                zeroing: false,
                suppress_exceptions: false,
            },
            op(
                ymm(1),
                ymm(2),
                None,
                X86SatFpFormat::F32,
                VecElementType::I8,
                VecWidth::V128,
                true,
                false,
                false,
            ),
            op(
                xmm(1),
                xmm(2),
                None,
                X86SatFpFormat::F32,
                VecElementType::I8,
                VecWidth::V128,
                true,
                true,
                false,
            ),
            shaped(
                xmm(1),
                xmm(2),
                X86SatFpFormat::F64,
                VecElementType::I32,
                VecWidth::V128,
                true,
                false,
            ),
            op(
                xmm(1),
                xmm(2),
                None,
                X86SatFpFormat::F32,
                VecElementType::I8,
                VecWidth::V128,
                true,
                false,
                true,
            ),
            op(
                zmm(1),
                zmm(2),
                Some(k(0)),
                X86SatFpFormat::F64,
                VecElementType::I64,
                VecWidth::V512,
                false,
                false,
                false,
            ),
            op(
                xmm(1),
                xmm(2),
                None,
                X86SatFpFormat::F64,
                VecElementType::I8,
                VecWidth::V128,
                true,
                false,
                false,
            ),
            rounded(
                zmm(1),
                zmm(2),
                None,
                VecWidth::V512,
                true,
                FpRoundMode::Dynamic,
                false,
                true,
            ),
            rounded(
                zmm(1),
                zmm(2),
                None,
                VecWidth::V512,
                true,
                FpRoundMode::RoundUp,
                false,
                false,
            ),
            rounded(
                xmm(1),
                xmm(2),
                None,
                VecWidth::V128,
                true,
                FpRoundMode::RoundNearest,
                false,
                true,
            ),
            rounded(
                zmm(1),
                zmm(2),
                None,
                VecWidth::V512,
                true,
                FpRoundMode::RoundNearestTiesAway,
                false,
                true,
            ),
        ] {
            let mut code = CodeBuffer::new();
            assert!(matches!(
                lowerer.try_lower(&malformed, &mut code).unwrap(),
                Err(LowerError::InvalidRegister(_) | LowerError::UnsupportedOperation(_))
            ));
            assert!(code.as_slice().is_empty());
        }
    }

    #[test]
    fn emits_scalar_map5_saturation_llvm_encodings_and_rejects_bad_shapes() {
        let lowerer = Avx10Lowerer::new();
        let gpr = |reg| VReg::Arch(ArchReg::X86(reg));
        let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
        let scalar =
            |dst, src, elem, int_width, signed, suppress_exceptions| OpKind::X86ScalarFpToIntSat {
                dst,
                src,
                elem,
                int_width,
                signed,
                suppress_exceptions,
            };

        for (kind, expected) in [
            (
                scalar(
                    gpr(X86Reg::Rax),
                    xmm(2),
                    VecElementType::F64,
                    OpWidth::W64,
                    true,
                    true,
                ),
                &[0x62, 0xF5, 0xFF, 0x18, 0x6D, 0xC2][..],
            ),
            (
                scalar(
                    gpr(X86Reg::R17),
                    xmm(18),
                    VecElementType::F32,
                    OpWidth::W32,
                    false,
                    false,
                ),
                &[0x62, 0xA5, 0x7E, 0x08, 0x6C, 0xCA][..],
            ),
            (
                scalar(
                    gpr(X86Reg::R31),
                    xmm(30),
                    VecElementType::F64,
                    OpWidth::W64,
                    false,
                    true,
                ),
                &[0x62, 0x05, 0xFF, 0x18, 0x6C, 0xFE][..],
            ),
            (
                scalar(
                    gpr(X86Reg::R8),
                    xmm(17),
                    VecElementType::F32,
                    OpWidth::W32,
                    true,
                    false,
                ),
                &[0x62, 0x35, 0x7E, 0x08, 0x6D, 0xC1][..],
            ),
        ] {
            let mut code = CodeBuffer::new();
            lowerer
                .try_lower(&kind, &mut code)
                .expect("scalar saturation conversion must be recognized")
                .unwrap();
            assert_eq!(code.as_slice(), expected, "{kind:?}");
        }

        for malformed in [
            scalar(
                gpr(X86Reg::Rax),
                xmm(2),
                VecElementType::F16,
                OpWidth::W32,
                true,
                false,
            ),
            scalar(
                gpr(X86Reg::Rax),
                xmm(2),
                VecElementType::F32,
                OpWidth::W16,
                true,
                false,
            ),
            scalar(
                gpr(X86Reg::Rax),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                VecElementType::F32,
                OpWidth::W32,
                true,
                false,
            ),
            scalar(
                xmm(1),
                xmm(2),
                VecElementType::F32,
                OpWidth::W32,
                true,
                false,
            ),
        ] {
            let mut code = CodeBuffer::new();
            assert!(matches!(
                lowerer.try_lower(&malformed, &mut code).unwrap(),
                Err(LowerError::InvalidRegister(_) | LowerError::UnsupportedOperation(_))
            ));
            assert!(code.as_slice().is_empty());
        }
    }
}
