//! VEX instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::insn;

impl X86_64Vcpu {
    pub(in crate::backend::emulator::x86_64) fn execute_vex_arith(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Determine operation type based on opcode
        let op: fn(f32, f32) -> f32 = match opcode {
            0x58 => |a, b| a + b,  // ADD
            0x59 => |a, b| a * b,  // MUL
            0x5C => |a, b| a - b,  // SUB
            0x5D => |a, b| a.min(b), // MIN
            0x5E => |a, b| a / b,  // DIV
            0x5F => |a, b| a.max(b), // MAX
            0x51 => |_a, b| b.sqrt(), // SQRT (unary, uses only src2)
            _ => unreachable!(),
        };
        let op_d: fn(f64, f64) -> f64 = match opcode {
            0x58 => |a, b| a + b,
            0x59 => |a, b| a * b,
            0x5C => |a, b| a - b,
            0x5D => |a, b| a.min(b),
            0x5E => |a, b| a / b,
            0x5F => |a, b| a.max(b),
            0x51 => |_a, b| b.sqrt(),
            _ => unreachable!(),
        };

        match vex_pp {
            0 => {
                // VEX.0F (NP) - packed single (PS)
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                // Process 4 floats in low 128 bits
                let r0 = op(f32::from_bits(src1_lo as u32), f32::from_bits(src2_lo as u32));
                let r1 = op(f32::from_bits((src1_lo >> 32) as u32), f32::from_bits((src2_lo >> 32) as u32));
                let r2 = op(f32::from_bits(src1_hi as u32), f32::from_bits(src2_hi as u32));
                let r3 = op(f32::from_bits((src1_hi >> 32) as u32), f32::from_bits((src2_hi >> 32) as u32));
                self.regs.xmm[xmm_dst][0] = r0.to_bits() as u64 | ((r1.to_bits() as u64) << 32);
                self.regs.xmm[xmm_dst][1] = r2.to_bits() as u64 | ((r3.to_bits() as u64) << 32);

                if vex_l == 1 {
                    // YMM - process high 128 bits too
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r4 = op(f32::from_bits(src1_hi2 as u32), f32::from_bits(src2_hi2 as u32));
                    let r5 = op(f32::from_bits((src1_hi2 >> 32) as u32), f32::from_bits((src2_hi2 >> 32) as u32));
                    let r6 = op(f32::from_bits(src1_hi3 as u32), f32::from_bits(src2_hi3 as u32));
                    let r7 = op(f32::from_bits((src1_hi3 >> 32) as u32), f32::from_bits((src2_hi3 >> 32) as u32));
                    self.regs.ymm_high[xmm_dst][0] = r4.to_bits() as u64 | ((r5.to_bits() as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = r6.to_bits() as u64 | ((r7.to_bits() as u64) << 32);
                } else {
                    // VEX.128 clears upper bits
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            1 => {
                // VEX.66.0F - packed double (PD)
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                let r0 = op_d(f64::from_bits(src1_lo), f64::from_bits(src2_lo));
                let r1 = op_d(f64::from_bits(src1_hi), f64::from_bits(src2_hi));
                self.regs.xmm[xmm_dst][0] = r0.to_bits();
                self.regs.xmm[xmm_dst][1] = r1.to_bits();

                if vex_l == 1 {
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r2 = op_d(f64::from_bits(src1_hi2), f64::from_bits(src2_hi2));
                    let r3 = op_d(f64::from_bits(src1_hi3), f64::from_bits(src2_hi3));
                    self.regs.ymm_high[xmm_dst][0] = r2.to_bits();
                    self.regs.ymm_high[xmm_dst][1] = r3.to_bits();
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VEX.F3.0F - scalar single (SS)
                let src2 = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                let src1 = f32::from_bits(self.regs.xmm[xmm_src1][0] as u32);
                let result = op(src1, src2);
                // Copy src1 to dst, then overwrite low 32 bits
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result.to_bits() as u64;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            3 => {
                // VEX.F2.0F - scalar double (SD)
                let src2 = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                let src1 = f64::from_bits(self.regs.xmm[xmm_src1][0]);
                let result = op_d(src1, src2);
                self.regs.xmm[xmm_dst][0] = result.to_bits();
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            _ => unreachable!(),
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
