//! Helper-backed x86 scalar multiply lowering.

use std::collections::HashMap;

use super::*;

impl X86_64Lowerer {
    /// Fuse the x86 lifter's exact implicit widening `MUL/IMUL r/m` pair. The
    /// helper stages the source in a 16-byte aligned caller frame and restores
    /// the original architectural `RAX:RDX` before the native group-3
    /// instruction commits either result register.
    pub(crate) fn try_lower_jit_mem_widening_mul_source(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) =
            crate::smir::lower::runtime::x86_jit_mem_widening_mul_source_sequence_len(
                block,
                idx,
                true,
                virtual_definitions,
                virtual_uses,
            )
        else {
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
            _ => unreachable!("validated widening multiply starts with Load"),
        };
        let width = mem_width
            .to_op_width()
            .expect("validated widening multiply has an integer width");
        let (digit, flags) = match &block.ops[idx + 1].kind {
            OpKind::MulU { flags, .. } => (4, *flags),
            OpKind::MulS { flags, .. } => (5, *flags),
            _ => unreachable!("validated widening multiply consumer"),
        };

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

        let preserve_flags = flags == FlagUpdate::None;
        if preserve_flags {
            self.code.emit_u8(0x9C); // pushfq
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_group3_m_disp(
                digit,
                PhysReg::Rsp,
                if preserve_flags { 8 } else { 0 },
                DispSize::Auto,
                width,
            );
        }
        if preserve_flags {
            self.code.emit_u8(0x9D); // popfq
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(Some(consumed))
    }

    /// Fuse a VEX/APX memory-source `MULX`. The helper performs the complete
    /// architectural read before either destination changes. On success, safe
    /// legacy destinations use the original VEX memory form directly; guest
    /// RSP/RBP and APX EGPR destinations commit through the canonical state
    /// file in architectural low-then-high assignment order.
    pub(crate) fn try_lower_jit_mem_mulx_source(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) = crate::smir::lower::runtime::x86_jit_mem_mulx_source_sequence_len(
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
            _ => unreachable!("validated MULX pair starts with Load"),
        };
        let (dst_lo, dst_hi, width) = match &block.ops[idx + 1].kind {
            OpKind::MulU {
                dst_lo,
                dst_hi: Some(dst_hi),
                width,
                ..
            } => (*dst_lo, *dst_hi, *width),
            _ => unreachable!("validated MULX pair has exact consumer"),
        };
        let dst_lo_index =
            Self::x86_gpr_index(dst_lo).expect("validated MULX low destination is a GPR");
        let dst_hi_index =
            Self::x86_gpr_index(dst_hi).expect("validated MULX high destination is a GPR");

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

        let direct_destination = |index: u8| index < 16 && !matches!(index, 4 | 5);
        if direct_destination(dst_lo_index) && direct_destination(dst_hi_index) {
            let dst_lo_reg = self.get_dst_reg(dst_lo)?;
            let dst_hi_reg = self.get_dst_reg(dst_hi)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_vex_bmi_rm_disp_pp(
                0xF6,
                X86SsePrefix::Repne,
                dst_hi_reg,
                PhysReg::Rsp,
                0,
                dst_lo_reg,
                width,
            );
        } else {
            // The helper's successful path has already restored every live GPR
            // from a canonical GuestRegs snapshot. Use that snapshot directly;
            // no second spill is needed before scratch-register execution.
            self.code.emit_u8(0x50); // preserve guest RAX; source moves to [rsp+8]
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, 2 * 8, width);
                emitter.emit_mov_rm(PhysReg::R8, PhysReg::Rsp, 8, width);
                emitter.emit_vex_bmi_rr_pp(
                    0xF6,
                    X86SsePrefix::Repne,
                    PhysReg::Rdi,
                    PhysReg::R8,
                    PhysReg::Rcx,
                    width,
                );
            }

            self.emit_store_gpr_slot_from_reg(dst_lo_index, PhysReg::Rcx, width)?;
            if dst_lo_index == 5 {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rcx, OpWidth::W64);
            }
            self.emit_store_gpr_slot_from_reg(dst_hi_index, PhysReg::Rdi, width)?;
            if dst_hi_index == 5 {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdi, OpWidth::W64);
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
