//! VEX instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::insn;

impl X86_64Vcpu {
    pub(in crate::backend::emulator::x86_64) fn execute_vex_movmskp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, _, _) = self.decode_modrm(ctx)?;
        if is_memory {
            return Err(Error::Emulator("VMOVMSK* requires XMM/YMM source".to_string()));
        }

        let xmm_src = rm as usize;
        let mut result = 0u64;

        if vex_pp == 0 {
            // VMOVMSKPS: extract sign bits of singles
            let lo = self.regs.xmm[xmm_src][0];
            let hi = self.regs.xmm[xmm_src][1];

            result |= ((lo >> 31) & 1) as u64;
            result |= ((lo >> 63) & 1) << 1;
            result |= ((hi >> 31) & 1) << 2;
            result |= ((hi >> 63) & 1) << 3;

            if vex_l == 1 {
                let hi2 = self.regs.ymm_high[xmm_src][0];
                let hi3 = self.regs.ymm_high[xmm_src][1];

                result |= ((hi2 >> 31) & 1) << 4;
                result |= ((hi2 >> 63) & 1) << 5;
                result |= ((hi3 >> 31) & 1) << 6;
                result |= ((hi3 >> 63) & 1) << 7;
            }
        } else {
            // VMOVMSKPD: extract sign bits of doubles
            let lo = self.regs.xmm[xmm_src][0];
            let hi = self.regs.xmm[xmm_src][1];

            result |= ((lo >> 63) & 1) as u64;
            result |= ((hi >> 63) & 1) << 1;

            if vex_l == 1 {
                let hi2 = self.regs.ymm_high[xmm_src][0];
                let hi3 = self.regs.ymm_high[xmm_src][1];

                result |= ((hi2 >> 63) & 1) << 2;
                result |= ((hi3 >> 63) & 1) << 3;
            }
        }

        self.set_reg(reg, result, 4);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX reciprocal square root: VRSQRTPS, VRSQRTSS
    pub(in crate::backend::emulator::x86_64) fn execute_vex_rsqrt(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if vex_pp == 2 {
            // VRSQRTSS: scalar single
            let xmm_src1 = vvvv as usize;
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let result = (1.0f32 / src.sqrt()).to_bits() as u64;
            self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        } else {
            // VRSQRTPS: packed singles
            let rsqrt = |v: u64| -> u64 {
                let f0 = f32::from_bits(v as u32);
                let f1 = f32::from_bits((v >> 32) as u32);
                let r0 = (1.0f32 / f0.sqrt()).to_bits() as u64;
                let r1 = (1.0f32 / f1.sqrt()).to_bits() as u64;
                r0 | (r1 << 32)
            };

            if vex_l == 0 {
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] = rsqrt(src_lo);
                self.regs.xmm[xmm_dst][1] = rsqrt(src_hi);
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            } else {
                let (src0, src1, src2, src3) = if is_memory {
                    (
                        self.read_mem(addr, 8)?,
                        self.read_mem(addr + 8, 8)?,
                        self.read_mem(addr + 16, 8)?,
                        self.read_mem(addr + 24, 8)?,
                    )
                } else {
                    (
                        self.regs.xmm[rm as usize][0],
                        self.regs.xmm[rm as usize][1],
                        self.regs.ymm_high[rm as usize][0],
                        self.regs.ymm_high[rm as usize][1],
                    )
                };
                self.regs.xmm[xmm_dst][0] = rsqrt(src0);
                self.regs.xmm[xmm_dst][1] = rsqrt(src1);
                self.regs.ymm_high[xmm_dst][0] = rsqrt(src2);
                self.regs.ymm_high[xmm_dst][1] = rsqrt(src3);
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX reciprocal: VRCPPS, VRCPSS
    pub(in crate::backend::emulator::x86_64) fn execute_vex_rcp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        if vex_pp == 2 {
            // VRCPSS: scalar single
            let xmm_src1 = vvvv as usize;
            let src = if is_memory {
                f32::from_bits(self.read_mem(addr, 4)? as u32)
            } else {
                f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
            };
            let result = (1.0f32 / src).to_bits() as u64;
            self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
            self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            self.regs.ymm_high[xmm_dst][0] = 0;
            self.regs.ymm_high[xmm_dst][1] = 0;
        } else {
            // VRCPPS: packed singles
            let rcp = |v: u64| -> u64 {
                let f0 = f32::from_bits(v as u32);
                let f1 = f32::from_bits((v >> 32) as u32);
                let r0 = (1.0f32 / f0).to_bits() as u64;
                let r1 = (1.0f32 / f1).to_bits() as u64;
                r0 | (r1 << 32)
            };

            if vex_l == 0 {
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] = rcp(src_lo);
                self.regs.xmm[xmm_dst][1] = rcp(src_hi);
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            } else {
                let (src0, src1, src2, src3) = if is_memory {
                    (
                        self.read_mem(addr, 8)?,
                        self.read_mem(addr + 8, 8)?,
                        self.read_mem(addr + 16, 8)?,
                        self.read_mem(addr + 24, 8)?,
                    )
                } else {
                    (
                        self.regs.xmm[rm as usize][0],
                        self.regs.xmm[rm as usize][1],
                        self.regs.ymm_high[rm as usize][0],
                        self.regs.ymm_high[rm as usize][1],
                    )
                };
                self.regs.xmm[xmm_dst][0] = rcp(src0);
                self.regs.xmm[xmm_dst][1] = rcp(src1);
                self.regs.ymm_high[xmm_dst][0] = rcp(src2);
                self.regs.ymm_high[xmm_dst][1] = rcp(src3);
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX zero: VZEROUPPER, VZEROALL
    pub(in crate::backend::emulator::x86_64) fn execute_vex_vzero(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        if vex_l == 0 {
            // VZEROUPPER: zero upper 128 bits of all YMM registers
            for i in 0..16 {
                self.regs.ymm_high[i][0] = 0;
                self.regs.ymm_high[i][1] = 0;
            }
        } else {
            // VZEROALL: zero all YMM registers
            for i in 0..16 {
                self.regs.xmm[i][0] = 0;
                self.regs.xmm[i][1] = 0;
                self.regs.ymm_high[i][0] = 0;
                self.regs.ymm_high[i][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX MXCSR: VLDMXCSR, VSTMXCSR
    pub(in crate::backend::emulator::x86_64) fn execute_vex_ldst_mxcsr(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.peek_u8()?;
        let reg_op = (modrm >> 3) & 0x07;
        let (_, _, is_memory, addr, _) = self.decode_modrm(ctx)?;

        if !is_memory {
            return Err(Error::Emulator("VLDMXCSR/VSTMXCSR require memory operand".to_string()));
        }

        match reg_op {
            2 => {
                // VLDMXCSR: load MXCSR from memory
                // Treat as NOP - we don't emulate MXCSR rounding/exception behavior
                let _ = self.read_mem(addr, 4)?;
            }
            3 => {
                // VSTMXCSR: store MXCSR to memory
                // Return default MXCSR value (0x1F80)
                self.write_mem(addr, 0x1F80u64, 4)?;
            }
            _ => {
                return Err(Error::Emulator(format!("invalid VEX 0xAE /{}", reg_op)));
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX add-subtract: VADDSUBPS, VADDSUBPD
    pub(in crate::backend::emulator::x86_64) fn execute_vex_addsubp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
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

        if vex_pp == 3 {
            // VADDSUBPS: alternating sub/add for singles
            // dst[0] = src1[0] - src2[0], dst[1] = src1[1] + src2[1], etc.
            let addsub_ps = |a: u64, b: u64| -> u64 {
                let a0 = f32::from_bits(a as u32);
                let a1 = f32::from_bits((a >> 32) as u32);
                let b0 = f32::from_bits(b as u32);
                let b1 = f32::from_bits((b >> 32) as u32);
                let r0 = (a0 - b0).to_bits() as u64;
                let r1 = (a1 + b1).to_bits() as u64;
                r0 | (r1 << 32)
            };

            self.regs.xmm[xmm_dst][0] = addsub_ps(src1_lo, src2_lo);
            self.regs.xmm[xmm_dst][1] = addsub_ps(src1_hi, src2_hi);

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = addsub_ps(src1_hi2, src2_hi2);
                self.regs.ymm_high[xmm_dst][1] = addsub_ps(src1_hi3, src2_hi3);
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // VADDSUBPD: alternating sub/add for doubles
            // dst[0] = src1[0] - src2[0], dst[1] = src1[1] + src2[1]
            let a0 = f64::from_bits(src1_lo);
            let a1 = f64::from_bits(src1_hi);
            let b0 = f64::from_bits(src2_lo);
            let b1 = f64::from_bits(src2_hi);
            self.regs.xmm[xmm_dst][0] = (a0 - b0).to_bits();
            self.regs.xmm[xmm_dst][1] = (a1 + b1).to_bits();

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                let a2 = f64::from_bits(src1_hi2);
                let a3 = f64::from_bits(src1_hi3);
                let b2 = f64::from_bits(src2_hi2);
                let b3 = f64::from_bits(src2_hi3);
                self.regs.ymm_high[xmm_dst][0] = (a2 - b2).to_bits();
                self.regs.ymm_high[xmm_dst][1] = (a3 + b3).to_bits();
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX horizontal add: VHADDPS, VHADDPD
    pub(in crate::backend::emulator::x86_64) fn execute_vex_haddp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
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

        if vex_pp == 3 {
            // VHADDPS: horizontal add for singles
            // dst[0] = src1[0] + src1[1], dst[1] = src1[2] + src1[3]
            // dst[2] = src2[0] + src2[1], dst[3] = src2[2] + src2[3]
            let hadd_ps = |lo: u64, hi: u64| -> u64 {
                let s0 = f32::from_bits(lo as u32);
                let s1 = f32::from_bits((lo >> 32) as u32);
                let s2 = f32::from_bits(hi as u32);
                let s3 = f32::from_bits((hi >> 32) as u32);
                let r0 = (s0 + s1).to_bits() as u64;
                let r1 = (s2 + s3).to_bits() as u64;
                r0 | (r1 << 32)
            };

            self.regs.xmm[xmm_dst][0] = hadd_ps(src1_lo, src1_hi);
            self.regs.xmm[xmm_dst][1] = hadd_ps(src2_lo, src2_hi);

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = hadd_ps(src1_hi2, src1_hi3);
                self.regs.ymm_high[xmm_dst][1] = hadd_ps(src2_hi2, src2_hi3);
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // VHADDPD: horizontal add for doubles
            // dst[0] = src1[0] + src1[1], dst[1] = src2[0] + src2[1]
            let a0 = f64::from_bits(src1_lo);
            let a1 = f64::from_bits(src1_hi);
            let b0 = f64::from_bits(src2_lo);
            let b1 = f64::from_bits(src2_hi);
            self.regs.xmm[xmm_dst][0] = (a0 + a1).to_bits();
            self.regs.xmm[xmm_dst][1] = (b0 + b1).to_bits();

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                let a2 = f64::from_bits(src1_hi2);
                let a3 = f64::from_bits(src1_hi3);
                let b2 = f64::from_bits(src2_hi2);
                let b3 = f64::from_bits(src2_hi3);
                self.regs.ymm_high[xmm_dst][0] = (a2 + a3).to_bits();
                self.regs.ymm_high[xmm_dst][1] = (b2 + b3).to_bits();
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX horizontal subtract: VHSUBPS, VHSUBPD
    pub(in crate::backend::emulator::x86_64) fn execute_vex_hsubp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
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

        if vex_pp == 3 {
            // VHSUBPS: horizontal subtract for singles
            let hsub_ps = |lo: u64, hi: u64| -> u64 {
                let s0 = f32::from_bits(lo as u32);
                let s1 = f32::from_bits((lo >> 32) as u32);
                let s2 = f32::from_bits(hi as u32);
                let s3 = f32::from_bits((hi >> 32) as u32);
                let r0 = (s0 - s1).to_bits() as u64;
                let r1 = (s2 - s3).to_bits() as u64;
                r0 | (r1 << 32)
            };

            self.regs.xmm[xmm_dst][0] = hsub_ps(src1_lo, src1_hi);
            self.regs.xmm[xmm_dst][1] = hsub_ps(src2_lo, src2_hi);

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = hsub_ps(src1_hi2, src1_hi3);
                self.regs.ymm_high[xmm_dst][1] = hsub_ps(src2_hi2, src2_hi3);
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        } else {
            // VHSUBPD: horizontal subtract for doubles
            let a0 = f64::from_bits(src1_lo);
            let a1 = f64::from_bits(src1_hi);
            let b0 = f64::from_bits(src2_lo);
            let b1 = f64::from_bits(src2_hi);
            self.regs.xmm[xmm_dst][0] = (a0 - a1).to_bits();
            self.regs.xmm[xmm_dst][1] = (b0 - b1).to_bits();

            if vex_l == 1 {
                let (src2_hi2, src2_hi3) = if is_memory {
                    (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                } else {
                    (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                };
                let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                let a2 = f64::from_bits(src1_hi2);
                let a3 = f64::from_bits(src1_hi3);
                let b2 = f64::from_bits(src2_hi2);
                let b3 = f64::from_bits(src2_hi3);
                self.regs.ymm_high[xmm_dst][0] = (a2 - a3).to_bits();
                self.regs.ymm_high[xmm_dst][1] = (b2 - b3).to_bits();
            } else {
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX-encoded shuffle: VPSHUFD/VPSHUFHW/VPSHUFLW
    pub(in crate::backend::emulator::x86_64) fn execute_vex_cmp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm8 = ctx.consume_u8()?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        match vex_pp {
            0 => {
                // VCMPPS - packed single
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                let r0 = if self.cmp_predicate_f32(f32::from_bits(src1_lo as u32), f32::from_bits(src2_lo as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                let r1 = if self.cmp_predicate_f32(f32::from_bits((src1_lo >> 32) as u32), f32::from_bits((src2_lo >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                let r2 = if self.cmp_predicate_f32(f32::from_bits(src1_hi as u32), f32::from_bits(src2_hi as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                let r3 = if self.cmp_predicate_f32(f32::from_bits((src1_hi >> 32) as u32), f32::from_bits((src2_hi >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                self.regs.xmm[xmm_dst][0] = r0 as u64 | ((r1 as u64) << 32);
                self.regs.xmm[xmm_dst][1] = r2 as u64 | ((r3 as u64) << 32);

                if vex_l == 1 {
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r4 = if self.cmp_predicate_f32(f32::from_bits(src1_hi2 as u32), f32::from_bits(src2_hi2 as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    let r5 = if self.cmp_predicate_f32(f32::from_bits((src1_hi2 >> 32) as u32), f32::from_bits((src2_hi2 >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    let r6 = if self.cmp_predicate_f32(f32::from_bits(src1_hi3 as u32), f32::from_bits(src2_hi3 as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    let r7 = if self.cmp_predicate_f32(f32::from_bits((src1_hi3 >> 32) as u32), f32::from_bits((src2_hi3 >> 32) as u32), imm8) { 0xFFFFFFFFu32 } else { 0 };
                    self.regs.ymm_high[xmm_dst][0] = r4 as u64 | ((r5 as u64) << 32);
                    self.regs.ymm_high[xmm_dst][1] = r6 as u64 | ((r7 as u64) << 32);
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            1 => {
                // VCMPPD - packed double
                let (src2_lo, src2_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                let src1_lo = self.regs.xmm[xmm_src1][0];
                let src1_hi = self.regs.xmm[xmm_src1][1];

                let r0 = if self.cmp_predicate_f64(f64::from_bits(src1_lo), f64::from_bits(src2_lo), imm8) { !0u64 } else { 0 };
                let r1 = if self.cmp_predicate_f64(f64::from_bits(src1_hi), f64::from_bits(src2_hi), imm8) { !0u64 } else { 0 };
                self.regs.xmm[xmm_dst][0] = r0;
                self.regs.xmm[xmm_dst][1] = r1;

                if vex_l == 1 {
                    let (src2_hi2, src2_hi3) = if is_memory {
                        (self.read_mem(addr + 16, 8)?, self.read_mem(addr + 24, 8)?)
                    } else {
                        (self.regs.ymm_high[rm as usize][0], self.regs.ymm_high[rm as usize][1])
                    };
                    let src1_hi2 = self.regs.ymm_high[xmm_src1][0];
                    let src1_hi3 = self.regs.ymm_high[xmm_src1][1];
                    let r2 = if self.cmp_predicate_f64(f64::from_bits(src1_hi2), f64::from_bits(src2_hi2), imm8) { !0u64 } else { 0 };
                    let r3 = if self.cmp_predicate_f64(f64::from_bits(src1_hi3), f64::from_bits(src2_hi3), imm8) { !0u64 } else { 0 };
                    self.regs.ymm_high[xmm_dst][0] = r2;
                    self.regs.ymm_high[xmm_dst][1] = r3;
                } else {
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VCMPSS - scalar single
                let src2 = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                let src1 = f32::from_bits(self.regs.xmm[xmm_src1][0] as u32);
                let result = if self.cmp_predicate_f32(src1, src2, imm8) { 0xFFFFFFFFu32 } else { 0 };
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result as u64;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            3 => {
                // VCMPSD - scalar double
                let src2 = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                let src1 = f64::from_bits(self.regs.xmm[xmm_src1][0]);
                let result = if self.cmp_predicate_f64(src1, src2, imm8) { !0u64 } else { 0 };
                self.regs.xmm[xmm_dst][0] = result;
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
