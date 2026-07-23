//! Direct execution for APX-promoted scalar MOVRS encodings.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

impl X86_64Vcpu {
    /// Execute `EVEX.LLZ.{NP,66}.MAP4 {8A,8B} !(11):rrr:bbb`.
    pub(crate) fn execute_apx_movrs(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.expect("APX MOVRS requires EVEX context");
        let is_byte = opcode == 0x8A;

        // Type APX-EVEX-MOVRS fixes VVVVV=11111, LL=0, and every P2
        // payload bit except V4 to zero. The byte form additionally requires
        // W=0 and NP. Both forms require a memory source.
        if !matches!(evex.pp, 0 | 1)
            || evex.nd
            || evex.nf
            || evex.z
            || evex.ll != 0
            || evex.aaa & 0x03 != 0
            || evex.vvvv != 0x0F
            || !evex.v_prime
            || (is_byte && (evex.w || evex.pp != 0))
            || ctx.peek_u8()? >> 6 == 3
        {
            return self.inject_invalid_opcode();
        }

        let width = if is_byte {
            1
        } else {
            Self::apx_scalar_op_size(ctx)
        };
        let (reg, _, is_memory, addr, _) = self.decode_modrm(ctx)?;
        debug_assert!(is_memory, "APX MOVRS excludes ModR/M register sources");
        let destination = reg | ctx.evex_dest_reg();
        let value = self.read_mem(addr, width)?;
        self.set_reg(destination, value, width);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
