//! Shared test helpers for x86_64 instruction tests.
//!
//! This module provides common utilities for setting up test VMs
//! and checking instruction behavior.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use rax::backend::emulator::x86_64::{X86_64Vcpu, flags};
use rax::cpu::{Registers, SystemRegisters, VCpu, VcpuExit};
use rax::error::Result;

/// Standard code address for tests
pub const CODE_ADDR: u64 = 0x1000;

/// Standard stack address for tests
pub const STACK_ADDR: u64 = 0x8000;

/// Standard data address for memory operand tests
pub const DATA_ADDR: u64 = 0x2000;

/// Create a test VM with the given code and initial register state.
pub fn setup_vm(code: &[u8], initial_regs: Option<Registers>) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    // Create 16MB of guest memory
    let mem_size = 16 * 1024 * 1024;
    let regions = vec![(GuestAddress(0), mem_size)];
    let mem = Arc::new(GuestMemoryMmap::<()>::from_ranges(&regions).unwrap());

    // Write code at address 0x1000
    mem.write_slice(code, GuestAddress(CODE_ADDR)).unwrap();

    // Create vcpu
    let mut vcpu = X86_64Vcpu::new(0, mem.clone());

    // Set up initial registers
    let mut regs = initial_regs.unwrap_or_else(Registers::default);
    regs.rip = CODE_ADDR;
    regs.rsp = STACK_ADDR;
    // Preserve flags from initial_regs but ensure reserved bit 1 is set
    regs.rflags |= 0x2;
    vcpu.set_regs(&regs).unwrap();

    // Set up system registers - disable paging for simpler testing
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = 0x00050033; // PE but NOT PG (no paging)
    sregs.cr4 = 0x20; // PAE
    sregs.efer = 0x500; // LMA, LME for long mode
    vcpu.set_sregs(&sregs).unwrap();

    (vcpu, mem)
}

/// Run the VM until HLT and return final register state
pub fn run_until_hlt(vcpu: &mut X86_64Vcpu) -> Result<Registers> {
    loop {
        match vcpu.run()? {
            VcpuExit::Hlt => break,
            VcpuExit::IoIn { .. } | VcpuExit::IoOut { .. } => continue,
            _ => continue,
        }
    }
    vcpu.get_regs()
}

/// Check if a specific flag is set
#[inline]
pub fn flag_set(rflags: u64, flag: u64) -> bool {
    (rflags & flag) != 0
}

/// Check if carry flag is set
#[inline]
pub fn cf_set(rflags: u64) -> bool {
    flag_set(rflags, flags::bits::CF)
}

/// Check if zero flag is set
#[inline]
pub fn zf_set(rflags: u64) -> bool {
    flag_set(rflags, flags::bits::ZF)
}

/// Check if sign flag is set
#[inline]
pub fn sf_set(rflags: u64) -> bool {
    flag_set(rflags, flags::bits::SF)
}

/// Check if overflow flag is set
#[inline]
pub fn of_set(rflags: u64) -> bool {
    flag_set(rflags, flags::bits::OF)
}

/// Check if parity flag is set
#[inline]
pub fn pf_set(rflags: u64) -> bool {
    flag_set(rflags, flags::bits::PF)
}

/// Check if auxiliary carry flag is set
#[inline]
pub fn af_set(rflags: u64) -> bool {
    flag_set(rflags, flags::bits::AF)
}

/// Check if direction flag is set
#[inline]
pub fn df_set(rflags: u64) -> bool {
    flag_set(rflags, flags::bits::DF)
}

/// Write a value to memory at DATA_ADDR
pub fn write_mem_u8(mem: &GuestMemoryMmap, value: u8) {
    mem.write_slice(&[value], GuestAddress(DATA_ADDR)).unwrap();
}

pub fn write_mem_u16(mem: &GuestMemoryMmap, value: u16) {
    mem.write_slice(&value.to_le_bytes(), GuestAddress(DATA_ADDR)).unwrap();
}

pub fn write_mem_u32(mem: &GuestMemoryMmap, value: u32) {
    mem.write_slice(&value.to_le_bytes(), GuestAddress(DATA_ADDR)).unwrap();
}

pub fn write_mem_u64(mem: &GuestMemoryMmap, value: u64) {
    mem.write_slice(&value.to_le_bytes(), GuestAddress(DATA_ADDR)).unwrap();
}

/// Write a value to memory at a specific address
pub fn write_mem_at_u8(mem: &GuestMemoryMmap, addr: u64, value: u8) {
    mem.write_slice(&[value], GuestAddress(addr)).unwrap();
}

pub fn write_mem_at_u16(mem: &GuestMemoryMmap, addr: u64, value: u16) {
    mem.write_slice(&value.to_le_bytes(), GuestAddress(addr)).unwrap();
}

pub fn write_mem_at_u32(mem: &GuestMemoryMmap, addr: u64, value: u32) {
    mem.write_slice(&value.to_le_bytes(), GuestAddress(addr)).unwrap();
}

pub fn write_mem_at_u64(mem: &GuestMemoryMmap, addr: u64, value: u64) {
    mem.write_slice(&value.to_le_bytes(), GuestAddress(addr)).unwrap();
}

/// Read a value from memory at DATA_ADDR
pub fn read_mem_u8(mem: &GuestMemoryMmap) -> u8 {
    let mut buf = [0u8; 1];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    buf[0]
}

pub fn read_mem_u16(mem: &GuestMemoryMmap) -> u16 {
    let mut buf = [0u8; 2];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    u16::from_le_bytes(buf)
}

pub fn read_mem_u32(mem: &GuestMemoryMmap) -> u32 {
    let mut buf = [0u8; 4];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    u32::from_le_bytes(buf)
}

pub fn read_mem_u64(mem: &GuestMemoryMmap) -> u64 {
    let mut buf = [0u8; 8];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    u64::from_le_bytes(buf)
}

/// Read a value from memory at a specific address
pub fn read_mem_at_u8(mem: &GuestMemoryMmap, addr: u64) -> u8 {
    let mut buf = [0u8; 1];
    mem.read_slice(&mut buf, GuestAddress(addr)).unwrap();
    buf[0]
}

pub fn read_mem_at_u16(mem: &GuestMemoryMmap, addr: u64) -> u16 {
    let mut buf = [0u8; 2];
    mem.read_slice(&mut buf, GuestAddress(addr)).unwrap();
    u16::from_le_bytes(buf)
}

pub fn read_mem_at_u32(mem: &GuestMemoryMmap, addr: u64) -> u32 {
    let mut buf = [0u8; 4];
    mem.read_slice(&mut buf, GuestAddress(addr)).unwrap();
    u32::from_le_bytes(buf)
}

pub fn read_mem_at_u64(mem: &GuestMemoryMmap, addr: u64) -> u64 {
    let mut buf = [0u8; 8];
    mem.read_slice(&mut buf, GuestAddress(addr)).unwrap();
    u64::from_le_bytes(buf)
}
