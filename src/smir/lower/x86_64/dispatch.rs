//! Top-level per-op lowering dispatch (lower_op)

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

    /// Lower a single operation
    pub(crate) fn lower_op(&mut self, op: &crate::smir::ir::ops::SmirOp) -> Result<(), LowerError> {
        if self.lower_mmx_rr(op)? {
            return Ok(());
        }
        // AVX10 owns the EVEX-native vector operations that do not have a
        // legacy scalar lowering below. Keep this dispatch in the production
        // path so the dedicated encoder is exercised by normal JIT lowering,
        // not only by its module-local unit tests.
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
            // Data Movement
            // ================================================================
            OpKind::Mov { dst, src, width } => {
                if Self::mov_touches_state_backed_gpr(&op.kind) {
                    return self.lower_state_backed_gpr_mov(*dst, src, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let use_modrm_imm = matches!(op.x86_hint, Some(X86OpHint::MovImmModRm));
                match src {
                    SrcOperand::Reg(r) => {
                        let src_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_reg, src_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if use_modrm_imm {
                            emitter.emit_mov_rm_imm(dst_reg, *val, *width);
                        } else {
                            emitter.emit_mov_ri(dst_reg, *val, *width);
                        }
                    }
                    SrcOperand::Imm64(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if *width == OpWidth::W64 {
                            emitter.emit_mov_ri_imm64(dst_reg, *val);
                        } else {
                            emitter.emit_mov_ri(dst_reg, *val, *width);
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Mov with shifted/extended operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Lea { dst, addr } => {
                let dst_reg = self.get_dst_reg(*dst)?;

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        // LEA dst, [base] is just a MOV
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rr(dst_reg, base_reg, OpWidth::W64);
                    }
                    Address::BaseOffset {
                        base,
                        offset,
                        disp_size,
                    } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_lea_disp(dst_reg, base_reg, *offset as i32, *disp_size);
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                        disp_size,
                    } => {
                        let index_reg = self.get_reg(*index)?;
                        let base_phys = match base {
                            Some(b) => Some(self.get_reg(*b)?),
                            None => None,
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_lea_sib_disp(
                            dst_reg, base_phys, index_reg, *scale, *disp, *disp_size,
                        );
                    }
                    Address::PcRel { offset, base, .. } => {
                        if self.guest_pcrel_lea_immediates {
                            if let Some(base_pc) = base {
                                let target = base_pc.wrapping_add_signed(*offset);
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_ri(dst_reg, target as i64, OpWidth::W64);
                                return Ok(());
                            }
                        }

                        let disp_offset = {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_lea_pcrel(dst_reg, 0)
                        };
                        let insn_end = self.code.position();

                        let disp = if let Some(base_pc) = base {
                            let target = base_pc.wrapping_add_signed(*offset);
                            let disp = if self.pcrel_adjust {
                                let next_rip = self.guest_base as i64 + insn_end as i64;
                                target as i64 - next_rip
                            } else {
                                *offset
                            };
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Lea".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            self.relocations.push(Relocation {
                                offset: disp_offset,
                                kind: RelocKind::PcRel32,
                                target: RelocTarget::GuestAddr(target),
                            });
                            disp
                        } else {
                            let disp = *offset;
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Lea".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            disp
                        };

                        self.code.patch_i32(disp_offset, disp as i32);
                    }
                    Address::Absolute(addr) => {
                        // LEA with absolute address - just MOV the constant
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_ri(dst_reg, *addr as i64, OpWidth::W64);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Lea with {:?} address", addr),
                        });
                    }
                }
            }

            OpKind::Xchg { reg1, reg2, width } => {
                if x86_state_backed_gpr_xchg_candidate(op) {
                    if !x86_state_backed_gpr_xchg_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Xchg".to_string(),
                            operand: format!("invalid x86 GPR exchange {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_xchg(*reg1, *reg2, *width);
                }
                if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::InvalidOperand {
                        op: "Xchg".to_string(),
                        operand: format!("unsupported width {width:?}"),
                    });
                }
                let reg1 = self.get_dst_reg(*reg1)?;
                let reg2 = self.get_dst_reg(*reg2)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_xchg(reg1, reg2, *width);
            }

            OpKind::CMove {
                dst,
                src,
                cond,
                width,
            } => {
                if x86_state_backed_gpr_cmove_candidate(op) {
                    if !x86_state_backed_gpr_cmove_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed CMOVcc".to_string(),
                            operand: format!("invalid x86 GPR conditional move {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_cmove(*dst, *src, *cond, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let x86_cond = X86Cond::from_condition(*cond);

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_cmovcc(x86_cond, dst_reg, src_reg, *width);
            }

            OpKind::Select {
                dst,
                cond,
                src_true,
                src_false,
                width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let cond_reg = self.get_reg(*cond)?;
                let true_reg = self.get_reg(*src_true)?;
                let false_reg = self.get_reg(*src_false)?;
                Self::ensure_flag_stack_operands_safe(
                    "Select",
                    &[dst_reg, cond_reg, true_reg, false_reg],
                )?;

                self.code.emit_u8(0x9C); // pushfq
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_test_rr(cond_reg, cond_reg, OpWidth::W64);
                    if dst_reg == true_reg {
                        emitter.emit_cmovcc(X86Cond::E, dst_reg, false_reg, *width);
                    } else if dst_reg == false_reg {
                        emitter.emit_cmovcc(X86Cond::Ne, dst_reg, true_reg, *width);
                    } else {
                        emitter.emit_mov_rr(dst_reg, false_reg, *width);
                        emitter.emit_cmovcc(X86Cond::Ne, dst_reg, true_reg, *width);
                    }
                }
                self.code.emit_u8(0x9D); // popfq
            }

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
                // For two-operand IMUL (dst = src1 * src2), we use the efficient form
                // For widening multiply (dst_hi:dst_lo = src1 * src2), we use IMUL with RAX
                if dst_hi.is_some() {
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

            // ================================================================
            // Bitwise Operations
            // ================================================================
            OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "And",
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
                        emitter.emit_alu_rr_dir(0x20, dst_reg, operand, *width, encoding);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("And", &[dst_reg, src1_reg])?;
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
                            emitter.emit_alu_acc_imm(0x24, *val, *width);
                        } else {
                            emitter.emit_and_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "And with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::AndNot {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_and_not_candidate(op) {
                    if !x86_state_backed_gpr_and_not_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed AndNot".to_string(),
                            operand: format!("invalid x86 GPR ANDN {width:?} {flags:?}"),
                        });
                    }
                    let SrcOperand::Reg(src2) = src2 else {
                        unreachable!();
                    };
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_and_not(
                        *dst,
                        *src1,
                        *src2,
                        *width,
                        defined_rflags_mask,
                    );
                }
                self.lower_and_not(*dst, *src1, src2, *width, *flags)?;
            }

            OpKind::Or {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "Or",
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
                        emitter.emit_alu_rr_dir(0x08, dst_reg, operand, *width, encoding);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("Or", &[dst_reg, src1_reg])?;
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
                            emitter.emit_alu_acc_imm(0x0C, *val, *width);
                        } else {
                            emitter.emit_or_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Or with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Xor {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let preserve_flags = !flags.updates_any();

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe(
                                "Xor",
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
                        emitter.emit_alu_rr_dir(0x30, dst_reg, operand, *width, encoding);
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    SrcOperand::Imm(val) => {
                        if preserve_flags {
                            Self::ensure_flag_stack_operands_safe("Xor", &[dst_reg, src1_reg])?;
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
                            emitter.emit_alu_acc_imm(0x34, *val, *width);
                        } else {
                            emitter.emit_xor_ri(dst_reg, *val, *width);
                        }
                        if preserve_flags {
                            self.code.emit_u8(0x9D); // popfq
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Xor with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Not { dst, src, width } => {
                if x86_state_backed_gpr_not_candidate(op) {
                    if !x86_state_backed_gpr_not_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Not".to_string(),
                            operand: format!("invalid x86 GPR complement {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_not(*dst, *src, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_not(dst_reg, *width);
            }

            OpKind::Bswap { dst, src, width } => {
                if x86_state_backed_gpr_bswap_candidate(op) {
                    if !x86_state_backed_gpr_bswap_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bswap".to_string(),
                            operand: format!("invalid x86 GPR byte swap {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bswap(*dst, *src, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if dst_reg != src_reg {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rr(dst_reg, src_reg, *width);
                }

                match width {
                    OpWidth::W16 => {
                        self.code.emit_u8(0x9C); // pushfq
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_rol_ri(dst_reg, 8, *width);
                        self.code.emit_u8(0x9D); // popfq
                    }
                    OpWidth::W32 | OpWidth::W64 => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_bswap(dst_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "Bswap".to_string(),
                            operand: format!("unsupported width {width:?}"),
                        });
                    }
                }
            }

            OpKind::Bt { src, index, width } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bt".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Test,
                        None,
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Test, None, *src, index, *width)?;
            }

            OpKind::Bts {
                dst,
                src,
                index,
                width,
            } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bts".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Set,
                        Some(*dst),
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Set, Some(*dst), *src, index, *width)?;
            }

            OpKind::Btr {
                dst,
                src,
                index,
                width,
            } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Btr".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Reset,
                        Some(*dst),
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Reset, Some(*dst), *src, index, *width)?;
            }

            OpKind::Btc {
                dst,
                src,
                index,
                width,
            } => {
                if x86_state_backed_gpr_bit_test_candidate(op) {
                    if !x86_state_backed_gpr_bit_test_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Btc".to_string(),
                            operand: format!("invalid x86 GPR bit test {width:?} {index:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_test(
                        BitTestRegOp::Complement,
                        Some(*dst),
                        *src,
                        index,
                        *width,
                    );
                }
                self.lower_bit_test(BitTestRegOp::Complement, Some(*dst), *src, index, *width)?;
            }

            OpKind::Crc32C {
                dst,
                crc,
                data,
                data_width,
            } => {
                if x86_state_backed_gpr_crc32_candidate(op) {
                    if !x86_state_backed_gpr_crc32_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Crc32C".to_string(),
                            operand: format!("invalid x86 GPR CRC32C {data_width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_crc32c(*dst, *crc, *data, *data_width);
                }
                self.lower_crc32c(*dst, *crc, *data, *data_width)?;
            }

            OpKind::Bsf {
                dst,
                src,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bit_scan_candidate(op) {
                    if !x86_state_backed_gpr_bit_scan_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bsf".to_string(),
                            operand: format!("invalid x86 GPR bit scan {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_scan(*dst, *src, *width, *flags, false);
                }
                self.lower_bit_scan(*dst, *src, *width, *flags, false)?;
            }

            OpKind::Bsr {
                dst,
                src,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bit_scan_candidate(op) {
                    if !x86_state_backed_gpr_bit_scan_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bsr".to_string(),
                            operand: format!("invalid x86 GPR bit scan {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_bit_scan(*dst, *src, *width, *flags, true);
                }
                self.lower_bit_scan(*dst, *src, *width, *flags, true)?;
            }

            OpKind::Bextr {
                dst,
                src,
                control,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bextr_bzhi_candidate(op) {
                    if !x86_state_backed_gpr_bextr_bzhi_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bextr".to_string(),
                            operand: format!("invalid x86 GPR BEXTR {width:?} {flags:?}"),
                        });
                    }
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x841),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_bextr_bzhi(
                        *dst,
                        *src,
                        *control,
                        *width,
                        defined_rflags_mask,
                        false,
                    );
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Bextr width {width:?}"),
                    });
                }
                let defined_rflags_mask = match flags {
                    FlagUpdate::None => None,
                    FlagUpdate::Specific(set)
                        if *set == FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF) =>
                    {
                        Some(0x841)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "Bextr".to_string(),
                            operand: format!("unsupported flag update {flags:?}"),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let control_reg = self.get_reg(*control)?;
                Self::ensure_flag_stack_operands_safe("Bextr", &[dst_reg, src_reg, control_reg])?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_bmi_rr(0xF7, dst_reg, src_reg, control_reg, *width);
                self.finish_bmi_flags(dst_reg, defined_rflags_mask);
            }

            OpKind::Bzhi {
                dst,
                src,
                index,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_bextr_bzhi_candidate(op) {
                    if !x86_state_backed_gpr_bextr_bzhi_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Bzhi".to_string(),
                            operand: format!("invalid x86 GPR BZHI {width:?} {flags:?}"),
                        });
                    }
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_bextr_bzhi(
                        *dst,
                        *src,
                        *index,
                        *width,
                        defined_rflags_mask,
                        true,
                    );
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Bzhi width {width:?}"),
                    });
                }
                let defined_rflags_mask = match flags {
                    FlagUpdate::None => None,
                    FlagUpdate::Specific(set)
                        if *set
                            == FlagSet::CF
                                .union(FlagSet::ZF)
                                .union(FlagSet::SF)
                                .union(FlagSet::OF) =>
                    {
                        Some(0x8C1)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "Bzhi".to_string(),
                            operand: format!("unsupported flag update {flags:?}"),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let index_reg = self.get_reg(*index)?;
                Self::ensure_flag_stack_operands_safe("Bzhi", &[dst_reg, src_reg, index_reg])?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_bmi_rr(0xF5, dst_reg, src_reg, index_reg, *width);
                self.finish_bmi_flags(dst_reg, defined_rflags_mask);
            }

            OpKind::X86Bls {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                if x86_state_backed_gpr_bls_candidate(op) {
                    if !x86_state_backed_gpr_bls_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86Bls".to_string(),
                            operand: format!("invalid x86 GPR BLS {kind:?} {width:?} {flags:?}"),
                        });
                    }
                    let defined_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(0x8C1),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_bls(
                        *dst,
                        *src,
                        *width,
                        *kind,
                        defined_rflags_mask,
                    );
                }
                self.lower_x86_bls(*dst, *src, *width, *kind, *flags)?;
            }

            OpKind::X86Adx {
                dst,
                src1,
                src2,
                width,
                kind,
                flags,
            } => {
                if x86_state_backed_gpr_adx_candidate(op) {
                    if !x86_state_backed_gpr_adx_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86Adx".to_string(),
                            operand: format!("invalid x86 GPR ADX {kind:?} {width:?} {flags:?}"),
                        });
                    }
                    let output_rflags_mask = match flags {
                        FlagUpdate::None => None,
                        FlagUpdate::Specific(_) => Some(match kind {
                            X86AdxKind::Adcx => 1,
                            X86AdxKind::Adox => 1 << 11,
                        }),
                        _ => unreachable!(),
                    };
                    return self.lower_state_backed_gpr_adx(
                        *dst,
                        *src1,
                        *src2,
                        *width,
                        *kind,
                        output_rflags_mask,
                    );
                }
                self.lower_x86_adx(*dst, *src1, *src2, *width, *kind, *flags)?;
            }

            OpKind::Pdep {
                dst,
                src,
                mask,
                width,
            } => {
                if x86_state_backed_gpr_pdep_pext_candidate(op) {
                    if !x86_state_backed_gpr_pdep_pext_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Pdep".to_string(),
                            operand: format!("invalid x86 GPR PDEP {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_pdep_pext(*dst, *src, *mask, *width, false);
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Pdep width {width:?}"),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let mask_reg = self.get_reg(*mask)?;
                Self::ensure_flag_stack_operands_safe("Pdep", &[dst_reg, src_reg, mask_reg])?;

                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_bmi_rr_pp(
                    0xF5,
                    X86SsePrefix::Repne,
                    dst_reg,
                    mask_reg,
                    src_reg,
                    *width,
                );
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Pext {
                dst,
                src,
                mask,
                width,
            } => {
                if x86_state_backed_gpr_pdep_pext_candidate(op) {
                    if !x86_state_backed_gpr_pdep_pext_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Pext".to_string(),
                            operand: format!("invalid x86 GPR PEXT {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_pdep_pext(*dst, *src, *mask, *width, true);
                }
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("Pext width {width:?}"),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let mask_reg = self.get_reg(*mask)?;
                Self::ensure_flag_stack_operands_safe("Pext", &[dst_reg, src_reg, mask_reg])?;

                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_vex_bmi_rr_pp(
                    0xF5,
                    X86SsePrefix::Rep,
                    dst_reg,
                    mask_reg,
                    src_reg,
                    *width,
                );
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Clz { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                Self::ensure_count_native_stack_safe("Clz", dst_reg, src_reg)?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lzcnt(dst_reg, src_reg, *width);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Ctz { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                Self::ensure_count_native_stack_safe("Ctz", dst_reg, src_reg)?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_tzcnt(dst_reg, src_reg, *width);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::Popcnt { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                Self::ensure_count_native_stack_safe("Popcnt", dst_reg, src_reg)?;
                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_popcnt(dst_reg, src_reg, *width);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::X86Count {
                dst,
                src,
                width,
                kind,
                flags,
            } => {
                if x86_state_backed_gpr_count_candidate(op) {
                    if !x86_state_backed_gpr_count_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86Count".to_string(),
                            operand: format!("invalid x86 GPR count {kind:?} {width:?} {flags:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_count(*dst, *src, *width, *kind, *flags);
                }
                self.lower_x86_count(*dst, *src, *width, *kind, *flags)?;
            }

            // ================================================================
            // Shifts
            // ================================================================
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_shift_candidate(op) {
                    if !x86_state_backed_gpr_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shl".to_string(),
                            operand: format!(
                                "invalid x86 GPR shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_shift(
                        *dst,
                        *src,
                        amount,
                        *width,
                        *flags,
                        ShiftRegOp::Shl,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Shl, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_shift_candidate(op) {
                    if !x86_state_backed_gpr_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shr".to_string(),
                            operand: format!(
                                "invalid x86 GPR shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_shift(
                        *dst,
                        *src,
                        amount,
                        *width,
                        *flags,
                        ShiftRegOp::Shr,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Shr, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_shift_candidate(op) {
                    if !x86_state_backed_gpr_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Sar".to_string(),
                            operand: format!(
                                "invalid x86 GPR shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_shift(
                        *dst,
                        *src,
                        amount,
                        *width,
                        *flags,
                        ShiftRegOp::Sar,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Sar, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Rol {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_rotate_candidate(op) {
                    if !x86_state_backed_gpr_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Rol".to_string(),
                            operand: format!(
                                "invalid x86 GPR rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self
                        .lower_state_backed_gpr_rotate(*dst, *src, amount, *width, *flags, false);
                }
                self.lower_shift_reg_op(ShiftRegOp::Rol, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Ror {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_rotate_candidate(op) {
                    if !x86_state_backed_gpr_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Ror".to_string(),
                            operand: format!(
                                "invalid x86 GPR rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self
                        .lower_state_backed_gpr_rotate(*dst, *src, amount, *width, *flags, true);
                }
                self.lower_shift_reg_op(ShiftRegOp::Ror, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_carry_rotate_candidate(op) {
                    if !x86_state_backed_gpr_carry_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Rcl".to_string(),
                            operand: format!(
                                "invalid x86 GPR carry rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_carry_rotate(
                        *dst, *src, amount, *width, *flags, false,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Rcl, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_carry_rotate_candidate(op) {
                    if !x86_state_backed_gpr_carry_rotate_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Rcr".to_string(),
                            operand: format!(
                                "invalid x86 GPR carry rotate {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_carry_rotate(
                        *dst, *src, amount, *width, *flags, true,
                    );
                }
                self.lower_shift_reg_op(ShiftRegOp::Rcr, *dst, *src, amount, *width, *flags)?;
            }

            OpKind::Shld {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_double_shift_candidate(op) {
                    if !x86_state_backed_gpr_double_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shld".to_string(),
                            operand: format!(
                                "invalid x86 GPR double shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_double_shift(
                        *dst, *dst, *src, amount, *width, *flags, true,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                match amount {
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shld_rr_imm(dst_reg, src_reg, *val as u8, *width);
                    }
                    SrcOperand::Reg(r) => {
                        let amt_reg = self.get_reg(*r)?;
                        if amt_reg != PhysReg::Rcx {
                            return Err(LowerError::InvalidOperand {
                                op: "Shld".to_string(),
                                operand: "requires CL".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shld_rr_cl(dst_reg, src_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Shld with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Shrd {
                dst,
                src,
                amount,
                width,
                flags,
            } => {
                if x86_state_backed_gpr_double_shift_candidate(op) {
                    if !x86_state_backed_gpr_double_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed Shrd".to_string(),
                            operand: format!(
                                "invalid x86 GPR double shift {dst:?} {src:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_double_shift(
                        *dst, *dst, *src, amount, *width, *flags, false,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                match amount {
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shrd_rr_imm(dst_reg, src_reg, *val as u8, *width);
                    }
                    SrcOperand::Reg(r) => {
                        let amt_reg = self.get_reg(*r)?;
                        if amt_reg != PhysReg::Rcx {
                            return Err(LowerError::InvalidOperand {
                                op: "Shrd".to_string(),
                                operand: "requires CL".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_shrd_rr_cl(dst_reg, src_reg, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Shrd with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::X86NddDoubleShift {
                dst,
                base,
                fill,
                amount,
                width,
                left,
                flags,
            } => {
                if x86_state_backed_gpr_double_shift_candidate(op) {
                    if !x86_state_backed_gpr_double_shift_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed X86NddDoubleShift".to_string(),
                            operand: format!(
                                "invalid x86 GPR NDD double shift {dst:?} {base:?} {fill:?} {amount:?} {width:?} {flags:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_double_shift(
                        *dst, *base, *fill, amount, *width, *flags, *left,
                    );
                }
                self.lower_x86_ndd_double_shift(*dst, *base, *fill, amount, *width, *left, *flags)?
            }

            // ================================================================
            // Comparisons
            // ================================================================
            OpKind::Cmp { src1, src2, width } => {
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        let encoding = alu_hint.unwrap_or(X86AluEncoding::RmReg);
                        emitter.emit_alu_rr_dir(0x38, src1_reg, src2_reg, *width, encoding);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if matches!(alu_hint, Some(X86AluEncoding::AccImm))
                            && src1_reg == PhysReg::Rax
                        {
                            emitter.emit_alu_acc_imm(0x3C, *val, *width);
                        } else {
                            emitter.emit_cmp_ri(src1_reg, *val, *width);
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Cmp with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::Test { src1, src2, width } => {
                let src1_reg = self.get_reg(*src1)?;

                match src2 {
                    SrcOperand::Reg(r) => {
                        let src2_reg = self.get_reg(*r)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_test_rr(src1_reg, src2_reg, *width);
                    }
                    SrcOperand::Imm(val) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_test_ri(src1_reg, *val, *width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "Test with shifted operand".to_string(),
                        });
                    }
                }
            }

            OpKind::SetCC { dst, cond, width } => {
                if x86_state_backed_gpr_setcc_candidate(op) {
                    if !x86_state_backed_gpr_setcc_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed SETcc".to_string(),
                            operand: format!("invalid x86 GPR conditional set {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_setcc(*dst, *cond, *width);
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let x86_cond = X86Cond::from_condition(*cond);

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_setcc(x86_cond, dst_reg);

                // Zero-extend to full width if needed
                if *width != OpWidth::W8 {
                    emitter.emit_movzx(dst_reg, dst_reg, OpWidth::W8, *width);
                }
            }

            OpKind::ReadFlags { dst } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                Self::ensure_flag_stack_operands_safe("ReadFlags", &[dst_reg])?;

                self.code.emit_u8(0x9C); // pushfq
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_pop(dst_reg);
            }

            OpKind::WriteFlags { src } => {
                let src_reg = self.get_reg(*src)?;
                Self::ensure_flag_stack_operands_safe("WriteFlags", &[src_reg])?;

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(src_reg);
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling,
            } => {
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FpCompare".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let pp = match elem {
                    VecElementType::F32 => X86SsePrefix::None,
                    VecElementType::F64 => X86SsePrefix::OpSize,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86FpCompare {elem:?}"),
                        });
                    }
                };
                let opcode = if *signaling { 0x2F } else { 0x2E };
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    self.emit_vec_rr(
                        VecEncoding {
                            width: VecWidth::V128,
                            opcode,
                            ..enc_hint
                        },
                        src1_reg,
                        src2_reg,
                        0,
                    );
                } else if src1_reg.vec_ext2() != 0 || src2_reg.vec_ext2() != 0 {
                    self.emit_vec_rr(
                        VecEncoding {
                            kind: VecEncodingKind::Evex,
                            map: X86VecMap::Map0F,
                            pp,
                            opcode,
                            width: VecWidth::V128,
                            w: *elem == VecElementType::F64,
                        },
                        src1_reg,
                        src2_reg,
                        0,
                    );
                } else {
                    let prefix = if pp == X86SsePrefix::OpSize {
                        Some(0x66)
                    } else {
                        None
                    };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, src1_reg, src2_reg);
                }
            }

            OpKind::X86GetExponent {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetExponent".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (map, w) = match elem {
                    VecElementType::F16 => (X86VecMap::Map6, false),
                    VecElementType::F32 => (X86VecMap::Map0F38, false),
                    VecElementType::F64 => (X86VecMap::Map0F38, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetExponent".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = if *scalar { 0x43 } else { 0x42 };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: hint_map,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_map == map
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86GetExponent".to_string(),
                        operand: "non-canonical VGETEXP shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    map,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    None,
                );
            }

            OpKind::X86GetMantissa {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetMantissa".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (pp, w) = match elem {
                    VecElementType::F16 => (X86SsePrefix::None, false),
                    VecElementType::F32 => (X86SsePrefix::OpSize, false),
                    VecElementType::F64 => (X86SsePrefix::OpSize, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86GetMantissa".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = if *scalar { 0x27 } else { 0x26 };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_pp == pp
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86GetMantissa".to_string(),
                        operand: "non-canonical VGETMANT shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F3A,
                    pp,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    Some(*imm),
                );
            }

            OpKind::X86RoundScale {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86RoundScale".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (pp, w, opcode) = match (elem, scalar) {
                    (VecElementType::F16, false) => (X86SsePrefix::None, false, 0x08),
                    (VecElementType::F16, true) => (X86SsePrefix::None, false, 0x0A),
                    (VecElementType::F32, false) => (X86SsePrefix::OpSize, false, 0x08),
                    (VecElementType::F32, true) => (X86SsePrefix::OpSize, false, 0x0A),
                    (VecElementType::F64, false) => (X86SsePrefix::OpSize, true, 0x09),
                    (VecElementType::F64, true) => (X86SsePrefix::OpSize, true, 0x0B),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86RoundScale".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_pp == pp
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86RoundScale".to_string(),
                        operand: "non-canonical VRNDSCALE shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F3A,
                    pp,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    Some(*imm),
                );
            }

            OpKind::X86Reduce {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Reduce".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (pp, w, opcode) = match (elem, scalar) {
                    (VecElementType::F16, false) => (X86SsePrefix::None, false, 0x56),
                    (VecElementType::F16, true) => (X86SsePrefix::None, false, 0x57),
                    (VecElementType::F32, false) => (X86SsePrefix::OpSize, false, 0x56),
                    (VecElementType::F32, true) => (X86SsePrefix::OpSize, false, 0x57),
                    (VecElementType::F64, false) => (X86SsePrefix::OpSize, true, 0x56),
                    (VecElementType::F64, true) => (X86SsePrefix::OpSize, true, 0x57),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Reduce".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = register_matches_width(dst_reg, *width)
                    && register_matches_width(src_reg, *width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128
                            && *lanes == 1
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && merge_reg.is_none()
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_pp == pp
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Reduce".to_string(),
                        operand: "non-canonical VREDUCE shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F3A,
                    pp,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    Some(*imm),
                );
            }

            OpKind::X86Range {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Range".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Range".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && *imm <= 0x0F
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128 && *lanes == 1
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let opcode = if *scalar { 0x51 } else { 0x50 };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Range".to_string(),
                        operand: "non-canonical VRANGE shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr_imm_sae(
                    X86VecMap::Map0F3A,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    *imm,
                );
            }

            OpKind::X86FixupImm {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86FixupImm".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86FixupImm".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        *width == VecWidth::V128 && *lanes == 1
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let opcode = if *scalar { 0x55 } else { 0x54 };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F3A,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FixupImm".to_string(),
                        operand: "non-canonical VFIXUPIMM shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr_imm_sae(
                    X86VecMap::Map0F3A,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    *imm,
                );
            }

            OpKind::X86Exp2 {
                dst,
                src,
                mask,
                elem,
                width,
                lanes,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Exp2".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86Exp2".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let valid_shape = dst_reg.is_zmm()
                    && src_reg.is_zmm()
                    && *width == VecWidth::V512
                    && *lanes == width.lanes(*elem) as u8
                    && (!*mask_zeroing || aaa != 0);
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0xC8,
                        width: VecWidth::V512,
                        w: hint_w,
                    }) if hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Exp2".to_string(),
                        operand: "non-canonical VEXP2 shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F38,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    0xC8,
                    dst_reg,
                    None,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    None,
                );
            }

            OpKind::X86Recip14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86Rsqrt14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86Rsqrt14 { .. });
                let op_name = if rsqrt { "X86Rsqrt14" } else { "X86Recip14" };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = match (rsqrt, *scalar) {
                    (false, false) => 0x4C,
                    (false, true) => 0x4D,
                    (true, false) => 0x4E,
                    (true, true) => 0x4F,
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        dst_reg.is_xmm()
                            && src_reg.is_xmm()
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                            && *width == VecWidth::V128
                            && *lanes == 1
                    } else {
                        register_matches_width(dst_reg, *width)
                            && register_matches_width(src_reg, *width)
                            && merge_reg.is_none()
                            && *lanes == width.lanes(*elem) as u8
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: op_name.to_string(),
                        operand: format!(
                            "non-canonical {} shape or encoding metadata",
                            if rsqrt { "VRSQRT14" } else { "VRCP14" }
                        ),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F38,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    false,
                    None,
                );
            }

            OpKind::X86RecipFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86RsqrtFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86RsqrtFp16 { .. });
                let op_name = if rsqrt {
                    "X86RsqrtFp16"
                } else {
                    "X86RecipFp16"
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let opcode = match (rsqrt, *scalar) {
                    (false, false) => 0x4C,
                    (false, true) => 0x4D,
                    (true, false) => 0x4E,
                    (true, true) => 0x4F,
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let valid_shape = (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        dst_reg.is_xmm()
                            && src_reg.is_xmm()
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                            && *width == VecWidth::V128
                            && *lanes == 1
                    } else {
                        register_matches_width(dst_reg, *width)
                            && register_matches_width(src_reg, *width)
                            && merge_reg.is_none()
                            && *lanes == width.lanes(VecElementType::F16) as u8
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map6,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: false,
                    }) if hint_opcode == opcode && hint_width == *width
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: op_name.to_string(),
                        operand: format!(
                            "non-canonical {} shape or encoding metadata",
                            if rsqrt { "VRSQRTFP16" } else { "VRCPFP16" }
                        ),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map6,
                    X86SsePrefix::OpSize,
                    *width,
                    false,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    false,
                    None,
                );
            }

            OpKind::X86Recip28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            }
            | OpKind::X86Rsqrt28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86Rsqrt28 { .. });
                let op_name = if rsqrt { "X86Rsqrt28" } else { "X86Recip28" };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let merge_reg = merge.map(|reg| self.get_reg(reg)).transpose()?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let w = match elem {
                    VecElementType::F32 => false,
                    VecElementType::F64 => true,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: op_name.to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let opcode = match (rsqrt, *scalar) {
                    (false, false) => 0xCA,
                    (false, true) => 0xCB,
                    (true, false) => 0xCC,
                    (true, true) => 0xCD,
                };
                let valid_shape = (!*mask_zeroing || aaa != 0)
                    && if *scalar {
                        dst_reg.is_xmm()
                            && src_reg.is_xmm()
                            && merge_reg.is_some_and(|reg| reg.is_xmm())
                            && *width == VecWidth::V128
                            && *lanes == 1
                    } else {
                        dst_reg.is_zmm()
                            && src_reg.is_zmm()
                            && merge_reg.is_none()
                            && *width == VecWidth::V512
                            && *lanes == width.lanes(*elem) as u8
                    };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_opcode == opcode && hint_width == *width && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: op_name.to_string(),
                        operand: format!(
                            "non-canonical {} shape or encoding metadata",
                            if rsqrt { "VRSQRT28" } else { "VRCP28" }
                        ),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_unary_fp_rr(
                    X86VecMap::Map0F38,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    merge_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    None,
                );
            }

            OpKind::X86ScaleF {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86ScaleF".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let (map, w) = match elem {
                    VecElementType::F16 => (X86VecMap::Map6, false),
                    VecElementType::F32 => (X86VecMap::Map0F38, false),
                    VecElementType::F64 => (X86VecMap::Map0F38, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86ScaleF".to_string(),
                            operand: format!("unsupported element {elem:?}"),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let valid_round = matches!(
                    round,
                    FpRoundMode::Dynamic
                        | FpRoundMode::RoundNearest
                        | FpRoundMode::RoundDown
                        | FpRoundMode::RoundUp
                        | FpRoundMode::RoundTowardZero
                ) && (*suppress_exceptions == (*round != FpRoundMode::Dynamic));
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && (!*mask_zeroing || aaa != 0)
                    && valid_round
                    && if *scalar {
                        *width == VecWidth::V128 && *lanes == 1
                    } else {
                        *lanes == width.lanes(*elem) as u8
                            && (!*suppress_exceptions || *width == VecWidth::V512)
                    };
                let opcode = if *scalar { 0x2D } else { 0x2C };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: hint_map,
                        pp: X86SsePrefix::OpSize,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: hint_w,
                    }) if hint_map == map
                        && hint_opcode == opcode
                        && hint_width == *width
                        && hint_w == w
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86ScaleF".to_string(),
                        operand: "non-canonical VSCALEF shape or encoding metadata".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr(
                    map,
                    X86SsePrefix::OpSize,
                    *width,
                    w,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *round,
                    *suppress_exceptions,
                );
            }

            OpKind::X86FP16Complex {
                dst,
                src1,
                src2,
                mask,
                width,
                pairs,
                scalar,
                mask_zeroing,
                accumulate,
                conjugate,
                round,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86FP16Complex".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let register_matches_width = |reg: PhysReg, expected: VecWidth| {
                    matches!(
                        (reg, expected),
                        (PhysReg::Xmm(_), VecWidth::V128)
                            | (PhysReg::Ymm(_), VecWidth::V256)
                            | (PhysReg::Zmm(_), VecWidth::V512)
                    )
                };
                let register_width = if *scalar { VecWidth::V128 } else { *width };
                let embedded_rounding = *round != FpRoundMode::Dynamic;
                let valid_round = matches!(
                    round,
                    FpRoundMode::Dynamic
                        | FpRoundMode::RoundNearest
                        | FpRoundMode::RoundDown
                        | FpRoundMode::RoundUp
                        | FpRoundMode::RoundTowardZero
                );
                let valid_shape = register_matches_width(dst_reg, register_width)
                    && register_matches_width(src1_reg, register_width)
                    && register_matches_width(src2_reg, register_width)
                    && dst_reg != src1_reg
                    && dst_reg != src2_reg
                    && (!*mask_zeroing || aaa != 0)
                    && valid_round
                    && if *scalar {
                        *width == VecWidth::V128 && *pairs == 1
                    } else {
                        *pairs == (width.bytes() / 4) as u8
                            && (!embedded_rounding || *width == VecWidth::V512)
                    };
                let pp = if *conjugate {
                    X86SsePrefix::Repne
                } else {
                    X86SsePrefix::Rep
                };
                let opcode = match (*accumulate, *scalar) {
                    (true, false) => 0x56,
                    (true, true) => 0x57,
                    (false, false) => 0xD6,
                    (false, true) => 0xD7,
                };
                let valid_hint = matches!(
                    op.x86_hint,
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map6,
                        pp: hint_pp,
                        opcode: hint_opcode,
                        width: hint_width,
                        w: false,
                    }) if hint_pp == pp && hint_opcode == opcode && hint_width == *width
                );
                if !valid_shape || !valid_hint {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FP16Complex".to_string(),
                        operand: "non-canonical AVX512-FP16 complex shape or encoding metadata"
                            .to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_fp_rrr(
                    X86VecMap::Map6,
                    pp,
                    *width,
                    false,
                    opcode,
                    dst_reg,
                    src1_reg,
                    src2_reg,
                    aaa,
                    *mask_zeroing,
                    *round,
                    embedded_rounding,
                );
            }

            OpKind::X86FpToInt {
                dst,
                src,
                elem,
                int_width,
                signed,
                truncate,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if dst_reg.is_vec() || !src_reg.is_vec() || src_reg.vec_ext2() != 0 {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FpToInt".to_string(),
                        operand: "requires a GPR destination and XMM0-XMM15 source".to_string(),
                    });
                }
                if !matches!(int_width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("X86FpToInt width {int_width:?}"),
                    });
                }
                if !*signed
                    || *suppress_exceptions
                    || (*truncate && *round != FpRoundMode::RoundTowardZero)
                    || (!*truncate && *round != FpRoundMode::Dynamic)
                {
                    return Err(LowerError::UnsupportedOp {
                        op: format!(
                            "X86FpToInt signed={signed}, rounding {round:?}, truncate={truncate}, sae={suppress_exceptions}"
                        ),
                    });
                }
                let prefix = match elem {
                    VecElementType::F32 => 0xF3,
                    VecElementType::F64 => 0xF2,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86FpToInt element {elem:?}"),
                        });
                    }
                };
                let opcode = if *truncate { 0x2C } else { 0x2D };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_fp_to_int_rr(prefix, opcode, dst_reg, src_reg, *int_width);
            }

            OpKind::X86IntToFp {
                dst,
                merge,
                src,
                elem,
                int_width,
                signed,
                round,
                suppress_exceptions,
                zero_upper,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let merge_reg = self.get_reg(*merge)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec()
                    || merge_reg != dst_reg
                    || src_reg.is_vec()
                    || dst_reg.vec_ext2() != 0
                    || *zero_upper
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86IntToFp".to_string(),
                        operand: "native legacy lowering requires dst=merge XMM0-XMM15".to_string(),
                    });
                }
                if !matches!(int_width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("X86IntToFp width {int_width:?}"),
                    });
                }
                if !*signed || *round != FpRoundMode::Dynamic || *suppress_exceptions {
                    return Err(LowerError::UnsupportedOp {
                        op: format!(
                            "X86IntToFp signed={signed}, rounding {round:?}, sae={suppress_exceptions}"
                        ),
                    });
                }
                let prefix = match elem {
                    VecElementType::F32 => 0xF3,
                    VecElementType::F64 => 0xF2,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86IntToFp element {elem:?}"),
                        });
                    }
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_fp_to_int_rr(prefix, 0x2A, dst_reg, src_reg, *int_width);
            }

            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask,
                from,
                to,
                mask_zeroing,
                round,
                suppress_exceptions,
                zero_upper,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let merge_reg = self.get_reg(*merge)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec()
                    || !src_reg.is_vec()
                    || merge_reg != dst_reg
                    || dst_reg.vec_ext2() != 0
                    || src_reg.vec_ext2() != 0
                    || mask.is_some()
                    || *mask_zeroing
                    || *round != FpRoundMode::Dynamic
                    || *suppress_exceptions
                    || *zero_upper
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86FpConvert".to_string(),
                        operand: "native legacy lowering requires dst=merge XMM0-XMM15".to_string(),
                    });
                }
                let prefix = match (*from, *to) {
                    (VecElementType::F32, VecElementType::F64) => Some(0xF3),
                    (VecElementType::F64, VecElementType::F32) => Some(0xF2),
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86FpConvert {from:?}->{to:?}"),
                        });
                    }
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_mov_rr(prefix, 0x5A, dst_reg, src_reg);
            }

            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from,
                to,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                report_fp16_denormal,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpConvert".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                if *report_fp16_denormal {
                    return Err(LowerError::UnsupportedOp {
                        op: "X86PackedFpConvert FP16 denormal reporting".to_string(),
                    });
                }
                let pp = match (*from, *to) {
                    (VecElementType::F32, VecElementType::F64) => X86SsePrefix::None,
                    (VecElementType::F64, VecElementType::F32) => X86SsePrefix::OpSize,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("X86PackedFpConvert {from:?}->{to:?}"),
                        });
                    }
                };
                let instruction_width = match (*from, *lanes) {
                    (VecElementType::F64, 2) => VecWidth::V128,
                    (VecElementType::F64, 4) => VecWidth::V256,
                    (VecElementType::F64, 8) => VecWidth::V512,
                    (VecElementType::F32, 2 | 4 | 8) => *dst_width,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid packed conversion lane count".to_string(),
                        });
                    }
                };
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                if *mask_zeroing && aaa == 0 {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpConvert".to_string(),
                        operand: "zeroing requires a nonzero opmask".to_string(),
                    });
                }
                if let Some(X86OpHint::EvexOp { map, .. }) = op.x86_hint {
                    if !*zero_upper
                        || !matches!(*lanes, 2 | 4 | 8)
                        || *suppress_exceptions != (*round != FpRoundMode::Dynamic)
                        || (*round != FpRoundMode::Dynamic
                            && !(*from == VecElementType::F64
                                && *lanes == 8
                                && instruction_width == VecWidth::V512))
                    {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid EVEX packed conversion shape".to_string(),
                        });
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_evex_masked_rr(
                        map,
                        pp,
                        instruction_width,
                        *from == VecElementType::F64,
                        0x5A,
                        dst_reg,
                        src_reg,
                        aaa,
                        *mask_zeroing,
                        *round != FpRoundMode::Dynamic,
                        *round,
                    );
                } else if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    if !*zero_upper
                        || mask.is_some()
                        || *mask_zeroing
                        || *round != FpRoundMode::Dynamic
                        || *suppress_exceptions
                        || !matches!(*lanes, 2 | 4)
                        || !matches!(instruction_width, VecWidth::V128 | VecWidth::V256)
                    {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid VEX packed conversion shape".to_string(),
                        });
                    }
                    self.emit_vec_rr(
                        VecEncoding {
                            pp,
                            opcode: 0x5A,
                            width: instruction_width,
                            ..enc_hint
                        },
                        dst_reg,
                        src_reg,
                        0,
                    );
                } else {
                    if *zero_upper
                        || mask.is_some()
                        || *mask_zeroing
                        || *round != FpRoundMode::Dynamic
                        || *suppress_exceptions
                        || *lanes != 2
                        || *dst_width != VecWidth::V128
                        || dst_reg.vec_ext2() != 0
                        || src_reg.vec_ext2() != 0
                    {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpConvert".to_string(),
                            operand: "invalid legacy packed conversion shape".to_string(),
                        });
                    }
                    let prefix = if pp == X86SsePrefix::OpSize {
                        Some(0x66)
                    } else {
                        None
                    };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, 0x5A, dst_reg, src_reg);
                }
            }

            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem,
                fp_elem,
                signed,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode, w) = match (*int_elem, *fp_elem, *signed) {
                    (VecElementType::I32, VecElementType::F32, true) => {
                        (X86SsePrefix::None, 0x5B, false)
                    }
                    (VecElementType::I64, VecElementType::F32, true) => {
                        (X86SsePrefix::None, 0x5B, true)
                    }
                    (VecElementType::I32, VecElementType::F64, true) => {
                        (X86SsePrefix::Rep, 0xE6, false)
                    }
                    (VecElementType::I64, VecElementType::F64, true) => {
                        (X86SsePrefix::Rep, 0xE6, true)
                    }
                    (VecElementType::I32, VecElementType::F32, false) => {
                        (X86SsePrefix::Repne, 0x7A, false)
                    }
                    (VecElementType::I64, VecElementType::F32, false) => {
                        (X86SsePrefix::Repne, 0x7A, true)
                    }
                    (VecElementType::I32, VecElementType::F64, false) => {
                        (X86SsePrefix::Rep, 0x7A, false)
                    }
                    (VecElementType::I64, VecElementType::F64, false) => {
                        (X86SsePrefix::Rep, 0x7A, true)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp".to_string(),
                            operand: "elements must be I32/I64 to F32/F64".to_string(),
                        });
                    }
                };
                let operation_bytes = u32::from(*lanes) * int_elem.bytes().max(fp_elem.bytes());
                let operation_width = match operation_bytes {
                    16 => VecWidth::V128,
                    32 => VecWidth::V256,
                    64 => VecWidth::V512,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp".to_string(),
                            operand: "invalid packed conversion lane count".to_string(),
                        });
                    }
                };
                let exact_width = |bytes: u32| match bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let register_width = |bytes: u32| match bytes {
                    0..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let expected_src_width = exact_width(u32::from(*lanes) * int_elem.bytes());
                let expected_dst_width = register_width(u32::from(*lanes) * fp_elem.bytes());
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                if *src_width != expected_src_width
                    || *dst_width != expected_dst_width
                    || (*mask_zeroing && aaa == 0)
                    || *round == FpRoundMode::RoundNearestTiesAway
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp".to_string(),
                        operand: "invalid packed integer-to-FP shape".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::EvexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        let exact_no_er =
                            *int_elem == VecElementType::I32 && *fp_elem == VecElementType::F64;
                        if map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || hinted_w != w
                            || !*zero_upper
                            || *suppress_exceptions != (*round != FpRoundMode::Dynamic)
                            || (*suppress_exceptions
                                && (operation_width != VecWidth::V512 || exact_no_er))
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedIntToFp".to_string(),
                                operand: "invalid EVEX packed conversion metadata".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_evex_masked_rr(
                            map,
                            pp,
                            operation_width,
                            w,
                            opcode,
                            dst_reg,
                            src_reg,
                            aaa,
                            *mask_zeroing,
                            *suppress_exceptions,
                            *round,
                        );
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        let vex_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        if !vex_family
                            || map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || !matches!(operation_width, VecWidth::V128 | VecWidth::V256)
                            || !*zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != FpRoundMode::Dynamic
                            || *suppress_exceptions
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedIntToFp".to_string(),
                                operand: "invalid VEX packed conversion metadata".to_string(),
                            });
                        }
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width: operation_width,
                                w: hinted_w,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    None => {
                        let legacy_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        if !legacy_family
                            || *zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != FpRoundMode::Dynamic
                            || *suppress_exceptions
                            || operation_width != VecWidth::V128
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedIntToFp".to_string(),
                                operand: "invalid legacy packed conversion shape".to_string(),
                            });
                        }
                        let prefix = match pp {
                            X86SsePrefix::None => None,
                            X86SsePrefix::Rep => Some(0xF3),
                            _ => unreachable!(),
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src_reg);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "X86PackedIntToFp without canonical encoding metadata".to_string(),
                        });
                    }
                }
            }

            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                signed,
                truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpToInt".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode, w) = match (*fp_elem, *int_elem, *signed, *truncate) {
                    (VecElementType::F32, VecElementType::I32, true, false) => {
                        (X86SsePrefix::OpSize, 0x5B, false)
                    }
                    (VecElementType::F32, VecElementType::I32, true, true) => {
                        (X86SsePrefix::Rep, 0x5B, false)
                    }
                    (VecElementType::F64, VecElementType::I32, true, false) => {
                        (X86SsePrefix::Repne, 0xE6, true)
                    }
                    (VecElementType::F64, VecElementType::I32, true, true) => {
                        (X86SsePrefix::OpSize, 0xE6, true)
                    }
                    (VecElementType::F32, VecElementType::I64, true, false) => {
                        (X86SsePrefix::OpSize, 0x7B, false)
                    }
                    (VecElementType::F64, VecElementType::I64, true, false) => {
                        (X86SsePrefix::OpSize, 0x7B, true)
                    }
                    (VecElementType::F32, VecElementType::I64, true, true) => {
                        (X86SsePrefix::OpSize, 0x7A, false)
                    }
                    (VecElementType::F64, VecElementType::I64, true, true) => {
                        (X86SsePrefix::OpSize, 0x7A, true)
                    }
                    (VecElementType::F32, VecElementType::I32, false, false) => {
                        (X86SsePrefix::None, 0x79, false)
                    }
                    (VecElementType::F64, VecElementType::I32, false, false) => {
                        (X86SsePrefix::None, 0x79, true)
                    }
                    (VecElementType::F32, VecElementType::I32, false, true) => {
                        (X86SsePrefix::None, 0x78, false)
                    }
                    (VecElementType::F64, VecElementType::I32, false, true) => {
                        (X86SsePrefix::None, 0x78, true)
                    }
                    (VecElementType::F32, VecElementType::I64, false, false) => {
                        (X86SsePrefix::OpSize, 0x79, false)
                    }
                    (VecElementType::F64, VecElementType::I64, false, false) => {
                        (X86SsePrefix::OpSize, 0x79, true)
                    }
                    (VecElementType::F32, VecElementType::I64, false, true) => {
                        (X86SsePrefix::OpSize, 0x78, false)
                    }
                    (VecElementType::F64, VecElementType::I64, false, true) => {
                        (X86SsePrefix::OpSize, 0x78, true)
                    }
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpToInt".to_string(),
                            operand: "elements must be F32/F64 to I32/I64".to_string(),
                        });
                    }
                };
                let operation_bytes = u32::from(*lanes) * fp_elem.bytes().max(int_elem.bytes());
                let operation_width = match operation_bytes {
                    16 => VecWidth::V128,
                    32 => VecWidth::V256,
                    64 => VecWidth::V512,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpToInt".to_string(),
                            operand: "invalid packed conversion lane count".to_string(),
                        });
                    }
                };
                let exact_width = |bytes: u32| match bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let register_width = |bytes: u32| match bytes {
                    0..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let expected_src_width = exact_width(u32::from(*lanes) * fp_elem.bytes());
                let expected_dst_width = register_width(u32::from(*lanes) * int_elem.bytes());
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFpToInt".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let rounding_valid = if *truncate {
                    *round == FpRoundMode::RoundTowardZero
                } else {
                    *round != FpRoundMode::RoundNearestTiesAway
                        && *suppress_exceptions == (*round != FpRoundMode::Dynamic)
                };
                if *src_width != expected_src_width
                    || *dst_width != expected_dst_width
                    || (*mask_zeroing && aaa == 0)
                    || !rounding_valid
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFpToInt".to_string(),
                        operand: "invalid packed FP-to-integer shape".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::EvexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        if map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || hinted_w != w
                            || !*zero_upper
                            || (*suppress_exceptions && operation_width != VecWidth::V512)
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedFpToInt".to_string(),
                                operand: "invalid EVEX packed conversion metadata".to_string(),
                            });
                        }
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if *truncate && *suppress_exceptions {
                            emitter.emit_evex_masked_rr(
                                map,
                                pp,
                                VecWidth::V128,
                                w,
                                opcode,
                                dst_reg,
                                src_reg,
                                aaa,
                                *mask_zeroing,
                                true,
                                FpRoundMode::Dynamic,
                            );
                        } else {
                            emitter.emit_evex_masked_rr(
                                map,
                                pp,
                                operation_width,
                                w,
                                opcode,
                                dst_reg,
                                src_reg,
                                aaa,
                                *mask_zeroing,
                                *suppress_exceptions,
                                *round,
                            );
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp: hinted_pp,
                        opcode: hinted_opcode,
                        width: hinted_width,
                        w: hinted_w,
                    }) => {
                        let vex_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        let expected_round = if *truncate {
                            FpRoundMode::RoundTowardZero
                        } else {
                            FpRoundMode::Dynamic
                        };
                        if !vex_family
                            || map != X86VecMap::Map0F
                            || hinted_pp != pp
                            || hinted_opcode != opcode
                            || hinted_width != operation_width
                            || !matches!(operation_width, VecWidth::V128 | VecWidth::V256)
                            || !*zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != expected_round
                            || *suppress_exceptions
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedFpToInt".to_string(),
                                operand: "invalid VEX packed conversion metadata".to_string(),
                            });
                        }
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width: operation_width,
                                w: hinted_w,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    None => {
                        let legacy_family = *signed
                            && *int_elem == VecElementType::I32
                            && matches!(fp_elem, VecElementType::F32 | VecElementType::F64);
                        let expected_round = if *truncate {
                            FpRoundMode::RoundTowardZero
                        } else {
                            FpRoundMode::Dynamic
                        };
                        if !legacy_family
                            || *zero_upper
                            || mask.is_some()
                            || *mask_zeroing
                            || *round != expected_round
                            || *suppress_exceptions
                            || operation_width != VecWidth::V128
                            || dst_reg.vec_ext2() != 0
                            || src_reg.vec_ext2() != 0
                        {
                            return Err(LowerError::InvalidOperand {
                                op: "X86PackedFpToInt".to_string(),
                                operand: "invalid legacy packed conversion shape".to_string(),
                            });
                        }
                        let prefix = match pp {
                            X86SsePrefix::None => None,
                            X86SsePrefix::OpSize => Some(0x66),
                            X86SsePrefix::Rep => Some(0xF3),
                            X86SsePrefix::Repne => Some(0xF2),
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src_reg);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "X86PackedFpToInt without canonical encoding metadata".to_string(),
                        });
                    }
                }
            }

            OpKind::X86PackedIntToFp16 {
                dst,
                src,
                mask,
                int_elem,
                signed,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp16".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode, w) = match (*int_elem, *signed) {
                    (VecElementType::I16, true) => (X86SsePrefix::Rep, 0x7D, false),
                    (VecElementType::I16, false) => (X86SsePrefix::Repne, 0x7D, false),
                    (VecElementType::I32, true) => (X86SsePrefix::None, 0x5B, false),
                    (VecElementType::I32, false) => (X86SsePrefix::Repne, 0x7A, false),
                    (VecElementType::I64, true) => (X86SsePrefix::None, 0x5B, true),
                    (VecElementType::I64, false) => (X86SsePrefix::Repne, 0x7A, true),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp16".to_string(),
                            operand: "integer element must be I16, I32, or I64".to_string(),
                        });
                    }
                };
                let expected_lanes = src_width.lanes(*int_elem) as u8;
                let dst_bytes = u32::from(expected_lanes) * VecElementType::F16.bytes();
                let expected_dst_width = match dst_bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedIntToFp16".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let Some(X86OpHint::EvexOp {
                    map,
                    pp: hinted_pp,
                    opcode: hinted_opcode,
                    width: hinted_width,
                    w: hinted_w,
                }) = op.x86_hint
                else {
                    return Err(LowerError::UnsupportedOp {
                        op: "X86PackedIntToFp16 without canonical EVEX metadata".to_string(),
                    });
                };
                if map != X86VecMap::Map5
                    || hinted_pp != pp
                    || hinted_opcode != opcode
                    || hinted_width != *src_width
                    || hinted_w != w
                    || *lanes != expected_lanes
                    || *dst_width != expected_dst_width
                    || !*zero_upper
                    || (*mask_zeroing && aaa == 0)
                    || *round == FpRoundMode::RoundNearestTiesAway
                    || *suppress_exceptions != (*round != FpRoundMode::Dynamic)
                    || (*suppress_exceptions && *src_width != VecWidth::V512)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedIntToFp16".to_string(),
                        operand: "invalid packed integer-to-FP16 EVEX shape".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_evex_masked_rr(
                    X86VecMap::Map5,
                    pp,
                    *src_width,
                    w,
                    opcode,
                    dst_reg,
                    src_reg,
                    aaa,
                    *mask_zeroing,
                    *suppress_exceptions,
                    *round,
                );
            }

            OpKind::X86PackedFp16ToInt {
                dst,
                src,
                mask,
                int_elem,
                signed,
                truncate,
                lanes,
                src_width,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFp16ToInt".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                let (pp, opcode) = match (*int_elem, *signed, *truncate) {
                    (VecElementType::I16, true, false) => (X86SsePrefix::OpSize, 0x7D),
                    (VecElementType::I16, true, true) => (X86SsePrefix::OpSize, 0x7C),
                    (VecElementType::I16, false, false) => (X86SsePrefix::None, 0x7D),
                    (VecElementType::I16, false, true) => (X86SsePrefix::None, 0x7C),
                    (VecElementType::I32, true, false) => (X86SsePrefix::OpSize, 0x5B),
                    (VecElementType::I32, true, true) => (X86SsePrefix::Rep, 0x5B),
                    (VecElementType::I32, false, false) => (X86SsePrefix::None, 0x79),
                    (VecElementType::I32, false, true) => (X86SsePrefix::None, 0x78),
                    (VecElementType::I64, true, false) => (X86SsePrefix::OpSize, 0x7B),
                    (VecElementType::I64, true, true) => (X86SsePrefix::OpSize, 0x7A),
                    (VecElementType::I64, false, false) => (X86SsePrefix::OpSize, 0x79),
                    (VecElementType::I64, false, true) => (X86SsePrefix::OpSize, 0x78),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFp16ToInt".to_string(),
                            operand: "integer element must be I16, I32, or I64".to_string(),
                        });
                    }
                };
                let expected_lanes = dst_width.lanes(*int_elem) as u8;
                let src_bytes = u32::from(expected_lanes) * 2;
                let expected_src_width = match src_bytes {
                    0..=8 => VecWidth::V64,
                    9..=16 => VecWidth::V128,
                    17..=32 => VecWidth::V256,
                    _ => VecWidth::V512,
                };
                let aaa = match mask {
                    None => 0,
                    Some(VReg::Arch(ArchReg::X86(X86Reg::K(n @ 1..=7)))) => *n,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86PackedFp16ToInt".to_string(),
                            operand: "mask must be architectural k1-k7".to_string(),
                        });
                    }
                };
                let Some(X86OpHint::EvexOp {
                    map,
                    pp: hinted_pp,
                    opcode: hinted_opcode,
                    width: hinted_width,
                    w: hinted_w,
                }) = op.x86_hint
                else {
                    return Err(LowerError::UnsupportedOp {
                        op: "X86PackedFp16ToInt without canonical EVEX metadata".to_string(),
                    });
                };
                let rounding_valid = if *truncate {
                    *round == FpRoundMode::RoundTowardZero
                } else {
                    *suppress_exceptions == (*round != FpRoundMode::Dynamic)
                        && *round != FpRoundMode::RoundNearestTiesAway
                };
                if map != X86VecMap::Map5
                    || hinted_pp != pp
                    || hinted_opcode != opcode
                    || hinted_width != *dst_width
                    || hinted_w
                    || *lanes != expected_lanes
                    || *src_width != expected_src_width
                    || !*zero_upper
                    || (*mask_zeroing && aaa == 0)
                    || !rounding_valid
                    || (*suppress_exceptions && *dst_width != VecWidth::V512)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86PackedFp16ToInt".to_string(),
                        operand: "invalid packed FP16-to-integer EVEX shape".to_string(),
                    });
                }
                let mut emitter = X86Emitter::new(&mut self.code);
                if *truncate && *suppress_exceptions {
                    // SAE-only forms use EVEX.b=1 with L'L ignored. Emit the
                    // canonical LLVM encoding (L'L=00b), while ZMM operands
                    // still select the architecturally fixed 512-bit form.
                    emitter.emit_evex_masked_rr(
                        X86VecMap::Map5,
                        pp,
                        VecWidth::V128,
                        false,
                        opcode,
                        dst_reg,
                        src_reg,
                        aaa,
                        *mask_zeroing,
                        true,
                        FpRoundMode::Dynamic,
                    );
                } else {
                    emitter.emit_evex_masked_rr(
                        X86VecMap::Map5,
                        pp,
                        *dst_width,
                        false,
                        opcode,
                        dst_reg,
                        src_reg,
                        aaa,
                        *mask_zeroing,
                        *suppress_exceptions,
                        *round,
                    );
                }
            }

            OpKind::X86PackedFpConvertStore { .. } => {
                return Err(LowerError::UnsupportedOp {
                    op: "X86PackedFpConvertStore".to_string(),
                });
            }

            OpKind::MaterializeFlags => {}

            OpKind::SetCF { value } => {
                let mut emitter = X86Emitter::new(&mut self.code);
                if *value {
                    emitter.emit_stc();
                } else {
                    emitter.emit_clc();
                }
            }

            OpKind::SetDF { value } => {
                let mut emitter = X86Emitter::new(&mut self.code);
                if *value {
                    emitter.emit_std();
                } else {
                    emitter.emit_cld();
                }
            }

            OpKind::CmcCF => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_cmc();
            }

            // ================================================================
            // Memory Operations
            // ================================================================
            OpKind::VLoad { dst, addr, width } => {
                if self.mem_helpers {
                    return self.emit_jit_vector_mem_op(
                        op.guest_pc,
                        true,
                        *dst,
                        addr,
                        *width,
                        op.x86_hint,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                if !dst_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VLoad".to_string(),
                        operand: "destination must be vector register".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[dst_reg],
                    );
                    self.emit_vec_mem(enc, dst_reg, None, addr)?;
                } else {
                    if *width != VecWidth::V128 || self.vec_requires_vex(&[dst_reg]) {
                        let enc = self.coerce_vec_encoding(
                            self.default_vec_mov_encoding(*width, 0x6F, op.x86_hint),
                            &[dst_reg],
                        );
                        self.emit_vec_mem(enc, dst_reg, None, addr)?;
                    } else {
                        let prefix = self.legacy_vec_move_prefix(op.x86_hint);
                        self.emit_sse_mov_mem(prefix, 0x6F, dst_reg, addr)?;
                    }
                }
            }

            OpKind::VStore { src, addr, width } => {
                if self.mem_helpers {
                    return self.emit_jit_vector_mem_op(
                        op.guest_pc,
                        false,
                        *src,
                        addr,
                        *width,
                        op.x86_hint,
                    );
                }
                let src_reg = self.get_reg(*src)?;
                if !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VStore".to_string(),
                        operand: "source must be vector register".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[src_reg],
                    );
                    self.emit_vec_mem(enc, src_reg, None, addr)?;
                } else {
                    if *width != VecWidth::V128 || self.vec_requires_vex(&[src_reg]) {
                        let enc = self.coerce_vec_encoding(
                            self.default_vec_mov_encoding(*width, 0x7F, op.x86_hint),
                            &[src_reg],
                        );
                        self.emit_vec_mem(enc, src_reg, None, addr)?;
                    } else {
                        let prefix = self.legacy_vec_move_prefix(op.x86_hint);
                        self.emit_sse_mov_mem(prefix, 0x7F, src_reg, addr)?;
                    }
                }
            }

            OpKind::VMov { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VMov".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[dst_reg, src_reg],
                    );
                    let opcode = enc.opcode;
                    let (reg, rm) = if opcode == 0x7F || opcode == 0x29 {
                        (src_reg, dst_reg)
                    } else {
                        (dst_reg, src_reg)
                    };
                    self.emit_vec_rr(enc, reg, rm, 0);
                } else {
                    if *width != VecWidth::V128 || self.vec_requires_vex(&[dst_reg, src_reg]) {
                        let enc = self.coerce_vec_encoding(
                            self.default_vec_mov_encoding(*width, 0x6F, op.x86_hint),
                            &[dst_reg, src_reg],
                        );
                        self.emit_vec_rr(enc, dst_reg, src_reg, 0);
                    } else {
                        let prefix = self.legacy_vec_move_prefix(op.x86_hint);
                        let opcode = self.sse_opcode(op.x86_hint, 0x6F);
                        let (reg, rm) = if opcode == 0x7F {
                            (src_reg, dst_reg)
                        } else {
                            (dst_reg, src_reg)
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, opcode, reg, rm);
                    }
                }
            }

            OpKind::VAdd {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VAdd {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VAdd".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::I32 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0xFE),
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x58),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x58),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VAdd {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: *elem == VecElementType::F64,
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    let (prefix, opcode) = match elem {
                        VecElementType::I8 => (Some(0x66), 0xFC),
                        VecElementType::I16 => (Some(0x66), 0xFD),
                        VecElementType::I32 => (Some(0x66), 0xFE),
                        VecElementType::I64 => (Some(0x66), 0xD4),
                        VecElementType::F32 => (None, 0x58),
                        VecElementType::F64 => (Some(0x66), 0x58),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VAdd {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VSub {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VSub".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::I32 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0xFA),
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x5C),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x5C),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VSub {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: *elem == VecElementType::F64,
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    let (prefix, opcode) = match elem {
                        VecElementType::I8 => (Some(0x66), 0xF8),
                        VecElementType::I16 => (Some(0x66), 0xF9),
                        VecElementType::I32 => (Some(0x66), 0xFA),
                        VecElementType::I64 => (Some(0x66), 0xFB),
                        VecElementType::F32 => (None, 0x5C),
                        VecElementType::F64 => (Some(0x66), 0x5C),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VSub {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VAddSubSat {
                dst,
                src1,
                src2,
                elem,
                lanes,
                subtract,
                signed,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!(
                            "VAddSubSat {:?}x{} subtract={} signed={}",
                            elem, lanes, subtract, signed
                        ),
                    }
                })?;
                let opcode = match (*elem, *subtract, *signed) {
                    (VecElementType::I8, false, true) => 0xEC,
                    (VecElementType::I16, false, true) => 0xED,
                    (VecElementType::I8, false, false) => 0xDC,
                    (VecElementType::I16, false, false) => 0xDD,
                    (VecElementType::I8, true, true) => 0xE8,
                    (VecElementType::I16, true, true) => 0xE9,
                    (VecElementType::I8, true, false) => 0xD8,
                    (VecElementType::I16, true, false) => 0xD9,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "VAddSubSat {:?}x{} subtract={} signed={}",
                                elem, lanes, subtract, signed
                            ),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VAddSubSat".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp: X86SsePrefix::OpSize,
                            opcode,
                            width,
                            w: false,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    let prefix = self.sse_prefix(op.x86_hint).or(Some(0x66));
                    let opcode = self.sse_opcode(op.x86_hint, opcode);
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VMax {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VMax".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x5F),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x5F),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMax {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: *elem == VecElementType::F64,
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    let (prefix, opcode) = match elem {
                        VecElementType::F32 => (None, 0x5F),
                        VecElementType::F64 => (Some(0x66), 0x5F),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMax {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VX86MinMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
                min,
            } => {
                let width =
                    if *lanes == 1 && matches!(elem, VecElementType::F32 | VecElementType::F64) {
                        VecWidth::V128
                    } else {
                        self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                            LowerError::UnsupportedOp {
                                op: format!("VX86MinMax {:?}x{}", elem, lanes),
                            }
                        })?
                    };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VX86MinMax".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                let opcode = if *min { 0x5D } else { 0x5F };
                let pp = match (*elem, *lanes == 1) {
                    (VecElementType::F32, false) => X86SsePrefix::None,
                    (VecElementType::F64, false) => X86SsePrefix::OpSize,
                    (VecElementType::F32, true) => X86SsePrefix::Rep,
                    (VecElementType::F64, true) => X86SsePrefix::Repne,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("VX86MinMax {:?}x{}", elem, lanes),
                        });
                    }
                };

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width,
                            opcode,
                            ..enc_hint
                        },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp,
                            opcode,
                            width,
                            w: *elem == VecElementType::F64,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0xF3), 0x6F, dst_reg, src1_reg);
                    }
                    let prefix = match pp {
                        X86SsePrefix::None => None,
                        X86SsePrefix::OpSize => Some(0x66),
                        X86SsePrefix::Rep => Some(0xF3),
                        X86SsePrefix::Repne => Some(0xF2),
                    };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VDiv {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VDiv {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VDiv".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let pp = match elem {
                        VecElementType::F32 => X86SsePrefix::None,
                        VecElementType::F64 => X86SsePrefix::OpSize,
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VDiv {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp,
                            opcode: 0x5E,
                            width,
                            w: *elem == VecElementType::F64,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    let prefix = match elem {
                        VecElementType::F32 => None,
                        VecElementType::F64 => Some(0x66),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VDiv {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, 0x5E, dst_reg, src2_reg);
                }
            }

            OpKind::VCmp {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
            } if matches!(
                (elem, cond),
                (
                    VecElementType::I8
                        | VecElementType::I16
                        | VecElementType::I32
                        | VecElementType::I64,
                    VecCmpCond::Eq | VecCmpCond::Gt
                )
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VCmp {:?} {:?}x{}", cond, elem, lanes),
                    }
                })?;
                if !matches!(width, VecWidth::V128 | VecWidth::V256) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("VCmp {:?} {:?}x{}", cond, elem, lanes),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 16,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VCmp".to_string(),
                        operand: "requires matching low vector registers".to_string(),
                    });
                }
                let (expected_map, expected_opcode) = match (*elem, *cond) {
                    (VecElementType::I8, VecCmpCond::Gt) => (X86VecMap::Map0F, 0x64),
                    (VecElementType::I16, VecCmpCond::Gt) => (X86VecMap::Map0F, 0x65),
                    (VecElementType::I32, VecCmpCond::Gt) => (X86VecMap::Map0F, 0x66),
                    (VecElementType::I8, VecCmpCond::Eq) => (X86VecMap::Map0F, 0x74),
                    (VecElementType::I16, VecCmpCond::Eq) => (X86VecMap::Map0F, 0x75),
                    (VecElementType::I32, VecCmpCond::Eq) => (X86VecMap::Map0F, 0x76),
                    (VecElementType::I64, VecCmpCond::Eq) => (X86VecMap::Map0F38, 0x29),
                    (VecElementType::I64, VecCmpCond::Gt) => (X86VecMap::Map0F38, 0x37),
                    _ => unreachable!(),
                };

                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if width == VecWidth::V128
                            && dst_reg == src1_reg
                            && prefix == X86SsePrefix::OpSize
                            && opcode == expected_opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if expected_map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && opcode == expected_opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed VCmp {:?} {:?}x{}",
                                cond, elem, lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VInterleave {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                high,
            } if matches!(
                elem,
                VecElementType::I8
                    | VecElementType::I16
                    | VecElementType::I32
                    | VecElementType::I64
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VInterleave {:?}x{}", elem, lanes),
                    }
                })?;
                if *block_lanes != (16 / elem.bytes()) as u8 {
                    return Err(LowerError::InvalidOperand {
                        op: "VInterleave".to_string(),
                        operand: "requires 128-bit lane blocks".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VInterleave".to_string(),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let opcode = match (*elem, *high) {
                    (VecElementType::I8, false) => 0x60,
                    (VecElementType::I16, false) => 0x61,
                    (VecElementType::I32, false) => 0x62,
                    (VecElementType::I64, false) => 0x6C,
                    (VecElementType::I8, true) => 0x68,
                    (VecElementType::I16, true) => 0x69,
                    (VecElementType::I32, true) => 0x6A,
                    (VecElementType::I64, true) => 0x6D,
                    _ => unreachable!(),
                };
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };

                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && match elem {
                            VecElementType::I8 | VecElementType::I16 => true,
                            VecElementType::I32 => !w,
                            VecElementType::I64 => w,
                            _ => false,
                        } =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed VInterleave {:?}x{}", elem, lanes),
                        });
                    }
                }
            }

            OpKind::VPackSat {
                dst,
                src1,
                src2,
                src_elem,
                to_unsigned,
                src_lanes,
                block_lanes,
            } if matches!(src_elem, VecElementType::I16 | VecElementType::I32) => {
                let width = self
                    .vec_width_from_lanes(*src_elem, *src_lanes)
                    .ok_or_else(|| LowerError::UnsupportedOp {
                        op: format!("VPackSat {:?}x{}", src_elem, src_lanes),
                    })?;
                if *block_lanes != (16 / src_elem.bytes()) as u8 {
                    return Err(LowerError::InvalidOperand {
                        op: "VPackSat".to_string(),
                        operand: "requires 128-bit lane blocks".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let r_m_reg = self.get_reg(*src1)?;
                let first_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, r_m_reg, first_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VPackSat".to_string(),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let (map, opcode) = match (*src_elem, *to_unsigned) {
                    (VecElementType::I16, false) => (X86VecMap::Map0F, 0x63),
                    (VecElementType::I16, true) => (X86VecMap::Map0F, 0x67),
                    (VecElementType::I32, false) => (X86VecMap::Map0F, 0x6B),
                    (VecElementType::I32, true) => (X86VecMap::Map0F38, 0x2B),
                    _ => unreachable!(),
                };
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == first_reg
                        && [dst_reg, r_m_reg, first_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, r_m_reg);
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, r_m_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, r_m_reg, first_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            first_reg,
                            r_m_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && (*src_elem == VecElementType::I16 || !w) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            first_reg,
                            r_m_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed VPackSat {:?}x{}",
                                src_elem, src_lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VByteShuffle {
                dst,
                src,
                control,
                lanes,
                block_lanes,
            } => {
                let width = self
                    .vec_width_from_lanes(VecElementType::I8, *lanes)
                    .ok_or_else(|| LowerError::UnsupportedOp {
                        op: format!("VByteShuffle I8x{lanes}"),
                    })?;
                if *block_lanes != 16 {
                    return Err(LowerError::InvalidOperand {
                        op: "VByteShuffle".to_string(),
                        operand: "requires 16-byte lane blocks".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let control_reg = self.get_reg(*control)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src_reg, control_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VByteShuffle".to_string(),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if width == VecWidth::V128
                            && dst_reg == src_reg
                            && [dst_reg, src_reg, control_reg].into_iter().all(low_vector)
                            && prefix == X86SsePrefix::OpSize
                            && opcode == 0x00 =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), 0x00, dst_reg, control_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && opcode == 0x00
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src_reg, control_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src_reg,
                            control_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && opcode == 0x00
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src_reg,
                            control_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed VByteShuffle I8x{lanes}"),
                        });
                    }
                }
            }

            OpKind::VHorizontalBin {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                subtract,
                saturating,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VHorizontalBin {:?}x{}", elem, lanes),
                    }
                })?;
                if !matches!(elem, VecElementType::I16 | VecElementType::I32)
                    || *block_lanes != (16 / elem.bytes()) as u8
                    || (*saturating && *elem != VecElementType::I16)
                    || width == VecWidth::V512
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VHorizontalBin".to_string(),
                        operand: "requires exact 128-bit I16/I32 lane blocks".to_string(),
                    });
                }
                let opcode = match (elem, subtract, saturating) {
                    (VecElementType::I16, false, false) => 0x01,
                    (VecElementType::I32, false, false) => 0x02,
                    (VecElementType::I16, false, true) => 0x03,
                    (VecElementType::I16, true, false) => 0x05,
                    (VecElementType::I32, true, false) => 0x06,
                    (VecElementType::I16, true, true) => 0x07,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "VHorizontalBin".to_string(),
                            operand: "unsupported element/mode combination".to_string(),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VHorizontalBin".to_string(),
                        operand: "requires matching XMM/YMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed VHorizontalBin {:?}x{}",
                                elem, lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VMulShiftSat {
                dst,
                src1,
                src2,
                src_elem,
                lanes,
                signed1,
                signed2,
                shift_left,
                round,
                sat_bits,
                out_shift,
            } => {
                if *src_elem != VecElementType::I16 || *shift_left != 0 || *sat_bits != 0 {
                    return Err(LowerError::InvalidOperand {
                        op: "VMulShiftSat PMULH[RU]SW".to_string(),
                        operand: "requires I16 multiply, zero left shift, and no saturation"
                            .to_string(),
                    });
                }
                let (expected_map, expected_opcode, mnemonic) = match (
                    *signed1, *signed2, *round, *out_shift,
                ) {
                    (true, true, true, 15) => (X86VecMap::Map0F38, 0x0B, "PMULHRSW"),
                    (true, true, false, 16) => (X86VecMap::Map0F, 0xE5, "PMULHW"),
                    (false, false, false, 16) => (X86VecMap::Map0F, 0xE4, "PMULHUW"),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "VMulShiftSat PMULH[RU]SW".to_string(),
                            operand: "requires signed rounded >>15, signed >>16, or unsigned >>16 semantics"
                                .to_string(),
                        });
                    }
                };
                let width = self
                    .vec_width_from_lanes(VecElementType::I16, *lanes)
                    .ok_or_else(|| LowerError::UnsupportedOp {
                        op: format!("VMulShiftSat {mnemonic} I16x{lanes}"),
                    })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: format!("VMulShiftSat {mnemonic}"),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == expected_opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if expected_map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(
                                Some(0x66),
                                expected_opcode,
                                dst_reg,
                                src2_reg,
                            );
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), expected_opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == expected_opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode: expected_opcode,
                                width,
                                // The PMULH[RU]SW family is WIG. Canonicalize the native
                                // encoding instead of replaying a noncanonical
                                // guest W=1 payload on the host.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == expected_opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode: expected_opcode,
                                width,
                                // The PMULH[RU]SW family is WIG; use the canonical host
                                // encoding for both guest W values.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed {mnemonic} {width:?}"),
                        });
                    }
                }
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op: lane_op @ (VLaneOp::Min | VLaneOp::Max),
                signed,
                set_ovf: false,
            } if matches!(
                elem,
                VecElementType::I8
                    | VecElementType::I16
                    | VecElementType::I32
                    | VecElementType::I64
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VLane {:?} {:?}x{}", lane_op, elem, lanes),
                    }
                })?;
                let (map, opcode) = match (*elem, *lane_op, *signed) {
                    (VecElementType::I8, VLaneOp::Min, false) => (X86VecMap::Map0F, 0xDA),
                    (VecElementType::I8, VLaneOp::Max, false) => (X86VecMap::Map0F, 0xDE),
                    (VecElementType::I16, VLaneOp::Min, true) => (X86VecMap::Map0F, 0xEA),
                    (VecElementType::I16, VLaneOp::Max, true) => (X86VecMap::Map0F, 0xEE),
                    (VecElementType::I8, VLaneOp::Min, true) => (X86VecMap::Map0F38, 0x38),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, true) => {
                        (X86VecMap::Map0F38, 0x39)
                    }
                    (VecElementType::I16, VLaneOp::Min, false) => (X86VecMap::Map0F38, 0x3A),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, false) => {
                        (X86VecMap::Map0F38, 0x3B)
                    }
                    (VecElementType::I8, VLaneOp::Max, true) => (X86VecMap::Map0F38, 0x3C),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, true) => {
                        (X86VecMap::Map0F38, 0x3D)
                    }
                    (VecElementType::I16, VLaneOp::Max, false) => (X86VecMap::Map0F38, 0x3E),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, false) => {
                        (X86VecMap::Map0F38, 0x3F)
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("VLane {:?} {:?}x{}", lane_op, elem, lanes),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VLane packed integer min/max".to_string(),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if *elem != VecElementType::I64
                        && width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if *elem != VecElementType::I64
                        && encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                // All packed-integer min/max VEX encodings are WIG.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && match elem {
                            VecElementType::I8 | VecElementType::I16 => true,
                            VecElementType::I32 => !w,
                            VecElementType::I64 => w,
                            _ => false,
                        } =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                // EVEX byte/word W is ignored; dword/qword use W0/W1.
                                w: *elem == VecElementType::I64,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed packed integer {:?} {:?}x{}",
                                lane_op, elem, lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op: VLaneOp::Sign,
                signed: true,
                set_ovf: false,
            } if matches!(
                elem,
                VecElementType::I8 | VecElementType::I16 | VecElementType::I32
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VLane Sign {:?}x{}", elem, lanes),
                    }
                })?;
                if !matches!(width, VecWidth::V128 | VecWidth::V256) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("VLane Sign {:?}x{}", elem, lanes),
                    });
                }
                let opcode = match elem {
                    VecElementType::I8 => 0x08,
                    VecElementType::I16 => 0x09,
                    VecElementType::I32 => 0x0A,
                    _ => unreachable!("guarded PSIGN element width"),
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let low_vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 16,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(low_vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VLane Sign PSIGN[BWD]".to_string(),
                        operand: "requires matching low XMM/YMM registers".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                // VPSIGNB/W/D are WIG. Canonicalize guest W=1 to W=0
                                // instead of replaying a noncanonical host encoding.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed PSIGN[BWD] {width:?}"),
                        });
                    }
                }
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op: VLaneOp::AvgRnd,
                signed: false,
                set_ovf: false,
            } if matches!(elem, VecElementType::I8 | VecElementType::I16) => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VLane AvgRnd {:?}x{}", elem, lanes),
                    }
                })?;
                let opcode = match elem {
                    VecElementType::I8 => 0xE0,
                    VecElementType::I16 => 0xE3,
                    _ => unreachable!("guarded PAVG element width"),
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VLane AvgRnd PAVG[BW]".to_string(),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed PAVG[BW] {width:?}"),
                        });
                    }
                }
            }

            OpKind::VSadBytes {
                dst,
                src1,
                src2,
                width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VSadBytes PSADBW".to_string(),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if *width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == 0xF6 =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0x66), 0xF6, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == 0xF6
                        && encoded_width == *width
                        && *width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode: 0xF6,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == 0xF6
                        && encoded_width == *width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode: 0xF6,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed PSADBW {width:?}"),
                        });
                    }
                }
            }

            OpKind::X86Phminposuw { dst, src } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let low_xmm = |reg: PhysReg| matches!(reg, PhysReg::Xmm(0..=15));
                if !low_xmm(dst_reg) || !low_xmm(src_reg) {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Phminposuw".to_string(),
                        operand: "requires low XMM registers".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::OpSize,
                        opcode: 0x41,
                    }) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), 0x41, dst_reg, src_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0x41,
                        width: VecWidth::V128,
                        ..
                    }) => {
                        // VEX.W is ignored architecturally; emit canonical W0.
                        // vvvv=0 is encoded inverted as the required 1111b.
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map: X86VecMap::Map0F38,
                                pp: X86SsePrefix::OpSize,
                                opcode: 0x41,
                                width: VecWidth::V128,
                                w: false,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "unhinted or malformed PHMINPOSUW".to_string(),
                        });
                    }
                }
            }

            OpKind::X86MovMask {
                dst,
                src,
                elem,
                lanes,
                dst_width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let valid_gpr = matches!(
                    dst_reg,
                    PhysReg::Rax
                        | PhysReg::Rcx
                        | PhysReg::Rdx
                        | PhysReg::Rbx
                        | PhysReg::Rsi
                        | PhysReg::Rdi
                        | PhysReg::R8
                        | PhysReg::R9
                        | PhysReg::R10
                        | PhysReg::R11
                        | PhysReg::R12
                        | PhysReg::R13
                        | PhysReg::R14
                        | PhysReg::R15
                );
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::InvalidOperand {
                        op: "X86MovMask".to_string(),
                        operand: format!("invalid {elem:?} lane count {lanes}"),
                    }
                })?;
                let valid_source = matches!(
                    (width, src_reg),
                    (VecWidth::V128, PhysReg::Xmm(0..=15)) | (VecWidth::V256, PhysReg::Ymm(0..=15))
                );
                if !valid_gpr
                    || !valid_source
                    || !matches!(
                        (elem, lanes),
                        (VecElementType::I8, 16 | 32)
                            | (VecElementType::F32, 4 | 8)
                            | (VecElementType::F64, 2 | 4)
                    )
                    || !matches!(dst_width, OpWidth::W32 | OpWidth::W64)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86MovMask".to_string(),
                        operand: "requires a safe legacy GPR and matching low XMM/YMM source"
                            .to_string(),
                    });
                }
                let encoding_matches = |opcode: u8, pp: X86SsePrefix| match (opcode, pp, elem) {
                    (0x50, X86SsePrefix::None, VecElementType::F32)
                    | (0x50, X86SsePrefix::OpSize, VecElementType::F64)
                    | (0xD7, X86SsePrefix::OpSize, VecElementType::I8) => true,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if width == VecWidth::V128 && encoding_matches(opcode, prefix) =>
                    {
                        let legacy_prefix = match prefix {
                            X86SsePrefix::None => None,
                            X86SsePrefix::OpSize => Some(0x66),
                            _ => unreachable!("validated MOVMSK legacy prefix"),
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_mask_rr(
                            legacy_prefix,
                            opcode,
                            dst_reg,
                            src_reg,
                            *dst_width == OpWidth::W64,
                        );
                    }
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F,
                        pp,
                        opcode,
                        width: encoded_width,
                        ..
                    }) if *dst_width == OpWidth::W32
                        && encoded_width == width
                        && width != VecWidth::V512
                        && encoding_matches(opcode, pp) =>
                    {
                        // Every family member is WIG; emit canonical VEX.W0.
                        // vvvv=0 becomes the required encoded 1111b.
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map: X86VecMap::Map0F,
                                pp,
                                opcode,
                                width,
                                w: false,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "unhinted or malformed MOVMSK/PMOVMSKB".to_string(),
                        });
                    }
                }
            }

            OpKind::X86MovdQ {
                dst,
                src,
                width,
                zero_upper,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let safe_gpr = |reg: PhysReg| {
                    matches!(
                        reg,
                        PhysReg::Rax
                            | PhysReg::Rcx
                            | PhysReg::Rdx
                            | PhysReg::Rbx
                            | PhysReg::Rsi
                            | PhysReg::Rdi
                            | PhysReg::R8
                            | PhysReg::R9
                            | PhysReg::R10
                            | PhysReg::R11
                            | PhysReg::R12
                            | PhysReg::R13
                            | PhysReg::R14
                            | PhysReg::R15
                    )
                };
                let (xmm, gpr, vector_dst) = match (dst_reg, src_reg) {
                    (xmm @ PhysReg::Xmm(0..=31), gpr) if safe_gpr(gpr) => (xmm, gpr, true),
                    (gpr, xmm @ PhysReg::Xmm(0..=31)) if safe_gpr(gpr) => (xmm, gpr, false),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86MovdQ".to_string(),
                            operand: "requires one safe GPR and one XMM register".to_string(),
                        });
                    }
                };
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::InvalidOperand {
                        op: "X86MovdQ".to_string(),
                        operand: "width must be 32 or 64 bits".to_string(),
                    });
                }
                let expected_opcode = if vector_dst { 0x6E } else { 0x7E };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if prefix == X86SsePrefix::OpSize
                            && opcode == expected_opcode
                            && xmm.encoding() < 16
                            && !*zero_upper =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_movd_q_rr(opcode, xmm, gpr, *width);
                    }
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::OpSize,
                        opcode,
                        width: VecWidth::V128,
                        w,
                    }) if opcode == expected_opcode
                        && w == (*width == OpWidth::W64)
                        && xmm.encoding() < 16
                        && *zero_upper == vector_dst =>
                    {
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map: X86VecMap::Map0F,
                                pp: X86SsePrefix::OpSize,
                                opcode,
                                width: VecWidth::V128,
                                w,
                            },
                            xmm,
                            gpr,
                            0,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::OpSize,
                        opcode,
                        width: VecWidth::V128,
                        w,
                    }) if opcode == expected_opcode
                        && w == (*width == OpWidth::W64)
                        && *zero_upper == vector_dst =>
                    {
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map: X86VecMap::Map0F,
                                pp: X86SsePrefix::OpSize,
                                opcode,
                                width: VecWidth::V128,
                                w,
                            },
                            xmm,
                            gpr,
                            0,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "unhinted or malformed MOVD/MOVQ".to_string(),
                        });
                    }
                }
            }

            OpKind::VMpsadbw {
                dst,
                src1,
                src2,
                mask,
                width,
                imm,
                zeroing,
            } => {
                if mask.is_some() || *zeroing {
                    return Err(LowerError::UnsupportedOp {
                        op: "masked AVX10.2 VMPSADBW requires EVEX lowering".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 16,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VMpsadbw MPSADBW".to_string(),
                        operand: "requires matching low XMM/YMM registers".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if *width == VecWidth::V128
                        && dst_reg == src1_reg
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == 0x42 =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op3a_rr_imm(Some(0x66), 0x42, dst_reg, src2_reg, *imm);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F3A
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == 0x42
                        && encoded_width == *width =>
                    {
                        self.emit_vec_rrr_imm(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode: 0x42,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                            *imm,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed MPSADBW {width:?}"),
                        });
                    }
                }
            }

            OpKind::VDotProduct {
                dst,
                acc: VReg::Imm(0),
                src1,
                src2,
                mask: None,
                src_elem,
                acc_elem,
                width,
                src1_unsigned,
                saturate,
                zeroing: false,
            } if matches!(
                (src_elem, acc_elem, src1_unsigned, saturate),
                (VecElementType::I8, VecElementType::I16, true, true)
                    | (VecElementType::I16, VecElementType::I32, false, false)
            ) =>
            {
                let maddubs = *src_elem == VecElementType::I8;
                let instruction = if maddubs { "PMADDUBSW" } else { "PMADDWD" };
                let expected_map = if maddubs {
                    X86VecMap::Map0F38
                } else {
                    X86VecMap::Map0F
                };
                let expected_opcode = if maddubs { 0x04 } else { 0xF5 };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: format!("VDotProduct {instruction}"),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if *width == VecWidth::V128
                            && dst_reg == src1_reg
                            && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                            && prefix == X86SsePrefix::OpSize
                            && opcode == expected_opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if maddubs {
                            emitter.emit_sse_op38_rr(
                                Some(0x66),
                                expected_opcode,
                                dst_reg,
                                src2_reg,
                            );
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), expected_opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && opcode == expected_opcode
                        && encoded_width == *width
                        && *width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && opcode == expected_opcode
                        && encoded_width == *width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed {instruction} {width:?}"),
                        });
                    }
                }
            }

            OpKind::VUnary {
                dst,
                src,
                elem,
                lanes,
                op: VecUnaryOp::Abs,
            } if matches!(
                elem,
                VecElementType::I8
                    | VecElementType::I16
                    | VecElementType::I32
                    | VecElementType::I64
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VUnary Abs {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VUnary Abs".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc) = self.vec_hint(op.x86_hint) {
                    self.emit_vec_rr(VecEncoding { width, ..enc }, dst_reg, src_reg, 0);
                } else if matches!(op.x86_hint, Some(X86OpHint::SseOp { .. })) {
                    if *elem == VecElementType::I64 || width != VecWidth::V128 {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("legacy VUnary Abs {:?}x{}", elem, lanes),
                        });
                    }
                    let prefix = self.sse_prefix(op.x86_hint).or(Some(0x66));
                    let opcode = self.sse_opcode(
                        op.x86_hint,
                        match elem {
                            VecElementType::I8 => 0x1C,
                            VecElementType::I16 => 0x1D,
                            VecElementType::I32 => 0x1E,
                            _ => unreachable!(),
                        },
                    );
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_op38_rr(prefix, opcode, dst_reg, src_reg);
                } else {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("unhinted VUnary Abs {:?}x{}", elem, lanes),
                    });
                }
            }

            OpKind::VUnary {
                elem, lanes, op, ..
            } => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("VUnary {:?} {:?}x{} (x86)", op, elem, lanes),
                });
            }

            OpKind::VReduce {
                elem, lanes, op, ..
            } => {
                // Vector across-lanes reduction (ADDV/SMAXV/…) is emitted only
                // by the AArch64 lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VReduce {:?} {:?}x{} (x86)", op, elem, lanes),
                });
            }

            OpKind::VFMinMaxNm { elem, lanes, .. } => {
                // FP numeric min/max (FMAXNM/FMINNM) is emitted only by the
                // AArch64 lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VFMinMaxNm {:?}x{} (x86)", elem, lanes),
                });
            }

            OpKind::VPermute2 {
                elem, lanes, kind, ..
            } => {
                // Vector permute (ZIP/UZP/TRN) is emitted only by the AArch64
                // lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VPermute2 {:?} {:?}x{} (x86)", kind, elem, lanes),
                });
            }

            OpKind::VTableLookup {
                num_tables, lanes, ..
            } => {
                // Vector table lookup (TBL/TBX) is emitted only by the AArch64
                // lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VTableLookup {num_tables}-table x{lanes} (x86)"),
                });
            }

            OpKind::VMul {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VMul {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VMul".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if *elem == VecElementType::I64
                    || width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::I16 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0xD5),
                        VecElementType::I32 => (X86VecMap::Map0F38, X86SsePrefix::OpSize, 0x40),
                        VecElementType::I64 => (X86VecMap::Map0F38, X86SsePrefix::OpSize, 0x40),
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x59),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x59),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMul {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if *elem == VecElementType::I64
                        || self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg])
                    {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: matches!(elem, VecElementType::I64 | VecElementType::F64),
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    match elem {
                        VecElementType::I16 => {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_sse_mov_rr(Some(0x66), 0x6F, dst_reg, src1_reg);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_sse_mov_rr(Some(0x66), 0xD5, dst_reg, src2_reg);
                        }
                        VecElementType::I32 => {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_sse_mov_rr(Some(0x66), 0x6F, dst_reg, src1_reg);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_sse_op38_rr(Some(0x66), 0x40, dst_reg, src2_reg);
                        }
                        VecElementType::F32 | VecElementType::F64 => {
                            let prefix = if matches!(elem, VecElementType::F64) {
                                Some(0x66)
                            } else {
                                None
                            };
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_sse_mov_rr(prefix, 0x59, dst_reg, src2_reg);
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMul {:?}x{}", elem, lanes),
                            });
                        }
                    }
                }
            }

            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            }
            | OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            }
            | OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            }
            | OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let default_opcode = match &op.kind {
                    OpKind::VAnd { .. } => 0x54,
                    OpKind::VAndNot { .. } => 0x55,
                    OpKind::VOr { .. } => 0x56,
                    OpKind::VXor { .. } => 0x57,
                    _ => unreachable!(),
                };
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "vector logic".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                } else if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if *width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let kind = if self.vec_requires_evex(*width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp: X86SsePrefix::None,
                            opcode: default_opcode,
                            width: *width,
                            w: false,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    let prefix = self.sse_prefix(op.x86_hint);
                    let opcode = self.sse_opcode(op.x86_hint, default_opcode);
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x28, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VShift {
                dst,
                src,
                amount,
                shift,
                elem,
                lanes,
            } => {
                if *shift != ShiftOp::Lsl || *elem != VecElementType::I32 {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("VShift {:?} {:?}x{}", shift, elem, lanes),
                    });
                }
                let imm = match amount {
                    SrcOperand::Imm(val) => {
                        if *val < 0 || *val > u8::MAX as i64 {
                            return Err(LowerError::InvalidOperand {
                                op: "VShift".to_string(),
                                operand: "imm out of range".to_string(),
                            });
                        }
                        *val as u8
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "VShift with non-imm".to_string(),
                        });
                    }
                };

                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VShift {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VShift".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src_reg],
                    );
                    self.emit_vec_shift_imm(enc, dst_reg, src_reg, imm);
                } else if width != VecWidth::V128 || self.vec_requires_vex(&[dst_reg, src_reg]) {
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0x72,
                        width,
                        w: false,
                    };
                    self.emit_vec_shift_imm(enc, dst_reg, src_reg, imm);
                } else {
                    let prefix = Some(0x66);
                    if dst_reg != src_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    if let Some(prefix) = prefix {
                        emitter.code.emit_u8(prefix);
                    }
                    emitter.emit_rex_for_xmm(dst_reg, dst_reg);
                    emitter.code.emit_u8(0x0F);
                    emitter.code.emit_u8(0x72);
                    emitter.emit_modrm_digit(0b11, 6, dst_reg);
                    emitter.code.emit_u8(imm);
                }
            }

            OpKind::X86CheckAlignment { addr, alignment } => {
                self.emit_x86_check_alignment(op.guest_pc, addr, *alignment)?;
            }

            OpKind::X86CacheControl { kind, .. } if *kind == X86CacheControlKind::Cldemote => {
                // CLDEMOTE is an architecturally ignorable cache-placement
                // hint and raises no memory-address exception. Executing no
                // host instruction therefore preserves guest semantics without
                // exposing the guest linear address to the host cache hierarchy.
            }

            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => {
                // JIT memory mode: route through the MMU helper-call path
                // (translate + fault-bail) instead of a direct host-pointer load.
                if self.mem_helpers {
                    return self.emit_jit_mem_op(
                        op.guest_pc,
                        true,
                        Some(*dst),
                        None,
                        None,
                        None,
                        None,
                        addr,
                        *width,
                        *sign,
                        0,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);
                let preserve_x86_partial = matches!(dst, VReg::Arch(ArchReg::X86(_)))
                    && matches!(op_width, OpWidth::W8 | OpWidth::W16)
                    && matches!(sign, SignExtend::Zero);
                let needs_extend = op_width != OpWidth::W64 && !preserve_x86_partial;

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm(dst_reg, base_reg, 0, op_width);

                        // Sign/zero extend if loading smaller than 64-bit
                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    // 32-bit loads automatically zero-extend
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::BaseOffset {
                        base,
                        offset,
                        disp_size,
                    } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm_disp(
                            dst_reg,
                            base_reg,
                            *offset as i32,
                            *disp_size,
                            op_width,
                        );

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::PcRel { offset, base, .. } => {
                        let disp_offset = {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rm_pcrel(dst_reg, 0, op_width)
                        };
                        let insn_end = self.code.position();

                        let disp = if let Some(base_pc) = base {
                            let target = (*base_pc as i64 + *offset) as u64;
                            let disp = if self.pcrel_adjust {
                                let next_rip = self.guest_base as i64 + insn_end as i64;
                                target as i64 - next_rip
                            } else {
                                *offset
                            };
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Load".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            self.relocations.push(Relocation {
                                offset: disp_offset,
                                kind: RelocKind::PcRel32,
                                target: RelocTarget::GuestAddr(target),
                            });
                            disp
                        } else {
                            let disp = *offset;
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Load".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            disp
                        };

                        self.code.patch_i32(disp_offset, disp as i32);

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        let mut emitter = X86Emitter::new(&mut self.code);
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    let mut emitter = X86Emitter::new(&mut self.code);
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::Absolute(abs_addr) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm_abs(dst_reg, *abs_addr, op_width);

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                        disp_size,
                    } => {
                        let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                        let index_reg = self.get_reg(*index)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm_sib_disp(
                            dst_reg, base_reg, index_reg, *scale, *disp, *disp_size, op_width,
                        );

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Load with unsupported addressing: {:?}", addr),
                        });
                    }
                }
            }

            OpKind::Store { src, addr, width } => {
                // JIT memory mode: route through the MMU helper-call path.
                if self.mem_helpers {
                    let (src_reg, src_imm) = match src {
                        VReg::Imm(imm) => (None, Some(*imm)),
                        other => (Some(*other), None),
                    };
                    return self.emit_jit_mem_op(
                        op.guest_pc,
                        false,
                        None,
                        None,
                        src_reg,
                        src_imm,
                        None,
                        addr,
                        *width,
                        SignExtend::Zero,
                        0,
                    );
                }
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);

                if let VReg::Imm(imm) = src {
                    let imm_ok = match op_width {
                        OpWidth::W64 => *imm >= i32::MIN as i64 && *imm <= i32::MAX as i64,
                        OpWidth::W128 => false,
                        _ => true,
                    };

                    if imm_ok {
                        match addr {
                            Address::Direct(base) => {
                                let base_reg = self.get_reg(*base)?;
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_disp(
                                    base_reg,
                                    0,
                                    DispSize::Auto,
                                    *imm,
                                    op_width,
                                );
                                return Ok(());
                            }
                            Address::BaseOffset {
                                base,
                                offset,
                                disp_size,
                            } => {
                                let base_reg = self.get_reg(*base)?;
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_disp(
                                    base_reg,
                                    *offset as i32,
                                    *disp_size,
                                    *imm,
                                    op_width,
                                );
                                return Ok(());
                            }
                            Address::PcRel { offset, base, .. } => {
                                let disp_offset = {
                                    let mut emitter = X86Emitter::new(&mut self.code);
                                    emitter.emit_mov_mi_pcrel(0, op_width, *imm)
                                };
                                let insn_end = self.code.position();

                                let disp = if let Some(base_pc) = base {
                                    let target = (*base_pc as i64 + *offset) as u64;
                                    let disp = if self.pcrel_adjust {
                                        let next_rip = self.guest_base as i64 + insn_end as i64;
                                        target as i64 - next_rip
                                    } else {
                                        *offset
                                    };
                                    if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                        return Err(LowerError::InvalidOperand {
                                            op: "Store".to_string(),
                                            operand: "PcRel offset out of range".to_string(),
                                        });
                                    }
                                    self.relocations.push(Relocation {
                                        offset: disp_offset,
                                        kind: RelocKind::PcRel32,
                                        target: RelocTarget::GuestAddr(target),
                                    });
                                    disp
                                } else {
                                    let disp = *offset;
                                    if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                        return Err(LowerError::InvalidOperand {
                                            op: "Store".to_string(),
                                            operand: "PcRel offset out of range".to_string(),
                                        });
                                    }
                                    disp
                                };

                                self.code.patch_i32(disp_offset, disp as i32);
                                return Ok(());
                            }
                            Address::Absolute(abs_addr) => {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_abs(*abs_addr, *imm, op_width);
                                return Ok(());
                            }
                            Address::BaseIndexScale {
                                base,
                                index,
                                scale,
                                disp,
                                disp_size,
                            } => {
                                let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                                let index_reg = self.get_reg(*index)?;
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_sib_disp(
                                    base_reg, index_reg, *scale, *disp, *disp_size, *imm, op_width,
                                );
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }

                let src_reg = self.get_reg(*src)?;

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr(base_reg, 0, src_reg, op_width);
                    }
                    Address::BaseOffset {
                        base,
                        offset,
                        disp_size,
                    } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr_disp(
                            base_reg,
                            *offset as i32,
                            *disp_size,
                            src_reg,
                            op_width,
                        );
                    }
                    Address::PcRel { offset, base, .. } => {
                        let disp_offset = {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_mr_pcrel(0, src_reg, op_width)
                        };
                        let insn_end = self.code.position();

                        let disp = if let Some(base_pc) = base {
                            let target = (*base_pc as i64 + *offset) as u64;
                            let disp = if self.pcrel_adjust {
                                let next_rip = self.guest_base as i64 + insn_end as i64;
                                target as i64 - next_rip
                            } else {
                                *offset
                            };
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Store".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            self.relocations.push(Relocation {
                                offset: disp_offset,
                                kind: RelocKind::PcRel32,
                                target: RelocTarget::GuestAddr(target),
                            });
                            disp
                        } else {
                            let disp = *offset;
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Store".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            disp
                        };

                        self.code.patch_i32(disp_offset, disp as i32);
                    }
                    Address::Absolute(abs_addr) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr_abs(*abs_addr, src_reg, op_width);
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                        disp_size,
                    } => {
                        let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                        let index_reg = self.get_reg(*index)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr_sib_disp(
                            base_reg, index_reg, *scale, *disp, *disp_size, src_reg, op_width,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Store with unsupported addressing: {:?}", addr),
                        });
                    }
                }
            }

            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed,
            } => {
                let skip = self.emit_predicated_memory_guard("PredLoad", *cond, addr, None)?;
                let load = SmirOp::new(
                    op.id,
                    op.guest_pc,
                    OpKind::Load {
                        dst: *dst,
                        addr: addr.clone(),
                        width: *width,
                        sign: *signed,
                    },
                );
                self.lower_op(&load)?;
                self.patch_rel32_to_current(skip)?;
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::PredStore {
                src,
                cond,
                addr,
                width,
            } => {
                let skip =
                    self.emit_predicated_memory_guard("PredStore", *cond, addr, Some(src))?;
                let store = SmirOp::new(
                    op.id,
                    op.guest_pc,
                    OpKind::Store {
                        src: Self::pred_store_src_to_vreg(src)?,
                        addr: addr.clone(),
                        width: *width,
                    },
                );
                self.lower_op(&store)?;
                self.patch_rel32_to_current(skip)?;
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::RepStos {
                dst,
                src,
                count,
                width,
            } => {
                let dst_reg = self.get_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let count_reg = self.get_reg(*count)?;

                if dst_reg != PhysReg::Rdi || src_reg != PhysReg::Rax || count_reg != PhysReg::Rcx {
                    return Err(LowerError::InvalidOperand {
                        op: "RepStos".to_string(),
                        operand: "requires RDI/RAX/RCX".to_string(),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                match width {
                    MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8 => {
                        emitter.emit_rep_stos(*width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RepStos width {:?}", width),
                        });
                    }
                }
            }

            OpKind::RepMovs {
                dst,
                src,
                count,
                width,
            } => {
                let dst_reg = self.get_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let count_reg = self.get_reg(*count)?;

                if dst_reg != PhysReg::Rdi || src_reg != PhysReg::Rsi || count_reg != PhysReg::Rcx {
                    return Err(LowerError::InvalidOperand {
                        op: "RepMovs".to_string(),
                        operand: "requires RDI/RSI/RCX".to_string(),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                match width {
                    MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8 => {
                        emitter.emit_rep_movs(*width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RepMovs width {:?}", width),
                        });
                    }
                }
            }

            OpKind::X86String {
                kind,
                rep,
                accumulator,
                src_index,
                dst_index,
                count,
                src_segment,
                width,
                address_width,
            } => {
                let require = |actual: PhysReg, expected: PhysReg, role: &str| {
                    if actual == expected {
                        Ok(())
                    } else {
                        Err(LowerError::InvalidOperand {
                            op: format!("X86String {kind:?} {rep:?}"),
                            operand: format!("{role} requires {expected:?}"),
                        })
                    }
                };
                if matches!(
                    kind,
                    X86StringKind::Stos | X86StringKind::Lods | X86StringKind::Scas
                ) {
                    require(self.get_reg(*accumulator)?, PhysReg::Rax, "accumulator")?;
                }
                if matches!(
                    kind,
                    X86StringKind::Movs | X86StringKind::Lods | X86StringKind::Cmps
                ) {
                    require(self.get_reg(*src_index)?, PhysReg::Rsi, "source index")?;
                }
                if matches!(
                    kind,
                    X86StringKind::Movs
                        | X86StringKind::Stos
                        | X86StringKind::Scas
                        | X86StringKind::Cmps
                ) {
                    require(self.get_reg(*dst_index)?, PhysReg::Rdi, "destination index")?;
                }
                if *rep != X86RepMode::None {
                    require(self.get_reg(*count)?, PhysReg::Rcx, "repeat count")?;
                }
                if src_segment.is_some() {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("X86String {kind:?} with guest segment base"),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_x86_string(*kind, *rep, *width, *address_width)?;
            }

            OpKind::X86ReadTsc { dst_lo, dst_hi } => {
                let lo = self.get_dst_reg(*dst_lo)?;
                let hi = self.get_dst_reg(*dst_hi)?;
                if lo != PhysReg::Rax || hi != PhysReg::Rdx {
                    return Err(LowerError::InvalidOperand {
                        op: "X86ReadTsc".to_string(),
                        operand: "requires EAX/EDX destinations".to_string(),
                    });
                }
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0x31);
            }

            OpKind::X86Random { dst, width, seed } => {
                if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Random".to_string(),
                        operand: format!("unsupported width {width:?}"),
                    });
                }
                let dst = self.get_dst_reg(*dst)?;
                Self::ensure_flag_stack_operands_safe("X86Random", &[dst])?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_x86_random(dst, *width, *seed);
            }

            OpKind::X86ReadPid { dst } => {
                let index =
                    Self::x86_gpr_index(*dst).ok_or_else(|| LowerError::InvalidOperand {
                        op: "X86ReadPid".to_string(),
                        operand: "destination must be an architectural x86 GPR".to_string(),
                    })?;
                if matches!(index, 4 | 5) || index > 31 {
                    return Err(LowerError::InvalidOperand {
                        op: "X86ReadPid".to_string(),
                        operand: "RSP/RBP cannot be an RDPID destination in native code"
                            .to_string(),
                    });
                }
                if index <= 15 {
                    let dst = self.get_dst_reg(*dst)?;
                    Self::ensure_flag_stack_operands_safe("X86ReadPid", &[dst])?;
                    let mut emitter = X86Emitter::new(&mut self.code);
                    // RDPID reports guest IA32_TSC_AUX, not the host thread's
                    // TSC_AUX. The destination is architecturally overwritten,
                    // so it is also the flag-neutral state-pointer scratch.
                    emitter.emit_mov_rm(dst, PhysReg::Rbp, X86_STATE_PTR_AT_RBP, OpWidth::W64);
                    emitter.emit_mov_rm(dst, dst, X86_GUEST_TSC_AUX_OFFSET, OpWidth::W32);
                } else {
                    // APX EGPRs have no physical host counterpart. Preserve two
                    // legacy scratches, zero-extend TSC_AUX through ECX, and
                    // commit it directly to GuestRegs.gpr[index].
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_push(PhysReg::Rax);
                    emitter.emit_push(PhysReg::Rcx);
                    emitter.emit_mov_rm(
                        PhysReg::Rax,
                        PhysReg::Rbp,
                        X86_STATE_PTR_AT_RBP,
                        OpWidth::W64,
                    );
                    emitter.emit_mov_rm(
                        PhysReg::Rcx,
                        PhysReg::Rax,
                        X86_GUEST_TSC_AUX_OFFSET,
                        OpWidth::W32,
                    );
                    emitter.emit_mov_mr(
                        PhysReg::Rax,
                        i32::from(index) * 8,
                        PhysReg::Rcx,
                        OpWidth::W64,
                    );
                    emitter.emit_pop(PhysReg::Rcx);
                    emitter.emit_pop(PhysReg::Rax);
                }
            }

            OpKind::X86XGetBv {
                dst_low,
                dst_high,
                selector,
            } => {
                let low = self.get_dst_reg(*dst_low)?;
                let high = self.get_dst_reg(*dst_high)?;
                let selector = self.get_reg(*selector)?;
                if low != PhysReg::Rax || high != PhysReg::Rdx || selector != PhysReg::Rcx {
                    return Err(LowerError::InvalidOperand {
                        op: "X86XGetBv".to_string(),
                        operand: "requires EAX/EDX destinations and ECX selector".to_string(),
                    });
                }

                // Preserve all architectural flags and the old RAX until both
                // fault conditions have been ruled out. A deoptimization must
                // restart XGETBV in the interpreter with byte-exact input state.
                self.code.emit_u8(0x9C); // pushfq
                self.code.emit_u8(0x50); // push rax
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x8B);
                self.code.emit_u8(0x45);
                self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rax,[rbp+state]

                // test dword [rax+cr4], CR4.OSXSAVE
                self.code.emit_u8(0xF7);
                self.code.emit_u8(0x80);
                self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
                self.code.emit_u32(1 << 18);
                // jz .fault (#UD in the interpreter)
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0x84);
                let osxsave_fault = self.code.position();
                self.code.emit_u32(0);

                // Only XCR0 (ECX=0) and XINUSE (ECX=1) exist in this model.
                self.code.emit_u8(0x83);
                self.code.emit_u8(0xF9);
                self.code.emit_u8(0x01); // cmp ecx,1
                // ja .fault (#GP(0) in the interpreter)
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0x87);
                let selector_fault = self.code.position();
                self.code.emit_u32(0);

                // rdx = XCR0; ECX=1 selects XINUSE & XCR0.
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x8B);
                self.code.emit_u8(0x90);
                self.code.emit_u32(X86_GUEST_XCR0_OFFSET as u32);
                self.code.emit_u8(0x85);
                self.code.emit_u8(0xC9); // test ecx,ecx
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0x84); // jz .selected
                let xcr0_selected = self.code.position();
                self.code.emit_u32(0);
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x23);
                self.code.emit_u8(0x90);
                self.code.emit_u32(X86_GUEST_XGETBV1_OFFSET as u32); // and rdx,[rax+xgetbv1]
                let selected = self.code.position();
                self.code.patch_i32(
                    xcr0_selected,
                    (selected as i64 - (xcr0_selected as i64 + 4)) as i32,
                );

                // Split the selected 64-bit value into zero-extended EDX:EAX.
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x89);
                self.code.emit_u8(0xD0); // mov rax,rdx
                self.code.emit_u8(0x48);
                self.code.emit_u8(0xC1);
                self.code.emit_u8(0xEA);
                self.code.emit_u8(0x20); // shr rdx,32
                self.code.emit_u8(0x89);
                self.code.emit_u8(0xC0); // mov eax,eax (zero-extend low half)
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8); // discard saved RAX
                }
                self.code.emit_u8(0x9D); // popfq
                self.code.emit_u8(0xE9); // jmp .done
                let success_done = self.code.position();
                self.code.emit_u32(0);

                let fault = self.code.position();
                for branch in [osxsave_fault, selector_fault] {
                    self.code
                        .patch_i32(branch, (fault as i64 - (branch as i64 + 4)) as i32);
                }
                self.code.emit_u8(0x58); // restore old RAX
                self.code.emit_u8(0x9D); // restore flags
                self.emit_native_exit(op.guest_pc);

                let done = self.code.position();
                self.code.patch_i32(
                    success_done,
                    (done as i64 - (success_done as i64 + 4)) as i32,
                );
            }

            OpKind::IoIn { dst, port, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                if dst_reg != PhysReg::Rax {
                    return Err(LowerError::InvalidOperand {
                        op: "IoIn".to_string(),
                        operand: "destination must be RAX".to_string(),
                    });
                }

                let imm_port = if let VReg::Imm(val) = port {
                    if *val < 0 || *val > u8::MAX as i64 {
                        return Err(LowerError::InvalidOperand {
                            op: "IoIn".to_string(),
                            operand: "port immediate out of range".to_string(),
                        });
                    }
                    Some(*val as u8)
                } else {
                    None
                };

                if imm_port.is_none() {
                    let port_reg = self.get_reg(*port)?;
                    if port_reg != PhysReg::Rdx {
                        return Err(LowerError::InvalidOperand {
                            op: "IoIn".to_string(),
                            operand: "port must be DX".to_string(),
                        });
                    }
                }

                match width {
                    MemWidth::B1 => {
                        if let Some(port) = imm_port {
                            self.code.emit_u8(0xE4);
                            self.code.emit_u8(port);
                        } else {
                            self.code.emit_u8(0xEC);
                        }
                    }
                    MemWidth::B2 => {
                        self.code.emit_u8(0x66);
                        if let Some(port) = imm_port {
                            self.code.emit_u8(0xE5);
                            self.code.emit_u8(port);
                        } else {
                            self.code.emit_u8(0xED);
                        }
                    }
                    MemWidth::B4 => {
                        if let Some(port) = imm_port {
                            self.code.emit_u8(0xE5);
                            self.code.emit_u8(port);
                        } else {
                            self.code.emit_u8(0xED);
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("IoIn width {:?}", width),
                        });
                    }
                }
            }

            OpKind::IoOut { port, value, width } => {
                let value_reg = self.get_reg(*value)?;
                if value_reg != PhysReg::Rax {
                    return Err(LowerError::InvalidOperand {
                        op: "IoOut".to_string(),
                        operand: "value must be RAX".to_string(),
                    });
                }

                let imm_port = if let VReg::Imm(val) = port {
                    if *val < 0 || *val > u8::MAX as i64 {
                        return Err(LowerError::InvalidOperand {
                            op: "IoOut".to_string(),
                            operand: "port immediate out of range".to_string(),
                        });
                    }
                    Some(*val as u8)
                } else {
                    None
                };

                if imm_port.is_none() {
                    let port_reg = self.get_reg(*port)?;
                    if port_reg != PhysReg::Rdx {
                        return Err(LowerError::InvalidOperand {
                            op: "IoOut".to_string(),
                            operand: "port must be DX".to_string(),
                        });
                    }
                }

                match width {
                    MemWidth::B1 => {
                        if let Some(port) = imm_port {
                            self.code.emit_u8(0xE6);
                            self.code.emit_u8(port);
                        } else {
                            self.code.emit_u8(0xEE);
                        }
                    }
                    MemWidth::B2 => {
                        self.code.emit_u8(0x66);
                        if let Some(port) = imm_port {
                            self.code.emit_u8(0xE7);
                            self.code.emit_u8(port);
                        } else {
                            self.code.emit_u8(0xEF);
                        }
                    }
                    MemWidth::B4 => {
                        if let Some(port) = imm_port {
                            self.code.emit_u8(0xE7);
                            self.code.emit_u8(port);
                        } else {
                            self.code.emit_u8(0xEF);
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("IoOut width {:?}", width),
                        });
                    }
                }
            }

            // ================================================================
            // Extensions
            // ================================================================
            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                if x86_state_backed_gpr_extend_candidate(op) {
                    if !x86_state_backed_gpr_extend_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed MOVZX".to_string(),
                            operand: format!(
                                "invalid x86 GPR extension {from_width:?}->{to_width:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_extend(
                        *dst,
                        *src,
                        *from_width,
                        *to_width,
                        false,
                        matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)),
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    Self::ensure_legacy_high_byte_movx_shape(
                        "MOVZX",
                        src_reg,
                        *from_width,
                        *to_width,
                    )?;
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_push(src_reg);
                    emitter.emit_movzx_rm_disp(
                        dst_reg,
                        PhysReg::Rsp,
                        1,
                        DispSize::Auto,
                        *from_width,
                        *to_width,
                    );
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8);
                } else {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    if *from_width == OpWidth::W32 && *to_width == OpWidth::W64 {
                        // 32-bit mov automatically zero-extends
                        emitter.emit_mov_rr(dst_reg, src_reg, OpWidth::W32);
                    } else {
                        emitter.emit_movzx(dst_reg, src_reg, *from_width, *to_width);
                    }
                }
            }

            OpKind::SignExtend {
                dst,
                src,
                from_width,
                to_width,
            } => {
                if x86_state_backed_gpr_extend_candidate(op) {
                    if !x86_state_backed_gpr_extend_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed MOVSX".to_string(),
                            operand: format!(
                                "invalid x86 GPR extension {from_width:?}->{to_width:?}"
                            ),
                        });
                    }
                    return self.lower_state_backed_gpr_extend(
                        *dst,
                        *src,
                        *from_width,
                        *to_width,
                        true,
                        matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)),
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;

                if matches!(op.x86_hint, Some(X86OpHint::LegacyHighByteReg)) {
                    Self::ensure_legacy_high_byte_movx_shape(
                        "MOVSX",
                        src_reg,
                        *from_width,
                        *to_width,
                    )?;
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_push(src_reg);
                    emitter.emit_movsx_rm_disp(
                        dst_reg,
                        PhysReg::Rsp,
                        1,
                        DispSize::Auto,
                        *from_width,
                        *to_width,
                    );
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8);
                } else {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_movsx(dst_reg, src_reg, *from_width, *to_width);
                }
            }

            OpKind::Cwd { dst, src, width } => {
                if !matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Rax)))
                    || !matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Rdx)))
                {
                    return Err(LowerError::InvalidOperand {
                        op: "Cwd".to_string(),
                        operand: "requires RAX/RDX".to_string(),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                match width {
                    OpWidth::W16 => emitter.emit_cwd(),
                    OpWidth::W32 => emitter.emit_cdq(),
                    OpWidth::W64 => emitter.emit_cqo(),
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Cwd width {:?}", width),
                        });
                    }
                }
            }

            OpKind::X86X87Control { kind, addr }
                if matches!(
                    kind,
                    X86X87ControlKind::EnterMmx | X86X87ControlKind::EmptyMmx
                ) =>
            {
                let (name, tag_word) = match kind {
                    X86X87ControlKind::EnterMmx => ("EnterMmx", 0),
                    X86X87ControlKind::EmptyMmx => ("EmptyMmx", 0xFFFF),
                    _ => unreachable!(),
                };
                if addr.is_some() {
                    return Err(LowerError::InvalidOperand {
                        op: format!("X86X87Control {name}"),
                        operand: "must not have a memory address".to_string(),
                    });
                }
                // Preserve architectural RAX and RFLAGS while committing the
                // guest tag word at this exact post-instruction point.
                self.code.emit_u8(0x50); // push rax
                self.code.emit_bytes(&[
                    0x48,
                    0x8B,
                    0x45,
                    X86_STATE_PTR_AT_RBP as u8, // mov rax,[rbp+state]
                    0x48,
                    0xC7,
                    0x80, // mov qword ptr [rax+disp32],imm32
                ]);
                self.code.emit_u32(X86_GUEST_X87_TAG_WORD_OFFSET as u32);
                self.code.emit_u32(tag_word);
                self.code.emit_u8(0x58); // pop rax
            }

            // ================================================================
            // Misc
            // ================================================================
            OpKind::Fence { kind } => match kind {
                FenceKind::LoadLoad => self.code.emit_bytes(&[0x0F, 0xAE, 0xE8]),
                FenceKind::Full => self.code.emit_bytes(&[0x0F, 0xAE, 0xF0]),
                FenceKind::StoreStore => self.code.emit_bytes(&[0x0F, 0xAE, 0xF8]),
                other => {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("x86 native fence {other:?}"),
                    });
                }
            },

            OpKind::Nop => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_nop();
            }

            OpKind::Breakpoint => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_int3();
            }

            OpKind::Leave => {
                self.code.emit_u8(0xC9);
            }

            OpKind::Undefined { .. } => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_ud2();
            }

            // Unimplemented ops
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("{:?}", op.kind),
                });
            }
        }

        Ok(())
    }
}
