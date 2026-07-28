//! Helper-backed x86 lowering for AMD TBM memory-source operations.

use std::collections::HashMap;

use super::*;

impl X86_64Lowerer {
    /// Fuse the exact `Load` plus XOP TBM scalar consumer recognized by
    /// `x86_jit_mem_tbm_source_sequence_len`.
    ///
    /// A successful MMU helper load leaves its zero-extended result in a
    /// 16-byte caller-owned stack frame. Ordinary identity-mapped destinations
    /// consume that value through their dead incoming host register. Guest
    /// RSP/RBP destinations compute through RDX, commit to `GuestRegs`, then
    /// reload the complete identity map. A load fault exits at the load's guest
    /// PC before destination or flags can change.
    pub(crate) fn try_lower_jit_mem_tbm_source(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) = crate::smir::lower::runtime::x86_jit_mem_tbm_source_sequence_len(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        let load = &block.ops[idx];
        let (addr, mem_width) = match &load.kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } => (addr, *width),
            _ => unreachable!("validated TBM memory sequence starts with Load"),
        };
        let consumer = &block.ops[idx + 1].kind;
        let (dst, width) = match consumer {
            OpKind::X86Tbm { dst, width, .. } | OpKind::Bextr { dst, width, .. } => (*dst, *width),
            _ => unreachable!("validated TBM memory sequence has exact consumer"),
        };
        let dst_index = Self::x86_gpr_index(dst)
            .expect("validated TBM memory destination is an architectural GPR");

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        self.emit_jit_mem_op(
            load.guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            addr,
            mem_width,
            SignExtend::Zero,
            16,
        )?;

        let state_backed = matches!(dst_index, 4 | 5);
        let result_reg = if state_backed {
            PhysReg::Rdx
        } else {
            self.get_dst_reg(dst)?
        };
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(result_reg, PhysReg::Rsp, 0, width);
        }
        match consumer {
            OpKind::X86Tbm { kind, flags, .. } => {
                let defined_rflags_mask = match flags {
                    FlagUpdate::None => None,
                    FlagUpdate::Specific(_) => Some(0x8C1),
                    _ => unreachable!("validated TBM flag contract"),
                };
                self.emit_x86_tbm_regs(result_reg, result_reg, width, *kind, defined_rflags_mask);
            }
            OpKind::Bextr {
                control: VReg::Imm(control),
                flags,
                ..
            } => {
                let defined_rflags_mask = match flags {
                    FlagUpdate::None => None,
                    FlagUpdate::Specific(_) => Some(0x841),
                    _ => unreachable!("validated immediate BEXTR flag contract"),
                };
                self.emit_x86_bextr_imm_regs(
                    result_reg,
                    result_reg,
                    *control,
                    width,
                    defined_rflags_mask,
                )?;
            }
            _ => unreachable!("validated TBM memory consumer"),
        }

        if state_backed {
            self.code.emit_u8(0x50); // preserve guest RAX while loading state
            self.emit_load_state_ptr_rax();
            self.emit_store_gpr_slot_from_reg(dst_index, result_reg, width)?;
            if dst_index == 5 {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, result_reg, OpWidth::W64);
            }
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
            }
            self.emit_reload_all(PhysReg::Rcx);
            self.emit_flag_preserving_stack_pop8();
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(Some(consumed))
    }
}
