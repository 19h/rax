//! Native-helper bridge for fault-precise far RET (`CA`/`CB`).

use super::X86_64Vcpu;
use crate::smir::ir::types::OpWidth;

/// Execute one protected IA-32e far RET through the owning vCPU. Encoding bits
/// 1:0 select W16/W32/W64, bit two records REX2/APX, and bits 31:16 carry the
/// unsigned immediate parameter-release count. Zero returns to direct replay;
/// one commits the helper-supplied dynamic target in `exit_pc`.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_far_return(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    encoding: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if encoding & !0xFFFF_0007 != 0
        || !vcpu.sregs.cs.l
        || vcpu.sregs.efer & (1 << 10) == 0
        || vcpu.sregs.cr0 & 1 == 0
        || vcpu.regs.rflags & crate::isa::x86_64::flags::bits::VM != 0
        || encoding & 0x4 != 0 && !vcpu.apx_enabled()
    {
        return 0;
    }
    let width = match encoding & 3 {
        0 => OpWidth::W16,
        1 => OpWidth::W32,
        2 => OpWidth::W64,
        _ => return 0,
    };
    let pop_bytes = (encoding >> 16) as u16;

    let saved_trace = vcpu.jit_mem_trace.clone();
    let saved_log = vcpu.jit_mem_log.clone();
    let mem_record_checkpoint = vcpu.mmu.mem_record_checkpoint();
    let saved_vcpu_rsp = vcpu.regs.rsp;
    vcpu.regs.rsp = state.gpr[4];
    if vcpu.return_far_long_mode(width, pop_bytes, true).is_err() {
        vcpu.regs.rsp = saved_vcpu_rsp;
        vcpu.jit_mem_trace = saved_trace;
        vcpu.jit_mem_log = saved_log;
        vcpu.mmu
            .restore_mem_record_checkpoint(mem_record_checkpoint);
        return 0;
    }

    state.gpr[4] = vcpu.regs.rsp;
    state.exit_pc = vcpu.regs.rip;
    state.cs_l = u64::from(vcpu.sregs.cs.l);
    state.cpl = u64::from(vcpu.sregs.cs.selector & 3);
    1
}
