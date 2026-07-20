//! Interpreter callout used by x86-64 lift-through-call JIT regions.

use super::*;

/// Run the interpreter for a guest near CALL's callee.
///
/// The native caller publishes its live architectural state in `gr`. This
/// helper performs the remaining CALL effect (the 8-byte return-address push),
/// executes the callee in the interpreter, and republishes the post-call state
/// before native execution resumes at `return_pc`.
///
/// Returns `1` after a clean return. Returns `0` and writes `gr.exit_pc` when
/// the stack push faults, the callee yields a VMM-bound exit, or interpretation
/// fails. A push fault leaves guest RSP unchanged and deoptimizes at `call_pc`;
/// the target is never executed.
///
/// # Safety
///
/// `gr` must point to the live [`crate::smir::lower::runtime::GuestRegs`] for
/// the owning [`X86_64Vcpu`], and `gr.ctx` must contain that vCPU's address.
pub(super) unsafe extern "C" fn rax_jit_call(
    gr: *mut crate::smir::lower::runtime::GuestRegs,
    target_pc: u64,
    return_pc: u64,
    call_pc: u64,
) -> u64 {
    let gr = unsafe { &mut *gr };
    let vcpu = unsafe { &mut *(gr.ctx as *mut X86_64Vcpu) };

    // Sync marshalled native state -> vcpu interpreter state. The JIT works
    // with materialized flags, so clear any pending lazy operation.
    vcpu.regs.rax = gr.gpr[0];
    vcpu.regs.rcx = gr.gpr[1];
    vcpu.regs.rdx = gr.gpr[2];
    vcpu.regs.rbx = gr.gpr[3];
    vcpu.regs.rsp = gr.gpr[4];
    vcpu.regs.rbp = gr.gpr[5];
    vcpu.regs.rsi = gr.gpr[6];
    vcpu.regs.rdi = gr.gpr[7];
    vcpu.regs.r8 = gr.gpr[8];
    vcpu.regs.r9 = gr.gpr[9];
    vcpu.regs.r10 = gr.gpr[10];
    vcpu.regs.r11 = gr.gpr[11];
    vcpu.regs.r12 = gr.gpr[12];
    vcpu.regs.r13 = gr.gpr[13];
    vcpu.regs.r14 = gr.gpr[14];
    vcpu.regs.r15 = gr.gpr[15];
    vcpu.regs.r16 = gr.gpr[16];
    vcpu.regs.r17 = gr.gpr[17];
    vcpu.regs.r18 = gr.gpr[18];
    vcpu.regs.r19 = gr.gpr[19];
    vcpu.regs.r20 = gr.gpr[20];
    vcpu.regs.r21 = gr.gpr[21];
    vcpu.regs.r22 = gr.gpr[22];
    vcpu.regs.r23 = gr.gpr[23];
    vcpu.regs.r24 = gr.gpr[24];
    vcpu.regs.r25 = gr.gpr[25];
    vcpu.regs.r26 = gr.gpr[26];
    vcpu.regs.r27 = gr.gpr[27];
    vcpu.regs.r28 = gr.gpr[28];
    vcpu.regs.r29 = gr.gpr[29];
    vcpu.regs.r30 = gr.gpr[30];
    vcpu.regs.r31 = gr.gpr[31];
    vcpu.regs.rflags =
        (gr.rflags & !flags::bits::AC) | if gr.ac_flag != 0 { flags::bits::AC } else { 0 };
    vcpu.sregs.fs.base = gr.fs_base;
    vcpu.sregs.gs.base = gr.gs_base;
    vcpu.kernel_gs_base = gr.kernel_gs_base;
    vcpu.pkru = gr.pkru;
    // Publish every control register carried by the native ABI so the direct
    // callee observes prior native state and later native reads observe every
    // control-register write committed by the callee.
    vcpu.sregs.cr0 = gr.cr0;
    vcpu.sregs.cr2 = gr.cr2;
    vcpu.sregs.cr3 = gr.cr3;
    vcpu.sregs.cr4 = gr.cr4;
    vcpu.sregs.cr8 = gr.cr8;
    vcpu.sregs.efer = gr.efer;
    vcpu.sregs.dr0 = gr.dr0;
    vcpu.sregs.dr1 = gr.dr1;
    vcpu.sregs.dr2 = gr.dr2;
    vcpu.sregs.dr3 = gr.dr3;
    vcpu.sregs.dr6 = gr.dr6;
    vcpu.sregs.dr7 = gr.dr7;
    vcpu.lazy_flags = LazyFlags {
        op: LazyFlagOp::None,
        ..Default::default()
    };
    if gr.vector_active != 0 {
        for index in 0..16 {
            let low = gr.get_zmm(index);
            vcpu.regs.xmm[index] = [low[0], low[1]];
            vcpu.regs.ymm_high[index] = [low[2], low[3]];
            vcpu.regs.zmm_high[index] = [low[4], low[5], low[6], low[7]];
            vcpu.regs.zmm_ext[index] = gr.get_zmm(index + 16);
        }
        vcpu.regs.k = gr.k;
        vcpu.mxcsr = gr.mxcsr;
    }
    if gr.mmx_active != 0 {
        vcpu.regs.mm = gr.mm;
        vcpu.fpu.tag_word = gr.x87_tag_word as u16;
    }

    // The target operand, if memory-indirect, has already been read by the
    // lowered MMU helper. The architectural return-address push is next. push64
    // updates RSP only after a successful write, so its error path is precise.
    let mut ok = if vcpu.push64(return_pc).is_ok() {
        vcpu.regs.rip = target_pc;
        1
    } else {
        vcpu.regs.rip = call_pc;
        0
    };

    if ok != 0 {
        // Run the callee to completion. The step cap is a runaway backstop;
        // normal run-loop scheduling is retained by the periodic yield check.
        let start_time = std::time::Instant::now();
        let mut steps: u64 = 0;
        loop {
            if vcpu.regs.rip == return_pc {
                break;
            }
            steps += 1;
            if steps > 500_000_000 {
                ok = 0;
                break;
            }
            if vcpu.jit_callout_should_yield(&start_time, steps) {
                vcpu.jit_callout_exit = Some(VcpuExit::Hlt);
                ok = 0;
                break;
            }
            // This direct-step loop bypasses the run-loop SMC drain.
            vcpu.drain_smc();
            match vcpu.step() {
                Ok(None) => {}
                Ok(Some(exit)) => {
                    vcpu.jit_callout_exit = Some(exit);
                    ok = 0;
                    break;
                }
                Err(_) => {
                    ok = 0;
                    break;
                }
            }
        }
    }

    // Sync vcpu state back into the marshalled file. Materialize flags first so
    // the native reload or trampoline sees current architectural RFLAGS.
    vcpu.materialize_flags();
    if gr.vector_active != 0 {
        for index in 0..16 {
            gr.set_zmm(
                index,
                [
                    vcpu.regs.xmm[index][0],
                    vcpu.regs.xmm[index][1],
                    vcpu.regs.ymm_high[index][0],
                    vcpu.regs.ymm_high[index][1],
                    vcpu.regs.zmm_high[index][0],
                    vcpu.regs.zmm_high[index][1],
                    vcpu.regs.zmm_high[index][2],
                    vcpu.regs.zmm_high[index][3],
                ],
            );
            gr.set_zmm(index + 16, vcpu.regs.zmm_ext[index]);
        }
        gr.k = vcpu.regs.k;
        gr.mxcsr = vcpu.mxcsr;
    }
    if gr.mmx_active != 0 {
        gr.mm = vcpu.regs.mm;
        gr.x87_tag_word = u64::from(vcpu.fpu.tag_word);
    }
    gr.gpr[0] = vcpu.regs.rax;
    gr.gpr[1] = vcpu.regs.rcx;
    gr.gpr[2] = vcpu.regs.rdx;
    gr.gpr[3] = vcpu.regs.rbx;
    gr.gpr[4] = vcpu.regs.rsp;
    gr.gpr[5] = vcpu.regs.rbp;
    gr.gpr[6] = vcpu.regs.rsi;
    gr.gpr[7] = vcpu.regs.rdi;
    gr.gpr[8] = vcpu.regs.r8;
    gr.gpr[9] = vcpu.regs.r9;
    gr.gpr[10] = vcpu.regs.r10;
    gr.gpr[11] = vcpu.regs.r11;
    gr.gpr[12] = vcpu.regs.r12;
    gr.gpr[13] = vcpu.regs.r13;
    gr.gpr[14] = vcpu.regs.r14;
    gr.gpr[15] = vcpu.regs.r15;
    gr.gpr[16] = vcpu.regs.r16;
    gr.gpr[17] = vcpu.regs.r17;
    gr.gpr[18] = vcpu.regs.r18;
    gr.gpr[19] = vcpu.regs.r19;
    gr.gpr[20] = vcpu.regs.r20;
    gr.gpr[21] = vcpu.regs.r21;
    gr.gpr[22] = vcpu.regs.r22;
    gr.gpr[23] = vcpu.regs.r23;
    gr.gpr[24] = vcpu.regs.r24;
    gr.gpr[25] = vcpu.regs.r25;
    gr.gpr[26] = vcpu.regs.r26;
    gr.gpr[27] = vcpu.regs.r27;
    gr.gpr[28] = vcpu.regs.r28;
    gr.gpr[29] = vcpu.regs.r29;
    gr.gpr[30] = vcpu.regs.r30;
    gr.gpr[31] = vcpu.regs.r31;
    gr.rflags = vcpu.regs.rflags & !flags::bits::AC;
    gr.ac_flag = u64::from(vcpu.regs.rflags & flags::bits::AC != 0);
    gr.fs_base = vcpu.sregs.fs.base;
    gr.gs_base = vcpu.sregs.gs.base;
    gr.kernel_gs_base = vcpu.kernel_gs_base;
    gr.pkru = vcpu.pkru;
    gr.xcr0 = vcpu.xcr0;
    gr.xgetbv1 = vcpu.xgetbv1_value;
    gr.cr4 = vcpu.sregs.cr4;
    gr.cr0 = vcpu.sregs.cr0;
    gr.cr2 = vcpu.sregs.cr2;
    gr.cr3 = vcpu.sregs.cr3;
    gr.cr8 = vcpu.sregs.cr8;
    gr.efer = vcpu.sregs.efer;
    gr.cs_l = u64::from(vcpu.sregs.cs.l);
    gr.tr_type = u64::from(vcpu.sregs.tr.type_ & 0x0F);
    gr.dr0 = vcpu.sregs.dr0;
    gr.dr1 = vcpu.sregs.dr1;
    gr.dr2 = vcpu.sregs.dr2;
    gr.dr3 = vcpu.sregs.dr3;
    gr.dr6 = vcpu.sregs.dr6;
    gr.dr7 = vcpu.sregs.dr7;
    gr.cpl = if vcpu.regs.rflags & flags::bits::VM != 0 {
        3
    } else {
        u64::from(vcpu.sregs.cs.selector & 3)
    };
    gr.apx_enabled = u64::from(vcpu.apx_enabled());
    gr.cpuid_xeon_phi_avx512 = u64::from(vcpu.xeon_phi_avx512_enabled());
    gr.cpuid_vp2intersect = u64::from(vcpu.vp2intersect_enabled());
    gr.cpuid_sse4a = u64::from(vcpu.sse4a_enabled());
    if ok == 0 {
        gr.exit_pc = vcpu.regs.rip;
    }
    ok
}

/// Lift-through-calls is enabled by default. `RAX_JIT_NO_CALL=1` restores
/// call-as-frontier behavior; the CPU additionally restricts callouts to
/// 64-bit code segments.
pub(super) fn jit_call_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| jit_default_enabled(std::env::var_os("RAX_JIT_NO_CALL").is_some()))
}

/// Whether the x86-64 callout lowerer can resolve this target without an
/// allocator-owned virtual address component.
pub(super) fn jit_call_target_supported(
    target: &crate::smir::ir::CallTarget,
    mem_helpers: bool,
) -> bool {
    use crate::smir::ir::CallTarget;

    match target {
        CallTarget::GuestAddr(_) | CallTarget::Indirect(_) => true,
        CallTarget::IndirectMem(addr) => mem_helpers && addr.is_x86_state_backed_shape(),
        CallTarget::X86IndirectMemAddr32(addr) => {
            mem_helpers && addr.is_x86_addr32_state_backed_shape()
        }
        _ => false,
    }
}

/// Whether this call target invokes the guest-MMU helper before its callout.
pub(super) fn jit_call_target_uses_mem_helper(target: &crate::smir::ir::CallTarget) -> bool {
    use crate::smir::ir::CallTarget;

    match target {
        CallTarget::IndirectMem(addr) => addr.is_x86_state_backed_shape(),
        CallTarget::X86IndirectMemAddr32(addr) => addr.is_x86_addr32_state_backed_shape(),
        _ => false,
    }
}
