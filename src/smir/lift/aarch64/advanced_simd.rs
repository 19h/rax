//! Integer AdvSIMD families composed from width-explicit SMIR operations.

use crate::isa::arm::decoder::{DecodedInsn, Mnemonic, Operand};
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType, VecPermuteKind,
    VecWidth,
};
use crate::smir::lift::{LiftContext, LiftError};

use super::Aarch64Lifter;

impl Aarch64Lifter {
    pub(super) fn lift_extended_neon(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<Option<Vec<SmirOp>>, LiftError> {
        let mut ops = Vec::new();
        match insn.mnemonic {
            Mnemonic::VHADD
            | Mnemonic::VRHADD
            | Mnemonic::VHSUB
            | Mnemonic::VMAX
            | Mnemonic::VMIN
            | Mnemonic::VABD
            | Mnemonic::VABA
            | Mnemonic::VCGT
            | Mnemonic::VCGE
            | Mnemonic::VCEQ
            | Mnemonic::VTST
            | Mnemonic::VMLA
            | Mnemonic::VMLS
            | Mnemonic::VSHL
            | Mnemonic::VRSHL
            | Mnemonic::VPADD
            | Mnemonic::VPMAX
            | Mnemonic::VPMIN
                if insn.raw & 0x9f20_0400 == 0x0e20_0400 =>
            {
                self.lift_neon_lane_instruction(insn, pc, &mut ops, ctx)?;
            }
            Mnemonic::VSHR | Mnemonic::VSRA | Mnemonic::VSHL
                if insn.raw & 0x9f80_0400 == 0x0f00_0400 =>
            {
                self.lift_neon_shift_immediate(insn, pc, &mut ops)?;
            }
            Mnemonic::VDUP | Mnemonic::VMOV if insn.raw & 0x9fe0_8400 == 0x0e00_0400 => {
                self.lift_neon_copy(insn, pc, &mut ops, ctx)?;
            }
            Mnemonic::VRSHR | Mnemonic::VRSRA if insn.raw & 0x9f80_0400 == 0x0f00_0400 => {
                self.lift_neon_rounded_shift(insn, pc, &mut ops, ctx)?;
            }
            _ => return Ok(None),
        }
        Ok(Some(ops))
    }

    pub(super) fn lift_neon_lane_instruction(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rd, rn, rm) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn)), Some(Operand::FpReg(rm))) => {
                (rd, rn, rm)
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid AdvSIMD three-same operands".to_string(),
                ));
            }
        };

        let dst = Self::fp_vreg(rd);
        let src1 = Self::fp_vreg(rn);
        let src2 = Self::fp_vreg(rm);
        let q = (insn.raw >> 30) & 1;
        let size = (insn.raw >> 22) & 3;
        let signed = (insn.raw >> 29) & 1 == 0;
        let elem = Self::neon_integer_element(size)?;
        let scalar = insn.raw & (1 << 28) != 0;
        let lanes = if scalar { 1 } else { ((8 << q) >> size) as u8 };
        let lane_op = |dst, src1, src2, op| OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op,
            signed,
            set_ovf: false,
        };

        match insn.mnemonic {
            Mnemonic::VPADD | Mnemonic::VPMAX | Mnemonic::VPMIN => {
                let even_lanes = ctx.alloc_vreg();
                let odd_lanes = ctx.alloc_vreg();
                for (temporary, kind) in [
                    (even_lanes, VecPermuteKind::Uzp1),
                    (odd_lanes, VecPermuteKind::Uzp2),
                ] {
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::VPermute2 {
                            dst: temporary,
                            src1,
                            src2,
                            elem,
                            lanes,
                            kind,
                        },
                    );
                }
                let op = match insn.mnemonic {
                    Mnemonic::VPADD => VLaneOp::Add,
                    Mnemonic::VPMAX => VLaneOp::Max,
                    _ => VLaneOp::Min,
                };
                Self::push_lifted_op(ops, pc, lane_op(dst, even_lanes, odd_lanes, op));
            }
            Mnemonic::VHSUB => {
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VNavg {
                        dst,
                        src1,
                        src2,
                        elem,
                        lanes,
                        signed,
                    },
                );
            }
            Mnemonic::VCGT | Mnemonic::VCGE | Mnemonic::VCEQ => {
                let cond = match (insn.mnemonic, signed) {
                    (Mnemonic::VCGT, true) => VecCmpCond::Gt,
                    (Mnemonic::VCGT, false) => VecCmpCond::Gtu,
                    (Mnemonic::VCGE, true) => VecCmpCond::Ge,
                    (Mnemonic::VCGE, false) => VecCmpCond::Geu,
                    _ => VecCmpCond::Eq,
                };
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VCmp {
                        dst,
                        src1,
                        src2,
                        cond,
                        elem,
                        lanes,
                    },
                );
            }
            Mnemonic::VTST => {
                let tested = ctx.alloc_vreg();
                Self::push_lifted_op(ops, pc, lane_op(tested, src1, src2, VLaneOp::And));
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VCmp {
                        dst,
                        src1: tested,
                        src2: VReg::Imm(0),
                        cond: VecCmpCond::Ne,
                        elem,
                        lanes,
                    },
                );
            }
            Mnemonic::VABA | Mnemonic::VMLA | Mnemonic::VMLS => {
                let intermediate = ctx.alloc_vreg();
                let producer = if insn.mnemonic == Mnemonic::VABA {
                    VLaneOp::AbsDiff
                } else {
                    VLaneOp::Mul
                };
                let consumer = if insn.mnemonic == Mnemonic::VMLS {
                    VLaneOp::Sub
                } else {
                    VLaneOp::Add
                };
                Self::push_lifted_op(ops, pc, lane_op(intermediate, src1, src2, producer));
                Self::push_lifted_op(ops, pc, lane_op(dst, dst, intermediate, consumer));
            }
            _ => {
                let op = match insn.mnemonic {
                    Mnemonic::VHADD => VLaneOp::Avg,
                    Mnemonic::VRHADD => VLaneOp::AvgRnd,
                    Mnemonic::VMAX => VLaneOp::Max,
                    Mnemonic::VMIN => VLaneOp::Min,
                    Mnemonic::VABD => VLaneOp::AbsDiff,
                    Mnemonic::VSHL => VLaneOp::ShiftSigned8,
                    Mnemonic::VRSHL => VLaneOp::ShiftSigned8Rnd,
                    _ => {
                        return Err(LiftError::Internal(
                            "invalid AdvSIMD lane instruction dispatch".to_string(),
                        ));
                    }
                };
                Self::push_lifted_op(ops, pc, lane_op(dst, src1, src2, op));
            }
        }
        Ok(())
    }

    fn lift_neon_shift_immediate(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
    ) -> Result<(), LiftError> {
        let (rd, rn, amount) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn)), Some(Operand::Imm(imm))) => {
                (rd, rn, imm.effective_value())
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid AdvSIMD immediate-shift operands".to_string(),
                ));
            }
        };

        let immh = (insn.raw >> 19) & 15;
        if immh == 0 {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: format!("AdvSIMD shift with immh=0: {:#010x}", insn.raw),
            });
        }
        let size = 31 - immh.leading_zeros();
        let elem = Self::neon_integer_element(size)?;
        let bits = i64::from(8 << size);
        let q = (insn.raw >> 30) & 1;
        let unsigned = (insn.raw >> 29) & 1 != 0;
        let is_left_shift = insn.mnemonic == Mnemonic::VSHL;
        let dst = Self::fp_vreg(rd);
        let src = Self::fp_vreg(rn);
        let width = if q == 1 {
            VecWidth::V128
        } else {
            VecWidth::V64
        };

        if !is_left_shift && amount == bits && unsigned {
            // Generic VShift masks its count. USHR #esize must produce zero;
            // USRA #esize preserves active lanes and clears the upper 64 bits
            // of a 64-bit vector destination.
            let kind = if insn.mnemonic == Mnemonic::VSRA {
                OpKind::VOr {
                    dst,
                    src1: dst,
                    src2: dst,
                    width,
                }
            } else {
                OpKind::VXor {
                    dst,
                    src1: src,
                    src2: src,
                    width,
                }
            };
            Self::push_lifted_op(ops, pc, kind);
            return Ok(());
        }

        // Signed right shifts by esize and esize-1 both replicate the sign.
        // Normalize that boundary before using the generic masked-count IR.
        let amount = amount.min(bits - 1);
        let shift = if is_left_shift {
            ShiftOp::Lsl
        } else if unsigned {
            ShiftOp::Lsr
        } else {
            ShiftOp::Asr
        };
        let lanes = ((8 << q) >> size) as u8;
        let kind = if insn.mnemonic == Mnemonic::VSRA {
            OpKind::VShiftAcc {
                dst,
                src,
                amount: SrcOperand::Imm(amount),
                shift,
                elem,
                lanes,
            }
        } else {
            OpKind::VShift {
                dst,
                src,
                amount: SrcOperand::Imm(amount),
                shift,
                elem,
                lanes,
            }
        };
        Self::push_lifted_op(ops, pc, kind);
        if insn.mnemonic == Mnemonic::VSRA && q == 0 {
            // VShiftAcc preserves lanes outside the operation's active range.
            // A64 .8B/.4H/.2S destinations instead clear bits [127:64]. A
            // width-limited OR preserves active bits and explicitly clears
            // the inactive half in both interpretation and native lowering.
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::VOr {
                    dst,
                    src1: dst,
                    src2: dst,
                    width: VecWidth::V64,
                },
            );
        }
        Ok(())
    }

    fn lift_neon_rounded_shift(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let (rd, rn, amount) = match (
            insn.operands.get(0),
            insn.operands.get(1),
            insn.operands.get(2),
        ) {
            (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn)), Some(Operand::Imm(amount))) => {
                (rd, rn, amount.effective_value())
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid AdvSIMD rounded-shift operands".to_string(),
                ));
            }
        };
        let immh = (insn.raw >> 19) & 0xf;
        if immh == 0 {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "AdvSIMD rounded shift with immh=0".to_string(),
            });
        }
        let size = 31 - immh.leading_zeros();
        let elem = Self::neon_integer_element(size)?;
        let q = (insn.raw >> 30) & 1;
        let lanes = ((8 << q) >> size) as u8;
        let signed = (insn.raw >> 29) & 1 == 0;
        let dst = Self::fp_vreg(rd);
        let accumulating = insn.mnemonic == Mnemonic::VRSRA;
        let counts = ctx.alloc_vreg();
        let shifted = if accumulating {
            // Keep the source result separate from the old destination value.
            ctx.alloc_vreg()
        } else {
            dst
        };
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::VBroadcast {
                dst: counts,
                scalar: VReg::Imm(-amount),
                elem,
                lanes,
            },
        );
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::VLane {
                dst: shifted,
                src1: Self::fp_vreg(rn),
                src2: counts,
                elem,
                lanes,
                op: VLaneOp::ShiftSigned8Rnd,
                signed,
                set_ovf: false,
            },
        );
        if accumulating {
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::VLane {
                    dst,
                    src1: dst,
                    src2: shifted,
                    elem,
                    lanes,
                    op: VLaneOp::Add,
                    signed: false,
                    set_ovf: false,
                },
            );
        }
        Ok(())
    }

    fn lift_neon_copy(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let imm5 = (insn.raw >> 16) & 31;
        let size = (imm5 & 15).trailing_zeros();
        let elem = Self::neon_integer_element(size)?;
        let q = (insn.raw >> 30) & 1;
        let lanes = ((8 << q) >> size) as u8;
        let lane = match insn.operands.get(2) {
            Some(Operand::Imm(imm)) => imm.effective_value() as u8,
            _ => {
                return Err(LiftError::Internal(
                    "missing AdvSIMD copy lane index".to_string(),
                ));
            }
        };

        match (insn.mnemonic, insn.operands.get(0), insn.operands.get(1)) {
            (Mnemonic::VDUP, Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn))) => {
                let scalar = ctx.alloc_vreg();
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: Self::fp_vreg(rn),
                        lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VBroadcast {
                        dst: Self::fp_vreg(rd),
                        scalar,
                        elem,
                        lanes,
                    },
                );
            }
            (Mnemonic::VDUP, Some(Operand::FpReg(rd)), Some(Operand::Reg(rn))) => {
                let dst = Self::fp_vreg(rd);
                let scalar = self.arm_reg(rn);
                let kind = if scalar == VReg::Imm(0) {
                    OpKind::VXor {
                        dst,
                        src1: dst,
                        src2: dst,
                        width: if q == 1 {
                            VecWidth::V128
                        } else {
                            VecWidth::V64
                        },
                    }
                } else {
                    OpKind::VBroadcast {
                        dst,
                        scalar,
                        elem,
                        lanes,
                    }
                };
                Self::push_lifted_op(ops, pc, kind);
            }
            (Mnemonic::VMOV, Some(Operand::FpReg(rd)), Some(Operand::Reg(rn))) => {
                let dst = Self::fp_vreg(rd);
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VInsertLane {
                        dst,
                        vec: dst,
                        scalar: self.arm_reg(rn),
                        lane,
                        elem,
                    },
                );
            }
            (Mnemonic::VMOV, Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn))) => {
                let source_lane = match insn.operands.get(3) {
                    Some(Operand::Imm(imm)) => imm.effective_value() as u8,
                    _ => {
                        return Err(LiftError::Internal(
                            "missing AdvSIMD insertion source lane".to_string(),
                        ));
                    }
                };
                let scalar = ctx.alloc_vreg();
                let dst = Self::fp_vreg(rd);
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: Self::fp_vreg(rn),
                        lane: source_lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                );
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VInsertLane {
                        dst,
                        vec: dst,
                        scalar,
                        lane,
                        elem,
                    },
                );
            }
            (Mnemonic::VMOV, Some(Operand::Reg(rd)), Some(Operand::FpReg(rn))) => {
                let dst = self.dst_reg(rd, ctx);
                let sign = if (insn.raw >> 11) & 15 == 0b0101 {
                    SignExtend::Sign
                } else {
                    SignExtend::Zero
                };
                Self::push_lifted_op(
                    ops,
                    pc,
                    OpKind::VExtractLane {
                        dst,
                        vec: Self::fp_vreg(rn),
                        lane,
                        elem,
                        sign,
                    },
                );
                if sign == SignExtend::Sign && !rd.is_64bit {
                    Self::push_lifted_op(
                        ops,
                        pc,
                        OpKind::Mov {
                            dst,
                            src: SrcOperand::Reg(dst),
                            width: self.reg_width(rd),
                        },
                    );
                }
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid AdvSIMD copy operands".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn neon_integer_element(size: u32) -> Result<VecElementType, LiftError> {
        match size {
            0 => Ok(VecElementType::I8),
            1 => Ok(VecElementType::I16),
            2 => Ok(VecElementType::I32),
            3 => Ok(VecElementType::I64),
            _ => Err(LiftError::Internal(format!(
                "invalid AdvSIMD integer element size {size}"
            ))),
        }
    }
}
