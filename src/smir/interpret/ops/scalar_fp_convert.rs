//! Precise x86 scalar floating-point precision-conversion semantics.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::types::{FpRoundMode, VReg, VecElementType};

impl SmirInterpreter {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_x86_fp_convert(
        &self,
        ctx: &mut SmirContext,
        dst: VReg,
        merge: VReg,
        src: VReg,
        mask: Option<VReg>,
        from: VecElementType,
        to: VecElementType,
        mask_zeroing: bool,
        round: FpRoundMode,
        suppress_exceptions: bool,
        zero_upper: bool,
    ) {
        // Validate the complete semantic shape before reading or committing
        // architectural state. In particular, ties-away is not an x86 MXCSR
        // or EVEX rounding mode and must not reach the exact-rounding core's
        // invariant-only branch.
        let (from_format, to_format) = match (from, to) {
            (VecElementType::F16, VecElementType::F32) => (X86_SIMD_F16, X86_SIMD_F32),
            (VecElementType::F16, VecElementType::F64) => (X86_SIMD_F16, X86_SIMD_F64),
            (VecElementType::F32, VecElementType::F16) => (X86_SIMD_F32, X86_SIMD_F16),
            (VecElementType::F32, VecElementType::F64) => (X86_SIMD_F32, X86_SIMD_F64),
            (VecElementType::F64, VecElementType::F16) => (X86_SIMD_F64, X86_SIMD_F16),
            (VecElementType::F64, VecElementType::F32) => (X86_SIMD_F64, X86_SIMD_F32),
            _ => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: ctx.pc,
                    opcode: 0,
                });
                return;
            }
        };
        let mode = if round == FpRoundMode::Dynamic {
            self.dynamic_fp_round_mode(ctx)
        } else {
            round
        };
        if mode == FpRoundMode::RoundNearestTiesAway {
            ctx.request_exit(ExitReason::Undefined {
                addr: ctx.pc,
                opcode: 0,
            });
            return;
        }

        let mut result = Self::read_vec(ctx, merge);
        if zero_upper {
            result[2..].fill(0);
        }
        let active = mask.map_or(true, |reg| ctx.read_vreg(reg) & 1 != 0);
        if !active {
            let scalar_bits = if mask_zeroing {
                0
            } else {
                Self::get_lane(&Self::read_vec(ctx, dst), 0, to.bytes() * 8)
            };
            Self::set_lane(&mut result, 0, to.bytes() * 8, scalar_bits);
            Self::write_vec(ctx, dst, result);
            return;
        }

        let source = if matches!(src, VReg::Virtual(_)) {
            ctx.read_vreg(src)
        } else {
            Self::get_lane(&Self::read_vec(ctx, src), 0, from.bytes() * 8)
        };
        let mxcsr = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => 0x1F80,
        };
        let conversion_mxcsr = if to == VecElementType::F16 {
            // Scalar conversions to FP16 retain gradual-underflow results
            // independently of MXCSR.FTZ, like their packed counterparts.
            mxcsr & !(1 << 15)
        } else {
            mxcsr
        };
        let converted = Self::x86_simd_fp_convert_precision(
            source,
            from_format,
            to_format,
            mode,
            conversion_mxcsr,
            true,
        );
        if !suppress_exceptions {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr |= converted.status;
            }
            if Self::x86_simd_fp_unmasked(converted.status, mxcsr) {
                ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                return;
            }
        }
        Self::set_lane(&mut result, 0, to.bytes() * 8, converted.bits);
        Self::write_vec(ctx, dst, result);
    }
}
