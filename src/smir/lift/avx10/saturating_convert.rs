//! Standalone AVX10.2 MAP5 saturating-conversion lifting.

use super::*;
use crate::smir::ir::ops::x86_sat_fp_to_int_widths;

fn vector_reg(reg: u8, width: VecWidth) -> VReg {
    let reg = match width {
        VecWidth::V64 | VecWidth::V128 => X86Reg::Xmm(reg),
        VecWidth::V256 => X86Reg::Ymm(reg),
        VecWidth::V512 => X86Reg::Zmm(reg),
    };
    VReg::Arch(ArchReg::X86(reg))
}

impl Avx10Lifter {
    /// Lift packed VCVT[T]{PH,PS,BF16}2I[U]BS register forms.
    pub(super) fn lift_vcvt_fp_to_i8_sat(
        &self,
        evex: &EvexPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        fp_format: X86SatFpFormat,
        signed: bool,
        truncate: bool,
    ) -> Result<LiftResult, LiftError> {
        let (modrm, consumed) = self.decode_modrm(bytes, pc)?;
        let embedded_control = evex.b_bit && !modrm.is_memory;

        if evex.vvvv != 0
            || evex.v_prime
            || (evex.z && evex.aaa == 0)
            || (fp_format == X86SatFpFormat::BF16 && embedded_control)
            || (!embedded_control && evex.ll == 3)
            || (embedded_control && truncate && evex.ll != 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if modrm.is_memory {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "standalone AVX10 MAP5 saturation conversion memory operand".into(),
            });
        }

        let dst_reg = evex.dest_reg(modrm.reg);
        let src_reg = evex.rm_reg(modrm.rm);
        let suppress_exceptions = embedded_control;
        let width = if suppress_exceptions {
            VecWidth::V512
        } else {
            evex.vec_width()
        };
        let round = if fp_format == X86SatFpFormat::BF16 {
            if truncate {
                FpRoundMode::RoundTowardZero
            } else {
                FpRoundMode::RoundNearest
            }
        } else if truncate {
            FpRoundMode::RoundTowardZero
        } else if suppress_exceptions {
            match evex.ll {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                3 => FpRoundMode::RoundTowardZero,
                _ => unreachable!("EVEX L'L is two bits"),
            }
        } else {
            FpRoundMode::Dynamic
        };

        let dst = vector_reg(dst_reg, width);
        let src = vector_reg(src_reg, width);

        let op = SmirOp::new(
            ctx.next_op_id(),
            pc,
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask: (evex.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(evex.aaa)))),
                fp_elem: fp_format,
                int_elem: VecElementType::I8,
                width,
                signed,
                truncate,
                round,
                zeroing: evex.z,
                suppress_exceptions,
            },
        );

        Ok(LiftResult::fallthrough(vec![op], evex.bytes + 1 + consumed))
    }

    /// Lift packed truncating FP32/FP64-to-I32/I64 saturation conversions.
    pub(super) fn lift_vcvtt_fp_to_int_sat(
        &self,
        evex: &EvexPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        fp_format: X86SatFpFormat,
        int_elem: VecElementType,
        signed: bool,
    ) -> Result<LiftResult, LiftError> {
        let (modrm, consumed) = self.decode_modrm(bytes, pc)?;

        if evex.vvvv != 0
            || evex.v_prime
            || evex.ll == 3
            || (evex.z && evex.aaa == 0)
            || (evex.b_bit && !modrm.is_memory && evex.ll != 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if modrm.is_memory {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "standalone AVX10 MAP5 saturation conversion memory operand".into(),
            });
        }

        let dst_reg = evex.dest_reg(modrm.reg);
        let src_reg = evex.rm_reg(modrm.rm);
        let suppress_exceptions = evex.b_bit;
        let encoded_width = if suppress_exceptions {
            VecWidth::V512
        } else {
            evex.vec_width()
        };
        let (src_width, width) = match (fp_format, int_elem, encoded_width) {
            (X86SatFpFormat::F64, VecElementType::I32, VecWidth::V128) => {
                (VecWidth::V128, VecWidth::V64)
            }
            (X86SatFpFormat::F64, VecElementType::I32, VecWidth::V256) => {
                (VecWidth::V256, VecWidth::V128)
            }
            (X86SatFpFormat::F64, VecElementType::I32, VecWidth::V512) => {
                (VecWidth::V512, VecWidth::V256)
            }
            (X86SatFpFormat::F32, VecElementType::I64, VecWidth::V128) => {
                (VecWidth::V64, VecWidth::V128)
            }
            (X86SatFpFormat::F32, VecElementType::I64, VecWidth::V256) => {
                (VecWidth::V128, VecWidth::V256)
            }
            (X86SatFpFormat::F32, VecElementType::I64, VecWidth::V512) => {
                (VecWidth::V256, VecWidth::V512)
            }
            (_, _, width) => (width, width),
        };
        if x86_sat_fp_to_int_widths(fp_format, int_elem, width, true)
            != Some((src_width, encoded_width))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let dst = vector_reg(dst_reg, width);
        let src = vector_reg(src_reg, src_width);

        let op = SmirOp::new(
            ctx.next_op_id(),
            pc,
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask: (evex.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(evex.aaa)))),
                fp_elem: fp_format,
                int_elem,
                width,
                signed,
                truncate: true,
                round: FpRoundMode::RoundTowardZero,
                zeroing: evex.z,
                suppress_exceptions,
            },
        );

        Ok(LiftResult::fallthrough(vec![op], evex.bytes + 1 + consumed))
    }
}
