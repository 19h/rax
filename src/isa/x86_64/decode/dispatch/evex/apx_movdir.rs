//! Direct execution for APX-promoted MOVDIR64B and MOVDIRI encodings.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute;
use crate::vm::vcpu::VcpuExit;

impl X86_64Vcpu {
    /// Execute `EVEX.LLZ.66.MAP4.W0 F8 !(11):rrr:bbb` MOVDIR64B.
    pub(crate) fn execute_apx_movdir64b(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.expect("APX MOVDIR64B requires EVEX context");
        if evex.pp != 1 || evex.w || !Self::apx_movdir_fields_valid(ctx) {
            return self.inject_invalid_opcode();
        }
        execute::data::movdir64b_apx(self, ctx)
    }

    /// Execute `EVEX.LLZ.NP.MAP4 F9 !(11):rrr:bbb` MOVDIRI.
    pub(crate) fn execute_apx_movdiri(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.expect("APX MOVDIRI requires EVEX context");
        if evex.pp != 0 || !Self::apx_movdir_fields_valid(ctx) {
            return self.inject_invalid_opcode();
        }
        execute::data::movdiri_apx(self, ctx)
    }

    fn apx_movdir_fields_valid(ctx: &InsnContext) -> bool {
        let evex = ctx.evex.expect("APX MOVDIR requires EVEX context");
        !evex.nd
            && !evex.nf
            && !evex.z
            && evex.ll == 0
            && evex.aaa == 0
            && evex.vvvv == 0x0F
            && evex.v_prime
    }
}
