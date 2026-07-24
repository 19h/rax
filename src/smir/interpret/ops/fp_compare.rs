//! x86 scalar floating-point comparisons that write RFLAGS.

use crate::smir::interpret::{
    SmirInterpreter, X86_SIMD_F16, X86_SIMD_F32, X86_SIMD_F64, X86SimdFpResult,
};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::types::{VReg, VecElementType, VecWidth};

impl SmirInterpreter {
    /// Execute x86 packed/scalar vector comparisons with per-lane NaN
    /// precedence and precise aggregate MXCSR status. Complexity is O(L) time
    /// for L active lanes and O(1) auxiliary space.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_x86_vector_fp_compare(
        &self,
        ctx: &mut SmirContext,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        mask: Option<VReg>,
        elem: VecElementType,
        width: VecWidth,
        lanes: u8,
        predicate: u8,
        scalar: bool,
        mask_destination: bool,
        zero_upper: bool,
        suppress_exceptions: bool,
    ) {
        let first = Self::read_vec(ctx, src1);
        let second = Self::read_vec(ctx, src2);
        let mxcsr = match &ctx.arch_regs {
            ArchRegState::X86_64(x86) => x86.mxcsr,
            _ => 0x1F80,
        };
        let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
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
        let signaling = matches!(
            predicate & 0x1F,
            1 | 2 | 5 | 6 | 9 | 10 | 13 | 14 | 16 | 19 | 20 | 23 | 24 | 27 | 28 | 31
        );
        let mut status = 0u32;
        let mut mask_result = 0u64;
        let mut vector_result = if scalar {
            first
        } else {
            Self::read_vec(ctx, dst)
        };
        if zero_upper && !mask_destination {
            vector_result[(width.bytes() / 8) as usize..].fill(0);
        }
        for lane in 0..lanes {
            if active & (1u64 << lane) == 0 {
                continue;
            }
            let first_raw = Self::get_lane(&first, lane, elem.bytes() * 8);
            let second_raw = Self::get_lane(&second, lane, elem.bytes() * 8);
            // AVX-512-FP16 comparisons always consume binary16 denormals:
            // MXCSR.DAZ is ignored, while an active denormal operand reports
            // MXCSR.DE unless NaN handling takes precedence for this lane.
            let value = |raw| {
                if elem == VecElementType::F16 {
                    X86SimdFpResult {
                        bits: raw,
                        status: u32::from(Self::x86_simd_fp_is_denormal(raw, format)) << 1,
                    }
                } else {
                    Self::x86_simd_fp_apply_daz(raw, format, mxcsr)
                }
            };
            let first_value = value(first_raw);
            let second_value = value(second_raw);
            let first_nan = Self::x86_simd_fp_is_nan(first_value.bits, format);
            let second_nan = Self::x86_simd_fp_is_nan(second_value.bits, format);
            let invalid = Self::x86_simd_fp_is_snan(first_value.bits, format)
                || Self::x86_simd_fp_is_snan(second_value.bits, format)
                || (signaling && (first_nan || second_nan));

            if first_nan || second_nan {
                // Intel SDM Vol. 1 D.4.2.2: NaN handling has precedence over
                // every non-invalid floating-point exception. Suppress DE for
                // this lane even when the other operand is denormal; other
                // active non-NaN lanes can still contribute DE independently.
                status |= u32::from(invalid);
            } else {
                status |= first_value.status | second_value.status;
            }

            let relation = if first_nan || second_nan {
                3
            } else {
                let ordering = match elem {
                    VecElementType::F16 => Self::x86_fp16_to_f32(first_value.bits as u16)
                        .partial_cmp(&Self::x86_fp16_to_f32(second_value.bits as u16)),
                    VecElementType::F32 => f32::from_bits(first_value.bits as u32)
                        .partial_cmp(&f32::from_bits(second_value.bits as u32)),
                    VecElementType::F64 => f64::from_bits(first_value.bits)
                        .partial_cmp(&f64::from_bits(second_value.bits)),
                    _ => unreachable!(),
                };
                match ordering {
                    Some(std::cmp::Ordering::Greater) => 0,
                    Some(std::cmp::Ordering::Less) => 1,
                    Some(std::cmp::Ordering::Equal) => 2,
                    None => 3,
                }
            };
            const TRUTH_TABLES: [u8; 16] = [
                0b0100, 0b0010, 0b0110, 0b1000, 0b1011, 0b1101, 0b1001, 0b0111, 0b1100, 0b1010,
                0b1110, 0b0000, 0b0011, 0b0101, 0b0001, 0b1111,
            ];
            let is_true = TRUTH_TABLES[usize::from(predicate & 0x0F)] & (1u8 << relation) != 0;
            if mask_destination {
                if is_true {
                    mask_result |= 1u64 << lane;
                }
            } else {
                Self::set_lane(
                    &mut vector_result,
                    lane,
                    elem.bytes() * 8,
                    if is_true {
                        if elem == VecElementType::F32 {
                            u64::from(u32::MAX)
                        } else {
                            u64::MAX
                        }
                    } else {
                        0
                    },
                );
            }
        }
        if !suppress_exceptions {
            if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                x86.mxcsr |= status;
            }
            if Self::x86_simd_fp_unmasked(status, mxcsr) {
                ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                return;
            }
        }
        if mask_destination {
            ctx.write_vreg(dst, mask_result);
        } else {
            Self::write_vec(ctx, dst, vector_result);
        }
    }

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
