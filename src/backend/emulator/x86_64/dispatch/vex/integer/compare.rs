//! VEX integer instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::super::cpu::{InsnContext, X86_64Vcpu};

impl X86_64Vcpu {
    pub(in crate::backend::emulator::x86_64) fn execute_vex_pcmpeq(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };

        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        match opcode {
            // PCMPEQB
            0x74 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_eq_bytes(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_eq_bytes(src1_hi, src2_hi);
            }
            // PCMPEQW
            0x75 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_eq_words(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_eq_words(src1_hi, src2_hi);
            }
            // PCMPEQD
            0x76 => {
                self.regs.xmm[xmm_dst][0] = self.cmp_eq_dwords(src1_lo, src2_lo);
                self.regs.xmm[xmm_dst][1] = self.cmp_eq_dwords(src1_hi, src2_hi);
            }
            _ => unreachable!(),
        }

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];

            match opcode {
                0x74 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_eq_bytes(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_eq_bytes(src1_hi3, src2_hi3);
                }
                0x75 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_eq_words(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_eq_words(src1_hi3, src2_hi3);
                }
                0x76 => {
                    self.regs.ymm_high[xmm_dst][0] = self.cmp_eq_dwords(src1_hi2, src2_hi2);
                    self.regs.ymm_high[xmm_dst][1] = self.cmp_eq_dwords(src1_hi3, src2_hi3);
                }
                _ => {}
            }
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    // Helper: compare bytes for equality
    fn cmp_eq_bytes(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let va = (a >> (i * 8)) & 0xFF;
            let vb = (b >> (i * 8)) & 0xFF;
            if va == vb {
                result |= 0xFF << (i * 8);
            }
        }
        result
    }

    // Helper: compare words for equality
    fn cmp_eq_words(&self, a: u64, b: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..4 {
            let va = (a >> (i * 16)) & 0xFFFF;
            let vb = (b >> (i * 16)) & 0xFFFF;
            if va == vb {
                result |= 0xFFFF << (i * 16);
            }
        }
        result
    }

    // Helper: compare dwords for equality
    fn cmp_eq_dwords(&self, a: u64, b: u64) -> u64 {
        let lo_a = a as u32;
        let lo_b = b as u32;
        let hi_a = (a >> 32) as u32;
        let hi_b = (b >> 32) as u32;
        let lo_res = if lo_a == lo_b { 0xFFFFFFFFu64 } else { 0 };
        let hi_res = if hi_a == hi_b { 0xFFFFFFFFu64 } else { 0 };
        lo_res | (hi_res << 32)
    }

    pub(in crate::backend::emulator::x86_64) fn execute_vex_pcmpeqq(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        self.regs.xmm[xmm_dst][0] = if src1_lo == src2_lo { !0u64 } else { 0 };
        self.regs.xmm[xmm_dst][1] = if src1_hi == src2_hi { !0u64 } else { 0 };

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = if src1_hi2 == src2_hi2 { !0u64 } else { 0 };
            self.regs.ymm_high[xmm_dst][1] = if src1_hi3 == src2_hi3 { !0u64 } else { 0 };
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::backend::emulator::x86_64) fn execute_vex_pcmpgtq(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];

        self.regs.xmm[xmm_dst][0] = if (src1_lo as i64) > (src2_lo as i64) { !0u64 } else { 0 };
        self.regs.xmm[xmm_dst][1] = if (src1_hi as i64) > (src2_hi as i64) { !0u64 } else { 0 };

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = if (src1_hi2 as i64) > (src2_hi2 as i64) { !0u64 } else { 0 };
            self.regs.ymm_high[xmm_dst][1] = if (src1_hi3 as i64) > (src2_hi3 as i64) { !0u64 } else { 0 };
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

}
