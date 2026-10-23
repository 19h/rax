//! Native lift and cached execution must use instruction-fetch permissions.

use super::*;
use crate::isa::arm::common::memory::StandardMemory;

const CODE: [u32; 4] = [
    0xd2824680, // MOVZ X0,#0x1234
    0xf1000421, // SUBS X1,X1,#1
    0x54ffffc1, // B.NE 0
    0xd4200000, // BRK #0: native frontier
];

fn paged_cpu(el: u8, leaf: u64) -> AArch64Cpu {
    let mut memory = FlatMemory::new(0, 0x100000);
    for (address, value) in [
        (0x10000, 0x20003),
        (0x20000, 0x30003),
        (0x30000, 0x40003),
        (0x40000, leaf),
    ] {
        memory.write_u64(address, value).unwrap();
    }
    for (index, word) in CODE.iter().enumerate() {
        memory.write_u32(0x80000 + index as u64 * 4, *word).unwrap();
    }
    let config = AArch64Config {
        initial_el: el,
        ..AArch64Config::default()
    };
    let mut cpu = AArch64Cpu::new(config, Box::new(memory));
    cpu.sysregs.el1.sctlr = 1;
    cpu.sysregs.el1.ttbr0 = 0x10000;
    cpu.sysregs.el1.tcr = 16 | (16 << 16) | (2 << 30) | (5 << 32);
    cpu.sysregs.el1.mair = 0xff;
    cpu.update_mmu_config();
    cpu.set_pc(0);
    cpu.set_x(0, 0xfeedface);
    cpu.set_x(1, 1);
    cpu
}

#[test]
fn native_lifting_cannot_read_code_through_data_permissions() {
    for (el, leaf) in [
        (1, 0x80403 | (1 << 53)),
        (1, 0x80443),
        (0, 0x80443 | (1 << 54)),
    ] {
        let mut cpu = paged_cpu(el, leaf);
        assert!(cpu.jit_compile_region().is_none(), "EL{el} leaf={leaf:#x}");
        assert!(matches!(cpu.step(), Err(ArmError::MemoryError(info))
            if info.access == crate::isa::arm::common::cpu::AccessType::InstructionFetch));
        assert_eq!(cpu.get_pc(), 0);
        assert_eq!(cpu.get_x(0), 0xfeedface);
    }
}

#[test]
fn cached_native_regions_observe_later_execute_permission_changes() {
    let mut cpu = paged_cpu(1, 0x80403);
    let region = cpu.jit_compile_region().expect("executable scalar loop");
    cpu.memory.write_u64(0x40000, 0x80403 | (1 << 53)).unwrap();
    assert!(!cpu.jit_run_region(&region));
    assert_eq!(cpu.get_pc(), 0);
    assert_eq!(cpu.get_x(0), 0xfeedface);
    assert_eq!(cpu.get_x(1), 1);
    cpu.step_system().unwrap();
    assert_eq!((cpu.sysregs.el1.esr >> 26) & 0x3f, 0x21);
    assert_eq!(cpu.sysregs.el1.elr, 0);
    assert_eq!(cpu.sysregs.el1.far, 0);
}

#[test]
fn native_code_remains_executable_on_a_readonly_supervisor_page() {
    let mut cpu = paged_cpu(1, 0x80483);
    let region = cpu
        .jit_compile_region()
        .expect("readonly executable scalar loop");
    assert!(cpu.jit_run_region(&region));
    assert_eq!(cpu.get_pc(), 12);
    assert_eq!(cpu.get_x(0), 0x1234);
    assert_eq!(cpu.get_x(1), 0);
}

#[test]
fn backing_fetch_failures_preserve_instruction_intent_and_virtual_pc() {
    let mut cpu = AArch64Cpu::new(
        AArch64Config::default(),
        Box::new(FlatMemory::new(0, 0x1000)),
    );
    cpu.set_pc(0x2000);
    assert!(matches!(cpu.step(), Err(ArmError::MemoryError(info))
        if info.address == 0x2000 && info.access == crate::isa::arm::common::cpu::AccessType::InstructionFetch));
    let mut cpu = paged_cpu(1, 0x200403);
    assert!(matches!(cpu.step(), Err(ArmError::MemoryError(info))
        if info.address == 0
            && info.access == crate::isa::arm::common::cpu::AccessType::InstructionFetch
            && info.fault_type == MemoryFaultType::External));
}

#[test]
fn native_instruction_bytes_do_not_follow_data_byte_order() {
    let mut memory = StandardMemory::with_ram(0, 0x10000);
    memory.set_big_endian(true);
    let bytes: Vec<_> = CODE.into_iter().flat_map(u32::to_le_bytes).collect();
    memory.write(0, &bytes).unwrap();
    let mut cpu = AArch64Cpu::new(AArch64Config::default(), Box::new(memory));
    cpu.set_x(1, 1);
    let region = cpu
        .jit_compile_region()
        .expect("instructions remain little endian");
    assert!(cpu.jit_run_region(&region));
    assert_eq!(cpu.get_pc(), 12);
    assert_eq!(cpu.get_x(0), 0x1234);
}

#[test]
fn cached_regions_reject_changed_physical_source_mappings() {
    let mut cpu = paged_cpu(1, 0x80403);
    let region = cpu.jit_compile_region().expect("original code mapping");
    cpu.memory.write_u32(0xa0000, 0xd2886420).unwrap(); // MOVZ X0,#0x4321
    cpu.memory.write_u64(0x40000, 0xa0403).unwrap();
    assert!(!cpu.jit_run_region(&region));
    assert_eq!(cpu.get_pc(), 0);
    assert_eq!(cpu.get_x(0), 0xfeedface);
    cpu.step().unwrap();
    assert_eq!(cpu.get_x(0), 0x4321);
}

#[test]
fn second_source_page_denial_defers_fault_delivery_until_that_instruction() {
    let mut cpu = paged_cpu(1, 0x80403);
    cpu.memory.write_u64(0x40008, 0xa0403).unwrap();
    for (index, word) in CODE.iter().enumerate() {
        let va = 0xff8 + index as u64 * 4;
        let pa = if va < 0x1000 {
            0x80000 + va
        } else {
            0xa0000 + va - 0x1000
        };
        cpu.memory.write_u32(pa, *word).unwrap();
    }
    cpu.set_pc(0xff8);
    let region = cpu
        .jit_compile_region()
        .expect("code spanning two mapped pages");
    cpu.memory.write_u64(0x40008, 0xa0403 | (1 << 53)).unwrap();
    assert!(!cpu.jit_run_region(&region));
    assert_eq!(cpu.get_pc(), 0xff8);
    cpu.step().unwrap();
    cpu.step().unwrap();
    assert_eq!(cpu.get_pc(), 0x1000);
    assert!(matches!(cpu.step(), Err(ArmError::MemoryError(info))
        if info.address == 0x1000 && info.access == crate::isa::arm::common::cpu::AccessType::InstructionFetch));
}
