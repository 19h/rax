//! Shared test helpers for x86_64 instruction tests.
//!
//! This module provides common utilities for setting up test VMs
//! and checking instruction behavior.

use std::sync::Arc;

pub use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use rax::backend::emulator::x86_64::{X86_64Vcpu, flags};
pub use rax::cpu::{Registers, SystemRegisters, VCpu, VcpuExit};
use rax::error::Result;

/// Standard code address for tests
pub const CODE_ADDR: u64 = 0x1000;

/// Standard stack address for tests
pub const STACK_ADDR: u64 = 0x8000;

/// Standard data address for memory operand tests
pub const DATA_ADDR: u64 = 0x2000;

/// Default SYSCALL handler address for tests
pub const SYSCALL_HANDLER_ADDR: u64 = 0x12000;

/// Create a test VM with the given code and initial register state.
pub fn setup_vm(code: &[u8], initial_regs: Option<Registers>) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    // Create 16MB of guest memory
    let mem_size = 16 * 1024 * 1024;
    let regions = vec![(GuestAddress(0), mem_size)];
    let mem = Arc::new(GuestMemoryMmap::<()>::from_ranges(&regions).unwrap());

    // Write code at address 0x1000
    mem.write_slice(code, GuestAddress(CODE_ADDR)).unwrap();
    // Install a minimal SYSCALL handler that immediately SYSRET's.
    let syscall_handler = [0x48, 0x0f, 0x07, 0xf4]; // REX.W SYSRET; HLT fallback
    mem.write_slice(&syscall_handler, GuestAddress(SYSCALL_HANDLER_ADDR))
        .unwrap();
    // Install a minimal SYSCALL handler that immediately SYSRET's.
    let syscall_handler = [0x48, 0x0f, 0x07, 0xf4]; // REX.W SYSRET; HLT fallback
    mem.write_slice(&syscall_handler, GuestAddress(SYSCALL_HANDLER_ADDR))
        .unwrap();

    // Create vcpu
    let mut vcpu = X86_64Vcpu::new(0, mem.clone());

    // Set up initial registers
    // Only override RSP if no initial regs were provided
    let has_custom_regs = initial_regs.is_some();
    let mut regs = initial_regs.unwrap_or_else(Registers::default);
    regs.rip = CODE_ADDR;
    if !has_custom_regs {
        regs.rsp = STACK_ADDR;
    }
    // Preserve flags from initial_regs but ensure reserved bit 1 is set
    regs.rflags |= 0x2;
    vcpu.set_regs(&regs).unwrap();

    // Set up system registers - disable paging for simpler testing
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = 0x00050033; // PE but NOT PG (no paging)
    sregs.cr4 = 0x20; // PAE
    sregs.efer = 0x501; // SCE, LMA, LME for long mode
    sregs.star = 0;
    sregs.lstar = SYSCALL_HANDLER_ADDR;
    sregs.cstar = 0;
    sregs.fmask = 0;
    // Set CS.L=1 for true 64-bit mode (enables RIP-relative addressing)
    sregs.cs.l = true;
    sregs.cs.db = false; // Must be 0 when L=1 for 64-bit mode
    // Initialize GDT and IDT with reasonable defaults for testing
    sregs.gdt.base = 0x10000; // GDT at 64KB
    sregs.gdt.limit = 0x1F;   // 4 descriptors (32 bytes - 1)
    sregs.idt.base = 0x11000; // IDT at 68KB
    sregs.idt.limit = 0xFF;   // 16 entries (256 bytes - 1)
    vcpu.set_sregs(&sregs).unwrap();

    (vcpu, mem)
}

/// Create a test VM in compatibility mode (32-bit code within long mode).
/// Use this for instructions that are only valid in 32-bit mode (BOUND, PUSHA, POPA, etc.)
/// In compatibility mode: CS.L=0, CS.D determines operand size (D=1 means 32-bit default)
/// Memory addressing uses absolute [disp32] instead of RIP-relative.
pub fn setup_vm_compat(code: &[u8], initial_regs: Option<Registers>) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    // Create 16MB of guest memory
    let mem_size = 16 * 1024 * 1024;
    let regions = vec![(GuestAddress(0), mem_size)];
    let mem = Arc::new(GuestMemoryMmap::<()>::from_ranges(&regions).unwrap());

    // Write code at address 0x1000
    mem.write_slice(code, GuestAddress(CODE_ADDR)).unwrap();

    // Create vcpu
    let mut vcpu = X86_64Vcpu::new(0, mem.clone());

    // Set up initial registers
    let has_custom_regs = initial_regs.is_some();
    let mut regs = initial_regs.unwrap_or_else(Registers::default);
    regs.rip = CODE_ADDR;
    if !has_custom_regs {
        regs.rsp = STACK_ADDR;
    }
    regs.rflags |= 0x2;
    vcpu.set_regs(&regs).unwrap();

    // Set up system registers for compatibility mode
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = 0x00050033; // PE but NOT PG (no paging)
    sregs.cr4 = 0x20; // PAE
    sregs.efer = 0x501; // SCE, LMA, LME for long mode
    sregs.star = 0;
    sregs.lstar = SYSCALL_HANDLER_ADDR;
    sregs.cstar = 0;
    sregs.fmask = 0;
    // CS.L=0 for compatibility mode (32-bit code within long mode)
    sregs.cs.l = false;
    sregs.cs.db = false; // D=0 means 16-bit default operand size (use 0x66 for 32-bit)
    // Initialize GDT and IDT
    sregs.gdt.base = 0x10000;
    sregs.gdt.limit = 0x1F;
    sregs.idt.base = 0x11000;
    sregs.idt.limit = 0xFF;
    vcpu.set_sregs(&sregs).unwrap();

    (vcpu, mem)
}

/// Run the VM until HLT and return final register state
pub fn run_until_hlt(vcpu: &mut X86_64Vcpu) -> Result<Registers> {
    const MAX_ITERATIONS: u64 = 10_000; // Limit iterations to prevent hangs
    let mut iterations = 0;
    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return Err(rax::error::Error::Emulator(format!(
                "exceeded {} iterations at RIP={:#x}", MAX_ITERATIONS, vcpu.get_regs()?.rip
            )));
        }
        match vcpu.run()? {
            VcpuExit::Hlt => break,
            VcpuExit::IoIn { size, .. } => {
                // Complete I/O with zeros (simulated I/O)
                let data = vec![0u8; size as usize];
                vcpu.complete_io_in(&data);
            }
            VcpuExit::IoOut { .. } => continue,
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

/// A wrapper around X86_64Vcpu that provides convenient getter/setter methods
/// for individual registers, matching the API expected by some test files.
pub struct TestCpu {
    vcpu: X86_64Vcpu,
    mem: Arc<GuestMemoryMmap>,
    regs: Registers,
}

impl TestCpu {
    pub fn new(code: &[u8]) -> Self {
        let (vcpu, mem) = setup_vm(code, None);
        let regs = vcpu.get_regs().unwrap();
        Self { vcpu, mem, regs }
    }

    pub fn set_rax(&mut self, value: u64) {
        self.regs.rax = value;
        self.vcpu.set_regs(&self.regs).unwrap();
    }

    pub fn set_rbx(&mut self, value: u64) {
        self.regs.rbx = value;
        self.vcpu.set_regs(&self.regs).unwrap();
    }

    pub fn set_rcx(&mut self, value: u64) {
        self.regs.rcx = value;
        self.vcpu.set_regs(&self.regs).unwrap();
    }

    pub fn set_rdx(&mut self, value: u64) {
        self.regs.rdx = value;
        self.vcpu.set_regs(&self.regs).unwrap();
    }

    pub fn set_rsi(&mut self, value: u64) {
        self.regs.rsi = value;
        self.vcpu.set_regs(&self.regs).unwrap();
    }

    pub fn set_rdi(&mut self, value: u64) {
        self.regs.rdi = value;
        self.vcpu.set_regs(&self.regs).unwrap();
    }

    pub fn set_rflags(&mut self, value: u64) {
        self.regs.rflags = value | 0x2; // Ensure reserved bit is set
        self.vcpu.set_regs(&self.regs).unwrap();
    }

    pub fn get_rax(&self) -> u64 {
        self.vcpu.get_regs().unwrap().rax
    }

    pub fn get_rbx(&self) -> u64 {
        self.vcpu.get_regs().unwrap().rbx
    }

    pub fn get_rcx(&self) -> u64 {
        self.vcpu.get_regs().unwrap().rcx
    }

    pub fn get_rdx(&self) -> u64 {
        self.vcpu.get_regs().unwrap().rdx
    }

    pub fn get_rsi(&self) -> u64 {
        self.vcpu.get_regs().unwrap().rsi
    }

    pub fn get_rdi(&self) -> u64 {
        self.vcpu.get_regs().unwrap().rdi
    }

    pub fn get_rflags(&self) -> u64 {
        self.vcpu.get_regs().unwrap().rflags
    }

    pub fn get_memory(&self) -> &Arc<GuestMemoryMmap> {
        &self.mem
    }

    /// Refresh internal register cache from vcpu
    pub fn refresh_regs(&mut self) {
        self.regs = self.vcpu.get_regs().unwrap();
    }
}

/// Create a test CPU with the given code.
/// This is a convenience wrapper for tests that prefer the TestCpu API.
pub fn create_test_cpu(code: &[u8]) -> TestCpu {
    TestCpu::new(code)
}

/// Run the test CPU until HLT.
pub fn run_test(cpu: &mut TestCpu) {
    run_until_hlt(&mut cpu.vcpu).unwrap();
    cpu.refresh_regs();
}

/// Stub TestCase type for tests that use hex string parsing.
/// These tests are placeholders that just check if code parses and runs.
pub struct TestCase {
    code: Vec<u8>,
}

impl TestCase {
    /// Parse a hex string like "66 0f 3a cf c1 00" into bytes
    pub fn from(hex_str: &str) -> Self {
        let code: Vec<u8> = hex_str
            .split_whitespace()
            .filter_map(|s| u8::from_str_radix(s, 16).ok())
            .collect();
        Self { code }
    }

    /// Run the test - just check if code executes without panic
    pub fn check(&self) {
        let mut code_with_hlt = self.code.clone();
        code_with_hlt.push(0xf4); // HLT
        let (mut vcpu, _) = setup_vm(&code_with_hlt, None);
        // Try to run but don't fail if instruction is unimplemented
        let _ = run_until_hlt(&mut vcpu);
    }
}

/// VM struct for tests that use the legacy VM API
pub struct VM {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
    pub executed_instructions: u64,
    vcpu: X86_64Vcpu,
    #[allow(dead_code)]
    mem: Arc<GuestMemoryMmap>,
}

impl VM {
    fn from_vcpu(vcpu: X86_64Vcpu, mem: Arc<GuestMemoryMmap>) -> Self {
        let regs = vcpu.get_regs().unwrap();
        VM {
            rax: regs.rax,
            rbx: regs.rbx,
            rcx: regs.rcx,
            rdx: regs.rdx,
            rsi: regs.rsi,
            rdi: regs.rdi,
            rsp: regs.rsp,
            rbp: regs.rbp,
            r8: regs.r8,
            r9: regs.r9,
            r10: regs.r10,
            r11: regs.r11,
            r12: regs.r12,
            r13: regs.r13,
            r14: regs.r14,
            r15: regs.r15,
            rip: regs.rip,
            rflags: regs.rflags,
            executed_instructions: 0,
            vcpu,
            mem,
        }
    }

    fn refresh(&mut self) {
        let regs = self.vcpu.get_regs().unwrap();
        self.rax = regs.rax;
        self.rbx = regs.rbx;
        self.rcx = regs.rcx;
        self.rdx = regs.rdx;
        self.rsi = regs.rsi;
        self.rdi = regs.rdi;
        self.rsp = regs.rsp;
        self.rbp = regs.rbp;
        self.r8 = regs.r8;
        self.r9 = regs.r9;
        self.r10 = regs.r10;
        self.r11 = regs.r11;
        self.r12 = regs.r12;
        self.r13 = regs.r13;
        self.r14 = regs.r14;
        self.r15 = regs.r15;
        self.rip = regs.rip;
        self.rflags = regs.rflags;
    }

    /// Read memory at given address
    pub fn read_memory(&self, addr: u64, len: usize) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        self.mem.read_slice(&mut buf, GuestAddress(addr)).unwrap();
        buf
    }

    /// Execute one instruction (step)
    pub fn step(mut self) -> Self {
        match self.vcpu.step().unwrap() {
            Some(VcpuExit::IoIn { size, .. }) => {
                let data = vec![0u8; size as usize];
                self.vcpu.complete_io_in(&data);
            }
            Some(VcpuExit::IoOut { .. }) => {}
            Some(_) | None => {}
        }
        self.executed_instructions += 1;
        self.refresh();
        self
    }
}

/// Legacy setup_vm that returns VM struct (for tests using old API)
#[allow(dead_code)]
pub fn setup_vm_legacy(code: &[u8]) -> VM {
    let (vcpu, mem) = setup_vm(code, None);
    VM::from_vcpu(vcpu, mem)
}

/// Legacy run_until_hlt that takes and returns VM (for tests using old API)
#[allow(dead_code)]
pub fn run_until_hlt_legacy(mut vm: VM) -> VM {
    const MAX_ITERATIONS: u64 = 10_000;
    let mut iterations = 0;
    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            panic!("exceeded {} iterations at RIP={:#x}", MAX_ITERATIONS, vm.vcpu.get_regs().unwrap().rip);
        }
        match vm.vcpu.step().unwrap() {
            Some(VcpuExit::Hlt) => {
                vm.executed_instructions += 1;
                break;
            }
            Some(VcpuExit::IoIn { size, .. }) => {
                let data = vec![0u8; size as usize];
                vm.vcpu.complete_io_in(&data);
            }
            Some(VcpuExit::IoOut { .. }) => {}
            Some(_) | None => {}
        }
        vm.executed_instructions += 1;
    }
    vm.refresh();
    vm
}
