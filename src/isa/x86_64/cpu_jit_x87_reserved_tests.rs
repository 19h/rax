//! Native x86-64 JIT frontier coverage for reserved x87 escape forms.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const FRONTIER_PC: u64 = 9;

fn memory_with_frontier(frontier: &[u8]) -> Arc<GuestMemoryMmap> {
    let mut code = vec![
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
        0xEB, 0x02, // jmp reserved x87 form
        0x90, 0x90, // unreachable padding
    ];
    code.extend_from_slice(frontier);
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(&code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rax = 0xFFFF_FFFF_FFFF_FFFF;
    vcpu.regs.rbx = 0x0FED_CBA9_8765_4321;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08C5 | flags::bits::DF;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn assert_exact_noncommitting_frontier(frontier: &[u8]) {
    let mut vcpu = test_vcpu(memory_with_frontier(frontier));
    let region = vcpu
        .jit_compile_region()
        .expect("compile region ending at reserved x87 form")
        .expect("supported prefix must remain native before reserved x87 frontier");

    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rax, 0x1234_5678, "{frontier:02X?}");
    assert_eq!(vcpu.regs.rip, FRONTIER_PC, "{frontier:02X?}");

    let before = (
        vcpu.regs.rax,
        vcpu.regs.rbx,
        vcpu.regs.rsp,
        vcpu.regs.rbp,
        vcpu.regs.rflags,
    );
    let error = vcpu
        .step()
        .expect_err("reserved x87 frontier must deliver #UD");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "expected #UD delivery failure for {frontier:02X?}, got {error}"
    );
    assert_eq!(
        (
            vcpu.regs.rax,
            vcpu.regs.rbx,
            vcpu.regs.rsp,
            vcpu.regs.rbp,
            vcpu.regs.rflags,
        ),
        before,
        "{frontier:02X?}"
    );
    assert_eq!(vcpu.regs.rip, FRONTIER_PC, "{frontier:02X?}");
}

#[test]
fn jit_reserved_x87_cells_exit_at_exact_noncommitting_frontiers() {
    for frontier in [
        &[0xDB, 0xE5][..],
        &[0xD9, 0x0D, 0x00, 0x40, 0x00, 0x00],
        &[0xF0, 0xD9, 0xD0],
    ] {
        assert_exact_noncommitting_frontier(frontier);
    }
}
