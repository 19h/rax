//! map0f3a.rs

use crate::error::{Error, Result};
use crate::isa::x86_64::decode::dispatch::evex::*;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::{execute, flags};

impl X86_64Vcpu {
    /// EVEX 0F3A opcode map (mm=3)
    pub(crate) fn execute_evex_0f3a(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        let avx10_vminmax_disabled = !self.avx10_vminmax_enabled();

        match opcode {
            // ============================================================================
            // EVEX integer compare with imm8 predicate (write into k-mask)
            // ============================================================================

            // VPERMQ/VPERMPD immediate qword permutes.
            0x00 if evex.pp == 1 && evex.w => execute::simd::evex_permute_qword_imm(self, ctx),
            0x01 if evex.pp == 1 && evex.w => execute::simd::evex_permute_qword_imm(self, ctx),
            // VPERMILPS/VPERMILPD immediate lane-local permutes.
            0x04 if evex.pp == 1 && !evex.w => execute::simd::evex_permil_imm(self, ctx, 4),
            0x05 if evex.pp == 1 && evex.w => execute::simd::evex_permil_imm(self, ctx, 8),
            // VRNDSCALEPS/PD/PH and scalar SS/SD/SH.
            0x08 if evex.pp == 0 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::RndScale,
                false,
                true,
            ),
            0x08 if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                4,
                execute::simd::FpUnaryMathOp::RndScale,
                false,
                true,
            ),
            0x09 if evex.pp == 1 && evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                8,
                execute::simd::FpUnaryMathOp::RndScale,
                false,
                true,
            ),
            0x0A if evex.pp == 0 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::RndScale,
                true,
                true,
            ),
            0x0A if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                4,
                execute::simd::FpUnaryMathOp::RndScale,
                true,
                true,
            ),
            0x0B if evex.pp == 1 && evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                8,
                execute::simd::FpUnaryMathOp::RndScale,
                true,
                true,
            ),
            // VGF2P8AFFINEQB/VGF2P8AFFINEINVQB.
            0xCE if evex.pp == 1 && evex.w => execute::simd::evex_gf2p8_affine(self, ctx, false),
            0xCF if evex.pp == 1 && evex.w => execute::simd::evex_gf2p8_affine(self, ctx, true),

            // VPALIGNR.
            0x0F if evex.pp == 1 => execute::simd::evex_bw_palignr(self, ctx),
            0x0F => self.inject_undefined_instruction(),

            // VCVTPS2PH: packed FP32-to-FP16 store-style conversion with imm8 rounding control.
            0x1D if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_convert_store(self, ctx, 4, 2)
            }

            // VPEXTRB/W/D/Q and VEXTRACTPS.
            0x14 if evex.pp == 1 => execute::simd::evex_extract_scalar(self, ctx, 1, 4, true),
            0x15 if evex.pp == 1 => execute::simd::evex_extract_scalar(self, ctx, 2, 4, true),
            0x16 if evex.pp == 1 => {
                if evex.w {
                    execute::simd::evex_extract_scalar(self, ctx, 8, 8, true)
                } else {
                    execute::simd::evex_extract_scalar(self, ctx, 4, 4, true)
                }
            }
            0x17 if evex.pp == 1 => execute::simd::evex_extract_scalar(self, ctx, 4, 4, true),

            // VPINSRB/W/D/Q and VINSERTPS.
            0x20 if evex.pp == 1 => execute::simd::evex_pinsr(self, ctx, 1),
            0x21 if evex.pp == 1 => execute::simd::evex_insertps(self, ctx),
            0x22 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_pinsr(self, ctx, es)
            }

            // VSHUFF32x4/VSHUFF64x2 and VSHUFI32x4/VSHUFI64x2.
            0x23 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_shuffle_128_lanes(self, ctx, es)
            }
            0x43 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_shuffle_128_lanes(self, ctx, es)
            }
            0x23 | 0x43 => self.inject_undefined_instruction(),
            // VPCLMULQDQ.
            0x44 if evex.pp == 1 => execute::simd::evex_pclmulqdq(self, ctx),

            // VALIGND/Q (0x03): concatenate src2|src1 and align by imm8 elements.
            0x03 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_valign(self, ctx, es)
            }
            0x03 => self.inject_undefined_instruction(),

            // VPTERNLOGD/Q (0x25): destination is both input and output.
            0x25 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_ternlog(self, ctx, es)
            }
            // VGETMANTPS/PD/PH and scalar SS/SD/SH.
            0x26 if evex.pp == 0 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::GetMant,
                false,
                true,
            ),
            0x26 if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                4,
                execute::simd::FpUnaryMathOp::GetMant,
                false,
                true,
            ),
            0x26 if evex.pp == 1 && evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                8,
                execute::simd::FpUnaryMathOp::GetMant,
                false,
                true,
            ),
            0x27 if evex.pp == 0 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::GetMant,
                true,
                true,
            ),
            0x27 if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                4,
                execute::simd::FpUnaryMathOp::GetMant,
                true,
                true,
            ),
            0x27 if evex.pp == 1 && evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                8,
                execute::simd::FpUnaryMathOp::GetMant,
                true,
                true,
            ),

            // VINSERTF32x4/F64x2 and VINSERTI32x4/I64x2.
            0x18 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_insert_chunk(self, ctx, es, 16)
            }
            0x38 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_insert_chunk(self, ctx, es, 16)
            }
            // VEXTRACTF32x4/F64x2 and VEXTRACTI32x4/I64x2.
            0x19 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_extract_chunk(self, ctx, es, 16)
            }
            0x39 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_extract_chunk(self, ctx, es, 16)
            }
            // VINSERTF32x8/F64x4 and VINSERTI32x8/I64x4.
            0x1A if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_insert_chunk(self, ctx, es, 32)
            }
            0x3A if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_insert_chunk(self, ctx, es, 32)
            }
            // VEXTRACTF32x8/F64x4 and VEXTRACTI32x8/I64x4.
            0x1B if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_extract_chunk(self, ctx, es, 32)
            }
            0x3B if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_extract_chunk(self, ctx, es, 32)
            }

            // Immediate funnel shifts: VPSHLD* (0x70/0x71), VPSHRD* (0x72/0x73).
            0x70 if evex.pp == 1 && evex.w => execute::simd::evex_funnel_shift_imm(
                self,
                ctx,
                execute::simd::FunnelShiftKind::Left,
                2,
            ),
            0x71 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_funnel_shift_imm(
                    self,
                    ctx,
                    execute::simd::FunnelShiftKind::Left,
                    es,
                )
            }
            0x72 if evex.pp == 1 && evex.w => execute::simd::evex_funnel_shift_imm(
                self,
                ctx,
                execute::simd::FunnelShiftKind::Right,
                2,
            ),
            0x73 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_funnel_shift_imm(
                    self,
                    ctx,
                    execute::simd::FunnelShiftKind::Right,
                    es,
                )
            }

            // VPCMPUD (0x1E, W0) / VPCMPUQ (0x1E, W1): unsigned dword/qword
            0x1E if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_int_cmp(self, ctx, es, false, execute::simd::CmpPred::Eq, true)
            }
            // VPCMPD (0x1F, W0) / VPCMPQ (0x1F, W1): signed dword/qword
            0x1F if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_int_cmp(self, ctx, es, true, execute::simd::CmpPred::Eq, true)
            }
            // VPCMPUB (0x3E, W0) / VPCMPUW (0x3E, W1): unsigned byte/word
            0x3E if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_int_cmp(self, ctx, es, false, execute::simd::CmpPred::Eq, true)
            }
            // VPCMPB (0x3F, W0) / VPCMPW (0x3F, W1): signed byte/word
            0x3F if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_int_cmp(self, ctx, es, true, execute::simd::CmpPred::Eq, true)
            }
            // VCMPPH/SH compare into a k-mask destination.
            0xC2 if evex.pp == 0 && !evex.w => execute::simd::evex_fp_cmp(self, ctx, 2, false),
            0xC2 if evex.pp == 2 && !evex.w => execute::simd::evex_fp_cmp(self, ctx, 2, true),
            // VRANGEPS/PD and scalar SS/SD.
            0x50 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_ternary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpTernaryMathOp::Range,
                    false,
                    true,
                )
            }
            0x51 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_ternary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpTernaryMathOp::Range,
                    true,
                    true,
                )
            }
            // VFIXUPIMMPS/PD and scalar SS/SD.
            0x54 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fixupimm(self, ctx, es, false)
            }
            0x55 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fixupimm(self, ctx, es, true)
            }
            // VREDUCEPS/PD/PH and scalar SS/SD/SH.
            0x56 if evex.pp == 0 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::Reduce,
                false,
                true,
            ),
            0x56 if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                4,
                execute::simd::FpUnaryMathOp::Reduce,
                false,
                true,
            ),
            0x56 if evex.pp == 1 && evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                8,
                execute::simd::FpUnaryMathOp::Reduce,
                false,
                true,
            ),
            0x57 if evex.pp == 0 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                2,
                execute::simd::FpUnaryMathOp::Reduce,
                true,
                true,
            ),
            0x57 if evex.pp == 1 && !evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                4,
                execute::simd::FpUnaryMathOp::Reduce,
                true,
                true,
            ),
            0x57 if evex.pp == 1 && evex.w => execute::simd::evex_fp_unary_math(
                self,
                ctx,
                8,
                execute::simd::FpUnaryMathOp::Reduce,
                true,
                true,
            ),
            // VFPCLASSPS/PD/PH and VFPCLASSSS/SD/SH.
            0x66 if evex.pp == 0 && !evex.w => execute::simd::evex_fpclass(self, ctx, 2, false),
            0x66 if evex.pp == 1 && !evex.w => execute::simd::evex_fpclass(self, ctx, 4, false),
            0x66 if evex.pp == 1 && evex.w => execute::simd::evex_fpclass(self, ctx, 8, false),
            0x67 if evex.pp == 0 && !evex.w => execute::simd::evex_fpclass(self, ctx, 2, true),
            0x67 if evex.pp == 1 && !evex.w => execute::simd::evex_fpclass(self, ctx, 4, true),
            0x67 if evex.pp == 1 && evex.w => execute::simd::evex_fpclass(self, ctx, 8, true),

            // ============================================================================
            // AVX-512 VDBPSADBW Instruction
            // ============================================================================

            // VDBPSADBW (0x42) - Double Block Packed Sum-Absolute-Differences
            0x42 if evex.pp == 1 && !evex.w => execute::simd::evex_bw_dbpsadbw(self, ctx),
            0x42 => self.inject_undefined_instruction(),

            // ============================================================================
            // AVX10.2 VMINMAX Instructions
            // ============================================================================
            0x52 | 0x53 if matches!(evex.pp, 0 | 1) && avx10_vminmax_disabled => {
                self.inject_undefined_instruction()
            }
            // VMINMAXPS (0x52) - Minimum/Maximum of Packed Single-Precision Floats
            0x52 if evex.pp == 1 && !evex.w => self.execute_vminmax_ps(ctx),
            // VMINMAXPD (0x52) - Minimum/Maximum of Packed Double-Precision Floats
            0x52 if evex.pp == 1 && evex.w => self.execute_vminmax_pd(ctx),
            // VMINMAXSS (0x53) - Minimum/Maximum of Scalar Single-Precision Float
            0x53 if evex.pp == 1 && !evex.w => self.execute_vminmax_ss(ctx),
            // VMINMAXSD (0x53) - Minimum/Maximum of Scalar Double-Precision Float
            0x53 if evex.pp == 1 && evex.w => self.execute_vminmax_sd(ctx),

            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.0F3A opcode {:#04x} at RIP={:#x}",
                opcode, self.regs.rip
            ))),
        }
    }
}
