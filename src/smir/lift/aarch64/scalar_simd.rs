//! Integer scalar AdvSIMD and zero comparisons using existing SMIR semantics.

use crate::isa::arm::ExecutionState;
use crate::isa::arm::decoder::{DecodedInsn, Mnemonic, Operand};
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ShiftOp, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType, VecReduceOp, VecUnaryOp,
};
use crate::smir::lift::{LiftContext, LiftError};

use super::Aarch64Lifter;

impl Aarch64Lifter {
    pub(super) fn lift_integer_scalar_or_zero(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<Option<Vec<SmirOp>>, LiftError> {
        if insn.state != ExecutionState::Aarch64 {
            return Ok(None);
        }
        let scalar = insn.raw & (1 << 28) != 0;
        let zero_comparison = matches!(
            insn.mnemonic,
            Mnemonic::VCGT | Mnemonic::VCGE | Mnemonic::VCEQ | Mnemonic::VCLE | Mnemonic::VCLT
        ) && matches!(insn.operands.get(2), Some(Operand::Imm(value)) if value.effective_value() == 0);
        let scalar_integer = scalar
            && matches!(
                insn.mnemonic,
                Mnemonic::VADD
                    | Mnemonic::VSUB
                    | Mnemonic::VCGT
                    | Mnemonic::VCGE
                    | Mnemonic::VCEQ
                    | Mnemonic::VTST
                    | Mnemonic::VSHL
                    | Mnemonic::VRSHL
                    | Mnemonic::VABS
                    | Mnemonic::VNEG
                    | Mnemonic::VPADD
                    | Mnemonic::VSHR
                    | Mnemonic::VSRA
                    | Mnemonic::VRSHR
                    | Mnemonic::VRSRA
                    | Mnemonic::VCLE
                    | Mnemonic::VCLT
            );
        if !zero_comparison && !scalar_integer {
            return Ok(None);
        }
        let (dst, src) = match (insn.operands.first(), insn.operands.get(1)) {
            (Some(Operand::FpReg(dst)), Some(Operand::FpReg(src))) => {
                (Self::fp_vreg(dst), Self::fp_vreg(src))
            }
            _ => {
                return Err(LiftError::Internal(
                    "invalid A64 scalar or comparison-zero operands".to_string(),
                ));
            }
        };
        let size = (insn.raw >> 22) & 3;
        let q = (insn.raw >> 30) & 1;
        let elem = if scalar {
            VecElementType::I64
        } else {
            Self::neon_integer_element(size)?
        };
        let lanes = if scalar { 1 } else { ((8 << q) >> size) as u8 };
        let mut ops = Vec::new();

        if zero_comparison {
            let cond = match insn.mnemonic {
                Mnemonic::VCGT => VecCmpCond::Gt,
                Mnemonic::VCGE => VecCmpCond::Ge,
                Mnemonic::VCEQ => VecCmpCond::Eq,
                Mnemonic::VCLE => VecCmpCond::Le,
                Mnemonic::VCLT => VecCmpCond::Lt,
                _ => {
                    return Err(LiftError::Internal(
                        "invalid comparison-zero dispatch".to_string(),
                    ));
                }
            };
            Self::push_lifted_op(
                &mut ops,
                pc,
                OpKind::VCmp {
                    dst,
                    src1: src,
                    src2: VReg::Imm(0),
                    cond,
                    elem,
                    lanes,
                },
            );
        } else if insn.raw & 0xdf80_0400 == 0x5f00_0400 {
            self.lift_scalar_integer_shift(insn, pc, dst, src, &mut ops, ctx)?;
        } else {
            match insn.mnemonic {
                Mnemonic::VADD | Mnemonic::VSUB => {
                    let Some(Operand::FpReg(source)) = insn.operands.get(2) else {
                        return Err(LiftError::Internal(
                            "missing scalar arithmetic source".to_string(),
                        ));
                    };
                    let src2 = Self::fp_vreg(source);
                    let kind = if insn.mnemonic == Mnemonic::VADD {
                        OpKind::VAdd {
                            dst,
                            src1: src,
                            src2,
                            elem,
                            lanes,
                        }
                    } else {
                        OpKind::VSub {
                            dst,
                            src1: src,
                            src2,
                            elem,
                            lanes,
                        }
                    };
                    Self::push_lifted_op(&mut ops, pc, kind);
                }
                Mnemonic::VABS | Mnemonic::VNEG => {
                    let op = if insn.mnemonic == Mnemonic::VABS {
                        VecUnaryOp::Abs
                    } else {
                        VecUnaryOp::Neg
                    };
                    Self::push_lifted_op(
                        &mut ops,
                        pc,
                        OpKind::VUnary {
                            dst,
                            src,
                            elem,
                            lanes,
                            op,
                        },
                    );
                }
                Mnemonic::VPADD => Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::VReduce {
                        dst,
                        src,
                        elem,
                        lanes: 2,
                        op: VecReduceOp::Add,
                    },
                ),
                _ => self.lift_neon_lane_instruction(insn, pc, &mut ops, ctx)?,
            }
        }
        Ok(Some(ops))
    }

    fn lift_scalar_integer_shift(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        dst: VReg,
        src: VReg,
        ops: &mut Vec<SmirOp>,
        ctx: &mut LiftContext,
    ) -> Result<(), LiftError> {
        let Some(Operand::Imm(immediate)) = insn.operands.get(2) else {
            return Err(LiftError::Internal(
                "missing scalar shift immediate".to_string(),
            ));
        };
        let amount = immediate.effective_value();
        let elem = VecElementType::I64;
        let lanes = 1;
        if insn.mnemonic == Mnemonic::VSHL {
            if !(0..64).contains(&amount) {
                return Err(LiftError::Internal(format!(
                    "invalid scalar left-shift amount {amount}"
                )));
            }
            Self::push_lifted_op(
                ops,
                pc,
                OpKind::VShift {
                    dst,
                    src,
                    amount: SrcOperand::Imm(amount),
                    shift: ShiftOp::Lsl,
                    elem,
                    lanes,
                },
            );
            return Ok(());
        }
        if !(1..=64).contains(&amount) {
            return Err(LiftError::Internal(format!(
                "invalid scalar right-shift amount {amount}"
            )));
        }
        let accumulating = matches!(insn.mnemonic, Mnemonic::VSRA | Mnemonic::VRSRA);
        let rounded = matches!(insn.mnemonic, Mnemonic::VRSHR | Mnemonic::VRSRA);
        let counts = ctx.alloc_vreg();
        let shifted = if accumulating { ctx.alloc_vreg() } else { dst };
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
        // Signed-byte lane shifts represent the architectural #64 boundary
        // directly. VShift instead masks its count modulo the element width.
        Self::push_lifted_op(
            ops,
            pc,
            OpKind::VLane {
                dst: shifted,
                src1: src,
                src2: counts,
                elem,
                lanes,
                op: if rounded {
                    VLaneOp::ShiftSigned8Rnd
                } else {
                    VLaneOp::ShiftSigned8
                },
                signed: insn.raw & (1 << 29) == 0,
                set_ovf: false,
            },
        );
        if accumulating {
            // VLane::Add clears unused bits and snapshots the old accumulator.
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
}
