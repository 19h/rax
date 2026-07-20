//! Native-helper bridge for fault-precise indirect far JMP.

use super::X86_64Vcpu;
use crate::smir::ir::types::OpWidth;

/// Execute one long-mode `FF /5` through the owning vCPU. Encoding bits 1:0
/// select the far-pointer offset width, bit two records REX2/APX, and bit three
/// records an SS-based memory operand. Every failure rolls back speculative JIT
/// trace/log bookkeeping and returns zero so native code exits at the original
/// guest PC for exact direct replay.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_far_jump(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    pointer_address: u64,
    encoding: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if encoding & !0xF != 0
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
    let stack_segment = encoding & 0x8 != 0;

    let saved_trace = vcpu.jit_mem_trace.clone();
    let saved_log = vcpu.jit_mem_log.clone();
    let mem_record_checkpoint = vcpu.mmu.mem_record_checkpoint();
    if vcpu
        .jump_far_long_mode(pointer_address, width, stack_segment, true)
        .is_err()
    {
        vcpu.jit_mem_trace = saved_trace;
        vcpu.jit_mem_log = saved_log;
        vcpu.mmu
            .restore_mem_record_checkpoint(mem_record_checkpoint);
        return 0;
    }

    state.exit_pc = vcpu.regs.rip;
    state.cs_l = u64::from(vcpu.sregs.cs.l);
    state.cpl = u64::from(vcpu.sregs.cs.selector & 3);
    1
}
