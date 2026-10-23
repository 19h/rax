//! Cached native regions must obey current instruction-fetch permissions.

use super::X86_64Vcpu;
use crate::error::Error;
use crate::vm::vcpu::{SystemRegisters, VCpu, VcpuExit};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const ENTRY: u64 = 0x8000;
const LEAF: u64 = 0x4000 + (ENTRY >> 12) * 8;

fn cached_cpu() -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x40000)]).unwrap());
    for (address, value) in [(0x1000, 0x2003u64), (0x2000, 0x3003), (0x3000, 0x4003)] {
        memory
            .write_obj(value.to_le(), GuestAddress(address))
            .unwrap();
    }
    for page in 0..64u64 {
        memory
            .write_obj((page * 0x1000 | 3).to_le(), GuestAddress(0x4000 + page * 8))
            .unwrap();
    }
    // MOV EAX,2; JNZ ENTRY (untaken with seeded ZF); HLT frontier.
    memory
        .write_slice(&[0xb8, 2, 0, 0, 0, 0x75, 0xf9, 0xf4], GuestAddress(ENTRY))
        .unwrap();
    let mut cpu = X86_64Vcpu::new(0, memory.clone());
    cpu.sregs = SystemRegisters::default();
    cpu.sregs.cr0 = (1 << 31) | (1 << 16) | 1;
    cpu.sregs.cr3 = 0x1000;
    cpu.sregs.cr4 = 1 << 5;
    cpu.sregs.efer = (1 << 8) | (1 << 10) | (1 << 11);
    cpu.sregs.cs.l = true;
    cpu.sregs.cs.selector = 8;
    cpu.sregs.cs.type_ = 11;
    cpu.sregs.cs.s = true;
    cpu.sregs.cs.present = true;
    cpu.sregs.ss.selector = 16;
    cpu.sregs.ss.type_ = 3;
    cpu.sregs.ss.s = true;
    cpu.sregs.ss.present = true;
    cpu.regs.rip = ENTRY;
    cpu.regs.rsp = 0x30000;
    cpu.regs.rflags = 0x42;
    let region = cpu
        .jit_compile_region()
        .unwrap()
        .expect("native scalar loop");
    cpu.jit_cache
        .insert((ENTRY, cpu.jit_mode_tag()), Some(Arc::new(region)));
    (cpu, memory)
}

#[test]
fn cached_native_region_cannot_execute_after_its_source_becomes_nx() {
    let (mut cpu, memory) = cached_cpu();
    memory
        .write_obj((ENTRY | 3 | (1u64 << 63)).to_le(), GuestAddress(LEAF))
        .unwrap();
    cpu.mmu.flush_tlb();
    // No exception handler is installed. The run loop must attempt #PF delivery,
    // rather than execute the previously cached MOV and reach HLT.
    let result = cpu.run();
    assert!(
        matches!(result, Err(Error::FaultDelivery { .. })),
        "{result:?}"
    );
    assert_eq!(cpu.regs.rax, 0);
}

#[test]
fn read_only_source_pages_remain_executable_by_cached_native_regions() {
    let (mut cpu, memory) = cached_cpu();
    memory
        .write_obj((ENTRY | 1).to_le(), GuestAddress(LEAF))
        .unwrap();
    cpu.mmu.flush_tlb();
    assert!(matches!(cpu.run().unwrap(), VcpuExit::Hlt));
    assert_eq!(cpu.regs.rax, 2);
}

#[test]
fn protected_paging_rejects_new_and_cached_native_regions_at_the_current_frontier() {
    for control in [1_u64 << 21, 1 << 22, (1 << 21) | (1 << 22)] {
        let (mut cpu, _) = cached_cpu();
        let key = (ENTRY, cpu.jit_mode_tag());
        let region = cpu.jit_cache.get(&key).unwrap().as_ref().unwrap().clone();
        cpu.sregs.cr4 |= control;
        let before = cpu.get_state().unwrap();
        assert!(!cpu.jit_source_pages_accessible(&region));
        assert!(cpu.jit_compile_region().unwrap().is_none());
        assert!(!cpu.jit_try_block().unwrap());
        assert_eq!(
            format!("{:?}", cpu.get_state().unwrap()),
            format!("{before:?}")
        );
        assert!(matches!(cpu.run().unwrap(), VcpuExit::Hlt));
        assert_eq!(cpu.regs.rax, 2);
    }
}

#[test]
fn disabling_protected_paging_controls_reenables_the_same_cached_region() {
    let (mut cpu, _) = cached_cpu();
    let key = (ENTRY, cpu.jit_mode_tag());
    let region = cpu.jit_cache.get(&key).unwrap().as_ref().unwrap().clone();
    cpu.sregs.cr4 |= (1 << 21) | (1 << 22);
    assert!(!cpu.jit_source_pages_accessible(&region));
    cpu.sregs.cr4 &= !((1 << 21) | (1 << 22));
    assert!(cpu.jit_source_pages_accessible(&region));
    assert!(cpu.jit_try_block().unwrap());
    assert_eq!(cpu.regs.rax, 2);
}
