//! EVEX-encoded (AVX-512) instruction dispatch.
//!
//! EVEX prefix format (after 0x62):
//! - P0: R X B R' 0 m m m
//! - P1: W v v v v 1 p p
//! - P2: z L' L b V' a a a
//!
//! mm field (opcode map):
//! - 1: 0F (two-byte opcode)
//! - 2: 0F 38 (three-byte opcode)
//! - 3: 0F 3A (three-byte opcode with immediate)

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::insn;

impl X86_64Vcpu {
    /// Execute EVEX-encoded instruction.
    /// mm: opcode map (1=0F, 2=0F38, 3=0F3A)
    pub(in crate::backend::emulator::x86_64) fn execute_evex(
        &mut self,
        ctx: &mut InsnContext,
        mm: u8,
    ) -> Result<Option<VcpuExit>> {
        let opcode = ctx.consume_u8()?;

        match mm {
            1 => self.execute_evex_0f(ctx, opcode),
            2 => self.execute_evex_0f38(ctx, opcode),
            3 => self.execute_evex_0f3a(ctx, opcode),
            _ => Err(Error::Emulator(format!(
                "Invalid EVEX mm field {} at RIP={:#x}",
                mm, self.regs.rip
            ))),
        }
    }

    /// EVEX 0F opcode map (mm=1)
    fn execute_evex_0f(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.ok_or_else(|| {
            Error::Emulator("EVEX context missing".to_string())
        })?;

        match opcode {
            // VMOVUPS/VMOVAPS load (0x10/0x28)
            0x10 | 0x28 if evex.pp == 0 => {
                self.execute_evex_mov_load(ctx, opcode == 0x28)
            }
            // VMOVUPD/VMOVAPD load (0x10/0x28 with 66 prefix)
            0x10 | 0x28 if evex.pp == 1 => {
                self.execute_evex_mov_load(ctx, opcode == 0x28)
            }
            // VMOVUPS/VMOVAPS store (0x11/0x29)
            0x11 | 0x29 if evex.pp == 0 => {
                self.execute_evex_mov_store(ctx, opcode == 0x29)
            }
            // VMOVUPD/VMOVAPD store (0x11/0x29 with 66 prefix)
            0x11 | 0x29 if evex.pp == 1 => {
                self.execute_evex_mov_store(ctx, opcode == 0x29)
            }
            // VADDPS/VADDPD (0x58)
            0x58 => self.execute_evex_fp_arith(ctx, |a, b| a + b),
            // VMULPS/VMULPD (0x59)
            0x59 => self.execute_evex_fp_arith(ctx, |a, b| a * b),
            // VSUBPS/VSUBPD (0x5C)
            0x5C => self.execute_evex_fp_arith(ctx, |a, b| a - b),
            // VDIVPS/VDIVPD (0x5E)
            0x5E => self.execute_evex_fp_arith(ctx, |a, b| a / b),
            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.0F opcode {:#04x} at RIP={:#x}",
                opcode, self.regs.rip
            ))),
        }
    }

    /// EVEX move load (VMOVAPS/VMOVUPS, VMOVAPD/VMOVUPD)
    fn execute_evex_mov_load(
        &mut self,
        ctx: &mut InsnContext,
        aligned: bool,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.unwrap();
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        // Calculate full destination register (5 bits for ZMM16-31)
        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;

        // Vector length from L'L
        let vl = match evex.ll {
            0 => 16,  // 128-bit (XMM)
            1 => 32,  // 256-bit (YMM)
            2 => 64,  // 512-bit (ZMM)
            _ => 64,
        };

        if is_memory {
            // Check alignment for VMOVAPS/VMOVAPD
            if aligned && (addr % vl as u64) != 0 {
                return Err(Error::Emulator(format!(
                    "VMOVAPS: unaligned memory access at {:#x}", addr
                )));
            }
            // Load from memory to ZMM register
            self.load_zmm_from_mem(zmm_dst, addr, vl)?;
        } else {
            // Register to register move
            let zmm_src = if !evex.b { rm + 8 } else { rm };
            let zmm_src = zmm_src as usize; // ZMM16-31 not encoded in rm for reg-reg
            self.copy_zmm(zmm_dst, zmm_src, vl);
        }

        // Zero upper bits if not 512-bit
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

    /// EVEX move store (VMOVAPS/VMOVUPS, VMOVAPD/VMOVUPD)
    fn execute_evex_mov_store(
        &mut self,
        ctx: &mut InsnContext,
        aligned: bool,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.unwrap();
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        // Source register
        let zmm_src = if !evex.r { reg + 8 } else { reg };
        let zmm_src = if !evex.r_prime { zmm_src + 16 } else { zmm_src } as usize;

        // Vector length from L'L
        let vl = match evex.ll {
            0 => 16,  // 128-bit (XMM)
            1 => 32,  // 256-bit (YMM)
            2 => 64,  // 512-bit (ZMM)
            _ => 64,
        };

        if is_memory {
            // Check alignment for VMOVAPS/VMOVAPD
            if aligned && (addr % vl as u64) != 0 {
                return Err(Error::Emulator(format!(
                    "VMOVAPS: unaligned memory access at {:#x}", addr
                )));
            }
            // Store ZMM register to memory
            self.store_zmm_to_mem(zmm_src, addr, vl)?;
        } else {
            // Register to register move (destination is rm)
            let zmm_dst = if !evex.b { rm + 8 } else { rm } as usize;
            self.copy_zmm(zmm_dst, zmm_src, vl);

            // Zero upper bits if not 512-bit
            if vl < 64 && zmm_dst < 16 {
                if vl <= 16 {
                    self.regs.ymm_high[zmm_dst][0] = 0;
                    self.regs.ymm_high[zmm_dst][1] = 0;
                }
                self.regs.zmm_high[zmm_dst] = [0; 4];
            }
        }

        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// EVEX floating-point arithmetic (VADDPS/PD, VMULPS/PD, VSUBPS/PD, VDIVPS/PD)
    fn execute_evex_fp_arith<F>(
        &mut self,
        ctx: &mut InsnContext,
        op: F,
    ) -> Result<Option<VcpuExit>>
    where
        F: Fn(f32, f32) -> f32,
    {
        let evex = ctx.evex.unwrap();
        let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;

        // Destination register (5 bits)
        let zmm_dst = if !evex.r { reg + 8 } else { reg };
        let zmm_dst = if !evex.r_prime { zmm_dst + 16 } else { zmm_dst } as usize;

        // Source1 from vvvv
        let zmm_src1 = evex.vvvv as usize;

        // Vector length from L'L
        let vl = match evex.ll {
            0 => 16,  // 128-bit
            1 => 32,  // 256-bit
            2 => 64,  // 512-bit
            _ => 64,
        };

        // Number of f32 elements
        let num_elems = vl / 4;

        // Load source2
        let src2 = if is_memory {
            self.load_zmm_data(addr, vl)?
        } else {
            let zmm_src2 = if !evex.b { rm + 8 } else { rm } as usize;
            self.get_zmm_data(zmm_src2, vl)
        };

        // Get source1
        let src1 = self.get_zmm_data(zmm_src1, vl);

        // Perform operation
        let mut result = [0u8; 64];
        for i in 0..num_elems {
            let a = f32::from_le_bytes([src1[i*4], src1[i*4+1], src1[i*4+2], src1[i*4+3]]);
            let b = f32::from_le_bytes([src2[i*4], src2[i*4+1], src2[i*4+2], src2[i*4+3]]);
            let r = op(a, b);
            let bytes = r.to_le_bytes();
            result[i*4..i*4+4].copy_from_slice(&bytes);
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

    // ZMM register helper functions

    fn load_zmm_from_mem(&mut self, zmm: usize, addr: u64, vl: usize) -> Result<()> {
        if zmm < 16 {
            // ZMM0-15: load into xmm + ymm_high + zmm_high
            self.regs.xmm[zmm][0] = self.read_mem(addr, 8)?;
            self.regs.xmm[zmm][1] = self.read_mem(addr + 8, 8)?;
            if vl > 16 {
                self.regs.ymm_high[zmm][0] = self.read_mem(addr + 16, 8)?;
                self.regs.ymm_high[zmm][1] = self.read_mem(addr + 24, 8)?;
            }
            if vl > 32 {
                self.regs.zmm_high[zmm][0] = self.read_mem(addr + 32, 8)?;
                self.regs.zmm_high[zmm][1] = self.read_mem(addr + 40, 8)?;
                self.regs.zmm_high[zmm][2] = self.read_mem(addr + 48, 8)?;
                self.regs.zmm_high[zmm][3] = self.read_mem(addr + 56, 8)?;
            }
        } else {
            // ZMM16-31: load into zmm_ext
            let idx = zmm - 16;
            for i in 0..(vl / 8) {
                self.regs.zmm_ext[idx][i] = self.read_mem(addr + (i * 8) as u64, 8)?;
            }
        }
        Ok(())
    }

    fn store_zmm_to_mem(&mut self, zmm: usize, addr: u64, vl: usize) -> Result<()> {
        if zmm < 16 {
            self.write_mem(addr, self.regs.xmm[zmm][0], 8)?;
            self.write_mem(addr + 8, self.regs.xmm[zmm][1], 8)?;
            if vl > 16 {
                self.write_mem(addr + 16, self.regs.ymm_high[zmm][0], 8)?;
                self.write_mem(addr + 24, self.regs.ymm_high[zmm][1], 8)?;
            }
            if vl > 32 {
                self.write_mem(addr + 32, self.regs.zmm_high[zmm][0], 8)?;
                self.write_mem(addr + 40, self.regs.zmm_high[zmm][1], 8)?;
                self.write_mem(addr + 48, self.regs.zmm_high[zmm][2], 8)?;
                self.write_mem(addr + 56, self.regs.zmm_high[zmm][3], 8)?;
            }
        } else {
            let idx = zmm - 16;
            for i in 0..(vl / 8) {
                self.write_mem(addr + (i * 8) as u64, self.regs.zmm_ext[idx][i], 8)?;
            }
        }
        Ok(())
    }

    fn copy_zmm(&mut self, dst: usize, src: usize, vl: usize) {
        if dst < 16 && src < 16 {
            self.regs.xmm[dst] = self.regs.xmm[src];
            if vl > 16 {
                self.regs.ymm_high[dst] = self.regs.ymm_high[src];
            }
            if vl > 32 {
                self.regs.zmm_high[dst] = self.regs.zmm_high[src];
            }
        } else if dst >= 16 && src >= 16 {
            let d = dst - 16;
            let s = src - 16;
            for i in 0..(vl / 8) {
                self.regs.zmm_ext[d][i] = self.regs.zmm_ext[s][i];
            }
        } else if dst < 16 && src >= 16 {
            let s = src - 16;
            self.regs.xmm[dst][0] = self.regs.zmm_ext[s][0];
            self.regs.xmm[dst][1] = self.regs.zmm_ext[s][1];
            if vl > 16 {
                self.regs.ymm_high[dst][0] = self.regs.zmm_ext[s][2];
                self.regs.ymm_high[dst][1] = self.regs.zmm_ext[s][3];
            }
            if vl > 32 {
                self.regs.zmm_high[dst][0] = self.regs.zmm_ext[s][4];
                self.regs.zmm_high[dst][1] = self.regs.zmm_ext[s][5];
                self.regs.zmm_high[dst][2] = self.regs.zmm_ext[s][6];
                self.regs.zmm_high[dst][3] = self.regs.zmm_ext[s][7];
            }
        } else {
            // dst >= 16 && src < 16
            let d = dst - 16;
            self.regs.zmm_ext[d][0] = self.regs.xmm[src][0];
            self.regs.zmm_ext[d][1] = self.regs.xmm[src][1];
            if vl > 16 {
                self.regs.zmm_ext[d][2] = self.regs.ymm_high[src][0];
                self.regs.zmm_ext[d][3] = self.regs.ymm_high[src][1];
            }
            if vl > 32 {
                self.regs.zmm_ext[d][4] = self.regs.zmm_high[src][0];
                self.regs.zmm_ext[d][5] = self.regs.zmm_high[src][1];
                self.regs.zmm_ext[d][6] = self.regs.zmm_high[src][2];
                self.regs.zmm_ext[d][7] = self.regs.zmm_high[src][3];
            }
        }
    }

    fn get_zmm_data(&self, zmm: usize, vl: usize) -> [u8; 64] {
        let mut data = [0u8; 64];
        if zmm < 16 {
            data[0..8].copy_from_slice(&self.regs.xmm[zmm][0].to_le_bytes());
            data[8..16].copy_from_slice(&self.regs.xmm[zmm][1].to_le_bytes());
            if vl > 16 {
                data[16..24].copy_from_slice(&self.regs.ymm_high[zmm][0].to_le_bytes());
                data[24..32].copy_from_slice(&self.regs.ymm_high[zmm][1].to_le_bytes());
            }
            if vl > 32 {
                data[32..40].copy_from_slice(&self.regs.zmm_high[zmm][0].to_le_bytes());
                data[40..48].copy_from_slice(&self.regs.zmm_high[zmm][1].to_le_bytes());
                data[48..56].copy_from_slice(&self.regs.zmm_high[zmm][2].to_le_bytes());
                data[56..64].copy_from_slice(&self.regs.zmm_high[zmm][3].to_le_bytes());
            }
        } else {
            let idx = zmm - 16;
            for i in 0..(vl / 8) {
                let start = i * 8;
                data[start..start+8].copy_from_slice(&self.regs.zmm_ext[idx][i].to_le_bytes());
            }
        }
        data
    }

    fn load_zmm_data(&mut self, addr: u64, vl: usize) -> Result<[u8; 64]> {
        let mut data = [0u8; 64];
        for i in 0..(vl / 8) {
            let val = self.read_mem(addr + (i * 8) as u64, 8)?;
            let start = i * 8;
            data[start..start+8].copy_from_slice(&val.to_le_bytes());
        }
        Ok(data)
    }

    fn set_zmm_data(&mut self, zmm: usize, data: &[u8], vl: usize) {
        if zmm < 16 {
            self.regs.xmm[zmm][0] = u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]);
            self.regs.xmm[zmm][1] = u64::from_le_bytes([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]);
            if vl > 16 {
                self.regs.ymm_high[zmm][0] = u64::from_le_bytes([data[16], data[17], data[18], data[19], data[20], data[21], data[22], data[23]]);
                self.regs.ymm_high[zmm][1] = u64::from_le_bytes([data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31]]);
            }
            if vl > 32 {
                self.regs.zmm_high[zmm][0] = u64::from_le_bytes([data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39]]);
                self.regs.zmm_high[zmm][1] = u64::from_le_bytes([data[40], data[41], data[42], data[43], data[44], data[45], data[46], data[47]]);
                self.regs.zmm_high[zmm][2] = u64::from_le_bytes([data[48], data[49], data[50], data[51], data[52], data[53], data[54], data[55]]);
                self.regs.zmm_high[zmm][3] = u64::from_le_bytes([data[56], data[57], data[58], data[59], data[60], data[61], data[62], data[63]]);
            }
        } else {
            let idx = zmm - 16;
            for i in 0..(vl / 8) {
                let start = i * 8;
                self.regs.zmm_ext[idx][i] = u64::from_le_bytes([
                    data[start], data[start+1], data[start+2], data[start+3],
                    data[start+4], data[start+5], data[start+6], data[start+7]
                ]);
            }
        }
    }

    /// EVEX 0F38 opcode map (mm=2)
    fn execute_evex_0f38(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx.evex.ok_or_else(|| {
            Error::Emulator("EVEX context missing".to_string())
        })?;

        match opcode {
            // VPMULLD/VPMULLQ (0x40)
            // W=0: VPMULLD (32-bit elements)
            // W=1: VPMULLQ (64-bit elements)
            0x40 if evex.pp == 1 => {
                if evex.w {
                    insn::simd::vpmullq(self, ctx)
                } else {
                    insn::simd::vpmulld_evex(self, ctx)
                }
            }

            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.0F38 opcode {:#04x} (W={}) at RIP={:#x}",
                opcode, evex.w as u8, self.regs.rip
            ))),
        }
    }

    /// EVEX 0F3A opcode map (mm=3)
    fn execute_evex_0f3a(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        Err(Error::Emulator(format!(
            "Unimplemented EVEX.0F3A opcode {:#04x} at RIP={:#x}",
            opcode, self.regs.rip
        )))
    }
}
