//! VEX integer instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::super::cpu::{InsnContext, X86_64Vcpu};

impl X86_64Vcpu {
    pub(in crate::backend::emulator::x86_64) fn execute_vex_permilps_reg(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src = vvvv as usize;

        let (ctrl_lo, ctrl_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src_lo = self.regs.xmm[xmm_src][0];
        let src_hi = self.regs.xmm[xmm_src][1];

        let (dst_lo, dst_hi) = self.permilps_128(src_lo, src_hi, ctrl_lo, ctrl_hi);
        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (ctrl_hi2, ctrl_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src_hi2 = self.regs.ymm_high[xmm_src][0];
            let src_hi3 = self.regs.ymm_high[xmm_src][1];
            let (dst_hi2, dst_hi3) = self.permilps_128(src_hi2, src_hi3, ctrl_hi2, ctrl_hi3);
            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    fn permilps_128(&self, src_lo: u64, src_hi: u64, ctrl_lo: u64, ctrl_hi: u64) -> (u64, u64) {
        let floats = [
            src_lo as u32,
            (src_lo >> 32) as u32,
            src_hi as u32,
            (src_hi >> 32) as u32,
        ];
        let r0 = floats[(ctrl_lo & 3) as usize];
        let r1 = floats[((ctrl_lo >> 32) & 3) as usize];
        let r2 = floats[(ctrl_hi & 3) as usize];
        let r3 = floats[((ctrl_hi >> 32) & 3) as usize];
        ((r0 as u64) | ((r1 as u64) << 32), (r2 as u64) | ((r3 as u64) << 32))
    }

    pub(in crate::backend::emulator::x86_64) fn execute_vex_permilpd_reg(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src = vvvv as usize;

        let (ctrl_lo, ctrl_hi) = if is_memory {
            (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
        } else {
            (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
        };
        let src_lo = self.regs.xmm[xmm_src][0];
        let src_hi = self.regs.xmm[xmm_src][1];

        // Select based on bit 1 of each control qword
        let dst_lo = if (ctrl_lo >> 1) & 1 == 0 { src_lo } else { src_hi };
        let dst_hi = if (ctrl_hi >> 1) & 1 == 0 { src_lo } else { src_hi };
        self.regs.xmm[xmm_dst][0] = dst_lo;
        self.regs.xmm[xmm_dst][1] = dst_hi;

        if vex_l == 1 {
            let (ctrl_hi2, ctrl_hi3) = if is_memory {
                (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
            } else {
                (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
            };
            let src_hi2 = self.regs.ymm_high[xmm_src][0];
            let src_hi3 = self.regs.ymm_high[xmm_src][1];
            let dst_hi2 = if (ctrl_hi2 >> 1) & 1 == 0 { src_hi2 } else { src_hi3 };
            let dst_hi3 = if (ctrl_hi3 >> 1) & 1 == 0 { src_hi2 } else { src_hi3 };
            self.regs.ymm_high[xmm_dst][0] = dst_hi2;
            self.regs.ymm_high[xmm_dst][1] = dst_hi3;
        } else {
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

}
