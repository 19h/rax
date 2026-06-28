//! VEX instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::Result;

use super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::insn;

impl X86_64Vcpu {
    /// VEX.256/128.66.0F38.W0 13 /r: VCVTPH2PS.
    pub(in crate::backend::emulator::x86_64) fn execute_vex_vcvtph2ps(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vvvv != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let lanes = if vex_l == 0 { 4 } else { 8 };
        let mut out = [0u32; 8];

        for lane in 0..lanes {
            let half = if is_memory {
                self.read_mem(addr + (lane as u64 * 2), 2)? as u16
            } else {
                read_half_from_xmm(self.regs.xmm[rm as usize], lane)
            };
            out[lane] = f16_to_f32_bits(half);
        }

        self.regs.xmm[xmm_dst][0] = out[0] as u64 | ((out[1] as u64) << 32);
        self.regs.xmm[xmm_dst][1] = out[2] as u64 | ((out[3] as u64) << 32);
        if vex_l == 0 {
            self.regs.ymm_high[xmm_dst] = [0; 2];
        } else {
            self.regs.ymm_high[xmm_dst][0] = out[4] as u64 | ((out[5] as u64) << 32);
            self.regs.ymm_high[xmm_dst][1] = out[6] as u64 | ((out[7] as u64) << 32);
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX.256/128.66.0F3A.W0 1D /r ib: VCVTPS2PH.
    pub(in crate::backend::emulator::x86_64) fn execute_vex_vcvtps2ph(
        &mut self,
        ctx: &mut InsnContext,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        if vvvv != 0 {
            return self.inject_undefined_instruction();
        }

        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let imm = ctx.consume_u8()?;
        let rounding = if (imm & 0x04) != 0 {
            ((self.mxcsr >> 13) & 0x03) as u8
        } else {
            imm & 0x03
        };
        let xmm_src = reg as usize;
        let lanes = if vex_l == 0 { 4 } else { 8 };
        let mut halves = [0u16; 8];

        for lane in 0..lanes {
            let bits = if lane < 4 {
                let qword = self.regs.xmm[xmm_src][lane / 2];
                if lane & 1 == 0 {
                    qword as u32
                } else {
                    (qword >> 32) as u32
                }
            } else {
                let qword = self.regs.ymm_high[xmm_src][(lane - 4) / 2];
                if lane & 1 == 0 {
                    qword as u32
                } else {
                    (qword >> 32) as u32
                }
            };
            halves[lane] = f32_to_f16_bits(f32::from_bits(bits), rounding);
        }

        if is_memory {
            for (lane, half) in halves.iter().copied().enumerate().take(lanes) {
                self.write_mem(addr + (lane as u64 * 2), half as u64, 2)?;
            }
        } else {
            let xmm_dst = rm as usize;
            self.regs.xmm[xmm_dst][0] = halves[0] as u64
                | ((halves[1] as u64) << 16)
                | ((halves[2] as u64) << 32)
                | ((halves[3] as u64) << 48);
            self.regs.xmm[xmm_dst][1] = if vex_l == 0 {
                0
            } else {
                halves[4] as u64
                    | ((halves[5] as u64) << 16)
                    | ((halves[6] as u64) << 32)
                    | ((halves[7] as u64) << 48)
            };
            self.regs.ymm_high[xmm_dst] = [0; 2];
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    pub(in crate::backend::emulator::x86_64) fn execute_vex_cvt_fp(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        match vex_pp {
            0 => {
                // VCVTPS2PD: convert packed single to packed double
                // XMM: 2 singles -> 2 doubles, YMM: 4 singles -> 4 doubles
                if vex_l == 0 {
                    // 128-bit: read 64 bits (2 singles), produce 128 bits (2 doubles)
                    let src = if is_memory {
                        self.read_mem(addr, 8)?
                    } else {
                        self.regs.xmm[rm as usize][0]
                    };
                    let s0 = f32::from_bits(src as u32);
                    let s1 = f32::from_bits((src >> 32) as u32);
                    self.regs.xmm[xmm_dst][0] = (s0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (s1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: read 128 bits (4 singles), produce 256 bits (4 doubles)
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let s0 = f32::from_bits(src_lo as u32);
                    let s1 = f32::from_bits((src_lo >> 32) as u32);
                    let s2 = f32::from_bits(src_hi as u32);
                    let s3 = f32::from_bits((src_hi >> 32) as u32);
                    self.regs.xmm[xmm_dst][0] = (s0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (s1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = (s2 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][1] = (s3 as f64).to_bits();
                }
            }
            1 => {
                // VCVTPD2PS: convert packed double to packed single
                // XMM: 2 doubles -> 2 singles (in low 64 bits), YMM: 4 doubles -> 4 singles
                if vex_l == 0 {
                    // 128-bit: read 128 bits (2 doubles), produce 64 bits (2 singles)
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let d0 = f64::from_bits(src_lo);
                    let d1 = f64::from_bits(src_hi);
                    let s0 = (d0 as f32).to_bits() as u64;
                    let s1 = (d1 as f32).to_bits() as u64;
                    self.regs.xmm[xmm_dst][0] = s0 | (s1 << 32);
                    self.regs.xmm[xmm_dst][1] = 0;
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: read 256 bits (4 doubles), produce 128 bits (4 singles)
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
                    let d0 = f64::from_bits(src0);
                    let d1 = f64::from_bits(src1);
                    let d2 = f64::from_bits(src2);
                    let d3 = f64::from_bits(src3);
                    let s0 = (d0 as f32).to_bits() as u64;
                    let s1 = (d1 as f32).to_bits() as u64;
                    let s2 = (d2 as f32).to_bits() as u64;
                    let s3 = (d3 as f32).to_bits() as u64;
                    self.regs.xmm[xmm_dst][0] = s0 | (s1 << 32);
                    self.regs.xmm[xmm_dst][1] = s2 | (s3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VCVTSS2SD: convert scalar single to scalar double
                let xmm_src1 = vvvv as usize;
                let src = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                let result = (src as f64).to_bits();
                self.regs.xmm[xmm_dst][0] = result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            3 => {
                // VCVTSD2SS: convert scalar double to scalar single
                let xmm_src1 = vvvv as usize;
                let src = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                let result = (src as f32).to_bits() as u64;
                // Preserve upper bits from src1, write result to low 32 bits
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
                self.regs.ymm_high[xmm_dst][0] = 0;
                self.regs.ymm_high[xmm_dst][1] = 0;
            }
            _ => unreachable!(),
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX conversion 0x5B: VCVTDQ2PS, VCVTPS2DQ, VCVTTPS2DQ
    pub(in crate::backend::emulator::x86_64) fn execute_vex_cvt_dq_ps(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        // Helper to saturate f32 to i32
        fn f32_to_i32_saturate(f: f32) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f32 {
                i32::MIN // Intel returns 0x80000000 for overflow
            } else if f <= i32::MIN as f32 {
                i32::MIN
            } else {
                f as i32
            }
        }

        fn f32_to_i32_truncate(f: f32) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f32 {
                i32::MIN
            } else if f <= i32::MIN as f32 {
                i32::MIN
            } else {
                f.trunc() as i32
            }
        }

        match vex_pp {
            0 => {
                // VCVTDQ2PS: convert packed dword integers to packed singles
                if vex_l == 0 {
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let i0 = src_lo as i32;
                    let i1 = (src_lo >> 32) as i32;
                    let i2 = src_hi as i32;
                    let i3 = (src_hi >> 32) as i32;
                    let f0 = (i0 as f32).to_bits() as u64;
                    let f1 = (i1 as f32).to_bits() as u64;
                    let f2 = (i2 as f32).to_bits() as u64;
                    let f3 = (i3 as f32).to_bits() as u64;
                    self.regs.xmm[xmm_dst][0] = f0 | (f1 << 32);
                    self.regs.xmm[xmm_dst][1] = f2 | (f3 << 32);
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
                    let convert = |v: u64| -> u64 {
                        let i0 = v as i32;
                        let i1 = (v >> 32) as i32;
                        let f0 = (i0 as f32).to_bits() as u64;
                        let f1 = (i1 as f32).to_bits() as u64;
                        f0 | (f1 << 32)
                    };
                    self.regs.xmm[xmm_dst][0] = convert(src0);
                    self.regs.xmm[xmm_dst][1] = convert(src1);
                    self.regs.ymm_high[xmm_dst][0] = convert(src2);
                    self.regs.ymm_high[xmm_dst][1] = convert(src3);
                }
            }
            1 => {
                // VCVTPS2DQ: convert packed singles to packed dword integers (rounding)
                if vex_l == 0 {
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let f0 = f32::from_bits(src_lo as u32);
                    let f1 = f32::from_bits((src_lo >> 32) as u32);
                    let f2 = f32::from_bits(src_hi as u32);
                    let f3 = f32::from_bits((src_hi >> 32) as u32);
                    // Round to nearest-even (default MXCSR rounding mode).
                    let i0 = f32_to_i32_saturate(f0.round_ties_even()) as u32 as u64;
                    let i1 = f32_to_i32_saturate(f1.round_ties_even()) as u32 as u64;
                    let i2 = f32_to_i32_saturate(f2.round_ties_even()) as u32 as u64;
                    let i3 = f32_to_i32_saturate(f3.round_ties_even()) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
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
                    let convert = |v: u64| -> u64 {
                        let f0 = f32::from_bits(v as u32);
                        let f1 = f32::from_bits((v >> 32) as u32);
                        let i0 = f32_to_i32_saturate(f0.round_ties_even()) as u32 as u64;
                        let i1 = f32_to_i32_saturate(f1.round_ties_even()) as u32 as u64;
                        i0 | (i1 << 32)
                    };
                    self.regs.xmm[xmm_dst][0] = convert(src0);
                    self.regs.xmm[xmm_dst][1] = convert(src1);
                    self.regs.ymm_high[xmm_dst][0] = convert(src2);
                    self.regs.ymm_high[xmm_dst][1] = convert(src3);
                }
            }
            2 => {
                // VCVTTPS2DQ: convert packed singles to packed dword integers (truncate)
                if vex_l == 0 {
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let f0 = f32::from_bits(src_lo as u32);
                    let f1 = f32::from_bits((src_lo >> 32) as u32);
                    let f2 = f32::from_bits(src_hi as u32);
                    let f3 = f32::from_bits((src_hi >> 32) as u32);
                    let i0 = f32_to_i32_truncate(f0) as u32 as u64;
                    let i1 = f32_to_i32_truncate(f1) as u32 as u64;
                    let i2 = f32_to_i32_truncate(f2) as u32 as u64;
                    let i3 = f32_to_i32_truncate(f3) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
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
                    let convert = |v: u64| -> u64 {
                        let f0 = f32::from_bits(v as u32);
                        let f1 = f32::from_bits((v >> 32) as u32);
                        let i0 = f32_to_i32_truncate(f0) as u32 as u64;
                        let i1 = f32_to_i32_truncate(f1) as u32 as u64;
                        i0 | (i1 << 32)
                    };
                    self.regs.xmm[xmm_dst][0] = convert(src0);
                    self.regs.xmm[xmm_dst][1] = convert(src1);
                    self.regs.ymm_high[xmm_dst][0] = convert(src2);
                    self.regs.ymm_high[xmm_dst][1] = convert(src3);
                }
            }
            _ => return self.inject_undefined_instruction(),
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX conversion 0xE6: VCVTTPD2DQ, VCVTDQ2PD, VCVTPD2DQ
    pub(in crate::backend::emulator::x86_64) fn execute_vex_cvt_pd_dq(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_l: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;

        // Helper to saturate f64 to i32
        fn f64_to_i32_saturate(f: f64) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f64 {
                i32::MIN
            } else if f <= i32::MIN as f64 {
                i32::MIN
            } else {
                f as i32
            }
        }

        fn f64_to_i32_truncate(f: f64) -> i32 {
            if f.is_nan() {
                i32::MIN
            } else if f >= i32::MAX as f64 {
                i32::MIN
            } else if f <= i32::MIN as f64 {
                i32::MIN
            } else {
                f.trunc() as i32
            }
        }

        match vex_pp {
            1 => {
                // VCVTTPD2DQ: convert packed double to packed dword (truncate)
                if vex_l == 0 {
                    // 128-bit: 2 doubles -> 2 dwords in low 64 bits
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let d0 = f64::from_bits(src_lo);
                    let d1 = f64::from_bits(src_hi);
                    let i0 = f64_to_i32_truncate(d0) as u32 as u64;
                    let i1 = f64_to_i32_truncate(d1) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = 0;
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: 4 doubles -> 4 dwords in low 128 bits
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
                    let d0 = f64::from_bits(src0);
                    let d1 = f64::from_bits(src1);
                    let d2 = f64::from_bits(src2);
                    let d3 = f64::from_bits(src3);
                    let i0 = f64_to_i32_truncate(d0) as u32 as u64;
                    let i1 = f64_to_i32_truncate(d1) as u32 as u64;
                    let i2 = f64_to_i32_truncate(d2) as u32 as u64;
                    let i3 = f64_to_i32_truncate(d3) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            2 => {
                // VCVTDQ2PD: convert packed dword to packed double
                if vex_l == 0 {
                    // 128-bit: 2 dwords (64 bits) -> 2 doubles
                    let src = if is_memory {
                        self.read_mem(addr, 8)?
                    } else {
                        self.regs.xmm[rm as usize][0]
                    };
                    let i0 = src as i32;
                    let i1 = (src >> 32) as i32;
                    self.regs.xmm[xmm_dst][0] = (i0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (i1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: 4 dwords (128 bits) -> 4 doubles
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let i0 = src_lo as i32;
                    let i1 = (src_lo >> 32) as i32;
                    let i2 = src_hi as i32;
                    let i3 = (src_hi >> 32) as i32;
                    self.regs.xmm[xmm_dst][0] = (i0 as f64).to_bits();
                    self.regs.xmm[xmm_dst][1] = (i1 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][0] = (i2 as f64).to_bits();
                    self.regs.ymm_high[xmm_dst][1] = (i3 as f64).to_bits();
                }
            }
            3 => {
                // VCVTPD2DQ: convert packed double to packed dword (rounding)
                if vex_l == 0 {
                    // 128-bit: 2 doubles -> 2 dwords in low 64 bits
                    let (src_lo, src_hi) = if is_memory {
                        (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                    } else {
                        (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                    };
                    let d0 = f64::from_bits(src_lo);
                    let d1 = f64::from_bits(src_hi);
                    let i0 = f64_to_i32_saturate(d0.round_ties_even()) as u32 as u64;
                    let i1 = f64_to_i32_saturate(d1.round_ties_even()) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = 0;
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                } else {
                    // 256-bit: 4 doubles -> 4 dwords in low 128 bits
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
                    let d0 = f64::from_bits(src0);
                    let d1 = f64::from_bits(src1);
                    let d2 = f64::from_bits(src2);
                    let d3 = f64::from_bits(src3);
                    let i0 = f64_to_i32_saturate(d0.round_ties_even()) as u32 as u64;
                    let i1 = f64_to_i32_saturate(d1.round_ties_even()) as u32 as u64;
                    let i2 = f64_to_i32_saturate(d2.round_ties_even()) as u32 as u64;
                    let i3 = f64_to_i32_saturate(d3.round_ties_even()) as u32 as u64;
                    self.regs.xmm[xmm_dst][0] = i0 | (i1 << 32);
                    self.regs.xmm[xmm_dst][1] = i2 | (i3 << 32);
                    self.regs.ymm_high[xmm_dst][0] = 0;
                    self.regs.ymm_high[xmm_dst][1] = 0;
                }
            }
            _ => return self.inject_undefined_instruction(),
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX scalar int-to-float: VCVTSI2SS, VCVTSI2SD
    pub(in crate::backend::emulator::x86_64) fn execute_vex_cvtsi2s(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_w: u8,
        vvvv: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
        let xmm_dst = reg as usize;
        let xmm_src1 = vvvv as usize;

        // Source is 32-bit or 64-bit GPR/memory based on VEX.W
        let src = if is_memory {
            if vex_w != 0 {
                self.read_mem(addr, 8)? as i64
            } else {
                self.read_mem(addr, 4)? as i32 as i64
            }
        } else {
            if vex_w != 0 {
                self.get_reg(rm, 8) as i64
            } else {
                self.get_reg(rm, 4) as i32 as i64
            }
        };

        match vex_pp {
            2 => {
                // VCVTSI2SS: convert int to scalar single
                let result = (src as f32).to_bits() as u64;
                self.regs.xmm[xmm_dst][0] = (self.regs.xmm[xmm_src1][0] & !0xFFFFFFFF) | result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            }
            3 => {
                // VCVTSI2SD: convert int to scalar double
                let result = (src as f64).to_bits();
                self.regs.xmm[xmm_dst][0] = result;
                self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src1][1];
            }
            _ => unreachable!(),
        }
        self.regs.ymm_high[xmm_dst][0] = 0;
        self.regs.ymm_high[xmm_dst][1] = 0;

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX truncating float-to-int: VCVTTSS2SI, VCVTTSD2SI
    pub(in crate::backend::emulator::x86_64) fn execute_vex_cvtts2si(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let result = match vex_pp {
            2 => {
                // VCVTTSS2SI: convert scalar single to int (truncate)
                let src = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                if vex_w != 0 {
                    // 64-bit result
                    if src.is_nan() || src >= i64::MAX as f32 || src <= i64::MIN as f32 {
                        i64::MIN as u64
                    } else {
                        src.trunc() as i64 as u64
                    }
                } else {
                    // 32-bit result
                    if src.is_nan() || src >= i32::MAX as f32 || src <= i32::MIN as f32 {
                        i32::MIN as u32 as u64
                    } else {
                        src.trunc() as i32 as u32 as u64
                    }
                }
            }
            3 => {
                // VCVTTSD2SI: convert scalar double to int (truncate)
                let src = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                if vex_w != 0 {
                    if src.is_nan() || src >= i64::MAX as f64 || src <= i64::MIN as f64 {
                        i64::MIN as u64
                    } else {
                        src.trunc() as i64 as u64
                    }
                } else {
                    if src.is_nan() || src >= i32::MAX as f64 || src <= i32::MIN as f64 {
                        i32::MIN as u32 as u64
                    } else {
                        src.trunc() as i32 as u32 as u64
                    }
                }
            }
            _ => unreachable!(),
        };

        let size = if vex_w != 0 { 8 } else { 4 };
        self.set_reg(reg, result, size);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// VEX rounding float-to-int: VCVTSS2SI, VCVTSD2SI
    pub(in crate::backend::emulator::x86_64) fn execute_vex_cvts2si(
        &mut self,
        ctx: &mut InsnContext,
        vex_pp: u8,
        vex_w: u8,
    ) -> Result<Option<VcpuExit>> {
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let result = match vex_pp {
            2 => {
                // VCVTSS2SI: convert scalar single to int (round)
                let src = if is_memory {
                    f32::from_bits(self.read_mem(addr, 4)? as u32)
                } else {
                    f32::from_bits(self.regs.xmm[rm as usize][0] as u32)
                };
                if vex_w != 0 {
                    if src.is_nan() || src >= i64::MAX as f32 || src <= i64::MIN as f32 {
                        i64::MIN as u64
                    } else {
                        src.round_ties_even() as i64 as u64
                    }
                } else {
                    if src.is_nan() || src >= i32::MAX as f32 || src <= i32::MIN as f32 {
                        i32::MIN as u32 as u64
                    } else {
                        src.round_ties_even() as i32 as u32 as u64
                    }
                }
            }
            3 => {
                // VCVTSD2SI: convert scalar double to int (round)
                let src = if is_memory {
                    f64::from_bits(self.read_mem(addr, 8)?)
                } else {
                    f64::from_bits(self.regs.xmm[rm as usize][0])
                };
                if vex_w != 0 {
                    if src.is_nan() || src >= i64::MAX as f64 || src <= i64::MIN as f64 {
                        i64::MIN as u64
                    } else {
                        src.round_ties_even() as i64 as u64
                    }
                } else {
                    if src.is_nan() || src >= i32::MAX as f64 || src <= i32::MIN as f64 {
                        i32::MIN as u32 as u64
                    } else {
                        src.round_ties_even() as i32 as u32 as u64
                    }
                }
            }
            _ => unreachable!(),
        };

        let size = if vex_w != 0 { 8 } else { 4 };
        self.set_reg(reg, result, size);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}

fn read_half_from_xmm(xmm: [u64; 2], lane: usize) -> u16 {
    let qword = xmm[lane / 4];
    ((qword >> ((lane % 4) * 16)) & 0xFFFF) as u16
}

fn f16_to_f32_bits(bits: u16) -> u32 {
    let sign = ((bits & 0x8000) as u32) << 16;
    let exp = (bits >> 10) & 0x1f;
    let frac = (bits & 0x03ff) as u32;

    match exp {
        0 => {
            if frac == 0 {
                sign
            } else {
                let mut mant = frac;
                let mut unbiased_exp = -14i32;
                while (mant & 0x0400) == 0 {
                    mant <<= 1;
                    unbiased_exp -= 1;
                }
                mant &= 0x03ff;
                sign | (((unbiased_exp + 127) as u32) << 23) | (mant << 13)
            }
        }
        0x1f => {
            if frac == 0 {
                sign | 0x7f80_0000
            } else {
                sign | 0x7f80_0000 | 0x0040_0000 | (frac << 13)
            }
        }
        _ => sign | ((((exp as i32) - 15 + 127) as u32) << 23) | (frac << 13),
    }
}

fn f16_round_increment(negative: bool, base: u32, remainder: u32, shift: u32, rounding: u8) -> bool {
    if remainder == 0 {
        return false;
    }
    match rounding & 0x03 {
        0 => {
            let half = 1u32 << (shift - 1);
            remainder > half || (remainder == half && (base & 1) != 0)
        }
        1 => negative,
        2 => !negative,
        _ => false,
    }
}

fn f16_overflow_bits(sign: u32, negative: bool, rounding: u8) -> u16 {
    let round_to_infinity = match rounding & 0x03 {
        0 => true,
        1 => negative,
        2 => !negative,
        _ => false,
    };
    if round_to_infinity {
        (sign | 0x7c00) as u16
    } else {
        (sign | 0x7bff) as u16
    }
}

fn f32_to_f16_bits(value: f32, rounding: u8) -> u16 {
    let bits = value.to_bits();
    let sign = (bits >> 16) & 0x8000;
    let negative = sign != 0;
    let abs = bits & 0x7fff_ffff;
    let exp = (abs >> 23) as i32;
    let mant = abs & 0x007f_ffff;

    if exp == 0xff {
        if mant == 0 {
            return (sign | 0x7c00) as u16;
        }
        let payload = ((mant >> 13) | 0x0200).max(1);
        return (sign | 0x7c00 | payload) as u16;
    }

    if abs < 0x3300_0000 {
        if abs != 0 && matches!(rounding & 0x03, 1 if negative) {
            return (sign | 1) as u16;
        }
        if abs != 0 && matches!(rounding & 0x03, 2 if !negative) {
            return (sign | 1) as u16;
        }
        return sign as u16;
    }

    if abs < 0x3880_0000 {
        let mant24 = mant | 0x0080_0000;
        let shift = (126 - exp) as u32;
        let mut half_mant = mant24 >> shift;
        let remainder = mant24 & ((1u32 << shift) - 1);
        if f16_round_increment(negative, half_mant, remainder, shift, rounding) {
            half_mant += 1;
        }
        return (sign | half_mant) as u16;
    }

    let mut half = (abs - 0x3800_0000) >> 13;
    let remainder = abs & 0x1fff;
    if f16_round_increment(negative, half, remainder, 13, rounding) {
        half += 1;
    }

    if half >= 0x7c00 {
        f16_overflow_bits(sign, negative, rounding)
    } else {
        (sign | half) as u16
    }
}
