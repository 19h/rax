//! map0f38.rs

use crate::error::{Error, Result};
use crate::isa::x86_64::decode::dispatch::evex::*;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::{execute, flags};

impl X86_64Vcpu {
    /// EVEX 0F38 opcode map (mm=2)
    pub(crate) fn execute_evex_0f38(
        &mut self,
        ctx: &mut InsnContext,
        opcode: u8,
    ) -> Result<Option<VcpuExit>> {
        let evex = ctx
            .evex
            .ok_or_else(|| Error::Emulator("EVEX context missing".to_string()))?;
        let avx10_media_disabled = !self.avx10_media_enabled();

        match opcode {
            // VPMULLD/VPMULLQ (0x40)
            // W=0: VPMULLD (32-bit elements)
            // W=1: VPMULLQ (64-bit elements)
            0x40 if evex.pp == 1 => {
                if evex.w {
                    execute::simd::vpmullq(self, ctx)
                } else {
                    execute::simd::vpmulld_evex(self, ctx)
                }
            }
            // VPMADDUBSW (0x04)
            0x04 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MaddUBSW)
            }
            // VPMULHRSW (0x0B). EVEX.W is architecturally ignored (WIG).
            0x0B if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MulHighRoundSW)
            }
            // VCVTPH2PS.
            0x13 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_packed_fp_convert(self, ctx, 2, 4)
            }
            // VMOVNTDQA (66.0F38.2A) memory load.
            0x2A if evex.pp == 1 && !evex.w => execute::simd::evex_nt_load(self, ctx),
            // VSCALEFPS/PD and scalar VSCALEFSS/SD.
            0x2C if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_ternary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpTernaryMathOp::ScaleF,
                    false,
                    false,
                )
            }
            0x2D if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_ternary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpTernaryMathOp::ScaleF,
                    true,
                    false,
                )
            }
            // VGETEXPPS/PD and scalar VGETEXPSS/SD.
            0x42 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::GetExp,
                    false,
                    false,
                )
            }
            0x43 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::GetExp,
                    true,
                    false,
                )
            }
            // VRCP14PS/PD, VRSQRT14PS/PD, and scalar SS/SD forms.
            0x4C if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rcp14,
                    false,
                    false,
                )
            }
            0x4D if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rcp14,
                    true,
                    false,
                )
            }
            0x4E if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rsqrt14,
                    false,
                    false,
                )
            }
            0x4F if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rsqrt14,
                    true,
                    false,
                )
            }
            // VAESENC/VAESENCLAST/VAESDEC/VAESDECLAST (WIG).
            0xDC if evex.pp == 1 => {
                execute::simd::evex_vaes(self, ctx, execute::simd::VaesRound::Enc)
            }
            0xDD if evex.pp == 1 => {
                execute::simd::evex_vaes(self, ctx, execute::simd::VaesRound::EncLast)
            }
            0xDE if evex.pp == 1 => {
                execute::simd::evex_vaes(self, ctx, execute::simd::VaesRound::Dec)
            }
            0xDF if evex.pp == 1 => {
                execute::simd::evex_vaes(self, ctx, execute::simd::VaesRound::DecLast)
            }
            // VP2INTERSECTD/Q.
            0x68 if evex.pp == 3 && !self.vp2intersect_enabled() => {
                self.inject_undefined_instruction()
            }
            0x68 if evex.pp == 3 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_p2intersect(self, ctx, es)
            }
            // VPSHUFB.
            0x00 if evex.pp == 1 => execute::simd::evex_pshufb(self, ctx),
            // Per-element variable shifts: VPSRLV*, VPSRAV*, VPSLLV*.
            0x10 if evex.pp == 1 && evex.w => {
                execute::simd::evex_shift_per_elem(self, ctx, execute::simd::ShiftKind::Srl, 2)
            }
            0x11 if evex.pp == 1 && evex.w => {
                execute::simd::evex_shift_per_elem(self, ctx, execute::simd::ShiftKind::Sra, 2)
            }
            0x12 if evex.pp == 1 && evex.w => {
                execute::simd::evex_shift_per_elem(self, ctx, execute::simd::ShiftKind::Sll, 2)
            }
            // Variable funnel shifts: VPSHLDV* (0x70/0x71) and VPSHRDV* (0x72/0x73).
            0x70 if evex.pp == 1 && evex.w => execute::simd::evex_funnel_shift_per_elem(
                self,
                ctx,
                execute::simd::FunnelShiftKind::Left,
                2,
            ),
            0x71 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_funnel_shift_per_elem(
                    self,
                    ctx,
                    execute::simd::FunnelShiftKind::Left,
                    es,
                )
            }
            0x72 if evex.pp == 1 && evex.w => execute::simd::evex_funnel_shift_per_elem(
                self,
                ctx,
                execute::simd::FunnelShiftKind::Right,
                2,
            ),
            0x73 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_funnel_shift_per_elem(
                    self,
                    ctx,
                    execute::simd::FunnelShiftKind::Right,
                    es,
                )
            }
            // Per-element variable rotates: VPRORVD/Q (0x14), VPROLVD/Q (0x15).
            0x14 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_rotate_per_elem(self, ctx, execute::simd::RotateKind::Right, es)
            }
            0x15 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_rotate_per_elem(self, ctx, execute::simd::RotateKind::Left, es)
            }
            // VPMOVUS*: narrow with unsigned saturation.
            0x10 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                2,
                1,
                execute::simd::NarrowMode::UnsignedSaturate,
            ),
            0x11 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                4,
                1,
                execute::simd::NarrowMode::UnsignedSaturate,
            ),
            0x12 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                8,
                1,
                execute::simd::NarrowMode::UnsignedSaturate,
            ),
            0x13 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                4,
                2,
                execute::simd::NarrowMode::UnsignedSaturate,
            ),
            0x14 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                8,
                2,
                execute::simd::NarrowMode::UnsignedSaturate,
            ),
            0x15 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                8,
                4,
                execute::simd::NarrowMode::UnsignedSaturate,
            ),
            // VPABSB/W/D/Q (0x1C..0x1F)
            0x1C if evex.pp == 1 => execute::simd::evex_int_abs(self, ctx, 1),
            0x1D if evex.pp == 1 => execute::simd::evex_int_abs(self, ctx, 2),
            0x1E if evex.pp == 1 && !evex.w => execute::simd::evex_int_abs(self, ctx, 4),
            0x1F if evex.pp == 1 && evex.w => execute::simd::evex_int_abs(self, ctx, 8),
            // VPMULDQ (0x28)
            0x28 if evex.pp == 1 && evex.w => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MulDQ)
            }
            // VPMOVM2B/W (0x28, F3)
            0x28 if evex.pp == 2 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_mask_to_vec(self, ctx, es)
            }
            // VPMOVB/W2M (0x29, F3)
            0x29 if evex.pp == 2 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_vec_to_mask(self, ctx, es)
            }
            // VPBROADCASTMB2Q (0x2A, F3.W1)
            0x2A if evex.pp == 2 && evex.w => execute::simd::evex_broadcast_mask(self, ctx, 8, 8),
            0x2A if evex.pp == 2 => self.inject_undefined_instruction(),
            // VPBLENDMD/Q (0x64), VBLENDMPS/PD (0x65), VPBLENDMB/W (0x66).
            0x64 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_blend_select(self, ctx, es)
            }
            0x65 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_blend_select(self, ctx, es)
            }
            0x66 if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_blend_select(self, ctx, es)
            }
            // VPMOVS*: narrow with signed saturation.
            0x20 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                2,
                1,
                execute::simd::NarrowMode::SignedSaturate,
            ),
            0x21 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                4,
                1,
                execute::simd::NarrowMode::SignedSaturate,
            ),
            0x22 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                8,
                1,
                execute::simd::NarrowMode::SignedSaturate,
            ),
            0x23 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                4,
                2,
                execute::simd::NarrowMode::SignedSaturate,
            ),
            0x24 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                8,
                2,
                execute::simd::NarrowMode::SignedSaturate,
            ),
            0x25 if evex.pp == 2 && !evex.w => execute::simd::evex_int_narrow(
                self,
                ctx,
                8,
                4,
                execute::simd::NarrowMode::SignedSaturate,
            ),
            // VPMOVSX*/VPMOVZX* reserve vvvv, V', b, L'L=3, and {z} with
            // k0. The DQ forms additionally require W0; W is ignored for the
            // other ten opcodes.
            0x20..=0x25 | 0x30..=0x35
                if evex.pp == 1
                    && (evex.vvvv != 0xF
                        || !evex.v_prime
                        || evex.broadcast
                        || evex.ll == 3
                        || (evex.z && evex.aaa == 0)
                        || (matches!(opcode, 0x25 | 0x35) && evex.w)) =>
            {
                self.inject_undefined_instruction()
            }
            // VPMOVSX*: sign extend packed byte/word/dword elements.
            0x20 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 1, 2, true),
            0x21 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 1, 4, true),
            0x22 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 1, 8, true),
            0x23 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 2, 4, true),
            0x24 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 2, 8, true),
            0x25 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_int_extend(self, ctx, 4, 8, true)
            }
            // VPMOV*: narrow by truncating high bits.
            0x30 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_int_narrow(self, ctx, 2, 1, execute::simd::NarrowMode::Truncate)
            }
            0x31 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_int_narrow(self, ctx, 4, 1, execute::simd::NarrowMode::Truncate)
            }
            0x32 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_int_narrow(self, ctx, 8, 1, execute::simd::NarrowMode::Truncate)
            }
            0x33 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_int_narrow(self, ctx, 4, 2, execute::simd::NarrowMode::Truncate)
            }
            0x34 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_int_narrow(self, ctx, 8, 2, execute::simd::NarrowMode::Truncate)
            }
            0x35 if evex.pp == 2 && !evex.w => {
                execute::simd::evex_int_narrow(self, ctx, 8, 4, execute::simd::NarrowMode::Truncate)
            }
            // VPMOVZX*: zero extend packed byte/word/dword elements.
            0x30 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 1, 2, false),
            0x31 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 1, 4, false),
            0x32 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 1, 8, false),
            0x33 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 2, 4, false),
            0x34 if evex.pp == 1 => execute::simd::evex_int_extend(self, ctx, 2, 8, false),
            0x35 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_int_extend(self, ctx, 4, 8, false)
            }
            // VPTESTMB/W (66.0F38.26) and VPTESTNMB/W (F3.0F38.26)
            0x26 if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_int_test_mask(self, ctx, es, false)
            }
            0x26 if evex.pp == 2 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_int_test_mask(self, ctx, es, true)
            }
            // VPTESTMD/Q (66.0F38.27) and VPTESTNMD/Q (F3.0F38.27)
            0x27 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_int_test_mask(self, ctx, es, false)
            }
            0x27 if evex.pp == 2 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_int_test_mask(self, ctx, es, true)
            }
            // Broadcasts (pp=1 / 66):
            // VBROADCASTSS (0x18, W0): broadcast 32-bit float
            0x18 if evex.pp == 1 && !evex.w => execute::simd::evex_broadcast(self, ctx, 4),
            // VBROADCASTF32X2 (0x19, W0): broadcast 64-bit block, xmm/m64 source
            0x19 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_broadcast_block(self, ctx, 4, 8, 32, true)
            }
            // VBROADCASTSD (0x19, W1): broadcast 64-bit double
            0x19 if evex.pp == 1 && evex.w => execute::simd::evex_broadcast(self, ctx, 8),
            // VBROADCASTF32X4 / VBROADCASTF64X2 (0x1A), memory source
            0x1A if evex.pp == 1 => {
                if evex.w {
                    execute::simd::evex_broadcast_block(self, ctx, 8, 16, 32, false)
                } else {
                    execute::simd::evex_broadcast_block(self, ctx, 4, 16, 32, false)
                }
            }
            // VBROADCASTF32X8 / VBROADCASTF64X4 (0x1B), memory source
            0x1B if evex.pp == 1 => {
                if evex.w {
                    execute::simd::evex_broadcast_block(self, ctx, 8, 32, 64, false)
                } else {
                    execute::simd::evex_broadcast_block(self, ctx, 4, 32, 64, false)
                }
            }
            // VPBROADCASTD (0x58, W0): broadcast 32-bit integer
            0x58 if evex.pp == 1 && !evex.w => execute::simd::evex_broadcast(self, ctx, 4),
            // VBROADCASTI32X2 (0x59, W0): broadcast 64-bit block, xmm/m64 source
            0x59 if evex.pp == 1 && !evex.w => {
                execute::simd::evex_broadcast_block(self, ctx, 4, 8, 16, true)
            }
            // VPBROADCASTQ (0x59, W1): broadcast 64-bit integer
            0x59 if evex.pp == 1 && evex.w => execute::simd::evex_broadcast(self, ctx, 8),
            // VBROADCASTI32X4 / VBROADCASTI64X2 (0x5A), memory source
            0x5A if evex.pp == 1 => {
                if evex.w {
                    execute::simd::evex_broadcast_block(self, ctx, 8, 16, 32, false)
                } else {
                    execute::simd::evex_broadcast_block(self, ctx, 4, 16, 32, false)
                }
            }
            // VBROADCASTI32X8 / VBROADCASTI64X4 (0x5B), memory source
            0x5B if evex.pp == 1 => {
                if evex.w {
                    execute::simd::evex_broadcast_block(self, ctx, 8, 32, 64, false)
                } else {
                    execute::simd::evex_broadcast_block(self, ctx, 4, 32, 64, false)
                }
            }
            // VPBROADCASTB (0x78, W0): broadcast 8-bit integer
            0x78 if evex.pp == 1 && !evex.w => execute::simd::evex_broadcast(self, ctx, 1),
            // VPBROADCASTW (0x79, W0): broadcast 16-bit integer
            0x79 if evex.pp == 1 && !evex.w => execute::simd::evex_broadcast(self, ctx, 2),
            // VPBROADCASTB/W/D/Q GPR-source forms.
            0x7A if evex.pp == 1 && !evex.w => execute::simd::evex_broadcast_gpr(self, ctx, 1),
            0x7B if evex.pp == 1 && !evex.w => execute::simd::evex_broadcast_gpr(self, ctx, 2),
            0x7C if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_broadcast_gpr(self, ctx, es)
            }
            0x7A..=0x7C => self.inject_undefined_instruction(),

            // FP32/FP64 FMA 132/213/231 packed and scalar families.
            0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF if evex.pp == 1 => {
                execute::simd::evex_fma(self, ctx, opcode)
            }
            // V4FMADDPS/SS and V4FNMADDPS/SS source-block FMA forms.
            0x9A | 0x9B | 0xAA | 0xAB
                if evex.pp == 3 && !evex.w && !self.xeon_phi_avx512_enabled() =>
            {
                self.inject_undefined_instruction()
            }
            0x9A if evex.pp == 3 && !evex.w => {
                execute::simd::evex_4fmaddps(self, ctx, false, false)
            }
            0x9B if evex.pp == 3 && !evex.w => execute::simd::evex_4fmaddps(self, ctx, true, false),
            0xAA if evex.pp == 3 && !evex.w => execute::simd::evex_4fmaddps(self, ctx, false, true),
            0xAB if evex.pp == 3 && !evex.w => execute::simd::evex_4fmaddps(self, ctx, true, true),

            // VPCMPEQQ (0x29, W1): qword equality compare into mask
            0x29 if evex.pp == 1 && evex.w => {
                execute::simd::evex_int_cmp(self, ctx, 8, true, execute::simd::CmpPred::Eq, false)
            }
            // VPCMPGTQ (0x37, W1): qword signed greater-than compare into mask
            0x37 if evex.pp == 1 && evex.w => {
                execute::simd::evex_int_cmp(self, ctx, 8, true, execute::simd::CmpPred::Gt, false)
            }
            0x29 | 0x37 if evex.pp == 1 => self.inject_undefined_instruction(),
            // Packed integer min/max.
            0x38 if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MinSB)
            }
            // VPMOVM2D/Q (0x38, F3)
            0x38 if evex.pp == 2 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_mask_to_vec(self, ctx, es)
            }
            // VPMOVD/Q2M (0x39, F3)
            0x39 if evex.pp == 2 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_vec_to_mask(self, ctx, es)
            }
            // VPACKUSDW (0x2B)
            0x2B if evex.pp == 1 && !evex.w => execute::simd::evex_pack_saturate(
                self,
                ctx,
                execute::simd::PackKind::UnsignedDwordToUnsignedWord,
            ),
            0x39 if evex.pp == 1 => {
                let op = if evex.w {
                    execute::simd::IntOp::MinSQ
                } else {
                    execute::simd::IntOp::MinSD
                };
                execute::simd::evex_int_arith(self, ctx, op)
            }
            0x3A if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MinUW)
            }
            // VPBROADCASTMW2D (0x3A, F3.W0)
            0x3A if evex.pp == 2 && !evex.w => execute::simd::evex_broadcast_mask(self, ctx, 16, 4),
            0x3A if evex.pp == 2 => self.inject_undefined_instruction(),
            0x3B if evex.pp == 1 => {
                let op = if evex.w {
                    execute::simd::IntOp::MinUQ
                } else {
                    execute::simd::IntOp::MinUD
                };
                execute::simd::evex_int_arith(self, ctx, op)
            }
            0x3C if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MaxSB)
            }
            0x3D if evex.pp == 1 => {
                let op = if evex.w {
                    execute::simd::IntOp::MaxSQ
                } else {
                    execute::simd::IntOp::MaxSD
                };
                execute::simd::evex_int_arith(self, ctx, op)
            }
            0x3E if evex.pp == 1 => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::MaxUW)
            }
            0x3F if evex.pp == 1 => {
                let op = if evex.w {
                    execute::simd::IntOp::MaxUQ
                } else {
                    execute::simd::IntOp::MaxUD
                };
                execute::simd::evex_int_arith(self, ctx, op)
            }
            // VPLZCNTD/Q (0x44) - leading zero count for packed dwords/qwords.
            0x44 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_count(self, ctx, execute::simd::CountKind::Lzcnt, es)
            }
            0x45 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_shift_per_elem(self, ctx, execute::simd::ShiftKind::Srl, es)
            }
            0x46 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_shift_per_elem(self, ctx, execute::simd::ShiftKind::Sra, es)
            }
            0x47 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_shift_per_elem(self, ctx, execute::simd::ShiftKind::Sll, es)
            }

            // VPEXPANDB/VPEXPANDW (0x62)
            0x62 if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::vexpand_evex(
                    self,
                    ctx,
                    es,
                    if evex.w { "VPEXPANDW" } else { "VPEXPANDB" },
                )
            }
            // VPCOMPRESSB/VPCOMPRESSW (0x63)
            0x63 if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::vcompress_evex(
                    self,
                    ctx,
                    es,
                    if evex.w { "VPCOMPRESSW" } else { "VPCOMPRESSB" },
                )
            }

            // VPGATHERD*/Q* and VGATHERD*/Q*.
            0x90..=0x93 if evex.pp == 1 => execute::simd::evex_gather(self, ctx, opcode),
            // VPSCATTERD*/Q* and VSCATTERD*/Q*.
            0xA0..=0xA3 if evex.pp == 1 => execute::simd::evex_scatter(self, ctx, opcode),
            // VGATHERPF*/VSCATTERPF* opcode-extension forms.
            0xC6 | 0xC7 if evex.pp == 1 && !self.xeon_phi_avx512_enabled() => {
                self.inject_undefined_instruction()
            }
            0xC6 | 0xC7 if evex.pp == 1 => execute::simd::evex_vsib_prefetch(self, ctx, opcode),

            // VEXPANDPS/VEXPANDPD (0x88)
            0x88 if evex.pp == 1 => {
                if evex.w {
                    execute::simd::vexpand_evex(self, ctx, 8, "VEXPANDPD")
                } else {
                    execute::simd::vexpand_evex(self, ctx, 4, "VEXPANDPS")
                }
            }
            // VPEXPANDD/VPEXPANDQ (0x89)
            0x89 if evex.pp == 1 => {
                if evex.w {
                    execute::simd::vexpand_evex(self, ctx, 8, "VPEXPANDQ")
                } else {
                    execute::simd::vexpand_evex(self, ctx, 4, "VPEXPANDD")
                }
            }
            // VCOMPRESSPS/VCOMPRESSPD (0x8A)
            0x8A if evex.pp == 1 => {
                if evex.w {
                    execute::simd::vcompress_evex(self, ctx, 8, "VCOMPRESSPD")
                } else {
                    execute::simd::vcompress_evex(self, ctx, 4, "VCOMPRESSPS")
                }
            }
            // VPCOMPRESSD/VPCOMPRESSQ (0x8B)
            0x8B if evex.pp == 1 => {
                if evex.w {
                    execute::simd::vcompress_evex(self, ctx, 8, "VPCOMPRESSQ")
                } else {
                    execute::simd::vcompress_evex(self, ctx, 4, "VPCOMPRESSD")
                }
            }

            // ============================================================================
            // AVX10.1 VNNI Instructions
            // ============================================================================

            // VPDPBUSD (0x50) - Multiply and Add Unsigned and Signed Bytes
            0x50 if evex.pp == 1 && !evex.w => self.execute_vpdpbusd(ctx, false),
            // VPDPBUSDS (0x51) - Multiply and Add Unsigned and Signed Bytes with Saturation
            0x51 if evex.pp == 1 && !evex.w => self.execute_vpdpbusd(ctx, true),
            // VPDPWSSD (0x52) - Multiply and Add Signed Word Integers
            0x52 if evex.pp == 1 && !evex.w => self.execute_vpdpwssd(ctx, false),
            // VPDPWSSDS (0x53) - Multiply and Add Signed Word Integers with Saturation
            0x53 if evex.pp == 1 && !evex.w => self.execute_vpdpwssd(ctx, true),
            // VP4DPWSSD/VP4DPWSSDS source-block dot products.
            0x52 | 0x53 if evex.pp == 3 && !evex.w && !self.xeon_phi_avx512_enabled() => {
                self.inject_undefined_instruction()
            }
            0x52 if evex.pp == 3 && !evex.w => execute::simd::evex_4dpwssd(self, ctx, false),
            0x53 if evex.pp == 3 && !evex.w => execute::simd::evex_4dpwssd(self, ctx, true),

            // ============================================================================
            // AVX10.1 IFMA Instructions
            // ============================================================================

            // VPMADD52LUQ (0xB4) - Packed Multiply of Unsigned 52-bit and Add Low Qword
            0xB4 if evex.pp == 1 && evex.w => self.execute_vpmadd52(ctx, false),
            // VPMADD52HUQ (0xB5) - Packed Multiply of Unsigned 52-bit and Add High Qword
            0xB5 if evex.pp == 1 && evex.w => self.execute_vpmadd52(ctx, true),

            // ============================================================================
            // AVX10.1 VPOPCNTDQ Instructions
            // ============================================================================

            // VPOPCNTB/W (0x54) - Population count for packed bytes/words
            0x54 if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_count(self, ctx, execute::simd::CountKind::Popcnt, es)
            }
            // VPOPCNTD/Q (0x55) - Population count for packed dwords/qwords
            0x55 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_count(self, ctx, execute::simd::CountKind::Popcnt, es)
            }
            // VPCONFLICTD/Q (0xC4) - conflict detection for packed dwords/qwords
            0xC4 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_conflict(self, ctx, es)
            }
            // VEXP2PS/PD, VRCP28PS/PD, VRSQRT28PS/PD, and scalar 28-bit forms.
            0xC8 | 0xCA | 0xCB | 0xCC | 0xCD if evex.pp == 1 && !self.xeon_phi_avx512_enabled() => {
                self.inject_undefined_instruction()
            }
            0xC8 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Exp2,
                    false,
                    false,
                )
            }
            0xCA if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rcp,
                    false,
                    false,
                )
            }
            0xCB if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rcp,
                    true,
                    false,
                )
            }
            0xCC if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rsqrt,
                    false,
                    false,
                )
            }
            0xCD if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_fp_unary_math(
                    self,
                    ctx,
                    es,
                    execute::simd::FpUnaryMathOp::Rsqrt,
                    true,
                    false,
                )
            }
            // VGF2P8MULB (0xCF) - byte multiply in GF(2^8).
            0xCF if evex.pp == 1 && !evex.w => {
                execute::simd::evex_int_arith(self, ctx, execute::simd::IntOp::Gf2p8MulB)
            }

            // VPERMPS/VPERMPD and VPERMD/VPERMQ variable-index permutes.
            0x0C if evex.pp == 1 && !evex.w => execute::simd::evex_permil_var(self, ctx, 4),
            0x0D if evex.pp == 1 && evex.w => execute::simd::evex_permil_var(self, ctx, 8),
            0x16 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_permute_var(self, ctx, es, false, true)
            }
            0x36 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_permute_var(self, ctx, es, false, true)
            }

            // ============================================================================
            // AVX10.1 VBMI Instructions
            // ============================================================================

            // VPERMB (0x8D) - Permute Packed Bytes Elements
            0x8D if evex.pp == 1 && !evex.w => self.execute_vpermb(ctx),
            // VPERMW (0x8D, W1) - Permute Packed Word Elements
            0x8D if evex.pp == 1 && evex.w => {
                execute::simd::evex_permute_var(self, ctx, 2, true, false)
            }
            // VPMULTISHIFTQB (0x83, W1).
            0x83 if evex.pp == 1 && evex.w => execute::simd::evex_multishift_qb(self, ctx),
            // VPERMI2B/W (0x75) - two-table permute overwriting index.
            0x75 if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_two_table_permute(self, ctx, es, true, false)
            }
            // VPERMI2D/Q (0x76) and VPERMI2PS/PD (0x77).
            0x76 | 0x77 if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_two_table_permute(self, ctx, es, true, true)
            }
            // VPERMT2B/W (0x7D) - two-table permute overwriting table.
            0x7D if evex.pp == 1 => {
                let es = if evex.w { 2 } else { 1 };
                execute::simd::evex_two_table_permute(self, ctx, es, false, false)
            }
            // VPERMT2D/Q (0x7E) and VPERMT2PS/PD (0x7F).
            0x7E | 0x7F if evex.pp == 1 => {
                let es = if evex.w { 8 } else { 4 };
                execute::simd::evex_two_table_permute(self, ctx, es, false, true)
            }

            // ============================================================================
            // AVX10.1 BITALG Instructions
            // ============================================================================

            // VPSHUFBITQMB (0x8F) - Shuffle Bits from Quadword Elements Using Byte Indexes into Mask
            0x8F if evex.pp == 1 && !evex.w => self.execute_vpshufbitqmb(ctx),

            // ============================================================================
            // AVX10.1 BF16 Instructions
            // ============================================================================

            // VDPBF16PS (0x52) - Dot Product of BF16 Pairs Accumulated into FP32
            0x52 if evex.pp == 2 && !evex.w => self.execute_vdpbf16ps(ctx),
            // VCVTNEPS2BF16 (0x72) - Convert Packed Single to BF16
            0x72 if evex.pp == 2 && !evex.w => self.execute_vcvtneps2bf16(ctx),
            // VCVTNE2PS2BF16 (0x72) - Convert Two Packed Single to BF16
            0x72 if evex.pp == 3 && !evex.w => self.execute_vcvtne2ps2bf16(ctx),

            // ============================================================================
            // AVX10.2 Media Acceleration Instructions (VPDPB*/VPDPW*)
            // ============================================================================
            0x50 | 0x51 if matches!(evex.pp, 0 | 2 | 3) && !evex.w && avx10_media_disabled => {
                self.inject_undefined_instruction()
            }
            0xD2 | 0xD3 if evex.pp <= 2 && !evex.w && avx10_media_disabled => {
                self.inject_undefined_instruction()
            }
            // VPDPBSSD (0x50) - Multiply and Add Signed Byte Integers
            0x50 if evex.pp == 3 && !evex.w => self.execute_vpdpbssd(ctx, false),
            // VPDPBSSDS (0x51) - Multiply and Add Signed Byte Integers with Saturation
            0x51 if evex.pp == 3 && !evex.w => self.execute_vpdpbssd(ctx, true),
            // VPDPBSUD (0x50) - Multiply and Add Signed/Unsigned Byte Integers
            0x50 if evex.pp == 2 && !evex.w => self.execute_vpdpbsud(ctx, false),
            // VPDPBSUDS (0x51) - Multiply and Add Signed/Unsigned Byte Integers with Saturation
            0x51 if evex.pp == 2 && !evex.w => self.execute_vpdpbsud(ctx, true),
            // VPDPBUUD (0x50) - Multiply and Add Unsigned Byte Integers
            0x50 if evex.pp == 0 && !evex.w => self.execute_vpdpbuud(ctx, false),
            // VPDPBUUDS (0x51) - Multiply and Add Unsigned Byte Integers with Saturation
            0x51 if evex.pp == 0 && !evex.w => self.execute_vpdpbuud(ctx, true),
            // VPDPWSUD (0xD2) - Multiply and Add Signed/Unsigned Word Integers
            0xD2 if evex.pp == 2 && !evex.w => self.execute_vpdpwsud(ctx, false),
            // VPDPWSUDS (0xD3) - Multiply and Add Signed/Unsigned Word Integers with Saturation
            0xD3 if evex.pp == 2 && !evex.w => self.execute_vpdpwsud(ctx, true),
            // VPDPWUSD (0xD2) - Multiply and Add Unsigned/Signed Word Integers
            0xD2 if evex.pp == 1 && !evex.w => self.execute_vpdpwusd(ctx, false),
            // VPDPWUSDS (0xD3) - Multiply and Add Unsigned/Signed Word Integers with Saturation
            0xD3 if evex.pp == 1 && !evex.w => self.execute_vpdpwusd(ctx, true),
            // VPDPWUUD (0xD2) - Multiply and Add Unsigned Word Integers
            0xD2 if evex.pp == 0 && !evex.w => self.execute_vpdpwuud(ctx, false),
            // VPDPWUUDS (0xD3) - Multiply and Add Unsigned Word Integers with Saturation
            0xD3 if evex.pp == 0 && !evex.w => self.execute_vpdpwuud(ctx, true),

            // CMPccXADD uses EVEX.66.0F38.W{0,1} E0..EF /r. The emulator does
            // not implement the CMPCCXADD extension, so preserve architectural
            // unsupported-feature behavior instead of reporting an internal
            // unimplemented decode error.
            0xE0..=0xEF => self.inject_undefined_instruction(),

            _ => Err(Error::Emulator(format!(
                "Unimplemented EVEX.0F38 opcode {:#04x} (W={}) at RIP={:#x}",
                opcode, evex.w as u8, self.regs.rip
            ))),
        }
    }
}
