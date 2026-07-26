//! Precise x86 scalar floating-point-to-integer conversion semantics.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::types::{FpRoundMode, OpWidth, VReg, VecElementType};

impl SmirInterpreter {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_x86_fp_to_int(
        &self,
        ctx: &mut SmirContext,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        int_width: OpWidth,
        signed: bool,
        saturate: bool,
        truncate: bool,
        round: FpRoundMode,
        suppress_exceptions: bool,
    ) {
        let format = match elem {
            VecElementType::F16 => X86_SIMD_F16,
            VecElementType::F32 => X86_SIMD_F32,
            VecElementType::F64 => X86_SIMD_F64,
            _ => {
                ctx.request_exit(ExitReason::Undefined {
                    addr: ctx.pc,
                    opcode: 0,
                });
                return;
            }
        };
        if saturate && elem == VecElementType::F16 {
            ctx.request_exit(ExitReason::Undefined {
                addr: ctx.pc,
                opcode: 0,
            });
            return;
        }
        let int_bits = match int_width {
            OpWidth::W32 => 32,
            OpWidth::W64 => 64,
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
        if mode == FpRoundMode::RoundNearestTiesAway
            || (truncate && mode != FpRoundMode::RoundTowardZero)
        {
            ctx.request_exit(ExitReason::Undefined {
                addr: ctx.pc,
                opcode: 0,
            });
            return;
        }

        let mxcsr = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => 0x1F80,
        };
        let mut source_bits = if matches!(src, VReg::Virtual(_)) {
            ctx.read_vreg(src)
        } else {
            Self::get_lane(&Self::read_vec(ctx, src), 0, elem.bytes() * 8)
        };
        // Intel specifies only Invalid and Precision. Binary32/binary64 DAZ
        // still substitutes signed zero without reporting Denormal; binary16
        // inputs are handled normally and ignore MXCSR.DAZ.
        if format.total_bits != 16
            && mxcsr & (1 << 6) != 0
            && Self::x86_simd_fp_is_denormal(source_bits, format)
        {
            let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
            source_bits &= sign;
        }
        let converted = if saturate {
            Self::x86_simd_fp_to_int_sat(source_bits, format, int_bits, signed, mode)
        } else {
            Self::x86_simd_fp_to_int(source_bits, format, int_bits, signed, mode)
        };

        if !suppress_exceptions {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr |= converted.status;
            }
            if Self::x86_simd_fp_unmasked(converted.status, mxcsr) {
                ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                return;
            }
        }
        Self::write_gpr(ctx, dst, converted.bits, int_width);
    }
}
