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
