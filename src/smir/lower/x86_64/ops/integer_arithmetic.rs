//! Integer-arithmetic lowering

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86VecAlign, X86VecMap, X86X87ControlKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, DispSize, FenceKind, FpRoundMode, GuestAddr, MemWidth,
    OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType,
    VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{
    CallTarget, SmirBlock, SmirFunction, Terminator, X86InstructionBytes,
    x86_evex_native_replay_spans,
};

use crate::smir::lower::regalloc::{PhysReg, RegAlloc, RegLocation};
use crate::smir::lower::{
    CodeBuffer, LowerError, LowerResult, RelocKind, RelocTarget, Relocation, SmirLowerer,
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET,
    X86_GUEST_PAIR_STORE_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_GUEST_TSC_AUX_OFFSET, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};

impl X86_64Lowerer {
    pub(crate) fn lower_op_integer_arithmetic(
        &mut self,
        op: &crate::smir::ir::ops::SmirOp,
    ) -> Result<(), LowerError> {
        let is_non_accumulating_madd = matches!(
            &op.kind,
            OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I16,
                src1_unsigned: true,
                saturate: true,
                zeroing: false,
                ..
            } | OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I16,
                acc_elem: VecElementType::I32,
                src1_unsigned: false,
                saturate: false,
                zeroing: false,
                ..
            }
        );
        let is_classic_mpsadbw = matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VMpsadbw { .. },
                Some(X86OpHint::SseOp { .. } | X86OpHint::VexOp { .. })
            )
        );
        if !is_non_accumulating_madd && !is_classic_mpsadbw {
            if let Some(result) = avx10::Avx10Lowerer::new().try_lower(&op.kind, &mut self.code) {
                return result;
            }
        }

        let alu_hint = match op.x86_hint {
            Some(X86OpHint::AluEncoding(enc)) => Some(enc),
            _ => None,
        };

        match &op.kind {
            // ================================================================
            // Integer Arithmetic
            // ================================================================
            OpKind::Add {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                if Self::alu_touches_state_backed_stack_gpr(&op.kind) {
                    return self.lower_state_backed_stack_gpr_alu(
                        false, *dst, *src1, src2, *width, *flags,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "Add",
                                &[dst_reg, src1_reg, src2_reg],
                            )?;
                        }
                        let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                        let operand = if dst_reg != src1_reg && dst_reg == src2_reg {
                            src1_reg
                        } else {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            src2_reg
                        };
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_alu_rr_dir(0x00, dst_reg, operand, *width, encoding);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("Add", &[dst_reg, src1_reg])?;
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src1_reg {
                            emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && dst_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x04, *val, *width);
                        } else {
                            emitter.emit_add_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Add with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Sub {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                if Self::alu_touches_state_backed_stack_gpr(&op.kind) {
                    return self
                        .lower_state_backed_stack_gpr_alu(true, *dst, *src1, src2, *width, *flags);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "Sub",
                                &[dst_reg, src1_reg, src2_reg],
                            )?;
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        if dst_reg != src1_reg && dst_reg == src2_reg {
                            self.emit_noncommutative_alu_alias(
                                "Sub alias",
                                0x28,
                                dst_reg,
                                src1_reg,
                                src2_reg,
                                *width,
                            )?;
                        } else {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            if dst_reg != src1_reg {
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                            emitter.emit_alu_rr_dir(0x28, dst_reg, src2_reg, *width, encoding);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("Sub", &[dst_reg, src1_reg])?;
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src1_reg {
                            emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && dst_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x2C, *val, *width);
                        } else {
                            emitter.emit_sub_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Sub with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Adc {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                        if dst_reg != src1_reg && dst_reg == src2_reg {
                            // ADC is commutative (including its carry-in), so an
                            // APX NDD destination that aliases source 2 can use
                            // the old destination directly instead of destroying
                            // it with `mov dst, src1` first.
                            emitter.emit_alu_rr_dir(0x10, dst_reg, src1_reg, *width, encoding);
                        } else {
                            if dst_reg != src1_reg {
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            emitter.emit_alu_rr_dir(0x10, dst_reg, src2_reg, *width, encoding);
                        }
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src1_reg {
                            emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                        }
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && dst_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x14, *val, *width);
                        } else {
                            emitter.emit_adc_ri(dst_reg, *val, *width);
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Adc with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Sbb {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if dst_reg != src1_reg && dst_reg == src2_reg {
                            self.emit_noncommutative_alu_alias(
                                "Sbb alias",
                                0x18,
                                dst_reg,
                                src1_reg,
                                src2_reg,
                                *width,
                            )?;
                        } else {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            if dst_reg != src1_reg {
                                emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                            }
                            let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                            emitter.emit_alu_rr_dir(0x18, dst_reg, src2_reg, *width, encoding);
                        }
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if dst_reg != src1_reg {
                            emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                        }
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && dst_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x1C, *val, *width);
                        } else {
                            emitter.emit_sbb_ri(dst_reg, *val, *width);
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Sbb with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Neg {
                dst,
                src,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_neg_candidate(op) {
                    if !x86_state_backed_gpr_neg_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Neg".to_string(),
                            operand: format!("invalid x86 GPR negation {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_neg(*dst, *src, *width, *flags);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let preserve_flags = !flags.updates_any();
                if preserve_flags {
                    Self::ensure_flag_stack_operands_safe("Neg", &[dst_reg, src_reg])?;
                }

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                if preserve_flags {
                    self.code.emit_u8(0x9C); // pushfq
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_neg(dst_reg, *width);
                if preserve_flags {
                    self.code.emit_u8(0x9D); // popfq
                }
            }

            OpKind::Inc {
                dst,
                src,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_inc_dec_candidate(op) {
                    if !x86_state_backed_gpr_inc_dec_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Inc".to_string(),
                            operand: format!("invalid x86 GPR increment {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_inc_dec(*dst, *src, *width, *flags, false);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let preserve_flags = !flags.updates_any();
                if preserve_flags {
                    Self::ensure_flag_stack_operands_safe("Inc", &[dst_reg, src_reg])?;
                }

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                if preserve_flags {
                    self.code.emit_u8(0x9C); // pushfq
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_inc(dst_reg, *width);
                if preserve_flags {
                    self.code.emit_u8(0x9D); // popfq
                }
            }

            OpKind::Dec {
                dst,
                src,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_inc_dec_candidate(op) {
                    if !x86_state_backed_gpr_inc_dec_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Dec".to_string(),
                            operand: format!("invalid x86 GPR decrement {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_inc_dec(*dst, *src, *width, *flags, true);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let preserve_flags = !flags.updates_any();
                if preserve_flags {
                    Self::ensure_flag_stack_operands_safe("Dec", &[dst_reg, src_reg])?;
                }

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                if preserve_flags {
                    self.code.emit_u8(0x9C); // pushfq
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_dec(dst_reg, *width);
                if preserve_flags {
                    self.code.emit_u8(0x9D); // popfq
                }
            }

            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => {
                let preserve_flags = !flags.updates_any();
                let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
                let implicit_byte =
                    *width == OpWidth::W8 && dst_hi.is_none() && *dst_lo == rax && *src1 == rax;
                // For two-operand IMUL (dst = src1 * src2), we use the efficient form
                // For widening multiply (dst_hi:dst_lo = src1 * src2), we use IMUL with RAX
                // W8 implicit IMUL also enters this path: its complete product
                // is AX, represented by dst_lo=RAX and no separate high VReg.
                if dst_hi.is_some() || implicit_byte {
                    // Widening multiply: IMUL r/m -> RDX:RAX = RAX * r/m
                    // Move src1 to RAX
                    let src1_reg = self.get_reg(*src1)?;

                    // Get src2 and do IMUL
                    match src2 {
                        SrcOperand::Reg(r) => {
                            let src2_reg = self.get_reg(*r)?;
                            if preserve_flags {
                                Self::ensure_flag_stack_operands_safe(
                                    "MulS",
                                    &[src1_reg, src2_reg],
                                )?;
                            }
                            {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                            }
                            if preserve_flags {
                                self.code.emit_u8(0x9C); // pushfq
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_imul(src2_reg, *width);
                            if preserve_flags {
                                self.code.emit_u8(0x9D); // popfq
                            }
                        }
                        SrcOperand::Imm(val) => {
                            // Load immediate to a temp register
                            let temp = self.regalloc.get_scratch()?;
                            if preserve_flags {
                                Self::ensure_flag_stack_operands_safe("MulS", &[src1_reg, temp])?;
                            }
                            {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                            }
                            {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_ri(temp, *val, *width);
                            }
                            if preserve_flags {
                                self.code.emit_u8(0x9C); // pushfq
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_imul(temp, *width);
                            if preserve_flags {
                                self.code.emit_u8(0x9D); // popfq
                            }
                            self.regalloc.free_temp(temp);
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "MulS with shifted operand".to_string(),
                            });
                        }
                    }

                    // Move results to destination registers
                    let dst_lo_reg = self.get_dst_reg(*dst_lo)?;
                    if dst_lo_reg != PhysReg::Rax {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_lo_reg, PhysReg::Rax, *width);
                    }

                    if let Some(hi) = dst_hi {
                        let dst_hi_reg = self.get_dst_reg(*hi)?;
                        if dst_hi_reg != PhysReg::Rdx {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rr(dst_hi_reg, PhysReg::Rdx, *width);
                        }
                    }
                } else {
                    if *width == OpWidth::W8 {
                        return Err(LowerError::InvalidOperand {
                            op: "MulS".to_string(),
                            operand: "W8 requires the implicit AX product shape".to_string(),
                        });
                    }
                    // Two-operand form: dst = src1 * src2
                    let dst_reg = self.get_dst_reg(*dst_lo)?;
                    let src1_reg = self.get_reg(*src1)?;

                    match src2 {
                        SrcOperand::Reg(r) => {
                            let src2_reg = self.get_reg(*r)?;
                            if preserve_flags {
                                Self::ensure_flag_stack_operands_safe(
                                    "MulS",
                                    &[dst_reg, src1_reg, src2_reg],
                                )?;
                            }
                            if dst_reg != src1_reg && dst_reg == src2_reg {
                                if preserve_flags {
                                    self.code.emit_u8(0x9C); // pushfq
                                }
                                let mut emitter = X86Emitter::new(&mut self.code);
                                // Two-operand IMUL is commutative. Consume src1
                                // directly when an APX NDD destination aliases
                                // source 2, preserving the old destination.
                                emitter.emit_imul_rr(dst_reg, src1_reg, *width);
                                if preserve_flags {
                                    self.code.emit_u8(0x9D); // popfq
                                }
                            } else {
                                // Move src1 to dst, then IMUL dst, src2.
                                if dst_reg != src1_reg {
                                    let mut emitter = X86Emitter::new(&mut self.code);
                                    emitter.emit_mov_rr(dst_reg, src1_reg, *width);
                                }
                                if preserve_flags {
                                    self.code.emit_u8(0x9C); // pushfq
                                }
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_imul_rr(dst_reg, src2_reg, *width);
                                if preserve_flags {
                                    self.code.emit_u8(0x9D); // popfq
                                }
                            }
                        }
                        SrcOperand::Imm(val) => {
                            // Three-operand form: IMUL dst, src1, imm
                            if preserve_flags {
                                Self::ensure_flag_stack_operands_safe(
                                    "MulS",
                                    &[dst_reg, src1_reg],
                                )?;
                            }
                            let use_imm8 = match op.x86_hint {
                                Some(X86OpHint::ImulImm8) => true,
                                Some(X86OpHint::ImulImm32) => false,
                                _ => *val >= -128 && *val <= 127,
                            };
                            if preserve_flags {
                                self.code.emit_u8(0x9C); // pushfq
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_imul_rri_force(
                                dst_reg,
                                src1_reg,
                                *val as i32,
                                *width,
                                use_imm8,
                            );
                            if preserve_flags {
                                self.code.emit_u8(0x9D); // popfq
                            }
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "MulS with shifted operand".to_string(),
                            });
                        }
                    }
                }
            }

            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } => {
                if matches!(op.x86_hint, Some(X86OpHint::Mulx)) {
                    if !matches!(width, OpWidth::W32 | OpWidth::W64)
                        || *flags != FlagUpdate::None
                        || *src1 != VReg::Arch(ArchReg::X86(X86Reg::Rdx))
                    {
                        return Err(LowerError::InvalidOperand {
                            op: "MULX".to_string(),
                            operand: format!(
                                "requires W32/W64, FlagUpdate::None, and implicit RDX; got {width:?}, {flags:?}, {src1:?}"
                            ),
                        });
                    }
                    let Some(dst_hi) = dst_hi else {
                        return Err(LowerError::InvalidOperand {
                            op: "MULX".to_string(),
                            operand: "missing upper-half destination".to_string(),
                        });
                    };
                    let SrcOperand::Reg(src2) = src2 else {
                        return Err(LowerError::InvalidOperand {
                            op: "MULX".to_string(),
                            operand: "source must be a register after lifting".to_string(),
                        });
                    };

                    let dst_lo_reg = self.get_dst_reg(*dst_lo)?;
                    let dst_hi_reg = self.get_dst_reg(*dst_hi)?;
                    let src2_reg = self.get_reg(*src2)?;
                    Self::ensure_flag_stack_operands_safe(
                        "MULX",
                        &[dst_lo_reg, dst_hi_reg, PhysReg::Rdx, src2_reg],
                    )?;

                    // MULX dest_hi, dest_lo, src encodes the upper destination
                    // in ModR/M.reg and the lower destination in VEX.vvvv. The
                    // instruction reads implicit RDX and all explicit sources
                    // before either destination is committed, so every source/
                    // destination alias is natively safe. If both destinations
                    // alias, the ISA specifies that the upper half survives.
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_vex_bmi_rr_pp(
                        0xF6,
                        X86SsePrefix::Repne,
                        dst_hi_reg,
                        src2_reg,
                        dst_lo_reg,
                        *width,
                    );
                } else {
                    let preserve_flags = !flags.updates_any();
                    // Unsigned multiply always uses RAX
                    // MUL r/m -> RDX:RAX = RAX * r/m
                    let src1_reg = self.get_reg(*src1)?;

                    match src2 {
                        SrcOperand::Reg(r) => {
                            let src2_reg = self.get_reg(*r)?;
                            if preserve_flags {
                                Self::ensure_flag_stack_operands_safe(
                                    "MulU",
                                    &[src1_reg, src2_reg],
                                )?;
                            }
                            {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                            }
                            if preserve_flags {
                                self.code.emit_u8(0x9C); // pushfq
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mul(src2_reg, *width);
                            if preserve_flags {
                                self.code.emit_u8(0x9D); // popfq
                            }
                        }
                        SrcOperand::Imm(val) => {
                            let temp = self.regalloc.get_scratch()?;
                            if preserve_flags {
                                Self::ensure_flag_stack_operands_safe("MulU", &[src1_reg, temp])?;
                            }
                            {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                            }
                            {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_ri(temp, *val, *width);
                            }
                            if preserve_flags {
                                self.code.emit_u8(0x9C); // pushfq
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mul(temp, *width);
                            if preserve_flags {
                                self.code.emit_u8(0x9D); // popfq
                            }
                            self.regalloc.free_temp(temp);
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: "MulU with shifted operand".to_string(),
                            });
                        }
                    }

                    // Move results to destination registers
                    let dst_lo_reg = self.get_dst_reg(*dst_lo)?;
                    if dst_lo_reg != PhysReg::Rax {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_lo_reg, PhysReg::Rax, *width);
                    }

                    if let Some(hi) = dst_hi {
                        let dst_hi_reg = self.get_dst_reg(*hi)?;
                        if dst_hi_reg != PhysReg::Rdx {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rr(dst_hi_reg, PhysReg::Rdx, *width);
                        }
                    }
                }
            }

            OpKind::DivU {
                quot,
                rem,
                src1,
                src2,
                width,
                flags,
            } => {
                let preserve_flags = !flags.updates_any();
                let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
                let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
                let x86_implicit = *src1 == rax
                    && *quot == rax
                    && *rem
                        == if *width == OpWidth::W8 {
                            None
                        } else {
                            Some(rdx)
                        };

                // Unsigned divide: RDX:RAX / src2 -> RAX (quot), RDX (rem)
                // Generic non-x86 lowering uses a zero high half. Lifted x86
                // implicit DIV already carries the high half in RDX.
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("DivU", &[src1_reg, src2_reg])?;
                        }
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Move dividend to RAX
                            emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        if !x86_implicit {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Zero RDX
                            emitter.emit_zero_rdx();
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_div(src2_reg, *width);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        // DIV doesn't support immediate, need to load into temp
                        let temp = self.regalloc.get_scratch()?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("DivU", &[src1_reg, temp])?;
                        }
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Move dividend to RAX
                            emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                        }
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_ri(temp, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        if !x86_implicit {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Zero RDX
                            emitter.emit_zero_rdx();
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_div(temp, *width);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                        self.regalloc.free_temp(temp);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "DivU with shifted operand".to_string(),
                        });
                    }
                }

                // Move results to destination registers
                let quot_reg = self.get_dst_reg(*quot)?;
                if quot_reg != PhysReg::Rax {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(quot_reg, PhysReg::Rax, *width);
                }

                if let Some(r) = rem {
                    let rem_reg = self.get_dst_reg(*r)?;
                    if rem_reg != PhysReg::Rdx {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(rem_reg, PhysReg::Rdx, *width);
                    }
                }
            }

            OpKind::DivS {
                quot,
                rem,
                src1,
                src2,
                width,
                flags,
            } => {
                let preserve_flags = !flags.updates_any();
                let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
                let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
                let x86_implicit = *src1 == rax
                    && *quot == rax
                    && *rem
                        == if *width == OpWidth::W8 {
                            None
                        } else {
                            Some(rdx)
                        };

                // Signed divide: RDX:RAX / src2 -> RAX (quot), RDX (rem)
                // Generic non-x86 lowering sign-extends into RDX. Lifted x86
                // implicit IDIV already carries the high half in RDX.
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("DivS", &[src1_reg, src2_reg])?;
                        }
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Move dividend to RAX
                            emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        if !x86_implicit {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Sign-extend RAX into RDX:RAX
                            match width {
                                OpWidth::W64 => emitter.emit_cqo(),
                                OpWidth::W32 => emitter.emit_cdq(),
                                _ => {
                                    // For 16-bit: CWD, for 8-bit: CBW
                                    // We'll use the 32-bit form for smaller widths
                                    emitter.emit_cdq();
                                }
                            }
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_idiv(src2_reg, *width);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        // IDIV doesn't support immediate, need to load into temp
                        let temp = self.regalloc.get_scratch()?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("DivS", &[src1_reg, temp])?;
                        }
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Move dividend to RAX
                            emitter.emit_mov_rr(PhysReg::Rax, src1_reg, *width);
                        }
                        {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_ri(temp, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9C); // pushfq
                        }
                        if !x86_implicit {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            // Sign-extend RAX into RDX:RAX
                            match width {
                                OpWidth::W64 => emitter.emit_cqo(),
                                OpWidth::W32 => emitter.emit_cdq(),
                                _ => {
                                    // For 16-bit: CWD, for 8-bit: CBW
                                    // We'll use the 32-bit form for smaller widths
                                    emitter.emit_cdq();
                                }
                            }
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_idiv(temp, *width);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                        self.regalloc.free_temp(temp);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "DivS with shifted operand".to_string(),
                        });
                    }
                }

                // Move results to destination registers
                let quot_reg = self.get_dst_reg(*quot)?;
                if quot_reg != PhysReg::Rax {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(quot_reg, PhysReg::Rax, *width);
                }

                if let Some(r) = rem {
                    let rem_reg = self.get_dst_reg(*r)?;
                    if rem_reg != PhysReg::Rdx {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(rem_reg, PhysReg::Rdx, *width);
                    }
                }
            }

            _ => return self.lower_op_bitwise(op),
        }

        Ok(())
    }
}
