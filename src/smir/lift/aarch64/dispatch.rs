//! dispatch.rs

use crate::smir::lift::aarch64::*;
use std::collections::HashSet;

use crate::isa::arm::decoder::{
    AddressingMode, Condition as ArmCondition, DecodedInsn, ExtendType, FpRegSize, FpRegister,
    MemOffset, MemOperand, Mnemonic, Operand, Register, ShiftType,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader};

impl Aarch64Lifter {

    // ========================================================================
    // Instruction Lifting
    // ========================================================================

    /// Lift a single instruction to SMIR ops
    pub(crate) fn lift_insn_inner(
        &self,
        insn: &DecodedInsn,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let mut ops = Vec::new();
        let mut control = ControlFlow::Fallthrough;

        macro_rules! push_op {
            ($kind:expr) => {{
                ops.push(SmirOp::new(OpId(ops.len() as u16), pc, $kind));
            }};
        }

        match insn.mnemonic {
            // =================================================================
            // Arithmetic
            // =================================================================
            Mnemonic::ADD | Mnemonic::ADDS => {
                let (dst, src1, src2, width) = self.parse_arith_operands(insn, ctx)?;
                let flags = if insn.sets_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                };
                push_op!(OpKind::Add {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                });
            }

            Mnemonic::SUB | Mnemonic::SUBS => {
                let (dst, src1, src2, width) = self.parse_arith_operands(insn, ctx)?;
                let flags = if insn.sets_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                };
                push_op!(OpKind::Sub {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                });
            }

            Mnemonic::ADDG | Mnemonic::SUBG => {
                self.lift_add_sub_tags(insn, pc, &mut ops, ctx)?;
            }

            Mnemonic::ADC | Mnemonic::ADCS => {
                let (dst, src1, src2, width) = self.parse_arith_operands(insn, ctx)?;
                let flags = if insn.sets_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                };
                push_op!(OpKind::Adc {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                });
            }

            Mnemonic::SBC | Mnemonic::SBCS | Mnemonic::NGC | Mnemonic::NGCS => {
                let (dst, src1, src2, width) =
                    if matches!(insn.mnemonic, Mnemonic::NGC | Mnemonic::NGCS) {
                        if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rm))) =
                            (insn.operands.get(0), insn.operands.get(1))
                        {
                            (
                                self.dst_reg(rd, ctx),
                                VReg::Imm(0),
                                SrcOperand::Reg(self.arm_reg(rm)),
                                self.reg_width(rd),
                            )
                        } else {
                            return Err(LiftError::Internal("invalid ngc operands".to_string()));
                        }
                    } else {
                        self.parse_arith_operands(insn, ctx)?
                    };
                let flags = if insn.sets_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                };
                push_op!(OpKind::Sbb {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                });
            }

            Mnemonic::NEG | Mnemonic::NEGS => {
                if let Some(Operand::Reg(rd)) = insn.operands.get(0) {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    let flags = if insn.sets_flags {
                        FlagUpdate::All
                    } else {
                        FlagUpdate::None
                    };
                    match insn.operands.get(1) {
                        Some(Operand::Reg(rm)) => {
                            push_op!(OpKind::Neg {
                                dst,
                                src: self.arm_reg(rm),
                                width,
                                flags,
                            });
                        }
                        Some(op @ Operand::ShiftedReg(_)) => {
                            let src2 = self.operand_to_src(op, ctx)?;
                            push_op!(OpKind::Sub {
                                dst,
                                src1: VReg::Imm(0),
                                src2,
                                width,
                                flags,
                            });
                        }
                        _ => return Err(LiftError::Internal("invalid neg operands".to_string())),
                    }
                }
            }

            Mnemonic::MUL | Mnemonic::MNEG => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Reg(rm))) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    if insn.mnemonic == Mnemonic::MUL {
                        Self::push_mul_op(
                            &mut ops,
                            pc,
                            dst,
                            None,
                            self.arm_reg(rn),
                            self.arm_reg(rm),
                            width,
                            false,
                        );
                    } else {
                        push_op!(OpKind::MulSub {
                            dst,
                            acc: VReg::Imm(0),
                            src1: self.arm_reg(rn),
                            src2: self.arm_reg(rm),
                            width,
                        });
                    }
                }
            }

            Mnemonic::MADD | Mnemonic::MSUB => {
                if let (
                    Some(Operand::Reg(rd)),
                    Some(Operand::Reg(rn)),
                    Some(Operand::Reg(rm)),
                    Some(Operand::Reg(ra)),
                ) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                    insn.operands.get(3),
                ) {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);

                    if insn.mnemonic == Mnemonic::MADD {
                        push_op!(OpKind::MulAdd {
                            dst,
                            acc: self.arm_reg(ra),
                            src1: self.arm_reg(rn),
                            src2: self.arm_reg(rm),
                            width,
                        });
                    } else {
                        push_op!(OpKind::MulSub {
                            dst,
                            acc: self.arm_reg(ra),
                            src1: self.arm_reg(rn),
                            src2: self.arm_reg(rm),
                            width,
                        });
                    }
                }
            }

            Mnemonic::SMADDL
            | Mnemonic::SMSUBL
            | Mnemonic::SMNEGL
            | Mnemonic::UMADDL
            | Mnemonic::UMSUBL
            | Mnemonic::UMNEGL => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Reg(rm))) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    let dst = self.dst_reg(rd, ctx);
                    let src1 = self.widen_w_to_x(
                        &mut ops,
                        pc,
                        ctx,
                        rn,
                        matches!(
                            insn.mnemonic,
                            Mnemonic::SMADDL | Mnemonic::SMSUBL | Mnemonic::SMNEGL
                        ),
                    );
                    let src2 = self.widen_w_to_x(
                        &mut ops,
                        pc,
                        ctx,
                        rm,
                        matches!(
                            insn.mnemonic,
                            Mnemonic::SMADDL | Mnemonic::SMSUBL | Mnemonic::SMNEGL
                        ),
                    );
                    let product = ctx.alloc_vreg();
                    let signed = matches!(
                        insn.mnemonic,
                        Mnemonic::SMADDL | Mnemonic::SMSUBL | Mnemonic::SMNEGL
                    );
                    Self::push_mul_op(
                        &mut ops,
                        pc,
                        product,
                        None,
                        src1,
                        src2,
                        OpWidth::W64,
                        signed,
                    );
                    if matches!(insn.mnemonic, Mnemonic::SMADDL | Mnemonic::UMADDL) {
                        let Some(Operand::Reg(ra)) = insn.operands.get(3) else {
                            return Err(LiftError::Internal(
                                "missing multiply add accumulator".to_string(),
                            ));
                        };
                        push_op!(OpKind::Add {
                            dst,
                            src1: self.arm_reg(ra),
                            src2: SrcOperand::Reg(product),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        });
                    } else {
                        let acc = if matches!(insn.mnemonic, Mnemonic::SMNEGL | Mnemonic::UMNEGL) {
                            VReg::Imm(0)
                        } else {
                            let Some(Operand::Reg(ra)) = insn.operands.get(3) else {
                                return Err(LiftError::Internal(
                                    "missing multiply subtract accumulator".to_string(),
                                ));
                            };
                            self.arm_reg(ra)
                        };
                        push_op!(OpKind::Sub {
                            dst,
                            src1: acc,
                            src2: SrcOperand::Reg(product),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        });
                    }
                }
            }

            Mnemonic::SMULL | Mnemonic::UMULL => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Reg(rm))) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    let dst = self.dst_reg(rd, ctx);
                    let signed = insn.mnemonic == Mnemonic::SMULL;
                    let src1 = self.widen_w_to_x(&mut ops, pc, ctx, rn, signed);
                    let src2 = self.widen_w_to_x(&mut ops, pc, ctx, rm, signed);
                    Self::push_mul_op(&mut ops, pc, dst, None, src1, src2, OpWidth::W64, signed);
                }
            }

            Mnemonic::SMULH | Mnemonic::UMULH => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Reg(rm))) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    let dst = self.dst_reg(rd, ctx);
                    let lo = ctx.alloc_vreg();
                    Self::push_mul_op(
                        &mut ops,
                        pc,
                        lo,
                        Some(dst),
                        self.arm_reg(rn),
                        self.arm_reg(rm),
                        OpWidth::W64,
                        insn.mnemonic == Mnemonic::SMULH,
                    );
                }
            }

            Mnemonic::UDIV => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Reg(rm))) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    push_op!(OpKind::DivU {
                        quot: dst,
                        rem: None,
                        src1: self.arm_reg(rn),
                        src2: SrcOperand::Reg(self.arm_reg(rm)),
                        width,
                        flags: FlagUpdate::None,
                    });
                }
            }

            Mnemonic::SDIV => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn)), Some(Operand::Reg(rm))) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    push_op!(OpKind::DivS {
                        quot: dst,
                        rem: None,
                        src1: self.arm_reg(rn),
                        src2: SrcOperand::Reg(self.arm_reg(rm)),
                        width,
                        flags: FlagUpdate::None,
                    });
                }
            }

            Mnemonic::CRC32B
            | Mnemonic::CRC32H
            | Mnemonic::CRC32W
            | Mnemonic::CRC32X
            | Mnemonic::CRC32CB
            | Mnemonic::CRC32CH
            | Mnemonic::CRC32CW
            | Mnemonic::CRC32CX => self.lift_crc32(insn, &mut ops, pc, ctx),

            // =================================================================
            // Logical
            // =================================================================
            Mnemonic::AND | Mnemonic::ANDS => {
                let (dst, src1, src2, width) = self.parse_arith_operands(insn, ctx)?;
                let flags = if insn.sets_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                };
                push_op!(OpKind::And {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                });
            }

            Mnemonic::ORR | Mnemonic::ORRS => {
                let (dst, src1, src2, width) = self.parse_arith_operands(insn, ctx)?;
                let flags = if insn.sets_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                };
                push_op!(OpKind::Or {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                });
            }

            Mnemonic::EOR | Mnemonic::EORS => {
                let (dst, src1, src2, width) = self.parse_arith_operands(insn, ctx)?;
                let flags = if insn.sets_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                };
                push_op!(OpKind::Xor {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                });
            }

            Mnemonic::BIC | Mnemonic::BICS | Mnemonic::ORN | Mnemonic::EON => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let src1 = self.arm_reg(rn);
                    let width = self.reg_width(rd);
                    let flags = if insn.sets_flags {
                        FlagUpdate::All
                    } else {
                        FlagUpdate::None
                    };

                    let src2 = self.parse_operand2(insn, 2, ctx)?;

                    match insn.mnemonic {
                        Mnemonic::BIC | Mnemonic::BICS => {
                            push_op!(OpKind::AndNot {
                                dst,
                                src1,
                                src2,
                                width,
                                flags,
                            });
                        }
                        Mnemonic::ORN => {
                            let src2_reg =
                                self.materialize_src_operand(src2, width, pc, &mut ops, ctx);
                            let inverted = ctx.alloc_vreg();
                            push_op!(OpKind::Not {
                                dst: inverted,
                                src: src2_reg,
                                width,
                            });
                            push_op!(OpKind::Or {
                                dst,
                                src1,
                                src2: SrcOperand::Reg(inverted),
                                width,
                                flags,
                            });
                        }
                        Mnemonic::EON => {
                            let src2_reg =
                                self.materialize_src_operand(src2, width, pc, &mut ops, ctx);
                            let inverted = ctx.alloc_vreg();
                            push_op!(OpKind::Not {
                                dst: inverted,
                                src: src2_reg,
                                width,
                            });
                            push_op!(OpKind::Xor {
                                dst,
                                src1,
                                src2: SrcOperand::Reg(inverted),
                                width,
                                flags,
                            });
                        }
                        _ => unreachable!(),
                    }
                }
            }

            Mnemonic::MVN | Mnemonic::MVNS => {
                if let (Some(Operand::Reg(rd)), Some(src_op)) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    let src = self.operand_to_src(src_op, ctx)?;
                    let src = self.materialize_src_operand(src, width, pc, &mut ops, ctx);

                    push_op!(OpKind::Not { dst, src, width });
                }
            }

            Mnemonic::TST => {
                if let (Some(Operand::Reg(rn)), Some(op2)) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let tmp = ctx.alloc_vreg();
                    let src1 = self.arm_reg(rn);
                    let width = self.reg_width(rn);
                    let src2 = self.operand_to_src(op2, ctx)?;

                    push_op!(OpKind::And {
                        dst: tmp,
                        src1,
                        src2,
                        width,
                        flags: FlagUpdate::All,
                    });
                }
            }

            // =================================================================
            // Compare
            // =================================================================
            Mnemonic::CMP => {
                if let (Some(Operand::Reg(rn)), Some(op2)) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let tmp = ctx.alloc_vreg();
                    let src1 = self.arm_reg(rn);
                    let width = self.reg_width(rn);
                    let src2 = self.operand_to_src(op2, ctx)?;

                    push_op!(OpKind::Sub {
                        dst: tmp,
                        src1,
                        src2,
                        width,
                        flags: FlagUpdate::All,
                    });
                }
            }

            Mnemonic::CMN => {
                if let (Some(Operand::Reg(rn)), Some(op2)) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let tmp = ctx.alloc_vreg();
                    let src1 = self.arm_reg(rn);
                    let width = self.reg_width(rn);
                    let src2 = self.operand_to_src(op2, ctx)?;

                    push_op!(OpKind::Add {
                        dst: tmp,
                        src1,
                        src2,
                        width,
                        flags: FlagUpdate::All,
                    });
                }
            }

            // =================================================================
            // Move
            // =================================================================
            Mnemonic::MOV | Mnemonic::MOVS => {
                if let (Some(Operand::Reg(rd)), Some(src_op)) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    let src = self.operand_to_src(src_op, ctx)?;

                    push_op!(OpKind::Mov { dst, src, width });
                }
            }

            Mnemonic::MOVZ => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Imm(imm))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    let val = imm.effective_value();

                    push_op!(OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(val),
                        width,
                    });
                }
            }

            Mnemonic::MOVN => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Imm(imm))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    let val = !imm.effective_value();

                    push_op!(OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(val),
                        width,
                    });
                }
            }

            Mnemonic::MOVK => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Imm(imm))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    let shift = imm.shift;
                    let mask = !(0xFFFFu64 << shift);
                    let insert_val = (imm.value as u64) << shift;

                    let tmp = ctx.alloc_vreg();

                    push_op!(OpKind::And {
                        dst: tmp,
                        src1: self.arm_reg(rd),
                        src2: SrcOperand::Imm(mask as i64),
                        width,
                        flags: FlagUpdate::None,
                    });

                    push_op!(OpKind::Or {
                        dst,
                        src1: tmp,
                        src2: SrcOperand::Imm(insert_val as i64),
                        width,
                        flags: FlagUpdate::None,
                    });
                }
            }

            // =================================================================
            // Address Calculation
            // =================================================================
            Mnemonic::ADR => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Label(offset))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let target = (pc as i64).wrapping_add(*offset) as u64;

                    push_op!(OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(target as i64),
                        width: OpWidth::W64,
                    });
                }
            }

            Mnemonic::ADRP => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Label(offset))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let page = pc & !0xFFF;
                    let target = (page as i64).wrapping_add(*offset) as u64;

                    push_op!(OpKind::Mov {
                        dst,
                        src: SrcOperand::Imm(target as i64),
                        width: OpWidth::W64,
                    });
                }
            }

            // =================================================================
            // Shifts
            // =================================================================
            Mnemonic::LSL | Mnemonic::LSLS => {
                self.lift_shift(insn, ShiftOp::Lsl, pc, &mut ops, ctx)?;
            }

            Mnemonic::LSR | Mnemonic::LSRS => {
                self.lift_shift(insn, ShiftOp::Lsr, pc, &mut ops, ctx)?;
            }

            Mnemonic::ASR | Mnemonic::ASRS => {
                self.lift_shift(insn, ShiftOp::Asr, pc, &mut ops, ctx)?;
            }

            Mnemonic::ROR | Mnemonic::RORS => {
                self.lift_shift(insn, ShiftOp::Ror, pc, &mut ops, ctx)?;
            }

            Mnemonic::EXTR => {
                self.lift_extract(insn, pc, &mut ops, ctx)?;
            }

            Mnemonic::UBFX => {
                self.lift_bitfield(
                    insn,
                    BitfieldKind::Extract { sign_extend: false },
                    pc,
                    &mut ops,
                    ctx,
                )?;
            }

            Mnemonic::SBFX => {
                self.lift_bitfield(
                    insn,
                    BitfieldKind::Extract { sign_extend: true },
                    pc,
                    &mut ops,
                    ctx,
                )?;
            }

            Mnemonic::BFI | Mnemonic::BFC => {
                self.lift_bitfield(insn, BitfieldKind::Insert, pc, &mut ops, ctx)?;
            }

            Mnemonic::BFXIL => {
                self.lift_bitfield(insn, BitfieldKind::InsertLow, pc, &mut ops, ctx)?;
            }

            Mnemonic::UBFIZ => {
                self.lift_bitfield(
                    insn,
                    BitfieldKind::InsertZero { sign_extend: false },
                    pc,
                    &mut ops,
                    ctx,
                )?;
            }

            Mnemonic::SBFIZ => {
                self.lift_bitfield(
                    insn,
                    BitfieldKind::InsertZero { sign_extend: true },
                    pc,
                    &mut ops,
                    ctx,
                )?;
            }

            // =================================================================
            // Conditional Compare
            // =================================================================
            Mnemonic::CCMP | Mnemonic::CCMN => {
                self.lift_cond_compare(insn, pc, &mut ops, ctx)?;
            }

            // =================================================================
            // Extend
            // =================================================================
            Mnemonic::SXTB => {
                self.lift_extend(insn, OpWidth::W8, true, pc, &mut ops, ctx)?;
            }

            Mnemonic::SXTH => {
                self.lift_extend(insn, OpWidth::W16, true, pc, &mut ops, ctx)?;
            }

            Mnemonic::SXTW => {
                self.lift_extend(insn, OpWidth::W32, true, pc, &mut ops, ctx)?;
            }

            Mnemonic::UXTB => {
                self.lift_extend(insn, OpWidth::W8, false, pc, &mut ops, ctx)?;
            }

            Mnemonic::UXTH => {
                self.lift_extend(insn, OpWidth::W16, false, pc, &mut ops, ctx)?;
            }

            // =================================================================
            // Conditional Select
            // =================================================================
            Mnemonic::CSEL
            | Mnemonic::CSINC
            | Mnemonic::CSINV
            | Mnemonic::CSNEG
            | Mnemonic::CSET
            | Mnemonic::CSETM
            | Mnemonic::CINC
            | Mnemonic::CINV
            | Mnemonic::CNEG => {
                self.lift_cond_select(insn, pc, &mut ops, ctx)?;
            }

            // =================================================================
            // Bit manipulation
            // =================================================================
            Mnemonic::CLZ => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    push_op!(OpKind::Clz {
                        dst,
                        src: self.arm_reg(rn),
                        width,
                    });
                } else {
                    // Vector CLZ (two-register misc): per-lane count leading zeros.
                    self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Clz, false)?;
                }
            }

            Mnemonic::CLS => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let src = self.arm_reg(rn);
                    let width = self.reg_width(rd);

                    let sign_mask = ctx.alloc_vreg();
                    push_op!(OpKind::Sar {
                        dst: sign_mask,
                        src,
                        amount: SrcOperand::Imm(i64::from(width.bits() - 1)),
                        width,
                        flags: FlagUpdate::None,
                    });

                    let normalized = ctx.alloc_vreg();
                    push_op!(OpKind::Xor {
                        dst: normalized,
                        src1: src,
                        src2: SrcOperand::Reg(sign_mask),
                        width,
                        flags: FlagUpdate::None,
                    });

                    let leading = ctx.alloc_vreg();
                    push_op!(OpKind::Clz {
                        dst: leading,
                        src: normalized,
                        width,
                    });

                    push_op!(OpKind::Sub {
                        dst,
                        src1: leading,
                        src2: SrcOperand::Imm(1),
                        width,
                        flags: FlagUpdate::None,
                    });
                } else {
                    // Vector CLS: per-lane count leading sign bits.
                    self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Cls, false)?;
                }
            }

            Mnemonic::RBIT => {
                if let (Some(Operand::Reg(rd)), Some(Operand::Reg(rn))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let dst = self.dst_reg(rd, ctx);
                    let width = self.reg_width(rd);
                    push_op!(OpKind::Rbit {
                        dst,
                        src: self.arm_reg(rn),
                        width,
                    });
                } else {
                    // Vector RBIT: per-byte bit reverse.
                    self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Rbit, true)?;
                }
            }

            Mnemonic::REV => {
                self.lift_rev(insn, RevKind::Full, pc, &mut ops, ctx)?;
            }

            // REV16/REV32 have both a scalar GPR form (Operand::Reg) and a
            // vector form (Operand::FpReg) that reverses `elem`-sized elements
            // within each 16-/32-bit container. REV64 is vector-only (the GPR
            // 64-bit reverse is Mnemonic::REV).
            Mnemonic::REV16 => {
                if matches!(insn.operands.first(), Some(Operand::FpReg(_))) {
                    self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Rev16, false)?;
                } else {
                    self.lift_rev(insn, RevKind::Halfwords, pc, &mut ops, ctx)?;
                }
            }

            Mnemonic::REV32 => {
                if matches!(insn.operands.first(), Some(Operand::FpReg(_))) {
                    self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Rev32, false)?;
                } else {
                    self.lift_rev(insn, RevKind::Words, pc, &mut ops, ctx)?;
                }
            }

            Mnemonic::REV64 => {
                self.lift_vector_unary(insn, pc, &mut ops, VecUnaryOp::Rev64, false)?;
            }

            // Vector across-lanes integer reductions (scalar dst, vector src).
            Mnemonic::ADDV => {
                self.lift_vector_reduce(insn, pc, &mut ops, VecReduceOp::Add)?;
            }
            Mnemonic::SMAXV => {
                self.lift_vector_reduce(insn, pc, &mut ops, VecReduceOp::SMax)?;
            }
            Mnemonic::UMAXV => {
                self.lift_vector_reduce(insn, pc, &mut ops, VecReduceOp::UMax)?;
            }
            Mnemonic::SMINV => {
                self.lift_vector_reduce(insn, pc, &mut ops, VecReduceOp::SMin)?;
            }
            Mnemonic::UMINV => {
                self.lift_vector_reduce(insn, pc, &mut ops, VecReduceOp::UMin)?;
            }
            // SADDLV/UADDLV: widening add reduction (result is 2x the element).
            Mnemonic::SADDV => {
                self.lift_vector_reduce(insn, pc, &mut ops, VecReduceOp::SAddLong)?;
            }
            Mnemonic::UADDV => {
                self.lift_vector_reduce(insn, pc, &mut ops, VecReduceOp::UAddLong)?;
            }

            // Vector two-source permutes (ZIP/UZP/TRN).
            Mnemonic::ZIP1 => self.lift_vpermute(insn, pc, &mut ops, VecPermuteKind::Zip1)?,
            Mnemonic::ZIP2 => self.lift_vpermute(insn, pc, &mut ops, VecPermuteKind::Zip2)?,
            Mnemonic::UZP1 => self.lift_vpermute(insn, pc, &mut ops, VecPermuteKind::Uzp1)?,
            Mnemonic::UZP2 => self.lift_vpermute(insn, pc, &mut ops, VecPermuteKind::Uzp2)?,
            Mnemonic::TRN1 => self.lift_vpermute(insn, pc, &mut ops, VecPermuteKind::Trn1)?,
            Mnemonic::TRN2 => self.lift_vpermute(insn, pc, &mut ops, VecPermuteKind::Trn2)?,

            // Vector table lookup (TBL/TBX). Operand 0 = dst, 1 = table base
            // (Vn), 2 = index (Vm); len (bits[14:13]) gives #table regs - 1.
            Mnemonic::TBL | Mnemonic::TBX => {
                let (rd, rn, rm) = match (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    (
                        Some(Operand::FpReg(rd)),
                        Some(Operand::FpReg(rn)),
                        Some(Operand::FpReg(rm)),
                    ) => (rd, rn, rm),
                    _ => {
                        return Err(LiftError::Unsupported {
                            addr: pc,
                            mnemonic: format!("{:?}", insn.mnemonic),
                        });
                    }
                };
                let q = (insn.raw >> 30) & 1;
                let num_tables = (((insn.raw >> 13) & 0x3) + 1) as u8;
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VTableLookup {
                        dst: Self::fp_vreg(rd),
                        table: Self::fp_vreg(rn),
                        num_tables,
                        index: Self::fp_vreg(rm),
                        lanes: if q == 1 { 16 } else { 8 },
                        is_tbx: insn.mnemonic == Mnemonic::TBX,
                    },
                ));
            }

            // =================================================================
            // Load/Store
            // =================================================================
            Mnemonic::LDR | Mnemonic::LDAPUR | Mnemonic::LDTR => {
                if matches!(insn.operands.first(), Some(Operand::FpReg(_))) {
                    self.lift_vector_mem(insn, true, pc, &mut ops, ctx)?;
                } else {
                    let width = match insn.operands.first() {
                        Some(Operand::Reg(r)) if !r.is_64bit => MemWidth::B4,
                        _ => MemWidth::B8,
                    };
                    self.lift_load(insn, width, SignExtend::Zero, pc, &mut ops, ctx)?;
                }
            }

            Mnemonic::LDRB | Mnemonic::LDAPURB | Mnemonic::LDTRB => {
                self.lift_load(insn, MemWidth::B1, SignExtend::Zero, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDRH | Mnemonic::LDAPURH | Mnemonic::LDTRH => {
                self.lift_load(insn, MemWidth::B2, SignExtend::Zero, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDRSB | Mnemonic::LDAPURSB | Mnemonic::LDTRSB => {
                self.lift_load(insn, MemWidth::B1, SignExtend::Sign, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDRSH | Mnemonic::LDAPURSH | Mnemonic::LDTRSH => {
                self.lift_load(insn, MemWidth::B2, SignExtend::Sign, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDRSW | Mnemonic::LDAPURSW | Mnemonic::LDTRSW => {
                self.lift_load(insn, MemWidth::B4, SignExtend::Sign, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDXR | Mnemonic::LDAXR => {
                let width = match insn.operands.first() {
                    Some(Operand::Reg(r)) if !r.is_64bit => MemWidth::B4,
                    _ => MemWidth::B8,
                };
                self.lift_load_exclusive(insn, width, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDXRB | Mnemonic::LDAXRB => {
                self.lift_load_exclusive(insn, MemWidth::B1, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDXRH | Mnemonic::LDAXRH => {
                self.lift_load_exclusive(insn, MemWidth::B2, pc, &mut ops, ctx)?;
            }

            Mnemonic::STXR | Mnemonic::STLXR => {
                let width = match insn.operands.get(1) {
                    Some(Operand::Reg(r)) if !r.is_64bit => MemWidth::B4,
                    _ => MemWidth::B8,
                };
                self.lift_store_exclusive(insn, width, pc, &mut ops, ctx)?;
            }

            Mnemonic::STXRB | Mnemonic::STLXRB => {
                self.lift_store_exclusive(insn, MemWidth::B1, pc, &mut ops, ctx)?;
            }

            Mnemonic::STXRH | Mnemonic::STLXRH => {
                self.lift_store_exclusive(insn, MemWidth::B2, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDAR | Mnemonic::LDAPR | Mnemonic::LDLAR => {
                let width = match insn.operands.first() {
                    Some(Operand::Reg(r)) if !r.is_64bit => MemWidth::B4,
                    _ => MemWidth::B8,
                };
                self.lift_load(insn, width, SignExtend::Zero, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDARB | Mnemonic::LDAPRB | Mnemonic::LDLARB => {
                self.lift_load(insn, MemWidth::B1, SignExtend::Zero, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDARH | Mnemonic::LDAPRH | Mnemonic::LDLARH => {
                self.lift_load(insn, MemWidth::B2, SignExtend::Zero, pc, &mut ops, ctx)?;
            }

            Mnemonic::STR | Mnemonic::STLUR | Mnemonic::STTR => {
                if matches!(insn.operands.first(), Some(Operand::FpReg(_))) {
                    self.lift_vector_mem(insn, false, pc, &mut ops, ctx)?;
                } else {
                    let width = match insn.operands.first() {
                        Some(Operand::Reg(r)) if !r.is_64bit => MemWidth::B4,
                        _ => MemWidth::B8,
                    };
                    self.lift_store(insn, width, pc, &mut ops, ctx)?;
                }
            }

            Mnemonic::STRB | Mnemonic::STLURB | Mnemonic::STTRB => {
                self.lift_store(insn, MemWidth::B1, pc, &mut ops, ctx)?;
            }

            Mnemonic::STRH | Mnemonic::STLURH | Mnemonic::STTRH => {
                self.lift_store(insn, MemWidth::B2, pc, &mut ops, ctx)?;
            }

            Mnemonic::STLR | Mnemonic::STLLR => {
                let width = match insn.operands.first() {
                    Some(Operand::Reg(r)) if !r.is_64bit => MemWidth::B4,
                    _ => MemWidth::B8,
                };
                self.lift_store(insn, width, pc, &mut ops, ctx)?;
            }

            Mnemonic::STLRB | Mnemonic::STLLRB => {
                self.lift_store(insn, MemWidth::B1, pc, &mut ops, ctx)?;
            }

            Mnemonic::STLRH | Mnemonic::STLLRH => {
                self.lift_store(insn, MemWidth::B2, pc, &mut ops, ctx)?;
            }

            Mnemonic::SWP
            | Mnemonic::SWPA
            | Mnemonic::SWPAL
            | Mnemonic::SWPL
            | Mnemonic::CAS
            | Mnemonic::CASA
            | Mnemonic::CASAL
            | Mnemonic::CASL
            | Mnemonic::LDADD
            | Mnemonic::LDADDA
            | Mnemonic::LDADDAL
            | Mnemonic::LDADDL
            | Mnemonic::LDCLR
            | Mnemonic::LDEOR
            | Mnemonic::LDSET
            | Mnemonic::LDSMAX
            | Mnemonic::LDSMIN
            | Mnemonic::LDUMAX
            | Mnemonic::LDUMIN => {
                if matches!(
                    insn.mnemonic,
                    Mnemonic::CAS | Mnemonic::CASA | Mnemonic::CASAL | Mnemonic::CASL
                ) {
                    self.lift_cas(insn, pc, &mut ops, ctx)?;
                } else {
                    self.lift_atomic_rmw(insn, pc, &mut ops, ctx)?;
                }
            }

            Mnemonic::LDP | Mnemonic::LDNP | Mnemonic::LDTP | Mnemonic::LDTNP => {
                self.lift_load_pair(insn, SignExtend::Zero, pc, &mut ops, ctx)?;
            }

            Mnemonic::LDPSW => {
                self.lift_load_pair(insn, SignExtend::Sign, pc, &mut ops, ctx)?;
            }

            Mnemonic::STP | Mnemonic::STNP | Mnemonic::STTP | Mnemonic::STTNP => {
                self.lift_store_pair(insn, pc, &mut ops, ctx)?;
            }

            Mnemonic::PRFM => {
                push_op!(OpKind::Nop);
            }

            // =================================================================
            // Branches
            // =================================================================
            Mnemonic::B => {
                if let Some(Operand::Label(offset)) = insn.operands.get(0) {
                    let target = (pc as i64).wrapping_add(*offset) as u64;
                    control = ControlFlow::Branch { target };
                }
            }

            Mnemonic::BL => {
                if let Some(Operand::Label(offset)) = insn.operands.get(0) {
                    let target = (pc as i64).wrapping_add(*offset) as u64;
                    let ret_addr = pc + 4;

                    push_op!(OpKind::Mov {
                        dst: VReg::Arch(ArchReg::Arm(ArmReg::X(30))),
                        src: SrcOperand::Imm(ret_addr as i64),
                        width: OpWidth::W64,
                    });

                    control = ControlFlow::Call {
                        target: CallTarget::GuestAddr(target),
                    };
                }
            }

            Mnemonic::BR => {
                if let Some(Operand::Reg(rn)) = insn.operands.get(0) {
                    control = ControlFlow::IndirectBranch {
                        target: self.arm_reg(rn),
                    };
                }
            }

            Mnemonic::BLR => {
                if let Some(Operand::Reg(rn)) = insn.operands.get(0) {
                    let ret_addr = pc + 4;

                    push_op!(OpKind::Mov {
                        dst: VReg::Arch(ArchReg::Arm(ArmReg::X(30))),
                        src: SrcOperand::Imm(ret_addr as i64),
                        width: OpWidth::W64,
                    });

                    control = ControlFlow::Call {
                        target: CallTarget::Indirect(self.arm_reg(rn)),
                    };
                }
            }

            Mnemonic::RET => {
                let target = match insn.operands.get(0) {
                    Some(Operand::Reg(rn)) => self.arm_reg(rn),
                    _ => VReg::Arch(ArchReg::Arm(ArmReg::X(30))),
                };
                control = ControlFlow::IndirectBranch { target };
            }

            Mnemonic::BLRAA
            | Mnemonic::BLRAB
            | Mnemonic::RETAA
            | Mnemonic::RETAB
            | Mnemonic::PACIA
            | Mnemonic::PACIB
            | Mnemonic::PACDA
            | Mnemonic::PACDB
            | Mnemonic::AUTIA
            | Mnemonic::AUTIB
            | Mnemonic::AUTDA
            | Mnemonic::AUTDB
            | Mnemonic::PACIZA
            | Mnemonic::PACIZB
            | Mnemonic::PACDZA
            | Mnemonic::PACDZB
            | Mnemonic::AUTIZA
            | Mnemonic::AUTIZB
            | Mnemonic::AUTDZA
            | Mnemonic::AUTDZB
            | Mnemonic::XPACI
            | Mnemonic::XPACD
            | Mnemonic::PACGA
            | Mnemonic::SUBP
            | Mnemonic::SUBPS
            | Mnemonic::IRG
            | Mnemonic::GMI => {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: format!("{:?}", insn.mnemonic),
                });
            }

            Mnemonic::BCC => {
                if let (Some(Operand::Label(offset)), Some(cond)) =
                    (insn.operands.get(0), insn.cond)
                {
                    let target = (pc as i64).wrapping_add(*offset) as u64;
                    let fallthrough = pc + 4;
                    control = ControlFlow::CondBranch {
                        cond: self.arm_cond(cond),
                        target,
                        fallthrough,
                    };
                }
            }

            Mnemonic::CBZ => {
                if let (Some(Operand::Reg(rn)), Some(Operand::Label(offset))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let target = (pc as i64).wrapping_add(*offset) as u64;
                    let fallthrough = pc + 4;
                    let width = self.reg_width(rn);
                    let cond = if width == OpWidth::W32 {
                        let tmp = ctx.alloc_vreg();
                        push_op!(OpKind::ZeroExtend {
                            dst: tmp,
                            src: self.arm_reg(rn),
                            from_width: OpWidth::W32,
                            to_width: OpWidth::W64,
                        });
                        tmp
                    } else {
                        self.arm_reg(rn)
                    };

                    control = ControlFlow::CondBranchReg {
                        cond,
                        taken: fallthrough,
                        not_taken: target,
                    };
                }
            }

            Mnemonic::CBNZ => {
                if let (Some(Operand::Reg(rn)), Some(Operand::Label(offset))) =
                    (insn.operands.get(0), insn.operands.get(1))
                {
                    let target = (pc as i64).wrapping_add(*offset) as u64;
                    let fallthrough = pc + 4;
                    let width = self.reg_width(rn);
                    let cond = if width == OpWidth::W32 {
                        let tmp = ctx.alloc_vreg();
                        push_op!(OpKind::ZeroExtend {
                            dst: tmp,
                            src: self.arm_reg(rn),
                            from_width: OpWidth::W32,
                            to_width: OpWidth::W64,
                        });
                        tmp
                    } else {
                        self.arm_reg(rn)
                    };

                    control = ControlFlow::CondBranchReg {
                        cond,
                        taken: target,
                        not_taken: fallthrough,
                    };
                }
            }

            Mnemonic::TBZ | Mnemonic::TBNZ => {
                if let (
                    Some(Operand::Reg(rt)),
                    Some(Operand::Imm(bit)),
                    Some(Operand::Label(offset)),
                ) = (
                    insn.operands.get(0),
                    insn.operands.get(1),
                    insn.operands.get(2),
                ) {
                    let bit = bit.effective_value() as u32;
                    let target = (pc as i64).wrapping_add(*offset) as u64;
                    let fallthrough = pc + 4;
                    let masked = ctx.alloc_vreg();
                    let mask = (1u64 << bit) as i64;

                    push_op!(OpKind::And {
                        dst: masked,
                        src1: self.arm_reg(rt),
                        src2: SrcOperand::Imm64(mask),
                        width: self.reg_width(rt),
                        flags: FlagUpdate::None,
                    });

                    let (taken, not_taken) = if insn.mnemonic == Mnemonic::TBNZ {
                        (target, fallthrough)
                    } else {
                        (fallthrough, target)
                    };
                    control = ControlFlow::CondBranchReg {
                        cond: masked,
                        taken,
                        not_taken,
                    };
                }
            }

            // =================================================================
            // System
            // =================================================================
            Mnemonic::NOP
            | Mnemonic::BTI
            | Mnemonic::DGH
            | Mnemonic::YIELD
            | Mnemonic::WFE
            | Mnemonic::WFI
            | Mnemonic::WFET
            | Mnemonic::WFIT
            | Mnemonic::SEV
            | Mnemonic::SEVL => {
                push_op!(OpKind::Nop);
            }

            Mnemonic::HINT => {
                let imm = match insn.operands.first() {
                    Some(Operand::Imm(imm)) => imm.effective_value(),
                    _ => return Err(LiftError::Internal("invalid HINT operands".to_string())),
                };
                match imm {
                    // DGH is a data-gathering hint with no visible architectural state.
                    6 => push_op!(OpKind::Nop),
                    _ if self.strict => {
                        return Err(LiftError::Unsupported {
                            addr: pc,
                            mnemonic: format!("HINT #{imm}"),
                        });
                    }
                    _ => {
                        control = ControlFlow::Trap {
                            kind: TrapKind::Undefined,
                        };
                    }
                }
            }

            Mnemonic::CFINV => {
                push_op!(OpKind::Xor {
                    dst: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
                    src1: VReg::Arch(ArchReg::Arm(ArmReg::Nzcv)),
                    src2: SrcOperand::Imm(NZCV_C),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                });
            }

            Mnemonic::AXFLAG => {
                self.lift_axflag(pc, &mut ops, ctx);
            }

            Mnemonic::XAFLAG => {
                self.lift_xaflag(pc, &mut ops, ctx);
            }

            Mnemonic::MRS => {
                self.lift_mrs(insn, pc, &mut ops, ctx)?;
            }

            Mnemonic::MSR => {
                self.lift_msr(insn, pc, &mut ops, ctx)?;
            }

            Mnemonic::SVC => {
                control = ControlFlow::Syscall;
            }

            Mnemonic::BRK => {
                control = ControlFlow::Trap {
                    kind: TrapKind::Breakpoint,
                };
            }

            Mnemonic::HLT => {
                control = ControlFlow::Trap {
                    kind: TrapKind::Halt,
                };
            }

            Mnemonic::UDF => {
                control = ControlFlow::Trap {
                    kind: TrapKind::Undefined,
                };
            }

            Mnemonic::CLREX => {
                push_op!(OpKind::ClearExclusive);
            }

            Mnemonic::DMB | Mnemonic::DSB | Mnemonic::ISB | Mnemonic::SB => {
                push_op!(OpKind::Fence {
                    kind: FenceKind::Full,
                });
            }

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
                    return Ok((ops, control));
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
                // FADD/FSUB/FMUL/FDIV map to the native IEEE ops; FMAX/FMIN map
                // to the lowerer's native AArch64 FMAX/FMIN (same ARM NaN
                // propagation). The numeric-IEEE variants FMAXNM/FMINNM differ
                // (maxNum/minNum) and have no dedicated lowering, so they bail
                // to the interpreter.
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
                    return Ok((ops, control));
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
                    return Ok((ops, control));
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

            // =================================================================
            // NEON Integer Three-Same
            // =================================================================
            Mnemonic::VADD | Mnemonic::VSUB | Mnemonic::VMUL | Mnemonic::VMAX | Mnemonic::VMIN => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vec rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vec rn".to_string())),
                };
                let rm = match insn.operands.get(2) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vec rm".to_string())),
                };
                let q = (insn.raw >> 30) & 1;
                let size = (insn.raw >> 22) & 3;
                let (elem, lanes) = match (size, q) {
                    (0, 0) => (VecElementType::I8, 8),
                    (0, 1) => (VecElementType::I8, 16),
                    (1, 0) => (VecElementType::I16, 4),
                    (1, 1) => (VecElementType::I16, 8),
                    (2, 0) => (VecElementType::I32, 2),
                    (2, 1) => (VecElementType::I32, 4),
                    (3, 0) => (VecElementType::I64, 1),
                    (3, 1) => (VecElementType::I64, 2),
                    _ => (VecElementType::I8, 16),
                };
                let kind = match insn.mnemonic {
                    Mnemonic::VADD => OpKind::VAdd {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        elem,
                        lanes,
                    },
                    Mnemonic::VSUB => OpKind::VSub {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        elem,
                        lanes,
                    },
                    Mnemonic::VMUL => OpKind::VMul {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        elem,
                        lanes,
                    },
                    Mnemonic::VMAX => OpKind::VMax {
                        dst: Self::fp_vreg(rd),
                        src1: Self::fp_vreg(rn),
                        src2: Self::fp_vreg(rm),
                        elem,
                        lanes,
                    },
                    Mnemonic::VMIN => {
                        let u = (insn.raw >> 29) & 1;
                        OpKind::VMin {
                            dst: Self::fp_vreg(rd),
                            src1: Self::fp_vreg(rn),
                            src2: Self::fp_vreg(rm),
                            elem,
                            lanes,
                            signed: u == 0,
                        }
                    }
                    _ => unreachable!(),
                };
                push_op!(kind);
            }

            // =================================================================
            // NEON Logical Three-Same
            // =================================================================
            Mnemonic::VAND
            | Mnemonic::VBIC
            | Mnemonic::VORR
            | Mnemonic::VORN
            | Mnemonic::VEOR
            | Mnemonic::VBSL
            | Mnemonic::VBIT
            | Mnemonic::VBIF => {
                let rd = match insn.operands.get(0) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vec rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vec rn".to_string())),
                };
                let rm = match insn.operands.get(2) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vec rm".to_string())),
                };
                let q = (insn.raw >> 30) & 1;
                let vec_width = if q == 1 {
                    VecWidth::V128
                } else {
                    VecWidth::V64
                };
                match insn.mnemonic {
                    Mnemonic::VAND => {
                        push_op!(OpKind::VAnd {
                            dst: Self::fp_vreg(rd),
                            src1: Self::fp_vreg(rn),
                            src2: Self::fp_vreg(rm),
                            width: vec_width,
                        });
                    }
                    Mnemonic::VORR => {
                        push_op!(OpKind::VOr {
                            dst: Self::fp_vreg(rd),
                            src1: Self::fp_vreg(rn),
                            src2: Self::fp_vreg(rm),
                            width: vec_width,
                        });
                    }
                    Mnemonic::VEOR => {
                        push_op!(OpKind::VXor {
                            dst: Self::fp_vreg(rd),
                            src1: Self::fp_vreg(rn),
                            src2: Self::fp_vreg(rm),
                            width: vec_width,
                        });
                    }
                    Mnemonic::VBIC => {
                        let not_rm = ctx.alloc_vreg();
                        push_op!(OpKind::VXor {
                            dst: not_rm,
                            src1: Self::fp_vreg(rm),
                            src2: VReg::Imm(-1),
                            width: vec_width,
                        });
                        push_op!(OpKind::VAnd {
                            dst: Self::fp_vreg(rd),
                            src1: Self::fp_vreg(rn),
                            src2: not_rm,
                            width: vec_width,
                        });
                    }
                    Mnemonic::VORN => {
                        let not_rm = ctx.alloc_vreg();
                        push_op!(OpKind::VXor {
                            dst: not_rm,
                            src1: Self::fp_vreg(rm),
                            src2: VReg::Imm(-1),
                            width: vec_width,
                        });
                        push_op!(OpKind::VOr {
                            dst: Self::fp_vreg(rd),
                            src1: Self::fp_vreg(rn),
                            src2: not_rm,
                            width: vec_width,
                        });
                    }
                    Mnemonic::VBSL => {
                        push_op!(OpKind::VBitSelect {
                            dst: Self::fp_vreg(rd),
                            mask: Self::fp_vreg(rd),
                            src_true: Self::fp_vreg(rn),
                            src_false: Self::fp_vreg(rm),
                            width: vec_width,
                        });
                    }
                    Mnemonic::VBIT => {
                        push_op!(OpKind::VBitSelect {
                            dst: Self::fp_vreg(rd),
                            mask: Self::fp_vreg(rm),
                            src_true: Self::fp_vreg(rn),
                            src_false: Self::fp_vreg(rd),
                            width: vec_width,
                        });
                    }
                    Mnemonic::VBIF => {
                        push_op!(OpKind::VBitSelect {
                            dst: Self::fp_vreg(rd),
                            mask: Self::fp_vreg(rm),
                            src_true: Self::fp_vreg(rd),
                            src_false: Self::fp_vreg(rn),
                            width: vec_width,
                        });
                    }
                    _ => unreachable!(),
                };
            }

            // =================================================================
            // NEON vector fused multiply-add (FMLA / FMLS)
            // =================================================================
            Mnemonic::FMLA | Mnemonic::FMLS => {
                let rd = match insn.operands.first() {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vfma rd".to_string())),
                };
                let rn = match insn.operands.get(1) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vfma rn".to_string())),
                };
                let rm = match insn.operands.get(2) {
                    Some(Operand::FpReg(r)) => r,
                    _ => return Err(LiftError::Internal("missing vfma rm".to_string())),
                };
                // FP three-same: sz (bit22) selects F32/F64, Q (bit30) the width.
                let q = (insn.raw >> 30) & 1;
                let sz = (insn.raw >> 22) & 1;
                let (elem, lanes) = if sz == 0 {
                    (VecElementType::F32, if q == 1 { 4u8 } else { 2 })
                } else {
                    (VecElementType::F64, 2) // 2D (Q=1); 1D is reserved
                };
                push_op!(OpKind::VFma {
                    dst: Self::fp_vreg(rd),
                    src1: Self::fp_vreg(rn),
                    src2: Self::fp_vreg(rm),
                    // FMLA/FMLS are destructive: the destination is the accumulator
                    // (vd = vd ± vn*vm), matching the native vector FMLA encoding.
                    acc: Self::fp_vreg(rd),
                    elem,
                    lanes,
                    negate_product: insn.mnemonic == Mnemonic::FMLS,
                    negate_acc: false,
                });
            }

            // =================================================================
            // Unhandled
            // =================================================================
            _ => {
                if self.strict {
                    return Err(LiftError::Unsupported {
                        addr: pc,
                        mnemonic: format!("{:?}", insn.mnemonic),
                    });
                }
                control = ControlFlow::Trap {
                    kind: TrapKind::Undefined,
                };
            }
        }

        Ok((ops, control))
    }
}
