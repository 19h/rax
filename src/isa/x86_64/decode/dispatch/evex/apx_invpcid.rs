//! Direct execution for APX-promoted INVPCID.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute;
use crate::vm::vcpu::VcpuExit;

impl X86_64Vcpu {
    /// Execute `EVEX.LLZ.F3.MAP4.WIG F2 !(11):rrr:bbb` INVPCID.
    pub(crate) fn execute_apx_invpcid(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.expect("APX INVPCID requires EVEX context");
        let modrm = ctx.peek_u8()?;
        if evex.pp != 2
            || evex.nd
            || evex.nf
            || evex.z
            || evex.ll != 0
            || evex.aaa != 0
            || evex.vvvv != 0x0F
            || !evex.v_prime
            || modrm >> 6 == 3
        {
            return self.inject_invalid_opcode();
        }
        execute::system::invpcid_apx(self, ctx)
    }
}
