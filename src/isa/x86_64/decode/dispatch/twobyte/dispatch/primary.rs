//! Two-byte opcode instruction implementation for x86_64 emulator.

use crate::error::{Error, Result};
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute;
use crate::isa::x86_64::flags;

#[inline]
fn is_legacy_0f_simd_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        0x10..=0x17
            | 0x28..=0x2F
            | 0x50..=0x77
            | 0x78..=0x79
            | 0x7C..=0x7F
            | 0xC2
            | 0xC4..=0xC6
            | 0xD0..=0xFE
    )
}

impl X86_64Vcpu {
    #[inline(always)]
    pub(in crate::isa::x86_64) fn execute_0f(
        &mut self,
        ctx: &mut InsnContext,
    ) -> Result<Option<VcpuExit>> {
        let opcode2 = ctx.consume_u8()?;
        // Operand-free EMMS is valid with REX2. Other legacy SIMD forms remain
        // fail-closed until their vector-register and memory EGPR extensions
        // are decoded distinctly.
        if is_legacy_0f_simd_opcode(opcode2)
            && opcode2 != 0x77
            && self.reject_rex2_for_legacy_simd(ctx)?
        {
            return Ok(None);
        }

        // Record precise opcode key for profiling
        #[cfg(feature = "profiling")]
        crate::observability::profiling::set_current_opcode_key(
            crate::observability::profiling::OpcodeKey::TwoByte(opcode2),
        );

        match opcode2 {
            // System
            0x00 => execute::system::group6(self, ctx),
            0x01 => self.execute_0f01(ctx),
            0x02 => execute::system::lar(self, ctx),
            0x03 => execute::system::lsl(self, ctx),
            0x05 => execute::system::syscall(self, ctx),
            0x06 => execute::system::clts(self, ctx),
            0x07 => execute::system::sysret(self, ctx),
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
            // UD2 - Undefined Instruction (intentional #UD exception)
            0x0B => {
                // Inject #UD so the guest's own handler runs. The kernel encodes
                // WARN()/BUG() as UD2 + a __bug_table entry: do_invalid_op /
                // report_bug then either prints a warning and RESUMES (WARN) or
                // panics (BUG). A genuinely-unreachable UD2 (reached only via a
                // mis-emulated branch) faults as "invalid opcode", which is far
                // more diagnosable than silently skipping it into garbage.
                // (An earlier "skip the first N kernel-text UD2s" workaround
                // masked real emulation bugs and corrupted control flow — removed.)
                // #UD is a fault: RIP stays on the faulting instruction.
                self.inject_exception(6, None)?; // #UD = vector 6
                Ok(None)
            }
            0x20 => execute::system::mov_r_cr(self, ctx),
            0x21 => execute::system::mov_r_dr(self, ctx),
            0x22 => execute::system::mov_cr_r(self, ctx),
            0x23 => execute::system::mov_dr_r(self, ctx),
            0x30 => execute::system::wrmsr(self, ctx),
            0x31 => execute::system::rdtsc(self, ctx),
            0x32 => execute::system::rdmsr(self, ctx),
            0x33 => execute::system::rdpmc(self, ctx),
            0x34 => execute::system::sysenter(self, ctx),
            0x35 => execute::system::sysexit(self, ctx),
            0xA0 => execute::data::push_sreg(self, ctx, 4), // PUSH FS
            0xA1 => execute::data::pop_sreg(self, ctx, 4),  // POP FS
            0xA8 => execute::data::push_sreg(self, ctx, 5), // PUSH GS
            0xA9 => execute::data::pop_sreg(self, ctx, 5),  // POP GS
            0xAA => {
                // RSM is only valid while resuming from SMM; the emulator does
                // not expose SMM state, so normal execution receives #UD.
                self.inject_undefined_instruction()
            }
            0xA2 => execute::system::cpuid(self, ctx),
            0xAE => self.execute_0fae(ctx),

            // Control flow
            0x40..=0x4F => execute::control::cmovcc(self, ctx, opcode2 & 0x0F),
            0x80..=0x8F => execute::control::jcc_rel32(self, ctx, opcode2 & 0x0F),
            0x90..=0x9F => execute::control::setcc(self, ctx, opcode2 & 0x0F),

            // Data movement
            0xB2 => execute::data::lss(self, ctx),
            0xB4 => execute::data::lfs(self, ctx),
            0xB5 => execute::data::lgs(self, ctx),
            0xB6 => execute::data::movzx_r_rm8(self, ctx),
            0xB7 => execute::data::movzx_r_rm16(self, ctx),
            0xBE => execute::data::movsx_r_rm8(self, ctx),
            0xBF => execute::data::movsx_r_rm16(self, ctx),
            0xC8..=0xCF => execute::data::bswap(self, ctx, opcode2),

            // Arithmetic
            0xAF => execute::arith::imul_r_rm(self, ctx),

            // Bit manipulation
            0xA3 => execute::bit::bt_rm_r(self, ctx),
            0xAB => execute::bit::bts_rm_r(self, ctx),
            0xB3 => execute::bit::btr_rm_r(self, ctx),
            0xBB => execute::bit::btc_rm_r(self, ctx),
            0xBA => execute::bit::group8(self, ctx),
            0xB8 => execute::bit::popcnt(self, ctx),
            // BSF/TZCNT and BSR/LZCNT share opcodes - F3 prefix differentiates
            0xBC => {
                if ctx.rep_prefix == Some(0xF3) {
                    execute::bit::tzcnt(self, ctx)
                } else {
                    execute::bit::bsf(self, ctx)
                }
            }
            0xBD => {
                if ctx.rep_prefix == Some(0xF3) {
                    execute::bit::lzcnt(self, ctx)
                } else {
                    execute::bit::bsr(self, ctx)
                }
            }

            // CMPXCHG
            0xB0 => execute::data::cmpxchg_rm8_r8(self, ctx),
            0xB1 => execute::data::cmpxchg_rm_r(self, ctx),
            // UD1 - Undefined Instruction (intentional #UD exception with ModRM)
            0xB9 => {
                // UD1 has a ModR/M byte but always generates #UD
                // Don't advance RIP - #UD is a fault, exception points to faulting instruction
                let _modrm = ctx.consume_u8()?;
                self.inject_exception(6, None)?; // #UD = vector 6
                Ok(None)
            }

            // XADD
            0xC0 => execute::data::xadd_rm8_r8(self, ctx),
            0xC1 => execute::data::xadd_rm_r(self, ctx),

            // SHLD/SHRD
            0xA4 => execute::shift::shld_imm8(self, ctx),
            0xA5 => execute::shift::shld_cl(self, ctx),
            0xAC => execute::shift::shrd_imm8(self, ctx),
            0xAD => execute::shift::shrd_cl(self, ctx),

            // Reserved NOP variants
            0x19 | 0x1A | 0x1B | 0x1D => execute::system::nop_rm(self, ctx),
            0x1C => execute::system::cldemote(self, ctx),
            0x1E => execute::system::endbr(self, ctx),
            0x1F => execute::system::nop_rm(self, ctx),

            // Prefetch hints
            0x0D => execute::simd::prefetchw(self, ctx),
            0x18 => execute::simd::prefetchh(self, ctx),

            // MOVUPS/MOVUPD (0x10/0x11 unaligned), MOVAPS/MOVAPD (0x28/0x29 aligned)
            0x10 => execute::simd::movups_load(self, ctx),
            0x11 => execute::simd::movups_store(self, ctx),
            0x12 => {
                if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 12: MOVDDUP xmm1, xmm2/m64
                    execute::simd::movddup(self, ctx)
                } else if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 12: MOVSLDUP xmm1, xmm2/m128
                    execute::simd::movsldup(self, ctx)
                } else {
                    // NP/66 0F 12: MOVLPS/MOVHLPS xmm, m64/xmm
                    execute::simd::movlps_load(self, ctx)
                }
            }
            0x13 => execute::simd::movlps_store(self, ctx),
            0x16 => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 16: MOVSHDUP xmm1, xmm2/m128
                    execute::simd::movshdup(self, ctx)
                } else {
                    // NP/66 0F 16: MOVHPS/MOVLHPS xmm, m64/xmm
                    execute::simd::movhps_load(self, ctx)
                }
            }
            0x17 => execute::simd::movhps_store(self, ctx),
            0x28 => execute::simd::movaps_load(self, ctx),
            0x29 => execute::simd::movaps_store(self, ctx),

            // SSE logical operations
            0x54 => execute::simd::andps(self, ctx),
            0x55 => execute::simd::andnps(self, ctx),
            0x56 => execute::simd::orps(self, ctx),
            0x57 => execute::simd::xorps(self, ctx),

            // MOVMSKPS/MOVMSKPD - extract sign bits
            0x50 => self.execute_movmsk(ctx),

            // SSE arithmetic
            0x51 => self.execute_sse_sqrt(ctx),
            0x52 => self.execute_sse_rsqrt(ctx),
            0x53 => self.execute_sse_rcp(ctx),
            0x58 => self.execute_sse_add(ctx),
            0x59 => self.execute_sse_mul(ctx),
            0x5C => self.execute_sse_sub(ctx),
            0x5D => self.execute_sse_min(ctx),
            0x5E => self.execute_sse_div(ctx),
            0x5F => self.execute_sse_max(ctx),

            // SSE unpack
            0x14 => self.execute_sse_unpcklps(ctx),
            0x15 => self.execute_sse_unpckhps(ctx),
            // SSE2/MMX integer unpack
            0x60 | 0x61 | 0x62 | 0x68 | 0x69 | 0x6A | 0x6C | 0x6D => {
                self.execute_punpck(ctx, opcode2)
            }

            // MOVD/MOVQ
            0x6E => {
                if ctx.operand_size_override {
                    // 66 0F 6E: MOVD/MOVQ xmm, r/m32 (or r/m64 with REX.W)
                    execute::simd::movd_xmm_rm(self, ctx)
                } else {
                    // NP 0F 6E: MOVD/MOVQ mm, r/m32 (or r/m64 with REX.W)
                    execute::simd::movd_mm_rm(self, ctx)
                }
            }
            0x7E => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 7E: MOVQ xmm1, xmm2/m64
                    execute::simd::movq_xmm_xmm_m64(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 7E: MOVD/MOVQ r/m32, xmm (or r/m64 with REX.W)
                    execute::simd::movd_rm_xmm(self, ctx)
                } else {
                    // NP 0F 7E: MOVD/MOVQ r/m32, mm (or r/m64 with REX.W)
                    execute::simd::movd_rm_mm(self, ctx)
                }
            }
            0xD6 => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F D6: MOVQ2DQ xmm, mm - move mm to low qword of xmm
                    execute::simd::movq2dq(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F D6: MOVDQ2Q mm, xmm - move low qword of xmm to mm
                    execute::simd::movdq2q(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F D6: MOVQ xmm2/m64, xmm1
                    execute::simd::movq_xmm_m64_xmm(self, ctx)
                } else {
                    self.inject_undefined_instruction()
                }
            }
            // Packed integer insert/extract
            0xD7 => execute::simd::pmovmskb(self, ctx),
            // Packed integer add (SSE2/MMX)
            0xD4 => execute::simd::paddq_packed(self, ctx),
            0xFC => execute::simd::paddb_packed(self, ctx),
            0xFD => execute::simd::paddw_packed(self, ctx),
            0xFE => execute::simd::paddd_packed(self, ctx),
            // Packed integer saturating add (SSE2/MMX)
            0xEC => execute::simd::paddsb_packed(self, ctx),
            0xED => execute::simd::paddsw_packed(self, ctx),
            0xDC => execute::simd::paddusb_packed(self, ctx),
            0xDD => execute::simd::paddusw_packed(self, ctx),
            // Packed integer subtract (SSE2)
            0xD8 | 0xD9 | 0xE8 | 0xE9 | 0xF8 | 0xF9 | 0xFA | 0xFB => {
                execute::simd::psub_packed(self, ctx, opcode2)
            }
            // Packed integer logical (SSE2/MMX)
            0xDB => execute::simd::pand(self, ctx),
            0xDF => execute::simd::pandn(self, ctx),
            0xEB => execute::simd::por(self, ctx),
            // Packed integer compare (SSE2/MMX)
            0x74 => execute::simd::pcmpeqb(self, ctx),
            0x75 => execute::simd::pcmpeqw(self, ctx),
            0x76 => execute::simd::pcmpeqd(self, ctx),
            0x64 => execute::simd::pcmpgtb(self, ctx),
            0x65 => execute::simd::pcmpgtw(self, ctx),
            0x66 => execute::simd::pcmpgtd(self, ctx),
            // MOVNTQ - non-temporal store MMX (0F E7)
            // MOVNTDQ - non-temporal store XMM (66 0F E7)
            0xE7 => {
                if ctx.operand_size_override {
                    self.execute_movnt_store(ctx)
                } else {
                    execute::simd::movntq(self, ctx)
                }
            }
            // Packed integer min/max (SSE2)
            0xDA => execute::simd::pminub(self, ctx),
            0xDE => execute::simd::pmaxub(self, ctx),
            0xEA => execute::simd::pminsw(self, ctx),
            0xEE => execute::simd::pmaxsw(self, ctx),

            // Packed integer multiply (SSE2/MMX)
            0xD5 => execute::simd::pmullw(self, ctx), // PMULLW
            0xE4 => execute::simd::pmulhuw(self, ctx), // PMULHUW
            0xE5 => execute::simd::pmulhw(self, ctx), // PMULHW
            0xF4 => execute::simd::pmuludq(self, ctx), // PMULUDQ
            0xF5 => execute::simd::pmaddwd(self, ctx), // PMADDWD

            // PXOR (SSE2) - XOR packed integers
            0xEF => execute::simd::pxor(self, ctx),

            // SSE/SSE2 Conversion Instructions
            0x5A => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 5A: CVTSS2SD xmm1, xmm2/m32
                    execute::simd::cvtss2sd(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 5A: CVTSD2SS xmm1, xmm2/m64
                    execute::simd::cvtsd2ss(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 5A: CVTPD2PS xmm1, xmm2/m128
                    execute::simd::cvtpd2ps(self, ctx)
                } else {
                    // NP 0F 5A: CVTPS2PD xmm1, xmm2/m64
                    execute::simd::cvtps2pd(self, ctx)
                }
            }
            0x5B => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 5B: CVTTPS2DQ xmm1, xmm2/m128
                    execute::simd::cvttps2dq(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 5B: CVTPS2DQ xmm1, xmm2/m128
                    execute::simd::cvtps2dq(self, ctx)
                } else {
                    // NP 0F 5B: CVTDQ2PS xmm1, xmm2/m128
                    execute::simd::cvtdq2ps(self, ctx)
                }
            }
            0x2A => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 2A: CVTSI2SS xmm1, r/m32 or r/m64
                    execute::simd::cvtsi2ss(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 2A: CVTSI2SD xmm1, r/m32 or r/m64
                    execute::simd::cvtsi2sd(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 2A: CVTPI2PD xmm, mm/m64
                    execute::simd::cvtpi2pd(self, ctx)
                } else {
                    // NP 0F 2A: CVTPI2PS xmm, mm/m64
                    execute::simd::cvtpi2ps(self, ctx)
                }
            }
            0x2C => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 2C: CVTTSS2SI r32/r64, xmm1/m32
                    execute::simd::cvttss2si(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 2C: CVTTSD2SI r32/r64, xmm1/m64
                    execute::simd::cvttsd2si(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 2C: CVTTPD2PI mm, xmm/m128
                    execute::simd::cvttpd2pi(self, ctx)
                } else {
                    // NP 0F 2C: CVTTPS2PI mm, xmm/m64
                    execute::simd::cvttps2pi(self, ctx)
                }
            }
            0x2D => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 2D: CVTSS2SI r32/r64, xmm1/m32
                    execute::simd::cvtss2si(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F 2D: CVTSD2SI r32/r64, xmm1/m64
                    execute::simd::cvtsd2si(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 2D: CVTPD2PI mm, xmm/m128
                    execute::simd::cvtpd2pi(self, ctx)
                } else {
                    // NP 0F 2D: CVTPS2PI mm, xmm/m64
                    execute::simd::cvtps2pi(self, ctx)
                }
            }
            // MOVNTSS/MOVNTSD - AMD SSE4A scalar non-temporal stores.
            0x2B if ctx.rep_prefix == Some(0xF3) => {
                execute::simd::execute_sse4a_movnt_store(self, ctx, 4)
            }
            0x2B if ctx.rep_prefix == Some(0xF2) => {
                execute::simd::execute_sse4a_movnt_store(self, ctx, 8)
            }
            // MOVNTPS/MOVNTPD - packed non-temporal stores.
            0x2B => self.execute_movnt_store(ctx),
            0x2E => execute::simd::ucomiss_ucomisd(self, ctx),
            // COMISS/COMISD - compare scalar and set EFLAGS
            0x2F => self.execute_comiss(ctx),
            // AMD SSE4A EXTRQ/INSERTQ register-only bitfield operations.
            0x78 | 0x79 => execute::simd::execute_sse4a_bitfield(self, ctx, opcode2),
            0xE6 => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F E6: CVTDQ2PD xmm1, xmm2/m64
                    execute::simd::cvtdq2pd(self, ctx)
                } else if ctx.rep_prefix == Some(0xF2) {
                    // F2 0F E6: CVTPD2DQ xmm1, xmm2/m128
                    execute::simd::cvtpd2dq(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F E6: CVTTPD2DQ xmm1, xmm2/m128
                    execute::simd::cvttpd2dq(self, ctx)
                } else {
                    self.inject_undefined_instruction()
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
                    execute::simd::movdqu_xmm_xmm_m128(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 6F: MOVDQA xmm, xmm/m128 (aligned)
                    execute::simd::movdqa_xmm_xmm_m128(self, ctx)
                } else {
                    // NP 0F 6F: MOVQ mm, mm/m64 (MMX)
                    execute::simd::movq_mm_mm_m64(self, ctx)
                }
            }

            // PSHUFD/PSHUFHW/PSHUFLW (0x70)
            0x70 => self.execute_pshufd(ctx),

            // MOVDQA/MOVDQU/MOVQ store (0x7F)
            0x7F => {
                if ctx.rep_prefix == Some(0xF3) {
                    // F3 0F 7F: MOVDQU xmm/m128, xmm (unaligned)
                    execute::simd::movdqu_xmm_m128_xmm(self, ctx)
                } else if ctx.operand_size_override {
                    // 66 0F 7F: MOVDQA xmm/m128, xmm (aligned)
                    execute::simd::movdqa_xmm_m128_xmm(self, ctx)
                } else {
                    // NP 0F 7F: MOVQ mm/m64, mm (MMX)
                    execute::simd::movq_mm_m64_mm(self, ctx)
                }
            }

            // SSE2/MMX shift immediate (groups 12, 13, 14)
            0x71 => self.execute_shift_imm_group12(ctx),
            0x72 => self.execute_shift_imm_group13(ctx),
            0x73 => self.execute_shift_imm_group14(ctx),

            // SSE3 horizontal add/sub
            0x7C => self.execute_hadd(ctx),
            0x7D => self.execute_hsub(ctx),

            // SSE3 ADDSUBPS/ADDSUBPD
            0xD0 => self.execute_addsubps(ctx),

            // CMPPS/CMPPD/CMPSS/CMPSD (0xC2)
            0xC2 => self.execute_cmpps(ctx),

            // SHUFPS/SHUFPD (0xC6)
            0xC6 => self.execute_shufps(ctx),

            // PINSRW (0xC4)
            0xC4 => self.execute_pinsrw(ctx),

            // PEXTRW (0xC5)
            0xC5 => self.execute_pextrw(ctx),

            // MOVNTI - non-temporal store (0xC3)
            0xC3 => execute::simd::movnti(self, ctx),

            // PSADBW - sum of absolute differences (0xF6)
            0xF6 => execute::simd::psadbw(self, ctx),

            // MASKMOVDQU/MASKMOVQ (0xF7)
            0xF7 => execute::simd::maskmovdqu(self, ctx),

            // LDDQU (F2 0F F0)
            0xF0 if ctx.rep_prefix == Some(0xF2) => execute::simd::lddqu(self, ctx),

            // PAVGB/PAVGW - packed average (0xE0/0xE3)
            0xE0 => execute::simd::pavgb(self, ctx),
            0xE3 => execute::simd::pavgw(self, ctx),

            // PACKSSWB/PACKSSDW - pack with saturation (0x63/0x6B)
            0x63 => execute::simd::packsswb(self, ctx),
            0x6B => execute::simd::packssdw(self, ctx),

            // PACKUSWB - pack unsigned with saturation (0x67)
            0x67 => execute::simd::packuswb(self, ctx),

            // Packed integer shift by XMM count
            // PSRLW/PSRLD/PSRLQ xmm, xmm/m128 (66 0F D1/D2/D3)
            0xD1 => execute::simd::packed_shift_xmm_count(self, ctx, opcode2),
            0xD2 => execute::simd::packed_shift_xmm_count(self, ctx, opcode2),
            0xD3 => execute::simd::packed_shift_xmm_count(self, ctx, opcode2),
            // PSRAW/PSRAD xmm, xmm/m128 (66 0F E1/E2)
            0xE1 => execute::simd::packed_shift_xmm_count(self, ctx, opcode2),
            0xE2 => execute::simd::packed_shift_xmm_count(self, ctx, opcode2),
            // PSLLW/PSLLD/PSLLQ xmm, xmm/m128 (66 0F F1/F2/F3)
            0xF1 => execute::simd::packed_shift_xmm_count(self, ctx, opcode2),
            0xF2 if ctx.rep_prefix.is_none() => {
                execute::simd::packed_shift_xmm_count(self, ctx, opcode2)
            }
            0xF3 if ctx.rep_prefix.is_none() => {
                execute::simd::packed_shift_xmm_count(self, ctx, opcode2)
            }

            // EMMS - Empty MMX State (0F 77)
            0x77 => execute::simd::emms(self, ctx),

            // 0F C7 - Group 9: CMPXCHG8B/16B, RDRAND, RDSEED, etc.
            0xC7 => self.execute_group9(ctx),

            _ => self.inject_undefined_instruction(),
        }
    }
}
