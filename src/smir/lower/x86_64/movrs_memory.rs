//! Fault-precise helper-backed lowering for legacy high-byte MOVRS.

use super::*;
use std::collections::HashMap;

impl X86_64Lowerer {
    /// Lower a MOVRS load to guest RSP/RBP through the helper's canonical
    /// `GuestRegs` destination path rather than mapping it onto host RSP/RBP.
    pub(crate) fn try_lower_jit_movrs_state_backed(
        &mut self,
        block: &SmirBlock,
        idx: usize,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) =
            crate::smir::lower::runtime::x86_jit_movrs_state_backed_load_sequence_len(
                block,
                idx,
                true,
                self.x86_instruction_bytes
                    .get(&(block.id, block.ops[idx].guest_pc)),
            )
        else {
            return Ok(None);
        };
        let load = &block.ops[idx];
        let (dst, addr, width, sign) = match &load.kind {
            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => (*dst, addr, *width, *sign),
            _ => unreachable!("validated state-backed MOVRS is a Load"),
        };
        self.emit_jit_mem_op(
            load.guest_pc,
            true,
            Some(dst),
            None,
            None,
            None,
            None,
            addr,
            width,
            sign,
            0,
        )?;
        if Self::x86_gpr_index(dst) == Some(5) {
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
            }
            self.emit_sync_saved_rbp_from_state(PhysReg::Rcx);
            self.emit_reload_all(PhysReg::Rcx);
        }
        Ok(Some(consumed))
    }

    /// Fuse the exact `MOVRS AH/CH/DH/BH,m8` load-and-merge sequence. The MMU
    /// helper stages its byte on the native stack; the architectural parent is
    /// changed only after a successful read, and host status flags are restored
    /// after the merge.
    pub(crate) fn try_lower_jit_movrs_high_byte(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(consumed) = crate::smir::lower::runtime::x86_jit_movrs_high_byte_sequence_len(
            block,
            idx,
            true,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };

        let load = &block.ops[idx];
        let addr = match &load.kind {
            OpKind::Load { addr, .. } => addr,
            _ => unreachable!("validated high-byte MOVRS starts with Load"),
        };
        let parent = match block.ops[idx + 3].kind {
            OpKind::And { src1, .. } => src1,
            _ => unreachable!("validated high-byte MOVRS preserves its parent"),
        };
        let destination = self.get_dst_reg(parent)?;
        let scratch = if destination == PhysReg::Rax {
            PhysReg::Rdx
        } else {
            PhysReg::Rax
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
            MemWidth::B1,
            SignExtend::Zero,
            16,
        )?;

        self.code.emit_u8(0x9C); // pushfq
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_push(scratch);
            // Two pushes place the helper result 16 bytes above RSP.
            emitter.emit_mov_rm(scratch, PhysReg::Rsp, 16, OpWidth::W64);
            emitter.emit_shl_ri(scratch, 8, OpWidth::W64);
            emitter.emit_and_ri(scratch, 0xFF00, OpWidth::W64);
            emitter.emit_and_ri(destination, !0xFF00_u64 as i64, OpWidth::W64);
            emitter.emit_or_rr(destination, scratch, OpWidth::W64);
            emitter.emit_pop(scratch);
        }
        self.code.emit_u8(0x9D); // popfq
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(Some(consumed))
    }
}
