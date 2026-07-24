//! Precise x86 scalar integer-to-floating-point conversion semantics.

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::types::{FpRoundMode, OpWidth, VReg, VecElementType};

impl SmirInterpreter {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_x86_int_to_fp(
        &self,
        ctx: &mut SmirContext,
        dst: VReg,
        merge: VReg,
        src: VReg,
        elem: VecElementType,
        int_width: OpWidth,
        signed: bool,
        round: FpRoundMode,
        suppress_exceptions: bool,
        zero_upper: bool,
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
        if mode == FpRoundMode::RoundNearestTiesAway {
            ctx.request_exit(ExitReason::Undefined {
                addr: ctx.pc,
                opcode: 0,
            });
            return;
        }

        let raw = ctx.read_vreg(src) & int_width.mask();
        let value = if signed {
            let shift = 128 - int_bits;
            (i128::from(raw) << shift) >> shift
        } else {
            i128::from(raw)
        };
        let negative = value < 0;
        let magnitude = if negative {
            (-value) as u128
        } else {
            value as u128
        };
        let mxcsr = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => 0x1F80,
        };
        let converted =
            Self::x86_simd_fp_round_exact(negative, magnitude, 0, false, format, mode, mxcsr);

        let mut result = Self::read_vec(ctx, merge);
        Self::set_lane(&mut result, 0, format.total_bits, converted.bits);
        if zero_upper {
            result[2..].fill(0);
        }

        if !suppress_exceptions {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr |= converted.status;
            }
            if Self::x86_simd_fp_unmasked(converted.status, mxcsr) {
                ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                return;
            }
        }
        Self::write_vec(ctx, dst, result);
    }
}
