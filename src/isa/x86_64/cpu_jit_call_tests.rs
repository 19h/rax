//! Native and helper-level tests for x86-64 lift-through-call JIT regions.

use super::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn test_vcpu_with_mem() -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    (X86_64Vcpu::new(0, memory.clone()), memory)
}

fn test_vcpu() -> X86_64Vcpu {
    test_vcpu_with_mem().0
}

fn configure_long_mode_jit(vcpu: &mut X86_64Vcpu) {
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rflags = 0x246;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(true);
}

#[test]
fn jit_callout_housekeeping_yields_on_run_loop_slice() {
    let mut vcpu = test_vcpu();
    let expired = Instant::now()
        .checked_sub(Duration::from_millis(2))
        .unwrap_or_else(Instant::now);

    assert!(!vcpu.jit_callout_should_yield(&expired, LAPIC_POLL_STRIDE - 1));
    assert!(vcpu.jit_callout_should_yield(&expired, LAPIC_POLL_STRIDE));
}

#[test]
fn jit_callout_synchronizes_callee_vector_opmask_and_mmx_state() {
    use crate::smir::lower::runtime::GuestRegs;

    let mem = Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // vprold $7,%zmm2,%zmm1{%k4}{z}; ldmxcsr 4(%rip);
    // paddb %mm1,%mm0; ret; .long 0x5f80
    let mut returning_callee = vec![
        0x62, 0xf1, 0x75, 0xcc, 0x72, 0xca, 0x07, 0x0f, 0xae, 0x15, 0x04, 0x00, 0x00, 0x00, 0x0f,
        0xfc, 0xc1, 0xc3,
    ];
    returning_callee.extend_from_slice(&0x5f80u32.to_le_bytes());
    mem.write_slice(&returning_callee, GuestAddress(0x100))
        .unwrap();
    let mut vcpu = X86_64Vcpu::new(0, mem.clone());
    vcpu.sregs.cr0 = 0x21;
    vcpu.sregs.cr4 = 0x20 | (1 << 9) | (1 << 18);
    vcpu.sregs.efer = 0x500;
    vcpu.sregs.cs.limit = u32::MAX;
    vcpu.sregs.cs.present = true;
    vcpu.sregs.cs.s = true;
    vcpu.sregs.cs.l = true;

    let source = [
        0x0123_4567_89ab_cdef,
        0x1111_2222_3333_4444,
        0x8000_0001_7fff_ffff,
        0xdead_beef_cafe_babe,
        0x0102_0304_0506_0708,
        0xf0e0_d0c0_b0a0_9080,
        0x1357_9bdf_2468_ace0,
        0xffff_ffff_0000_0001,
    ];
    let mask = 0x9669u64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        let input = (source[lane / 2] >> ((lane % 2) * 32)) as u32;
        let output = if ((mask >> lane) & 1) != 0 {
            input.rotate_left(7)
        } else {
            0
        };
        expected[lane / 2] |= u64::from(output) << ((lane % 2) * 32);
    }

    let mut gr = GuestRegs::default();
    gr.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    gr.gpr[4] = 0x8000;
    gr.rflags = 0x2;
    gr.vector_active = 1;
    gr.set_zmm(1, [u64::MAX; 8]);
    gr.set_zmm(2, source);
    gr.set_zmm(31, [0x3131_3131_3131_3131; 8]);
    gr.k[4] = mask;
    gr.k[7] = 0x7777_7777_7777_7777;
    gr.mxcsr = 0x3f80;
    gr.mm[0] = 0x00ff_7f80_0102_0304;
    gr.mm[1] = 0x0102_0304_0506_0708;
    gr.mmx_active = 1;

    let ok = unsafe { rax_jit_call(&mut gr, 0x100, 0x200, 0x80) };
    assert_eq!(ok, 1);
    assert_eq!(
        gr.get_zmm(1),
        expected,
        "callee destination was not returned"
    );
    assert_eq!(gr.get_zmm(2), source, "caller source was not preserved");
    assert_eq!(
        gr.get_zmm(31),
        [0x3131_3131_3131_3131; 8],
        "high ZMM state was not preserved"
    );
    assert_eq!(gr.k[4], mask);
    assert_eq!(gr.k[7], 0x7777_7777_7777_7777);
    assert_eq!(gr.mxcsr, 0x5f80, "callee MXCSR was not returned");
    assert_eq!(gr.mm[0], 0x0101_8284_0608_0A0C);
    assert_eq!(gr.mm[1], 0x0102_0304_0506_0708);

    // A successful callout can change XCR0 before native execution resumes.
    // Publish that control state into GuestRegs so a later lowered XGETBV
    // in the same region observes the callee's value rather than the entry
    // snapshot.
    let xsetbv_callee = [
        0xB9, 0, 0, 0, 0, // mov ecx,0
        0xB8, 0xE7, 0, 0, 0, // mov eax,0xE7 (x87|SSE|AVX|AVX-512)
        0x31, 0xD2, // xor edx,edx
        0x0F, 0x01, 0xD1, // xsetbv
        0xC3, // ret
    ];
    mem.write_slice(&xsetbv_callee, GuestAddress(0x500))
        .unwrap();
    let ok = unsafe { rax_jit_call(&mut gr, 0x500, 0x600, 0x480) };
    assert_eq!(ok, 1);
    assert_eq!(gr.xcr0, 0xE7, "callee XCR0 was not returned");
    assert_eq!(gr.cr4, vcpu.sregs.cr4, "callee CR4 was not returned");

    // A callee that mutates vector state and then yields HLT must publish
    // that state before returning `ok=0`; the run loop consumes the stashed
    // exit while leaving the interpreter state exactly at the yield.
    let mut yielding_callee = vec![
        0x62, 0xf1, 0x75, 0xcc, 0x72, 0xca, 0x07, 0x0f, 0xae, 0x15, 0x01, 0x00, 0x00, 0x00, 0xf4,
    ];
    yielding_callee.extend_from_slice(&0x7f80u32.to_le_bytes());
    mem.write_slice(&yielding_callee, GuestAddress(0x300))
        .unwrap();
    gr.set_zmm(1, [u64::MAX; 8]);
    gr.gpr[4] = 0x8000;
    let ok = unsafe { rax_jit_call(&mut gr, 0x300, 0x400, 0x280) };
    assert_eq!(ok, 0);
    assert_eq!(gr.get_zmm(1), expected, "bailing callee lost vector state");
    assert_eq!(gr.mxcsr, 0x7f80, "bailing callee lost MXCSR state");
    assert!(matches!(vcpu.jit_callout_exit, Some(VcpuExit::Hlt)));
}

#[test]
fn jit_callout_return_push_fault_deopts_at_call_pc_without_executing_target() {
    use crate::smir::lower::runtime::GuestRegs;

    let (mut vcpu, memory) = test_vcpu_with_mem();
    memory
        .write_slice(&[0x48, 0xFF, 0xC0, 0xC3], GuestAddress(0x100))
        .unwrap();
    configure_long_mode_jit(&mut vcpu);

    let mut gr = GuestRegs::default();
    gr.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    gr.gpr[0] = 41;
    gr.gpr[4] = 0x10008;
    gr.rflags = 0x246;
    let call_pc = 0x80;

    let ok = unsafe { rax_jit_call(&mut gr, 0x100, 0x200, call_pc) };

    assert_eq!(ok, 0);
    assert_eq!(gr.exit_pc, call_pc);
    assert_eq!(gr.gpr[4], 0x10008, "faulting push must not update RSP");
    assert_eq!(gr.gpr[0], 41, "faulting CALL must not execute its target");
    assert_eq!(gr.rflags, 0x246);
    assert_eq!(vcpu.regs.rip, call_pc);
    assert!(vcpu.jit_callout_exit.is_none());
}

#[test]
fn jit_compiles_and_executes_rip_relative_memory_indirect_call() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    // call qword ptr [rip+0x0ffa]; mov rbx,rax; jmp next; hlt
    memory
        .write_slice(
            &[
                0xFF, 0x15, 0xFA, 0x0F, 0x00, 0x00, 0x48, 0x89, 0xC3, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    memory
        .write_slice(&0x2000u64.to_le_bytes(), GuestAddress(0x1000))
        .unwrap();
    memory
        .write_slice(&[0x48, 0xFF, 0xC0, 0xC3], GuestAddress(0x2000))
        .unwrap();
    configure_long_mode_jit(&mut vcpu);
    vcpu.regs.rax = 41;
    vcpu.regs.rbx = 0xDEAD_BEEF;
    vcpu.regs.rsp = 0x8000;

    let region = vcpu
        .jit_compile_region()
        .expect("compile RIP-relative CALL region")
        .expect("state-backed memory-indirect CALL should remain native");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rax, 42);
    assert_eq!(vcpu.regs.rbx, 42, "native continuation did not resume");
    assert_eq!(vcpu.regs.rsp, 0x8000);
    assert_eq!(vcpu.regs.rip, 11);
}

#[test]
fn jit_memory_indirect_call_uses_pre_push_rsp_for_target_operand() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    // call qword ptr [rsp]; mov rbx,rax; jmp next; hlt
    memory
        .write_slice(
            &[0xFF, 0x14, 0x24, 0x48, 0x89, 0xC3, 0xEB, 0x00, 0xF4],
            GuestAddress(0),
        )
        .unwrap();
    memory
        .write_slice(&0x2000u64.to_le_bytes(), GuestAddress(0x8000))
        .unwrap();
    memory
        .write_slice(&[0x48, 0xFF, 0xC0, 0xC3], GuestAddress(0x2000))
        .unwrap();
    configure_long_mode_jit(&mut vcpu);
    vcpu.regs.rax = 41;
    vcpu.regs.rbx = 0xDEAD_BEEF;
    vcpu.regs.rsp = 0x8000;

    let region = vcpu
        .jit_compile_region()
        .expect("compile RSP-relative CALL region")
        .expect("CALL [RSP] should retain a state-backed target");
    vcpu.jit_run_region_native(&region);

    let mut target_bytes = [0u8; 8];
    memory
        .read_slice(&mut target_bytes, GuestAddress(0x8000))
        .unwrap();
    assert_eq!(u64::from_le_bytes(target_bytes), 0x2000);
    assert_eq!(vcpu.regs.rax, 42);
    assert_eq!(vcpu.regs.rbx, 42);
    assert_eq!(vcpu.regs.rsp, 0x8000);
    assert_eq!(vcpu.regs.rip, 8);
}

#[test]
fn jit_memory_indirect_call_target_fault_restarts_at_call_without_stack_effect() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    // The 8-byte target begins at 0xffff and crosses the mapped-memory end.
    // The continuation is otherwise native, proving the CALL itself was
    // admitted before the target load deoptimized.
    memory
        .write_slice(
            &[
                0xFF, 0x15, 0xF9, 0xFF, 0x00, 0x00, 0x48, 0xFF, 0xC0, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    configure_long_mode_jit(&mut vcpu);
    vcpu.regs.rax = 41;
    vcpu.regs.rsp = 0x8000;
    let original_flags = vcpu.regs.rflags;

    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting target-load region")
        .expect("state-backed memory-indirect CALL should be JIT eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rip, 0, "target fault must restart the CALL");
    assert_eq!(vcpu.regs.rsp, 0x8000, "target fault must precede the push");
    assert_eq!(vcpu.regs.rax, 41, "continuation must not execute");
    assert_eq!(vcpu.regs.rflags, original_flags);
}

#[test]
fn jit_memory_indirect_call_stack_fault_restarts_at_call_without_target_execution() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    memory
        .write_slice(
            &[
                0xFF, 0x15, 0xFA, 0x0F, 0x00, 0x00, 0x48, 0x89, 0xC3, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    memory
        .write_slice(&0x2000u64.to_le_bytes(), GuestAddress(0x1000))
        .unwrap();
    memory
        .write_slice(&[0x48, 0xFF, 0xC0, 0xC3], GuestAddress(0x2000))
        .unwrap();
    configure_long_mode_jit(&mut vcpu);
    vcpu.regs.rax = 41;
    vcpu.regs.rbx = 0xDEAD_BEEF;
    vcpu.regs.rsp = 0x10008;

    let region = vcpu
        .jit_compile_region()
        .expect("compile stack-faulting CALL region")
        .expect("memory-indirect CALL should be JIT eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rip, 0, "stack fault must restart the CALL");
    assert_eq!(vcpu.regs.rsp, 0x10008, "faulting push must not update RSP");
    assert_eq!(
        vcpu.regs.rax, 41,
        "callee must not execute after push failure"
    );
    assert_eq!(vcpu.regs.rbx, 0xDEAD_BEEF, "continuation must not execute");
}

#[test]
fn jit_callouts_remain_disabled_outside_64_bit_code_segments() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    memory
        .write_slice(&[0xE8, 0x00, 0x00, 0x00, 0x00, 0xC3], GuestAddress(0))
        .unwrap();
    vcpu.sregs.cs.db = true;
    vcpu.sregs.cs.l = false;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rflags = 0x2;
    vcpu.set_jit_call(true);

    assert!(
        vcpu.jit_compile_region()
            .expect("compatibility-mode JIT query")
            .is_none(),
        "32-bit near CALL must remain an interpreter frontier"
    );
}

#[test]
fn jit_callout_rejects_legacy_operand_size_override_until_widths_are_unified() {
    let (mut vcpu, memory) = test_vcpu_with_mem();
    // 66 call qword ptr [rip+0xff9]; ret. The direct interpreter currently
    // assigns a 16-bit near-CALL stack width to this encoding, whereas the
    // callout ABI is intentionally 64-bit only.
    memory
        .write_slice(
            &[0x66, 0xFF, 0x15, 0xF9, 0x0F, 0x00, 0x00, 0xC3],
            GuestAddress(0),
        )
        .unwrap();
    memory
        .write_slice(&0x2000u64.to_le_bytes(), GuestAddress(0x1000))
        .unwrap();
    configure_long_mode_jit(&mut vcpu);
    vcpu.regs.rsp = 0x8000;

    assert!(
        vcpu.jit_compile_region()
            .expect("operand-size override JIT query")
            .is_none(),
        "non-unified CALL width must remain an interpreter frontier"
    );
}
