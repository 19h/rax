//! Shared helper-backed EVEX scalar memory stack replay emission.

use super::{X86_64Lowerer, X86Cond, X86Emitter};
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::types::{Address, GuestAddr, MemWidth, OpWidth, SignExtend};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;

impl X86_64Lowerer {
    /// Stage one exact scalar helper load in a 16-byte nonarchitectural stack
    /// slot and execute a byte-validated EVEX `[rsp]` replay.
    ///
    /// If a writemask exists, a live-host-K bit-0 guard bypasses the helper
    /// when the architectural access is suppressed. Both paths restore guest
    /// RAX and RFLAGS before replay; the helper fault frontier restores the
    /// reserved stack adjustment before leaving native code.
    pub(super) fn emit_evex_scalar_memory_stack_replay(
        &mut self,
        guest_pc: GuestAddr,
        address: &Address,
        memory_width: MemWidth,
        writemask: Option<u8>,
        stack_instruction: X86InstructionBytes,
    ) -> Result<(), LowerError> {
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        let inactive = if let Some(mask) = writemask {
            self.code.emit_u8(0x9C); // pushfq
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_opmask_mask_to_rax64(mask);
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_test_ri(PhysReg::Rax, 1, OpWidth::W64);
            }
            Some(self.emit_jcc_placeholder(X86Cond::E))
        } else {
            None
        };

        if inactive.is_some() {
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
        }
        self.emit_jit_mem_op(
            guest_pc,
            true,
            None,
            Some(16),
            None,
            None,
            None,
            address,
            memory_width,
            SignExtend::Zero,
            16,
        )?;
        if let Some(inactive) = inactive {
            self.code.emit_u8(0xE9);
            let execute = self.code.position();
            self.code.emit_u32(0);
            self.patch_rel32_to_current(inactive)?;
            self.code.emit_u8(0x58); // pop guest RAX
            self.code.emit_u8(0x9D); // restore exact pre-guard flags
            self.patch_rel32_to_current(execute)?;
        }
        self.code.emit_bytes(stack_instruction.as_slice());
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(())
    }
}
