//! Standalone AVX10.2 MAP5 saturating-conversion lifting.

use super::*;

impl Avx10Lifter {
    /// Lift VCVT[T]PS2IBS/VCVT[T]PS2IUBS.
    pub(super) fn lift_vcvtps2ibs(
        &self,
        evex: &EvexPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        signed: bool,
        truncate: bool,
    ) -> Result<LiftResult, LiftError> {
        let (modrm, consumed) = self.decode_modrm(bytes, pc)?;
        let embedded_control = evex.b_bit && !modrm.is_memory;

        if evex.vvvv != 0
            || evex.v_prime
            || (evex.z && evex.aaa == 0)
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
        let round = if truncate {
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

        let dst = self.zmm(dst_reg);
        let src = self.zmm(src_reg);

        let op = SmirOp::new(
            ctx.next_op_id(),
            pc,
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask: (evex.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(evex.aaa)))),
                fp_elem: VecElementType::F32,
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

    /// Lift VCVTTPD2QQS/VCVTTPD2UQQS.
    pub(super) fn lift_vcvttpd2qqs(
        &self,
        evex: &EvexPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
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
        let width = if suppress_exceptions {
            VecWidth::V512
        } else {
            evex.vec_width()
        };

        let dst = self.zmm(dst_reg);
        let src = self.zmm(src_reg);

        let op = SmirOp::new(
            ctx.next_op_id(),
            pc,
            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask: (evex.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(evex.aaa)))),
                fp_elem: VecElementType::F64,
                int_elem: VecElementType::I64,
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
