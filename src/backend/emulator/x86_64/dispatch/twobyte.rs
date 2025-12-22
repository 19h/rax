//! Two-byte (0x0F-prefixed) opcode dispatch for the x86_64 CPU emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::flags;
use super::super::insn;

impl X86_64Vcpu {
    /// Execute two-byte opcodes (0x0F prefix).
    pub(in crate::backend::emulator::x86_64) fn execute_0f(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let opcode2 = ctx.consume_u8()?;

        match opcode2 {
            // System
            0x00 => insn::system::group6(self, ctx),
            0x01 => self.execute_0f01(ctx),
            0x02 => insn::system::lar(self, ctx),
            0x03 => insn::system::lsl(self, ctx),
            // INVD/WBINVD - cache invalidation (NOP in emulator)
            0x08 => {
                // INVD - Invalidate internal caches
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x09 => {
                // WBINVD - Write back and invalidate caches
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x20 => insn::system::mov_r_cr(self, ctx),
            0x22 => insn::system::mov_cr_r(self, ctx),
            0x30 => insn::system::wrmsr(self, ctx),
            0x31 => insn::system::rdtsc(self, ctx),
            0x32 => insn::system::rdmsr(self, ctx),
            0xA2 => insn::system::cpuid(self, ctx),
            0xAE => self.execute_0fae(ctx),

            // Control flow
            0x40..=0x4F => insn::control::cmovcc(self, ctx, opcode2 & 0x0F),
            0x80..=0x8F => insn::control::jcc_rel32(self, ctx, opcode2 & 0x0F),
            0x90..=0x9F => insn::control::setcc(self, ctx, opcode2 & 0x0F),

            // Data movement
            0xB6 => insn::data::movzx_r_rm8(self, ctx),
            0xB7 => insn::data::movzx_r_rm16(self, ctx),
            0xBE => insn::data::movsx_r_rm8(self, ctx),
            0xBF => insn::data::movsx_r_rm16(self, ctx),
            0xC8..=0xCF => insn::data::bswap(self, ctx, opcode2),

            // Arithmetic
            0xAF => insn::arith::imul_r_rm(self, ctx),

            // Bit manipulation
            0xA3 => insn::bit::bt_rm_r(self, ctx),
            0xAB => insn::bit::bts_rm_r(self, ctx),
            0xB3 => insn::bit::btr_rm_r(self, ctx),
            0xBB => insn::bit::btc_rm_r(self, ctx),
            0xBA => insn::bit::group8(self, ctx),
            0xB8 => insn::bit::popcnt(self, ctx),
            // BSF/TZCNT and BSR/LZCNT share opcodes - F3 prefix differentiates
            0xBC => {
                if ctx.rep_prefix == Some(0xF3) {
                    insn::bit::tzcnt(self, ctx)
                } else {
                    insn::bit::bsf(self, ctx)
                }
            }
            0xBD => {
                if ctx.rep_prefix == Some(0xF3) {
                    insn::bit::lzcnt(self, ctx)
                } else {
                    insn::bit::bsr(self, ctx)
                }
            }

            // CMPXCHG
            0xB0 => insn::data::cmpxchg_rm8_r8(self, ctx),
            0xB1 => insn::data::cmpxchg_rm_r(self, ctx),

            // XADD
            0xC0 => insn::data::xadd_rm8_r8(self, ctx),
            0xC1 => insn::data::xadd_rm_r(self, ctx),

            // SHLD/SHRD
            0xA4 => insn::shift::shld_imm8(self, ctx),
            0xA5 => insn::shift::shld_cl(self, ctx),
            0xAC => insn::shift::shrd_imm8(self, ctx),
            0xAD => insn::shift::shrd_cl(self, ctx),

            // NOP variants
            0x1E => insn::system::endbr(self, ctx),
            0x1F => insn::system::nop_rm(self, ctx),

            // MOVUPS/MOVUPD (0x10/0x11 unaligned), MOVAPS/MOVAPD (0x28/0x29 aligned)
            0x10 => {
                // NP 0F 10: MOVUPS xmm, xmm/m128 (unaligned)
                // 66 0F 10: MOVUPD xmm, xmm/m128 (unaligned)
                // F3 0F 10: MOVSS xmm, xmm/m32
                // F2 0F 10: MOVSD xmm, xmm/m64
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;

                if ctx.rep_prefix == Some(0xF3) {
                    // MOVSS - move scalar single
                    let value = if is_memory {
                        self.read_mem(addr, 4)?
                    } else {
                        self.regs.xmm[rm as usize][0] & 0xFFFFFFFF
                    };
                    if is_memory {
                        self.regs.xmm[xmm_dst][0] = value;
                        self.regs.xmm[xmm_dst][1] = 0;
                    } else {
                        // Reg-to-reg: merge low 32 bits, keep rest
                        self.regs.xmm[xmm_dst][0] =
                            (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | value;
                    }
                } else if ctx.rep_prefix == Some(0xF2) {
                    // MOVSD - move scalar double
                    let value = if is_memory {
                        self.read_mem(addr, 8)?
                    } else {
                        self.regs.xmm[rm as usize][0]
                    };
                    if is_memory {
                        self.regs.xmm[xmm_dst][0] = value;
                        self.regs.xmm[xmm_dst][1] = 0;
                    } else {
                        self.regs.xmm[xmm_dst][0] = value;
                    }
                } else {
                    // MOVUPS/MOVUPD - move packed (unaligned OK)
                    if is_memory {
                        self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                        self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                    } else {
                        let xmm_src = rm as usize;
                        self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                        self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                    }
                }
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x11 => {
                // NP 0F 11: MOVUPS xmm/m128, xmm (unaligned store)
                // 66 0F 11: MOVUPD xmm/m128, xmm (unaligned store)
                // F3 0F 11: MOVSS xmm/m32, xmm
                // F2 0F 11: MOVSD xmm/m64, xmm
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_src = reg as usize;

                if ctx.rep_prefix == Some(0xF3) {
                    // MOVSS store
                    let value = self.regs.xmm[xmm_src][0] & 0xFFFFFFFF;
                    if is_memory {
                        self.write_mem(addr, value, 4)?;
                    } else {
                        let xmm_dst = rm as usize;
                        self.regs.xmm[xmm_dst][0] =
                            (self.regs.xmm[xmm_dst][0] & !0xFFFFFFFF) | value;
                    }
                } else if ctx.rep_prefix == Some(0xF2) {
                    // MOVSD store
                    let value = self.regs.xmm[xmm_src][0];
                    if is_memory {
                        self.write_mem(addr, value, 8)?;
                    } else {
                        let xmm_dst = rm as usize;
                        self.regs.xmm[xmm_dst][0] = value;
                    }
                } else {
                    // MOVUPS/MOVUPD store
                    if is_memory {
                        self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                        self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                    } else {
                        let xmm_dst = rm as usize;
                        self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                        self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                    }
                }
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x28 => {
                // NP 0F 28: MOVAPS xmm, xmm/m128
                // 66 0F 28: MOVAPD xmm, xmm/m128
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;

                if is_memory {
                    if addr & 0xF != 0 {
                        return Err(Error::Emulator(format!(
                            "MOVAPS/MOVAPD: unaligned memory access at {:#x}",
                            addr
                        )));
                    }
                    self.regs.xmm[xmm_dst][0] = self.read_mem(addr, 8)?;
                    self.regs.xmm[xmm_dst][1] = self.read_mem(addr + 8, 8)?;
                } else {
                    let xmm_src = rm as usize;
                    self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                    self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                }
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x29 => {
                // NP 0F 29: MOVAPS xmm/m128, xmm
                // 66 0F 29: MOVAPD xmm/m128, xmm
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_src = reg as usize;

                if is_memory {
                    if addr & 0xF != 0 {
                        return Err(Error::Emulator(format!(
                            "MOVAPS/MOVAPD: unaligned memory access at {:#x}",
                            addr
                        )));
                    }
                    self.write_mem(addr, self.regs.xmm[xmm_src][0], 8)?;
                    self.write_mem(addr + 8, self.regs.xmm[xmm_src][1], 8)?;
                } else {
                    let xmm_dst = rm as usize;
                    self.regs.xmm[xmm_dst][0] = self.regs.xmm[xmm_src][0];
                    self.regs.xmm[xmm_dst][1] = self.regs.xmm[xmm_src][1];
                }
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // SSE logical operations
            0x54 => {
                // NP 0F 54: ANDPS xmm, xmm/m128
                // 66 0F 54: ANDPD xmm, xmm/m128
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] &= src_lo;
                self.regs.xmm[xmm_dst][1] &= src_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x55 => {
                // NP 0F 55: ANDNPS xmm, xmm/m128
                // 66 0F 55: ANDNPD xmm, xmm/m128
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] = (!self.regs.xmm[xmm_dst][0]) & src_lo;
                self.regs.xmm[xmm_dst][1] = (!self.regs.xmm[xmm_dst][1]) & src_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x56 => {
                // NP 0F 56: ORPS xmm, xmm/m128
                // 66 0F 56: ORPD xmm, xmm/m128
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] |= src_lo;
                self.regs.xmm[xmm_dst][1] |= src_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            0x57 => {
                // NP 0F 57: XORPS xmm, xmm/m128
                // 66 0F 57: XORPD xmm, xmm/m128
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                let xmm_dst = reg as usize;
                let (src_lo, src_hi) = if is_memory {
                    (self.read_mem(addr, 8)?, self.read_mem(addr + 8, 8)?)
                } else {
                    (self.regs.xmm[rm as usize][0], self.regs.xmm[rm as usize][1])
                };
                self.regs.xmm[xmm_dst][0] ^= src_lo;
                self.regs.xmm[xmm_dst][1] ^= src_hi;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }

            // SSE arithmetic
            0x51 => self.execute_sse_sqrt(ctx),
            0x58 => self.execute_sse_add(ctx),
            0x59 => self.execute_sse_mul(ctx),
            0x5C => self.execute_sse_sub(ctx),
            0x5D => self.execute_sse_min(ctx),
            0x5E => self.execute_sse_div(ctx),
            0x5F => self.execute_sse_max(ctx),

            // SSE unpack
            0x14 => self.execute_sse_unpcklps(ctx),
            0x15 => self.execute_sse_unpckhps(ctx),

            // MOVD/MOVQ
            0x6E => {
                if ctx.operand_size_override {
                    // 66 0F 6E: MOVD/MOVQ xmm, r/m32 (or r/m64 with REX.W)
                    insn::simd::movd_xmm_rm(self, ctx)
                } else {
                    // NP 0F 6E: MOVD/MOVQ mm, r/m32 (or r/m64 with REX.W)
                    insn::simd::movd_mm_rm(self, ctx)
                }
            }
            0x7E => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 7E: MOVQ xmm1, xmm2/m64
                    insn::simd::movq_xmm_xmm_m64(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 7E: MOVD/MOVQ r/m32, xmm (or r/m64 with REX.W)
                    insn::simd::movd_rm_xmm(self, ctx)
                } else {
                    // NP 0F 7E: MOVD/MOVQ r/m32, mm (or r/m64 with REX.W)
                    insn::simd::movd_rm_mm(self, ctx)
                }
            }
            0xD6 => {
                if ctx.operand_size_override {
                    // 66 0F D6: MOVQ xmm2/m64, xmm1
                    insn::simd::movq_xmm_m64_xmm(self, ctx)
                } else {
                    Err(Error::Emulator(format!(
                        "unimplemented 0x0F 0xD6 opcode variant at RIP={:#x}",
                        self.regs.rip
                    )))
                }
            }

            // SSE/SSE2 Conversion Instructions
            0x5A => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 5A: CVTSS2SD xmm1, xmm2/m32
                    insn::simd::cvtss2sd(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 5A: CVTSD2SS xmm1, xmm2/m64
                    insn::simd::cvtsd2ss(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 5A: CVTPD2PS xmm1, xmm2/m128
                    insn::simd::cvtpd2ps(self, ctx)
                } else {
                    // NP 0F 5A: CVTPS2PD xmm1, xmm2/m64
                    insn::simd::cvtps2pd(self, ctx)
                }
            }
            0x5B => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 5B: CVTTPS2DQ xmm1, xmm2/m128
                    insn::simd::cvttps2dq(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 5B: CVTPS2DQ xmm1, xmm2/m128
                    insn::simd::cvtps2dq(self, ctx)
                } else {
                    // NP 0F 5B: CVTDQ2PS xmm1, xmm2/m128
                    insn::simd::cvtdq2ps(self, ctx)
                }
            }
            0x2A => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 2A: CVTSI2SS xmm1, r/m32 or r/m64
                    insn::simd::cvtsi2ss(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 2A: CVTSI2SD xmm1, r/m32 or r/m64
                    insn::simd::cvtsi2sd(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 2A: CVTPI2PD xmm, mm/m64
                    insn::simd::cvtpi2pd(self, ctx)
                } else {
                    // NP 0F 2A: CVTPI2PS xmm, mm/m64
                    insn::simd::cvtpi2ps(self, ctx)
                }
            }
            0x2C => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 2C: CVTTSS2SI r32/r64, xmm1/m32
                    insn::simd::cvttss2si(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 2C: CVTTSD2SI r32/r64, xmm1/m64
                    insn::simd::cvttsd2si(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 2C: CVTTPD2PI mm, xmm/m128
                    insn::simd::cvttpd2pi(self, ctx)
                } else {
                    // NP 0F 2C: CVTTPS2PI mm, xmm/m64
                    insn::simd::cvttps2pi(self, ctx)
                }
            }
            0x2D => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 2D: CVTSS2SI r32/r64, xmm1/m32
                    insn::simd::cvtss2si(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 2D: CVTSD2SI r32/r64, xmm1/m64
                    insn::simd::cvtsd2si(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 2D: CVTPD2PI mm, xmm/m128
                    insn::simd::cvtpd2pi(self, ctx)
                } else {
                    // NP 0F 2D: CVTPS2PI mm, xmm/m64
                    insn::simd::cvtps2pi(self, ctx)
                }
            }
            0xE6 => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F E6: CVTDQ2PD xmm1, xmm2/m64
                    insn::simd::cvtdq2pd(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F E6: CVTPD2DQ xmm1, xmm2/m128
                    insn::simd::cvtpd2dq(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F E6: CVTTPD2DQ xmm1, xmm2/m128
                    insn::simd::cvttpd2dq(self, ctx)
                } else {
                    Err(Error::Emulator(format!(
                        "unimplemented 0x0F 0xE6 opcode variant at RIP={:#x}",
                        self.regs.rip
                    )))
                }
            }

            // 0F 38 escape - MOVBE and other instructions
            0x38 => self.execute_0f38(ctx),

            // MOVDQA/MOVDQU/MOVQ load (0x6F)
            0x6F => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 6F: MOVDQU xmm, xmm/m128 (unaligned)
                    insn::simd::movdqu_xmm_xmm_m128(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 6F: MOVDQA xmm, xmm/m128 (aligned)
                    insn::simd::movdqa_xmm_xmm_m128(self, ctx)
                } else {
                    // NP 0F 6F: MOVQ mm, mm/m64 (MMX)
                    insn::simd::movq_mm_mm_m64(self, ctx)
                }
            }

            // PSHUFD/PSHUFHW/PSHUFLW (0x70)
            0x70 => self.execute_pshufd(ctx),

            // MOVDQA/MOVDQU/MOVQ store (0x7F)
            0x7F => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 7F: MOVDQU xmm/m128, xmm (unaligned)
                    insn::simd::movdqu_xmm_m128_xmm(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 7F: MOVDQA xmm/m128, xmm (aligned)
                    insn::simd::movdqa_xmm_m128_xmm(self, ctx)
                } else {
                    // NP 0F 7F: MOVQ mm/m64, mm (MMX)
                    insn::simd::movq_mm_m64_mm(self, ctx)
                }
            }

            // CMPPS/CMPPD/CMPSS/CMPSD (0xC2)
            0xC2 => self.execute_cmpps(ctx),

            _ => Err(Error::Emulator(format!(
                "unimplemented 0x0F opcode: {:#04x} at RIP={:#x}",
                opcode2, self.regs.rip
            ))),
        }
    }

    /// Execute 0x0F 0x38 opcodes (3-byte escape)
    pub(in crate::backend::emulator::x86_64) fn execute_0f38(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let opcode3 = ctx.consume_u8()?;

        match opcode3 {
            // MOVBE r, m16/32/64 (load with byte swap)
            0xF0 => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                if !is_memory {
                    return Err(Error::Emulator("MOVBE requires memory operand".to_string()));
                }
                let size = ctx.op_size;
                let value = self.read_mem(addr, size)?;
                // Byte swap based on operand size
                let swapped = match size {
                    2 => (value as u16).swap_bytes() as u64,
                    4 => (value as u32).swap_bytes() as u64,
                    8 => value.swap_bytes(),
                    _ => value,
                };
                self.set_reg(reg, swapped, size);
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            // MOVBE m16/32/64, r (store with byte swap)
            0xF1 => {
                let (reg, rm, is_memory, addr, _) = self.decode_modrm(ctx)?;
                if !is_memory {
                    return Err(Error::Emulator("MOVBE requires memory operand".to_string()));
                }
                let size = ctx.op_size;
                let value = self.get_reg(reg, size);
                // Byte swap based on operand size
                let swapped = match size {
                    2 => (value as u16).swap_bytes() as u64,
                    4 => (value as u32).swap_bytes() as u64,
                    8 => value.swap_bytes(),
                    _ => value,
                };
                self.write_mem(addr, swapped, size)?;
                self.regs.rip += ctx.cursor as u64;
                Ok(None)
            }
            _ => Err(Error::Emulator(format!(
                "unimplemented 0x0F 0x38 opcode: {:#04x} at RIP={:#x}",
                opcode3, self.regs.rip
            ))),
        }
    }

    /// Execute 0x0F 0x01 opcodes (Group 7 + special instructions)
    pub(in crate::backend::emulator::x86_64) fn execute_0f01(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        // Peek at modrm to determine instruction
        let modrm = ctx.peek_u8()?;

        // Check for special instructions with mod=3
        if modrm >> 6 == 3 {
            match modrm {
                0xD0 => {
                    // XGETBV (0x0F 0x01 0xD0) - Get extended control register
                    ctx.consume_u8()?; // consume modrm
                                       // ECX specifies which XCR (only XCR0 is typically supported)
                                       // Returns XCR value in EDX:EAX (zero-extended in 64-bit mode)
                                       // For XCR0, return x87 bit set (bit 0) and SSE bit (bit 1)
                    let xcr0 = 0x03u64; // x87 + SSE always enabled
                                        // In 64-bit mode, writes to EAX/EDX zero-extend to RAX/RDX
                    self.regs.rax = xcr0 & 0xFFFFFFFF;
                    self.regs.rdx = (xcr0 >> 32) & 0xFFFFFFFF;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD1 => {
                    // XSETBV (0x0F 0x01 0xD1) - Set extended control register
                    ctx.consume_u8()?; // consume modrm
                                       // In emulator, just NOP (we ignore the write)
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD5 => {
                    // XEND (0x0F 0x01 0xD5) - End transaction
                    ctx.consume_u8()?; // consume modrm
                                       // TSX not supported, treat as NOP
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xD6 => {
                    // XTEST (0x0F 0x01 0xD6) - Test if in transactional execution
                    ctx.consume_u8()?; // consume modrm
                                       // TSX not supported, ZF=1 (not in transaction)
                    self.regs.rflags |= flags::bits::ZF;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                0xF9 => {
                    // RDTSCP (0x0F 0x01 0xF9)
                    ctx.consume_u8()?; // consume modrm
                    insn::system::rdtscp(self, ctx)
                }
                _ => insn::system::group7(self, ctx),
            }
        } else {
            insn::system::group7(self, ctx)
        }
    }

    /// Execute 0x0F 0xAE opcodes (Group 15 - fences, CLFLUSH, etc.)
    pub(in crate::backend::emulator::x86_64) fn execute_0fae(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let modrm = ctx.consume_u8()?;
        let reg_op = (modrm >> 3) & 0x07;

        // Memory fences (mod=3, specific reg values)
        if modrm >> 6 == 3 {
            match reg_op {
                5 => insn::system::lfence(self, ctx), // LFENCE (E8-EF)
                6 => insn::system::mfence(self, ctx), // MFENCE (F0-F7)
                7 => insn::system::sfence(self, ctx), // SFENCE (F8-FF)
                _ => {
                    return Err(Error::Emulator(format!(
                        "unimplemented 0F AE /{} (mod=3) at RIP={:#x}",
                        reg_op, self.regs.rip
                    )));
                }
            }
        } else {
            // Memory operand forms (FXSAVE, FXRSTOR, LDMXCSR, STMXCSR, XSAVE, XRSTOR, CLFLUSH)
            let modrm_start = ctx.cursor - 1;
            let (addr, extra) = self.decode_modrm_addr(ctx, modrm_start)?;
            ctx.cursor = modrm_start + 1 + extra;

            match reg_op {
                0 => {
                    // FXSAVE - save FPU/SSE state (512 bytes)
                    // Zero the area first
                    for i in 0..64 {
                        self.write_mem(addr + i * 8, 0u64, 8)?;
                    }
                    // FCW at offset 0
                    self.write_mem16(addr, self.fpu.control_word)?;
                    // FSW at offset 2
                    self.write_mem16(addr + 2, self.fpu.status_word)?;
                    // Abridged FTW at offset 4 (1 byte, 1 bit per register)
                    let mut abtw = 0u8;
                    for i in 0..8 {
                        let tag = (self.fpu.tag_word >> (i * 2)) & 3;
                        if tag != 3 {
                            abtw |= 1 << i;
                        }
                    }
                    self.mmu.write_u8(addr + 4, abtw, &self.sregs)?;
                    // FOP at offset 6
                    self.write_mem16(addr + 6, self.fpu.last_opcode)?;
                    // FIP at offset 8 (8 bytes in 64-bit mode)
                    self.write_mem64(addr + 8, self.fpu.instr_ptr)?;
                    // FDP at offset 16 (8 bytes in 64-bit mode)
                    self.write_mem64(addr + 16, self.fpu.data_ptr)?;
                    // MXCSR at offset 24
                    self.write_mem32(addr + 24, 0x1F80)?;
                    // MXCSR_MASK at offset 28
                    self.write_mem32(addr + 28, 0xFFFF)?;
                    // ST0-ST7 at offset 32 (16 bytes each)
                    for i in 0..8 {
                        let bytes = insn::fpu::f64_to_f80_pub(self.fpu.st[i]);
                        self.write_bytes(addr + 32 + (i as u64) * 16, &bytes)?;
                    }
                    // XMM0-XMM15 at offset 160 (16 bytes each)
                    for i in 0..16 {
                        let xmm = self.regs.xmm[i];
                        self.write_mem64(addr + 160 + (i as u64) * 16, xmm[0])?;
                        self.write_mem64(addr + 160 + (i as u64) * 16 + 8, xmm[1])?;
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                1 => {
                    // FXRSTOR - restore FPU/SSE state (512 bytes)
                    // FCW at offset 0
                    self.fpu.control_word = self.read_mem16(addr)?;
                    // FSW at offset 2
                    self.fpu.status_word = self.read_mem16(addr + 2)?;
                    self.fpu.top = ((self.fpu.status_word >> 11) & 7) as u8;
                    // Abridged FTW at offset 4
                    let abtw = self.mmu.read_u8(addr + 4, &self.sregs)?;
                    self.fpu.tag_word = 0;
                    for i in 0..8 {
                        if abtw & (1 << i) != 0 {
                            self.fpu.tag_word |= 0 << (i * 2); // Valid
                        } else {
                            self.fpu.tag_word |= 3 << (i * 2); // Empty
                        }
                    }
                    // FOP at offset 6
                    self.fpu.last_opcode = self.read_mem16(addr + 6)?;
                    // FIP at offset 8
                    self.fpu.instr_ptr = self.read_mem64(addr + 8)?;
                    // FDP at offset 16
                    self.fpu.data_ptr = self.read_mem64(addr + 16)?;
                    // ST0-ST7 at offset 32
                    for i in 0..8 {
                        let bytes = self.read_bytes(addr + 32 + (i as u64) * 16, 10)?;
                        self.fpu.st[i] = insn::fpu::f80_to_f64_pub(&bytes);
                    }
                    // XMM0-XMM15 at offset 160
                    for i in 0..16 {
                        self.regs.xmm[i][0] = self.read_mem64(addr + 160 + (i as u64) * 16)?;
                        self.regs.xmm[i][1] =
                            self.read_mem64(addr + 160 + (i as u64) * 16 + 8)?;
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                2 => {
                    // LDMXCSR - load MXCSR register from memory
                    // Just skip - treat as NOP
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                3 => {
                    // STMXCSR - store MXCSR register to memory
                    // Store default MXCSR value (0x1F80)
                    self.write_mem(addr, 0x1F80u64, 4)?;
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                4 => {
                    // XSAVE - save extended processor state
                    // EDX:EAX specifies which components to save
                    // Write minimal XSAVE area header (64 bytes)
                    // XSTATE_BV at offset 0 (8 bytes) - saved components
                    let xcr0 = 0x03u64; // x87 + SSE
                    self.write_mem(addr + 512, xcr0, 8)?; // XSTATE_BV in header
                    self.write_mem(addr + 520, 0u64, 8)?; // XCOMP_BV
                                                           // Zero rest of legacy region
                    for i in 0..64 {
                        self.write_mem(addr + i * 8, 0u64, 8)?;
                    }
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                5 => {
                    // XRSTOR - restore extended processor state
                    // Just skip - we don't actually restore state
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                7 => {
                    // CLFLUSH/CLFLUSHOPT - treat as NOP
                    self.regs.rip += ctx.cursor as u64;
                    Ok(None)
                }
                _ => {
                    return Err(Error::Emulator(format!(
                        "unimplemented 0F AE /{} at RIP={:#x}",
                        reg_op, self.regs.rip
                    )));
                }
            }
        }
    }

    /// SSE packed single/double add (0x58)
    fn execute_sse_add(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_sub(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_mul(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_div(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_sqrt(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_min(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_max(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_unpcklps(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_sse_unpckhps(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
    fn execute_pshufd(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
            // PSHUFW: shuffle MMX words (NP 0F 70) - not commonly used in 64-bit
            return Err(Error::Emulator("PSHUFW (MMX) not implemented".to_string()));
        }
        self.regs.rip += ctx.cursor as u64;
        Ok(None)
    }

    /// SSE CMPPS/CMPPD/CMPSS/CMPSD (0x0F 0xC2)
    fn execute_cmpps(&mut self, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
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
