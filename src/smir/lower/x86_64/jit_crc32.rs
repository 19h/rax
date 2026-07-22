//! Native x86 CRC32 fusion for memory-source SMIR pairs.

use super::*;
use std::collections::HashMap;

impl X86_64Lowerer {
    /// Fuse the x86 lifter's `Load virtual; Crc32C dst,dst,virtual` memory
    /// source into one MMU helper call followed by native CRC32. The helper
    /// stages its zero-extended result in a caller-owned host-stack slot, so a
    /// fault exits before the accumulator changes. Identity-mapped destinations
    /// consume that slot directly; guest RSP/RBP and APX EGPR destinations
    /// commit through their coherent `GuestRegs` slots after helper success.
    pub(crate) fn try_lower_jit_mem_crc32c(
        &mut self,
        ops: &[SmirOp],
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let (load_pc, temporary, addr, mem_width) = match ops.get(idx) {
            Some(SmirOp {
                guest_pc,
                kind:
                    OpKind::Load {
                        dst: VReg::Virtual(temporary),
                        addr,
                        width,
                        sign: SignExtend::Zero,
                    },
                ..
            }) => (*guest_pc, VReg::Virtual(*temporary), addr, *width),
            _ => return Ok(None),
        };
        let data_width = match mem_width.to_op_width() {
            Some(width @ (OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64)) => width,
            _ => return Ok(None),
        };
        let (dst, crc) = match ops.get(idx + 1) {
            Some(SmirOp {
                guest_pc,
                kind:
                    OpKind::Crc32C {
                        dst,
                        crc,
                        data,
                        data_width: crc_width,
                    },
                ..
            }) if *guest_pc == load_pc && *data == temporary && *crc_width == data_width => {
                (*dst, *crc)
            }
            _ => return Ok(None),
        };
        if dst != crc {
            return Ok(None);
        }

        // Refuse malformed/non-SSA input in which the elided virtual value is
        // redefined or observed outside the immediately following CRC op.
        if virtual_definitions.get(&temporary) != Some(&1)
            || virtual_uses.get(&temporary) != Some(&1)
        {
            return Ok(None);
        }

        let dst_index = Self::x86_gpr_index(dst).ok_or_else(|| LowerError::InvalidOperand {
            op: "Crc32C memory".to_string(),
            operand: "destination is not an architectural x86 GPR".to_string(),
        })?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        self.emit_jit_mem_op(
            load_pc,
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
            Self::ensure_flag_stack_operands_safe("Crc32C memory", &[dst_reg])?;
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_crc32_rm(dst_reg, PhysReg::Rsp, 0, data_width);
        } else {
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_push(PhysReg::Rax);
                emitter.emit_push(PhysReg::Rdx);
            }
            self.emit_load_state_ptr_rax();
            self.emit_struct_mov(
                PhysReg::Rax,
                PhysReg::Rdx.encoding(),
                i32::from(dst_index) * 8,
                false,
            );
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_crc32_rm(PhysReg::Rdx, PhysReg::Rsp, 16, data_width);
            }
            // CRC32 always produces a 32-bit remainder and zero-extends the
            // complete architectural destination, including the W=1 form.
            self.emit_store_gpr_slot_from_reg(dst_index, PhysReg::Rdx, OpWidth::W32)?;
            if dst_index == 5 {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, OpWidth::W64);
            }
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_pop(PhysReg::Rdx);
                emitter.emit_pop(PhysReg::Rax);
            }
        }
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(Some(2))
    }
}
