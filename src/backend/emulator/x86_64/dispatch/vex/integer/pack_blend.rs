//! VEX integer instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::super::cpu::{InsnContext, X86_64Vcpu};

impl X86_64Vcpu {
    pub(in crate::backend::emulator::x86_64) fn execute_vex_pblendvb(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm = ctx.consume_u8()?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;
        let xmm_mask = ((imm >> 4) & 0xF) as usize;

        let (src2_lo, src2_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src1_lo = self.regs.xmm[xmm_src1][0];
        let src1_hi = self.regs.xmm[xmm_src1][1];
        let mask_lo = self.regs.xmm[xmm_mask][0];
        let mask_hi = self.regs.xmm[xmm_mask][1];

        self.regs.xmm[xmm_dst][0] = self.blend_bytes(src1_lo, src2_lo, mask_lo);
        self.regs.xmm[xmm_dst][1] = self.blend_bytes(src1_hi, src2_hi, mask_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            let mask_hi2 = self.regs.ymm_high[xmm_mask][0];
            let mask_hi3 = self.regs.ymm_high[xmm_mask][1];
            self.regs.ymm_high[xmm_dst][0] = self.blend_bytes(src1_hi2, src2_hi2, mask_hi2);
            self.regs.ymm_high[xmm_dst][1] = self.blend_bytes(src1_hi3, src2_hi3, mask_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn blend_bytes(&self, a: u64, b: u64, mask: u64) -> u64 {
        let mut result = 0u64;
        for i in 0..8 {
            let m = (mask >> (i * 8 + 7)) & 1;
            let v = if m != 0 {
                (b >> (i * 8)) & 0xFF
            } else {
                (a >> (i * 8)) & 0xFF
            };
            result |= v << (i * 8);
        }
        result
    }

    pub(in crate::backend::emulator::x86_64) fn execute_vex_packusdw(
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

        self.regs.xmm[xmm_dst][0] = self.pack_usdw(src1_lo, src1_hi);
        self.regs.xmm[xmm_dst][1] = self.pack_usdw(src2_lo, src2_hi);

        if vex_l == 1 {
            let (src2_hi2, src2_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
            let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = self.pack_usdw(src1_hi2, src1_hi3);
            self.regs.ymm_high[xmm_dst][1] = self.pack_usdw(src2_hi2, src2_hi3);
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn pack_usdw(&self, lo: u64, hi: u64) -> u64 {
        let saturate = |v: i32| -> u16 {
            if v < 0 { 0u16 }
            else if v > 65535 { 0xFFFFu16 }
            else { v as u16 }
        };
        let w0 = saturate(lo as i32) as u64;
        let w1 = saturate((lo >> 32) as i32) as u64;
        let w2 = saturate(hi as i32) as u64;
        let w3 = saturate((hi >> 32) as i32) as u64;
        w0 | (w1 << 16) | (w2 << 32) | (w3 << 48)
    }

}
