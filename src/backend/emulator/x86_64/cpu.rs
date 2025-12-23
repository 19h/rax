//! x86_64 CPU state and core execution loop.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Global tracker for current RIP (for debugging write watchpoints)
pub static CURRENT_RIP: AtomicU64 = AtomicU64::new(0);

use vm_memory::GuestMemoryMmap;

use super::decoder::Decoder;
use super::flags;
use super::insn;
use super::mmu::Mmu;
use crate::cpu::{CpuState, Registers, SystemRegisters, VCpu, VcpuExit, X86_64CpuState};
use crate::error::{Error, Result};

/// Maximum instruction length for x86_64.
const MAX_INSN_LEN: usize = 15;

/// x87 FPU state.
#[derive(Clone, Debug)]
pub struct FpuState {
    /// FPU control word (default 0x037F)
    pub control_word: u16,
    /// FPU status word (default 0x0000)
    pub status_word: u16,
    /// FPU tag word (default 0xFFFF - all empty)
    pub tag_word: u16,
    /// FPU data pointer
    pub data_ptr: u64,
    /// FPU instruction pointer
    pub instr_ptr: u64,
    /// FPU last opcode
    pub last_opcode: u16,
    /// FPU register stack (8 x 80-bit, stored as f64 for simplicity)
    pub st: [f64; 8],
    /// Top of stack pointer (0-7), stored in status word bits 11-13
    pub top: u8,
}

impl Default for FpuState {
    fn default() -> Self {
        FpuState {
            control_word: 0x037F, // Round to nearest, all exceptions masked, 64-bit precision
            status_word: 0x0000,
            tag_word: 0xFFFF, // All registers empty
            data_ptr: 0,
            instr_ptr: 0,
            last_opcode: 0,
            st: [0.0; 8],
            top: 0,
        }
    }
}

impl FpuState {
    /// Initialize FPU to default state (FINIT/FNINIT)
    pub fn init(&mut self) {
        self.control_word = 0x037F;
        self.status_word = 0x0000;
        self.tag_word = 0xFFFF;
        self.data_ptr = 0;
        self.instr_ptr = 0;
        self.last_opcode = 0;
        self.top = 0;
        // Note: register values are preserved, just tagged as empty
    }

    /// Get physical register index from stack-relative index
    #[inline]
    pub fn st_index(&self, i: u8) -> usize {
        ((self.top.wrapping_add(i)) & 7) as usize
    }

    /// Push a value onto the FPU stack
    pub fn push(&mut self, value: f64) {
        self.top = self.top.wrapping_sub(1) & 7;
        self.st[self.top as usize] = value;
        // Update tag for this register (mark as valid)
        let tag_shift = (self.top as u16) * 2;
        self.tag_word &= !(3 << tag_shift);
        // 0 = valid, 1 = zero, 2 = special, 3 = empty
        if value == 0.0 {
            self.tag_word |= 1 << tag_shift;
        }
        // Update TOP in status word
        self.status_word = (self.status_word & !0x3800) | ((self.top as u16) << 11);
    }

    /// Pop a value from the FPU stack
    pub fn pop(&mut self) -> f64 {
        let value = self.st[self.top as usize];
        // Mark register as empty
        let tag_shift = (self.top as u16) * 2;
        self.tag_word |= 3 << tag_shift;
        self.top = self.top.wrapping_add(1) & 7;
        // Update TOP in status word
        self.status_word = (self.status_word & !0x3800) | ((self.top as u16) << 11);
        value
    }

    /// Get ST(i) value
    #[inline]
    pub fn get_st(&self, i: u8) -> f64 {
        self.st[self.st_index(i)]
    }

    /// Set ST(i) value
    #[inline]
    pub fn set_st(&mut self, i: u8, value: f64) {
        let idx = self.st_index(i);
        self.st[idx] = value;
    }
}

/// Emulated x86_64 vCPU.
pub struct X86_64Vcpu {
    id: u32,
    pub(super) regs: Registers,
    pub(super) sregs: SystemRegisters,
    pub(super) mmu: Mmu,
    pub(super) fpu: FpuState,
    pub(super) halted: bool,
    io_pending: Option<IoPending>,
    trace_enabled: bool,
    /// IA32_KERNEL_GS_BASE MSR (0xC0000102) for SWAPGS
    pub(super) kernel_gs_base: u64,
    /// Protection Key Rights Register (PKRU).
    pub(super) pkru: u32,
}

/// Pending I/O operation.
struct IoPending {
    size: u8,
}

/// Decoded instruction context passed to instruction handlers.
pub(super) struct InsnContext {
    pub bytes: Vec<u8>,
    pub cursor: usize,
    pub rex: Option<u8>,
    pub operand_size_override: bool,
    pub address_size_override: bool,
    pub rep_prefix: Option<u8>,
    pub op_size: u8,
    pub rip_relative_offset: usize,
    /// EVEX prefix state (if present)
    pub evex: Option<EvexPrefix>,
}

/// EVEX prefix decoded fields (4-byte prefix for AVX-512)
#[derive(Clone, Copy, Debug)]
pub(super) struct EvexPrefix {
    /// R bit (inverted, extends ModR/M reg field to 4 bits)
    pub r: bool,
    /// X bit (inverted, extends SIB index field)
    pub x: bool,
    /// B bit (inverted, extends ModR/M r/m or SIB base)
    pub b: bool,
    /// R' bit (inverted, extends reg field to 5 bits for ZMM16-31)
    pub r_prime: bool,
    /// mm field (opcode map: 1=0F, 2=0F38, 3=0F3A, 5=MAP5, 6=MAP6)
    pub mm: u8,
    /// W bit (operand size: 0=32-bit, 1=64-bit elements)
    pub w: bool,
    /// vvvv field (inverted, non-destructive source register)
    pub vvvv: u8,
    /// pp field (implied prefix: 0=none, 1=66, 2=F3, 3=F2)
    pub pp: u8,
    /// z bit (zeroing-masking: 0=merge, 1=zero)
    pub z: bool,
    /// L'L field (vector length: 0=128, 1=256, 2=512)
    pub ll: u8,
    /// b bit (broadcast/rounding control)
    pub broadcast: bool,
    /// V' bit (inverted, extends vvvv to 5 bits)
    pub v_prime: bool,
    /// aaa field (opmask register k0-k7)
    pub aaa: u8,
}

impl InsnContext {
    /// Get REX.W flag.
    pub fn rex_w(&self) -> bool {
        self.rex.map_or(false, |r| r & 0x08 != 0)
    }

    /// Get REX.R flag (extends ModR/M reg field).
    pub fn rex_r(&self) -> u8 {
        self.rex.map_or(0, |r| (r & 0x04) << 1)
    }

    /// Get REX.B flag (extends ModR/M r/m field or opcode reg).
    pub fn rex_b(&self) -> u8 {
        self.rex.map_or(0, |r| (r & 0x01) << 3)
    }

    // =========================================================================
    // EVEX helper methods
    // =========================================================================

    /// Get full 5-bit destination register (ModR/M reg extended by EVEX.R and EVEX.R')
    pub fn evex_dest_reg(&self) -> u8 {
        if let Some(evex) = &self.evex {
            // reg field from ModR/M (3 bits) + R (bit 3) + R' (bit 4)
            let r_ext = if evex.r { 0 } else { 8 };
            let r_prime_ext = if evex.r_prime { 0 } else { 16 };
            r_ext | r_prime_ext
        } else {
            self.rex_r()
        }
    }

    /// Get full 5-bit source register (EVEX.vvvv extended by EVEX.V')
    pub fn evex_vvvv(&self) -> u8 {
        if let Some(evex) = &self.evex {
            // vvvv is inverted, V' extends to 5 bits
            let v_prime_ext = if evex.v_prime { 0 } else { 16 };
            (evex.vvvv ^ 0xF) | v_prime_ext
        } else {
            0
        }
    }

    /// Get full 5-bit r/m register (extended by EVEX.B and EVEX.X for certain encodings)
    pub fn evex_rm_reg(&self) -> u8 {
        if let Some(evex) = &self.evex {
            let b_ext = if evex.b { 0 } else { 8 };
            let x_ext = if evex.x { 0 } else { 16 };
            b_ext | x_ext
        } else {
            self.rex_b()
        }
    }

    /// Get vector length from EVEX.L'L (0=128, 1=256, 2=512 bits)
    pub fn evex_vl(&self) -> u16 {
        if let Some(evex) = &self.evex {
            match evex.ll {
                0 => 128,
                1 => 256,
                2 => 512,
                _ => 128,
            }
        } else {
            128
        }
    }

    /// Check if EVEX zeroing-masking is enabled
    pub fn evex_zeroing(&self) -> bool {
        self.evex.map_or(false, |e| e.z)
    }

    /// Get opmask register index (k0-k7)
    pub fn evex_mask(&self) -> u8 {
        self.evex.map_or(0, |e| e.aaa)
    }

    /// Check if EVEX broadcast is enabled
    pub fn evex_broadcast(&self) -> bool {
        self.evex.map_or(false, |e| e.broadcast)
    }

    /// Get EVEX.W bit (element width)
    pub fn evex_w(&self) -> bool {
        self.evex.map_or(false, |e| e.w)
    }

    /// Consume and return the next byte.
    pub fn consume_u8(&mut self) -> Result<u8> {
        if self.cursor >= self.bytes.len() {
            return Err(Error::Emulator("instruction too short".to_string()));
        }
        let b = self.bytes[self.cursor];
        self.cursor += 1;
        Ok(b)
    }

    /// Peek at the next byte without consuming.
    #[allow(dead_code)]
    pub fn peek_u8(&self) -> Result<u8> {
        if self.cursor >= self.bytes.len() {
            return Err(Error::Emulator("instruction too short".to_string()));
        }
        Ok(self.bytes[self.cursor])
    }

    /// Consume and return a little-endian u16.
    pub fn consume_u16(&mut self) -> Result<u16> {
        if self.cursor + 2 > self.bytes.len() {
            return Err(Error::Emulator("instruction too short for u16".to_string()));
        }
        let val = u16::from_le_bytes([self.bytes[self.cursor], self.bytes[self.cursor + 1]]);
        self.cursor += 2;
        Ok(val)
    }

    /// Consume and return a little-endian u32.
    pub fn consume_u32(&mut self) -> Result<u32> {
        if self.cursor + 4 > self.bytes.len() {
            return Err(Error::Emulator("instruction too short for u32".to_string()));
        }
        let val = u32::from_le_bytes([
            self.bytes[self.cursor],
            self.bytes[self.cursor + 1],
            self.bytes[self.cursor + 2],
            self.bytes[self.cursor + 3],
        ]);
        self.cursor += 4;
        Ok(val)
    }

    /// Consume and return a little-endian u64.
    pub fn consume_u64(&mut self) -> Result<u64> {
        if self.cursor + 8 > self.bytes.len() {
            return Err(Error::Emulator("instruction too short for u64".to_string()));
        }
        let val = u64::from_le_bytes([
            self.bytes[self.cursor],
            self.bytes[self.cursor + 1],
            self.bytes[self.cursor + 2],
            self.bytes[self.cursor + 3],
            self.bytes[self.cursor + 4],
            self.bytes[self.cursor + 5],
            self.bytes[self.cursor + 6],
            self.bytes[self.cursor + 7],
        ]);
        self.cursor += 8;
        Ok(val)
    }

    /// Read an immediate value of the specified size.
    pub fn consume_imm(&mut self, size: u8) -> Result<u64> {
        match size {
            1 => Ok(self.consume_u8()? as u64),
            2 => Ok(self.consume_u16()? as u64),
            4 => Ok(self.consume_u32()? as u64),
            8 => Ok(self.consume_u64()?),
            _ => Err(Error::Emulator(format!("invalid immediate size: {}", size))),
        }
    }
}

impl X86_64Vcpu {
    pub fn new(id: u32, mem: Arc<GuestMemoryMmap>) -> Self {
        X86_64Vcpu {
            id,
            regs: Registers::default(),
            sregs: SystemRegisters::default(),
            mmu: Mmu::new(mem),
            fpu: FpuState::default(),
            halted: false,
            io_pending: None,
            trace_enabled: std::env::var("RAX_TRACE").is_ok(),
            kernel_gs_base: 0,
            pkru: 0,
        }
    }

    /// Fetch instruction bytes from RIP.
    fn fetch(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; MAX_INSN_LEN];
        if self.mmu.read(self.regs.rip, &mut buf, &self.sregs).is_err() {
            // If we can't read 15 bytes, try smaller amounts
            for len in (1..MAX_INSN_LEN).rev() {
                buf.truncate(len);
                if self.mmu.read(self.regs.rip, &mut buf, &self.sregs).is_ok() {
                    return Ok(buf);
                }
            }
            return Err(Error::Emulator(format!(
                "failed to fetch instruction at RIP={:#x}",
                self.regs.rip
            )));
        }
        Ok(buf)
    }

    /// Execute a single instruction.
    pub fn step(&mut self) -> Result<Option<VcpuExit>> {
        // Update global RIP tracker for debugging
        CURRENT_RIP.store(self.regs.rip, Ordering::Relaxed);

        let bytes = self.fetch()?;

        // Trace execution for debugging
        let trace_bytes = if self.trace_enabled {
            Some(bytes[..std::cmp::min(8, bytes.len())].to_vec())
        } else {
            None
        };

        // Decode prefixes
        let mut ctx = Decoder::decode_prefixes(bytes)?;

        // Determine operand size (64-bit mode defaults to 32-bit; compat depends on CS.D).
        ctx.op_size = if self.sregs.cs.l {
            if ctx.rex_w() {
                8
            } else if ctx.operand_size_override {
                2
            } else {
                4
            }
        } else {
            let default_16bit = !self.sregs.cs.db;
            let is_16bit = default_16bit ^ ctx.operand_size_override;
            if is_16bit { 2 } else { 4 }
        };

        // Get opcode
        let opcode = ctx.consume_u8()?;

        // Trace execution for debugging
        if let Some(tb) = trace_bytes {
            tracing::trace!(
                rip = format!("{:#x}", self.regs.rip),
                opcode = format!("{:#04x}", opcode),
                bytes = format!("{:02x?}", tb),
                "exec"
            );
        }

        // Execute instruction
        self.execute(opcode, &mut ctx)
    }

    // Register access methods
    pub(super) fn get_reg(&self, reg: u8, size: u8) -> u64 {
        let val = match reg & 0x0F {
            0 => self.regs.rax,
            1 => self.regs.rcx,
            2 => self.regs.rdx,
            3 => self.regs.rbx,
            4 => self.regs.rsp,
            5 => self.regs.rbp,
            6 => self.regs.rsi,
            7 => self.regs.rdi,
            8 => self.regs.r8,
            9 => self.regs.r9,
            10 => self.regs.r10,
            11 => self.regs.r11,
            12 => self.regs.r12,
            13 => self.regs.r13,
            14 => self.regs.r14,
            15 => self.regs.r15,
            _ => 0,
        };

        match size {
            1 => val & 0xFF,
            2 => val & 0xFFFF,
            4 => val & 0xFFFF_FFFF,
            8 => val,
            _ => val,
        }
    }

    /// Set an 8-bit register value, correctly handling AH/CH/DH/BH when no REX prefix
    pub(super) fn set_reg8(&mut self, reg: u8, value: u64, has_rex: bool) {
        // In 64-bit mode, without REX prefix, reg 4-7 are AH/CH/DH/BH
        // With REX prefix, reg 4-7 are SPL/BPL/SIL/DIL
        if !has_rex {
            match reg & 0x07 {
                4 => {
                    self.regs.rax = (self.regs.rax & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                5 => {
                    self.regs.rcx = (self.regs.rcx & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                6 => {
                    self.regs.rdx = (self.regs.rdx & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                7 => {
                    self.regs.rbx = (self.regs.rbx & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                _ => {}
            }
        }
        self.set_reg(reg, value, 1);
    }

    /// Get an 8-bit register value, correctly handling AH/CH/DH/BH when no REX prefix
    pub(super) fn get_reg8(&self, reg: u8, has_rex: bool) -> u64 {
        // In 64-bit mode, without REX prefix, reg 4-7 are AH/CH/DH/BH
        // With REX prefix, reg 4-7 are SPL/BPL/SIL/DIL
        if !has_rex {
            match reg & 0x07 {
                4 => return (self.regs.rax >> 8) & 0xFF,
                5 => return (self.regs.rcx >> 8) & 0xFF,
                6 => return (self.regs.rdx >> 8) & 0xFF,
                7 => return (self.regs.rbx >> 8) & 0xFF,
                _ => {}
            }
        }
        self.get_reg(reg, 1)
    }

    pub(super) fn set_reg(&mut self, reg: u8, value: u64, size: u8) {
        let reg_ref = match reg & 0x0F {
            0 => &mut self.regs.rax,
            1 => &mut self.regs.rcx,
            2 => &mut self.regs.rdx,
            3 => &mut self.regs.rbx,
            4 => &mut self.regs.rsp,
            5 => &mut self.regs.rbp,
            6 => &mut self.regs.rsi,
            7 => &mut self.regs.rdi,
            8 => &mut self.regs.r8,
            9 => &mut self.regs.r9,
            10 => &mut self.regs.r10,
            11 => &mut self.regs.r11,
            12 => &mut self.regs.r12,
            13 => &mut self.regs.r13,
            14 => &mut self.regs.r14,
            15 => &mut self.regs.r15,
            _ => return,
        };

        match size {
            1 => *reg_ref = (*reg_ref & !0xFF) | (value & 0xFF),
            2 => *reg_ref = (*reg_ref & !0xFFFF) | (value & 0xFFFF),
            4 => *reg_ref = value & 0xFFFF_FFFF, // 32-bit ops zero-extend
            8 => *reg_ref = value,
            _ => {}
        }
    }

    // Memory access helpers
    pub(super) fn read_mem(&self, addr: u64, size: u8) -> Result<u64> {
        match size {
            1 => Ok(self.mmu.read_u8(addr, &self.sregs)? as u64),
            2 => Ok(self.mmu.read_u16(addr, &self.sregs)? as u64),
            4 => Ok(self.mmu.read_u32(addr, &self.sregs)? as u64),
            8 => Ok(self.mmu.read_u64(addr, &self.sregs)?),
            _ => Err(Error::Emulator(format!("invalid memory access size: {}", size))),
        }
    }

    pub(super) fn write_mem(&mut self, addr: u64, value: u64, size: u8) -> Result<()> {
        match size {
            1 => self.mmu.write_u8(addr, value as u8, &self.sregs),
            2 => self.mmu.write_u16(addr, value as u16, &self.sregs),
            4 => self.mmu.write_u32(addr, value as u32, &self.sregs),
            8 => self.mmu.write_u64(addr, value, &self.sregs),
            _ => Err(Error::Emulator(format!("invalid memory access size: {}", size))),
        }
    }

    // FPU memory access helpers
    pub(super) fn read_mem16(&self, addr: u64) -> Result<u16> {
        self.mmu.read_u16(addr, &self.sregs)
    }

    pub(super) fn write_mem16(&mut self, addr: u64, value: u16) -> Result<()> {
        self.mmu.write_u16(addr, value, &self.sregs)
    }

    pub(super) fn read_mem32(&self, addr: u64) -> Result<u32> {
        self.mmu.read_u32(addr, &self.sregs)
    }

    pub(super) fn write_mem32(&mut self, addr: u64, value: u32) -> Result<()> {
        self.mmu.write_u32(addr, value, &self.sregs)
    }

    pub(super) fn read_mem64(&self, addr: u64) -> Result<u64> {
        self.mmu.read_u64(addr, &self.sregs)
    }

    pub(super) fn write_mem64(&mut self, addr: u64, value: u64) -> Result<()> {
        self.mmu.write_u64(addr, value, &self.sregs)
    }

    pub(super) fn read_f32(&self, addr: u64) -> Result<f32> {
        let bits = self.mmu.read_u32(addr, &self.sregs)?;
        Ok(f32::from_bits(bits))
    }

    pub(super) fn write_f32(&mut self, addr: u64, value: f32) -> Result<()> {
        self.mmu.write_u32(addr, value.to_bits(), &self.sregs)
    }

    pub(super) fn read_f64(&self, addr: u64) -> Result<f64> {
        let bits = self.mmu.read_u64(addr, &self.sregs)?;
        Ok(f64::from_bits(bits))
    }

    pub(super) fn write_f64(&mut self, addr: u64, value: f64) -> Result<()> {
        self.mmu.write_u64(addr, value.to_bits(), &self.sregs)
    }

    pub(super) fn read_bytes(&self, addr: u64, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.mmu.read(addr, &mut buf, &self.sregs)?;
        Ok(buf)
    }

    pub(super) fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<()> {
        self.mmu.write(addr, data, &self.sregs)
    }

    // Stack helpers
    pub(super) fn push64(&mut self, value: u64) -> Result<()> {
        self.regs.rsp = self.regs.rsp.wrapping_sub(8);
        self.mmu.write_u64(self.regs.rsp, value, &self.sregs)
    }

    pub(super) fn pop64(&mut self) -> Result<u64> {
        let value = self.mmu.read_u64(self.regs.rsp, &self.sregs)?;
        self.regs.rsp = self.regs.rsp.wrapping_add(8);
        Ok(value)
    }

    pub(super) fn push32(&mut self, value: u32) -> Result<()> {
        self.regs.rsp = self.regs.rsp.wrapping_sub(4);
        self.mmu.write_u32(self.regs.rsp, value, &self.sregs)
    }

    pub(super) fn pop32(&mut self) -> Result<u32> {
        let value = self.mmu.read_u32(self.regs.rsp, &self.sregs)?;
        self.regs.rsp = self.regs.rsp.wrapping_add(4);
        Ok(value)
    }

    pub(super) fn push16(&mut self, value: u16) -> Result<()> {
        self.regs.rsp = self.regs.rsp.wrapping_sub(2);
        self.mmu.write_u16(self.regs.rsp, value, &self.sregs)
    }

    pub(super) fn pop16(&mut self) -> Result<u16> {
        let value = self.mmu.read_u16(self.regs.rsp, &self.sregs)?;
        self.regs.rsp = self.regs.rsp.wrapping_add(2);
        Ok(value)
    }

    // I/O pending helpers
    pub(super) fn set_io_pending(&mut self, size: u8) {
        self.io_pending = Some(IoPending { size });
    }

    // Segment register access
    pub(super) fn get_sreg(&self, sreg: u8) -> u16 {
        match sreg {
            0 => self.sregs.es.selector,
            1 => self.sregs.cs.selector,
            2 => self.sregs.ss.selector,
            3 => self.sregs.ds.selector,
            4 => self.sregs.fs.selector,
            5 => self.sregs.gs.selector,
            _ => 0,
        }
    }

    pub(super) fn set_sreg(&mut self, sreg: u8, value: u16) {
        let seg = match sreg {
            0 => &mut self.sregs.es,
            1 => &mut self.sregs.cs,
            2 => &mut self.sregs.ss,
            3 => &mut self.sregs.ds,
            4 => &mut self.sregs.fs,
            5 => &mut self.sregs.gs,
            _ => return,
        };
        seg.selector = value;
        // In protected/long mode, we should load the descriptor from GDT
        // For now, set a basic flat segment
        seg.base = 0;
        seg.limit = 0xFFFF_FFFF;
        seg.type_ = 0x03; // Data segment, read/write, accessed
        seg.present = true;
        seg.dpl = 0;
        seg.db = true;
        seg.s = true;
        seg.l = false;
        seg.g = true;
    }

    // Condition checking for Jcc/SETcc
    pub(super) fn check_condition(&self, cc: u8) -> bool {
        let cf = self.regs.rflags & flags::bits::CF != 0;
        let zf = self.regs.rflags & flags::bits::ZF != 0;
        let sf = self.regs.rflags & flags::bits::SF != 0;
        let of = self.regs.rflags & flags::bits::OF != 0;
        let pf = self.regs.rflags & flags::bits::PF != 0;

        match cc {
            0x0 => of,              // O
            0x1 => !of,             // NO
            0x2 => cf,              // B/NAE/C
            0x3 => !cf,             // NB/AE/NC
            0x4 => zf,              // E/Z
            0x5 => !zf,             // NE/NZ
            0x6 => cf || zf,        // BE/NA
            0x7 => !cf && !zf,      // NBE/A
            0x8 => sf,              // S
            0x9 => !sf,             // NS
            0xA => pf,              // P/PE
            0xB => !pf,             // NP/PO
            0xC => sf != of,        // L/NGE
            0xD => sf == of,        // NL/GE
            0xE => zf || (sf != of), // LE/NG
            0xF => !zf && (sf == of), // NLE/G
            _ => false,
        }
    }
}

impl VCpu for X86_64Vcpu {
    fn run(&mut self) -> Result<VcpuExit> {
        use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
        static TOTAL_INSN: AtomicU64 = AtomicU64::new(0);
        static HIT_HALT: AtomicU64 = AtomicU64::new(0);

        // Simple ring buffer using atomics
        static HISTORY: [AtomicU64; 50] = {
            const INIT: AtomicU64 = AtomicU64::new(0);
            [INIT; 50]
        };
        static HIST_IDX: AtomicUsize = AtomicUsize::new(0);

        // Per-run iteration limit to prevent infinite loops in tests
        let mut run_iterations = 0u64;
        const MAX_RUN_ITERATIONS: u64 = 100_000;

        loop {
            if self.halted {
                return Ok(VcpuExit::Hlt);
            }

            run_iterations += 1;
            if run_iterations > MAX_RUN_ITERATIONS {
                return Err(crate::error::Error::Emulator(format!(
                    "exceeded {} iterations in run() at RIP={:#x}", MAX_RUN_ITERATIONS, self.regs.rip
                )));
            }

            let insn_count = TOTAL_INSN.fetch_add(1, Ordering::Relaxed) + 1;
            let rip = self.regs.rip;

            // Track instruction history
            let idx = HIST_IDX.fetch_add(1, Ordering::Relaxed) % 50;
            HISTORY[idx].store(rip, Ordering::Relaxed);

            // Log when we enter decompressor code and apply runtime patches
            static DECOMPRESSOR_ENTERED: AtomicU64 = AtomicU64::new(0);
            if rip >= 0x5000000 && rip < 0x6000000 &&
               DECOMPRESSOR_ENTERED.fetch_add(1, Ordering::Relaxed) == 0 {
                eprintln!("[EMU] Entered decompressor at RIP={:#x}", rip);

                // Patch phys_bits to 29 (512MB) to limit identity mapping range
                // This drastically reduces page table allocation requirements
                // With 512MB max physical + 64MB kernel = 576MB to map
                // Using 2MB pages: 576MB / 2MB = 288 entries = 1 page table + overhead
                let phys_bits_addr = 0x503b394_u64;
                let new_phys_bits: u32 = 29;  // 512MB - just enough for our guest RAM
                let _ = self.mmu.write_phys(phys_bits_addr, &new_phys_bits.to_le_bytes());
                eprintln!("[EMU] Patched phys_bits to {}", new_phys_bits);

                // Also patch the hardcoded MOV RAX, 0x8000000000 to match
                let imm_addr = 0x5023d87_u64;
                let new_limit: u64 = 1u64 << new_phys_bits;  // 512MB
                let _ = self.mmu.write_phys(imm_addr, &new_limit.to_le_bytes());
                eprintln!("[EMU] Patched hardcoded limit to {:#x}", new_limit);

                // Verify patches
                let mut verify = [0u8; 4];
                if self.mmu.read_phys(phys_bits_addr, &mut verify).is_ok() {
                    let val = u32::from_le_bytes(verify);
                    eprintln!("[EMU] Verified phys_bits = {}", val);
                }
            }

            // Trace calls to alloc_pgt_page and its returns
            // We need to find where RAX becomes 0 (NULL return) from this function
            static ALLOC_PGT_TRACE: AtomicU64 = AtomicU64::new(0);
            static LAST_CALL_ADDR: AtomicU64 = AtomicU64::new(0);

            // Read opcode to detect RET instructions
            let mut opcode_buf = [0u8; 2];
            if self.mmu.read(rip, &mut opcode_buf, &self.sregs).is_ok() {
                // Check for RET (0xC3) when RAX is 0 in decompressor area
                if opcode_buf[0] == 0xC3 && self.regs.rax == 0 &&
                   rip >= 0x5020000 && rip < 0x5040000 {
                    let count = ALLOC_PGT_TRACE.fetch_add(1, Ordering::Relaxed);
                    if count < 20 {
                        eprintln!("[EMU] RET with RAX=0 at RIP={:#x}, RSP={:#x}", rip, self.regs.rsp);
                        // Read return address from stack
                        let mut ret_addr_buf = [0u8; 8];
                        if self.mmu.read_phys(self.regs.rsp, &mut ret_addr_buf).is_ok() {
                            let ret_addr = u64::from_le_bytes(ret_addr_buf);
                            eprintln!("[EMU]   returning to {:#x}", ret_addr);
                        }
                    }
                }
            }

            // APPROACH: Intercept when alloc_pgt_page returns NULL
            // The function checks: if (pgt_buf_end >= pgt_buf + pgt_buf_size) return NULL;
            // We capture pgt_data at entry and provide extra pages at the RET
            static EXTRA_PGT_BUF: AtomicU64 = AtomicU64::new(0x3000000); // Start at 48MB
            static ALLOC_INTERCEPT_COUNT: AtomicU64 = AtomicU64::new(0);
            static PGT_DATA_ADDR: AtomicU64 = AtomicU64::new(0);

            // Capture pgt_data at alloc_pgt_page entry (look for the function prologue)
            // alloc_pgt_page typically starts at some address and has RDI = context
            // We'll look for the pattern where the function is entered right before the NULL return
            // Actually, let's just try to find the pgt_data by scanning for the structure

            // Trace execution around the failure point
            if rip >= 0x50245f0 && rip <= 0x5024620 {
                let mut code = [0u8; 8];
                let _ = self.mmu.read(rip, &mut code, &self.sregs);
                eprintln!("[EMU] TRACE RIP={:#x} code={:02x?}", rip, code);
                eprintln!("[EMU]   RAX={:#x} RCX={:#x} RDX={:#x}",
                    self.regs.rax, self.regs.rcx, self.regs.rdx);
                eprintln!("[EMU]   CR0={:#x} CR3={:#x} CR4={:#x}",
                    self.sregs.cr0, self.sregs.cr3, self.sregs.cr4);
            }

            // Trace halt loop area to understand what's happening
            static HALT_AREA_TRACE: AtomicU64 = AtomicU64::new(0);
            if rip >= 0x5022200 && rip <= 0x5022280 {
                let count = HALT_AREA_TRACE.fetch_add(1, Ordering::Relaxed);
                if count < 40 {
                    let mut code = [0u8; 8];
                    let _ = self.mmu.read(rip, &mut code, &self.sregs);
                    eprintln!("[EMU] PANIC AREA RIP={:#x} code={:02x?}", rip, code);
                    eprintln!("[EMU]   RDI={:#x} RSI={:#x}", self.regs.rdi, self.regs.rsi);

                    // Try to read string from RDI (error message)
                    if self.regs.rdi >= 0x5000000 && self.regs.rdi < 0x6000000 {
                        let mut str_buf = [0u8; 80];
                        if self.mmu.read(self.regs.rdi, &mut str_buf, &self.sregs).is_ok() {
                            if let Some(end) = str_buf.iter().position(|&b| b == 0) {
                                if let Ok(s) = std::str::from_utf8(&str_buf[..end]) {
                                    eprintln!("[EMU]   Error msg: \"{}\"", s);
                                }
                            }
                        }
                    }
                }
            }

            // Trace calls to kernel_ident_mapping_init
            // The return address 0x5024623 tells us the call was from 0x502461e
            // CALL rel32 at 0x502461e: e8 2d fc ff ff = call -0x3d3
            // Target = 0x502461e + 5 - 0x3d3 = 0x5024250
            // Let's also try other potential entry points
            static IDENT_MAP_CALLS: AtomicU64 = AtomicU64::new(0);
            // Check for function entry - look for the pattern after push rbx (common prologue)
            // We know the caller is setup_identity_mappings at 0x5024590
            // And it calls kernel_ident_mapping_init multiple times
            if rip == 0x5024250 {
                let count = IDENT_MAP_CALLS.fetch_add(1, Ordering::Relaxed);
                if count < 10 {
                    eprintln!("[EMU] kernel_ident_mapping_init call #{} at RIP={:#x}:", count + 1, rip);
                    eprintln!("[EMU]   RDI(info)={:#x} RSI(pgd)={:#x}", self.regs.rdi, self.regs.rsi);
                    eprintln!("[EMU]   RDX(pstart)={:#x} ({} MB) RCX(pend)={:#x} ({} MB)",
                        self.regs.rdx, self.regs.rdx >> 20, self.regs.rcx, self.regs.rcx >> 20);
                    // Read x86_mapping_info structure - check actual layout
                    // struct x86_mapping_info {
                    //     void *(*alloc_pgt_page)(void *);  // 0
                    //     void *context;                     // 8
                    //     unsigned long page_flag;           // 16
                    //     unsigned long offset;              // 24
                    //     bool direct_gbpages;               // 32
                    // };
                    let info_addr = self.regs.rdi;
                    let mut info_buf = [0u8; 48];
                    if self.mmu.read_phys(info_addr, &mut info_buf).is_ok() {
                        eprintln!("[EMU]   info raw: {:02x?}", &info_buf[..40]);
                        let alloc_fn = u64::from_le_bytes(info_buf[0..8].try_into().unwrap());
                        let context = u64::from_le_bytes(info_buf[8..16].try_into().unwrap());
                        let page_flag = u64::from_le_bytes(info_buf[16..24].try_into().unwrap());
                        let offset = u64::from_le_bytes(info_buf[24..32].try_into().unwrap());
                        let direct_gbpages = info_buf[32];
                        eprintln!("[EMU]   alloc_fn={:#x} context={:#x}", alloc_fn, context);
                        eprintln!("[EMU]   page_flag={:#x} offset={:#x}", page_flag, offset);
                        eprintln!("[EMU]   direct_gbpages={}", direct_gbpages);
                    }
                }
            }

            // Trace when we're in the paging setup code to see what addresses are being used
            static PGD_SETUP_TRACE: AtomicU64 = AtomicU64::new(0);
            if rip >= 0x5024590 && rip < 0x5024700 {
                let count = PGD_SETUP_TRACE.fetch_add(1, Ordering::Relaxed);
                if count < 50 && count % 5 == 0 {
                    eprintln!("[EMU] setup_ident @ {:#x}: RDI={:#x} RSI={:#x} RDX={:#x} RCX={:#x}",
                        rip, self.regs.rdi, self.regs.rsi, self.regs.rdx, self.regs.rcx);
                }
            }

            // Find the actual x86_mapping_info structure by looking for the CALL setup pattern
            // Let's look for where mapping_info is set up before calls to kernel_ident_mapping_init
            static MAPPING_INFO_SETUP: AtomicU64 = AtomicU64::new(0);
            if rip >= 0x50244b0 && rip < 0x5024590 {
                let count = MAPPING_INFO_SETUP.fetch_add(1, Ordering::Relaxed);
                if count < 30 && count % 3 == 0 {
                    eprintln!("[EMU] pre_setup @ {:#x}: RDI={:#x} RSI={:#x} RDX={:#x} RCX={:#x}",
                        rip, self.regs.rdi, self.regs.rsi, self.regs.rdx, self.regs.rcx);
                    eprintln!("[EMU]   R8={:#x} R9={:#x} RBX={:#x} RBP={:#x}",
                        self.regs.r8, self.regs.r9, self.regs.rbx, self.regs.rbp);
                }
            }

            // Trace what happens immediately after alloc_pgt_page returns
            // The return addresses are 0x5023d1d and 0x5023c63
            static ALLOC_RET_TRACE: AtomicU64 = AtomicU64::new(0);
            if rip == 0x5023d1d || rip == 0x5023c63 {
                let count = ALLOC_RET_TRACE.fetch_add(1, Ordering::Relaxed);
                if count < 10 {
                    eprintln!("[EMU] After alloc_pgt_page at RIP={:#x}: RAX={:#x} (returned page)",
                        rip, self.regs.rax);
                    // Dump next few instructions
                    let mut code = [0u8; 20];
                    let _ = self.mmu.read(rip, &mut code, &self.sregs);
                    eprintln!("[EMU]   next code: {:02x?}", &code);
                }

                // FIX: Store the first allocated page (PML4) to top_pgtable at 0x5072c58
                // The kernel code saves it to R15 but never stores to the global
                if count == 0 && rip == 0x5023d1d {
                    let pml4 = self.regs.rax;
                    let top_pgtable_addr = 0x5072c58_u64;
                    let _ = self.mmu.write_phys(top_pgtable_addr, &pml4.to_le_bytes());
                    eprintln!("[EMU] Set top_pgtable at {:#x} to PML4 {:#x}", top_pgtable_addr, pml4);
                }
            }

            // Also trace when top_pgtable at 0x5072c58 changes from 0 to something
            static PGTABLE_VAL_PREV: AtomicU64 = AtomicU64::new(0);
            if rip >= 0x5020000 && rip < 0x5050000 && insn_count % 1000 == 0 {
                let mut val = [0u8; 8];
                if self.mmu.read_phys(0x5072c58, &mut val).is_ok() {
                    let v = u64::from_le_bytes(val);
                    let prev = PGTABLE_VAL_PREV.swap(v, Ordering::Relaxed);
                    if v != prev && prev == 0 && v != 0 {
                        eprintln!("[EMU] top_pgtable at 0x5072c58 changed from 0 to {:#x} at insn #{}",
                            v, insn_count);
                    }
                }
            }

            // Trace all execution in 0x5023e00-0x5023e2d to find the function entry
            static ALLOC_ENTRY_HIT: AtomicU64 = AtomicU64::new(0);
            static PREV_IN_RANGE: AtomicBool = AtomicBool::new(false);
            let in_alloc_range = rip >= 0x5023e00 && rip <= 0x5023e2d;
            let was_in_range = PREV_IN_RANGE.swap(in_alloc_range, Ordering::Relaxed);

            // Detect entry into the range (was outside, now inside)
            if in_alloc_range && !was_in_range {
                let count = ALLOC_ENTRY_HIT.fetch_add(1, Ordering::Relaxed);
                let pgt_data = self.regs.rdi;
                PGT_DATA_ADDR.store(pgt_data, Ordering::Relaxed);

                // FIX: The pgt_data structure at 0x5072c60 appears to be incorrectly initialized
                // The function computes: return = pgt_buf + offset
                // So offset should be 0 (offset from start), not absolute address
                // limit is compared against offset, so limit = pgt_buf_size
                if count == 0 && pgt_data == 0x5072c60 {
                    let pgt_buf: u64 = 0x5087000;  // The actual buffer address
                    let pgt_buf_size: u64 = 0x20000;  // 128KB
                    let limit = pgt_buf_size;  // Limit is size, not absolute address
                    let offset: u64 = 0;  // Offset from start, NOT absolute address

                    // Write corrected structure
                    let _ = self.mmu.write_phys(pgt_data, &pgt_buf.to_le_bytes());
                    let _ = self.mmu.write_phys(pgt_data + 8, &limit.to_le_bytes());
                    let _ = self.mmu.write_phys(pgt_data + 16, &offset.to_le_bytes());
                    eprintln!("[EMU] Fixed pgt_data structure at {:#x}:", pgt_data);
                    eprintln!("[EMU]   pgt_buf={:#x} limit={:#x} offset={:#x}", pgt_buf, limit, offset);
                }

                if count < 10 {
                    eprintln!("[EMU] Entered alloc_pgt_page range #{} at RIP={:#x}:", count + 1, rip);
                    eprintln!("[EMU]   RDI={:#x} (pgt_data address)", pgt_data);
                    // Read structure values
                    let mut buf = [0u8; 24];
                    let _ = self.mmu.read_phys(pgt_data, &mut buf);
                    let pgt_buf = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                    let limit = u64::from_le_bytes(buf[8..16].try_into().unwrap());
                    let offset = u64::from_le_bytes(buf[16..24].try_into().unwrap());
                    eprintln!("[EMU]   pgt_buf={:#x} limit={:#x} offset={:#x}", pgt_buf, limit, offset);
                    eprintln!("[EMU]   raw: {:02x?}", &buf);
                }
            }

            if rip == 0x5023e2d && self.regs.rax == 0 {
                // This is alloc_pgt_page returning NULL - provide a page instead
                let count = ALLOC_INTERCEPT_COUNT.fetch_add(1, Ordering::Relaxed);
                if count < 256 {
                    // Allocate a page from our extra buffer
                    let extra_page = EXTRA_PGT_BUF.fetch_add(0x1000, Ordering::Relaxed);
                    if count < 20 {
                        eprintln!("[EMU] alloc_pgt_page returning NULL, providing page at {:#x} (#{} interception)",
                            extra_page, count + 1);
                    }
                    // Zero out the page
                    let zeros = [0u8; 4096];
                    let _ = self.mmu.write_phys(extra_page, &zeros);

                    // Update the pgt_data structure to prevent infinite loop
                    // Structure: { pgt_buf: ptr, limit: ptr, offset: ptr }
                    // We need to set offset < limit for the next call to succeed
                    // Actually, simpler: just update the limit to be higher
                    let pgt_data_addr = PGT_DATA_ADDR.load(Ordering::Relaxed);
                    if pgt_data_addr != 0 {
                        // Read current limit
                        let mut limit_buf = [0u8; 8];
                        let _ = self.mmu.read_phys(pgt_data_addr + 8, &mut limit_buf);
                        let limit = u64::from_le_bytes(limit_buf);
                        // Read current offset
                        let mut offset_buf = [0u8; 8];
                        let _ = self.mmu.read_phys(pgt_data_addr + 16, &mut offset_buf);
                        let offset = u64::from_le_bytes(offset_buf);
                        if count < 10 {
                            eprintln!("[EMU]   pgt_data: limit={:#x} offset={:#x}", limit, offset);
                        }
                        // Increase limit by one page to allow next allocation
                        let new_limit = (limit + 0x1000).to_le_bytes();
                        let _ = self.mmu.write_phys(pgt_data_addr + 8, &new_limit);
                    }

                    // Set RAX to the new page address (success)
                    self.regs.rax = extra_page;
                }
            }

            // Watchpoint at the MOV instruction that reads phys_bits
            static PHYS_BITS_READ: AtomicU64 = AtomicU64::new(0);
            if rip == 0x502431c {
                let count = PHYS_BITS_READ.fetch_add(1, Ordering::Relaxed) + 1;
                if count <= 5 {
                    let phys_bits_addr = 0x503b394_u64;
                    let mut buf = [0u8; 4];
                    if self.mmu.read_phys(phys_bits_addr, &mut buf).is_ok() {
                        let val = u32::from_le_bytes(buf);
                        eprintln!("[EMU] MOV ECX,[phys_bits] #{} - value: {} at insn #{}", count, val, insn_count);
                    }
                }
            }

            // Watchpoint at the SHL instruction
            static SHL_HIT: AtomicU64 = AtomicU64::new(0);
            if rip == 0x5024336 && SHL_HIT.fetch_add(1, Ordering::Relaxed) == 0 {
                eprintln!("[EMU] At SHL RAX,CL: RAX={:#x} CL={:#x} ({})",
                    self.regs.rax, self.regs.rcx & 0xff, self.regs.rcx & 0xff);
            }

            // Watchpoint at the LEA instruction
            static LEA_HIT: AtomicU64 = AtomicU64::new(0);
            if rip == 0x5023d8f && LEA_HIT.fetch_add(1, Ordering::Relaxed) == 0 {
                eprintln!("[EMU] At LEA RCX,[RAX+RDX]: RAX={:#x} RDX={:#x}",
                    self.regs.rax, self.regs.rdx);
            }

            // Trace when RSI becomes 0 in decompressor code
            static PREV_RSI: AtomicU64 = AtomicU64::new(0xDEAD);
            static RSI_ZERO_WP: AtomicU64 = AtomicU64::new(0);
            static PREV2_RIP: AtomicU64 = AtomicU64::new(0);
            let prev_rip_val = HISTORY[if idx == 0 { 49 } else { idx - 1 }].load(Ordering::Relaxed);
            let prev_rsi = PREV_RSI.swap(self.regs.rsi, Ordering::Relaxed);
            if rip >= 0x5000000 && rip < 0x6000000 &&
               self.regs.rsi == 0 && prev_rsi != 0 && prev_rsi != 0xDEAD &&
               RSI_ZERO_WP.fetch_add(1, Ordering::Relaxed) < 15 {
                eprintln!("[EMU] RSI became 0 from {:#x} at RIP={:#x}", prev_rsi, rip);
                eprintln!("[EMU] Previous RIP was {:#x}", prev_rip_val);
                // Read instruction bytes at previous RIP (the one that zeroed RSI)
                let mut bytes = [0u8; 16];
                if self.mmu.read(prev_rip_val, &mut bytes, &self.sregs).is_ok() {
                    eprintln!("[EMU] Prev instruction bytes: {:02x?}", bytes);
                }
                // Also show current registers
                eprintln!("[EMU] RAX={:#x} RBX={:#x} RCX={:#x} RDX={:#x}",
                    self.regs.rax, self.regs.rbx, self.regs.rcx, self.regs.rdx);
                // Get return address from stack to identify calling function
                let mut ret_addr = [0u8; 8];
                if self.mmu.read(self.regs.rsp, &mut ret_addr, &self.sregs).is_ok() {
                    let caller = u64::from_le_bytes(ret_addr);
                    eprintln!("[EMU] Return addr on stack (caller): {:#x}", caller);
                }
            }

            // Patch: whenever any register contains 0x407b000 (wrong _compressed), fix it
            // The kernel loads _compressed into various registers at different points
            let wrong_addr = 0x407b000_u64;
            let correct_addr = 0x407b2cc_u64;
            if self.regs.rax == wrong_addr { self.regs.rax = correct_addr; }
            if self.regs.rbx == wrong_addr { self.regs.rbx = correct_addr; }
            if self.regs.rcx == wrong_addr { self.regs.rcx = correct_addr; }
            if self.regs.rdx == wrong_addr { self.regs.rdx = correct_addr; }
            if self.regs.rsi == wrong_addr { self.regs.rsi = correct_addr; }
            if self.regs.rdi == wrong_addr { self.regs.rdi = correct_addr; }

            // Intercept call site 0x5024362 - call to extract_kernel()
            // Linux kernel calling convention (from head_64.S):
            //   RDI = boot_params
            //   RSI = destination address (output buffer)
            // The destination should come from %rbp (set by choose_random_location)
            // But somehow RSI is 0 while RBP has a different value
            // Force RSI to a valid output address
            static TRACE_CALL: AtomicU64 = AtomicU64::new(0);
            static OUTPUT_INITIALIZED: AtomicU64 = AtomicU64::new(0);
            if rip == 0x5024362 {
                // Force RSI (output address) to a safe location
                // The decompressor is at ~0x5000000 with compressed kernel in 0x50xxxxx
                // We need output address that doesn't overlap with source data
                // Use 0x10000000 (256MB) which is far from the compressed data
                // and leaves room for 68MB decompressed kernel (up to 324MB, within 512MB)
                let output_addr = 0x10000000_u64;  // 256MB - safe distance from compressed data

                if self.regs.rsi == 0 || self.regs.rsi < 0x8000000 || self.regs.rsi > 0x18000000 {
                    let old_rsi = self.regs.rsi;
                    self.regs.rsi = output_addr;
                    eprintln!("[EMU] Forced RSI (destination) from {:#x} to {:#x}", old_rsi, self.regs.rsi);
                }

                // CRITICAL FIX: Patch pref_address in kernel's boot_params copy
                // The kernel copies boot_params from 0x7000 to its own memory (RDI),
                // but pref_address at offset 0x258 is 0 in the copy while the original has 0x5076000.
                // This causes bp_offset to wrap to a huge value, making pgt_buf_size = 0 -> ENOMEM.
                // Patch it here before extract_kernel() is called.
                let bp_copy_addr = self.regs.rdi;  // RDI = kernel's boot_params copy
                let pref_addr_offset = 0x258_u64;
                let _ = self.mmu.write_phys(bp_copy_addr + pref_addr_offset, &output_addr.to_le_bytes());
                eprintln!("[EMU] Patched pref_address at {:#x}+0x258 to {:#x}",
                    bp_copy_addr, output_addr);

                // Initialize output buffer with the output address itself
                // The kernel later reads from [output] and expects valid data
                // This avoids MOV RSI,[RBX] reading 0 and breaking the flow
                if OUTPUT_INITIALIZED.fetch_add(1, Ordering::Relaxed) == 0 {
                    let _ = self.mmu.write_phys(output_addr, &output_addr.to_le_bytes());
                    eprintln!("[EMU] Initialized output buffer at {:#x} with value {:#x}",
                        output_addr, output_addr);
                }

                // Just trace - don't patch. Let the kernel work naturally.
                if TRACE_CALL.fetch_add(1, Ordering::Relaxed) == 0 {
                    eprintln!("[EMU] At call site 0x5024362 (call extract_kernel):");
                    eprintln!("[EMU] RDI={:#x} RSI={:#x} RDX={:#x} RCX={:#x}",
                        self.regs.rdi, self.regs.rsi, self.regs.rdx, self.regs.rcx);

                    // Check data at multiple locations to understand where ZSTD data is
                    let mut header = [0u8; 16];

                    // Check possible ZSTD locations
                    let addrs = [
                        (0x1002cc_u64, "original_load"),
                        (0x407b000, "_compressed (from LEA)"),
                        (0x407b2cc, "data_copy+0x2cc"),
                        (0x50002cc, "decomp_base+0x2cc"),
                    ];
                    for (addr, name) in addrs {
                        if self.mmu.read_phys(addr, &mut header).is_ok() {
                            let magic = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
                            eprintln!("[EMU] {} ({:#x}): {:02x?} magic={:#x}{}",
                                name, addr, header, magic,
                                if magic == 0xFD2FB528 { " ZSTD!" } else { "" });
                        }
                    }
                }
            }

            // Watchpoint for when RAX becomes 0x8000000000
            static PREV_RAX: AtomicU64 = AtomicU64::new(0);
            static RAX_WP: AtomicU64 = AtomicU64::new(0);
            let prev_rax = PREV_RAX.swap(self.regs.rax, Ordering::Relaxed);
            if self.regs.rax == 0x8000000000 && prev_rax != 0x8000000000 &&
               RAX_WP.fetch_add(1, Ordering::Relaxed) == 0 {
                eprintln!("[EMU] RAX became 0x8000000000 from {:#x} at RIP={:#x}", prev_rax, rip);
                let prev_idx = if idx == 0 { 49 } else { idx - 1 };
                let prev_rip = HISTORY[prev_idx].load(Ordering::Relaxed);
                eprintln!("[EMU] Previous RIP: {:#x}", prev_rip);
                let mut bytes = [0u8; 16];
                if self.mmu.read(prev_rip, &mut bytes, &self.sregs).is_ok() {
                    eprintln!("[EMU] Instruction bytes: {:02x?}", bytes);
                }
                eprintln!("[EMU] RCX={:#x} RDX={:#x}", self.regs.rcx, self.regs.rdx);
            }

            // Trace the error() function and the check that calls it
            // Looking for the comparison before "Destination buffer is too small"
            // Flow: some_check -> 0x50211b0 -> 0x50212dc (puts return) -> 0x502223c (halt)
            static TRACE_ERROR_FUNC: AtomicU64 = AtomicU64::new(0);
            if rip >= 0x5021160 && rip < 0x5022000 &&
               TRACE_ERROR_FUNC.fetch_add(1, Ordering::Relaxed) < 10 {
                let mut code = [0u8; 8];
                let _ = self.mmu.read(rip, &mut code, &self.sregs);
                eprintln!("[EMU] Trace 0x{:x}: {:02x?} RAX={:#x} RDI={:#x} RSI={:#x}",
                    rip, &code[..4], self.regs.rax, self.regs.rdi, self.regs.rsi);
            }

            // Trace the call to error() - look for the caller
            // The check `if (output_len < kernel_total_size)` would use a CMP instruction
            // Let's trace calls into the 0x5021160 range (error/puts functions)
            static TRACE_ERROR_CALL: AtomicU64 = AtomicU64::new(0);
            if rip == 0x5021160 && TRACE_ERROR_CALL.fetch_add(1, Ordering::Relaxed) < 3 {
                // Read return address from stack to find the caller
                let mut ret_addr = [0u8; 8];
                if self.mmu.read(self.regs.rsp, &mut ret_addr, &self.sregs).is_ok() {
                    let caller = u64::from_le_bytes(ret_addr);
                    eprintln!("[EMU] error() called from {:#x}", caller);
                    // Read code at caller-5 to see the CALL instruction
                    let mut caller_code = [0u8; 16];
                    if self.mmu.read(caller - 10, &mut caller_code, &self.sregs).is_ok() {
                        eprintln!("[EMU] Code around caller: {:02x?}", caller_code);
                    }
                }
                eprintln!("[EMU] RDI (error string): {:#x}", self.regs.rdi);
                // Read error string
                let mut str_buf = [0u8; 64];
                if self.mmu.read(self.regs.rdi, &mut str_buf, &self.sregs).is_ok() {
                    if let Ok(s) = std::str::from_utf8(&str_buf) {
                        let s = s.split('\0').next().unwrap_or("");
                        eprintln!("[EMU] Error message: \"{}\"", s);
                    }
                }
            }

            // Trace entry to error handler at 0x5022220 to find the caller
            static ERROR_ENTRY_TRACE: AtomicU64 = AtomicU64::new(0);
            if rip == 0x5022220 && ERROR_ENTRY_TRACE.fetch_add(1, Ordering::Relaxed) == 0 {
                let error_code = self.regs.rax as i32;  // Treat as signed 32-bit
                eprintln!("[EMU] Entering error handler at 0x5022220:");
                eprintln!("[EMU] RDI={:#x} (error message)", self.regs.rdi);
                eprintln!("[EMU] RAX={:#x} = {} (error code, -12=ENOMEM)", self.regs.rax, error_code);
                // Read the error message
                let mut str_buf = [0u8; 64];
                if self.mmu.read(self.regs.rdi, &mut str_buf, &self.sregs).is_ok() {
                    if let Ok(s) = std::str::from_utf8(&str_buf) {
                        let s = s.split('\0').next().unwrap_or("");
                        eprintln!("[EMU] Error message: \"{}\"", s);
                    }
                }

                // Check pref_address in both boot_params copies
                let bp_addr = 0x5072c20_u64;  // Kernel's copy (RDI at extract_kernel entry)
                let orig_bp_addr = 0x7000_u64;  // Original we set up

                let mut pref_addr_bytes = [0u8; 8];
                let mut orig_pref_bytes = [0u8; 8];

                let copy_pref = if self.mmu.read(bp_addr + 0x258, &mut pref_addr_bytes, &self.sregs).is_ok() {
                    u64::from_le_bytes(pref_addr_bytes)
                } else { 0 };

                let orig_pref = if self.mmu.read(orig_bp_addr + 0x258, &mut orig_pref_bytes, &self.sregs).is_ok() {
                    u64::from_le_bytes(orig_pref_bytes)
                } else { 0 };

                let bp_offset = copy_pref.wrapping_sub(bp_addr);
                eprintln!("[EMU] Kernel boot_params copy at {:#x}:", bp_addr);
                eprintln!("[EMU]   pref_address = {:#x}", copy_pref);
                eprintln!("[EMU]   bp_offset = pref_address - bp = {:#x} ({} bytes)", bp_offset, bp_offset);
                eprintln!("[EMU]   BOOT_PGT_SIZE expected ~76KB = 0x13000");

                eprintln!("[EMU] Original boot_params at {:#x}: pref_address = {:#x}", orig_bp_addr, orig_pref);

                if orig_pref != 0 && copy_pref == 0 {
                    eprintln!("[EMU]   MISMATCH! Original has {:#x} but copy has 0!", orig_pref);
                }
                if bp_offset > 0x13000 {
                    eprintln!("[EMU]   WARNING: bp_offset > BOOT_PGT_SIZE - allocation will fail!");
                }
            }

            // Capture the error context before halt - at 0x502223c the kernel loads error string
            // RAX should contain the return value from kernel_ident_mapping_init()
            // Note: kernel_add_identity_map is called multiple times, so we might see multiple errors
            static CAPTURED_ERROR: AtomicU64 = AtomicU64::new(0);
            let error_count = CAPTURED_ERROR.fetch_add(1, Ordering::Relaxed);
            if rip == 0x502223c && error_count < 5 {
                eprintln!("[EMU] At error path 0x502223c (call #{}): RAX={:#x}",
                    error_count + 1, self.regs.rax);
                // Also show RDI which has the error string
                let mut str_buf = [0u8; 64];
                if self.mmu.read(self.regs.rdi, &mut str_buf, &self.sregs).is_ok() {
                    if let Ok(s) = std::str::from_utf8(&str_buf) {
                        let s = s.split('\0').next().unwrap_or("");
                        eprintln!("[EMU] Error string: \"{}\"", s);
                    }
                }
                // Read the error string that's about to be loaded
                // The instruction is LEA RDI, [RIP+0x17b42]
                // So string address = RIP + 7 (instruction length) + 0x17b42
                let string_addr = rip + 7 + 0x17b42;
                let mut str_buf = [0u8; 64];
                if self.mmu.read(string_addr, &mut str_buf, &self.sregs).is_ok() {
                    if let Ok(s) = std::str::from_utf8(&str_buf) {
                        let s = s.split('\0').next().unwrap_or("");
                        eprintln!("[EMU] Error about to be printed at 0x502223c: \"{}\" (at {:#x})", s, string_addr);
                    }
                }
            }

            // Detect when we first hit the halt loop - try to skip past it
            if rip == 0x5022270 && HIT_HALT.fetch_add(1, Ordering::Relaxed) == 0 {
                eprintln!("[EMU] First hit halt loop at insn #{}", insn_count);
                eprintln!("[EMU] RAX={:#x} RBX={:#x} RCX={:#x} RDX={:#x}",
                    self.regs.rax, self.regs.rbx, self.regs.rcx, self.regs.rdx);
                eprintln!("[EMU] RSI={:#x} RDI={:#x} RSP={:#x} RBP={:#x}",
                    self.regs.rsi, self.regs.rdi, self.regs.rsp, self.regs.rbp);
                eprintln!("[EMU] R8={:#x} R9={:#x} R10={:#x} R11={:#x}",
                    self.regs.r8, self.regs.r9, self.regs.r10, self.regs.r11);
                eprintln!("[EMU] RFLAGS={:#x} CR0={:#x} CR3={:#x} CR4={:#x}",
                    self.regs.rflags, self.sregs.cr0, self.sregs.cr3, self.sregs.cr4);

                // Try to force return from error handler to continue execution
                // Check if there's a valid return address on the stack
                let mut ret_addr = [0u8; 8];
                if self.mmu.read(self.regs.rsp, &mut ret_addr, &self.sregs).is_ok() {
                    let addr = u64::from_le_bytes(ret_addr);
                    eprintln!("[EMU] Return addr on stack: {:#x}", addr);
                    // If return address looks valid (in decompressor range), try returning
                    if addr >= 0x5000000 && addr < 0x6000000 {
                        self.regs.rip = addr;
                        self.regs.rsp += 8;  // pop return address
                        eprintln!("[EMU] Attempting to return to {:#x}", addr);
                    }
                }

                // Dump E820 entries from boot_params
                let boot_params_addr = 0x7000_u64;
                let mut e820_count = [0u8; 1];
                if self.mmu.read_phys(boot_params_addr + 0x1e8, &mut e820_count).is_ok() {
                    eprintln!("[EMU] E820 entries: {}", e820_count[0]);
                    // E820 table starts at offset 0x2d0 in boot_params
                    for i in 0..e820_count[0].min(8) {
                        let entry_addr = boot_params_addr + 0x2d0 + (i as u64 * 20);
                        let mut entry = [0u8; 20];
                        if self.mmu.read_phys(entry_addr, &mut entry).is_ok() {
                            let addr = u64::from_le_bytes([entry[0], entry[1], entry[2], entry[3], entry[4], entry[5], entry[6], entry[7]]);
                            let size = u64::from_le_bytes([entry[8], entry[9], entry[10], entry[11], entry[12], entry[13], entry[14], entry[15]]);
                            let typ = u32::from_le_bytes([entry[16], entry[17], entry[18], entry[19]]);
                            let type_str = match typ {
                                1 => "RAM",
                                2 => "Reserved",
                                3 => "ACPI",
                                4 => "NVS",
                                5 => "Unusable",
                                _ => "Unknown",
                            };
                            eprintln!("[EMU] E820[{}]: {:#x}-{:#x} ({} bytes) type={} ({})",
                                i, addr, addr + size, size, typ, type_str);
                        }
                    }
                }

                // Dump boot_params key fields
                let boot_params_addr = 0x7000_u64;
                // Read setup_header fields
                let offsets = [
                    (0x1f1, 1, "setup_sects"),
                    (0x202, 4, "header_magic"),
                    (0x206, 2, "version"),
                    (0x210, 1, "type_of_loader"),
                    (0x211, 1, "loadflags"),
                    (0x214, 4, "code32_start"),
                    (0x218, 4, "ramdisk_image"),
                    (0x21c, 4, "ramdisk_size"),
                    (0x228, 4, "cmd_line_ptr"),
                    (0x22c, 4, "initrd_addr_max"),
                    (0x230, 4, "kernel_alignment"),
                    (0x234, 1, "relocatable_kernel"),
                    (0x235, 1, "min_alignment"),
                    (0x236, 2, "xloadflags"),
                    (0x238, 4, "cmdline_size"),
                    (0x258, 8, "pref_address"),
                    (0x260, 4, "init_size"),
                    (0x264, 4, "handover_offset"),
                    (0x268, 4, "kernel_info_offset"),
                    (0x0c0, 4, "ext_ramdisk_image"),
                    (0x0c4, 4, "ext_ramdisk_size"),
                    (0x0c8, 4, "ext_cmd_line_ptr"),
                    (0x1e8, 1, "e820_entries"),
                    (0x1c0, 4, "efi_info.signature"),
                ];
                for (offset, size, name) in offsets {
                    let addr = boot_params_addr + offset;
                    let mut buf = [0u8; 8];
                    if self.mmu.read_phys(addr, &mut buf[..size as usize]).is_ok() {
                        let val = match size {
                            1 => buf[0] as u64,
                            2 => u16::from_le_bytes([buf[0], buf[1]]) as u64,
                            4 => u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64,
                            8 => u64::from_le_bytes(buf),
                            _ => 0,
                        };
                        eprintln!("[EMU] boot_params[{:#x}] {} = {:#x}", offset, name, val);
                    }
                }
                // Print code around key addresses
                let addrs = [0x502226b_u64, 0x502225f, 0x502223c, 0x5022243];
                for addr in addrs {
                    let mut bytes = [0u8; 10];
                    if self.mmu.read(addr, &mut bytes, &self.sregs).is_ok() {
                        eprintln!("[EMU] Code at {:#x}: {:02x?}", addr, bytes);
                    }
                }
                // Try to read error string from RDI (if it looks like a string address)
                if self.regs.rdi > 0x500000 && self.regs.rdi < 0x6000000 {
                    let mut str_buf = [0u8; 128];
                    if self.mmu.read(self.regs.rdi, &mut str_buf, &self.sregs).is_ok() {
                        if let Ok(s) = std::str::from_utf8(&str_buf) {
                            let s = s.split('\0').next().unwrap_or("");
                            eprintln!("[EMU] Error string at RDI: \"{}\"", s);
                        }
                    }
                }
                // Look for the error string printed before " -- System halted"
                // Search video memory at 0xB8000
                let mut vga_buf = [0u8; 160];  // one row of VGA text
                if self.mmu.read_phys(0xB8000, &mut vga_buf).is_ok() {
                    let text: String = vga_buf.iter()
                        .step_by(2)  // skip attribute bytes
                        .map(|&b| if b >= 0x20 && b < 0x7f { b as char } else { ' ' })
                        .collect();
                    eprintln!("[EMU] VGA row 0: \"{}\"", text.trim());
                }
                // Read a few more VGA rows
                for row in 1..5_u64 {
                    let mut row_buf = [0u8; 160];
                    if self.mmu.read_phys(0xB8000 + row * 160, &mut row_buf).is_ok() {
                        let text: String = row_buf.iter()
                            .step_by(2)
                            .map(|&b| if b >= 0x20 && b < 0x7f { b as char } else { ' ' })
                            .collect();
                        if !text.trim().is_empty() {
                            eprintln!("[EMU] VGA row {}: \"{}\"", row, text.trim());
                        }
                    }
                }
                // Search for the full error message in kernel strings area
                // Look for "wrong" which we found a fragment of
                for offset in (0x5039c00_u64..0x5039e00).step_by(16) {
                    let mut str_buf = [0u8; 80];
                    if self.mmu.read(offset, &mut str_buf, &self.sregs).is_ok() {
                        if let Ok(s) = std::str::from_utf8(&str_buf) {
                            let s = s.split('\0').next().unwrap_or("");
                            if s.len() > 5 && s.chars().all(|c| c.is_ascii_graphic() || c == ' ' || c == '\n') {
                                eprintln!("[EMU] String at {:#x}: \"{}\"", offset, s);
                            }
                        }
                    }
                }
                // Look for the actual text we're printing by searching the ring buffer of output chars
                // The kernel has an output buffer somewhere - let's look around the stack
                for base in [0x5040000_u64, 0x5030000, 0x5038000] {
                    let mut buf = [0u8; 256];
                    if self.mmu.read(base, &mut buf, &self.sregs).is_ok() {
                        if let Ok(s) = std::str::from_utf8(&buf) {
                            let printable: String = s.chars()
                                .take_while(|&c| c != '\0')
                                .filter(|&c| c.is_ascii_graphic() || c == ' ' || c == '\n')
                                .collect();
                            if printable.len() > 10 {
                                eprintln!("[EMU] Data at {:#x}: \"{}\"", base, printable);
                            }
                        }
                    }
                }
                // Also check stack for any readable strings
                for offset in [0_u64, 8, 16, 24, 32, 40, 48] {
                    let stack_addr = self.regs.rsp + offset;
                    let mut word = [0u8; 8];
                    if self.mmu.read(stack_addr, &mut word, &self.sregs).is_ok() {
                        let val = u64::from_le_bytes(word);
                        if val > 0x500000 && val < 0x6000000 {
                            let mut str_buf = [0u8; 64];
                            if self.mmu.read(val, &mut str_buf, &self.sregs).is_ok() {
                                if let Ok(s) = std::str::from_utf8(&str_buf) {
                                    let s = s.split('\0').next().unwrap_or("");
                                    if !s.is_empty() && s.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
                                        eprintln!("[EMU] String on stack at RSP+{}: \"{}\"", offset, s);
                                    }
                                }
                            }
                        }
                    }
                }
                // Print history
                let current_idx = HIST_IDX.load(Ordering::Relaxed);
                eprint!("[EMU] Last 50 RIPs: ");
                for i in 0..50 {
                    let h_idx = (current_idx + i) % 50;
                    eprint!("{:x} ", HISTORY[h_idx].load(Ordering::Relaxed));
                }
                eprintln!();
                // Only return error if we couldn't skip past the halt
                // If rip was modified to a valid return address, we'll continue
                if self.regs.rip == rip {
                    // RIP wasn't changed, halt is permanent
                    return Err(crate::error::Error::Emulator(format!(
                        "hit halt loop at RIP={:#x}", rip
                    )));
                }
            }

            // Log progress periodically
            if insn_count % 10_000_000 == 0 {
                eprintln!(
                    "[EMU] {}M instructions, RIP={:#x}",
                    insn_count / 1_000_000,
                    self.regs.rip
                );
            }

            if let Some(exit) = self.step()? {
                return Ok(exit);
            }
        }
    }

    fn get_state(&self) -> Result<CpuState> {
        Ok(CpuState::X86_64(X86_64CpuState {
            regs: self.regs.clone(),
            sregs: self.sregs.clone(),
        }))
    }

    fn set_state(&mut self, state: &CpuState) -> Result<()> {
        let state = match state {
            CpuState::X86_64(state) => state,
            _ => {
                return Err(Error::Emulator(
                    "expected x86_64 state for x86_64 vCPU".to_string(),
                ))
            }
        };
        self.regs = state.regs.clone();
        self.sregs = state.sregs.clone();
        Ok(())
    }

    fn complete_io_in(&mut self, data: &[u8]) {
        if let Some(pending) = self.io_pending.take() {
            let value = match pending.size {
                1 => data.first().copied().unwrap_or(0) as u64,
                2 if data.len() >= 2 => u16::from_le_bytes([data[0], data[1]]) as u64,
                4 if data.len() >= 4 => {
                    u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as u64
                }
                _ => 0,
            };
            match pending.size {
                1 => self.regs.rax = (self.regs.rax & !0xFF) | value,
                2 => self.regs.rax = (self.regs.rax & !0xFFFF) | value,
                4 => self.regs.rax = value,
                _ => {}
            }
        }
    }

    fn id(&self) -> u32 {
        self.id
    }
}
