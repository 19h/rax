//! map0f.rs

use crate::error::{Error, Result};
use crate::isa::x86_64::decode::dispatch::evex::*;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::{execute, flags};

impl X86_64Vcpu {
    /// EVEX 0F opcode map (mm=1)
    pub(crate) fn execute_evex_0f(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;

        if opcode == 0x01 && ctx.peek_u8()? == 0xC5 {
            // PCONFIG has no EVEX encoding. Consume the fixed ModR/M byte so
            // instruction fetch remains precise, then deliver architectural
            // #UD instead of leaking the generic unimplemented-opcode error.
            ctx.consume_u8()?;
            return self.inject_undefined_instruction();
        }

        match opcode {
            // VMOVUPS/VMOVAPS load (0x10/0x28): ps lanes, masked. Aligned for 0x28.
            0x10 | 0x28 if evex.pp == 0 => {
                execute::simd::evex_mov_masked_load(self, ctx, 4, opcode == 0x28)
            }
            // VMOVUPD/VMOVAPD load (0x10/0x28 with 66 prefix): pd lanes, masked.
            0x10 | 0x28 if evex.pp == 1 => {
                execute::simd::evex_mov_masked_load(self, ctx, 8, opcode == 0x28)
            }
            // VMOVSS/VMOVSD scalar load/reg-reg move forms.
            0x10 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_scalar_fp_move(self, ctx, 4, false)
            }
            0x10 if evex.pp == 3 && evex.w => {
                execute::simd::evex_scalar_fp_move(self, ctx, 8, false)
            }
            // VMOVUPS/VMOVAPS store (0x11/0x29): ps lanes, masked. Aligned for 0x29.
            0x11 | 0x29 if evex.pp == 0 => {
                execute::simd::evex_mov_masked_store(self, ctx, 4, opcode == 0x29)
            }
            // VMOVUPD/VMOVAPD store (0x11/0x29 with 66 prefix): pd lanes, masked.
            0x11 | 0x29 if evex.pp == 1 => {
                execute::simd::evex_mov_masked_store(self, ctx, 8, opcode == 0x29)
            }
            // VMOVSS/VMOVSD scalar store/reg-reg move forms.
            0x11 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_scalar_fp_move(self, ctx, 4, true)
            }
            0x11 if evex.pp == 3 && evex.w => {
                execute::simd::evex_scalar_fp_move(self, ctx, 8, true)
            }
            // VMOVLPS/VMOVHLPS and VMOVHPS/VMOVLHPS.
            0x12 if evex.pp == 0 && !evex.w => {
                execute::simd::evex_high_low_move(self, ctx, false, true)
            }
            0x16 if evex.pp == 0 && !evex.w => {
                execute::simd::evex_high_low_move(self, ctx, true, true)
            }
            // VMOVLPD and VMOVHPD.
            0x12 if evex.pp == 1 && evex.w => {
                execute::simd::evex_high_low_move(self, ctx, false, false)
            }
            0x16 if evex.pp == 1 && evex.w => {
                execute::simd::evex_high_low_move(self, ctx, true, false)
            }
            // VMOVSLDUP/VMOVDDUP and VMOVSHDUP.
            0x12 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_duplicate_lanes(self, ctx, 4, false)
            }
            0x12 if evex.pp == 3 && evex.w => {
                execute::simd::evex_duplicate_lanes(self, ctx, 8, false)
            }
            0x16 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_duplicate_lanes(self, ctx, 4, true)
            }
            // VMOVNTPS (NP.0F.2B) and VMOVNTPD (66.0F.W1.2B) memory stores.
            0x2B if (evex.pp == 0 && !evex.w) || (evex.pp == 1 && evex.w) => {
                execute::simd::evex_nt_store(self, ctx)
            }
            // VMOVD/VMOVQ: GPR/memory to XMM.
            0x6E if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_gpr_or_mem_to_xmm(self, ctx, es)
            }
            // VMOVD/VMOVQ: XMM to GPR/memory.
            0x7E if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_xmm_to_gpr_or_mem(self, ctx, es)
            }
            // VMOVQ: XMM/m64 to XMM.
            0x7E if evex.pp == 2 && evex.w => execute::simd::evex_movq_vec_load(self, ctx),
            // VMOVQ: XMM to XMM/m64.
            0xD6 if evex.pp == 1 && evex.w => execute::simd::evex_movq_vec_store(self, ctx),
            // VMOVNTDQ (66.0F.E7) memory store.
            0xE7 if evex.pp == 1 && !evex.w => execute::simd::evex_nt_store(self, ctx),
            // VUCOMISS/VUCOMISD and VCOMISS/VCOMISD: scalar compare into RFLAGS.
            0x2E if evex.pp == 0 && !evex.w => execute::simd::evex_comi(self, ctx, 4, false),
            0x2E if evex.pp == 1 && evex.w => execute::simd::evex_comi(self, ctx, 8, false),
            0x2F if evex.pp == 0 && !evex.w => execute::simd::evex_comi(self, ctx, 4, true),
            0x2F if evex.pp == 1 && evex.w => execute::simd::evex_comi(self, ctx, 8, true),
            // Scalar FP/integer conversions.
            0x2A if evex.pp == 2 => execute::simd::evex_gpr_to_fp(self, ctx, 4, false),
            0x2A if evex.pp == 3 => execute::simd::evex_gpr_to_fp(self, ctx, 8, false),
            0x2C if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 4, false, true),
            0x2C if evex.pp == 3 => execute::simd::evex_fp_to_gpr(self, ctx, 8, false, true),
            0x2D if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 4, false, false),
            0x2D if evex.pp == 3 => execute::simd::evex_fp_to_gpr(self, ctx, 8, false, false),
            0x5A if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_convert(self, ctx, 4, 8)
            }
            0x5A if evex.pp == 1 && evex.w => {
                execute::simd::evex_packed_fp_convert(self, ctx, 8, 4)
            }
            0x5B if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 4, 4, true)
            }
            0x5B if evex.pp == 0 && evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 8, 4, true)
            }
            0x5B if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 4, false, false)
            }
            0x5B if evex.pp == 2 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 4, false, true)
            }
            0x5A if evex.pp == 2 && !evex.w => {
                execute::simd::evex_fp_scalar_convert(self, ctx, 4, 8)
            }
            0x5A if evex.pp == 3 && evex.w => {
                execute::simd::evex_fp_scalar_convert(self, ctx, 8, 4)
            }
            0x78 if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 4, true, true)
            }
            0x78 if evex.pp == 0 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 4, true, true)
            }
            0x78 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 8, true, true)
            }
            0x78 if evex.pp == 1 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 8, true, true)
            }
            0x78 if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 4, true, true),
            0x78 if evex.pp == 3 => execute::simd::evex_fp_to_gpr(self, ctx, 8, true, true),
            0x79 if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 4, true, false)
            }
            0x79 if evex.pp == 0 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 4, true, false)
            }
            0x79 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 8, true, false)
            }
            0x79 if evex.pp == 1 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 8, true, false)
            }
            0x79 if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 4, true, false),
            0x79 if evex.pp == 3 => execute::simd::evex_fp_to_gpr(self, ctx, 8, true, false),
            0x7A if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 8, false, true)
            }
            0x7A if evex.pp == 1 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 8, false, true)
            }
            0x7A if evex.pp == 2 && !evex.w => {
                // F3.0F.W0 7A = VCVTUDQ2PD: u32 -> f64
                execute::simd::evex_packed_int_to_fp(self, ctx, 4, 8, false)
            }
            0x7A if evex.pp == 2 && evex.w => {
                // F3.0F.W1 7A = VCVTUQQ2PD: u64 -> f64
                execute::simd::evex_packed_int_to_fp(self, ctx, 8, 8, false)
            }
            0x7A if evex.pp == 3 && !evex.w => {
                // F2.0F.W0 7A = VCVTUDQ2PS: u32 -> f32
                execute::simd::evex_packed_int_to_fp(self, ctx, 4, 4, false)
            }
            0x7A if evex.pp == 3 && evex.w => {
                // F2.0F.W1 7A = VCVTUQQ2PS: u64 -> f32
                execute::simd::evex_packed_int_to_fp(self, ctx, 8, 4, false)
            }
            0x7B if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 4, 8, false, false)
            }
            0x7B if evex.pp == 1 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 8, false, false)
            }
            0x7B if evex.pp == 2 => execute::simd::evex_gpr_to_fp(self, ctx, 4, true),
            0x7B if evex.pp == 3 => execute::simd::evex_gpr_to_fp(self, ctx, 8, true),
            0xE6 if evex.pp == 1 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 4, false, true)
            }
            0xE6 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 4, 8, true)
            }
            0xE6 if evex.pp == 2 && evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 8, 8, true)
            }
            0xE6 if evex.pp == 3 && evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 8, 4, false, false)
            }
            // VADDSS/VADDSD scalar forms. These must be matched before packed PS/PD.
            0x58 if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp_scalar_arith_f32(ctx, |a, b| a + b)
            }
            0x58 if evex.pp == 3 && evex.w => {
                self.execute_evex_fp_scalar_arith_f64(ctx, |a, b| a + b)
            }
            // VADDPS (pp=0/W=0) / VADDPD (pp=1/W=1) (0x58)
            0x58 if evex.pp == 1 || evex.w => self.execute_evex_fp_arith_pd(ctx, |a, b| a + b),
            0x58 => self.execute_evex_fp_arith_ps(ctx, |a, b| a + b),
            // VMULSS/VMULSD scalar forms.
            0x59 if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp_scalar_arith_f32(ctx, |a, b| a * b)
            }
            0x59 if evex.pp == 3 && evex.w => {
                self.execute_evex_fp_scalar_arith_f64(ctx, |a, b| a * b)
            }
            // VMULPS / VMULPD (0x59)
            0x59 if evex.pp == 1 || evex.w => self.execute_evex_fp_arith_pd(ctx, |a, b| a * b),
            0x59 => self.execute_evex_fp_arith_ps(ctx, |a, b| a * b),
            // VSUBSS/VSUBSD scalar forms.
            0x5C if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp_scalar_arith_f32(ctx, |a, b| a - b)
            }
            0x5C if evex.pp == 3 && evex.w => {
                self.execute_evex_fp_scalar_arith_f64(ctx, |a, b| a - b)
            }
            // VSUBPS / VSUBPD (0x5C)
            0x5C if evex.pp == 1 || evex.w => self.execute_evex_fp_arith_pd(ctx, |a, b| a - b),
            0x5C => self.execute_evex_fp_arith_ps(ctx, |a, b| a - b),
            // VDIVSS/VDIVSD scalar forms.
            0x5E if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp_scalar_arith_f32(ctx, |a, b| a / b)
            }
            0x5E if evex.pp == 3 && evex.w => {
                self.execute_evex_fp_scalar_arith_f64(ctx, |a, b| a / b)
            }
            // VDIVPS / VDIVPD (0x5E)
            0x5E if evex.pp == 1 || evex.w => self.execute_evex_fp_arith_pd(ctx, |a, b| a / b),
            0x5E => self.execute_evex_fp_arith_ps(ctx, |a, b| a / b),
            // VSQRTSS/VSQRTSD scalar forms.
            0x51 if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp_scalar_arith_f32(ctx, |_, b| b.sqrt())
            }
            0x51 if evex.pp == 3 && evex.w => {
                self.execute_evex_fp_scalar_arith_f64(ctx, |_, b| b.sqrt())
            }
            // VSQRTPS / VSQRTPD (0x51)
            0x51 if evex.pp == 1 && evex.w => self.execute_evex_fp_unary_pd(ctx, |a| a.sqrt()),
            0x51 if evex.pp == 0 && !evex.w => self.execute_evex_fp_unary_ps(ctx, |a| a.sqrt()),
            // VMINSS/VMINSD scalar forms.
            0x5D if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp_scalar_arith_f32(ctx, Self::x86_min_f32)
            }
            0x5D if evex.pp == 3 && evex.w => {
                self.execute_evex_fp_scalar_arith_f64(ctx, Self::x86_min_f64)
            }
            // VMINPS / VMINPD (0x5D)
            0x5D if evex.pp == 1 && evex.w => self.execute_evex_fp_arith_pd(ctx, Self::x86_min_f64),
            0x5D if evex.pp == 0 && !evex.w => {
                self.execute_evex_fp_arith_ps(ctx, Self::x86_min_f32)
            }
            // VMAXSS/VMAXSD scalar forms.
            0x5F if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp_scalar_arith_f32(ctx, Self::x86_max_f32)
            }
            0x5F if evex.pp == 3 && evex.w => {
                self.execute_evex_fp_scalar_arith_f64(ctx, Self::x86_max_f64)
            }
            // VMAXPS / VMAXPD (0x5F)
            0x5F if evex.pp == 1 && evex.w => self.execute_evex_fp_arith_pd(ctx, Self::x86_max_f64),
            0x5F if evex.pp == 0 && !evex.w => {
                self.execute_evex_fp_arith_ps(ctx, Self::x86_max_f32)
            }
            // VANDPS/VANDPD, VANDNPS/VANDNPD, VORPS/VORPD, VXORPS/VXORPD.
            0x54 if evex.pp == 0 && !evex.w => self.execute_evex_fp_bitwise(ctx, 4, |a, b| a & b),
            0x54 if evex.pp == 1 && evex.w => self.execute_evex_fp_bitwise(ctx, 8, |a, b| a & b),
            0x55 if evex.pp == 0 && !evex.w => {
                self.execute_evex_fp_bitwise(ctx, 4, |a, b| (!a) & b)
            }
            0x55 if evex.pp == 1 && evex.w => self.execute_evex_fp_bitwise(ctx, 8, |a, b| (!a) & b),
            0x56 if evex.pp == 0 && !evex.w => self.execute_evex_fp_bitwise(ctx, 4, |a, b| a | b),
            0x56 if evex.pp == 1 && evex.w => self.execute_evex_fp_bitwise(ctx, 8, |a, b| a | b),
            0x57 if evex.pp == 0 && !evex.w => self.execute_evex_fp_bitwise(ctx, 4, |a, b| a ^ b),
            0x57 if evex.pp == 1 && evex.w => self.execute_evex_fp_bitwise(ctx, 8, |a, b| a ^ b),
            // VUNPCKLPS/PD and VUNPCKHPS/PD.
            0x14 if evex.pp == 0 && !evex.w => execute::simd::evex_unpack(self, ctx, 4, false),
            0x15 if evex.pp == 0 && !evex.w => execute::simd::evex_unpack(self, ctx, 4, true),
            0x14 if evex.pp == 1 && evex.w => execute::simd::evex_unpack(self, ctx, 8, false),
            0x15 if evex.pp == 1 && evex.w => execute::simd::evex_unpack(self, ctx, 8, true),
            // VPINSRW and VPEXTRW register-destination form.
            0xC4 if evex.pp == 1 => execute::simd::evex_pinsr(self, ctx, 2),
            0xC5 if evex.pp == 1 => execute::simd::evex_extract_word_rm_src(self, ctx),

            // ================================================================
            // Broadened EVEX coverage: integer/logical/compare/move/broadcast/shift
            // All of the following require the 66 implied prefix (pp=1) unless noted.
            // ================================================================

            // VMOVDQA32/64 load (0x6F pp=1): W0=DQA32 (dword), W1=DQA64 (qword)
            0x6F if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_mov_masked_load(self, ctx, es, true)
            }
            // VMOVDQU32/64 load (0x6F pp=2/F3): W0=DQU32, W1=DQU64
            0x6F if evex.pp == 2 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_mov_masked_load(self, ctx, es, false)
            }
            // VMOVDQU8/16 load (0x6F pp=3/F2): W0=DQU8 (byte), W1=DQU16 (word)
            0x6F if evex.pp == 3 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_mov_masked_load(self, ctx, es, false)
            }
            // VMOVDQA32/64 store (0x7F pp=1)
            0x7F if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_mov_masked_store(self, ctx, es, true)
            }
            // VMOVDQU32/64 store (0x7F pp=2/F3)
            0x7F if evex.pp == 2 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_mov_masked_store(self, ctx, es, false)
            }
            // VMOVDQU8/16 store (0x7F pp=3/F2)
            0x7F if evex.pp == 3 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_mov_masked_store(self, ctx, es, false)
            }

            // Logical: VPANDD/Q (0xDB), VPANDND/Q (0xDF), VPORD/Q (0xEB), VPXORD/Q (0xEF)
            0xDB if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::And)
            }
            0xDF if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::Andn)
            }
            0xEB if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::Or)
            }
            0xEF if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::Xor)
            }

            // Integer add: VPADDB/W/D/Q (0xFC/0xFD/0xFE/0xD4)
            0xFC if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddB)
            }
            0xFD if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddW)
            }
            0xFE if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddD)
            }
            0xD4 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddQ)
            }
            // Integer sub: VPSUBB/W/D/Q (0xF8/0xF9/0xFA/0xFB)
            0xF8 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubB)
            }
            0xF9 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubW)
            }
            0xFA if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubD)
            }
            0xFB if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubQ)
            }
            // VPUNPCKL* / VPUNPCKH* integer interleaves.
            0x60 if evex.pp == 1 && !evex.w => execute::simd::evex_unpack(self, ctx, 1, false),
            0x61 if evex.pp == 1 && !evex.w => execute::simd::evex_unpack(self, ctx, 2, false),
            0x62 if evex.pp == 1 && !evex.w => execute::simd::evex_unpack(self, ctx, 4, false),
            0x68 if evex.pp == 1 && !evex.w => execute::simd::evex_unpack(self, ctx, 1, true),
            0x69 if evex.pp == 1 && !evex.w => execute::simd::evex_unpack(self, ctx, 2, true),
            0x6A if evex.pp == 1 && !evex.w => execute::simd::evex_unpack(self, ctx, 4, true),
            0x6C if evex.pp == 1 && evex.w => execute::simd::evex_unpack(self, ctx, 8, false),
            0x6D if evex.pp == 1 && evex.w => execute::simd::evex_unpack(self, ctx, 8, true),
            // VPMULLW (0xD5)
            0xD5 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MullW)
            }
            // Saturating add/sub, averages, min/max, and multiply/madd word/dword forms.
            0xD8 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubSatUB)
            }
            0xD9 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubSatUW)
            }
            0xDA if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MinUB)
            }
            0xDC if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddSatUB)
            }
            0xDD if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddSatUW)
            }
            0xDE if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MaxUB)
            }
            0xE0 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AvgB)
            }
            0xE3 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AvgW)
            }
            0xE4 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MulHighUW)
            }
            0xE5 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MulHighSW)
            }
            0xE8 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubSatSB)
            }
            0xE9 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::SubSatSW)
            }
            0xEA if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MinSW)
            }
            0xEC if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddSatSB)
            }
            0xED if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::AddSatSW)
            }
            0xEE if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MaxSW)
            }
            0xF4 if evex.pp == 1 && evex.w => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MulUDQ)
            }
            0xF5 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MaddWD)
            }
            // VPACKSSWB/VPACKSSDW/VPACKUSWB.
            0x63 if evex.pp == 1 => execute::simd::evex_pack_saturate(
                self,
                ctx,
                execute::simd::PackKind::SignedWordToSignedByte,
            ),
            0x67 if evex.pp == 1 => execute::simd::evex_pack_saturate(
                self,
                ctx,
                execute::simd::PackKind::UnsignedWordToUnsignedByte,
            ),
            0x6B if evex.pp == 1 && !evex.w => execute::simd::evex_pack_saturate(
                self,
                ctx,
                execute::simd::PackKind::SignedDwordToSignedWord,
            ),
            // VCMPPS/PD/SS/SD compare into a k-mask destination.
            0xC2 if evex.pp == 0 && !evex.w => execute::simd::evex_fp_cmp(self, ctx, 4, false),
            0xC2 if evex.pp == 1 && evex.w => execute::simd::evex_fp_cmp(self, ctx, 8, false),
            0xC2 if evex.pp == 2 && !evex.w => execute::simd::evex_fp_cmp(self, ctx, 4, true),
            0xC2 if evex.pp == 3 && evex.w => execute::simd::evex_fp_cmp(self, ctx, 8, true),
            // VSHUFPS/VSHUFPD.
            0xC6 if evex.pp == 0 && !evex.w => execute::simd::evex_shufp(self, ctx, 4),
            0xC6 if evex.pp == 1 && evex.w => execute::simd::evex_shufp(self, ctx, 8),
            // VPSHUFD/HW/LW.
            0x70 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_shuffle_imm(self, ctx, execute::simd::ShuffleImmKind::Dword)
            }
            0x70 if evex.pp == 2 => {
                execute::simd::evex_shuffle_imm(self, ctx, execute::simd::ShuffleImmKind::HighWord)
            }
            0x70 if evex.pp == 3 => {
                execute::simd::evex_shuffle_imm(self, ctx, execute::simd::ShuffleImmKind::LowWord)
            }

            // Compare into mask (fixed predicate forms), pp=1 (66):
            // VPCMPEQB/W/D (0x74/0x75/0x76), VPCMPGTB/W/D (0x64/0x65/0x66)
            0x74 if evex.pp == 1 => {
                execute::simd::evex_int_cmp(self, ctx, 1, true, execute::simd::CmpPred::Eq, false)
            }
            0x75 if evex.pp == 1 => {
                execute::simd::evex_int_cmp(self, ctx, 2, true, execute::simd::CmpPred::Eq, false)
            }
            0x76 if evex.pp == 1 => {
                execute::simd::evex_int_cmp(self, ctx, 4, true, execute::simd::CmpPred::Eq, false)
            }
            0x64 if evex.pp == 1 => {
                execute::simd::evex_int_cmp(self, ctx, 1, true, execute::simd::CmpPred::Gt, false)
            }
            0x65 if evex.pp == 1 => {
                execute::simd::evex_int_cmp(self, ctx, 2, true, execute::simd::CmpPred::Gt, false)
            }
            0x66 if evex.pp == 1 => {
                execute::simd::evex_int_cmp(self, ctx, 4, true, execute::simd::CmpPred::Gt, false)
            }

            // Packed shift by immediate (group opcodes 0x71/0x72/0x73 with /reg selecting op)
            // 0x71: VPSRLW(/2), VPSRAW(/4), VPSLLW(/6)  (word)
            // 0x72: VPSRLD(/2), VPSRAD(/4), VPSLLD(/6)  (dword, or qword for SRA via W1)
            // 0x73: VPSRLQ(/2), VPSLLQ(/6)              (qword)
            0x71 if evex.pp == 1 => {
                let modrm = ctx.peek_u8()?;
                let sub = (modrm >> 3) & 0x7;
                let es = 2;
                match sub {
                    2 => {
                        execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Srl, es)
                    }
                    4 => {
                        execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Sra, es)
                    }
                    6 => {
                        execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Sll, es)
                    }
                    _ => self.inject_invalid_opcode(),
                }
            }
            0x72 if evex.pp == 1 => {
                // Need the /reg field to pick the operation.
                let modrm = ctx.peek_u8()?;
                let sub = (modrm >> 3) & 0x7;
                let es = if evex.w { 8 } else { 4 };
                match sub {
                    0 => execute::simd::evex_rotate_imm(
                        self,
                        ctx,
                        execute::simd::RotateKind::Right,
                        es,
                    ),
                    1 => execute::simd::evex_rotate_imm(
                        self,
                        ctx,
                        execute::simd::RotateKind::Left,
                        es,
                    ),
                    2 => execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Srl, 4),
                    4 => {
                        // VPSRAD (W0=dword) / VPSRAQ (W1=qword)
                        execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Sra, es)
                    }
                    6 => execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Sll, 4),
                    _ => self.inject_invalid_opcode(),
                }
            }
            0x73 if evex.pp == 1 => {
                let modrm = ctx.peek_u8()?;
                let sub = (modrm >> 3) & 0x7;
                let es = 8;
                match sub {
                    2 => {
                        execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Srl, es)
                    }
                    3 => execute::simd::evex_shift_bytes_imm(
                        self,
                        ctx,
                        execute::simd::ByteShiftKind::Right,
                    ),
                    6 => {
                        execute::simd::evex_shift_imm(self, ctx, execute::simd::ShiftKind::Sll, es)
                    }
                    7 => execute::simd::evex_shift_bytes_imm(
                        self,
                        ctx,
                        execute::simd::ByteShiftKind::Left,
                    ),
                    _ => self.inject_invalid_opcode(),
                }
            }
            // Packed shift by xmm count: VPSRLW/D/Q (0xD1/0xD2/0xD3),
            // VPSRAW/D/Q (0xE1/0xE2), VPSLLW/D/Q (0xF1/0xF2/0xF3).
            0xD1 if evex.pp == 1 => {
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Srl, 2)
            }
            0xD2 if evex.pp == 1 => {
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Srl, 4)
            }
            0xD3 if evex.pp == 1 => {
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Srl, 8)
            }
            0xE2 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Sra, es)
            }
            0xE1 if evex.pp == 1 => {
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Sra, 2)
            }
            0xF1 if evex.pp == 1 => {
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Sll, 2)
            }
            0xF2 if evex.pp == 1 => {
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Sll, 4)
            }
            0xF3 if evex.pp == 1 => {
                execute::simd::evex_shift_var(self, ctx, execute::simd::ShiftKind::Sll, 8)
            }
            // VPSADBW.
            0xF6 if evex.pp == 1 => execute::simd::evex_psadbw(self, ctx),

            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.0F opcode {:#04x} at RIP={:#x}",
                opcode, self.regs.rip
            ))),
        }
    }
}
