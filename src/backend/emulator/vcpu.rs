//! Emulated vCPU implementation.

use std::sync::Arc;

use vm_memory::GuestMemoryMmap;

use super::flags;
use super::mmu::Mmu;
use crate::cpu::{Registers, SystemRegisters, VCpu, VcpuExit};
use crate::error::{Error, Result};

/// Maximum instruction length for x86_64.
const MAX_INSN_LEN: usize = 15;

/// Emulated vCPU.
pub struct EmulatorVcpu {
    id: u32,
    regs: Registers,
    sregs: SystemRegisters,
    mmu: Mmu,
    halted: bool,
    io_pending: Option<IoPending>,
}

/// Pending I/O operation.
struct IoPending {
    size: u8,
}

impl EmulatorVcpu {
    pub fn new(id: u32, mem: Arc<GuestMemoryMmap>) -> Self {
        EmulatorVcpu {
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
        // Try to read as many bytes as possible
        if let Err(_) = self.mmu.read(self.regs.rip, &mut buf, &self.sregs) {
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

        // Decode and execute
        let mut cursor = 0;
        let mut rex: Option<u8> = None;
        let mut operand_size_override = false;
        let mut address_size_override = false;
        let mut rep_prefix: Option<u8> = None; // 0xF2=REPNE, 0xF3=REP/REPE

        // Parse prefixes
        loop {
            if cursor >= bytes.len() {
                return Err(Error::Emulator("instruction too short".to_string()));
            }
            let b = bytes[cursor];
            match b {
                0x66 => operand_size_override = true,
                0x67 => address_size_override = true,
                0x40..=0x4F => rex = Some(b),
                0xF0 => {} // LOCK - ignore for now
                0xF2 | 0xF3 => rep_prefix = Some(b),
                0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 => {} // Segment overrides
                _ => break,
            }
            cursor += 1;
        }

        if cursor >= bytes.len() {
            return Err(Error::Emulator("instruction too short after prefixes".to_string()));
        }

        let opcode = bytes[cursor];
        cursor += 1;

        // Determine operand size
        let rex_w = rex.map_or(false, |r| r & 0x08 != 0);
        let op_size: u8 = if rex_w {
            8
        } else if operand_size_override {
            2
        } else {
            4
        };

        let _ = address_size_override; // Will be used later

        // Execute based on opcode
        match opcode {
            // NOP
            0x90 => {
                self.regs.rip += cursor as u64;
            }

            // HLT
            0xF4 => {
                self.regs.rip += cursor as u64;
                self.halted = true;
                return Ok(Some(VcpuExit::Hlt));
            }

            // CLI - Clear Interrupt Flag
            0xFA => {
                self.regs.rflags &= !flags::bits::IF;
                self.regs.rip += cursor as u64;
            }

            // STI - Set Interrupt Flag
            0xFB => {
                self.regs.rflags |= flags::bits::IF;
                self.regs.rip += cursor as u64;
            }

            // CLC - Clear Carry Flag
            0xF8 => {
                self.regs.rflags &= !flags::bits::CF;
                self.regs.rip += cursor as u64;
            }

            // STC - Set Carry Flag
            0xF9 => {
                self.regs.rflags |= flags::bits::CF;
                self.regs.rip += cursor as u64;
            }

            // CLD - Clear Direction Flag
            0xFC => {
                self.regs.rflags &= !flags::bits::DF;
                self.regs.rip += cursor as u64;
            }

            // STD - Set Direction Flag
            0xFD => {
                self.regs.rflags |= flags::bits::DF;
                self.regs.rip += cursor as u64;
            }

            // IN AL, imm8
            0xE4 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("IN: missing port".to_string()));
                }
                let port = bytes[cursor] as u16;
                cursor += 1;
                self.regs.rip += cursor as u64;
                self.io_pending = Some(IoPending { size: 1 });
                return Ok(Some(VcpuExit::IoIn { port, size: 1 }));
            }

            // IN AX/EAX, imm8
            0xE5 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("IN: missing port".to_string()));
                }
                let port = bytes[cursor] as u16;
                cursor += 1;
                let size = if operand_size_override { 2 } else { 4 };
                self.regs.rip += cursor as u64;
                self.io_pending = Some(IoPending { size });
                return Ok(Some(VcpuExit::IoIn { port, size }));
            }

            // IN AL, DX
            0xEC => {
                let port = self.regs.rdx as u16;
                self.regs.rip += cursor as u64;
                self.io_pending = Some(IoPending { size: 1 });
                return Ok(Some(VcpuExit::IoIn { port, size: 1 }));
            }

            // IN AX/EAX, DX
            0xED => {
                let port = self.regs.rdx as u16;
                let size = if operand_size_override { 2 } else { 4 };
                self.regs.rip += cursor as u64;
                self.io_pending = Some(IoPending { size });
                return Ok(Some(VcpuExit::IoIn { port, size }));
            }

            // OUT imm8, AL
            0xE6 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("OUT: missing port".to_string()));
                }
                let port = bytes[cursor] as u16;
                cursor += 1;
                self.regs.rip += cursor as u64;
                return Ok(Some(VcpuExit::IoOut {
                    port,
                    data: vec![self.regs.rax as u8],
                }));
            }

            // OUT imm8, AX/EAX
            0xE7 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("OUT: missing port".to_string()));
                }
                let port = bytes[cursor] as u16;
                cursor += 1;
                let data = if operand_size_override {
                    (self.regs.rax as u16).to_le_bytes().to_vec()
                } else {
                    (self.regs.rax as u32).to_le_bytes().to_vec()
                };
                self.regs.rip += cursor as u64;
                return Ok(Some(VcpuExit::IoOut { port, data }));
            }

            // OUT DX, AL
            0xEE => {
                let port = self.regs.rdx as u16;
                self.regs.rip += cursor as u64;
                return Ok(Some(VcpuExit::IoOut {
                    port,
                    data: vec![self.regs.rax as u8],
                }));
            }

            // OUT DX, AX/EAX
            0xEF => {
                let port = self.regs.rdx as u16;
                let data = if operand_size_override {
                    (self.regs.rax as u16).to_le_bytes().to_vec()
                } else {
                    (self.regs.rax as u32).to_le_bytes().to_vec()
                };
                self.regs.rip += cursor as u64;
                return Ok(Some(VcpuExit::IoOut { port, data }));
            }

            // MOV r8, imm8 (B0-B7)
            0xB0..=0xB7 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r8, imm8: missing immediate".to_string()));
                }
                let reg = (opcode - 0xB0) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let imm = bytes[cursor];
                cursor += 1;
                self.set_reg8(reg, imm);
                self.regs.rip += cursor as u64;
            }

            // MOV r16/32/64, imm16/32/64 (B8-BF)
            0xB8..=0xBF => {
                let reg = (opcode - 0xB8) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let imm = self.read_immediate(&bytes[cursor..], op_size)?;
                cursor += op_size as usize;
                self.set_reg(reg, imm, op_size);
                self.regs.rip += cursor as u64;
            }

            // Two-byte opcode (0x0F prefix)
            0x0F => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0x0F: missing second opcode byte".to_string()));
                }
                let opcode2 = bytes[cursor];
                cursor += 1;

                match opcode2 {
                    // CPUID
                    0xA2 => {
                        self.execute_cpuid();
                        self.regs.rip += cursor as u64;
                    }

                    // RDMSR
                    0x32 => {
                        let ecx = self.regs.rcx as u32;
                        let value = self.read_msr(ecx)?;
                        self.regs.rax = (value & 0xFFFF_FFFF) as u64;
                        self.regs.rdx = (value >> 32) as u64;
                        self.regs.rip += cursor as u64;
                    }

                    // WRMSR
                    0x30 => {
                        let ecx = self.regs.rcx as u32;
                        let value = ((self.regs.rdx & 0xFFFF_FFFF) << 32) | (self.regs.rax & 0xFFFF_FFFF);
                        self.write_msr(ecx, value)?;
                        self.regs.rip += cursor as u64;
                    }

                    // Jcc rel32 (0x80-0x8F)
                    0x80..=0x8F => {
                        if cursor + 4 > bytes.len() {
                            return Err(Error::Emulator("Jcc: missing displacement".to_string()));
                        }
                        let disp = i32::from_le_bytes([
                            bytes[cursor],
                            bytes[cursor + 1],
                            bytes[cursor + 2],
                            bytes[cursor + 3],
                        ]) as i64;
                        cursor += 4;
                        let cc = opcode2 & 0x0F;
                        if self.check_condition(cc) {
                            self.regs.rip = (self.regs.rip as i64 + cursor as i64 + disp) as u64;
                        } else {
                            self.regs.rip += cursor as u64;
                        }
                    }

                    // SETcc r/m8 (0x90-0x9F)
                    0x90..=0x9F => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("SETcc: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let cc = opcode2 & 0x0F;
                        let value = if self.check_condition(cc) { 1u8 } else { 0u8 };

                        if modrm >> 6 == 3 {
                            self.set_reg(rm, value as u64, 1);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.mmu.write_u8(addr, value, &self.sregs)?;
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // MOVZX r, r/m8 (0xB6)
                    0xB6 => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("MOVZX: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                        let value = if modrm >> 6 == 3 {
                            self.get_reg(rm, 1) as u8
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.mmu.read_u8(addr, &self.sregs)?
                        };
                        self.set_reg(reg, value as u64, op_size);
                        self.regs.rip += cursor as u64;
                    }

                    // MOVZX r, r/m16 (0xB7)
                    0xB7 => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("MOVZX: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                        let value = if modrm >> 6 == 3 {
                            self.get_reg(rm, 2) as u16
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.mmu.read_u16(addr, &self.sregs)?
                        };
                        self.set_reg(reg, value as u64, op_size);
                        self.regs.rip += cursor as u64;
                    }

                    // MOVSX r, r/m8 (0xBE)
                    0xBE => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("MOVSX: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                        let value = if modrm >> 6 == 3 {
                            self.get_reg(rm, 1) as u8
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.mmu.read_u8(addr, &self.sregs)?
                        };
                        // Sign-extend
                        let extended = value as i8 as i64 as u64;
                        self.set_reg(reg, extended, op_size);
                        self.regs.rip += cursor as u64;
                    }

                    // MOVSX r, r/m16 (0xBF)
                    0xBF => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("MOVSX: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                        let value = if modrm >> 6 == 3 {
                            self.get_reg(rm, 2) as u16
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.mmu.read_u16(addr, &self.sregs)?
                        };
                        // Sign-extend
                        let extended = value as i16 as i64 as u64;
                        self.set_reg(reg, extended, op_size);
                        self.regs.rip += cursor as u64;
                    }

                    // IMUL r, r/m (0xAF)
                    0xAF => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("IMUL: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let dst = self.get_reg(reg, op_size);

                        let src = if modrm >> 6 == 3 {
                            self.get_reg(rm, op_size)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, op_size)?
                        };

                        let result = match op_size {
                            2 => ((dst as i16).wrapping_mul(src as i16)) as u16 as u64,
                            4 => ((dst as i32).wrapping_mul(src as i32)) as u32 as u64,
                            8 => ((dst as i64).wrapping_mul(src as i64)) as u64,
                            _ => dst.wrapping_mul(src),
                        };
                        self.set_reg(reg, result, op_size);
                        // Flags: CF and OF are set if result was truncated
                        self.regs.rip += cursor as u64;
                    }

                    // BSWAP r32/r64 (0xC8-0xCF)
                    0xC8..=0xCF => {
                        let reg = (opcode2 - 0xC8) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let value = self.get_reg(reg, op_size);
                        let swapped = match op_size {
                            4 => (value as u32).swap_bytes() as u64,
                            8 => value.swap_bytes(),
                            _ => value,
                        };
                        self.set_reg(reg, swapped, op_size);
                        self.regs.rip += cursor as u64;
                    }

                    // ENDBR64/ENDBR32 (0x0F 0x1E) - CET instructions, treat as NOP
                    0x1E => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("ENDBR: missing ModR/M".to_string()));
                        }
                        // Skip the ModR/M byte (FA=ENDBR64, FB=ENDBR32)
                        cursor += 1;
                        self.regs.rip += cursor as u64;
                    }

                    // NOP (0x1F with ModR/M for multi-byte NOP)
                    0x1F => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("NOP: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        // Skip any additional bytes for memory operand
                        if modrm >> 6 != 3 {
                            let (_, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // Group 7 - LGDT, LIDT, INVLPG, etc. (0x0F 0x01)
                    0x01 => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("0F 01: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg_op = (modrm >> 3) & 0x07;

                        match reg_op {
                            // LGDT m16&64
                            2 => {
                                if modrm >> 6 == 3 {
                                    return Err(Error::Emulator("LGDT: requires memory operand".to_string()));
                                }
                                let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                                cursor += extra;
                                // Read 10 bytes: 2-byte limit + 8-byte base
                                let limit = self.mmu.read_u16(addr, &self.sregs)?;
                                let base = self.mmu.read_u64(addr + 2, &self.sregs)?;
                                self.sregs.gdt.limit = limit;
                                self.sregs.gdt.base = base;
                                self.regs.rip += cursor as u64;
                            }
                            // LIDT m16&64
                            3 => {
                                if modrm >> 6 == 3 {
                                    return Err(Error::Emulator("LIDT: requires memory operand".to_string()));
                                }
                                let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                                cursor += extra;
                                // Read 10 bytes: 2-byte limit + 8-byte base
                                let limit = self.mmu.read_u16(addr, &self.sregs)?;
                                let base = self.mmu.read_u64(addr + 2, &self.sregs)?;
                                self.sregs.idt.limit = limit;
                                self.sregs.idt.base = base;
                                self.regs.rip += cursor as u64;
                            }
                            // INVLPG m
                            7 => {
                                if modrm >> 6 == 3 {
                                    return Err(Error::Emulator("INVLPG: requires memory operand".to_string()));
                                }
                                let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                                cursor += extra;
                                // Invalidate TLB entry for address
                                self.mmu.invlpg(addr);
                                self.regs.rip += cursor as u64;
                            }
                            _ => {
                                return Err(Error::Emulator(format!(
                                    "unimplemented 0F 01 /{} at RIP={:#x}",
                                    reg_op, self.regs.rip
                                )));
                            }
                        }
                    }

                    // MOV r64, CRn (0x0F 0x20)
                    0x20 => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("MOV r, CR: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let cr = (modrm >> 3) & 0x07;
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let value = match cr {
                            0 => self.sregs.cr0,
                            2 => self.sregs.cr2,
                            3 => self.sregs.cr3,
                            4 => self.sregs.cr4,
                            _ => return Err(Error::Emulator(format!("MOV r, CR{}: unsupported", cr))),
                        };
                        self.set_reg(rm, value, 8);
                        self.regs.rip += cursor as u64;
                    }

                    // MOV CRn, r64 (0x0F 0x22)
                    0x22 => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("MOV CR, r: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let cr = (modrm >> 3) & 0x07;
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let value = self.get_reg(rm, 8);
                        match cr {
                            0 => self.sregs.cr0 = value,
                            2 => self.sregs.cr2 = value,
                            3 => self.sregs.cr3 = value,
                            4 => self.sregs.cr4 = value,
                            _ => return Err(Error::Emulator(format!("MOV CR{}, r: unsupported", cr))),
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // BT r/m, r (0x0F 0xA3)
                    0xA3 => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("BT: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let bit_offset = self.get_reg(reg, op_size);

                        let value = if modrm >> 6 == 3 {
                            self.get_reg(rm, op_size)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, op_size)?
                        };

                        let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);
                        let cf_bit = (value >> bit_pos) & 1;
                        if cf_bit != 0 {
                            self.regs.rflags |= flags::bits::CF;
                        } else {
                            self.regs.rflags &= !flags::bits::CF;
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // BTS r/m, r (0x0F 0xAB)
                    0xAB => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("BTS: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let bit_offset = self.get_reg(reg, op_size);
                        let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);

                        if modrm >> 6 == 3 {
                            let v = self.get_reg(rm, op_size);
                            let cf_bit = (v >> bit_pos) & 1;
                            if cf_bit != 0 { self.regs.rflags |= flags::bits::CF; } else { self.regs.rflags &= !flags::bits::CF; }
                            let new_val = v | (1 << bit_pos);
                            self.set_reg(rm, new_val, op_size);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let v = self.read_mem(addr, op_size)?;
                            let cf_bit = (v >> bit_pos) & 1;
                            if cf_bit != 0 { self.regs.rflags |= flags::bits::CF; } else { self.regs.rflags &= !flags::bits::CF; }
                            let new_val = v | (1 << bit_pos);
                            self.write_mem(addr, new_val, op_size)?;
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // BTR r/m, r (0x0F 0xB3)
                    0xB3 => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("BTR: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let bit_offset = self.get_reg(reg, op_size);
                        let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);

                        if modrm >> 6 == 3 {
                            let v = self.get_reg(rm, op_size);
                            let cf_bit = (v >> bit_pos) & 1;
                            if cf_bit != 0 { self.regs.rflags |= flags::bits::CF; } else { self.regs.rflags &= !flags::bits::CF; }
                            let new_val = v & !(1 << bit_pos);
                            self.set_reg(rm, new_val, op_size);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let v = self.read_mem(addr, op_size)?;
                            let cf_bit = (v >> bit_pos) & 1;
                            if cf_bit != 0 { self.regs.rflags |= flags::bits::CF; } else { self.regs.rflags &= !flags::bits::CF; }
                            let new_val = v & !(1 << bit_pos);
                            self.write_mem(addr, new_val, op_size)?;
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // BTC r/m, r (0x0F 0xBB)
                    0xBB => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("BTC: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                        let bit_offset = self.get_reg(reg, op_size);
                        let bit_pos = bit_offset & ((op_size * 8 - 1) as u64);

                        if modrm >> 6 == 3 {
                            let v = self.get_reg(rm, op_size);
                            let cf_bit = (v >> bit_pos) & 1;
                            if cf_bit != 0 { self.regs.rflags |= flags::bits::CF; } else { self.regs.rflags &= !flags::bits::CF; }
                            let new_val = v ^ (1 << bit_pos);
                            self.set_reg(rm, new_val, op_size);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let v = self.read_mem(addr, op_size)?;
                            let cf_bit = (v >> bit_pos) & 1;
                            if cf_bit != 0 { self.regs.rflags |= flags::bits::CF; } else { self.regs.rflags &= !flags::bits::CF; }
                            let new_val = v ^ (1 << bit_pos);
                            self.write_mem(addr, new_val, op_size)?;
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // BSF r, r/m (0x0F 0xBC)
                    0xBC => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("BSF: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                        let value = if modrm >> 6 == 3 {
                            self.get_reg(rm, op_size)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, op_size)?
                        };

                        if value == 0 {
                            self.regs.rflags |= flags::bits::ZF;
                            // Destination is undefined when source is 0
                        } else {
                            self.regs.rflags &= !flags::bits::ZF;
                            let bit_index = value.trailing_zeros() as u64;
                            self.set_reg(reg, bit_index, op_size);
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // BSR r, r/m (0x0F 0xBD)
                    0xBD => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("BSR: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                        let value = if modrm >> 6 == 3 {
                            self.get_reg(rm, op_size)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, op_size)?
                        };

                        if value == 0 {
                            self.regs.rflags |= flags::bits::ZF;
                            // Destination is undefined when source is 0
                        } else {
                            self.regs.rflags &= !flags::bits::ZF;
                            let bit_size = (op_size * 8) as u32;
                            let bit_index = (bit_size - 1 - value.leading_zeros()) as u64;
                            self.set_reg(reg, bit_index, op_size);
                        }
                        self.regs.rip += cursor as u64;
                    }

                    // Group 8 - BT/BTS/BTR/BTC with immediate (0x0F 0xBA)
                    0xBA => {
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("0F BA: missing ModR/M".to_string()));
                        }
                        let modrm = bytes[cursor];
                        cursor += 1;
                        let reg_op = (modrm >> 3) & 0x07;
                        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                        let (value, addr_opt) = if modrm >> 6 == 3 {
                            (self.get_reg(rm, op_size), None)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            (self.read_mem(addr, op_size)?, Some(addr))
                        };

                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("0F BA: missing imm8".to_string()));
                        }
                        let bit_pos = (bytes[cursor] & ((op_size * 8 - 1) as u8)) as u64;
                        cursor += 1;

                        let cf_bit = (value >> bit_pos) & 1;
                        if cf_bit != 0 { self.regs.rflags |= flags::bits::CF; } else { self.regs.rflags &= !flags::bits::CF; }

                        match reg_op {
                            4 => {} // BT - just test
                            5 => {  // BTS - test and set
                                let new_val = value | (1 << bit_pos);
                                if let Some(addr) = addr_opt {
                                    self.write_mem(addr, new_val, op_size)?;
                                } else {
                                    self.set_reg(rm, new_val, op_size);
                                }
                            }
                            6 => {  // BTR - test and reset
                                let new_val = value & !(1 << bit_pos);
                                if let Some(addr) = addr_opt {
                                    self.write_mem(addr, new_val, op_size)?;
                                } else {
                                    self.set_reg(rm, new_val, op_size);
                                }
                            }
                            7 => {  // BTC - test and complement
                                let new_val = value ^ (1 << bit_pos);
                                if let Some(addr) = addr_opt {
                                    self.write_mem(addr, new_val, op_size)?;
                                } else {
                                    self.set_reg(rm, new_val, op_size);
                                }
                            }
                            _ => {
                                return Err(Error::Emulator(format!(
                                    "unimplemented 0F BA /{} at RIP={:#x}",
                                    reg_op, self.regs.rip
                                )));
                            }
                        }
                        self.regs.rip += cursor as u64;
                    }

                    _ => {
                        return Err(Error::Emulator(format!(
                            "unimplemented 0x0F opcode: {:#04x} at RIP={:#x}",
                            opcode2, self.regs.rip
                        )));
                    }
                }
            }

            // JMP rel8
            0xEB => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("JMP rel8: missing displacement".to_string()));
                }
                let disp = bytes[cursor] as i8 as i64;
                cursor += 1;
                self.regs.rip = (self.regs.rip as i64 + cursor as i64 + disp) as u64;
            }

            // JMP rel32
            0xE9 => {
                if cursor + 4 > bytes.len() {
                    return Err(Error::Emulator("JMP rel32: missing displacement".to_string()));
                }
                let disp = i32::from_le_bytes([
                    bytes[cursor],
                    bytes[cursor + 1],
                    bytes[cursor + 2],
                    bytes[cursor + 3],
                ]) as i64;
                cursor += 4;
                self.regs.rip = (self.regs.rip as i64 + cursor as i64 + disp) as u64;
            }

            // CALL rel32
            0xE8 => {
                if cursor + 4 > bytes.len() {
                    return Err(Error::Emulator("CALL rel32: missing displacement".to_string()));
                }
                let disp = i32::from_le_bytes([
                    bytes[cursor],
                    bytes[cursor + 1],
                    bytes[cursor + 2],
                    bytes[cursor + 3],
                ]) as i64;
                cursor += 4;
                let ret_addr = self.regs.rip + cursor as u64;
                self.push64(ret_addr)?;
                self.regs.rip = (self.regs.rip as i64 + cursor as i64 + disp) as u64;
            }

            // RET
            0xC3 => {
                let ret_addr = self.pop64()?;
                self.regs.rip = ret_addr;
            }

            // RETF (far return)
            0xCB => {
                let ret_addr = self.pop64()?;
                let cs = self.pop64()? as u16;
                self.regs.rip = ret_addr;
                self.set_sreg(1, cs); // CS is segment register 1
            }

            // Jcc rel8 (0x70-0x7F)
            0x70..=0x7F => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("Jcc rel8: missing displacement".to_string()));
                }
                let disp = bytes[cursor] as i8 as i64;
                cursor += 1;
                let cc = opcode & 0x0F;
                if self.check_condition(cc) {
                    self.regs.rip = (self.regs.rip as i64 + cursor as i64 + disp) as u64;
                } else {
                    self.regs.rip += cursor as u64;
                }
            }

            // PUSH r64 (50-57)
            0x50..=0x57 => {
                let reg = (opcode - 0x50) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let value = self.get_reg(reg, 8);
                self.push64(value)?;
                self.regs.rip += cursor as u64;
            }

            // PUSH imm8 (sign-extended to 64-bit)
            0x6A => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("PUSH imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as i8 as i64 as u64;
                cursor += 1;
                self.push64(imm)?;
                self.regs.rip += cursor as u64;
            }

            // 0x62 - BOUND (32-bit) or EVEX prefix (64-bit)
            0x62 => {
                // Check if we're in 64-bit mode by looking at EFER.LMA
                let in_long_mode = (self.sregs.efer & 0x400) != 0; // EFER.LMA = bit 10

                if in_long_mode {
                    // In 64-bit mode, 0x62 is EVEX prefix (AVX-512)
                    // Show full debug info
                    let all_bytes: Vec<u8> = bytes.iter().take(16).cloned().collect();
                    let context_bytes: Vec<u8> = bytes[cursor..].iter().take(8).cloned().collect();
                    return Err(Error::Emulator(format!(
                        "EVEX at RIP={:#x}, all fetched bytes: {:02x?}, bytes after 0x62: {:02x?}, cursor={}",
                        self.regs.rip, all_bytes, context_bytes, cursor
                    )));
                } else {
                    // In 32-bit mode, this is BOUND (bounds check)
                    if cursor >= bytes.len() {
                        return Err(Error::Emulator("BOUND: missing ModR/M".to_string()));
                    }
                    let modrm = bytes[cursor];
                    cursor += 1;
                    // Skip memory operand (BOUND always uses memory)
                    if modrm >> 6 != 3 {
                        let (_, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                        cursor += extra;
                    }
                    // BOUND doesn't do anything if bounds are OK (we assume they are)
                    self.regs.rip += cursor as u64;
                }
            }

            // PUSH imm32 (sign-extended to 64-bit)
            0x68 => {
                if cursor + 4 > bytes.len() {
                    return Err(Error::Emulator("PUSH imm32: missing immediate".to_string()));
                }
                let imm = i32::from_le_bytes([
                    bytes[cursor],
                    bytes[cursor + 1],
                    bytes[cursor + 2],
                    bytes[cursor + 3],
                ]) as i64 as u64;
                cursor += 4;
                self.push64(imm)?;
                self.regs.rip += cursor as u64;
            }

            // POP r64 (58-5F)
            0x58..=0x5F => {
                let reg = (opcode - 0x58) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let value = self.pop64()?;
                self.set_reg(reg, value, 8);
                self.regs.rip += cursor as u64;
            }

            // MOV Sreg, r/m16 (0x8E)
            0x8E => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV Sreg: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let sreg = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let value = if modrm >> 6 == 3 {
                    // Register source
                    self.get_reg(rm, 2) as u16
                } else {
                    // Memory source
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.read_u16(addr, &self.sregs)?
                };

                self.set_sreg(sreg, value);
                self.regs.rip += cursor as u64;
            }

            // MOV r/m, Sreg (0x8C)
            0x8C => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r/m, Sreg: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let sreg = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let value = self.get_sreg(sreg);

                if modrm >> 6 == 3 {
                    self.set_reg(rm, value as u64, 2);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.write_u16(addr, value, &self.sregs)?;
                }
                self.regs.rip += cursor as u64;
            }

            // LEA r, m (0x8D)
            0x8D => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("LEA: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));

                let (addr, extra) = self.decode_modrm_addr(&bytes[cursor..], rex, cursor)?;
                cursor += 1 + extra;
                self.set_reg(reg, addr, op_size);
                self.regs.rip += cursor as u64;
            }

            // MOV r/m, r (0x89)
            0x89 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r/m, r: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let value = self.get_reg(reg, op_size);

                if modrm >> 6 == 3 {
                    self.set_reg(rm, value, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.write_mem(addr, value, op_size)?;
                }
                self.regs.rip += cursor as u64;
            }

            // MOV r, r/m (0x8B)
            0x8B => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r, r/m: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let value = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                self.set_reg(reg, value, op_size);
                self.regs.rip += cursor as u64;
            }

            // MOV r/m8, r8 (0x88)
            0x88 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r/m8, r8: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let value = self.get_reg(reg, 1) as u8;

                if modrm >> 6 == 3 {
                    self.set_reg(rm, value as u64, 1);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.write_u8(addr, value, &self.sregs)?;
                }
                self.regs.rip += cursor as u64;
            }

            // MOV r8, r/m8 (0x8A)
            0x8A => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r8, r/m8: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let value = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1) as u8
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)?
                };
                self.set_reg(reg, value as u64, 1);
                self.regs.rip += cursor as u64;
            }

            // ADD r/m8, r8 (0x00)
            0x00 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("ADD: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, 1);

                if modrm >> 6 == 3 {
                    let dst = self.get_reg(rm, 1);
                    let result = dst.wrapping_add(src) & 0xFF;
                    self.set_reg(rm, result, 1);
                    flags::update_flags_add(&mut self.regs.rflags, dst, src, result, 1);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let dst = self.mmu.read_u8(addr, &self.sregs)? as u64;
                    let result = dst.wrapping_add(src) & 0xFF;
                    self.mmu.write_u8(addr, result as u8, &self.sregs)?;
                    flags::update_flags_add(&mut self.regs.rflags, dst, src, result, 1);
                }
                self.regs.rip += cursor as u64;
            }

            // ADD r/m, r (0x01)
            0x01 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("ADD: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, op_size);

                if modrm >> 6 == 3 {
                    let dst = self.get_reg(rm, op_size);
                    let result = dst.wrapping_add(src);
                    self.set_reg(rm, result, op_size);
                    flags::update_flags_add(&mut self.regs.rflags, dst, src, result, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let dst = self.read_mem(addr, op_size)?;
                    let result = dst.wrapping_add(src);
                    self.write_mem(addr, result, op_size)?;
                    flags::update_flags_add(&mut self.regs.rflags, dst, src, result, op_size);
                }
                self.regs.rip += cursor as u64;
            }

            // ADD r8, r/m8 (0x02)
            0x02 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("ADD: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let dst = self.get_reg(reg, 1);

                let src = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)? as u64
                };
                let result = dst.wrapping_add(src) & 0xFF;
                self.set_reg(reg, result, 1);
                flags::update_flags_add(&mut self.regs.rflags, dst, src, result, 1);
                self.regs.rip += cursor as u64;
            }

            // ADD r, r/m (0x03)
            0x03 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("ADD: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let dst = self.get_reg(reg, op_size);

                let src = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst.wrapping_add(src);
                self.set_reg(reg, result, op_size);
                flags::update_flags_add(&mut self.regs.rflags, dst, src, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // SUB r/m, r (0x29)
            0x29 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("SUB: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, op_size);

                if modrm >> 6 == 3 {
                    let dst = self.get_reg(rm, op_size);
                    let result = dst.wrapping_sub(src);
                    self.set_reg(rm, result, op_size);
                    flags::update_flags_sub(&mut self.regs.rflags, dst, src, result, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let dst = self.read_mem(addr, op_size)?;
                    let result = dst.wrapping_sub(src);
                    self.write_mem(addr, result, op_size)?;
                    flags::update_flags_sub(&mut self.regs.rflags, dst, src, result, op_size);
                }
                self.regs.rip += cursor as u64;
            }

            // SUB r, r/m (0x2B)
            0x2B => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("SUB: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let dst = self.get_reg(reg, op_size);

                let src = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst.wrapping_sub(src);
                self.set_reg(reg, result, op_size);
                flags::update_flags_sub(&mut self.regs.rflags, dst, src, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // CMP r/m8, r8 (0x38)
            0x38 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("CMP r/m8, r8: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, 1) as u8;

                let dst = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1) as u8
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)?
                };
                let result = (dst as u8).wrapping_sub(src) as u64;
                flags::update_flags_sub(&mut self.regs.rflags, dst as u64, src as u64, result, 1);
                self.regs.rip += cursor as u64;
            }

            // CMP r/m, r (0x39)
            0x39 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("CMP: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, op_size);

                let dst = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst.wrapping_sub(src);
                flags::update_flags_sub(&mut self.regs.rflags, dst, src, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // CMP r, r/m (0x3B)
            0x3B => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("CMP: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let dst = self.get_reg(reg, op_size);

                let src = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst.wrapping_sub(src);
                flags::update_flags_sub(&mut self.regs.rflags, dst, src, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // TEST r/m8, r8 (0x84)
            0x84 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("TEST r/m8, r8: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, 1) as u8;

                let dst = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1) as u8
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)?
                };
                let result = (dst & src) as u64;
                flags::update_flags_logic(&mut self.regs.rflags, result, 1);
                self.regs.rip += cursor as u64;
            }

            // TEST r/m, r (0x85)
            0x85 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("TEST: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, op_size);

                let dst = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst & src;
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // AND r/m, r (0x21)
            0x21 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("AND: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, op_size);

                if modrm >> 6 == 3 {
                    let dst = self.get_reg(rm, op_size);
                    let result = dst & src;
                    self.set_reg(rm, result, op_size);
                    flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let dst = self.read_mem(addr, op_size)?;
                    let result = dst & src;
                    self.write_mem(addr, result, op_size)?;
                    flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                }
                self.regs.rip += cursor as u64;
            }

            // AND r, r/m (0x23)
            0x23 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("AND: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let dst = self.get_reg(reg, op_size);

                let src = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst & src;
                self.set_reg(reg, result, op_size);
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // OR r/m, r (0x09)
            0x09 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("OR: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, op_size);

                if modrm >> 6 == 3 {
                    let dst = self.get_reg(rm, op_size);
                    let result = dst | src;
                    self.set_reg(rm, result, op_size);
                    flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let dst = self.read_mem(addr, op_size)?;
                    let result = dst | src;
                    self.write_mem(addr, result, op_size)?;
                    flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                }
                self.regs.rip += cursor as u64;
            }

            // OR r, r/m (0x0B)
            0x0B => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("OR: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let dst = self.get_reg(reg, op_size);

                let src = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst | src;
                self.set_reg(reg, result, op_size);
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // XOR r/m, r (0x31)
            0x31 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("XOR: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let src = self.get_reg(reg, op_size);

                if modrm >> 6 == 3 {
                    let dst = self.get_reg(rm, op_size);
                    let result = dst ^ src;
                    self.set_reg(rm, result, op_size);
                    flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let dst = self.read_mem(addr, op_size)?;
                    let result = dst ^ src;
                    self.write_mem(addr, result, op_size)?;
                    flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                }
                self.regs.rip += cursor as u64;
            }

            // XOR r, r/m (0x33)
            0x33 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("XOR: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let dst = self.get_reg(reg, op_size);

                let src = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };
                let result = dst ^ src;
                self.set_reg(reg, result, op_size);
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // Group 1: Immediate byte operations (0x80) - r/m8, imm8
            0x80 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0x80 group: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let dst = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)? as u64
                };

                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0x80 group: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;

                let (result, update_dest) = match op {
                    0 => { // ADD
                        let r = (dst as u8).wrapping_add(imm as u8) as u64;
                        flags::update_flags_add(&mut self.regs.rflags, dst, imm, r, 1);
                        (r, true)
                    }
                    1 => { // OR
                        let r = (dst | imm) & 0xFF;
                        flags::update_flags_logic(&mut self.regs.rflags, r, 1);
                        (r, true)
                    }
                    2 => { // ADC
                        let cf = if self.regs.rflags & flags::bits::CF != 0 { 1u8 } else { 0 };
                        let r = (dst as u8).wrapping_add(imm as u8).wrapping_add(cf) as u64;
                        flags::update_flags_add(&mut self.regs.rflags, dst, imm.wrapping_add(cf as u64), r, 1);
                        (r, true)
                    }
                    3 => { // SBB
                        let cf = if self.regs.rflags & flags::bits::CF != 0 { 1u8 } else { 0 };
                        let r = (dst as u8).wrapping_sub(imm as u8).wrapping_sub(cf) as u64;
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm.wrapping_add(cf as u64), r, 1);
                        (r, true)
                    }
                    4 => { // AND
                        let r = (dst & imm) & 0xFF;
                        flags::update_flags_logic(&mut self.regs.rflags, r, 1);
                        (r, true)
                    }
                    5 => { // SUB
                        let r = (dst as u8).wrapping_sub(imm as u8) as u64;
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm, r, 1);
                        (r, true)
                    }
                    6 => { // XOR
                        let r = (dst ^ imm) & 0xFF;
                        flags::update_flags_logic(&mut self.regs.rflags, r, 1);
                        (r, true)
                    }
                    7 => { // CMP
                        let r = (dst as u8).wrapping_sub(imm as u8) as u64;
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm, r, 1);
                        (r, false)
                    }
                    _ => return Err(Error::Emulator(format!("invalid 0x80 /op: {}", op))),
                };

                if update_dest {
                    if modrm >> 6 == 3 {
                        self.set_reg(rm, result, 1);
                    } else {
                        let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                        self.mmu.write_u8(addr, result as u8, &self.sregs)?;
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // Group 1: Immediate operations (0x83) - r/m, imm8 sign-extended
            0x83 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0x83 group: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let dst = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };

                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0x83 group: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as i8 as i64 as u64;
                cursor += 1;

                let (result, update_dest) = match op {
                    0 => { // ADD
                        let r = dst.wrapping_add(imm);
                        flags::update_flags_add(&mut self.regs.rflags, dst, imm, r, op_size);
                        (r, true)
                    }
                    1 => { // OR
                        let r = dst | imm;
                        flags::update_flags_logic(&mut self.regs.rflags, r, op_size);
                        (r, true)
                    }
                    2 => { // ADC
                        let cf = if self.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };
                        let r = dst.wrapping_add(imm).wrapping_add(cf);
                        flags::update_flags_add(&mut self.regs.rflags, dst, imm.wrapping_add(cf), r, op_size);
                        (r, true)
                    }
                    3 => { // SBB
                        let cf = if self.regs.rflags & flags::bits::CF != 0 { 1u64 } else { 0 };
                        let r = dst.wrapping_sub(imm).wrapping_sub(cf);
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm.wrapping_add(cf), r, op_size);
                        (r, true)
                    }
                    4 => { // AND
                        let r = dst & imm;
                        flags::update_flags_logic(&mut self.regs.rflags, r, op_size);
                        (r, true)
                    }
                    5 => { // SUB
                        let r = dst.wrapping_sub(imm);
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm, r, op_size);
                        (r, true)
                    }
                    6 => { // XOR
                        let r = dst ^ imm;
                        flags::update_flags_logic(&mut self.regs.rflags, r, op_size);
                        (r, true)
                    }
                    7 => { // CMP
                        let r = dst.wrapping_sub(imm);
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm, r, op_size);
                        (r, false)
                    }
                    _ => return Err(Error::Emulator(format!("invalid 0x83 /op: {}", op))),
                };

                if update_dest {
                    if modrm >> 6 == 3 {
                        self.set_reg(rm, result, op_size);
                    } else {
                        // Re-decode address (we already consumed the modrm)
                        let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                        self.write_mem(addr, result, op_size)?;
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // Group 1: Immediate operations (0x81) - r/m, imm32
            0x81 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0x81 group: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let modrm_start = cursor - 1;

                let dst = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };

                // Read immediate (32-bit, sign-extended for 64-bit)
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;

                let (result, update_dest) = match op {
                    0 => { // ADD
                        let r = dst.wrapping_add(imm);
                        flags::update_flags_add(&mut self.regs.rflags, dst, imm, r, op_size);
                        (r, true)
                    }
                    1 => { // OR
                        let r = dst | imm;
                        flags::update_flags_logic(&mut self.regs.rflags, r, op_size);
                        (r, true)
                    }
                    4 => { // AND
                        let r = dst & imm;
                        flags::update_flags_logic(&mut self.regs.rflags, r, op_size);
                        (r, true)
                    }
                    5 => { // SUB
                        let r = dst.wrapping_sub(imm);
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm, r, op_size);
                        (r, true)
                    }
                    6 => { // XOR
                        let r = dst ^ imm;
                        flags::update_flags_logic(&mut self.regs.rflags, r, op_size);
                        (r, true)
                    }
                    7 => { // CMP
                        let r = dst.wrapping_sub(imm);
                        flags::update_flags_sub(&mut self.regs.rflags, dst, imm, r, op_size);
                        (r, false)
                    }
                    _ => return Err(Error::Emulator(format!("unimplemented 0x81 /op: {}", op))),
                };

                if update_dest {
                    if modrm >> 6 == 3 {
                        self.set_reg(rm, result, op_size);
                    } else {
                        let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                        self.write_mem(addr, result, op_size)?;
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // TEST r/m8, imm8 (0xF6 /0)
            0xF6 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xF6: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                match op {
                    0 | 1 => { // TEST r/m8, imm8
                        let dst = if modrm >> 6 == 3 {
                            self.get_reg(rm, 1)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.mmu.read_u8(addr, &self.sregs)? as u64
                        };
                        if cursor >= bytes.len() {
                            return Err(Error::Emulator("TEST: missing immediate".to_string()));
                        }
                        let imm = bytes[cursor] as u64;
                        cursor += 1;
                        let result = dst & imm;
                        flags::update_flags_logic(&mut self.regs.rflags, result, 1);
                    }
                    2 => { // NOT r/m8
                        if modrm >> 6 == 3 {
                            let val = self.get_reg(rm, 1);
                            self.set_reg(rm, !val, 1);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let val = self.mmu.read_u8(addr, &self.sregs)?;
                            self.mmu.write_u8(addr, !val, &self.sregs)?;
                        }
                    }
                    3 => { // NEG r/m8
                        if modrm >> 6 == 3 {
                            let val = self.get_reg(rm, 1) as u8;
                            let result = (val as i8).wrapping_neg() as u8;
                            self.set_reg(rm, result as u64, 1);
                            flags::update_flags_sub(&mut self.regs.rflags, 0, val as u64, result as u64, 1);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let val = self.mmu.read_u8(addr, &self.sregs)?;
                            let result = (val as i8).wrapping_neg() as u8;
                            self.mmu.write_u8(addr, result, &self.sregs)?;
                            flags::update_flags_sub(&mut self.regs.rflags, 0, val as u64, result as u64, 1);
                        }
                    }
                    _ => return Err(Error::Emulator(format!("unimplemented 0xF6 /op: {}", op))),
                }
                self.regs.rip += cursor as u64;
            }

            // TEST r/m, imm / NOT / NEG (0xF7)
            0xF7 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xF7: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let modrm_start = cursor - 1;

                match op {
                    0 | 1 => { // TEST r/m, imm
                        let dst = if modrm >> 6 == 3 {
                            self.get_reg(rm, op_size)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, op_size)?
                        };
                        let imm_size = if op_size == 8 { 4 } else { op_size };
                        let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                        let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                        cursor += imm_size as usize;
                        let result = dst & imm;
                        flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                    }
                    2 => { // NOT r/m
                        if modrm >> 6 == 3 {
                            let val = self.get_reg(rm, op_size);
                            self.set_reg(rm, !val, op_size);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let val = self.read_mem(addr, op_size)?;
                            self.write_mem(addr, !val, op_size)?;
                        }
                    }
                    3 => { // NEG r/m
                        if modrm >> 6 == 3 {
                            let val = self.get_reg(rm, op_size);
                            let result = match op_size {
                                1 => (val as i8).wrapping_neg() as u8 as u64,
                                2 => (val as i16).wrapping_neg() as u16 as u64,
                                4 => (val as i32).wrapping_neg() as u32 as u64,
                                8 => (val as i64).wrapping_neg() as u64,
                                _ => val,
                            };
                            self.set_reg(rm, result, op_size);
                            flags::update_flags_sub(&mut self.regs.rflags, 0, val, result, op_size);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                            cursor += extra;
                            let val = self.read_mem(addr, op_size)?;
                            let result = match op_size {
                                1 => (val as i8).wrapping_neg() as u8 as u64,
                                2 => (val as i16).wrapping_neg() as u16 as u64,
                                4 => (val as i32).wrapping_neg() as u32 as u64,
                                8 => (val as i64).wrapping_neg() as u64,
                                _ => val,
                            };
                            self.write_mem(addr, result, op_size)?;
                            flags::update_flags_sub(&mut self.regs.rflags, 0, val, result, op_size);
                        }
                    }
                    _ => return Err(Error::Emulator(format!("unimplemented 0xF7 /op: {}", op))),
                }
                self.regs.rip += cursor as u64;
            }

            // XCHG r, r/m (0x87)
            0x87 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("XCHG: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let reg_val = self.get_reg(reg, op_size);

                if modrm >> 6 == 3 {
                    let rm_val = self.get_reg(rm, op_size);
                    self.set_reg(reg, rm_val, op_size);
                    self.set_reg(rm, reg_val, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let mem_val = self.read_mem(addr, op_size)?;
                    self.set_reg(reg, mem_val, op_size);
                    self.write_mem(addr, reg_val, op_size)?;
                }
                self.regs.rip += cursor as u64;
            }

            // XCHG EAX/RAX, r (0x91-0x97) - 0x90 is NOP handled above
            0x91..=0x97 => {
                let reg = (opcode - 0x90) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let rax_val = self.get_reg(0, op_size);
                let reg_val = self.get_reg(reg, op_size);
                self.set_reg(0, reg_val, op_size);
                self.set_reg(reg, rax_val, op_size);
                self.regs.rip += cursor as u64;
            }

            // MOVSXD r64, r/m32 (0x63 with REX.W)
            0x63 => {
                if !rex_w {
                    return Err(Error::Emulator("MOVSXD without REX.W not supported".to_string()));
                }
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOVSXD: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let reg = ((modrm >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x04) << 1));
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let value = if modrm >> 6 == 3 {
                    self.get_reg(rm, 4) as u32
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    self.mmu.read_u32(addr, &self.sregs)?
                };
                // Sign-extend 32-bit to 64-bit
                let extended = value as i32 as i64 as u64;
                self.set_reg(reg, extended, 8);
                self.regs.rip += cursor as u64;
            }

            // INC r64 (0xFF /0) and DEC r64 (0xFF /1) - handled below
            // Group 5: INC/DEC/CALL/JMP/PUSH (0xFF)
            0xFF => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xFF group: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                match op {
                    0 => {
                        // INC r/m
                        if modrm >> 6 == 3 {
                            let val = self.get_reg(rm, op_size);
                            let result = val.wrapping_add(1);
                            self.set_reg(rm, result, op_size);
                            flags::update_flags_add(&mut self.regs.rflags, val, 1, result, op_size);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let val = self.read_mem(addr, op_size)?;
                            let result = val.wrapping_add(1);
                            self.write_mem(addr, result, op_size)?;
                            flags::update_flags_add(&mut self.regs.rflags, val, 1, result, op_size);
                        }
                    }
                    1 => {
                        // DEC r/m
                        if modrm >> 6 == 3 {
                            let val = self.get_reg(rm, op_size);
                            let result = val.wrapping_sub(1);
                            self.set_reg(rm, result, op_size);
                            flags::update_flags_sub(&mut self.regs.rflags, val, 1, result, op_size);
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            let val = self.read_mem(addr, op_size)?;
                            let result = val.wrapping_sub(1);
                            self.write_mem(addr, result, op_size)?;
                            flags::update_flags_sub(&mut self.regs.rflags, val, 1, result, op_size);
                        }
                    }
                    2 => {
                        // CALL r/m64
                        let target = if modrm >> 6 == 3 {
                            self.get_reg(rm, 8)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, 8)?
                        };
                        let ret_addr = self.regs.rip + cursor as u64;
                        self.push64(ret_addr)?;
                        self.regs.rip = target;
                        return Ok(None);
                    }
                    4 => {
                        // JMP r/m64
                        let target = if modrm >> 6 == 3 {
                            self.get_reg(rm, 8)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, 8)?
                        };
                        self.regs.rip = target;
                        return Ok(None);
                    }
                    6 => {
                        // PUSH r/m64
                        let val = if modrm >> 6 == 3 {
                            self.get_reg(rm, 8)
                        } else {
                            let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                            cursor += extra;
                            self.read_mem(addr, 8)?
                        };
                        self.push64(val)?;
                    }
                    _ => {
                        return Err(Error::Emulator(format!(
                            "unimplemented 0xFF /{}",
                            op
                        )));
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // MOV r/m, imm (0xC7 /0)
            0xC7 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r/m, imm: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let imm_size = if op_size == 8 { 4 } else { op_size }; // 64-bit uses 32-bit sign-extended
                if modrm >> 6 == 3 {
                    let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                    let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                    cursor += imm_size as usize;
                    self.set_reg(rm, imm, op_size);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                    let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                    cursor += imm_size as usize;
                    self.write_mem(addr, imm, op_size)?;
                }
                self.regs.rip += cursor as u64;
            }

            // MOV r/m8, imm8 (0xC6 /0)
            0xC6 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("MOV r/m8, imm8: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                cursor += 1;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                if modrm >> 6 == 3 {
                    if cursor >= bytes.len() {
                        return Err(Error::Emulator("MOV r/m8, imm8: missing immediate".to_string()));
                    }
                    let imm = bytes[cursor];
                    cursor += 1;
                    self.set_reg(rm, imm as u64, 1);
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[cursor - 1..], rex, cursor - 1)?;
                    cursor += extra;
                    if cursor >= bytes.len() {
                        return Err(Error::Emulator("MOV r/m8, imm8: missing immediate".to_string()));
                    }
                    let imm = bytes[cursor];
                    cursor += 1;
                    self.mmu.write_u8(addr, imm, &self.sregs)?;
                }
                self.regs.rip += cursor as u64;
            }

            // Shift/Rotate Group 2: r/m8, imm8 (0xC0)
            0xC0 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xC0: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let val = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1) as u8
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)?
                };

                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xC0: missing shift count".to_string()));
                }
                let count = bytes[cursor] & 0x1F;
                cursor += 1;

                let result = self.execute_shift8(op, val, count)?;

                if modrm >> 6 == 3 {
                    self.set_reg(rm, result as u64, 1);
                } else {
                    let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    self.mmu.write_u8(addr, result, &self.sregs)?;
                }
                self.regs.rip += cursor as u64;
            }

            // Shift/Rotate Group 2: r/m, imm8 (0xC1)
            0xC1 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xC1: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let val = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };

                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xC1: missing shift count".to_string()));
                }
                let count = bytes[cursor] & 0x3F;
                cursor += 1;

                let result = self.execute_shift(op, val, count, op_size)?;

                if modrm >> 6 == 3 {
                    self.set_reg(rm, result, op_size);
                } else {
                    let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    self.write_mem(addr, result, op_size)?;
                }
                self.regs.rip += cursor as u64;
            }

            // Shift/Rotate Group 2: r/m8, 1 (0xD0)
            0xD0 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xD0: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let val = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1) as u8
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)?
                };

                let result = self.execute_shift8(op, val, 1)?;

                if modrm >> 6 == 3 {
                    self.set_reg(rm, result as u64, 1);
                } else {
                    let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    self.mmu.write_u8(addr, result, &self.sregs)?;
                }
                self.regs.rip += cursor as u64;
            }

            // Shift/Rotate Group 2: r/m, 1 (0xD1)
            0xD1 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xD1: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

                let val = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };

                let result = self.execute_shift(op, val, 1, op_size)?;

                if modrm >> 6 == 3 {
                    self.set_reg(rm, result, op_size);
                } else {
                    let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    self.write_mem(addr, result, op_size)?;
                }
                self.regs.rip += cursor as u64;
            }

            // Shift/Rotate Group 2: r/m8, CL (0xD2)
            0xD2 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xD2: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let count = (self.regs.rcx & 0x1F) as u8;

                let val = if modrm >> 6 == 3 {
                    self.get_reg(rm, 1) as u8
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    cursor += extra;
                    self.mmu.read_u8(addr, &self.sregs)?
                };

                let result = self.execute_shift8(op, val, count)?;

                if modrm >> 6 == 3 {
                    self.set_reg(rm, result as u64, 1);
                } else {
                    let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    self.mmu.write_u8(addr, result, &self.sregs)?;
                }
                self.regs.rip += cursor as u64;
            }

            // Shift/Rotate Group 2: r/m, CL (0xD3)
            0xD3 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("0xD3: missing ModR/M".to_string()));
                }
                let modrm = bytes[cursor];
                let modrm_start = cursor;
                cursor += 1;
                let op = (modrm >> 3) & 0x07;
                let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
                let count = (self.regs.rcx & 0x3F) as u8;

                let val = if modrm >> 6 == 3 {
                    self.get_reg(rm, op_size)
                } else {
                    let (addr, extra) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    cursor += extra;
                    self.read_mem(addr, op_size)?
                };

                let result = self.execute_shift(op, val, count, op_size)?;

                if modrm >> 6 == 3 {
                    self.set_reg(rm, result, op_size);
                } else {
                    let (addr, _) = self.decode_modrm_addr(&bytes[modrm_start..], rex, modrm_start)?;
                    self.write_mem(addr, result, op_size)?;
                }
                self.regs.rip += cursor as u64;
            }

            // LEAVE (0xC9)
            0xC9 => {
                self.regs.rsp = self.regs.rbp;
                self.regs.rbp = self.pop64()?;
                self.regs.rip += cursor as u64;
            }

            // PUSHF (0x9C)
            0x9C => {
                self.push64(self.regs.rflags)?;
                self.regs.rip += cursor as u64;
            }

            // POPF (0x9D)
            0x9D => {
                let flags = self.pop64()?;
                // Preserve reserved bits
                self.regs.rflags = (flags & 0x00000000_00257FD5) | 0x2;
                self.regs.rip += cursor as u64;
            }

            // TEST AL, imm8 (0xA8)
            0xA8 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("TEST AL, imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;
                let result = (self.regs.rax & 0xFF) & imm;
                flags::update_flags_logic(&mut self.regs.rflags, result, 1);
                self.regs.rip += cursor as u64;
            }

            // TEST rAX, imm (0xA9)
            0xA9 => {
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;
                let result = self.get_reg(0, op_size) & imm;
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // CMP AL, imm8 (0x3C)
            0x3C => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("CMP AL, imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;
                let al = self.regs.rax & 0xFF;
                let result = al.wrapping_sub(imm);
                flags::update_flags_sub(&mut self.regs.rflags, al, imm, result, 1);
                self.regs.rip += cursor as u64;
            }

            // CMP rAX, imm (0x3D)
            0x3D => {
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;
                let rax = self.get_reg(0, op_size);
                let result = rax.wrapping_sub(imm);
                flags::update_flags_sub(&mut self.regs.rflags, rax, imm, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // ADD AL, imm8 (0x04)
            0x04 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("ADD AL, imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;
                let al = self.regs.rax & 0xFF;
                let result = al.wrapping_add(imm);
                self.regs.rax = (self.regs.rax & !0xFF) | (result & 0xFF);
                flags::update_flags_add(&mut self.regs.rflags, al, imm, result, 1);
                self.regs.rip += cursor as u64;
            }

            // ADD rAX, imm (0x05)
            0x05 => {
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;
                let rax = self.get_reg(0, op_size);
                let result = rax.wrapping_add(imm);
                self.set_reg(0, result, op_size);
                flags::update_flags_add(&mut self.regs.rflags, rax, imm, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // SUB AL, imm8 (0x2C)
            0x2C => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("SUB AL, imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;
                let al = self.regs.rax & 0xFF;
                let result = al.wrapping_sub(imm);
                self.regs.rax = (self.regs.rax & !0xFF) | (result & 0xFF);
                flags::update_flags_sub(&mut self.regs.rflags, al, imm, result, 1);
                self.regs.rip += cursor as u64;
            }

            // SUB rAX, imm (0x2D)
            0x2D => {
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;
                let rax = self.get_reg(0, op_size);
                let result = rax.wrapping_sub(imm);
                self.set_reg(0, result, op_size);
                flags::update_flags_sub(&mut self.regs.rflags, rax, imm, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // AND AL, imm8 (0x24)
            0x24 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("AND AL, imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;
                let result = (self.regs.rax & 0xFF) & imm;
                self.regs.rax = (self.regs.rax & !0xFF) | result;
                flags::update_flags_logic(&mut self.regs.rflags, result, 1);
                self.regs.rip += cursor as u64;
            }

            // AND rAX, imm (0x25)
            0x25 => {
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;
                let rax = self.get_reg(0, op_size);
                let result = rax & imm;
                self.set_reg(0, result, op_size);
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // OR AL, imm8 (0x0C)
            0x0C => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("OR AL, imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;
                let result = (self.regs.rax & 0xFF) | imm;
                self.regs.rax = (self.regs.rax & !0xFF) | result;
                flags::update_flags_logic(&mut self.regs.rflags, result, 1);
                self.regs.rip += cursor as u64;
            }

            // OR rAX, imm (0x0D)
            0x0D => {
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;
                let rax = self.get_reg(0, op_size);
                let result = rax | imm;
                self.set_reg(0, result, op_size);
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // XOR AL, imm8 (0x34)
            0x34 => {
                if cursor >= bytes.len() {
                    return Err(Error::Emulator("XOR AL, imm8: missing immediate".to_string()));
                }
                let imm = bytes[cursor] as u64;
                cursor += 1;
                let result = (self.regs.rax & 0xFF) ^ imm;
                self.regs.rax = (self.regs.rax & !0xFF) | result;
                flags::update_flags_logic(&mut self.regs.rflags, result, 1);
                self.regs.rip += cursor as u64;
            }

            // XOR rAX, imm (0x35)
            0x35 => {
                let imm_size = if op_size == 8 { 4 } else { op_size };
                let imm = self.read_immediate(&bytes[cursor..], imm_size)?;
                let imm = if op_size == 8 { imm as i32 as i64 as u64 } else { imm };
                cursor += imm_size as usize;
                let rax = self.get_reg(0, op_size);
                let result = rax ^ imm;
                self.set_reg(0, result, op_size);
                flags::update_flags_logic(&mut self.regs.rflags, result, op_size);
                self.regs.rip += cursor as u64;
            }

            // STOSB (0xAA) - with REP support
            0xAA => {
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    self.mmu.write_u8(self.regs.rdi, self.regs.rax as u8, &self.sregs)?;
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rdi = self.regs.rdi.wrapping_add(1);
                    } else {
                        self.regs.rdi = self.regs.rdi.wrapping_sub(1);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // STOSW/STOSD/STOSQ (0xAB) - with REP support
            0xAB => {
                let delta = op_size as u64;
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    self.write_mem(self.regs.rdi, self.regs.rax, op_size)?;
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rdi = self.regs.rdi.wrapping_add(delta);
                    } else {
                        self.regs.rdi = self.regs.rdi.wrapping_sub(delta);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // LODSB (0xAC) - with REP support
            0xAC => {
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val = self.mmu.read_u8(self.regs.rsi, &self.sregs)?;
                    self.regs.rax = (self.regs.rax & !0xFF) | (val as u64);
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rsi = self.regs.rsi.wrapping_add(1);
                    } else {
                        self.regs.rsi = self.regs.rsi.wrapping_sub(1);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // LODSW/LODSD/LODSQ (0xAD) - with REP support
            0xAD => {
                let delta = op_size as u64;
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val = self.read_mem(self.regs.rsi, op_size)?;
                    self.set_reg(0, val, op_size);
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rsi = self.regs.rsi.wrapping_add(delta);
                    } else {
                        self.regs.rsi = self.regs.rsi.wrapping_sub(delta);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // MOVSB (0xA4) - with REP support
            0xA4 => {
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val = self.mmu.read_u8(self.regs.rsi, &self.sregs)?;
                    self.mmu.write_u8(self.regs.rdi, val, &self.sregs)?;
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rsi = self.regs.rsi.wrapping_add(1);
                        self.regs.rdi = self.regs.rdi.wrapping_add(1);
                    } else {
                        self.regs.rsi = self.regs.rsi.wrapping_sub(1);
                        self.regs.rdi = self.regs.rdi.wrapping_sub(1);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // MOVSW/MOVSD/MOVSQ (0xA5) - with REP support
            0xA5 => {
                let delta = op_size as u64;
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val = self.read_mem(self.regs.rsi, op_size)?;
                    self.write_mem(self.regs.rdi, val, op_size)?;
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rsi = self.regs.rsi.wrapping_add(delta);
                        self.regs.rdi = self.regs.rdi.wrapping_add(delta);
                    } else {
                        self.regs.rsi = self.regs.rsi.wrapping_sub(delta);
                        self.regs.rdi = self.regs.rdi.wrapping_sub(delta);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // CMPSB (0xA6) - with REPE/REPNE support
            0xA6 => {
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val1 = self.mmu.read_u8(self.regs.rsi, &self.sregs)? as u64;
                    let val2 = self.mmu.read_u8(self.regs.rdi, &self.sregs)? as u64;
                    let result = val1.wrapping_sub(val2);
                    flags::update_flags_sub(&mut self.regs.rflags, val1, val2, result, 1);
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rsi = self.regs.rsi.wrapping_add(1);
                        self.regs.rdi = self.regs.rdi.wrapping_add(1);
                    } else {
                        self.regs.rsi = self.regs.rsi.wrapping_sub(1);
                        self.regs.rdi = self.regs.rdi.wrapping_sub(1);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                        let zf = (self.regs.rflags & flags::bits::ZF) != 0;
                        // REPE (0xF3): continue while equal (ZF=1)
                        // REPNE (0xF2): continue while not equal (ZF=0)
                        if rep_prefix == Some(0xF3) && !zf {
                            break;
                        }
                        if rep_prefix == Some(0xF2) && zf {
                            break;
                        }
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // CMPSW/CMPSD/CMPSQ (0xA7) - with REPE/REPNE support
            0xA7 => {
                let delta = op_size as u64;
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val1 = self.read_mem(self.regs.rsi, op_size)?;
                    let val2 = self.read_mem(self.regs.rdi, op_size)?;
                    let result = val1.wrapping_sub(val2);
                    flags::update_flags_sub(&mut self.regs.rflags, val1, val2, result, op_size);
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rsi = self.regs.rsi.wrapping_add(delta);
                        self.regs.rdi = self.regs.rdi.wrapping_add(delta);
                    } else {
                        self.regs.rsi = self.regs.rsi.wrapping_sub(delta);
                        self.regs.rdi = self.regs.rdi.wrapping_sub(delta);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                        let zf = (self.regs.rflags & flags::bits::ZF) != 0;
                        if rep_prefix == Some(0xF3) && !zf {
                            break;
                        }
                        if rep_prefix == Some(0xF2) && zf {
                            break;
                        }
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // SCASB (0xAE) - with REPE/REPNE support
            0xAE => {
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val = self.mmu.read_u8(self.regs.rdi, &self.sregs)? as u64;
                    let al = self.regs.rax & 0xFF;
                    let result = al.wrapping_sub(val);
                    flags::update_flags_sub(&mut self.regs.rflags, al, val, result, 1);
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rdi = self.regs.rdi.wrapping_add(1);
                    } else {
                        self.regs.rdi = self.regs.rdi.wrapping_sub(1);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                        let zf = (self.regs.rflags & flags::bits::ZF) != 0;
                        if rep_prefix == Some(0xF3) && !zf {
                            break;
                        }
                        if rep_prefix == Some(0xF2) && zf {
                            break;
                        }
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // SCASW/SCASD/SCASQ (0xAF) - with REPE/REPNE support
            0xAF => {
                let delta = op_size as u64;
                let count = if rep_prefix.is_some() { self.regs.rcx } else { 1 };
                for _ in 0..count {
                    if rep_prefix.is_some() && self.regs.rcx == 0 {
                        break;
                    }
                    let val = self.read_mem(self.regs.rdi, op_size)?;
                    let rax = self.get_reg(0, op_size);
                    let result = rax.wrapping_sub(val);
                    flags::update_flags_sub(&mut self.regs.rflags, rax, val, result, op_size);
                    if self.regs.rflags & flags::bits::DF == 0 {
                        self.regs.rdi = self.regs.rdi.wrapping_add(delta);
                    } else {
                        self.regs.rdi = self.regs.rdi.wrapping_sub(delta);
                    }
                    if rep_prefix.is_some() {
                        self.regs.rcx = self.regs.rcx.wrapping_sub(1);
                        let zf = (self.regs.rflags & flags::bits::ZF) != 0;
                        if rep_prefix == Some(0xF3) && !zf {
                            break;
                        }
                        if rep_prefix == Some(0xF2) && zf {
                            break;
                        }
                    }
                }
                self.regs.rip += cursor as u64;
            }

            // CBW/CWDE/CDQE (0x98)
            0x98 => {
                if rex_w {
                    // CDQE: sign-extend EAX to RAX
                    self.regs.rax = (self.regs.rax as i32) as i64 as u64;
                } else if operand_size_override {
                    // CBW: sign-extend AL to AX
                    let al = (self.regs.rax & 0xFF) as i8;
                    self.regs.rax = (self.regs.rax & !0xFFFF) | ((al as i16 as u16) as u64);
                } else {
                    // CWDE: sign-extend AX to EAX
                    let ax = (self.regs.rax & 0xFFFF) as i16;
                    self.regs.rax = (ax as i32) as u32 as u64;
                }
                self.regs.rip += cursor as u64;
            }

            // CWD/CDQ/CQO (0x99)
            0x99 => {
                if rex_w {
                    // CQO: sign-extend RAX to RDX:RAX
                    self.regs.rdx = if (self.regs.rax as i64) < 0 { !0u64 } else { 0 };
                } else if operand_size_override {
                    // CWD: sign-extend AX to DX:AX
                    let ax = (self.regs.rax & 0xFFFF) as i16;
                    self.regs.rdx = (self.regs.rdx & !0xFFFF) | if ax < 0 { 0xFFFF } else { 0 };
                } else {
                    // CDQ: sign-extend EAX to EDX:EAX
                    let eax = self.regs.rax as i32;
                    self.regs.rdx = if eax < 0 { 0xFFFF_FFFF } else { 0 };
                }
                self.regs.rip += cursor as u64;
            }

            // MOV moffs8, AL (0xA2)
            0xA2 => {
                let addr = self.read_immediate(&bytes[cursor..], 8)?;
                cursor += 8;
                self.mmu.write_u8(addr, self.regs.rax as u8, &self.sregs)?;
                self.regs.rip += cursor as u64;
            }

            // MOV moffs, rAX (0xA3)
            0xA3 => {
                let addr = self.read_immediate(&bytes[cursor..], 8)?;
                cursor += 8;
                self.write_mem(addr, self.regs.rax, op_size)?;
                self.regs.rip += cursor as u64;
            }

            // MOV AL, moffs8 (0xA0)
            0xA0 => {
                let addr = self.read_immediate(&bytes[cursor..], 8)?;
                cursor += 8;
                let val = self.mmu.read_u8(addr, &self.sregs)?;
                self.regs.rax = (self.regs.rax & !0xFF) | (val as u64);
                self.regs.rip += cursor as u64;
            }

            // MOV rAX, moffs (0xA1)
            0xA1 => {
                let addr = self.read_immediate(&bytes[cursor..], 8)?;
                cursor += 8;
                let val = self.read_mem(addr, op_size)?;
                self.set_reg(0, val, op_size);
                self.regs.rip += cursor as u64;
            }

            _ => {
                return Err(Error::Emulator(format!(
                    "unimplemented opcode: {:#04x} at RIP={:#x}",
                    opcode, self.regs.rip
                )));
            }
        }

        Ok(None)
    }

    /// Read an immediate value of the given size.
    fn read_immediate(&self, bytes: &[u8], size: u8) -> Result<u64> {
        if bytes.len() < size as usize {
            return Err(Error::Emulator("immediate value too short".to_string()));
        }
        Ok(match size {
            1 => bytes[0] as u64,
            2 => u16::from_le_bytes([bytes[0], bytes[1]]) as u64,
            4 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64,
            8 => u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
                bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            _ => return Err(Error::Emulator(format!("invalid immediate size: {}", size))),
        })
    }

    /// Get a general-purpose register value.
    fn get_reg(&self, reg: u8, size: u8) -> u64 {
        let full = match reg & 0x0F {
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
            1 => full & 0xFF,
            2 => full & 0xFFFF,
            4 => full & 0xFFFF_FFFF,
            8 => full,
            _ => full,
        }
    }

    /// Set a general-purpose register value.
    fn set_reg(&mut self, reg: u8, value: u64, size: u8) {
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

    /// Set an 8-bit register value.
    fn set_reg8(&mut self, reg: u8, value: u8) {
        // In 64-bit mode without REX, AH/BH/CH/DH are not accessible
        // With REX prefix, high byte registers are replaced with R8B-R15B
        self.set_reg(reg, value as u64, 1);
    }

    /// Push a 64-bit value onto the stack.
    fn push64(&mut self, value: u64) -> Result<()> {
        self.regs.rsp -= 8;
        self.mmu.write_u64(self.regs.rsp, value, &self.sregs)
    }

    /// Pop a 64-bit value from the stack.
    fn pop64(&mut self) -> Result<u64> {
        let value = self.mmu.read_u64(self.regs.rsp, &self.sregs)?;
        self.regs.rsp += 8;
        Ok(value)
    }

    /// Check a condition code.
    fn check_condition(&self, cc: u8) -> bool {
        let cf = (self.regs.rflags & flags::bits::CF) != 0;
        let zf = (self.regs.rflags & flags::bits::ZF) != 0;
        let sf = (self.regs.rflags & flags::bits::SF) != 0;
        let of = (self.regs.rflags & flags::bits::OF) != 0;
        let pf = (self.regs.rflags & flags::bits::PF) != 0;

        match cc {
            0x0 => of,                    // JO
            0x1 => !of,                   // JNO
            0x2 => cf,                    // JB/JC/JNAE
            0x3 => !cf,                   // JAE/JNB/JNC
            0x4 => zf,                    // JE/JZ
            0x5 => !zf,                   // JNE/JNZ
            0x6 => cf || zf,              // JBE/JNA
            0x7 => !cf && !zf,            // JA/JNBE
            0x8 => sf,                    // JS
            0x9 => !sf,                   // JNS
            0xA => pf,                    // JP/JPE
            0xB => !pf,                   // JNP/JPO
            0xC => sf != of,              // JL/JNGE
            0xD => sf == of,              // JGE/JNL
            0xE => zf || (sf != of),      // JLE/JNG
            0xF => !zf && (sf == of),     // JG/JNLE
            _ => false,
        }
    }

    /// Execute CPUID instruction.
    fn execute_cpuid(&mut self) {
        let leaf = self.regs.rax as u32;
        let subleaf = self.regs.rcx as u32;

        let (eax, ebx, ecx, edx) = match leaf {
            // Basic CPUID Information
            0 => {
                // Return max leaf and vendor string "RaxEmulato" (must be 12 chars)
                (0x01, 0x45786152, 0x6f74616c, 0x756d456d) // "RaxE" "lato" "muEm" -> "RaxEmulato"
            }
            1 => {
                // Processor signature and features
                // Basic features: FPU, PSE, PAE, APIC, MSR, CX8, CMOV, CLFLUSH
                let features_edx = 0x00800001 | (1 << 3) | (1 << 5) | (1 << 6) | (1 << 9) | (1 << 15) | (1 << 19);
                let features_ecx = 0; // No SSE3, etc for now
                (0x00000000, 0x00000000, features_ecx, features_edx)
            }
            0x80000000 => {
                // Extended CPUID Information - max extended leaf
                (0x80000001u32, 0, 0, 0)
            }
            0x80000001 => {
                // Extended features
                let features_edx = 1u32 << 29; // LM (Long Mode)
                (0, 0, 0, features_edx)
            }
            _ => (0, 0, 0, 0),
        };

        let _ = subleaf; // Would be used for subleaf-specific info

        self.regs.rax = eax as u64;
        self.regs.rbx = ebx as u64;
        self.regs.rcx = ecx as u64;
        self.regs.rdx = edx as u64;
    }

    /// Read a Model-Specific Register.
    fn read_msr(&self, msr: u32) -> Result<u64> {
        match msr {
            0xC0000080 => Ok(self.sregs.efer), // EFER
            0xC0000100 => Ok(self.sregs.fs.base), // FS.base
            0xC0000101 => Ok(self.sregs.gs.base), // GS.base
            0xC0000102 => Ok(0), // KernelGSbase
            _ => Ok(0), // Return 0 for unknown MSRs
        }
    }

    /// Write a Model-Specific Register.
    fn write_msr(&mut self, msr: u32, value: u64) -> Result<()> {
        match msr {
            0xC0000080 => self.sregs.efer = value, // EFER
            0xC0000100 => self.sregs.fs.base = value, // FS.base
            0xC0000101 => self.sregs.gs.base = value, // GS.base
            0xC0000102 => {} // KernelGSbase - ignore for now
            _ => {} // Ignore unknown MSRs
        }
        Ok(())
    }

    /// Execute 8-bit shift/rotate operation.
    fn execute_shift8(&mut self, op: u8, val: u8, count: u8) -> Result<u8> {
        if count == 0 {
            return Ok(val);
        }
        let count = count & 0x1F; // Mask to 5 bits for 8-bit ops
        let cf_bit = flags::bits::CF;
        let of_bit = flags::bits::OF;

        let (result, cf, of) = match op {
            0 => { // ROL
                let result = val.rotate_left(count as u32);
                let cf = (result & 1) != 0;
                let of = if count == 1 { ((result >> 7) ^ (result & 1)) != 0 } else { false };
                (result, cf, of)
            }
            1 => { // ROR
                let result = val.rotate_right(count as u32);
                let cf = (result >> 7) != 0;
                let of = if count == 1 { ((result >> 7) ^ ((result >> 6) & 1)) != 0 } else { false };
                (result, cf, of)
            }
            4 => { // SHL/SAL
                let result = if count >= 8 { 0 } else { val << count };
                let cf = if count > 0 && count <= 8 { (val >> (8 - count)) & 1 != 0 } else { false };
                let of = if count == 1 { ((result >> 7) ^ (cf as u8)) != 0 } else { false };
                (result, cf, of)
            }
            5 => { // SHR
                let result = if count >= 8 { 0 } else { val >> count };
                let cf = if count > 0 && count <= 8 { (val >> (count - 1)) & 1 != 0 } else { false };
                let of = if count == 1 { (val >> 7) != 0 } else { false };
                (result, cf, of)
            }
            7 => { // SAR
                let result = if count >= 8 {
                    if (val as i8) < 0 { 0xFF } else { 0 }
                } else {
                    ((val as i8) >> count) as u8
                };
                let cf = if count > 0 && count <= 8 { (val >> (count - 1)) & 1 != 0 } else { false };
                let of = false; // OF is always 0 for SAR with count=1
                (result, cf, of)
            }
            _ => return Err(Error::Emulator(format!("unimplemented shift8 op: {}", op))),
        };

        // Update flags
        if cf { self.regs.rflags |= cf_bit; } else { self.regs.rflags &= !cf_bit; }
        if of { self.regs.rflags |= of_bit; } else { self.regs.rflags &= !of_bit; }
        flags::update_flags_logic(&mut self.regs.rflags, result as u64, 1);

        Ok(result)
    }

    /// Execute shift/rotate operation for 16/32/64-bit operands.
    fn execute_shift(&mut self, op: u8, val: u64, count: u8, size: u8) -> Result<u64> {
        if count == 0 {
            return Ok(val);
        }
        let bits = size as u32 * 8;
        let mask = if bits == 64 { !0u64 } else { (1u64 << bits) - 1 };
        let cf_bit = flags::bits::CF;
        let of_bit = flags::bits::OF;

        let (result, cf, of) = match op {
            0 => { // ROL
                let count = count as u32 % bits;
                let result = if count == 0 { val & mask } else {
                    ((val << count) | (val >> (bits - count))) & mask
                };
                let cf = (result & 1) != 0;
                let of = if count == 1 { (((result >> (bits - 1)) ^ result) & 1) != 0 } else { false };
                (result, cf, of)
            }
            1 => { // ROR
                let count = count as u32 % bits;
                let result = if count == 0 { val & mask } else {
                    ((val >> count) | (val << (bits - count))) & mask
                };
                let cf = (result >> (bits - 1)) != 0;
                let of = if count == 1 { (((result >> (bits - 1)) ^ ((result >> (bits - 2)) & 1)) != 0) } else { false };
                (result, cf, of)
            }
            4 => { // SHL/SAL
                let result = if count as u32 >= bits { 0 } else { (val << count) & mask };
                let cf = if count > 0 && (count as u32) <= bits {
                    (val >> (bits - count as u32)) & 1 != 0
                } else { false };
                let of = if count == 1 { ((result >> (bits - 1)) ^ (cf as u64)) != 0 } else { false };
                (result, cf, of)
            }
            5 => { // SHR
                let result = if count as u32 >= bits { 0 } else { (val >> count) & mask };
                let cf = if count > 0 && (count as u32) <= bits {
                    (val >> (count - 1)) & 1 != 0
                } else { false };
                let of = if count == 1 { (val >> (bits - 1)) != 0 } else { false };
                (result, cf, of)
            }
            7 => { // SAR
                let result = if count as u32 >= bits {
                    let sign = (val >> (bits - 1)) & 1;
                    if sign != 0 { mask } else { 0 }
                } else {
                    match size {
                        2 => (((val & 0xFFFF) as i16 >> count) as u16) as u64,
                        4 => (((val & 0xFFFF_FFFF) as i32 >> count) as u32) as u64,
                        8 => ((val as i64) >> count) as u64,
                        _ => val >> count,
                    }
                };
                let cf = if count > 0 && (count as u32) <= bits {
                    (val >> (count - 1)) & 1 != 0
                } else { false };
                let of = false;
                (result, cf, of)
            }
            _ => return Err(Error::Emulator(format!("unimplemented shift op: {}", op))),
        };

        // Update flags
        if cf { self.regs.rflags |= cf_bit; } else { self.regs.rflags &= !cf_bit; }
        if of { self.regs.rflags |= of_bit; } else { self.regs.rflags &= !of_bit; }
        flags::update_flags_logic(&mut self.regs.rflags, result, size);

        Ok(result)
    }

    /// Get a segment register value by index.
    fn get_sreg(&self, sreg: u8) -> u16 {
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

    /// Set a segment register value by index.
    fn set_sreg(&mut self, sreg: u8, value: u16) {
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

    /// Read memory with the given size (1, 2, 4, or 8 bytes).
    fn read_mem(&self, addr: u64, size: u8) -> Result<u64> {
        match size {
            1 => Ok(self.mmu.read_u8(addr, &self.sregs)? as u64),
            2 => Ok(self.mmu.read_u16(addr, &self.sregs)? as u64),
            4 => Ok(self.mmu.read_u32(addr, &self.sregs)? as u64),
            8 => self.mmu.read_u64(addr, &self.sregs),
            _ => Err(Error::Emulator(format!("invalid memory read size: {}", size))),
        }
    }

    /// Write memory with the given size (1, 2, 4, or 8 bytes).
    fn write_mem(&self, addr: u64, value: u64, size: u8) -> Result<()> {
        match size {
            1 => self.mmu.write_u8(addr, value as u8, &self.sregs),
            2 => self.mmu.write_u16(addr, value as u16, &self.sregs),
            4 => self.mmu.write_u32(addr, value as u32, &self.sregs),
            8 => self.mmu.write_u64(addr, value, &self.sregs),
            _ => Err(Error::Emulator(format!("invalid memory write size: {}", size))),
        }
    }

    /// Decode ModR/M byte to get effective address.
    /// Returns (address, bytes_consumed_after_modrm).
    fn decode_modrm_addr(&self, bytes: &[u8], rex: Option<u8>, cursor_before: usize) -> Result<(u64, usize)> {
        if bytes.is_empty() {
            return Err(Error::Emulator("ModR/M: no bytes".to_string()));
        }
        let modrm = bytes[0];
        let mod_bits = modrm >> 6;
        let rm = (modrm & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));
        let mut extra = 0;

        // mod == 3 means register direct, shouldn't call this function
        if mod_bits == 3 {
            return Err(Error::Emulator("ModR/M: mod=3 is register, not memory".to_string()));
        }

        let mut addr: u64;

        if rm == 4 {
            // SIB byte follows
            if bytes.len() < 2 {
                return Err(Error::Emulator("ModR/M: missing SIB byte".to_string()));
            }
            let sib = bytes[1];
            extra += 1;
            let scale = 1u64 << (sib >> 6);
            let index = ((sib >> 3) & 0x07) | (rex.map_or(0, |r| (r & 0x02) << 2));
            let base_reg = (sib & 0x07) | (rex.map_or(0, |r| (r & 0x01) << 3));

            // Calculate base
            addr = if base_reg == 5 && mod_bits == 0 {
                // No base, disp32 follows
                0
            } else {
                self.get_reg(base_reg, 8)
            };

            // Add scaled index (index=4 means no index)
            if index != 4 {
                addr = addr.wrapping_add(self.get_reg(index, 8).wrapping_mul(scale));
            }

            // Handle displacement for base=5, mod=0 case
            if base_reg == 5 && mod_bits == 0 {
                if bytes.len() < 2 + 4 {
                    return Err(Error::Emulator("ModR/M: missing disp32 for SIB".to_string()));
                }
                let disp = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]) as i64;
                extra += 4;
                addr = (addr as i64).wrapping_add(disp) as u64;
            }
        } else if rm == 5 && mod_bits == 0 {
            // RIP-relative addressing (64-bit mode)
            // disp32 follows, relative to RIP after this instruction
            if bytes.len() < 5 {
                return Err(Error::Emulator("ModR/M: missing disp32 for RIP-relative".to_string()));
            }
            let disp = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as i64;
            extra += 4;
            // RIP points to the next instruction: current RIP + cursor_before + modrm(1) + disp32(4)
            let rip_after = self.regs.rip as i64 + cursor_before as i64 + 1 + 4;
            addr = rip_after.wrapping_add(disp) as u64;
        } else {
            // Regular register indirect
            addr = self.get_reg(rm, 8);
        }

        // Handle displacement
        match mod_bits {
            0 => {} // No displacement (except special cases handled above)
            1 => {
                // disp8
                if bytes.len() < extra + 2 {
                    return Err(Error::Emulator("ModR/M: missing disp8".to_string()));
                }
                let disp = bytes[extra + 1] as i8 as i64;
                extra += 1;
                addr = (addr as i64).wrapping_add(disp) as u64;
            }
            2 => {
                // disp32
                if bytes.len() < extra + 5 {
                    return Err(Error::Emulator("ModR/M: missing disp32".to_string()));
                }
                let disp = i32::from_le_bytes([
                    bytes[extra + 1],
                    bytes[extra + 2],
                    bytes[extra + 3],
                    bytes[extra + 4],
                ]) as i64;
                extra += 4;
                addr = (addr as i64).wrapping_add(disp) as u64;
            }
            _ => {}
        }

        Ok((addr, extra))
    }
}

impl VCpu for EmulatorVcpu {
    fn run(&mut self) -> Result<VcpuExit> {
        // If we were halted, check for interrupts
        if self.halted {
            // For now, just return Hlt - interrupt injection would wake us
            return Ok(VcpuExit::Hlt);
        }

        // Execute instructions until we hit an exit condition
        loop {
            match self.step()? {
                Some(exit) => return Ok(exit),
                None => continue,
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
            // Store result in RAX based on size
            match pending.size {
                1 => self.regs.rax = (self.regs.rax & !0xFF) | (data.get(0).copied().unwrap_or(0) as u64),
                2 => {
                    let val = if data.len() >= 2 {
                        u16::from_le_bytes([data[0], data[1]])
                    } else {
                        0
                    };
                    self.regs.rax = (self.regs.rax & !0xFFFF) | (val as u64);
                }
                4 => {
                    let val = if data.len() >= 4 {
                        u32::from_le_bytes([data[0], data[1], data[2], data[3]])
                    } else {
                        0
                    };
                    self.regs.rax = val as u64; // Zero-extends
                }
                _ => {}
            }
        }
        self.halted = false;
    }

    fn id(&self) -> u32 {
        self.id
    }
}
