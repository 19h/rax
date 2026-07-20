//! Native x86-64 JIT differentials for CLTS guest CR0 state and fault handoff.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CR0_PE: u64 = 1;
const CR0_TS: u64 = 1 << 3;
const RFLAGS_VM: u64 = 1 << 17;

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x50033 | CR0_TS;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct CLTS sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

#[test]
fn jit_clts_matches_direct_and_commits_cr0_through_the_vcpu_abi() {
    // clts; jmp hlt; hlt. The branch keeps CLTS in a non-frontier block.
    let memory = memory_with_code(&[0x0F, 0x06, 0xEB, 0x00, 0xF4]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 4);
    let region = native
        .jit_compile_region()
        .expect("compile CLTS region")
        .expect("CLTS must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.cr0, direct.sregs.cr0);
    assert_eq!(native.sregs.cr0 & CR0_TS, 0);
    assert_eq!(native.regs.rax, direct.regs.rax);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 4);
}

#[test]
fn jit_verify_snapshots_compares_and_adopts_clts_cr0_state() {
    let memory = memory_with_code(&[0x0F, 0x06, 0xEB, 0x00, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified CLTS region")
        .expect("verified CLTS region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.sregs.cr0 & CR0_TS, 0);
    assert_eq!(vcpu.regs.rip, 4);
}

#[test]
fn jit_clts_dynamic_privilege_faults_are_precise_and_noncommitting() {
    for (name, configure) in [
        ("protected-cpl3", (3u16, 0u64)),
        ("virtual-8086-with-cs-rpl0", (0u16, RFLAGS_VM)),
    ] {
        let memory = memory_with_code(&[0x0F, 0x06, 0xEB, 0x00, 0xF4]);
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cr0 = CR0_PE | CR0_TS | 0x30;
        vcpu.sregs.cs.selector = configure.0;
        vcpu.regs.rflags |= configure.1;
        let before = (vcpu.sregs.cr0, vcpu.regs.rax, vcpu.regs.rflags);

        let region = vcpu
            .jit_compile_region()
            .expect("compile guarded CLTS region")
            .expect("dynamic CLTS privilege must not block admission");
        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0, "{name}: precise fault PC");
        assert_eq!(
            (vcpu.sregs.cr0, vcpu.regs.rax, vcpu.regs.rflags),
            before,
            "{name}: fault must not commit"
        );
        assert!(
            vcpu.step().is_err(),
            "{name}: direct re-execution must deliver #GP(0)"
        );
        assert_eq!(vcpu.sregs.cr0, before.0, "{name}: direct fault changed CR0");
    }
}

#[test]
fn jit_clts_state_is_coherent_across_interpreter_callouts() {
    let memory = memory_with_code(&[
        0x0F, 0x06, // clts
        0xE8, 0xF9, 0x00, 0x00, 0x00, // call 100h
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    // mov rax,cr0; ret
    memory
        .write_slice(&[0x0F, 0x20, 0xC0, 0xC3], GuestAddress(0x100))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.set_jit_call(true);

    let region = vcpu
        .jit_compile_region()
        .expect("compile callout CLTS region")
        .expect("CLTS callout sequence must be native eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.sregs.cr0 & CR0_TS, 0);
    assert_eq!(vcpu.regs.rax & CR0_TS, 0, "callee observed stale CR0.TS");
    assert_eq!(vcpu.regs.rax, vcpu.sregs.cr0);
    assert_eq!(vcpu.regs.rsp, 0x8000);
    assert_eq!(vcpu.regs.rip, 9);
}
