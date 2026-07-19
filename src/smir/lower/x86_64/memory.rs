//! Memory-operand lowering and native-stack safety

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
    /// Enable lowering `Load`/`Store` as MMU helper calls (see `mem_helpers`).
    pub fn set_mem_helpers(&mut self, on: bool) {
        self.mem_helpers = on;
    }

    pub fn set_preserve_vector_mem_helpers(&mut self, on: bool) {
        self.preserve_vector_mem_helpers = on;
    }

    pub(crate) fn native_stack_dst(vreg: VReg) -> Option<X86Reg> {
        match vreg {
            VReg::Arch(ArchReg::X86(reg @ (X86Reg::Rsp | X86Reg::Rbp))) => Some(reg),
            _ => None,
        }
    }

    pub(crate) fn ensure_native_stack_dst_safe(vreg: VReg) -> Result<(), LowerError> {
        if let Some(reg) = Self::native_stack_dst(vreg) {
            return Err(LowerError::InvalidRegister(format!(
                "guest {reg:?} cannot be a native lowerer destination"
            )));
        }
        Ok(())
    }

    pub(crate) fn ensure_native_stack_dests_safe(op: &SmirOp) -> Result<(), LowerError> {
        if Self::mov_touches_state_backed_gpr(&op.kind)
            || Self::alu_touches_state_backed_stack_gpr(&op.kind)
            || x86_state_backed_gpr_extend_valid(op)
            || x86_state_backed_gpr_cmove_valid(op)
            || x86_state_backed_gpr_setcc_valid(op)
            || x86_state_backed_gpr_not_valid(op)
            || x86_state_backed_gpr_neg_valid(op)
            || x86_state_backed_gpr_inc_dec_valid(op)
            || x86_state_backed_gpr_rotate_valid(op)
            || x86_state_backed_gpr_shift_valid(op)
            || x86_state_backed_gpr_carry_rotate_valid(op)
            || x86_state_backed_gpr_double_shift_valid(op)
            || x86_state_backed_gpr_count_valid(op)
            || x86_state_backed_gpr_bit_scan_valid(op)
            || x86_state_backed_gpr_bit_test_valid(op)
            || x86_state_backed_gpr_crc32_valid(op)
            || x86_state_backed_gpr_and_not_valid(op)
            || x86_state_backed_gpr_bextr_bzhi_valid(op)
            || x86_state_backed_gpr_bls_valid(op)
            || x86_state_backed_gpr_adx_valid(op)
            || x86_state_backed_gpr_pdep_pext_valid(op)
            || x86_state_backed_gpr_bswap_valid(op)
            || x86_state_backed_gpr_xchg_valid(op)
            || x86_fsgsbase_shape_valid(&op.kind)
        {
            return Ok(());
        }
        for dst in op.kind.dests() {
            Self::ensure_native_stack_dst_safe(dst)?;
        }
        Ok(())
    }

    pub(crate) fn ensure_native_stack_memory_safe(
        op: &SmirOp,
        mem_helpers: bool,
    ) -> Result<(), LowerError> {
        if mem_helpers {
            return Ok(());
        }
        let address = match &op.kind {
            OpKind::Load { addr, .. } | OpKind::Store { addr, .. } => addr,
            _ => return Ok(()),
        };
        if let Some(reg) = address.regs().into_iter().find_map(Self::native_stack_dst) {
            return Err(LowerError::InvalidRegister(format!(
                "guest {reg:?} cannot address native memory without MMU helpers"
            )));
        }
        Ok(())
    }

    pub(crate) fn ensure_count_native_stack_safe(
        op: &'static str,
        dst_reg: PhysReg,
        src_reg: PhysReg,
    ) -> Result<(), LowerError> {
        if matches!(dst_reg, PhysReg::Rsp | PhysReg::Rbp)
            || matches!(src_reg, PhysReg::Rsp | PhysReg::Rbp)
        {
            return Err(LowerError::InvalidOperand {
                op: op.to_string(),
                operand: "RSP/RBP operands are not safe with flag-preserving count lowering"
                    .to_string(),
            });
        }

        Ok(())
    }

    pub(crate) fn emit_predicated_memory_guard(
        &mut self,
        op: &'static str,
        cond: VReg,
        addr: &Address,
        src: Option<&SrcOperand>,
    ) -> Result<usize, LowerError> {
        let cond_reg = self.get_reg(cond)?;
        let mut regs = vec![cond_reg];
        for reg in addr.regs() {
            regs.push(self.get_reg(reg)?);
        }
        if let Some(SrcOperand::Reg(src)) = src {
            regs.push(self.get_reg(*src)?);
        }
        Self::ensure_flag_stack_operands_safe(op, &regs)?;

        self.code.emit_u8(0x9C); // pushfq
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_test_ri(cond_reg, 1, OpWidth::W64);
        }
        Ok(self.emit_jcc_placeholder(X86Cond::E))
    }

    pub(crate) fn emit_sse_mov_mem(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        addr: &Address,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_mov_rm_disp(prefix, opcode, reg, base_reg, 0, DispSize::Auto);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_mov_rm_disp(
                    prefix,
                    opcode,
                    reg,
                    base_reg,
                    *offset as i32,
                    *disp_size,
                );
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
                emitter.emit_sse_mov_rm_sib_disp(
                    prefix, opcode, reg, base_reg, index_reg, *scale, *disp, *disp_size,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rm_pcrel(prefix, opcode, reg, 0)
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
                            op: "PcRel SSE mov".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel SSE mov".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_mov_rm_abs(prefix, opcode, reg, *addr);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("SSE mov with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_sse_op38_mem(
        &mut self,
        prefix: Option<u8>,
        opcode: u8,
        reg: PhysReg,
        addr: &Address,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_op38_rm_disp(prefix, opcode, reg, base_reg, 0, DispSize::Auto);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_op38_rm_disp(
                    prefix,
                    opcode,
                    reg,
                    base_reg,
                    *offset as i32,
                    *disp_size,
                );
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
                emitter.emit_sse_op38_rm_sib_disp(
                    prefix, opcode, reg, base_reg, index_reg, *scale, *disp, *disp_size,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_op38_rm_pcrel(prefix, opcode, reg, 0)
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
                            op: "PcRel SSE 0F 38".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel SSE 0F 38".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_sse_op38_rm_abs(prefix, opcode, reg, *addr);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("SSE 0F 38 with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_vec_mem(
        &mut self,
        encoding: VecEncoding,
        reg: PhysReg,
        vvvv_reg: Option<PhysReg>,
        addr: &Address,
    ) -> Result<(), LowerError> {
        let encoding = match vvvv_reg {
            Some(vreg) => self.coerce_vec_encoding(encoding, &[reg, vreg]),
            None => self.coerce_vec_encoding(encoding, &[reg]),
        };
        // The emitter accepts the logical register number and performs the
        // architectural VEX/EVEX inversion. Reserved vvvv=1111b encodes from
        // logical zero, not from register 31.
        let vvvv = vvvv_reg.map_or(0, |vreg| vreg.encoding() & 0x1F);
        let r = reg.vec_ext();
        let r2 = reg.vec_ext2();
        let w = encoding.w;

        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let b = base_reg.vec_ext();
                let b2 = base_reg.vec_ext2();
                let mut emitter = X86Emitter::new(&mut self.code);
                match encoding.kind {
                    VecEncodingKind::Vex => {
                        emitter.emit_vex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            0,
                            b,
                            vvvv,
                        );
                    }
                    VecEncodingKind::Evex => {
                        emitter.emit_evex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            0,
                            b,
                            r2,
                            0,
                            b2,
                            vvvv,
                        );
                    }
                }
                emitter.code.emit_u8(encoding.opcode);
                emitter.emit_modrm_mem_disp(reg, base_reg, 0, DispSize::Auto);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let b = base_reg.vec_ext();
                let b2 = base_reg.vec_ext2();
                let mut emitter = X86Emitter::new(&mut self.code);
                match encoding.kind {
                    VecEncodingKind::Vex => {
                        emitter.emit_vex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            0,
                            b,
                            vvvv,
                        );
                    }
                    VecEncodingKind::Evex => {
                        emitter.emit_evex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            0,
                            b,
                            r2,
                            0,
                            b2,
                            vvvv,
                        );
                    }
                }
                emitter.code.emit_u8(encoding.opcode);
                emitter.emit_modrm_mem_disp(reg, base_reg, *offset as i32, *disp_size);
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
                let base_bits = base_reg.unwrap_or(PhysReg::Rbp);
                let b = base_bits.vec_ext();
                let b2 = base_bits.vec_ext2();
                let x = index_reg.vec_ext();
                let x2 = index_reg.vec_ext2();
                let mut emitter = X86Emitter::new(&mut self.code);
                match encoding.kind {
                    VecEncodingKind::Vex => {
                        emitter.emit_vex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            x,
                            b,
                            vvvv,
                        );
                    }
                    VecEncodingKind::Evex => {
                        emitter.emit_evex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            x,
                            b,
                            r2,
                            x2,
                            b2,
                            vvvv,
                        );
                    }
                }
                emitter.code.emit_u8(encoding.opcode);
                emitter.emit_modrm_sib_disp(reg, base_reg, index_reg, *scale, *disp, *disp_size);
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    match encoding.kind {
                        VecEncodingKind::Vex => {
                            emitter.emit_vex_prefix(
                                encoding.map,
                                encoding.pp,
                                encoding.width,
                                w,
                                r,
                                0,
                                0,
                                vvvv,
                            );
                        }
                        VecEncodingKind::Evex => {
                            emitter.emit_evex_prefix(
                                encoding.map,
                                encoding.pp,
                                encoding.width,
                                w,
                                r,
                                0,
                                0,
                                r2,
                                0,
                                0,
                                vvvv,
                            );
                        }
                    }
                    emitter.code.emit_u8(encoding.opcode);
                    emitter.emit_modrm_pcrel(reg, 0)
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
                            op: "PcRel VEX/EVEX".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel VEX/EVEX".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                match encoding.kind {
                    VecEncodingKind::Vex => {
                        emitter.emit_vex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            0,
                            0,
                            vvvv,
                        );
                    }
                    VecEncodingKind::Evex => {
                        emitter.emit_evex_prefix(
                            encoding.map,
                            encoding.pp,
                            encoding.width,
                            w,
                            r,
                            0,
                            0,
                            r2,
                            0,
                            0,
                            vvvv,
                        );
                    }
                }
                emitter.code.emit_u8(encoding.opcode);
                emitter.emit_modrm_abs(reg, *addr);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("VEX/EVEX with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_movzx_mem(
        &mut self,
        dst: PhysReg,
        addr: &Address,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_movzx_rm_disp(dst, base_reg, 0, DispSize::Auto, src_width, dst_width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_movzx_rm_disp(
                    dst,
                    base_reg,
                    *offset as i32,
                    *disp_size,
                    src_width,
                    dst_width,
                );
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
                emitter.emit_movzx_rm_sib_disp(
                    dst, base_reg, index_reg, *scale, *disp, *disp_size, src_width, dst_width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_movzx_rm_pcrel(dst, 0, src_width, dst_width)
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
                            op: "PcRel movzx".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel movzx".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_movzx_rm_abs(dst, *addr, src_width, dst_width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("Movzx with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_movsx_mem(
        &mut self,
        dst: PhysReg,
        addr: &Address,
        src_width: OpWidth,
        dst_width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_movsx_rm_disp(dst, base_reg, 0, DispSize::Auto, src_width, dst_width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_movsx_rm_disp(
                    dst,
                    base_reg,
                    *offset as i32,
                    *disp_size,
                    src_width,
                    dst_width,
                );
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
                emitter.emit_movsx_rm_sib_disp(
                    dst, base_reg, index_reg, *scale, *disp, *disp_size, src_width, dst_width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_movsx_rm_pcrel(dst, 0, src_width, dst_width)
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
                            op: "PcRel movsx".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel movsx".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_movsx_rm_abs(dst, *addr, src_width, dst_width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("Movsx with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_alu_mem_reg(
        &mut self,
        opcode: u8,
        addr: &Address,
        reg: PhysReg,
        width: OpWidth,
        encoding: X86AluEncoding,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mem_disp(
                    opcode,
                    reg,
                    base_reg,
                    0,
                    DispSize::Auto,
                    width,
                    encoding,
                );
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mem_disp(
                    opcode,
                    reg,
                    base_reg,
                    *offset as i32,
                    *disp_size,
                    width,
                    encoding,
                );
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
                emitter.emit_alu_mem_sib_disp(
                    opcode, reg, base_reg, index_reg, *scale, *disp, *disp_size, width, encoding,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_alu_mem_pcrel(opcode, reg, 0, width, encoding)
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
                            op: "PcRel ALU".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel ALU".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mem_abs(opcode, reg, *addr, width, encoding);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("ALU with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_alu_mem_imm(
        &mut self,
        digit: u8,
        addr: &Address,
        imm: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mi_disp(digit, base_reg, 0, DispSize::Auto, imm, width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mi_disp(digit, base_reg, *offset as i32, *disp_size, imm, width);
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
                emitter.emit_alu_mi_sib_disp(
                    digit, base_reg, index_reg, *scale, *disp, *disp_size, imm, width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_alu_mi_pcrel(digit, 0, imm, width)
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
                            op: "PcRel ALU imm".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel ALU imm".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_alu_mi_abs(digit, *addr, imm, width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("ALU imm with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_test_mem_reg(
        &mut self,
        addr: &Address,
        reg: PhysReg,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_mr_disp(base_reg, 0, DispSize::Auto, reg, width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_mr_disp(base_reg, *offset as i32, *disp_size, reg, width);
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
                emitter.emit_test_mr_sib_disp(
                    base_reg, index_reg, *scale, *disp, *disp_size, reg, width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_test_mr_pcrel(0, reg, width)
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
                            op: "PcRel TEST".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel TEST".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_mr_abs(*addr, reg, width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("TEST with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_test_mem_imm(
        &mut self,
        addr: &Address,
        imm: i64,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_mi_disp(base_reg, 0, DispSize::Auto, imm, width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_mi_disp(base_reg, *offset as i32, *disp_size, imm, width);
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
                emitter.emit_test_mi_sib_disp(
                    base_reg, index_reg, *scale, *disp, *disp_size, imm, width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_test_mi_pcrel(0, imm, width)
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
                            op: "PcRel TEST imm".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel TEST imm".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_mi_abs(*addr, imm, width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("TEST imm with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_group3_mem(
        &mut self,
        digit: u8,
        addr: &Address,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_group3_m_disp(digit, base_reg, 0, DispSize::Auto, width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_group3_m_disp(digit, base_reg, *offset as i32, *disp_size, width);
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
                emitter.emit_group3_m_sib_disp(
                    digit, base_reg, index_reg, *scale, *disp, *disp_size, width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_group3_m_pcrel(digit, 0, width)
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
                            op: "PcRel Group3".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel Group3".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_group3_m_abs(digit, *addr, width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("Group3 with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_shift_mem(
        &mut self,
        digit: u8,
        addr: &Address,
        width: OpWidth,
        count: ShiftCount,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shift_m_disp(digit, base_reg, 0, DispSize::Auto, width, count);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shift_m_disp(
                    digit,
                    base_reg,
                    *offset as i32,
                    *disp_size,
                    width,
                    count,
                );
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
                emitter.emit_shift_m_sib_disp(
                    digit, base_reg, index_reg, *scale, *disp, *disp_size, width, count,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_shift_m_pcrel(digit, 0, width, count)
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
                            op: "PcRel shift".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel shift".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shift_m_abs(digit, *addr, width, count);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("Shift with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_group5_mem(&mut self, digit: u8, addr: &Address) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_group5_m_disp(digit, base_reg, 0, DispSize::Auto);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_group5_m_disp(digit, base_reg, *offset as i32, *disp_size);
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
                emitter
                    .emit_group5_m_sib_disp(digit, base_reg, index_reg, *scale, *disp, *disp_size);
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_group5_m_pcrel(digit, 0)
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
                            op: "PcRel Group5".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel Group5".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_group5_m_abs(digit, *addr);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("Group5 with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_shld_mem(
        &mut self,
        addr: &Address,
        src: PhysReg,
        amount: Option<u8>,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shld_mr_disp(base_reg, 0, DispSize::Auto, src, amount, width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shld_mr_disp(base_reg, *offset as i32, *disp_size, src, amount, width);
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
                emitter.emit_shld_mr_sib_disp(
                    base_reg, index_reg, *scale, *disp, *disp_size, src, amount, width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_shld_mr_pcrel(0, src, amount, width)
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
                            op: "PcRel SHLD".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel SHLD".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shld_mr_abs(*addr, src, amount, width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("SHLD with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_shrd_mem(
        &mut self,
        addr: &Address,
        src: PhysReg,
        amount: Option<u8>,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shrd_mr_disp(base_reg, 0, DispSize::Auto, src, amount, width);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shrd_mr_disp(base_reg, *offset as i32, *disp_size, src, amount, width);
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
                emitter.emit_shrd_mr_sib_disp(
                    base_reg, index_reg, *scale, *disp, *disp_size, src, amount, width,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_shrd_mr_pcrel(0, src, amount, width)
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
                            op: "PcRel SHRD".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel SHRD".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shrd_mr_abs(*addr, src, amount, width);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("SHRD with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn emit_imul_mem_imm(
        &mut self,
        dst: PhysReg,
        addr: &Address,
        imm: i32,
        width: OpWidth,
        use_imm8: bool,
    ) -> Result<(), LowerError> {
        match addr {
            Address::Direct(base) => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_imul_rmi_disp(dst, base_reg, 0, DispSize::Auto, imm, width, use_imm8);
            }
            Address::BaseOffset {
                base,
                offset,
                disp_size,
            } => {
                let base_reg = self.get_reg(*base)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_imul_rmi_disp(
                    dst,
                    base_reg,
                    *offset as i32,
                    *disp_size,
                    imm,
                    width,
                    use_imm8,
                );
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
                emitter.emit_imul_rmi_sib_disp(
                    dst, base_reg, index_reg, *scale, *disp, *disp_size, imm, width, use_imm8,
                );
            }
            Address::PcRel { offset, base, .. } => {
                let disp_offset = {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_imul_rmi_pcrel(dst, 0, imm, width, use_imm8)
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
                            op: "PcRel IMUL".to_string(),
                            operand: "offset out of range".to_string(),
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
                            op: "PcRel IMUL".to_string(),
                            operand: "offset out of range".to_string(),
                        });
                    }
                    disp
                };

                self.code.patch_i32(disp_offset, disp as i32);
            }
            Address::Absolute(addr) => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_imul_rmi_abs(dst, *addr, imm, width, use_imm8);
            }
            _ => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("IMUL with unsupported addressing: {:?}", addr),
                });
            }
        }

        Ok(())
    }

    pub(crate) fn try_lower_mem_extend(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        let (tmp, addr, mem_width, sign) = match ops.get(idx).map(|op| &op.kind) {
            Some(OpKind::Load {
                dst,
                addr,
                width,
                sign,
            }) => (*dst, addr, *width, *sign),
            _ => return Ok(None),
        };

        let op_width = match mem_width.to_op_width() {
            Some(width) => width,
            None => return Ok(None),
        };

        let next = match ops.get(idx + 1) {
            Some(op) => op,
            None => return Ok(None),
        };

        match &next.kind {
            OpKind::ZeroExtend {
                dst,
                src,
                from_width,
                to_width,
            } if *src == tmp && *from_width == op_width && sign == SignExtend::Zero => {
                let dst_reg = self.get_dst_reg(*dst)?;
                self.emit_movzx_mem(dst_reg, addr, *from_width, *to_width)?;
                return Ok(Some(2));
            }
            OpKind::SignExtend {
                dst,
                src,
                from_width,
                to_width,
            } if *src == tmp && *from_width == op_width && sign == SignExtend::Sign => {
                let dst_reg = self.get_dst_reg(*dst)?;
                self.emit_movsx_mem(dst_reg, addr, *from_width, *to_width)?;
                return Ok(Some(2));
            }
            _ => {}
        }

        Ok(None)
    }

    pub(crate) fn try_lower_mem_shift(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        if idx + 2 >= ops.len() {
            return Ok(None);
        }

        let (tmp, addr, mem_width, sign) = match ops.get(idx).map(|op| &op.kind) {
            Some(OpKind::Load {
                dst,
                addr,
                width,
                sign,
            }) => (*dst, addr, *width, *sign),
            _ => return Ok(None),
        };

        if sign != SignExtend::Zero {
            return Ok(None);
        }

        let op_width = match mem_width.to_op_width() {
            Some(width) => width,
            None => return Ok(None),
        };

        let (digit, amount, dst, src, width) = match &ops[idx + 1].kind {
            OpKind::Rol {
                dst,
                src,
                amount,
                width,
                ..
            } => (0, amount, dst, src, width),
            OpKind::Ror {
                dst,
                src,
                amount,
                width,
                ..
            } => (1, amount, dst, src, width),
            OpKind::Rcl {
                dst,
                src,
                amount,
                width,
                ..
            } => (2, amount, dst, src, width),
            OpKind::Rcr {
                dst,
                src,
                amount,
                width,
                ..
            } => (3, amount, dst, src, width),
            OpKind::Shl {
                dst,
                src,
                amount,
                width,
                ..
            } => (4, amount, dst, src, width),
            OpKind::Shr {
                dst,
                src,
                amount,
                width,
                ..
            } => (5, amount, dst, src, width),
            OpKind::Sar {
                dst,
                src,
                amount,
                width,
                ..
            } => (7, amount, dst, src, width),
            _ => return Ok(None),
        };

        if *dst != tmp || *src != tmp || *width != op_width {
            return Ok(None);
        }

        match &ops[idx + 2].kind {
            OpKind::Store {
                src,
                addr: store_addr,
                width: store_width,
            } if *src == tmp && *store_addr == *addr && *store_width == mem_width => {}
            _ => return Ok(None),
        }

        let count = match amount {
            SrcOperand::Imm(val) => {
                if *val < 0 || *val > u8::MAX as i64 {
                    return Ok(None);
                }
                let imm = *val as u8;
                if imm == 1 {
                    ShiftCount::One
                } else {
                    ShiftCount::Imm(imm)
                }
            }
            SrcOperand::Reg(reg) => {
                let amt_reg = self.get_reg(*reg)?;
                if amt_reg != PhysReg::Rcx {
                    return Ok(None);
                }
                ShiftCount::Cl
            }
            _ => return Ok(None),
        };

        self.emit_shift_mem(digit, addr, op_width, count)?;
        Ok(Some(3))
    }

    pub(crate) fn try_lower_mem_alu(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        let (tmp, addr, mem_width, sign) = match ops.get(idx).map(|op| &op.kind) {
            Some(OpKind::Load {
                dst,
                addr,
                width,
                sign,
            }) => (*dst, addr, *width, *sign),
            _ => return Ok(None),
        };

        if sign != SignExtend::Zero {
            return Ok(None);
        }

        let op_width = match mem_width.to_op_width() {
            Some(width) => width,
            None => return Ok(None),
        };

        if idx + 2 < ops.len() {
            if let OpKind::Store {
                src,
                addr: store_addr,
                width: store_width,
            } = &ops[idx + 2].kind
            {
                if *src == tmp && *store_width == mem_width && *store_addr == *addr {
                    match &ops[idx + 1].kind {
                        OpKind::Not { dst, src, width }
                            if *dst == tmp && *src == tmp && *width == op_width =>
                        {
                            self.emit_group3_mem(2, addr, op_width)?;
                            return Ok(Some(3));
                        }
                        OpKind::Neg {
                            dst,
                            src,
                            width,
                            flags,
                        } if *dst == tmp
                            && *src == tmp
                            && *width == op_width
                            && flags.updates_any() =>
                        {
                            self.emit_group3_mem(3, addr, op_width)?;
                            return Ok(Some(3));
                        }
                        _ => {}
                    }

                    if let Some((opcode, digit, src2)) = match &ops[idx + 1].kind {
                        OpKind::Add {
                            dst,
                            src1,
                            src2,
                            width,
                            ..
                        } if *dst == tmp && *src1 == tmp && *width == op_width => {
                            Some((0x00, 0, src2))
                        }
                        OpKind::Sub {
                            dst,
                            src1,
                            src2,
                            width,
                            ..
                        } if *dst == tmp && *src1 == tmp && *width == op_width => {
                            Some((0x28, 5, src2))
                        }
                        OpKind::Adc {
                            dst,
                            src1,
                            src2,
                            width,
                            ..
                        } if *dst == tmp && *src1 == tmp && *width == op_width => {
                            Some((0x10, 2, src2))
                        }
                        OpKind::Sbb {
                            dst,
                            src1,
                            src2,
                            width,
                            ..
                        } if *dst == tmp && *src1 == tmp && *width == op_width => {
                            Some((0x18, 3, src2))
                        }
                        OpKind::And {
                            dst,
                            src1,
                            src2,
                            width,
                            ..
                        } if *dst == tmp && *src1 == tmp && *width == op_width => {
                            Some((0x20, 4, src2))
                        }
                        OpKind::Or {
                            dst,
                            src1,
                            src2,
                            width,
                            ..
                        } if *dst == tmp && *src1 == tmp && *width == op_width => {
                            Some((0x08, 1, src2))
                        }
                        OpKind::Xor {
                            dst,
                            src1,
                            src2,
                            width,
                            ..
                        } if *dst == tmp && *src1 == tmp && *width == op_width => {
                            Some((0x30, 6, src2))
                        }
                        _ => None,
                    } {
                        match src2 {
                            SrcOperand::Reg(r) => {
                                let reg = self.get_reg(*r)?;
                                self.emit_alu_mem_reg(
                                    opcode,
                                    addr,
                                    reg,
                                    op_width,
                                    X86AluEncoding::RmReg,
                                )?;
                                return Ok(Some(3));
                            }
                            SrcOperand::Imm(val) => {
                                self.emit_alu_mem_imm(digit, addr, *val, op_width)?;
                                return Ok(Some(3));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if idx + 1 < ops.len() {
            match &ops[idx + 1].kind {
                OpKind::Test { src1, src2, width } if *width == op_width && *src1 == tmp => {
                    match src2 {
                        SrcOperand::Reg(r) => {
                            let reg = self.get_reg(*r)?;
                            self.emit_test_mem_reg(addr, reg, op_width)?;
                            return Ok(Some(2));
                        }
                        SrcOperand::Imm(val) => {
                            self.emit_test_mem_imm(addr, *val, op_width)?;
                            return Ok(Some(2));
                        }
                        _ => {}
                    }
                }
                OpKind::Cmp { src1, src2, width } if *width == op_width => match (src1, src2) {
                    (s1, SrcOperand::Reg(r)) if *s1 == tmp => {
                        let reg = self.get_reg(*r)?;
                        self.emit_alu_mem_reg(0x38, addr, reg, op_width, X86AluEncoding::RmReg)?;
                        return Ok(Some(2));
                    }
                    (s1, SrcOperand::Reg(r)) if *r == tmp => {
                        let reg = self.get_reg(*s1)?;
                        self.emit_alu_mem_reg(0x38, addr, reg, op_width, X86AluEncoding::RegRm)?;
                        return Ok(Some(2));
                    }
                    (s1, SrcOperand::Imm(val)) if *s1 == tmp => {
                        self.emit_alu_mem_imm(7, addr, *val, op_width)?;
                        return Ok(Some(2));
                    }
                    _ => {}
                },
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(r),
                    width,
                    ..
                } if *width == op_width && *dst == *src1 && *r == tmp => {
                    let reg = self.get_dst_reg(*dst)?;
                    self.emit_alu_mem_reg(0x00, addr, reg, op_width, X86AluEncoding::RegRm)?;
                    return Ok(Some(2));
                }
                OpKind::Sub {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(r),
                    width,
                    ..
                } if *width == op_width && *dst == *src1 && *r == tmp => {
                    let reg = self.get_dst_reg(*dst)?;
                    self.emit_alu_mem_reg(0x28, addr, reg, op_width, X86AluEncoding::RegRm)?;
                    return Ok(Some(2));
                }
                OpKind::Adc {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(r),
                    width,
                    ..
                } if *width == op_width && *dst == *src1 && *r == tmp => {
                    let reg = self.get_dst_reg(*dst)?;
                    self.emit_alu_mem_reg(0x10, addr, reg, op_width, X86AluEncoding::RegRm)?;
                    return Ok(Some(2));
                }
                OpKind::Sbb {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(r),
                    width,
                    ..
                } if *width == op_width && *dst == *src1 && *r == tmp => {
                    let reg = self.get_dst_reg(*dst)?;
                    self.emit_alu_mem_reg(0x18, addr, reg, op_width, X86AluEncoding::RegRm)?;
                    return Ok(Some(2));
                }
                OpKind::And {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(r),
                    width,
                    ..
                } if *width == op_width && *dst == *src1 && *r == tmp => {
                    let reg = self.get_dst_reg(*dst)?;
                    self.emit_alu_mem_reg(0x20, addr, reg, op_width, X86AluEncoding::RegRm)?;
                    return Ok(Some(2));
                }
                OpKind::Or {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(r),
                    width,
                    ..
                } if *width == op_width && *dst == *src1 && *r == tmp => {
                    let reg = self.get_dst_reg(*dst)?;
                    self.emit_alu_mem_reg(0x08, addr, reg, op_width, X86AluEncoding::RegRm)?;
                    return Ok(Some(2));
                }
                OpKind::Xor {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(r),
                    width,
                    ..
                } if *width == op_width && *dst == *src1 && *r == tmp => {
                    let reg = self.get_dst_reg(*dst)?;
                    self.emit_alu_mem_reg(0x30, addr, reg, op_width, X86AluEncoding::RegRm)?;
                    return Ok(Some(2));
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub(crate) fn try_lower_mem_imul(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        let (tmp, addr, mem_width, sign) = match ops.get(idx).map(|op| &op.kind) {
            Some(OpKind::Load {
                dst,
                addr,
                width,
                sign,
            }) => (*dst, addr, *width, *sign),
            _ => return Ok(None),
        };

        if sign != SignExtend::Zero {
            return Ok(None);
        }

        let op_width = match mem_width.to_op_width() {
            Some(width) => width,
            None => return Ok(None),
        };

        let op = match ops.get(idx + 1) {
            Some(op) => op,
            None => return Ok(None),
        };

        let (dst_lo, src1, src2, width) = match &op.kind {
            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                ..
            } if dst_hi.is_none() => (*dst_lo, *src1, src2, *width),
            _ => return Ok(None),
        };

        if src1 != tmp || width != op_width {
            return Ok(None);
        }

        let imm = match src2 {
            SrcOperand::Imm(val) => *val as i32,
            _ => return Ok(None),
        };

        let dst_reg = self.get_dst_reg(dst_lo)?;
        let use_imm8 = match op.x86_hint {
            Some(X86OpHint::ImulImm8) => true,
            Some(X86OpHint::ImulImm32) => false,
            _ => imm >= -128 && imm <= 127,
        };

        self.emit_imul_mem_imm(dst_reg, addr, imm, op_width, use_imm8)?;
        Ok(Some(2))
    }

    pub(crate) fn try_lower_mem_group3(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        let (tmp, addr, mem_width, sign) = match ops.get(idx).map(|op| &op.kind) {
            Some(OpKind::Load {
                dst,
                addr,
                width,
                sign,
            }) => (*dst, addr, *width, *sign),
            _ => return Ok(None),
        };

        if sign != SignExtend::Zero {
            return Ok(None);
        }

        let op_width = match mem_width.to_op_width() {
            Some(width) => width,
            None => return Ok(None),
        };

        let op = match ops.get(idx + 1) {
            Some(op) => op,
            None => return Ok(None),
        };

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));

        match &op.kind {
            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } if *width == op_width
                && *dst_lo == rax
                && *dst_hi
                    == if op_width == OpWidth::W8 {
                        None
                    } else {
                        Some(rdx)
                    }
                && *src1 == rax
                && flags.updates_any()
                && matches!(src2, SrcOperand::Reg(r) if *r == tmp) =>
            {
                self.emit_group3_mem(4, addr, op_width)?;
                return Ok(Some(2));
            }
            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            } if *width == op_width
                && *dst_lo == rax
                && *dst_hi
                    == if op_width == OpWidth::W8 {
                        None
                    } else {
                        Some(rdx)
                    }
                && *src1 == rax
                && flags.updates_any()
                && matches!(src2, SrcOperand::Reg(r) if *r == tmp) =>
            {
                self.emit_group3_mem(5, addr, op_width)?;
                return Ok(Some(2));
            }
            OpKind::DivU {
                quot,
                rem,
                src1,
                src2,
                width,
                flags,
            } if *width == op_width
                && *quot == rax
                && *rem
                    == if op_width == OpWidth::W8 {
                        None
                    } else {
                        Some(rdx)
                    }
                && *src1 == rax
                && flags.updates_any()
                && matches!(src2, SrcOperand::Reg(r) if *r == tmp) =>
            {
                self.emit_group3_mem(6, addr, op_width)?;
                return Ok(Some(2));
            }
            OpKind::DivS {
                quot,
                rem,
                src1,
                src2,
                width,
                flags,
            } if *width == op_width
                && *quot == rax
                && *rem
                    == if op_width == OpWidth::W8 {
                        None
                    } else {
                        Some(rdx)
                    }
                && *src1 == rax
                && flags.updates_any()
                && matches!(src2, SrcOperand::Reg(r) if *r == tmp) =>
            {
                self.emit_group3_mem(7, addr, op_width)?;
                return Ok(Some(2));
            }
            _ => {}
        }

        Ok(None)
    }

    pub(crate) fn try_lower_mem_shld(
        &mut self,
        ops: &[crate::smir::ir::ops::SmirOp],
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        if idx + 2 >= ops.len() {
            return Ok(None);
        }

        let (tmp, addr, mem_width, sign) = match &ops[idx].kind {
            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => (*dst, addr, *width, *sign),
            _ => return Ok(None),
        };

        if sign != SignExtend::Zero {
            return Ok(None);
        }

        let op_width = match mem_width.to_op_width() {
            Some(width) => width,
            None => return Ok(None),
        };

        let (is_shld, src_reg, amount) = match &ops[idx + 1].kind {
            OpKind::Shld {
                dst,
                src,
                amount,
                width,
                ..
            } if *dst == tmp && *width == op_width => (true, *src, amount),
            OpKind::Shrd {
                dst,
                src,
                amount,
                width,
                ..
            } if *dst == tmp && *width == op_width => (false, *src, amount),
            _ => return Ok(None),
        };

        if let OpKind::Store {
            src,
            addr: store_addr,
            width: store_width,
        } = &ops[idx + 2].kind
        {
            if *src != tmp || *store_width != mem_width || *store_addr != *addr {
                return Ok(None);
            }
        } else {
            return Ok(None);
        }

        let src_phys = self.get_reg(src_reg)?;
        let amount_imm = match amount {
            SrcOperand::Imm(val) => Some(*val as u8),
            SrcOperand::Reg(r) => {
                let amt_reg = self.get_reg(*r)?;
                if amt_reg != PhysReg::Rcx {
                    return Ok(None);
                }
                None
            }
            _ => return Ok(None),
        };

        if is_shld {
            self.emit_shld_mem(addr, src_phys, amount_imm, op_width)?;
        } else {
            self.emit_shrd_mem(addr, src_phys, amount_imm, op_width)?;
        }

        Ok(Some(3))
    }

    /// `mov [base+off], r<reg_enc>` (store) or `mov r<reg_enc>, [base+off]` (load),
    /// REX.W, mod=10 disp32. `base` is always RAX or RCX here (rm 0/1, no SIB).
    pub(crate) fn emit_struct_mov(&mut self, base: PhysReg, reg_enc: u8, off: i32, store: bool) {
        let mut rex = 0x48u8; // REX.W
        if reg_enc >= 8 {
            rex |= 0x04; // REX.R
        }
        if base.encoding() >= 8 {
            rex |= 0x01; // REX.B (base is rax/rcx -> unused)
        }
        self.code.emit_u8(rex);
        self.code.emit_u8(if store { 0x89 } else { 0x8B });
        self.code
            .emit_u8(0x80 | ((reg_enc & 7) << 3) | (base.encoding() & 7));
        self.code.emit_u32(off as u32);
    }

    /// `add rsi, imm` (REX.W 81 /0 id) when `v` fits i32; else bail.
    pub(crate) fn emit_add_rsi_imm(&mut self, v: i64) -> Result<(), LowerError> {
        if v == 0 {
            return Ok(());
        }
        if v < i32::MIN as i64 || v > i32::MAX as i64 {
            return Err(LowerError::UnsupportedOp {
                op: "jit-mem: disp out of i32 range".to_string(),
            });
        }
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x81);
        self.code.emit_u8(0xC6); // /0, rm=rsi(6)
        self.code.emit_u32(v as u32);
        Ok(())
    }

    /// Add a wrapping signed 64-bit displacement to RSI. The generic memory
    /// helper path accepts only architectural x86 disp32 values, but a lifted
    /// FS/GS RIP-relative address may already have folded a 64-bit next-RIP
    /// into `SegmentRel::disp`; alignment evaluation must reproduce that full
    /// wrapping calculation without dereferencing the resulting host address.
    pub(crate) fn emit_add_rsi_wrapping_i64(&mut self, value: i64) {
        if value == 0 {
            return;
        }
        if i32::try_from(value).is_ok() {
            self.code.emit_u8(0x48);
            self.code.emit_u8(0x81);
            self.code.emit_u8(0xC6); // add rsi, imm32
            self.code.emit_u32(value as u32);
        } else {
            self.emit_movabs(7, value as u64); // rdi = sign-preserving displacement bits
            self.code.emit_u8(0x48);
            self.code.emit_u8(0x01);
            self.code.emit_u8(0xFE); // add rsi, rdi
        }
    }

    /// Evaluate the explicit alignment precondition emitted for aligned x86
    /// SIMD memory operands. No guest address is dereferenced here. A mismatch
    /// returns at the instruction's current PC so the interpreter can deliver
    /// #GP(0) before the following memory access; success restores all GPRs and
    /// RFLAGS bit-for-bit and continues in the native region.
    pub(crate) fn emit_x86_check_alignment(
        &mut self,
        guest_pc: u64,
        addr: &Address,
        alignment: u8,
    ) -> Result<(), LowerError> {
        if !matches!(alignment, 16 | 32 | 64) {
            return Err(LowerError::InvalidOperand {
                op: "X86CheckAlignment".to_string(),
                operand: format!("unsupported alignment {alignment}"),
            });
        }

        // Snapshot live legacy GPRs before evaluating the address. Guest RSP
        // and RBP are deliberately absent from the spill and retain their
        // frozen values in GuestRegs; EGPRs are already state-backed.
        self.code.emit_u8(0x50); // push rax
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);
        self.emit_x86_state_address_rsi(addr)?;

        self.code.emit_u8(0x48);
        self.code.emit_u8(0xF7);
        self.code.emit_u8(0xC6); // test rsi, imm32
        self.code.emit_u32(u32::from(alignment - 1));
        let fault = self.emit_jcc_placeholder(X86Cond::Ne);

        // Aligned path: restore the snapshot and continue.
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x89);
        self.code.emit_u8(0xC1); // mov rcx, rax (state pointer)
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        // Misaligned path: restore first, then hand the current instruction to
        // the interpreter. Its existing X86CheckAlignment implementation emits
        // the architecturally precise #GP(0) without committing later ops.
        self.patch_rel32_to_current(fault)?;
        self.code.emit_u8(0x48);
        self.code.emit_u8(0x89);
        self.code.emit_u8(0xC1); // mov rcx, rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(guest_pc);
        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
