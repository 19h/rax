//! AVX10.2 MAP5 saturating-conversion lowering.

use super::*;

impl Avx10Lowerer {
    pub(super) fn lower_vcvt_fp_to_int_sat(
        &self,
        code: &mut CodeBuffer,
        dst: &VReg,
        src: &VReg,
        mask: Option<&VReg>,
        fp_elem: VecElementType,
        int_elem: VecElementType,
        width: VecWidth,
        signed: bool,
        zeroing: bool,
        suppress_exceptions: bool,
    ) -> Avx10LowerResult<()> {
        let vector_matches_width = |reg: &VReg| {
            matches!(
                (reg, width),
                (VReg::Arch(ArchReg::X86(X86Reg::Xmm(_))), VecWidth::V128)
                    | (VReg::Arch(ArchReg::X86(X86Reg::Ymm(_))), VecWidth::V256)
                    | (VReg::Arch(ArchReg::X86(X86Reg::Zmm(_))), VecWidth::V512)
            )
        };
        if !matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
            || !vector_matches_width(dst)
            || !vector_matches_width(src)
            || (zeroing && mask.is_none())
            || (suppress_exceptions && width != VecWidth::V512)
        {
            return Err(LowerError::UnsupportedOperation(
                "Saturation conversion: invalid vector, mask, or SAE shape".to_string(),
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

        let (opcode, w) = match (fp_elem, int_elem, signed) {
            (VecElementType::F32, VecElementType::I8, true) => (0x68, false),
            (VecElementType::F32, VecElementType::I8, false) => (0x6A, false),
            (VecElementType::F64, VecElementType::I64, true) => (0x6D, true),
            (VecElementType::F64, VecElementType::I64, false) => (0x6C, true),
            _ => {
                return Err(LowerError::UnsupportedOperation(
                    "Saturation conversion: invalid types".to_string(),
                ));
            }
        };

        let mut enc = EvexEncoder::new(code);
        enc.emit_evex_with_b(
            5, // MAP5
            1, // 66
            w,
            if suppress_exceptions {
                // Register {sae} encodes EVEX.L'L=00 while operating on ZMM.
                VecWidth::V128
            } else {
                width
            },
            dst_reg,
            0, // EVEX.vvvv is reserved and encodes 1111b.
            src_reg,
            mask_reg,
            zeroing,
            suppress_exceptions,
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
            |dst, src, mask, fp_elem, int_elem, width, signed, zeroing, suppress_exceptions| {
                OpKind::VCvtFpToIntSat {
                    dst,
                    src,
                    mask,
                    fp_elem,
                    int_elem,
                    width,
                    signed,
                    zeroing,
                    suppress_exceptions,
                }
            };

        for (kind, expected) in [
            (
                op(
                    xmm(1),
                    xmm(2),
                    None,
                    VecElementType::F32,
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
                    VecElementType::F32,
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
                    zmm(1),
                    zmm(2),
                    None,
                    VecElementType::F64,
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
                    VecElementType::F64,
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
                    VecElementType::F32,
                    VecElementType::I8,
                    VecWidth::V512,
                    true,
                    false,
                    true,
                ),
                &[0x62, 0xF5, 0x7D, 0x18, 0x68, 0xCA][..],
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
                ymm(1),
                ymm(2),
                None,
                VecElementType::F32,
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
                VecElementType::F32,
                VecElementType::I8,
                VecWidth::V128,
                true,
                true,
                false,
            ),
            op(
                xmm(1),
                xmm(2),
                None,
                VecElementType::F32,
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
                VecElementType::F64,
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
                VecElementType::F64,
                VecElementType::I8,
                VecWidth::V128,
                true,
                false,
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
