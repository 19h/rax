//! GS-base MSR writes affect architectural state, never guest RAM.
use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::{CpuState, MemAccess, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn check(index: u32, target: u64, canary: u64) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0f, 0x30, 0xf4], GuestAddress(0))
        .unwrap();
    for (address, value) in [(0x1000, 0x2007_u64), (0x2000, 0x3007), (0x3000, 0x4007)] {
        memory
            .write_slice(&value.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }
    for page in 0..16_u64 {
        memory
            .write_slice(
                &(page * 4096 | 7).to_le_bytes(),
                GuestAddress(0x4000 + page * 8),
            )
            .unwrap();
    }
    memory
        .write_slice(&canary.to_le_bytes(), GuestAddress(target))
        .unwrap();
    let mut cpu = X86_64Vcpu::new(0, memory.clone());
    let CpuState::X86_64(mut state) = cpu.get_state().unwrap() else {
        unreachable!()
    };
    state.sregs.cr0 = 0x8005_0033;
    state.sregs.cr3 = 0x1000;
    state.sregs.cr4 = 0x20;
    state.sregs.efer = 0xd00;
    state.sregs.cs.l = true;
    state.sregs.cs.selector = 0;
    state.regs.rip = 0;
    // This canonical base makes the former hard-coded per-CPU address alias
    // ordinary writable guest RAM. The base itself needs no mapped page.
    let base = target.wrapping_sub(0xffff_ffff_836e_e018);
    state.regs.rcx = u64::from(index);
    state.regs.rax = u64::from(base as u32);
    state.regs.rdx = base >> 32;
    cpu.set_state(&CpuState::X86_64(state)).unwrap();
    cpu.set_mem_recording(true);
    assert!(cpu.step().unwrap().is_none());
    let mut actual = [0; 8];
    memory
        .read_slice(&mut actual, GuestAddress(target))
        .unwrap();
    assert_eq!(
        u64::from_le_bytes(actual),
        canary,
        "MSR {index:x} target={target:x}"
    );
    let CpuState::X86_64(state) = cpu.get_state().unwrap() else {
        unreachable!()
    };
    let installed = match index {
        0xc0000100 => state.sregs.fs.base,
        0xc0000101 => state.sregs.gs.base,
        0xc0000102 => cpu.get_emulator_state().unwrap().kernel_gs_base,
        _ => unreachable!(),
    };
    assert_eq!(installed, base);
    assert_eq!(state.regs.rip, 2);
    let mut records = Vec::new();
    cpu.drain_mem_records(&mut records);
    assert!(
        records
            .iter()
            .all(|record| !matches!(record.access, MemAccess::Write))
    );
}

#[test]
fn gs_base_aliases_preserve_guest_memory_and_have_no_store_effect() {
    for target in [0x5000, 0x6000, 0x7000] {
        for value in [0, 0x200b0, 0x0123456789abcdef, u64::MAX] {
            check(0xc0000101, target, value);
        }
    }
}

#[test]
fn fs_base_and_kernel_gs_base_preserve_the_same_alias_canaries() {
    for index in [0xc0000100, 0xc0000102] {
        check(index, 0x6000, 0xfeedbeef_deadbabe);
    }
}
