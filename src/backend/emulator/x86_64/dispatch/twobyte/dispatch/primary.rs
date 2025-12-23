//! Two-byte opcode instruction implementation for x86_64 emulator.

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::super::aes;
use super::super::super::super::cpu::{InsnContext, X86_64Vcpu};
use super::super::super::super::flags;
use super::super::super::super::insn;

impl X86_64Vcpu {
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
            0x06 => insn::system::clts(self, ctx),
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
            0x10 => insn::simd::movups_load(self, ctx),
            0x11 => insn::simd::movups_store(self, ctx),
            0x12 => insn::simd::movlps_load(self, ctx),
            0x13 => insn::simd::movlps_store(self, ctx),
            0x16 => insn::simd::movhps_load(self, ctx),
            0x17 => insn::simd::movhps_store(self, ctx),
            0x28 => insn::simd::movaps_load(self, ctx),
            0x29 => insn::simd::movaps_store(self, ctx),

            // SSE logical operations
            0x54 => insn::simd::andps(self, ctx),
            0x55 => insn::simd::andnps(self, ctx),
            0x56 => insn::simd::orps(self, ctx),
            0x57 => insn::simd::xorps(self, ctx),

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

            // 0F 3A escape - PEXTR*, PINSR*, ROUND*, etc.
            0x3A => self.execute_0f3a(ctx),

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
}
