//! Fault-precise helper-backed lowering for scalar MOVBE memory operands.

use super::*;
use crate::smir::lower::runtime::X86JitMovbeMemoryDirection;
use std::collections::HashMap;

impl X86_64Lowerer {
    fn emit_jit_movbe_swap(&mut self, register: PhysReg, width: OpWidth) {
        match width {
            OpWidth::W16 => {
                self.code.emit_u8(0x9C); // pushfq: ROL defines status flags
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_rol_ri(register, 8, OpWidth::W16);
                self.code.emit_u8(0x9D); // popfq
            }
            OpWidth::W32 | OpWidth::W64 => {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_bswap(register, width);
            }
            _ => unreachable!("validated MOVBE width"),
        }
    }

    fn lower_jit_movbe_load(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let load = &block.ops[idx];
        let (addr, mem_width) = match &load.kind {
            OpKind::Load {
                addr,
                width,
                sign: SignExtend::Zero,
                ..
            } => (addr, *width),
            _ => unreachable!("validated MOVBE load sequence"),
        };
        let dst = match block.ops[idx + 1].kind {
            OpKind::Bswap { dst, .. } => dst,
            _ => unreachable!("validated MOVBE load consumer"),
        };
        let dst_index = Self::x86_gpr_index(dst).expect("validated MOVBE x86 GPR destination");

        // The helper writes the source into one caller-owned word; the second
        // word keeps the call aligned. A fault removes the complete frame and
        // exits before the architectural destination is changed.
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

        if dst_index <= 15 && !matches!(dst_index, 4 | 5) {
            let dst_reg = self.get_dst_reg(dst)?;
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(dst_reg, PhysReg::Rsp, 0, width);
            }
            self.emit_jit_movbe_swap(dst_reg, width);
        } else {
            // Guest RSP/RBP and EGPRs are state-backed. RAX holds the state
            // pointer and RDX the staged result while both guest values remain
            // preserved on the native stack.
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rax);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rsp, 16, width);
            }
            self.emit_jit_movbe_swap(PhysReg::Rdx, width);
            self.emit_store_gpr_slot_from_reg(dst_index, PhysReg::Rdx, width)?;
            if dst_index == 5 {
                let commit_width = if width == OpWidth::W16 {
                    OpWidth::W16
                } else {
                    OpWidth::W64
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
            }
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_pop(PhysReg::Rdx);
                emitter.emit_pop(PhysReg::Rax);
            }
        }

        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        Ok(())
    }

    fn lower_jit_movbe_store(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        width: OpWidth,
    ) -> Result<(), LowerError> {
        let (src, guest_pc) = match block.ops[idx].kind {
            OpKind::Bswap { src, .. } => (src, block.ops[idx].guest_pc),
            _ => unreachable!("validated MOVBE store producer"),
        };
        let (addr, mem_width) = match &block.ops[idx + 1].kind {
            OpKind::Store { addr, width, .. } => (addr, *width),
            _ => unreachable!("validated MOVBE store sequence"),
        };
        let src_index = Self::x86_gpr_index(src).expect("validated MOVBE x86 GPR source");

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        if src_index <= 15 && !matches!(src_index, 4 | 5) {
            let src_reg = self.get_reg(src)?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rsp, 0, src_reg, OpWidth::W64);
        } else {
            // State-backed sources cannot be represented by the host identity
            // map. Preserve guest RAX while staging the coherent state slot.
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rax);
            }
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(
                    PhysReg::Rax,
                    PhysReg::Rax,
                    i32::from(src_index) * 8,
                    OpWidth::W64,
                );
                emitter.emit_mov_mr(PhysReg::Rsp, 8, PhysReg::Rax, OpWidth::W64);
                emitter.emit_pop(PhysReg::Rax);
            }
        }

        // Compute only into the caller-owned word. A later store fault cannot
        // expose the swapped value through the architectural source register.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(PhysReg::Rax);
            emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rsp, 8, width);
        }
        self.emit_jit_movbe_swap(PhysReg::Rax, width);
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_mr(PhysReg::Rsp, 8, PhysReg::Rax, OpWidth::W64);
            emitter.emit_pop(PhysReg::Rax);
        }

        self.emit_jit_mem_op(
            guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(16),
            addr,
            mem_width,
            SignExtend::Zero,
            16,
        )?;
        let mut emitter = X86Emitter::new(&mut self.code);
        emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        Ok(())
    }

    pub(crate) fn try_lower_jit_movbe_memory(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_movbe_memory_sequence(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        match sequence.direction {
            X86JitMovbeMemoryDirection::Load => {
                self.lower_jit_movbe_load(block, idx, sequence.width)?
            }
            X86JitMovbeMemoryDirection::Store => {
                self.lower_jit_movbe_store(block, idx, sequence.width)?
            }
        }
        Ok(Some(sequence.consumed))
    }
}
