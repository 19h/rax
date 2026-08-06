//! Legacy SSE2 transfers between the MMX and XMM register files.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// `MOVQ2DQ xmm, mm` (`F3 0F D6 /r`).
pub fn movq2dq(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, _, _) = vcpu.decode_modrm(ctx)?;
    if is_memory {
        return vcpu.inject_undefined_instruction();
    }

    let xmm_dst = reg as usize;
    let mm_src = (rm & 0x07) as usize;
    vcpu.regs.xmm[xmm_dst] = [vcpu.regs.mm[mm_src], 0];
    vcpu.fpu.tag_word = 0;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// `MOVDQ2Q mm, xmm` (`F2 0F D6 /r`).
pub fn movdq2q(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let (reg, rm, is_memory, _, _) = vcpu.decode_modrm(ctx)?;
    if is_memory {
        return vcpu.inject_undefined_instruction();
    }

    let mm_dst = (reg & 0x07) as usize;
    let xmm_src = rm as usize;
    vcpu.regs.mm[mm_dst] = vcpu.regs.xmm[xmm_src][0];
    vcpu.fpu.tag_word = 0;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
