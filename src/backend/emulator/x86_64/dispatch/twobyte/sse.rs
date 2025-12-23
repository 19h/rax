//! Two-byte opcode instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::aes;
use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::flags;
use super::super::super::insn;

impl X86_64Vcpu {
    pub(in crate::backend::emulator::x86_64) fn execute_sse_add(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            // ADDSS - scalar single
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let dst = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            let result = dst + src;
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | result.to_bits() as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            // ADDSD - scalar double
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            let dst = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            self.regs.xmm[xmm_dst][0] = (dst + src).to_bits();
        } else if ctx.operand_size_override {
            // ADDPD - packed double (2 x f64)
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let d0 = f64::from_bits(self.regs.xmm[xmm_dst][0]) + f64::from_bits(src_lo);
            let d1 = f64::from_bits(self.regs.xmm[xmm_dst][1]) + f64::from_bits(src_hi);
            self.regs.xmm[xmm_dst][0] = d0.to_bits();
            self.regs.xmm[xmm_dst][1] = d1.to_bits();
        } else {
            // ADDPS - packed single (4 x f32)
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let dst_lo = self.regs.xmm[xmm_dst][0];
            let dst_hi = self.regs.xmm[xmm_dst][1];
            let r0 = f32::from_bits(dst_lo as u32) + f32::from_bits(src_lo as u32);
            let r1 = f32::from_bits((dst_lo >> 32) as u32) + f32::from_bits((src_lo >> 32) as u32);
            let r2 = f32::from_bits(dst_hi as u32) + f32::from_bits(src_hi as u32);
            let r3 = f32::from_bits((dst_hi >> 32) as u32) + f32::from_bits((src_hi >> 32) as u32);
            self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE packed single/double subtract (0x5C)
    pub(in crate::backend::emulator::x86_64) fn execute_sse_sub(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let dst = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | (dst - src).to_bits() as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            let dst = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            self.regs.xmm[xmm_dst][0] = (dst - src).to_bits();
        } else if ctx.operand_size_override {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            self.regs.xmm[xmm_dst][0] =
                (f64::from_bits(self.regs.xmm[xmm_dst][0]) - f64::from_bits(src_lo)).to_bits();
            self.regs.xmm[xmm_dst][1] =
                (f64::from_bits(self.regs.xmm[xmm_dst][1]) - f64::from_bits(src_hi)).to_bits();
        } else {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let (dst_lo, dst_hi) = (self.regs.xmm[xmm_dst][0], self.regs.xmm[xmm_dst][1]);
            let r0 = f32::from_bits(dst_lo as u32) - f32::from_bits(src_lo as u32);
            let r1 = f32::from_bits((dst_lo >> 32) as u32) - f32::from_bits((src_lo >> 32) as u32);
            let r2 = f32::from_bits(dst_hi as u32) - f32::from_bits(src_hi as u32);
            let r3 = f32::from_bits((dst_hi >> 32) as u32) - f32::from_bits((src_hi >> 32) as u32);
            self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE packed single/double multiply (0x59)
    pub(in crate::backend::emulator::x86_64) fn execute_sse_mul(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let dst = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | (dst * src).to_bits() as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            let dst = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            self.regs.xmm[xmm_dst][0] = (dst * src).to_bits();
        } else if ctx.operand_size_override {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            self.regs.xmm[xmm_dst][0] =
                (f64::from_bits(self.regs.xmm[xmm_dst][0]) * f64::from_bits(src_lo)).to_bits();
            self.regs.xmm[xmm_dst][1] =
                (f64::from_bits(self.regs.xmm[xmm_dst][1]) * f64::from_bits(src_hi)).to_bits();
        } else {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let (dst_lo, dst_hi) = (self.regs.xmm[xmm_dst][0], self.regs.xmm[xmm_dst][1]);
            let r0 = f32::from_bits(dst_lo as u32) * f32::from_bits(src_lo as u32);
            let r1 = f32::from_bits((dst_lo >> 32) as u32) * f32::from_bits((src_lo >> 32) as u32);
            let r2 = f32::from_bits(dst_hi as u32) * f32::from_bits(src_hi as u32);
            let r3 = f32::from_bits((dst_hi >> 32) as u32) * f32::from_bits((src_hi >> 32) as u32);
            self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE packed single/double divide (0x5E)
    pub(in crate::backend::emulator::x86_64) fn execute_sse_div(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let dst = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | (dst / src).to_bits() as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            let dst = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            self.regs.xmm[xmm_dst][0] = (dst / src).to_bits();
        } else if ctx.operand_size_override {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            self.regs.xmm[xmm_dst][0] =
                (f64::from_bits(self.regs.xmm[xmm_dst][0]) / f64::from_bits(src_lo)).to_bits();
            self.regs.xmm[xmm_dst][1] =
                (f64::from_bits(self.regs.xmm[xmm_dst][1]) / f64::from_bits(src_hi)).to_bits();
        } else {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let (dst_lo, dst_hi) = (self.regs.xmm[xmm_dst][0], self.regs.xmm[xmm_dst][1]);
            let r0 = f32::from_bits(dst_lo as u32) / f32::from_bits(src_lo as u32);
            let r1 = f32::from_bits((dst_lo >> 32) as u32) / f32::from_bits((src_lo >> 32) as u32);
            let r2 = f32::from_bits(dst_hi as u32) / f32::from_bits(src_hi as u32);
            let r3 = f32::from_bits((dst_hi >> 32) as u32) / f32::from_bits((src_hi >> 32) as u32);
            self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE packed single/double sqrt (0x51)
    pub(in crate::backend::emulator::x86_64) fn execute_sse_sqrt(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | src.sqrt().to_bits() as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            self.regs.xmm[xmm_dst][0] = src.sqrt().to_bits();
        } else if ctx.operand_size_override {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            self.regs.xmm[xmm_dst][0] = f64::from_bits(src_lo).sqrt().to_bits();
            self.regs.xmm[xmm_dst][1] = f64::from_bits(src_hi).sqrt().to_bits();
        } else {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let r0 = f32::from_bits(src_lo as u32).sqrt();
            let r1 = f32::from_bits((src_lo >> 32) as u32).sqrt();
            let r2 = f32::from_bits(src_hi as u32).sqrt();
            let r3 = f32::from_bits((src_hi >> 32) as u32).sqrt();
            self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE packed single/double min (0x5D)
    pub(in crate::backend::emulator::x86_64) fn execute_sse_min(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let dst = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | dst.min(src).to_bits() as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            let dst = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            self.regs.xmm[xmm_dst][0] = dst.min(src).to_bits();
        } else if ctx.operand_size_override {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            self.regs.xmm[xmm_dst][0] =
                f64::from_bits(self.regs.xmm[xmm_dst][0]).min(f64::from_bits(src_lo)).to_bits();
            self.regs.xmm[xmm_dst][1] =
                f64::from_bits(self.regs.xmm[xmm_dst][1]).min(f64::from_bits(src_hi)).to_bits();
        } else {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let (dst_lo, dst_hi) = (self.regs.xmm[xmm_dst][0], self.regs.xmm[xmm_dst][1]);
            let r0 = f32::from_bits(dst_lo as u32).min(f32::from_bits(src_lo as u32));
            let r1 = f32::from_bits((dst_lo >> 32) as u32).min(f32::from_bits((src_lo >> 32) as u32));
            let r2 = f32::from_bits(dst_hi as u32).min(f32::from_bits(src_hi as u32));
            let r3 = f32::from_bits((dst_hi >> 32) as u32).min(f32::from_bits((src_hi >> 32) as u32));
            self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE packed single/double max (0x5F)
    pub(in crate::backend::emulator::x86_64) fn execute_sse_max(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let dst = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | dst.max(src).to_bits() as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            let dst = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            self.regs.xmm[xmm_dst][0] = dst.max(src).to_bits();
        } else if ctx.operand_size_override {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            self.regs.xmm[xmm_dst][0] =
                f64::from_bits(self.regs.xmm[xmm_dst][0]).max(f64::from_bits(src_lo)).to_bits();
            self.regs.xmm[xmm_dst][1] =
                f64::from_bits(self.regs.xmm[xmm_dst][1]).max(f64::from_bits(src_hi)).to_bits();
        } else {
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let (dst_lo, dst_hi) = (self.regs.xmm[xmm_dst][0], self.regs.xmm[xmm_dst][1]);
            let r0 = f32::from_bits(dst_lo as u32).max(f32::from_bits(src_lo as u32));
            let r1 = f32::from_bits((dst_lo >> 32) as u32).max(f32::from_bits((src_lo >> 32) as u32));
            let r2 = f32::from_bits(dst_hi as u32).max(f32::from_bits(src_hi as u32));
            let r3 = f32::from_bits((dst_hi >> 32) as u32).max(f32::from_bits((src_hi >> 32) as u32));
            self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE UNPCKLPS/UNPCKLPD (0x14) - unpack and interleave low
    pub(in crate::backend::emulator::x86_64) fn execute_sse_unpcklps(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let (src_lo, _src_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let dst_lo = self.regs.xmm[xmm_dst][0];

        if ctx.operand_size_override {
            // UNPCKLPD - interleave low doubles
            // dst[63:0] = dst[63:0], dst[127:64] = src[63:0]
            self.regs.xmm[xmm_dst][1] = src_lo;
        } else {
            // UNPCKLPS - interleave low singles
            // dst[31:0] = dst[31:0], dst[63:32] = src[31:0]
            // dst[95:64] = dst[63:32], dst[127:96] = src[63:32]
            let d0 = dst_lo as u32;
            let d1 = (dst_lo >> 32) as u32;
            let s0 = src_lo as u32;
            let s1 = (src_lo >> 32) as u32;
            self.regs.xmm[xmm_dst][0] = d0 as u64 | ((s0 as u64) << 32);
            self.regs.xmm[xmm_dst][1] = d1 as u64 | ((s1 as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE UNPCKHPS/UNPCKHPD (0x15) - unpack and interleave high
    pub(in crate::backend::emulator::x86_64) fn execute_sse_unpckhps(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let (_src_lo, src_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let dst_hi = self.regs.xmm[xmm_dst][1];

        if ctx.operand_size_override {
            // UNPCKHPD - interleave high doubles
            // dst[63:0] = dst[127:64], dst[127:64] = src[127:64]
            self.regs.xmm[xmm_dst][0] = dst_hi;
            self.regs.xmm[xmm_dst][1] = src_hi;
        } else {
            // UNPCKHPS - interleave high singles
            // dst[31:0] = dst[95:64], dst[63:32] = src[95:64]
            // dst[95:64] = dst[127:96], dst[127:96] = src[127:96]
            let d2 = dst_hi as u32;
            let d3 = (dst_hi >> 32) as u32;
            let s2 = src_hi as u32;
            let s3 = (src_hi >> 32) as u32;
            self.regs.xmm[xmm_dst][0] = d2 as u64 | ((s2 as u64) << 32);
            self.regs.xmm[xmm_dst][1] = d3 as u64 | ((s3 as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE PSHUFD/PSHUFHW/PSHUFLW (0x0F 0x70)
    pub(in crate::backend::emulator::x86_64) fn execute_pshufd(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            // PSHUFHW: shuffle high words, preserve low qword
            // F3 0F 70 /r ib
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            // Low qword unchanged
            self.regs.xmm[xmm_dst][0] = src_lo;
            // High qword: shuffle the 4 words in src_hi
            let w0 = (src_hi >> (((imm8 >> 0) & 3) * 16)) as u16;
            let w1 = (src_hi >> (((imm8 >> 2) & 3) * 16)) as u16;
            let w2 = (src_hi >> (((imm8 >> 4) & 3) * 16)) as u16;
            let w3 = (src_hi >> (((imm8 >> 6) & 3) * 16)) as u16;
            self.regs.xmm[xmm_dst][1] =
                (w0 as u64) | ((w1 as u64) << 16) | ((w2 as u64) << 32) | ((w3 as u64) << 48);
        } else if ctx.rep_prefix == Some(0xF2) {
            // PSHUFLW: shuffle low words, preserve high qword
            // F2 0F 70 /r ib
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            // Low qword: shuffle the 4 words in src_lo
            let w0 = (src_lo >> (((imm8 >> 0) & 3) * 16)) as u16;
            let w1 = (src_lo >> (((imm8 >> 2) & 3) * 16)) as u16;
            let w2 = (src_lo >> (((imm8 >> 4) & 3) * 16)) as u16;
            let w3 = (src_lo >> (((imm8 >> 6) & 3) * 16)) as u16;
            self.regs.xmm[xmm_dst][0] =
                (w0 as u64) | ((w1 as u64) << 16) | ((w2 as u64) << 32) | ((w3 as u64) << 48);
            // High qword unchanged
            self.regs.xmm[xmm_dst][1] = src_hi;
        } else if ctx.operand_size_override {
            // PSHUFD: shuffle all 4 dwords
            // 66 0F 70 /r ib
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            // Combine into 4 dwords for easier indexing
            let dwords: [u32; 4] = [
                src_lo as u32,
                (src_lo >> 32) as u32,
                src_hi as u32,
                (src_hi >> 32) as u32,
            ];
            let d0 = dwords[((imm8 >> 0) & 3) as usize];
            let d1 = dwords[((imm8 >> 2) & 3) as usize];
            let d2 = dwords[((imm8 >> 4) & 3) as usize];
            let d3 = dwords[((imm8 >> 6) & 3) as usize];
            self.regs.xmm[xmm_dst][0] = (d0 as u64) | ((d1 as u64) << 32);
            self.regs.xmm[xmm_dst][1] = (d2 as u64) | ((d3 as u64) << 32);
        } else {
            // PSHUFW: shuffle MMX words (NP 0F 70)
            // mm1, mm2/m64, imm8 - shuffle 4 words in 64-bit MMX register
            let mm_dst = reg as usize & 0x7;
            let mm_src = rm as usize & 0x7;
            let src = if is_memory {
                self.read_mem(addr, 8)?
            } else {
                self.regs.mm[mm_src]
            };
            // Shuffle 4 16-bit words based on imm8
            let w0 = (src >> (((imm8 >> 0) & 3) * 16)) as u16;
            let w1 = (src >> (((imm8 >> 2) & 3) * 16)) as u16;
            let w2 = (src >> (((imm8 >> 4) & 3) * 16)) as u16;
            let w3 = (src >> (((imm8 >> 6) & 3) * 16)) as u16;
            self.regs.mm[mm_dst] =
                (w0 as u64) | ((w1 as u64) << 16) | ((w2 as u64) << 32) | ((w3 as u64) << 48);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE CMPPS/CMPPD/CMPSS/CMPSD (0x0F 0xC2)
    pub(in crate::backend::emulator::x86_64) fn execute_cmpps(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;

        if ctx.rep_prefix == Some(0xF3) {
            // CMPSS - scalar single
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let dst = f32::from_bits(self.regs.xmm[xmm_dst][0] as u32);
            let result = if self.cmp_predicate_f32(dst, src, imm8) {
                0xFFFFFFFFu32
            } else {
                0u32
            };
            self.regs.xmm[xmm_dst][0] =
                (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | result as u64;
        } else if ctx.rep_prefix == Some(0xF2) {
            // CMPSD - scalar double
            let src = if is_memory {
                f64::from_bits(self.read_mem(addr, 8)?)
            } else {
                f64::from_bits(self.regs.xmm[rm as usize][0])
            };
            let dst = f64::from_bits(self.regs.xmm[xmm_dst][0]);
            let result = if self.cmp_predicate_f64(dst, src, imm8) {
                !0u64
            } else {
                0u64
            };
            self.regs.xmm[xmm_dst][0] = result;
        } else if ctx.operand_size_override {
            // CMPPD - packed double
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let r0 = if self.cmp_predicate_f64(
                f64::from_bits(self.regs.xmm[xmm_dst][0]),
                f64::from_bits(src_lo),
                imm8,
            ) {
                !0u64
            } else {
                0u64
            };
            let r1 = if self.cmp_predicate_f64(
                f64::from_bits(self.regs.xmm[xmm_dst][1]),
                f64::from_bits(src_hi),
                imm8,
            ) {
                !0u64
            } else {
                0u64
            };
            self.regs.xmm[xmm_dst][0] = r0;
            self.regs.xmm[xmm_dst][1] = r1;
        } else {
            // CMPPS - packed single
            let (src_lo, src_hi) = if is_memory {
                (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
            } else {
                (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
            };
            let dst_lo = self.regs.xmm[xmm_dst][0];
            let dst_hi = self.regs.xmm[xmm_dst][1];
            let r0 = if self.cmp_predicate_f32(
                f32::from_bits(dst_lo as u32),
                f32::from_bits(src_lo as u32),
                imm8,
            ) {
                0xFFFFFFFFu32
            } else {
                0
            };
            let r1 = if self.cmp_predicate_f32(
                f32::from_bits((dst_lo >> 32) as u32),
                f32::from_bits((src_lo >> 32) as u32),
                imm8,
            ) {
                0xFFFFFFFFu32
            } else {
                0
            };
            let r2 = if self.cmp_predicate_f32(
                f32::from_bits(dst_hi as u32),
                f32::from_bits(src_hi as u32),
                imm8,
            ) {
                0xFFFFFFFFu32
            } else {
                0
            };
            let r3 = if self.cmp_predicate_f32(
                f32::from_bits((dst_hi >> 32) as u32),
                f32::from_bits((src_hi >> 32) as u32),
                imm8,
            ) {
                0xFFFFFFFFu32
            } else {
                0
            };
            self.regs.xmm[xmm_dst][0] = r0 as u64 | ((r1 as u64) << 32);
            self.regs.xmm[xmm_dst][1] = r2 as u64 | ((r3 as u64) << 32);
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// Helper for float comparison predicates (bits 2:0 for SSE, 4:0 for AVX)
    pub(in crate::backend::emulator::x86_64) fn cmp_predicate_f32(&self, a: f32, b: f32, pred: u8) -> bool {
        match pred & 0x1F {
            0x00 => a == b,                   // EQ_OQ
            0x01 => a < b,                    // LT_OS
            0x02 => a <= b,                   // LE_OS
            0x03 => a.is_nan() || b.is_nan(), // UNORD_Q
            0x04 => a != b || a.is_nan() || b.is_nan(), // NEQ_UQ
            0x05 => !(a < b),                 // NLT_US
            0x06 => !(a <= b),                // NLE_US
            0x07 => !a.is_nan() && !b.is_nan(), // ORD_Q
            0x08 => a == b || a.is_nan() || b.is_nan(), // EQ_UQ
            0x09 => !(a >= b),                // NGE_US
            0x0A => !(a > b),                 // NGT_US
            0x0B => false,                    // FALSE_OQ
            0x0C => a != b,                   // NEQ_OQ
            0x0D => a >= b,                   // GE_OS
            0x0E => a > b,                    // GT_OS
            0x0F => true,                     // TRUE_UQ
            0x10 => a == b,                   // EQ_OS
            0x11 => a < b || a.is_nan() || b.is_nan(), // LT_OQ
            0x12 => a <= b || a.is_nan() || b.is_nan(), // LE_OQ
            0x13 => a.is_nan() || b.is_nan(), // UNORD_S
            0x14 => a != b,                   // NEQ_US
            0x15 => !(a < b) || a.is_nan() || b.is_nan(), // NLT_UQ
            0x16 => !(a <= b) || a.is_nan() || b.is_nan(), // NLE_UQ
            0x17 => !a.is_nan() && !b.is_nan(), // ORD_S
            0x18 => a == b,                   // EQ_US
            0x19 => !(a >= b) || a.is_nan() || b.is_nan(), // NGE_UQ
            0x1A => !(a > b) || a.is_nan() || b.is_nan(), // NGT_UQ
            0x1B => false,                    // FALSE_OS
            0x1C => a != b || a.is_nan() || b.is_nan(), // NEQ_OS
            0x1D => a >= b || a.is_nan() || b.is_nan(), // GE_OQ
            0x1E => a > b || a.is_nan() || b.is_nan(), // GT_OQ
            0x1F => true,                     // TRUE_US
            _ => false,
        }
    }

    pub(in crate::backend::emulator::x86_64) fn cmp_predicate_f64(&self, a: f64, b: f64, pred: u8) -> bool {
        match pred & 0x1F {
            0x00 => a == b,
            0x01 => a < b,
            0x02 => a <= b,
            0x03 => a.is_nan() || b.is_nan(),
            0x04 => a != b || a.is_nan() || b.is_nan(),
            0x05 => !(a < b),
            0x06 => !(a <= b),
            0x07 => !a.is_nan() && !b.is_nan(),
            0x08 => a == b || a.is_nan() || b.is_nan(),
            0x09 => !(a >= b),
            0x0A => !(a > b),
            0x0B => false,
            0x0C => a != b,
            0x0D => a >= b,
            0x0E => a > b,
            0x0F => true,
            0x10 => a == b,
            0x11 => a < b || a.is_nan() || b.is_nan(),
            0x12 => a <= b || a.is_nan() || b.is_nan(),
            0x13 => a.is_nan() || b.is_nan(),
            0x14 => a != b,
            0x15 => !(a < b) || a.is_nan() || b.is_nan(),
            0x16 => !(a <= b) || a.is_nan() || b.is_nan(),
            0x17 => !a.is_nan() && !b.is_nan(),
            0x18 => a == b,
            0x19 => !(a >= b) || a.is_nan() || b.is_nan(),
            0x1A => !(a > b) || a.is_nan() || b.is_nan(),
            0x1B => false,
            0x1C => a != b || a.is_nan() || b.is_nan(),
            0x1D => a >= b || a.is_nan() || b.is_nan(),
            0x1E => a > b || a.is_nan() || b.is_nan(),
            0x1F => true,
            _ => false,
        }
    }
}
