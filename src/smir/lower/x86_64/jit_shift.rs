//! Helper-backed x86 VEX/APX BMI2 memory-source shift lowering.

use std::collections::HashMap;

use super::*;

impl X86_64Lowerer {
    /// Fuse the exact memory-source `SHLX`/`SHRX`/`SARX` pair emitted by the
    /// VEX/APX BMI2 lifter. The load helper stages a zero-extended scalar in a
    /// 16-byte caller-owned stack frame and restores every guest register
    /// before this method executes the shift.
    ///
    /// Safe legacy destination/count pairs use the restored identity register
    /// map directly. Guest RSP/RBP operands instead use the canonical
    /// `GuestRegs` snapshot because host RSP owns the native stack and host RBP
    /// owns the trampoline frame. Both paths execute a classic host variable
    /// shift under PUSHFQ/POPFQ, matching the existing register-form lowering
    /// without requiring a host BMI2 feature. A load fault returns at the load
    /// helper's precise guest frontier before the destination or flags change.
    pub(crate) fn try_lower_jit_mem_bmi2_shift_source(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) =
            crate::smir::lower::runtime::x86_jit_mem_bmi2_shift_source_sequence_len(
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
            _ => unreachable!("validated BMI2 memory shift starts with Load"),
        };
        let (dst, count, width, kind) = match &block.ops[idx + 1].kind {
            OpKind::Shl {
                dst,
                amount: SrcOperand::Reg(count),
                width,
                ..
            } => (*dst, *count, *width, ShiftRegOp::Shl),
            OpKind::Shr {
                dst,
                amount: SrcOperand::Reg(count),
                width,
                ..
            } => (*dst, *count, *width, ShiftRegOp::Shr),
            OpKind::Sar {
                dst,
                amount: SrcOperand::Reg(count),
                width,
                ..
            } => (*dst, *count, *width, ShiftRegOp::Sar),
            _ => unreachable!("validated BMI2 memory shift has exact consumer"),
        };
        let dst_index =
            Self::x86_gpr_index(dst).expect("validated BMI2 shift destination is a GPR");
        let count_index = Self::x86_gpr_index(count).expect("validated BMI2 shift count is a GPR");

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

        let state_backed = |index: u8| index >= 16 || matches!(index, 4 | 5);
        if !state_backed(dst_index) && !state_backed(count_index) {
            let dst_reg = self.get_dst_reg(dst)?;
            let count_reg = self.get_reg(count)?;

            self.code.emit_u8(0x9C); // pushfq: preserve all incoming status flags
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                let source_disp = if count_reg == PhysReg::Rcx {
                    8
                } else {
                    emitter.emit_push(PhysReg::Rcx);
                    emitter.emit_mov_rr(PhysReg::Rcx, count_reg, OpWidth::W64);
                    16
                };
                emitter.emit_shift_m_disp(
                    kind.digit(),
                    PhysReg::Rsp,
                    source_disp,
                    DispSize::Auto,
                    width,
                    ShiftCount::Cl,
                );
                if count_reg != PhysReg::Rcx {
                    emitter.emit_pop(PhysReg::Rcx);
                }
            }
            self.code.emit_u8(0x9D); // popfq
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(dst_reg, PhysReg::Rsp, 0, width);
        } else {
            // The helper's successful path already populated the canonical
            // GuestRegs snapshot. Read the count before committing a possibly
            // aliasing destination, then restore the complete identity map.
            self.code.emit_u8(0x50); // preserve guest RAX; source moves to [rsp+8]
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(
                    PhysReg::Rcx,
                    PhysReg::Rax,
                    i32::from(count_index) * 8,
                    OpWidth::W64,
                );
            }
            self.code.emit_u8(0x9C); // pushfq; source is now at [rsp+16]
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_shift_m_disp(
                    kind.digit(),
                    PhysReg::Rsp,
                    16,
                    DispSize::Auto,
                    width,
                    ShiftCount::Cl,
                );
            }
            self.code.emit_u8(0x9D); // popfq
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rsp, 8, width);
            }

            self.emit_store_gpr_slot_from_reg(dst_index, PhysReg::Rdx, width)?;
            if dst_index == 5 {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
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
