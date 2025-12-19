//! x86_64 CPU state and core execution loop.

use std::sync::Arc;

use vm_memory::GuestMemoryMmap;

use super::decoder::Decoder;
use super::flags;
use super::insn;
use super::mmu::Mmu;
use crate::cpu::{Registers, SystemRegisters, VCpu, VcpuExit};
use crate::error::{Error, Result};

/// Maximum instruction length for x86_64.
const MAX_INSN_LEN: usize = 15;

/// Emulated x86_64 vCPU.
pub struct X86_64Vcpu {
    id: u32,
    pub(super) regs: Registers,
    pub(super) sregs: SystemRegisters,
    pub(super) mmu: Mmu,
    pub(super) halted: bool,
    io_pending: Option<IoPending>,
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
            halted: false,
            io_pending: None,
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
    fn step(&mut self) -> Result<Option<VcpuExit>> {
        let bytes = self.fetch()?;

        // Decode prefixes
        let mut ctx = Decoder::decode_prefixes(bytes)?;

        // Determine operand size
        ctx.op_size = if ctx.rex_w() {
            8
        } else if ctx.operand_size_override {
            2
        } else {
            4
        };

        // Get opcode
        let opcode = ctx.consume_u8()?;

        // Execute instruction
        self.execute(opcode, &mut ctx)
    }

    /// Main instruction dispatch.
    fn execute(&mut self, opcode: u8, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        match opcode {
            // NOP
            0x90 => {
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // HLT
            0xF4 => {
                self.regs.rip += ctx.cursor as u64;
                self.halted = true;
                Ok(Some(VcpuExit::Hlt))
            }

            // Two-byte opcode (0x0F prefix)
            0x0F => self.execute_0f(ctx),

            // Control flow
            0xEB => insn::control::jmp_rel8(self, ctx),
            0xE9 => insn::control::jmp_rel32(self, ctx),
            0xE8 => insn::control::call_rel32(self, ctx),
            0xC3 => insn::control::ret(self, ctx),
            0xC2 => insn::control::ret_imm16(self, ctx),
            0xCB => insn::control::retf(self, ctx),
            0x70..=0x7F => insn::control::jcc_rel8(self, ctx, opcode & 0x0F),

            // I/O
            0xE4 => insn::io::in_al_imm8(self, ctx),
            0xE5 => insn::io::in_ax_imm8(self, ctx),
            0xEC => insn::io::in_al_dx(self, ctx),
            0xED => insn::io::in_ax_dx(self, ctx),
            0xE6 => insn::io::out_imm8_al(self, ctx),
            0xE7 => insn::io::out_imm8_ax(self, ctx),
            0xEE => insn::io::out_dx_al(self, ctx),
            0xEF => insn::io::out_dx_ax(self, ctx),

            // Data movement
            0xB0..=0xB7 => insn::data::mov_r8_imm8(self, ctx, opcode),
            0xB8..=0xBF => insn::data::mov_r_imm(self, ctx, opcode),
            0x88 => insn::data::mov_rm8_r8(self, ctx),
            0x89 => insn::data::mov_rm_r(self, ctx),
            0x8A => insn::data::mov_r8_rm8(self, ctx),
            0x8B => insn::data::mov_r_rm(self, ctx),
            0x8C => insn::data::mov_rm_sreg(self, ctx),
            0x8E => insn::data::mov_sreg_rm(self, ctx),
            0x8D => insn::data::lea(self, ctx),
            0xC6 => insn::data::mov_rm8_imm8(self, ctx),
            0xC7 => insn::data::mov_rm_imm(self, ctx),
            0x50..=0x57 => insn::data::push_r64(self, ctx, opcode),
            0x58..=0x5F => insn::data::pop_r64(self, ctx, opcode),
            0x6A => insn::data::push_imm8(self, ctx),
            0x68 => insn::data::push_imm32(self, ctx),
            0x87 => insn::data::xchg_r_rm(self, ctx),
            0x91..=0x97 => insn::data::xchg_rax_r(self, ctx, opcode),
            0x63 => insn::data::movsxd(self, ctx),

            // Arithmetic
            0x00 => insn::arith::add_rm8_r8(self, ctx),
            0x01 => insn::arith::add_rm_r(self, ctx),
            0x02 => insn::arith::add_r8_rm8(self, ctx),
            0x03 => insn::arith::add_r_rm(self, ctx),
            0x04 => insn::arith::add_al_imm8(self, ctx),
            0x05 => insn::arith::add_rax_imm(self, ctx),
            0x28 => insn::arith::sub_rm8_r8(self, ctx),
            0x29 => insn::arith::sub_rm_r(self, ctx),
            0x2A => insn::arith::sub_r8_rm8(self, ctx),
            0x2B => insn::arith::sub_r_rm(self, ctx),
            0x2C => insn::arith::sub_al_imm8(self, ctx),
            0x2D => insn::arith::sub_rax_imm(self, ctx),
            0x38 => insn::arith::cmp_rm8_r8(self, ctx),
            0x39 => insn::arith::cmp_rm_r(self, ctx),
            0x3A => insn::arith::cmp_r8_rm8(self, ctx),
            0x3B => insn::arith::cmp_r_rm(self, ctx),
            0x3C => insn::arith::cmp_al_imm8(self, ctx),
            0x3D => insn::arith::cmp_rax_imm(self, ctx),
            0x80 => insn::arith::group1_rm8_imm8(self, ctx),
            0x81 => insn::arith::group1_rm_imm32(self, ctx),
            0x83 => insn::arith::group1_rm_imm8(self, ctx),

            // Logic
            0x08 => insn::logic::or_rm8_r8(self, ctx),
            0x09 => insn::logic::or_rm_r(self, ctx),
            0x0A => insn::logic::or_r8_rm8(self, ctx),
            0x0B => insn::logic::or_r_rm(self, ctx),
            0x20 => insn::logic::and_rm8_r8(self, ctx),
            0x21 => insn::logic::and_rm_r(self, ctx),
            0x22 => insn::logic::and_r8_rm8(self, ctx),
            0x23 => insn::logic::and_r_rm(self, ctx),
            0x30 => insn::logic::xor_rm8_r8(self, ctx),
            0x31 => insn::logic::xor_rm_r(self, ctx),
            0x32 => insn::logic::xor_r8_rm8(self, ctx),
            0x33 => insn::logic::xor_r_rm(self, ctx),
            0x84 => insn::logic::test_rm8_r8(self, ctx),
            0x85 => insn::logic::test_rm_r(self, ctx),
            0xA8 => insn::logic::test_al_imm8(self, ctx),
            0xA9 => insn::logic::test_rax_imm(self, ctx),
            0xF6 => insn::logic::group3_rm8(self, ctx),
            0xF7 => insn::logic::group3_rm(self, ctx),

            // Shifts/Rotates
            0xC0 => insn::shift::group2_rm8_imm8(self, ctx),
            0xC1 => insn::shift::group2_rm_imm8(self, ctx),
            0xD0 => insn::shift::group2_rm8_1(self, ctx),
            0xD1 => insn::shift::group2_rm_1(self, ctx),
            0xD2 => insn::shift::group2_rm8_cl(self, ctx),
            0xD3 => insn::shift::group2_rm_cl(self, ctx),

            // System/Flags
            0xFA => insn::system::cli(self, ctx),
            0xFB => insn::system::sti(self, ctx),
            0xF8 => insn::system::clc(self, ctx),
            0xF9 => insn::system::stc(self, ctx),
            0xFC => insn::system::cld(self, ctx),
            0xFD => insn::system::std(self, ctx),
            0x9C => insn::system::pushf(self, ctx),
            0x9D => insn::system::popf(self, ctx),

            // Misc
            0xC9 => insn::data::leave(self, ctx),
            0xFF => insn::control::group5(self, ctx),
            0x62 => insn::data::bound_or_evex(self, ctx),

            // String operations (handled with REP prefix check)
            0xA4 => insn::string::movsb(self, ctx),
            0xA5 => insn::string::movs(self, ctx),
            0xAA => insn::string::stosb(self, ctx),
            0xAB => insn::string::stos(self, ctx),
            0xAC => insn::string::lodsb(self, ctx),
            0xAD => insn::string::lods(self, ctx),
            0xAE => insn::string::scasb(self, ctx),
            0xAF => insn::string::scas(self, ctx),
            0xA6 => insn::string::cmpsb(self, ctx),
            0xA7 => insn::string::cmps(self, ctx),

            _ => Err(Error::Emulator(format!(
                "unimplemented opcode: {:#04x} at RIP={:#x}",
                opcode, self.regs.rip
            ))),
        }
    }

    /// Execute two-byte opcodes (0x0F prefix).
    fn execute_0f(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let opcode2 = ctx.consume_u8()?;

        match opcode2 {
            // System
            0xA2 => insn::system::cpuid(self, ctx),
            0x30 => insn::system::wrmsr(self, ctx),
            0x32 => insn::system::rdmsr(self, ctx),
            0x01 => insn::system::group7(self, ctx),
            0x20 => insn::system::mov_r_cr(self, ctx),
            0x22 => insn::system::mov_cr_r(self, ctx),

            // Control flow
            0x80..=0x8F => insn::control::jcc_rel32(self, ctx, opcode2 & 0x0F),
            0x90..=0x9F => insn::control::setcc(self, ctx, opcode2 & 0x0F),

            // Data movement
            0xB6 => insn::data::movzx_r_rm8(self, ctx),
            0xB7 => insn::data::movzx_r_rm16(self, ctx),
            0xBE => insn::data::movsx_r_rm8(self, ctx),
            0xBF => insn::data::movsx_r_rm16(self, ctx),
            0xC8..=0xCF => insn::data::bswap(self, ctx, opcode2),

            // Arithmetic
            0xAF => insn::arith::imul_r_rm(self, ctx),

            // Bit manipulation
            0xA3 => insn::bit::bt_rm_r(self, ctx),
            0xAB => insn::bit::bts_rm_r(self, ctx),
            0xB3 => insn::bit::btr_rm_r(self, ctx),
            0xBB => insn::bit::btc_rm_r(self, ctx),
            0xBA => insn::bit::group8(self, ctx),
            0xBC => insn::bit::bsf(self, ctx),
            0xBD => insn::bit::bsr(self, ctx),

            // NOP variants
            0x1E => insn::system::endbr(self, ctx),
            0x1F => insn::system::nop_rm(self, ctx),

            _ => Err(Error::Emulator(format!(
                "unimplemented 0x0F opcode: {:#04x} at RIP={:#x}",
                opcode2, self.regs.rip
            ))),
        }
    }

    // Register access methods
    pub(super) fn get_reg(&self, reg: u8, size: u8) -> u64 {
        let val = match reg & 0x0F {
            0 => self.regs.rax,
            1 => self.regs.rcx,
            2 => self.regs.rdx,
            3 => self.regs.rbx,
            4 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    // AH in legacy mode
                    (self.regs.rax >> 8) & 0xFF
                } else {
                    self.regs.rsp
                }
            }
            5 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    // CH in legacy mode
                    (self.regs.rcx >> 8) & 0xFF
                } else {
                    self.regs.rbp
                }
            }
            6 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    // DH in legacy mode
                    (self.regs.rdx >> 8) & 0xFF
                } else {
                    self.regs.rsi
                }
            }
            7 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    // BH in legacy mode
                    (self.regs.rbx >> 8) & 0xFF
                } else {
                    self.regs.rdi
                }
            }
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

    pub(super) fn set_reg(&mut self, reg: u8, value: u64, size: u8) {
        let reg_ref = match reg & 0x0F {
            0 => &mut self.regs.rax,
            1 => &mut self.regs.rcx,
            2 => &mut self.regs.rdx,
            3 => &mut self.regs.rbx,
            4 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    // AH in legacy mode
                    self.regs.rax = (self.regs.rax & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                &mut self.regs.rsp
            }
            5 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    self.regs.rcx = (self.regs.rcx & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                &mut self.regs.rbp
            }
            6 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    self.regs.rdx = (self.regs.rdx & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                &mut self.regs.rsi
            }
            7 => {
                if size == 1 && self.sregs.efer & 0x400 == 0 {
                    self.regs.rbx = (self.regs.rbx & !0xFF00) | ((value & 0xFF) << 8);
                    return;
                }
                &mut self.regs.rdi
            }
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
        loop {
            if self.halted {
                return Ok(VcpuExit::Hlt);
            }

            if let Some(exit) = self.step()? {
                return Ok(exit);
            }
        }
    }

    fn get_regs(&self) -> Result<Registers> {
        Ok(self.regs.clone())
    }

    fn set_regs(&mut self, regs: &Registers) -> Result<()> {
        self.regs = regs.clone();
        Ok(())
    }

    fn get_sregs(&self) -> Result<SystemRegisters> {
        Ok(self.sregs.clone())
    }

    fn set_sregs(&mut self, sregs: &SystemRegisters) -> Result<()> {
        self.sregs = sregs.clone();
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
