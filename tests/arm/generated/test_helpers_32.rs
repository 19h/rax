//! A32/T32/T16 test helpers.
//!
//! Common test infrastructure for AArch32 instruction tests.
//! DO NOT EDIT MANUALLY.

#![allow(unused_imports)]
#![allow(dead_code)]

use rax::arm::{Armv7Config, Armv7Cpu, FlatMemory};

// Re-export types so tests can use them directly
pub use rax::arm::{ArmCpu, CpuExit};

/// Create a test CPU with default configuration (A32 mode)
pub fn create_test_cpu() -> Armv7Cpu {
    let memory = FlatMemory::new(0, 0x1000_0000);
    Armv7Cpu::new(Armv7Config::default(), Box::new(memory))
}

/// Create a test CPU in Thumb mode
pub fn create_thumb_cpu() -> Armv7Cpu {
    let memory = FlatMemory::new(0, 0x1000_0000);
    let mut cpu = Armv7Cpu::new(Armv7Config::default(), Box::new(memory));
    cpu.set_thumb(true);
    cpu
}

/// Write a 32-bit instruction to memory
pub fn write_insn(cpu: &mut Armv7Cpu, addr: u64, insn: u32) {
    cpu.write_memory(addr, &insn.to_le_bytes()).unwrap();
}

/// Write a 16-bit Thumb instruction to memory
pub fn write_insn16(cpu: &mut Armv7Cpu, addr: u64, insn: u16) {
    cpu.write_memory(addr, &insn.to_le_bytes()).unwrap();
}

/// Set a general purpose register (R0-R14)
pub fn set_w(cpu: &mut Armv7Cpu, reg: u8, value: u32) {
    cpu.set_gpr(reg, value);
}

/// Get a general purpose register (R0-R14)
pub fn get_w(cpu: &Armv7Cpu, reg: u8) -> u32 {
    cpu.get_gpr(reg)
}

/// Set the stack pointer (R13/SP)
pub fn set_sp(cpu: &mut Armv7Cpu, value: u32) {
    cpu.set_gpr(13, value);
}

/// Get the stack pointer (R13/SP)
pub fn get_sp(cpu: &Armv7Cpu) -> u32 {
    cpu.get_gpr(13)
}

/// Set the link register (R14/LR)
pub fn set_lr(cpu: &mut Armv7Cpu, value: u32) {
    cpu.set_gpr(14, value);
}

/// Get the link register (R14/LR)
pub fn get_lr(cpu: &Armv7Cpu) -> u32 {
    cpu.get_gpr(14)
}

/// Execute one instruction and return the exit status
pub fn step(cpu: &mut Armv7Cpu) -> CpuExit {
    cpu.step().unwrap()
}
