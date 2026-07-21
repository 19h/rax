//! Native-prefix/direct-frontier tests for x86 string port I/O.

use super::*;
use crate::vm::vcpu::{VCpu, VcpuExit};
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
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

#[test]
fn jit_runs_prefix_then_rep_insb_hands_off_each_element_at_exact_pc() {
    // ADD EAX,1; REP INSB; HLT.
    let memory = memory_with_code(&[0x83, 0xC0, 0x01, 0xF3, 0x6C, 0xF4]);
    let mut vcpu = test_vcpu(memory.clone());
    vcpu.regs.rax = 41;
    vcpu.regs.rcx = 2;
    vcpu.regs.rdx = 0x60;
    vcpu.regs.rdi = 0x2000;

    let region = vcpu
        .jit_compile_region()
        .expect("compile native prefix before REP INSB")
        .expect("supported prefix must remain JIT-eligible");
    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 3);
    assert_eq!(vcpu.regs.rax, 42);
    let flags_after_prefix = vcpu.regs.rflags;

    assert!(matches!(
        vcpu.step().expect("first REP INSB element"),
        Some(VcpuExit::IoIn {
            port: 0x60,
            size: 1,
        })
    ));
    assert_eq!(vcpu.regs.rip, 3, "one REP element remains");
    assert_eq!(vcpu.regs.rcx, 1);
    assert_eq!(vcpu.regs.rdi, 0x2001);
    assert_eq!(vcpu.regs.rflags, flags_after_prefix);
    vcpu.complete_io_in(&[0xA5]);

    assert!(matches!(
        vcpu.step().expect("final REP INSB element"),
        Some(VcpuExit::IoIn {
            port: 0x60,
            size: 1,
        })
    ));
    assert_eq!(vcpu.regs.rip, 5, "final element retires REP INSB");
    assert_eq!(vcpu.regs.rcx, 0);
    assert_eq!(vcpu.regs.rdi, 0x2002);
    assert_eq!(vcpu.regs.rflags, flags_after_prefix);
    vcpu.complete_io_in(&[0x5A]);

    let mut input = [0u8; 2];
    memory.read_slice(&mut input, GuestAddress(0x2000)).unwrap();
    assert_eq!(input, [0xA5, 0x5A]);
}

#[test]
fn jit_runs_prefix_then_fs_addr32_outsw_hands_off_without_speculation() {
    // ADD EBX,1; FS:addr32 OUTSW; HLT.
    let memory = memory_with_code(&[0x83, 0xC3, 0x01, 0x64, 0x67, 0x66, 0x6F, 0xF4]);
    memory
        .write_slice(&[0x34, 0x12], GuestAddress(0x3000))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rbx = 9;
    vcpu.regs.rdx = 0x03F8;
    vcpu.regs.rsi = 0xFFFF_FFFF_0000_2000;
    vcpu.sregs.fs.base = 0x1000;

    let region = vcpu
        .jit_compile_region()
        .expect("compile native prefix before FS:addr32 OUTSW")
        .expect("supported prefix must remain JIT-eligible");
    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 3);
    assert_eq!(vcpu.regs.rbx, 10);
    assert_eq!(vcpu.regs.rsi, 0xFFFF_FFFF_0000_2000);
    let flags_after_prefix = vcpu.regs.rflags;

    assert!(matches!(
        vcpu.step().expect("FS:addr32 OUTSW frontier"),
        Some(VcpuExit::IoOut { port: 0x03F8, data }) if data == [0x34, 0x12]
    ));
    assert_eq!(vcpu.regs.rip, 7);
    assert_eq!(vcpu.regs.rsi, 0x2002, "ESI update zero-extends into RSI");
    assert_eq!(vcpu.regs.rflags, flags_after_prefix);
}
