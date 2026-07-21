//! Native x86-64 JIT differential coverage for Group 15 fence aliases.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rax = 0x1234_5678_9ABC_DEF0;
    vcpu.regs.rbx = 0x0FED_CBA9_8765_4321;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    // Apple host translation clears imported AF across the linux/amd64 native
    // bridge; all other status and control flags remain differential inputs.
    vcpu.regs.rflags = 0x2 | 0x08C5 | (1 << 10);
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..16 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct fence-alias sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct fence-alias execution did not reach {target:#x}");
}

#[test]
fn jit_fence_aliases_match_direct_without_state_commit() {
    let memory = memory_with_code(&[
        0x0F, 0xAE, 0xEF, // LFENCE alias
        0x0F, 0xAE, 0xF7, // MFENCE alias
        0x0F, 0xAE, 0xFF, // SFENCE alias
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 11);
    let region = native
        .jit_compile_region()
        .expect("compile fence-alias region")
        .expect("every documented fence alias must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rax, direct.regs.rax);
    assert_eq!(native.regs.rbx, direct.regs.rbx);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rbp, direct.regs.rbp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, direct.regs.rip);
    assert_eq!(native.regs.rip, 11);
}

#[test]
fn jit_reserved_prefix_fence_aliases_match_the_direct_deterministic_policy() {
    let memory = memory_with_code(&[
        0x66, 0x0F, 0xAE, 0xEF, // 66 LFENCE alias
        0xF2, 0x0F, 0xAE, 0xE9, // F2 LFENCE alias
        0x66, 0x0F, 0xAE, 0xFF, // 66 SFENCE alias
        0xF3, 0x0F, 0xAE, 0xF9, // F3 SFENCE alias
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 18);
    let region = native
        .jit_compile_region()
        .expect("compile reserved-prefix fence-alias region")
        .expect("direct-policy fence aliases must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rax, direct.regs.rax);
    assert_eq!(native.regs.rbx, direct.regs.rbx);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rbp, direct.regs.rbp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, direct.regs.rip);
    assert_eq!(native.regs.rip, 18);
}

#[test]
fn jit_reserved_group15_slot_exits_at_the_exact_noncommitting_frontier() {
    let memory = memory_with_code(&[
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
        0xEB, 0x02, // jmp reserved Group-15 slot
        0x90, 0x90, // unreachable padding
        0x0F, 0xAE, 0xD0, // reserved register /2
    ]);
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile region ending at reserved Group-15 slot")
        .expect("supported prefix must remain native before Group-15 frontier");

    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rax, 0x1234_5678);
    assert_eq!(vcpu.regs.rip, 9);

    let before = (
        vcpu.regs.rax,
        vcpu.regs.rsp,
        vcpu.regs.rbp,
        vcpu.regs.rflags,
    );
    let error = vcpu
        .step()
        .expect_err("reserved Group-15 frontier must deliver #UD");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "expected #UD delivery failure, got {error}"
    );
    assert_eq!(
        (
            vcpu.regs.rax,
            vcpu.regs.rsp,
            vcpu.regs.rbp,
            vcpu.regs.rflags,
        ),
        before
    );
    assert_eq!(vcpu.regs.rip, 9);
}
