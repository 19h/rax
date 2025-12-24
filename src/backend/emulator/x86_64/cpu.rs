//! x86_64 CPU state and core execution loop.

use std::cell::Cell;
#[cfg(debug_assertions)]
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicU64;
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

/// Type of lazy flag operation - determines how to compute flags on demand
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum LazyFlagOp {
    /// No lazy flags - rflags is valid
    None,
    /// Add operation: CF = result < a, OF = signed overflow
    Add,
    /// Sub/CMP operation: CF = a < b (borrow), OF = signed overflow
    Sub,
    /// Logic operation (AND/OR/XOR/TEST): CF=OF=0
    Logic,
    /// Inc operation: like Add but CF preserved
    Inc,
    /// Dec operation: like Sub but CF preserved
    Dec,
}

/// Lazy flag state - stores operands to compute flags on demand
#[derive(Clone, Copy, Debug)]
pub(super) struct LazyFlags {
    pub op: LazyFlagOp,
    pub result: u64,
    pub src: u64,      // First operand (a)
    pub dst: u64,      // Second operand (b) - only used for Add/Sub
    pub size: u8,
}

impl Default for LazyFlags {
    fn default() -> Self {
        LazyFlags {
            op: LazyFlagOp::None,
            result: 0,
            src: 0,
            dst: 0,
            size: 4,
        }
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
    /// IA32_KERNEL_GS_BASE MSR (0xC0000102) for SWAPGS
    pub(super) kernel_gs_base: u64,
    /// Protection Key Rights Register (PKRU).
    pub(super) pkru: u32,
    /// Decoded instruction cache for avoiding re-decode in hot loops
    decode_cache: Box<[DecodeCacheEntry; DECODE_CACHE_SIZE]>,
    /// Lazy flag state for deferred flag computation (Cell for interior mutability in get_state)
    pub(super) lazy_flags: Cell<LazyFlags>,
}

/// Pending I/O operation.
enum IoInTarget {
    Reg,
    Mem { addr: u64 },
}

struct IoPending {
    size: u8,
    target: IoInTarget,
}

/// Maximum instruction length in bytes.
pub const MAX_INSN_LEN: usize = 15;

/// Decode cache size (must be power of 2 for fast indexing)
const DECODE_CACHE_SIZE: usize = 4096;
const DECODE_CACHE_MASK: usize = DECODE_CACHE_SIZE - 1;

/// Cached decoded instruction entry
#[derive(Clone, Copy, Debug)]
struct DecodeCacheEntry {
    /// RIP where this instruction lives (0 = invalid)
    rip: u64,
    /// Primary opcode byte
    opcode: u8,
    /// Decoded operand size
    op_size: u8,
    /// Cursor position after prefix decode (start of opcode)
    cursor: usize,
    /// REX prefix if present
    rex: Option<u8>,
    /// 0x66 prefix
    operand_size_override: bool,
    /// 0x67 prefix
    address_size_override: bool,
    /// REP/REPNE prefix
    rep_prefix: Option<u8>,
}

impl Default for DecodeCacheEntry {
    #[inline(always)]
    fn default() -> Self {
        DecodeCacheEntry {
            rip: 0,
            opcode: 0,
            op_size: 4,
            cursor: 0,
            rex: None,
            operand_size_override: false,
            address_size_override: false,
            rep_prefix: None,
        }
    }
}

/// Decoded instruction context passed to instruction handlers.
pub(super) struct InsnContext {
    /// Instruction bytes (fixed-size to avoid allocation)
    pub bytes: [u8; MAX_INSN_LEN],
    /// Actual number of valid bytes
    pub bytes_len: usize,
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
    #[inline(always)]
    pub fn rex_w(&self) -> bool {
        self.rex.map_or(false, |r| r & 0x08 != 0)
    }

    /// Get REX.R flag (extends ModR/M reg field).
    #[inline(always)]
    pub fn rex_r(&self) -> u8 {
        self.rex.map_or(0, |r| (r & 0x04) << 1)
    }

    /// Get REX.B flag (extends ModR/M r/m field or opcode reg).
    #[inline(always)]
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
    #[inline(always)]
    pub fn consume_u8(&mut self) -> Result<u8> {
        if self.cursor >= self.bytes_len {
            return Err(Error::Emulator("instruction too short".to_string()));
        }
        let b = self.bytes[self.cursor];
        self.cursor += 1;
        Ok(b)
    }

    /// Peek at the next byte without consuming.
    #[inline(always)]
    #[allow(dead_code)]
    pub fn peek_u8(&self) -> Result<u8> {
        if self.cursor >= self.bytes_len {
            return Err(Error::Emulator("instruction too short".to_string()));
        }
        Ok(self.bytes[self.cursor])
    }

    /// Consume and return a little-endian u16.
    #[inline(always)]
    pub fn consume_u16(&mut self) -> Result<u16> {
        if self.cursor + 2 > self.bytes_len {
            return Err(Error::Emulator("instruction too short for u16".to_string()));
        }
        let val = u16::from_le_bytes([self.bytes[self.cursor], self.bytes[self.cursor + 1]]);
        self.cursor += 2;
        Ok(val)
    }

    /// Consume and return a little-endian u32.
    #[inline(always)]
    pub fn consume_u32(&mut self) -> Result<u32> {
        if self.cursor + 4 > self.bytes_len {
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
    #[inline(always)]
    pub fn consume_u64(&mut self) -> Result<u64> {
        if self.cursor + 8 > self.bytes_len {
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
        // Use vec! to heap-allocate the cache, then convert to boxed array
        let cache_vec = vec![DecodeCacheEntry::default(); DECODE_CACHE_SIZE];
        let decode_cache: Box<[DecodeCacheEntry; DECODE_CACHE_SIZE]> =
            cache_vec.into_boxed_slice().try_into().unwrap();

        X86_64Vcpu {
            id,
            regs: Registers::default(),
            sregs: SystemRegisters::default(),
            mmu: Mmu::new(mem),
            fpu: FpuState::default(),
            halted: false,
            io_pending: None,
            kernel_gs_base: 0,
            pkru: 0,
            decode_cache,
            lazy_flags: Cell::new(LazyFlags::default()),
        }
    }

    /// Materialize lazy flags into rflags.
    /// Call this before any instruction that reads flags (Jcc, CMOVcc, SETcc, ADC, SBB, PUSHF, LAHF).
    #[inline]
    pub(super) fn materialize_flags(&mut self) {
        let lf = self.lazy_flags.get();
        if lf.op == LazyFlagOp::None {
            return; // Flags already materialized
        }

        let result = lf.result;
        let a = lf.src;
        let b = lf.dst;
        let size = lf.size;

        let mask = match size {
            1 => 0xFFu64,
            2 => 0xFFFFu64,
            4 => 0xFFFF_FFFFu64,
            _ => u64::MAX,
        };
        let result_m = result & mask;
        let a_m = a & mask;
        let b_m = b & mask;

        let sign_bit = match size {
            1 => 0x80u64,
            2 => 0x8000u64,
            4 => 0x8000_0000u64,
            _ => 0x8000_0000_0000_0000u64,
        };

        // Common flags for all operations
        let zf = result_m == 0;
        let sf = (result_m & sign_bit) != 0;
        let pf = (result as u8).count_ones() % 2 == 0;

        // Clear status flags (preserve CF for Inc/Dec)
        let cf_mask = if lf.op == LazyFlagOp::Inc || lf.op == LazyFlagOp::Dec {
            0 // Don't clear CF for INC/DEC
        } else {
            flags::bits::CF
        };
        self.regs.rflags &= !(cf_mask | flags::bits::ZF | flags::bits::SF | flags::bits::PF | flags::bits::OF | flags::bits::AF);

        // Set common flags
        if zf { self.regs.rflags |= flags::bits::ZF; }
        if sf { self.regs.rflags |= flags::bits::SF; }
        if pf { self.regs.rflags |= flags::bits::PF; }

        // Operation-specific flags
        match lf.op {
            LazyFlagOp::Add | LazyFlagOp::Inc => {
                let cf = result_m < a_m;
                let of = ((a_m ^ result_m) & (b_m ^ result_m) & sign_bit) != 0;
                let af = ((a_m ^ b_m ^ result_m) & 0x10) != 0;
                if lf.op == LazyFlagOp::Add && cf { self.regs.rflags |= flags::bits::CF; }
                if of { self.regs.rflags |= flags::bits::OF; }
                if af { self.regs.rflags |= flags::bits::AF; }
            }
            LazyFlagOp::Sub | LazyFlagOp::Dec => {
                let cf = a_m < b_m;
                let of = ((a_m ^ b_m) & (a_m ^ result_m) & sign_bit) != 0;
                let af = ((a_m ^ b_m ^ result_m) & 0x10) != 0;
                if lf.op == LazyFlagOp::Sub && cf { self.regs.rflags |= flags::bits::CF; }
                if of { self.regs.rflags |= flags::bits::OF; }
                if af { self.regs.rflags |= flags::bits::AF; }
            }
            LazyFlagOp::Logic => {
                // CF=0, OF=0 already cleared above; AF is undefined
            }
            LazyFlagOp::None => {}
        }

        // Mark flags as materialized
        self.lazy_flags.set(LazyFlags { op: LazyFlagOp::None, ..lf });
    }

    /// Compute what rflags would be if lazy flags were materialized (without modifying self).
    /// Used by get_state() to return accurate flags via &self.
    #[inline]
    fn compute_materialized_rflags(&self) -> u64 {
        let lf = self.lazy_flags.get();
        if lf.op == LazyFlagOp::None {
            return self.regs.rflags; // Already materialized
        }

        let result = lf.result;
        let a = lf.src;
        let b = lf.dst;
        let size = lf.size;

        let mask = match size {
            1 => 0xFFu64,
            2 => 0xFFFFu64,
            4 => 0xFFFF_FFFFu64,
            _ => u64::MAX,
        };
        let result_m = result & mask;
        let a_m = a & mask;
        let b_m = b & mask;

        let sign_bit = match size {
            1 => 0x80u64,
            2 => 0x8000u64,
            4 => 0x8000_0000u64,
            _ => 0x8000_0000_0000_0000u64,
        };

        // Common flags for all operations
        let zf = result_m == 0;
        let sf = (result_m & sign_bit) != 0;
        let pf = (result as u8).count_ones() % 2 == 0;

        // Start with current rflags, clear status flags (preserve CF for Inc/Dec)
        let cf_mask = if lf.op == LazyFlagOp::Inc || lf.op == LazyFlagOp::Dec {
            0 // Don't clear CF for INC/DEC
        } else {
            flags::bits::CF
        };
        let mut rflags = self.regs.rflags & !(cf_mask | flags::bits::ZF | flags::bits::SF | flags::bits::PF | flags::bits::OF | flags::bits::AF);

        // Set common flags
        if zf { rflags |= flags::bits::ZF; }
        if sf { rflags |= flags::bits::SF; }
        if pf { rflags |= flags::bits::PF; }

        // Operation-specific flags
        match lf.op {
            LazyFlagOp::Add | LazyFlagOp::Inc => {
                let cf = result_m < a_m;
                let of = ((a_m ^ result_m) & (b_m ^ result_m) & sign_bit) != 0;
                let af = ((a_m ^ b_m ^ result_m) & 0x10) != 0;
                if lf.op == LazyFlagOp::Add && cf { rflags |= flags::bits::CF; }
                if of { rflags |= flags::bits::OF; }
                if af { rflags |= flags::bits::AF; }
            }
            LazyFlagOp::Sub | LazyFlagOp::Dec => {
                let cf = a_m < b_m;
                let of = ((a_m ^ b_m) & (a_m ^ result_m) & sign_bit) != 0;
                let af = ((a_m ^ b_m ^ result_m) & 0x10) != 0;
                if lf.op == LazyFlagOp::Sub && cf { rflags |= flags::bits::CF; }
                if of { rflags |= flags::bits::OF; }
                if af { rflags |= flags::bits::AF; }
            }
            LazyFlagOp::Logic => {
                // CF=0, OF=0 already cleared above; AF is undefined
            }
            LazyFlagOp::None => {}
        }

        rflags
    }

    /// Set lazy flags for an Add operation
    #[inline(always)]
    pub(super) fn set_lazy_add(&mut self, a: u64, b: u64, result: u64, size: u8) {
        self.lazy_flags.set(LazyFlags {
            op: LazyFlagOp::Add,
            result,
            src: a,
            dst: b,
            size,
        });
    }

    /// Set lazy flags for a Sub/CMP operation
    #[inline(always)]
    pub(super) fn set_lazy_sub(&mut self, a: u64, b: u64, result: u64, size: u8) {
        self.lazy_flags.set(LazyFlags {
            op: LazyFlagOp::Sub,
            result,
            src: a,
            dst: b,
            size,
        });
    }

    /// Set lazy flags for a Logic operation (AND/OR/XOR/TEST)
    #[inline(always)]
    pub(super) fn set_lazy_logic(&mut self, result: u64, size: u8) {
        self.lazy_flags.set(LazyFlags {
            op: LazyFlagOp::Logic,
            result,
            src: 0,
            dst: 0,
            size,
        });
    }

    /// Set lazy flags for an Inc operation (CF preserved)
    #[inline(always)]
    pub(super) fn set_lazy_inc(&mut self, a: u64, result: u64, size: u8) {
        self.lazy_flags.set(LazyFlags {
            op: LazyFlagOp::Inc,
            result,
            src: a,
            dst: 1,
            size,
        });
    }

    /// Set lazy flags for a Dec operation (CF preserved)
    #[inline(always)]
    pub(super) fn set_lazy_dec(&mut self, a: u64, result: u64, size: u8) {
        self.lazy_flags.set(LazyFlags {
            op: LazyFlagOp::Dec,
            result,
            src: a,
            dst: 1,
            size,
        });
    }

    /// Clear lazy flags state (call after directly writing to rflags)
    #[inline(always)]
    pub(super) fn clear_lazy_flags(&mut self) {
        let lf = self.lazy_flags.get();
        self.lazy_flags.set(LazyFlags { op: LazyFlagOp::None, ..lf });
    }

    /// Fetch instruction bytes from RIP into a stack buffer.
    /// Returns (buffer, actual_length).
    #[inline]
    fn fetch(&mut self) -> Result<([u8; MAX_INSN_LEN], usize)> {
        let mut buf = [0u8; MAX_INSN_LEN];
        if self.mmu.read(self.regs.rip, &mut buf, &self.sregs).is_ok() {
            return Ok((buf, MAX_INSN_LEN));
        }
        // If we can't read 15 bytes, try smaller amounts
        for len in (1..MAX_INSN_LEN).rev() {
            if self.mmu.read(self.regs.rip, &mut buf[..len], &self.sregs).is_ok() {
                return Ok((buf, len));
            }
        }
        Err(Error::Emulator(format!(
            "failed to fetch instruction at RIP={:#x}",
            self.regs.rip
        )))
    }

    /// Compute decode cache index from RIP
    #[inline(always)]
    fn decode_cache_index(rip: u64) -> usize {
        (rip as usize) & DECODE_CACHE_MASK
    }

    /// Execute a single instruction.
    #[inline]
    pub fn step(&mut self) -> Result<Option<VcpuExit>> {
        // Update global RIP tracker for debugging (only in debug builds)
        #[cfg(debug_assertions)]
        CURRENT_RIP.store(self.regs.rip, Ordering::Relaxed);

        let rip = self.regs.rip;
        let cache_idx = Self::decode_cache_index(rip);

        // Check decode cache for a hit (copy to avoid borrow issues)
        let cached = self.decode_cache[cache_idx];
        if cached.rip == rip {
            // Cache hit! Fetch bytes and reconstruct context from cached decode
            let (bytes, bytes_len) = self.fetch()?;
            let mut ctx = InsnContext {
                bytes,
                bytes_len,
                cursor: cached.cursor + 1, // Skip past opcode byte
                rex: cached.rex,
                operand_size_override: cached.operand_size_override,
                address_size_override: cached.address_size_override,
                rep_prefix: cached.rep_prefix,
                op_size: cached.op_size,
                rip_relative_offset: 0,
                evex: None,
            };
            return self.execute(cached.opcode, &mut ctx);
        }

        // Cache miss - do full decode
        let (bytes, bytes_len) = self.fetch()?;

        // Decode prefixes
        let mut ctx = Decoder::decode_prefixes(bytes, bytes_len)?;

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

        // Save cursor before consuming opcode (for cache)
        let opcode_cursor = ctx.cursor;

        // Get opcode
        let opcode = ctx.consume_u8()?;

        // Cache the decoded instruction
        self.decode_cache[cache_idx] = DecodeCacheEntry {
            rip,
            opcode,
            op_size: ctx.op_size,
            cursor: opcode_cursor,
            rex: ctx.rex,
            operand_size_override: ctx.operand_size_override,
            address_size_override: ctx.address_size_override,
            rep_prefix: ctx.rep_prefix,
        };

        // Execute instruction
        self.execute(opcode, &mut ctx)
    }

    // Register access methods
    #[inline(always)]
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
    #[inline(always)]
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
    #[inline(always)]
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

    #[inline(always)]
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
    #[inline(always)]
    pub(super) fn read_mem(&mut self, addr: u64, size: u8) -> Result<u64> {
        match size {
            1 => Ok(self.mmu.read_u8(addr, &self.sregs)? as u64),
            2 => Ok(self.mmu.read_u16(addr, &self.sregs)? as u64),
            4 => Ok(self.mmu.read_u32(addr, &self.sregs)? as u64),
            8 => Ok(self.mmu.read_u64(addr, &self.sregs)?),
            _ => Err(Error::Emulator(format!("invalid memory access size: {}", size))),
        }
    }

    #[inline(always)]
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
    #[inline(always)]
    pub(super) fn read_mem16(&mut self, addr: u64) -> Result<u16> {
        self.mmu.read_u16(addr, &self.sregs)
    }

    #[inline(always)]
    pub(super) fn write_mem16(&mut self, addr: u64, value: u16) -> Result<()> {
        self.mmu.write_u16(addr, value, &self.sregs)
    }

    #[inline(always)]
    pub(super) fn read_mem32(&mut self, addr: u64) -> Result<u32> {
        self.mmu.read_u32(addr, &self.sregs)
    }

    #[inline(always)]
    pub(super) fn write_mem32(&mut self, addr: u64, value: u32) -> Result<()> {
        self.mmu.write_u32(addr, value, &self.sregs)
    }

    #[inline(always)]
    pub(super) fn read_mem64(&mut self, addr: u64) -> Result<u64> {
        self.mmu.read_u64(addr, &self.sregs)
    }

    #[inline(always)]
    pub(super) fn write_mem64(&mut self, addr: u64, value: u64) -> Result<()> {
        self.mmu.write_u64(addr, value, &self.sregs)
    }

    #[inline(always)]
    pub(super) fn read_f32(&mut self, addr: u64) -> Result<f32> {
        let bits = self.mmu.read_u32(addr, &self.sregs)?;
        Ok(f32::from_bits(bits))
    }

    #[inline(always)]
    pub(super) fn write_f32(&mut self, addr: u64, value: f32) -> Result<()> {
        self.mmu.write_u32(addr, value.to_bits(), &self.sregs)
    }

    #[inline(always)]
    pub(super) fn read_f64(&mut self, addr: u64) -> Result<f64> {
        let bits = self.mmu.read_u64(addr, &self.sregs)?;
        Ok(f64::from_bits(bits))
    }

    #[inline(always)]
    pub(super) fn write_f64(&mut self, addr: u64, value: f64) -> Result<()> {
        self.mmu.write_u64(addr, value.to_bits(), &self.sregs)
    }

    #[inline]
    pub(super) fn read_bytes(&mut self, addr: u64, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.mmu.read(addr, &mut buf, &self.sregs)?;
        Ok(buf)
    }

    #[inline]
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
    pub(super) fn set_io_pending_reg(&mut self, size: u8) {
        self.io_pending = Some(IoPending {
            size,
            target: IoInTarget::Reg,
        });
    }

    pub(super) fn set_io_pending_mem(&mut self, size: u8, addr: u64) {
        self.io_pending = Some(IoPending {
            size,
            target: IoInTarget::Mem { addr },
        });
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
        let preserve_mode = sreg == 1;
        let prev_db = seg.db;
        let prev_l = seg.l;
        seg.selector = value;
        // In protected/long mode, we should load the descriptor from GDT
        // For now, set a basic flat segment
        seg.base = 0;
        seg.limit = 0xFFFF_FFFF;
        seg.type_ = 0x03; // Data segment, read/write, accessed
        seg.present = true;
        seg.dpl = 0;
        seg.db = if preserve_mode { prev_db } else { true };
        seg.s = true;
        seg.l = if preserve_mode { prev_l } else { false };
        seg.g = true;
    }

    // Condition checking for Jcc/SETcc/CMOVcc - materializes lazy flags first
    pub(super) fn check_condition(&mut self, cc: u8) -> bool {
        // Materialize lazy flags before reading
        self.materialize_flags();

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
        loop {
            if self.halted {
                return Ok(VcpuExit::Hlt);
            }

            if let Some(exit) = self.step()? {
                return Ok(exit);
            }
        }
    }


    fn get_state(&self) -> Result<CpuState> {
        // Compute materialized rflags without modifying self
        let rflags = self.compute_materialized_rflags();
        let mut regs = self.regs.clone();
        regs.rflags = rflags;
        Ok(CpuState::X86_64(X86_64CpuState {
            regs,
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

            match pending.target {
                IoInTarget::Reg => match pending.size {
                    1 => self.regs.rax = (self.regs.rax & !0xFF) | value,
                    2 => self.regs.rax = (self.regs.rax & !0xFFFF) | value,
                    4 => self.regs.rax = value,
                    _ => {}
                },
                IoInTarget::Mem { addr } => {
                    let _ = self.write_mem(addr, value, pending.size);
                }
            }
        }
    }

    fn id(&self) -> u32 {
        self.id
    }
}
