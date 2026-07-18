//! map5.rs

use crate::isa::x86_64::decode::dispatch::evex::*;
use crate::error::{Error, Result};
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::{execute, flags};

impl X86_64Vcpu {

    /// EVEX MAP5 opcode map (mm=5) - AVX-512 FP16 instructions
    pub(crate) fn execute_evex_map5(&mut self, ctx: &mut InsnContext, opcode: u8) -> Result<Option<VcpuExit>> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        let avx10_sat_convert_disabled = !self.avx10_sat_convert_enabled();

        // MAP5 instructions are FP16 (half-precision) arithmetic
        // pp=0 (NP), W=0 for packed FP16
        match opcode {
            // AVX10.2 BF8 conversion families use MAP5 opcode 0x74. They are
            // not part of the base CPUID profile and are not implemented here.
            0x74 => self.inject_undefined_instruction(),
            // AVX10.2 BF16 scalar compare encodings are distinct from the
            // AVX-512-FP16 VCOMISH/VUCOMISH forms below.
            0x2E | 0x2F if evex.pp == 1 && !evex.w => self.inject_undefined_instruction(),
            // VUCOMISH/VCOMISH scalar FP16 compare into RFLAGS.
            0x2E if evex.pp == 0 && !evex.w => execute::simd::evex_comi(self, ctx, 2, false),
            0x2F if evex.pp == 0 && !evex.w => execute::simd::evex_comi(self, ctx, 2, true),
            // VMOVSH scalar load/reg-reg move and store forms.
            0x10 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_scalar_fp_move(self, ctx, 2, false)
            }
            0x11 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_scalar_fp_move(self, ctx, 2, true)
            }
            // VMOVW GPR/memory to XMM and XMM to GPR/memory.
            0x6E if evex.pp == 1 && !evex.w => execute::simd::evex_gpr_or_mem_to_xmm(self, ctx, 2),
            0x7E if evex.pp == 1 && !evex.w => execute::simd::evex_xmm_to_gpr_or_mem(self, ctx, 2),
            // Scalar FP16/integer and FP16 width conversions.
            0x1D if evex.pp == 0 && !evex.w => {
                execute::simd::evex_fp_scalar_convert(self, ctx, 4, 2)
            }
            0x1D if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_convert(self, ctx, 4, 2)
            }
            0x2A if evex.pp == 2 => execute::simd::evex_gpr_to_fp(self, ctx, 2, false),
            0x2C if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 2, false, true),
            0x2D if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 2, false, false),
            0x5A if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_convert(self, ctx, 2, 8)
            }
            0x5A if evex.pp == 1 && evex.w => {
                execute::simd::evex_packed_fp_convert(self, ctx, 8, 2)
            }
            0x5A if evex.pp == 2 && !evex.w => {
                execute::simd::evex_fp_scalar_convert(self, ctx, 2, 8)
            }
            0x5A if evex.pp == 3 && evex.w => {
                execute::simd::evex_fp_scalar_convert(self, ctx, 8, 2)
            }
            0x5B if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 4, 2, true)
            }
            0x5B if evex.pp == 0 && evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 8, 2, true)
            }
            0x5B if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 4, false, false)
            }
            0x5B if evex.pp == 2 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 4, false, true)
            }
            0x78 if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 4, true, true)
            }
            0x78 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 8, true, true)
            }
            0x78 if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 2, true, true),
            0x79 if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 4, true, false)
            }
            0x79 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 8, true, false)
            }
            0x79 if evex.pp == 2 => execute::simd::evex_fp_to_gpr(self, ctx, 2, true, false),
            0x7A if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 8, false, true)
            }
            0x7A if evex.pp == 3 && !evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 4, 2, false)
            }
            0x7A if evex.pp == 3 && evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 8, 2, false)
            }
            0x7B if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 8, false, false)
            }
            0x7B if evex.pp == 2 => execute::simd::evex_gpr_to_fp(self, ctx, 2, true),
            0x7C if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 2, true, true)
            }
            0x7C if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 2, false, true)
            }
            0x7D if evex.pp == 0 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 2, true, false)
            }
            0x7D if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_to_int(self, ctx, 2, 2, false, false)
            }
            0x7D if evex.pp == 2 && !evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 2, 2, true)
            }
            0x7D if evex.pp == 3 && !evex.w => {
                execute::simd::evex_packed_int_to_fp(self, ctx, 2, 2, false)
            }
            0x68 | 0x6A if evex.pp == 1 && !evex.w && avx10_sat_convert_disabled => {
                self.inject_undefined_instruction()
            }
            0x6C | 0x6D if evex.pp == 1 && evex.w && avx10_sat_convert_disabled => {
                self.inject_undefined_instruction()
            }
            // VCVTTPS2IBS (0x68) - Convert with Truncation Packed Single to Signed Byte with Saturation
            0x68 if evex.pp == 1 && !evex.w => self.execute_vcvttps2ibs(ctx),
            // VCVTTPS2IUBS (0x6A) - Convert with Truncation Packed Single to Unsigned Byte with Saturation
            0x6A if evex.pp == 1 && !evex.w => self.execute_vcvttps2iubs(ctx),
            // VCVTTPD2QQS (0x6D) - Convert with Truncation Packed Double to Signed Qword with Saturation
            0x6D if evex.pp == 1 && evex.w => self.execute_vcvttpd2qqs(ctx),
            // VCVTTPD2UQQS (0x6C) - Convert with Truncation Packed Double to Unsigned Qword with Saturation
            0x6C if evex.pp == 1 && evex.w => self.execute_vcvttpd2uqqs(ctx),
            // VSQRTPH (0x51, NP) / VSQRTSH (0x51, F3)
            0x51 if evex.pp == 0 && !evex.w => self.execute_evex_fp16_unary(ctx, |a| a.sqrt()),
            0x51 if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp16_scalar_arith(ctx, |_, b| b.sqrt())
            }
            // VADDPH/VADDSH (0x58)
            0x58 if evex.pp == 0 && !evex.w => self.execute_evex_fp16_arith(ctx, |a, b| a + b),
            0x58 if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp16_scalar_arith(ctx, |a, b| a + b)
            }
            // VMULPH/VMULSH (0x59)
            0x59 if evex.pp == 0 && !evex.w => self.execute_evex_fp16_arith(ctx, |a, b| a * b),
            0x59 if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp16_scalar_arith(ctx, |a, b| a * b)
            }
            // VSUBPH/VSUBSH (0x5C)
            0x5C if evex.pp == 0 && !evex.w => self.execute_evex_fp16_arith(ctx, |a, b| a - b),
            0x5C if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp16_scalar_arith(ctx, |a, b| a - b)
            }
            // VMINPH/VMINSH (0x5D)
            0x5D if evex.pp == 0 && !evex.w => self.execute_evex_fp16_arith(ctx, Self::x86_min_f32),
            0x5D if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp16_scalar_arith(ctx, Self::x86_min_f32)
            }
            // VDIVPH/VDIVSH (0x5E)
            0x5E if evex.pp == 0 && !evex.w => self.execute_evex_fp16_arith(ctx, |a, b| a / b),
            0x5E if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp16_scalar_arith(ctx, |a, b| a / b)
            }
            // VMAXPH/VMAXSH (0x5F)
            0x5F if evex.pp == 0 && !evex.w => self.execute_evex_fp16_arith(ctx, Self::x86_max_f32),
            0x5F if evex.pp == 2 && !evex.w => {
                self.execute_evex_fp16_scalar_arith(ctx, Self::x86_max_f32)
            }
            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.MAP5 opcode {:#04x} (pp={}) at RIP={:#x}",
                opcode, evex.pp, self.regs.rip
            ))),
        }
    }


    /// EVEX MAP6 opcode map - AVX-512 FP16 FMA instructions.
    pub(crate) fn execute_evex_map6(&mut self, ctx: &mut InsnContext, opcode: u8) -> Result<Option<VcpuExit>> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;

        match opcode {
            // Reserve MAP6 opcode 0x74 for unsupported AVX10.2 BF8 conversion
            // encodings rather than reporting an internal unimplemented decode.
            0x74 => self.inject_undefined_instruction(),
            // VCVTSH2SS scalar FP16-to-FP32 conversion.
            0x13 if evex.pp == 0 && !evex.w => {
                execute::simd::evex_fp_scalar_convert(self, ctx, 2, 4)
            }
            // VCVTPH2PSX packed FP16-to-FP32 conversion.
            0x13 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_convert(self, ctx, 2, 4)
            }
            // VSCALEFPH/SH.
            0x2C if evex.pp == 1 && !evex.w => execute::simd::evex_fp_ternary_math(
                self,
                ctx,
                2,
                execute::simd::FpTernaryMathOp::ScaleF,
                false,
                false,
            ),
            0x2D if evex.pp == 1 && !evex.w => execute::simd::evex_fp_ternary_math(
                self,
                ctx,
                2,
                execute::simd::FpTernaryMathOp::ScaleF,
                true,
                false,
            ),
            // VGETEXPPH/SH, VRCPPH/SH, and VRSQRTPH/SH.
            0x42 if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::GetExp,
                false,
                false,
            ),
            0x43 if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::GetExp,
                true,
                false,
            ),
            0x4C if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::Rcp,
                false,
                false,
            ),
            0x4D if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::Rcp,
                true,
                false,
            ),
            0x4E if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::Rsqrt,
                false,
                false,
            ),
            0x4F if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::Rsqrt,
                true,
                false,
            ),
            // V[FC]MADDCPH/SH and V[FC]MULCPH/SH complex FP16 arithmetic.
            0x56 if (evex.pp == 2 || evex.pp == 3) && !evex.w => {
                execute::simd::evex_fp16_complex(self, ctx, true, evex.pp == 3, false)
            }
            0x57 if (evex.pp == 2 || evex.pp == 3) && !evex.w => {
                execute::simd::evex_fp16_complex(self, ctx, true, evex.pp == 3, true)
            }
            0xD6 if (evex.pp == 2 || evex.pp == 3) && !evex.w => {
                execute::simd::evex_fp16_complex(self, ctx, false, evex.pp == 3, false)
            }
            0xD7 if (evex.pp == 2 || evex.pp == 3) && !evex.w => {
                execute::simd::evex_fp16_complex(self, ctx, false, evex.pp == 3, true)
            }
            // VFM*PH/VFM*SH FP16 FMA 132/213/231 packed and scalar families.
            0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF if evex.pp == 1 && !evex.w => {
                execute::simd::evex_fma_fp16(self, ctx, opcode)
            }
            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.MAP6 opcode {:#04x} (pp={}) at RIP={:#x}",
                opcode, evex.pp, self.regs.rip
            ))),
        }
    }
}
