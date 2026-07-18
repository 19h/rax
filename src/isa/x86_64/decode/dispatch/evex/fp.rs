//! fp.rs

use crate::isa::x86_64::decode::dispatch::evex::*;
use crate::error::{Error, Result};
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::{execute, flags};

impl X86_64Vcpu {

    /// EVEX single-precision FP arithmetic (VADDPS, VMULPS, VSUBPS, VDIVPS)
    pub(crate) fn execute_evex_fp_arith_ps<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f32, f32) -> f32,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        // Destination register (5 bits): reg + EVEX.R + EVEX.R'
        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;

        // Source1 from EVEX.vvvv (stored inverted) extended by EVEX.V'
        let zmm_src1 = ctx.evex_vvvv() as usize;

        // Vector length from L'L
        let vl = match evex.ll {
            0 => 16, // 128-bit
            1 => 32, // 256-bit
            2 => 64, // 512-bit
            _ => 64,
        };

        // Number of f32 elements
        let num_elems = vl / 4;
        let addr = if is_memory {
            let scale = if evex.broadcast { 4 } else { vl };
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
        } else {
            addr
        };

        // Load source2 (register operand also honors V'/X extension to 0-31)
        let src2 = if is_memory {
            if evex.broadcast {
                let value = self.read_mem(addr, 4)?.to_le_bytes();
                let mut data = [0u8; 64];
                for lane in 0..num_elems {
                    let base = lane * 4;
                    data[base..base + 4].copy_from_slice(&value[..4]);
                }
                data
            } else {
                self.load_zmm_data(addr, vl)?
            }
        } else {
            let zmm_src2 = Self::evex_rm_vec_reg(&evex, rm);
            self.get_zmm_data(zmm_src2, vl)
        };

        // Get source1
        let src1 = self.get_zmm_data(zmm_src1, vl);

        // Original destination contents (for merge masking)
        let dest_old = self.get_zmm_data(zmm_dst, vl);

        // Opmask: k0 => no masking (all elements active)
        let mask = Self::evex_kmask(&evex, &self.regs.k, num_elems);

        // Perform masked operation
        let mut result = [0u8; 64];
        for i in 0..num_elems {
            let base = i * 4;
            if (mask >> i) & 1 != 0 {
                let a = f32::from_le_bytes([
                    src1[base],
                    src1[base + 1],
                    src1[base + 2],
                    src1[base + 3],
                ]);
                let b = f32::from_le_bytes([
                    src2[base],
                    src2[base + 1],
                    src2[base + 2],
                    src2[base + 3],
                ]);
                let r = op(a, b);
                result[base..base + 4].copy_from_slice(&r.to_le_bytes());
            } else if evex.z {
                // Zeroing-masking: element becomes 0
            } else {
                // Merge-masking: keep original destination element
                result[base..base + 4].copy_from_slice(&dest_old[base..base + 4]);
            }
        }

        // Store result
        self.set_zmm_data(zmm_dst, &result[..vl], vl);

        // Zero upper bits if not 512-bit (for ZMM0-15)
        if vl < 64 && zmm_dst < 16 {
            if vl <= 16 {
                self.regs.ymm_high[zmm_dst][0] = 0;
                self.regs.ymm_high[zmm_dst][1] = 0;
            }
            self.regs.zmm_high[zmm_dst] = [0; 4];
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX double-precision FP arithmetic (VADDPD, VMULPD, VSUBPD, VDIVPD)
    pub(crate) fn execute_evex_fp_arith_pd<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f64, f64) -> f64,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        // Destination register (5 bits): reg + EVEX.R + EVEX.R'
        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;

        // Source1 from EVEX.vvvv (stored inverted) extended by EVEX.V'
        let zmm_src1 = ctx.evex_vvvv() as usize;

        // Vector length from L'L
        let vl = match evex.ll {
            0 => 16, // 128-bit
            1 => 32, // 256-bit
            2 => 64, // 512-bit
            _ => 64,
        };

        // Number of f64 elements
        let num_elems = vl / 8;
        let addr = if is_memory {
            let scale = if evex.broadcast { 8 } else { vl };
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
        } else {
            addr
        };

        // Load source2 (register operand also honors V'/X extension to 0-31)
        let src2 = if is_memory {
            if evex.broadcast {
                let value = self.read_mem(addr, 8)?.to_le_bytes();
                let mut data = [0u8; 64];
                for lane in 0..num_elems {
                    let base = lane * 8;
                    data[base..base + 8].copy_from_slice(&value);
                }
                data
            } else {
                self.load_zmm_data(addr, vl)?
            }
        } else {
            let zmm_src2 = Self::evex_rm_vec_reg(&evex, rm);
            self.get_zmm_data(zmm_src2, vl)
        };

        // Get source1
        let src1 = self.get_zmm_data(zmm_src1, vl);

        // Original destination contents (for merge masking)
        let dest_old = self.get_zmm_data(zmm_dst, vl);

        // Opmask: k0 => no masking (all elements active)
        let mask = Self::evex_kmask(&evex, &self.regs.k, num_elems);

        // Perform masked operation
        let mut result = [0u8; 64];
        for i in 0..num_elems {
            let base = i * 8;
            if (mask >> i) & 1 != 0 {
                let a = f64::from_le_bytes([
                    src1[base],
                    src1[base + 1],
                    src1[base + 2],
                    src1[base + 3],
                    src1[base + 4],
                    src1[base + 5],
                    src1[base + 6],
                    src1[base + 7],
                ]);
                let b = f64::from_le_bytes([
                    src2[base],
                    src2[base + 1],
                    src2[base + 2],
                    src2[base + 3],
                    src2[base + 4],
                    src2[base + 5],
                    src2[base + 6],
                    src2[base + 7],
                ]);
                let r = op(a, b);
                result[base..base + 8].copy_from_slice(&r.to_le_bytes());
            } else if evex.z {
                // Zeroing-masking: element becomes 0
            } else {
                // Merge-masking: keep original destination element
                result[base..base + 8].copy_from_slice(&dest_old[base..base + 8]);
            }
        }

        // Store result
        self.set_zmm_data(zmm_dst, &result[..vl], vl);

        // Zero upper bits if not 512-bit (for ZMM0-15)
        if vl < 64 && zmm_dst < 16 {
            if vl <= 16 {
                self.regs.ymm_high[zmm_dst][0] = 0;
                self.regs.ymm_high[zmm_dst][1] = 0;
            }
            self.regs.zmm_high[zmm_dst] = [0; 4];
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX packed single-precision unary FP operation (VSQRTPS).
    pub(crate) fn execute_evex_fp_unary_ps<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f32) -> f32,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;
        let vl = match evex.ll {
            0 => 16,
            1 => 32,
            2 => 64,
            _ => 64,
        };
        let num_elems = vl / 4;
        let addr = if is_memory {
            let scale = if evex.broadcast { 4 } else { vl };
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
        } else {
            addr
        };

        let src = if is_memory {
            if evex.broadcast {
                let value = self.read_mem(addr, 4)?.to_le_bytes();
                let mut data = [0u8; 64];
                for lane in 0..num_elems {
                    let base = lane * 4;
                    data[base..base + 4].copy_from_slice(&value[..4]);
                }
                data
            } else {
                self.load_zmm_data(addr, vl)?
            }
        } else {
            let zmm_src = Self::evex_rm_vec_reg(&evex, rm);
            self.get_zmm_data(zmm_src, vl)
        };

        let dest_old = self.get_zmm_data(zmm_dst, vl);
        let mask = Self::evex_kmask(&evex, &self.regs.k, num_elems);
        let mut result = [0u8; 64];
        for lane in 0..num_elems {
            let base = lane * 4;
            if (mask >> lane) & 1 != 0 {
                let value = f32::from_le_bytes(src[base..base + 4].try_into().unwrap());
                result[base..base + 4].copy_from_slice(&op(value).to_le_bytes());
            } else if evex.z {
                // Zeroing: leave as 0.
            } else {
                result[base..base + 4].copy_from_slice(&dest_old[base..base + 4]);
            }
        }

        self.set_zmm_data(zmm_dst, &result[..vl], vl);
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX packed double-precision unary FP operation (VSQRTPD).
    pub(crate) fn execute_evex_fp_unary_pd<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f64) -> f64,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;
        let vl = match evex.ll {
            0 => 16,
            1 => 32,
            2 => 64,
            _ => 64,
        };
        let num_elems = vl / 8;
        let addr = if is_memory {
            let scale = if evex.broadcast { 8 } else { vl };
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
        } else {
            addr
        };

        let src = if is_memory {
            if evex.broadcast {
                let value = self.read_mem(addr, 8)?.to_le_bytes();
                let mut data = [0u8; 64];
                for lane in 0..num_elems {
                    let base = lane * 8;
                    data[base..base + 8].copy_from_slice(&value);
                }
                data
            } else {
                self.load_zmm_data(addr, vl)?
            }
        } else {
            let zmm_src = Self::evex_rm_vec_reg(&evex, rm);
            self.get_zmm_data(zmm_src, vl)
        };

        let dest_old = self.get_zmm_data(zmm_dst, vl);
        let mask = Self::evex_kmask(&evex, &self.regs.k, num_elems);
        let mut result = [0u8; 64];
        for lane in 0..num_elems {
            let base = lane * 8;
            if (mask >> lane) & 1 != 0 {
                let value = f64::from_le_bytes(src[base..base + 8].try_into().unwrap());
                result[base..base + 8].copy_from_slice(&op(value).to_le_bytes());
            } else if evex.z {
                // Zeroing: leave as 0.
            } else {
                result[base..base + 8].copy_from_slice(&dest_old[base..base + 8]);
            }
        }

        self.set_zmm_data(zmm_dst, &result[..vl], vl);
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX scalar single-precision FP arithmetic (VADDSS, VMULSS, VSUBSS, VDIVSS).
    pub(crate) fn execute_evex_fp_scalar_arith_f32<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f32, f32) -> f32,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let dst = if !evex.r { reg + 8 } else { reg };
        let dst = if !evex.r_prime { dst + 16 } else { dst } as usize;
        let src1 = ctx.evex_vvvv() as usize;
        let addr = if is_memory {
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, 4)
        } else {
            addr
        };
        let src2 = if is_memory {
            f32::from_bits(self.read_mem(addr, 4)? as u32)
        } else {
            let src2_reg = Self::evex_rm_vec_reg(&evex, rm);
            let src2_data = self.get_zmm_data(src2_reg, 16);
            f32::from_bits(u32::from_le_bytes([
                src2_data[0],
                src2_data[1],
                src2_data[2],
                src2_data[3],
            ]))
        };

        let src1_data = self.get_zmm_data(src1, 16);
        let dest_old = self.get_zmm_data(dst, 16);
        let src1_scalar = f32::from_bits(u32::from_le_bytes([
            src1_data[0],
            src1_data[1],
            src1_data[2],
            src1_data[3],
        ]));

        let mut result = [0u8; 64];
        result[4..16].copy_from_slice(&src1_data[4..16]);
        if evex.aaa == 0 || (self.regs.k[evex.aaa as usize] & 1) != 0 {
            result[0..4].copy_from_slice(&op(src1_scalar, src2).to_bits().to_le_bytes());
        } else if evex.z {
            result[0..4].fill(0);
        } else {
            result[0..4].copy_from_slice(&dest_old[0..4]);
        }

        self.set_zmm_data(dst, &result[..16], 16);
        self.zero_zmm_upper_from_128(dst);
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX scalar double-precision FP arithmetic (VADDSD, VMULSD, VSUBSD, VDIVSD).
    pub(crate) fn execute_evex_fp_scalar_arith_f64<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f64, f64) -> f64,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let dst = if !evex.r { reg + 8 } else { reg };
        let dst = if !evex.r_prime { dst + 16 } else { dst } as usize;
        let src1 = ctx.evex_vvvv() as usize;
        let addr = if is_memory {
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, 8)
        } else {
            addr
        };
        let src2 = if is_memory {
            f64::from_bits(self.read_mem(addr, 8)?)
        } else {
            let src2_reg = Self::evex_rm_vec_reg(&evex, rm);
            let src2_data = self.get_zmm_data(src2_reg, 16);
            f64::from_bits(u64::from_le_bytes([
                src2_data[0],
                src2_data[1],
                src2_data[2],
                src2_data[3],
                src2_data[4],
                src2_data[5],
                src2_data[6],
                src2_data[7],
            ]))
        };

        let src1_data = self.get_zmm_data(src1, 16);
        let dest_old = self.get_zmm_data(dst, 16);
        let src1_scalar = f64::from_bits(u64::from_le_bytes([
            src1_data[0],
            src1_data[1],
            src1_data[2],
            src1_data[3],
            src1_data[4],
            src1_data[5],
            src1_data[6],
            src1_data[7],
        ]));

        let mut result = [0u8; 64];
        result[8..16].copy_from_slice(&src1_data[8..16]);
        if evex.aaa == 0 || (self.regs.k[evex.aaa as usize] & 1) != 0 {
            result[0..8].copy_from_slice(&op(src1_scalar, src2).to_bits().to_le_bytes());
        } else if evex.z {
            result[0..8].fill(0);
        } else {
            result[0..8].copy_from_slice(&dest_old[0..8]);
        }

        self.set_zmm_data(dst, &result[..16], 16);
        self.zero_zmm_upper_from_128(dst);
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX FP bitwise logical operation (VAND*/VANDN*/VOR*/VXOR*).
    pub(crate) fn execute_evex_fp_bitwise<F>(
        &mut self,
        ctx: &mut InsnContext,
        elem_size: usize,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(u8, u8) -> u8,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;
        let zmm_src1 = ctx.evex_vvvv() as usize;
        let vl = match evex.ll {
            0 => 16, // 128-bit
            1 => 32, // 256-bit
            2 => 64, // 512-bit
            _ => 64,
        };
        let num_elems = vl / elem_size;
        let addr = if is_memory {
            let scale = if evex.broadcast { elem_size } else { vl };
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
        } else {
            addr
        };

        let src2 = if is_memory {
            if evex.broadcast {
                let value = self.read_mem(addr, elem_size as u8)?;
                let value = value.to_le_bytes();
                let mut data = [0u8; 64];
                for lane in 0..num_elems {
                    let base = lane * elem_size;
                    data[base..base + elem_size].copy_from_slice(&value[..elem_size]);
                }
                data
            } else {
                self.load_zmm_data(addr, vl)?
            }
        } else {
            let zmm_src2 = Self::evex_rm_vec_reg(&evex, rm);
            self.get_zmm_data(zmm_src2, vl)
        };
        let src1 = self.get_zmm_data(zmm_src1, vl);
        let dest_old = self.get_zmm_data(zmm_dst, vl);
        let mask = Self::evex_kmask(&evex, &self.regs.k, num_elems);

        let mut result = [0u8; 64];
        for lane in 0..num_elems {
            let base = lane * elem_size;
            if (mask >> lane) & 1 != 0 {
                for byte in 0..elem_size {
                    result[base + byte] = op(src1[base + byte], src2[base + byte]);
                }
            } else if evex.z {
                // Zeroing: leave this element as 0.
            } else {
                result[base..base + elem_size].copy_from_slice(&dest_old[base..base + elem_size]);
            }
        }

        self.set_zmm_data(zmm_dst, &result[..vl], vl);

        if vl < 64 && zmm_dst < 16 {
            if vl <= 16 {
                self.regs.ymm_high[zmm_dst][0] = 0;
                self.regs.ymm_high[zmm_dst][1] = 0;
            }
            self.regs.zmm_high[zmm_dst] = [0; 4];
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX FP16 (half-precision) packed arithmetic/min/max.
    pub(crate) fn execute_evex_fp16_arith<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f32, f32) -> f32,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        // Destination register (5 bits)
        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;

        // Source1 from vvvv (inverted), extended by EVEX.V'
        let zmm_src1 = ctx.evex_vvvv() as usize;

        // Vector length from L'L
        let vl = match evex.ll {
            0 => 16, // 128-bit (8 FP16 values)
            1 => 32, // 256-bit (16 FP16 values)
            2 => 64, // 512-bit (32 FP16 values)
            _ => 64,
        };

        // Number of FP16 elements (2 bytes each)
        let num_elems = vl / 2;
        let addr = if is_memory {
            let scale = if evex.broadcast { 2 } else { vl };
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
        } else {
            addr
        };

        let src2 = if is_memory {
            if evex.broadcast {
                let value = self.read_mem(addr, 2)?.to_le_bytes();
                let mut data = [0u8; 64];
                for lane in 0..num_elems {
                    let base = lane * 2;
                    data[base..base + 2].copy_from_slice(&value[..2]);
                }
                data
            } else {
                self.load_zmm_data(addr, vl)?
            }
        } else {
            let zmm_src2 = Self::evex_rm_vec_reg(&evex, rm);
            self.get_zmm_data(zmm_src2, vl)
        };

        let src1 = self.get_zmm_data(zmm_src1, vl);
        let dest_old = self.get_zmm_data(zmm_dst, vl);
        let mask = Self::evex_kmask(&evex, &self.regs.k, num_elems);

        let mut result = [0u8; 64];
        for i in 0..num_elems {
            let base = i * 2;
            if (mask >> i) & 1 != 0 {
                let a_fp16 = u16::from_le_bytes([src1[base], src1[base + 1]]);
                let b_fp16 = u16::from_le_bytes([src2[base], src2[base + 1]]);
                let r_fp16 = f32_to_fp16(op(fp16_to_f32(a_fp16), fp16_to_f32(b_fp16)));
                result[base..base + 2].copy_from_slice(&r_fp16.to_le_bytes());
            } else if evex.z {
                // Zeroing: leave as 0.
            } else {
                result[base..base + 2].copy_from_slice(&dest_old[base..base + 2]);
            }
        }

        self.set_zmm_data(zmm_dst, &result[..vl], vl);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX FP16 packed unary operation (VSQRTPH).
    pub(crate) fn execute_evex_fp16_unary<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f32) -> f32,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;

        let vl = match evex.ll {
            0 => 16,
            1 => 32,
            2 => 64,
            _ => 64,
        };
        let num_elems = vl / 2;
        let addr = if is_memory {
            let scale = if evex.broadcast { 2 } else { vl };
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
        } else {
            addr
        };

        let src = if is_memory {
            if evex.broadcast {
                let value = self.read_mem(addr, 2)?.to_le_bytes();
                let mut data = [0u8; 64];
                for lane in 0..num_elems {
                    let base = lane * 2;
                    data[base..base + 2].copy_from_slice(&value[..2]);
                }
                data
            } else {
                self.load_zmm_data(addr, vl)?
            }
        } else {
            let zmm_src = Self::evex_rm_vec_reg(&evex, rm);
            self.get_zmm_data(zmm_src, vl)
        };

        let dest_old = self.get_zmm_data(zmm_dst, vl);
        let mask = Self::evex_kmask(&evex, &self.regs.k, num_elems);
        let mut result = [0u8; 64];

        for lane in 0..num_elems {
            let base = lane * 2;
            if (mask >> lane) & 1 != 0 {
                let value = u16::from_le_bytes([src[base], src[base + 1]]);
                let result_fp16 = f32_to_fp16(op(fp16_to_f32(value)));
                result[base..base + 2].copy_from_slice(&result_fp16.to_le_bytes());
            } else if evex.z {
                // Zeroing: leave as 0.
            } else {
                result[base..base + 2].copy_from_slice(&dest_old[base..base + 2]);
            }
        }

        self.set_zmm_data(zmm_dst, &result[..vl], vl);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }


    /// EVEX FP16 scalar arithmetic/min/max/sqrt.
    pub(crate) fn execute_evex_fp16_scalar_arith<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f32, f32) -> f32,
    {
        let evex = ctx.evex.unwrap();
        let modrm_start = ctx.cursor;
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        let dst = if !evex.r { reg + 8 } else { reg };
        let dst = if !evex.r_prime { dst + 16 } else { dst } as usize;
        let src1 = ctx.evex_vvvv() as usize;
        let addr = if is_memory {
            execute::simd::evex_scaled_disp8_addr(ctx, modrm_start, addr, 2)
        } else {
            addr
        };
        let src2 = if is_memory {
            self.read_mem(addr, 2)? as u16
        } else {
            let src2_reg = Self::evex_rm_vec_reg(&evex, rm);
            let src2_data = self.get_zmm_data(src2_reg, 16);
            u16::from_le_bytes([src2_data[0], src2_data[1]])
        };

        let src1_data = self.get_zmm_data(src1, 16);
        let dest_old = self.get_zmm_data(dst, 16);
        let src1_scalar = u16::from_le_bytes([src1_data[0], src1_data[1]]);

        let mut result = [0u8; 64];
        result[2..16].copy_from_slice(&src1_data[2..16]);
        if evex.aaa == 0 || (self.regs.k[evex.aaa as usize] & 1) != 0 {
            let r = f32_to_fp16(op(fp16_to_f32(src1_scalar), fp16_to_f32(src2)));
            result[0..2].copy_from_slice(&r.to_le_bytes());
        } else if evex.z {
            result[0..2].fill(0);
        } else {
            result[0..2].copy_from_slice(&dest_old[0..2]);
        }

        self.set_zmm_data(dst, &result[..16], 16);
        self.zero_zmm_upper_from_128(dst);

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }
}
