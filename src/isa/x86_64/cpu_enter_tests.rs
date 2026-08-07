//! Direct x86 ENTER transaction, prefix-order, and fault-frontier coverage.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn long_mode_vcpu(code: &[u8]) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory.clone());
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0xAD7;
    (vcpu, memory)
}

#[test]
fn direct_enter_rex_order_width_nesting_and_flags_are_exact() {
    for (name, code, expected_delta, expected_rbp) in [
        ("default W64", &[0xC8, 0x20, 0, 0][..], 8 + 0x20, 0x7FF8),
        (
            "66 selects W16",
            &[0x66, 0xC8, 0x20, 0, 0],
            2 + 0x20,
            0x7FFE,
        ),
        (
            "REX.W after 66 selects W64",
            &[0x66, 0x48, 0xC8, 0x20, 0, 0],
            8 + 0x20,
            0x7FF8,
        ),
        (
            "66 after REX invalidates REX and selects W16",
            &[0x48, 0x66, 0xC8, 0x20, 0, 0],
            2 + 0x20,
            0x7FFE,
        ),
    ] {
        let (mut vcpu, _) = long_mode_vcpu(code);
        assert!(vcpu.step().unwrap().is_none(), "{name}");
        assert_eq!(vcpu.regs.rsp, 0x8000 - expected_delta, "{name}");
        assert_eq!(vcpu.regs.rbp, expected_rbp, "{name}");
        assert_eq!(vcpu.regs.rflags, 0xAD7, "{name}");
    }

    let (mut nested, memory) = long_mode_vcpu(&[0xC8, 0, 0, 3]);
    memory
        .write_obj(0x1111_2222_3333_4444_u64, GuestAddress(0x6FF8))
        .unwrap();
    memory
        .write_obj(0x5555_6666_7777_8888_u64, GuestAddress(0x6FF0))
        .unwrap();
    nested.step().unwrap();
    assert_eq!(nested.regs.rsp, 0x7FE0);
    assert_eq!(nested.regs.rbp, 0x7FF8);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FF8)).unwrap(),
        0x7000
    );
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FF0)).unwrap(),
        0x1111_2222_3333_4444
    );
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FE8)).unwrap(),
        0x5555_6666_7777_8888
    );
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FE0)).unwrap(),
        0x7FF8
    );
}

fn sparse_fault_vcpu(code: &[u8]) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory = Arc::new(
        GuestMemoryMmap::<()>::from_ranges(&[
            (GuestAddress(0), 0x100),
            (GuestAddress(0x700), 0x100),
        ])
        .unwrap(),
    );
    memory.write_slice(code, GuestAddress(0)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory.clone());
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rsp = 0x800;
    vcpu.regs.rbp = 0x600;
    (vcpu, memory)
}

#[test]
fn direct_enter_final_probe_and_late_read_fault_leave_registers_restartable() {
    let (mut final_fault, memory) = sparse_fault_vcpu(&[0xC8, 0, 1, 0]);
    assert!(final_fault.step().is_err());
    assert_eq!(final_fault.regs.rip, 0);
    assert_eq!(final_fault.regs.rsp, 0x800);
    assert_eq!(final_fault.regs.rbp, 0x600);
    assert_eq!(memory.read_obj::<u64>(GuestAddress(0x7F8)).unwrap(), 0x600);

    let (mut read_fault, memory) = sparse_fault_vcpu(&[0xC8, 0, 0, 2]);
    assert!(read_fault.step().is_err());
    assert_eq!(read_fault.regs.rip, 0);
    assert_eq!(read_fault.regs.rsp, 0x800);
    assert_eq!(read_fault.regs.rbp, 0x600);
    assert_eq!(memory.read_obj::<u64>(GuestAddress(0x7F8)).unwrap(), 0x600);
}

#[test]
fn direct_enter_noncanonical_stack_range_raises_ss_before_memory_access() {
    let (mut vcpu, _) = long_mode_vcpu(&[0xC8, 0, 0, 0]);
    vcpu.regs.rsp = 0x0000_8000_0000_0008;
    let before = vcpu.regs.clone();
    let error = vcpu
        .step()
        .expect_err("noncanonical ENTER stack range must raise #SS(0)");
    assert!(
        error.to_string().contains("IDT entry 12 not present"),
        "expected #SS(0), got {error}"
    );
    assert_eq!(vcpu.regs.rip, before.rip);
    assert_eq!(vcpu.regs.rsp, before.rsp);
    assert_eq!(vcpu.regs.rbp, before.rbp);
}

#[test]
fn compatibility_enter_uses_ss_base_for_display_reads_and_stack_writes() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x3000)]).unwrap());
    memory
        .write_slice(&[0xC8, 0, 0, 2], GuestAddress(0))
        .unwrap();
    memory
        .write_obj(0xAABB_CCDD_u32, GuestAddress(0x16FC))
        .unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory.clone());
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = false;
    vcpu.sregs.cs.db = true;
    vcpu.sregs.ss.db = true;
    vcpu.sregs.ss.base = 0x1000;
    vcpu.regs.rsp = 0x800;
    vcpu.regs.rbp = 0x700;

    vcpu.step().unwrap();
    assert_eq!(vcpu.regs.rsp, 0x7F4);
    assert_eq!(vcpu.regs.rbp, 0x7FC);
    assert_eq!(memory.read_obj::<u32>(GuestAddress(0x17FC)).unwrap(), 0x700);
    assert_eq!(
        memory.read_obj::<u32>(GuestAddress(0x17F8)).unwrap(),
        0xAABB_CCDD
    );
    assert_eq!(memory.read_obj::<u32>(GuestAddress(0x17F4)).unwrap(), 0x7FC);
}
