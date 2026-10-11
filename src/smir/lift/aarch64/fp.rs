//! Scalar and vector floating-point instruction lifting.

use crate::isa::arm::decoder::{DecodedInsn, Mnemonic, Operand};
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    FpPrecision, FpRoundMode, OpId, OpWidth, SrcOperand, VReg, VecElementType, VecReduceOp,
    VecUnaryOp,
};
use crate::smir::lift::{LiftContext, LiftError};

use super::Aarch64Lifter;

impl Aarch64Lifter {
    pub(super) fn lift_fp_instruction(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<Option<Vec<SmirOp>>, LiftError> {
        let mut ops = Vec::new();
        macro_rules! push_op {
            ($kind:expr) => {{
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, $kind));
            }};
        }
        match insn.mnemonic {
            // =================================================================
            // Scalar FP - 2-source
            // =================================================================
            Mnemonic::FADD
            | Mnemonic::FSUB
            | Mnemonic::FMUL
            | Mnemonic::FDIV
            | Mnemonic::FMAX
            | Mnemonic::FMIN
            | Mnemonic::FMAXNM
            | Mnemonic::FMINNM => {
                // Across-lanes FP reductions FMAXV/FMINV/FMAXNMV/FMINNMV (across-
                // lanes marker bits[21:17]==11000, scalar dst + vector src, so 2
                // operands). Only the U=1 f32 (.4S) form is JIT'd here; the U=0
                // FP16 form bails. The 3-operand three-same and scalar 2-source
                // forms continue below.
                if insn.operands.get(2).is_none()
                    && (insn.raw >> 17) & 0x1F == 0b11000
                    && (insn.raw >> 29) & 1 == 1
                    && matches!(
                        insn.mnemonic,
                        Mnemonic::FMAX | Mnemonic::FMIN | Mnemonic::FMAXNM | Mnemonic::FMINNM
                    )
                {
                    let rop = match insn.mnemonic {
                        Mnemonic::FMAX => VecReduceOp::FMax,
                        Mnemonic::FMIN => VecReduceOp::FMin,
                        Mnemonic::FMAXNM => VecReduceOp::FMaxNm,
                        _ => VecReduceOp::FMinNm,
                    };
                    let (rd, rn) = match (insn.operands.get(0), insn.operands.get(1)) {
                        (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn))) => (rd, rn),
                        _ => {
                            return Err(LiftError::Internal("fp reduce operands".to_string()));
                        }
                    };
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VReduce {
                            dst: Self::fp_vreg(rd),
                            src: Self::fp_vreg(rn),
                            elem: VecElementType::F32,
                            lanes: 4,
                            op: rop,
                        },
                    ));
                    return Ok(Some(ops));
                }
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let rm = match insn.operands.get(2) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rm".to_string())),
                };
                // Vector FP three-same (bit 28 == 0) processes all lanes; the
                // scalar FP 2-source form (bit 28 == 1) takes the path below.
                // FMAX/FMIN preserve NaN propagation. FMAXNM/FMINNM use the
                // separate VFMinMaxNm operation to preserve their numeric
                // minimum/maximum behavior in interpretation and lowering.
                if (insn.raw >> 28) & 1 == 0 {
                    let q = (insn.raw >> 30) & 1;
                    let sz = (insn.raw >> 22) & 1;
                    let (elem, lanes) = if sz == 0 {
                        (VecElementType::F32, if q == 1 { 4u8 } else { 2 })
                    } else {
                        (VecElementType::F64, 2)
                    };
                    let dst = Self::fp_vreg(rd);
                    let src1 = Self::fp_vreg(rn);
                    let src2 = Self::fp_vreg(rm);
                    let vkind = match insn.mnemonic {
                        Mnemonic::FADD => OpKind::VAdd {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        Mnemonic::FSUB => OpKind::VSub {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        Mnemonic::FMUL => OpKind::VMul {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        Mnemonic::FDIV => OpKind::VDiv {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        Mnemonic::FMAX => OpKind::VMax {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                        },
                        Mnemonic::FMIN => OpKind::VMin {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                            signed: true,
                        },
                        // FMAXNM/FMINNM are IEEE maxNum/minNum (NaN-quiet),
                        // distinct from FMAX/FMIN (NaN-propagating).
                        Mnemonic::FMAXNM => OpKind::VFMinMaxNm {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                            min: false,
                        },
                        Mnemonic::FMINNM => OpKind::VFMinMaxNm {
                            dst,
                            src1,
                            src2,
                            elem,
                            lanes,
                            min: true,
                        },
                        _ => {
                            return Err(LiftError::Unsupported {
                                addr: pc,
                                mnemonic: format!("vector {:?}", insn.mnemonic),
                            });
                        }
                    };
                    ops.push(SmirOp::new(OpId(ops.len() as u16), pc, vkind));
                    return Ok(Some(ops));
                }
                let precision = Self::fp_precision(&rd.size);
                let kind = match insn.mnemonic {
                    Mnemonic::FADD => OpKind::FAdd {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                    Mnemonic::FSUB => OpKind::FSub {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                    Mnemonic::FMUL => OpKind::FMul {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                    Mnemonic::FDIV => OpKind::FDiv {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                    Mnemonic::FMAX | Mnemonic::FMAXNM => OpKind::FMax {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                    Mnemonic::FMIN | Mnemonic::FMINNM => OpKind::FMin {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                    _ => unreachable!(),
                };
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
            }

            Mnemonic::FNMUL => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let rm = match insn.operands.get(2) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rm".to_string())),
                };
                let precision = Self::fp_precision(&rd.size);
                let mul_dst = ctx.alloc_vreg();
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::FMul {
                        dst: mul_dst,
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                );
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::FNeg {
                        dst: Self::fp_vreg(rd),
                        src: mul_dst,
                        precision,
                    },
                );
            }

            // =================================================================
            // Scalar FP - 3-source (FMA)
            // =================================================================
            Mnemonic::FMADD | Mnemonic::FMSUB | Mnemonic::FNMADD | Mnemonic::FNMSUB => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let rm = match insn.operands.get(2) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rm".to_string())),
                };
                let ra = match insn.operands.get(3) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp ra".to_string())),
                };
                let precision = Self::fp_precision(&rd.size);
                match insn.mnemonic {
                    Mnemonic::FMADD => {
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FFma {
                                dst: Self::fp_vreg(rd),
                                src1: Self::fp_vreg(rn),
                                src2: Self::fp_vreg(rm),
                                src3: Self::fp_vreg(ra),
                                precision,
                            },
                        );
                    }
                    Mnemonic::FMSUB => {
                        let mul_dst = ctx.alloc_vreg();
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FMul {
                                dst: mul_dst,
                                src1: Self::fp_vreg(rn),
                                src2: Self::fp_vreg(rm),
                                precision,
                            },
                        );
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FSub {
                                dst: Self::fp_vreg(rd),
                                src1: mul_dst,
                                src2: Self::fp_vreg(ra),
                                precision,
                            },
                        );
                    }
                    Mnemonic::FNMADD => {
                        let mul_dst = ctx.alloc_vreg();
                        let neg_dst = ctx.alloc_vreg();
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FMul {
                                dst: mul_dst,
                                src1: Self::fp_vreg(rn),
                                src2: Self::fp_vreg(rm),
                                precision,
                            },
                        );
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FNeg {
                                dst: neg_dst,
                                src: mul_dst,
                                precision,
                            },
                        );
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FAdd {
                                dst: Self::fp_vreg(rd),
                                src1: neg_dst,
                                src2: Self::fp_vreg(ra),
                                precision,
                            },
                        );
                    }
                    Mnemonic::FNMSUB => {
                        let fma_dst = ctx.alloc_vreg();
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FFma {
                                dst: fma_dst,
                                src1: Self::fp_vreg(rn),
                                src2: Self::fp_vreg(rm),
                                src3: Self::fp_vreg(ra),
                                precision,
                            },
                        );
                        Self::push_lifted_op(
                            &mut ops,
                            pc,
                            OpKind::FNeg {
                                dst: Self::fp_vreg(rd),
                                src: fma_dst,
                                precision,
                            },
                        );
                    }
                    _ => unreachable!(),
                }
            }

            // =================================================================
            // Scalar FP - Unary
            // =================================================================
            Mnemonic::FMOV | Mnemonic::FABS | Mnemonic::FNEG | Mnemonic::FSQRT => {
                // The scalar FP 1-source forms (bit 28 == 1) take the path below.
                // The vector two-register-misc FABS/FNEG/FSQRT (bit 28 == 0)
                // share these mnemonics but operate per-lane; emit a VUnary.
                if (insn.raw >> 28) & 1 == 0 {
                    let vop = match insn.mnemonic {
                        Mnemonic::FABS => VecUnaryOp::FAbs,
                        Mnemonic::FNEG => VecUnaryOp::FNeg,
                        Mnemonic::FSQRT => VecUnaryOp::FSqrt,
                        // Vector FMOV is a different (immediate/dup) form; deopt.
                        _ => {
                            return Err(LiftError::Unsupported {
                                addr: pc,
                                mnemonic: format!("vector {:?}", insn.mnemonic),
                            });
                        }
                    };
                    let (rd, rn) = match (insn.operands.get(0), insn.operands.get(1)) {
                        (Some(Operand::FpReg(rd)), Some(Operand::FpReg(rn))) => (rd, rn),
                        _ => {
                            return Err(LiftError::Internal(
                                "vector FP unary operands".to_string(),
                            ));
                        }
                    };
                    let q = (insn.raw >> 30) & 1;
                    let sz = (insn.raw >> 22) & 1;
                    // 1D (sz=1, Q=0) is a reserved FP-vector arrangement; only 2D
                    // (Q=1) is valid for 64-bit FP elements. Bail to the interpreter
                    // (UNDEFINED) rather than silently promoting it to a 2D op — the
                    // lowerer derives Q from elem*lanes and would emit a valid 2D
                    // native instruction for the invalid guest encoding. (#54)
                    if sz == 1 && q == 0 {
                        return Err(LiftError::Unsupported {
                            addr: pc,
                            mnemonic: format!("vector {:?}", insn.mnemonic),
                        });
                    }
                    let (elem, lanes) = if sz == 0 {
                        (VecElementType::F32, if q == 1 { 4u8 } else { 2 })
                    } else {
                        (VecElementType::F64, 2)
                    };
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VUnary {
                            dst: Self::fp_vreg(rd),
                            src: Self::fp_vreg(rn),
                            elem,
                            lanes,
                            op: vop,
                        },
                    ));
                    return Ok(Some(ops));
                }
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let precision = Self::fp_precision(&rd.size);
                let kind = match insn.mnemonic {
                    Mnemonic::FMOV => OpKind::Mov {
                        dst: Self::fp_vreg(rd),
                        src: SrcOperand::Reg(Self::fp_vreg(rn)),
                        width: match precision {
                            FpPrecision::F32 => OpWidth::W32,
                            FpPrecision::F64 => OpWidth::W64,
                            _ => OpWidth::W32,
                        },
                    },
                    Mnemonic::FABS => OpKind::FAbs {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        precision,
                    },
                    Mnemonic::FNEG => OpKind::FNeg {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        precision,
                    },
                    Mnemonic::FSQRT => OpKind::FSqrt {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        precision,
                    },
                    _ => unreachable!(),
                };
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
            }

            // Vector integer NEG/ABS, bitwise NOT (size = byte) and per-byte
            // population count (CNT), all advanced-SIMD two-register misc. The
            // decoder only produces these mnemonics for the vector form.
            Mnemonic::VNEG => {
                self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Neg, false)?;
            }
            Mnemonic::VABS => {
                self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Abs, false)?;
            }
            Mnemonic::VMVN => {
                self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Not, true)?;
            }
            Mnemonic::CNT => {
                self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Cnt, true)?;
            }

            // =================================================================
            // Scalar FP - Compare
            // =================================================================
            Mnemonic::FCMP | Mnemonic::FCMPE => {
                let rn = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let precision = Self::fp_precision(&rn.size);
                let src2 = match insn.operands.get(1) {
                    Some(Operand::FpReg(rm)) => Self::fp_vreg(rm),
                    None => VReg::Imm(0),
                    _ => return Err(LiftError::Internal("invalid FCMP operand".to_string())),
                };
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::FCmp {
                        src1: Self::fp_vreg(rn),
                        src2,
                        precision,
                    },
                );
            }

            Mnemonic::FCCMP | Mnemonic::FCCMPE => {
                let rn = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let rm = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rm".to_string())),
                };
                let _nzcv_imm = match insn.operands.get(2) {
                    Some(Operand::Imm(imm)) => imm.effective_value(),
                    _ => return Err(LiftError::Internal("missing FCCMP nzcv".to_string())),
                };
                let precision = Self::fp_precision(&rn.size);
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::FCmp {
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        precision,
                    },
                );
            }

            // =================================================================
            // Scalar FP - Convert
            // =================================================================
            Mnemonic::FCVT => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let from = Self::fp_precision(&rn.size);
                let to = Self::fp_precision(&rd.size);
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::FConvert {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        from,
                        to,
                    },
                );
            }

            Mnemonic::FCVTNS
            | Mnemonic::FCVTNU
            | Mnemonic::FCVTAS
            | Mnemonic::FCVTAU
            | Mnemonic::FCVTPS
            | Mnemonic::FCVTPU
            | Mnemonic::FCVTMS
            | Mnemonic::FCVTMU
            | Mnemonic::FCVTZS
            | Mnemonic::FCVTZU => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let precision = Self::fp_precision(&rn.size);
                let int_width = match precision {
                    FpPrecision::F32 => OpWidth::W32,
                    FpPrecision::F64 => OpWidth::W64,
                    _ => OpWidth::W32,
                };
                let (signed, round) = match insn.mnemonic {
                    Mnemonic::FCVTNS => (true, FpRoundMode::RoundNearest),
                    Mnemonic::FCVTNU => (false, FpRoundMode::RoundNearest),
                    Mnemonic::FCVTAS => (true, FpRoundMode::RoundNearestTiesAway),
                    Mnemonic::FCVTAU => (false, FpRoundMode::RoundNearestTiesAway),
                    Mnemonic::FCVTPS => (true, FpRoundMode::RoundUp),
                    Mnemonic::FCVTPU => (false, FpRoundMode::RoundUp),
                    Mnemonic::FCVTMS => (true, FpRoundMode::RoundDown),
                    Mnemonic::FCVTMU => (false, FpRoundMode::RoundDown),
                    Mnemonic::FCVTZS => (true, FpRoundMode::RoundTowardZero),
                    Mnemonic::FCVTZU => (false, FpRoundMode::RoundTowardZero),
                    _ => unreachable!(),
                };
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::FpToInt {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        fp_precision: precision,
                        int_width,
                        signed,
                        round,
                    },
                );
            }

            Mnemonic::SCVTF | Mnemonic::UCVTF => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let fp_precision = Self::fp_precision(&rd.size);
                let int_width = match fp_precision {
                    FpPrecision::F32 => OpWidth::W32,
                    FpPrecision::F64 => OpWidth::W64,
                    _ => OpWidth::W32,
                };
                let signed = insn.mnemonic == Mnemonic::SCVTF;
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::IntToFp {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        int_width,
                        fp_precision,
                        signed,
                    },
                );
            }

            // =================================================================
            // Scalar FP - Round
            // =================================================================
            Mnemonic::FRINTN
            | Mnemonic::FRINTP
            | Mnemonic::FRINTM
            | Mnemonic::FRINTZ
            | Mnemonic::FRINTA
            | Mnemonic::FRINTX
            | Mnemonic::FRINTI => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let precision = Self::fp_precision(&rd.size);
                let mode = match insn.mnemonic {
                    Mnemonic::FRINTN => FpRoundMode::RoundNearest,
                    Mnemonic::FRINTP => FpRoundMode::RoundUp,
                    Mnemonic::FRINTM => FpRoundMode::RoundDown,
                    Mnemonic::FRINTZ => FpRoundMode::RoundTowardZero,
                    Mnemonic::FRINTA => FpRoundMode::RoundNearestTiesAway,
                    Mnemonic::FRINTX => FpRoundMode::Dynamic,
                    Mnemonic::FRINTI => FpRoundMode::Dynamic,
                    _ => unreachable!(),
                };
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::FRound {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        precision,
                        mode,
                    },
                );
            }

            // =================================================================
            // Scalar FP - Conditional Select
            // =================================================================
            Mnemonic::FCSEL => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rn".to_string())),
                };
                let rm = match insn.operands.get(2) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing fp rm".to_string())),
                };
                let cond = match insn.operands.get(3) {
                    Some(Operand::Cond(c)) => self.arm_cond(*c),
                    _ => return Err(LiftError::Internal("missing FCSEL cond".to_string())),
                };
                let width = match Self::fp_precision(&rd.size) {
                    FpPrecision::F32 => OpWidth::W32,
                    FpPrecision::F64 => OpWidth::W64,
                    FpPrecision::F16 => OpWidth::W16,
                    _ => OpWidth::W32,
                };
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::Mov {
                        dst: Self::fp_vreg(rd),
                        src: SrcOperand::Reg(Self::fp_vreg(rm)),
                        width,
                    },
                );
                Self::push_lifted_op(
                    &mut ops,
                    pc,
                    OpKind::CMove {
                        dst: Self::fp_vreg(rd),
                        src: Self::fp_vreg(rn),
                        cond,
                        width,
                    },
                );
            }

            _ => return Ok(None),
        }
        Ok(Some(ops))
    }
}
