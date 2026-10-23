//! Execute the production native scalar-memory helpers, including fault exits.

use super::*;
use crate::isa::arm::aarch64::{AArch64Config, AArch64Cpu};
use crate::isa::arm::common::cpu::ArmCpu;
use crate::isa::arm::common::memory::{ArmMemory, FlatMemory};

fn native_cpu(second_present: bool, alignment: bool) -> AArch64Cpu {
    let mut memory = FlatMemory::new(0, 0x100000);
    for (address, value) in [
        (0x10000, 0x20003),
        (0x20000, 0x30003),
        (0x30000, 0x40003),
        (0x40000, 0x80403),
        (0x40008, if second_present { 0xa0403 } else { 0 }),
    ] {
        memory.write_u64(address, value).unwrap();
    }
    let program = [
        0xf9400020u32, // LDR X0, [X1]
        0xf1000442,    // SUBS X2, X2, #1
        0x54ffffc1,    // B.NE 0
        0xd4200000,    // BRK #0: native frontier
    ];
    for (index, word) in program.into_iter().enumerate() {
        memory.write_u32(0x80000 + index as u64 * 4, word).unwrap();
    }
    memory.write(0x80ffc, &[0x11, 0x22, 0x33, 0x44]).unwrap();
    memory.write(0xa0000, &[0x55, 0x66, 0x77, 0x88]).unwrap();
    let mut cpu = AArch64Cpu::new(AArch64Config::default(), Box::new(memory));
    cpu.sysregs.el1.sctlr = 1 | if alignment { sctlr::A } else { 0 };
    cpu.sysregs.el1.ttbr0 = 0x10000;
    cpu.sysregs.el1.tcr = 16 | (16 << 16) | (2 << 30) | (5 << 32);
    cpu.sysregs.el1.mair = 0xff;
    cpu.update_mmu_config();
    cpu.set_jit_mem(true);
    cpu.set_pc(0);
    cpu.set_x(0, 0xfeedface);
    cpu.set_x(1, 0xffc);
    cpu.set_x(2, 1);
    cpu
}

#[test]
fn native_cross_page_load_uses_both_mappings() {
    let mut cpu = native_cpu(true, false);
    let region = cpu
        .jit_compile_region()
        .expect("scalar memory region admitted");
    assert!(cpu.jit_run_region(&region), "must execute native code");
    assert_eq!(cpu.get_x(0), 0x8877665544332211);
    assert_eq!(cpu.get_x(2), 0);
    assert_eq!(cpu.get_pc(), 12);
}

#[test]
fn native_load_fault_exits_preserve_destination_and_faulting_pc() {
    for (present, alignment, fault_type) in [
        (false, false, MemoryFaultType::Translation),
        (true, true, MemoryFaultType::Alignment),
    ] {
        let mut cpu = native_cpu(present, alignment);
        let region = cpu
            .jit_compile_region()
            .expect("scalar memory region admitted");
        assert!(
            cpu.jit_run_region(&region),
            "must execute native fault exit"
        );
        assert_eq!(cpu.get_pc(), 0);
        assert_eq!(cpu.get_x(0), 0xfeedface);
        assert_eq!(cpu.get_x(2), 1);
        assert!(matches!(cpu.step(), Err(ArmError::MemoryError(info))
            if info.fault_type == fault_type));
    }
}
