//! x86 scalar floating-point comparisons that write RFLAGS.

use crate::smir::interpret::{SmirInterpreter, X86_SIMD_F16, X86_SIMD_F32, X86_SIMD_F64};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::types::{VReg, VecElementType};

impl SmirInterpreter {
    /// Execute COMIS*/UCOMIS* with precise MXCSR status, trap, SAE, and flag
    /// commit behavior. Complexity is O(1) time and O(1) space.
    pub(crate) fn execute_x86_fp_compare(
        &self,
        ctx: &mut SmirContext,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        signaling: bool,
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
        let elem_bits = elem.bytes() * 8;
        let first_raw = Self::get_lane(&Self::read_vec(ctx, src1), 0, elem_bits);
        let second_raw = Self::get_lane(&Self::read_vec(ctx, src2), 0, elem_bits);
        let mxcsr = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => 0x1F80,
        };
        // Intel SDM Vol. 2: AVX512-FP16 instructions always handle FP16
        // denormal inputs; MXCSR.DAZ does not convert them to signed zero.
        // They still accrue the documented denormal-input status. FP32/FP64
        // COMI/UCOMI retain the ordinary MXCSR.DAZ behavior.
        let (first, first_status) = if elem == VecElementType::F16 {
            (
                first_raw,
                u32::from(Self::x86_simd_fp_is_denormal(first_raw, format)) << 1,
            )
        } else {
            let first = Self::x86_simd_fp_apply_daz(first_raw, format, mxcsr);
            (first.bits, first.status)
        };
        let (second, second_status) = if elem == VecElementType::F16 {
            (
                second_raw,
                u32::from(Self::x86_simd_fp_is_denormal(second_raw, format)) << 1,
            )
        } else {
            let second = Self::x86_simd_fp_apply_daz(second_raw, format, mxcsr);
            (second.bits, second.status)
        };
        let first_nan = Self::x86_simd_fp_is_nan(first, format);
        let second_nan = Self::x86_simd_fp_is_nan(second, format);
        let invalid = Self::x86_simd_fp_is_snan(first, format)
            || Self::x86_simd_fp_is_snan(second, format)
            || (signaling && (first_nan || second_nan));
        let status = first_status | second_status | u32::from(invalid);

        if !suppress_exceptions {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr |= status;
            }
            if Self::x86_simd_fp_unmasked(status, mxcsr) {
                // Intel SDM: an unmasked SIMD exception leaves EFLAGS
                // unchanged, including OF/SF/AF.
                ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                return;
            }
        }

        let ordering = if first_nan || second_nan {
            None
        } else {
            match elem {
                VecElementType::F16 => Self::x86_fp16_to_f32(first as u16)
                    .partial_cmp(&Self::x86_fp16_to_f32(second as u16)),
                VecElementType::F32 => {
                    f32::from_bits(first as u32).partial_cmp(&f32::from_bits(second as u32))
                }
                VecElementType::F64 => f64::from_bits(first).partial_cmp(&f64::from_bits(second)),
                _ => unreachable!("validated x86 scalar floating-point element type"),
            }
        };

        ctx.flags.materialize_all();
        ctx.flags.lazy = None;
        ctx.flags.materialized.of = false;
        ctx.flags.materialized.sf = false;
        ctx.flags.materialized.af = false;
        match ordering {
            None => {
                ctx.flags.materialized.zf = true;
                ctx.flags.materialized.pf = true;
                ctx.flags.materialized.cf = true;
            }
            Some(std::cmp::Ordering::Less) => {
                ctx.flags.materialized.zf = false;
                ctx.flags.materialized.pf = false;
                ctx.flags.materialized.cf = true;
            }
            Some(std::cmp::Ordering::Equal) => {
                ctx.flags.materialized.zf = true;
                ctx.flags.materialized.pf = false;
                ctx.flags.materialized.cf = false;
            }
            Some(std::cmp::Ordering::Greater) => {
                ctx.flags.materialized.zf = false;
                ctx.flags.materialized.pf = false;
                ctx.flags.materialized.cf = false;
            }
        }
    }
}
