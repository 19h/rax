//! Data-movement lowering

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
    fn lower_lea_width(
        &mut self,
        dst: VReg,
        addr: &Address,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        if !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
            return Err(LowerError::InvalidOperand {
                op: "Lea".to_string(),
                operand: format!("unsupported destination width {width:?}"),
            });
        }
        let dst_reg = self.get_dst_reg(dst)?;

        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rr(dst_reg, base_reg, width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea_disp_width(dst_reg, base_reg, *offset as i32, *disp_size, width);
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
                    Some(base) => Some(self.get_reg(*base)?),
                    None => None,
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea_sib_disp_width(
                    dst_reg, base_phys, index_reg, *scale, *disp, *disp_size, width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                if self.guest_pcrel_lea_immediates {
                    if let Some(base_pc) = base {
                        let target = base_pc.wrapping_add_signed(*offset);
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_ri(dst_reg, target as i64, width);
                        return Ok(());
                    }
                }

                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea_pcrel_width(dst_reg, 0, width)
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
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_ri(dst_reg, *addr as i64, width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("Lea with {addr:?} address"),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn lower_op_data_movement(
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

            OpKind::Lea { dst, addr } => self.lower_lea_width(*dst, addr, OpWidth::W64)?,
            OpKind::X86Lea { dst, addr, width } => {
                if x86_state_backed_gpr_lea_candidate(op) {
                    if !x86_state_backed_gpr_lea_valid(op) {
                        return Err(LowerError::InvalidOperand {
                            op: "state-backed LEA".to_string(),
                            operand: format!("invalid x86 GPR effective address {width:?}"),
                        });
                    }
                    return self.lower_state_backed_gpr_lea(*dst, addr, *width);
                }
                self.lower_lea_width(*dst, addr, *width)?;
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
                if !matches!(
                    width,
                    OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
                ) {
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

            _ => return self.lower_op_integer_arithmetic(op),
        }

        Ok(())
    }
}
