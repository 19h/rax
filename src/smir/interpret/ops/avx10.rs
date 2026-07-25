//! AVX10 op execution (stubs)

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86OpHint, X86ThreeDNowKind, X86X87ArithmeticDestination,
    X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind,
    X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth, X86XSaveKind, x86_sat_fp_to_int_widths,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use std::cmp::Ordering;
use std::collections::HashMap;

impl SmirInterpreter {
    pub(crate) fn execute_op_avx10(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // AVX10 OPERATIONS (Stubs - not yet implemented in interpreter)
            // ==================================================================
            OpKind::VFma {
                dst,
                src1,
                src2,
                acc,
                elem,
                lanes,
                negate_product,
                negate_acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let c = Self::read_vec(ctx, *acc);
                let mut out = [0u64; 16];
                for lane in 0..*lanes {
                    let bits = match elem {
                        VecElementType::F32 => {
                            let mut av = f32::from_bits(Self::get_lane(&a, lane, 32) as u32);
                            let bv = f32::from_bits(Self::get_lane(&b, lane, 32) as u32);
                            let mut cv = f32::from_bits(Self::get_lane(&c, lane, 32) as u32);
                            if *negate_product {
                                av = -av;
                            }
                            if *negate_acc {
                                cv = -cv;
                            }
                            u64::from(av.mul_add(bv, cv).to_bits())
                        }
                        VecElementType::F64 => {
                            let mut av = f64::from_bits(Self::get_lane(&a, lane, 64));
                            let bv = f64::from_bits(Self::get_lane(&b, lane, 64));
                            let mut cv = f64::from_bits(Self::get_lane(&c, lane, 64));
                            if *negate_product {
                                av = -av;
                            }
                            if *negate_acc {
                                cv = -cv;
                            }
                            av.mul_add(bv, cv).to_bits()
                        }
                        _ => unreachable!("VFma requires F32 or F64 elements"),
                    };
                    Self::set_lane(&mut out, lane, elem.bytes() * 8, bits);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::X86Fma(fma) => {
                if !fma.shape_valid() || !matches!(ctx.arch_regs, ArchRegState::X86_64(_)) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let first = Self::read_vec(ctx, fma.src1);
                let second = Self::read_vec(ctx, fma.src2);
                let third = Self::read_vec(ctx, fma.src3);
                let active = fma.mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => unreachable!("x86 FMA architecture checked above"),
                };
                // Intel SDM Vol. 1 §15.6.4 defines SAE as if every MXCSR
                // exception mask were set. This affects the numeric masked
                // response as well as suppressing #XM/flag updates: notably,
                // MXCSR.FTZ applies to an underflow result even when the
                // architectural UM bit is clear.
                let arithmetic_mxcsr = if fma.round == FpRoundMode::Dynamic {
                    mxcsr
                } else {
                    mxcsr | (0x3F << 7)
                };
                let mode = match fma.round {
                    FpRoundMode::Dynamic => self.dynamic_fp_round_mode(ctx),
                    mode => mode,
                };
                let format = match fma.elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => unreachable!("x86 FMA shape checked above"),
                };
                let mut result = [0u64; 16];
                let mut status = 0u32;
                for lane in 0..fma.lanes {
                    if active & (1u64 << lane) == 0 {
                        continue;
                    }
                    let architectural = [
                        Self::get_lane(&first, lane, format.total_bits),
                        Self::get_lane(&second, lane, format.total_bits),
                        Self::get_lane(&third, lane, format.total_bits),
                    ];
                    // Intel SDM Vol. 1, Table 14-17 defines FMA3 NaN selection
                    // in x/y/z arithmetic order. AMD APM Volume 1, Table 4-5
                    // and Figure 4-49 establish the corresponding a/b/c source
                    // order for FMA4.
                    let ordered = match fma.order {
                        X86FmaOrder::Order123 => architectural,
                        X86FmaOrder::Order132 => {
                            [architectural[0], architectural[2], architectural[1]]
                        }
                        X86FmaOrder::Order213 => {
                            [architectural[1], architectural[0], architectural[2]]
                        }
                        X86FmaOrder::Order231 => {
                            [architectural[1], architectural[2], architectural[0]]
                        }
                    };
                    let negate_product = matches!(
                        fma.kind,
                        X86FmaKind::NegativeMultiplyAdd | X86FmaKind::NegativeMultiplySub
                    );
                    let negate_accumulator = match fma.kind {
                        X86FmaKind::Sub | X86FmaKind::NegativeMultiplySub => true,
                        X86FmaKind::AddSub => lane & 1 == 0,
                        X86FmaKind::SubAdd => lane & 1 != 0,
                        X86FmaKind::Add | X86FmaKind::NegativeMultiplyAdd => false,
                    };
                    let computed = Self::x86_fma_boundary(
                        ordered[0],
                        ordered[1],
                        ordered[2],
                        format,
                        negate_product,
                        negate_accumulator,
                        mode,
                        arithmetic_mxcsr,
                    );
                    status |= computed.status;
                    Self::set_lane(&mut result, lane, format.total_bits, computed.bits);
                }
                if fma.round == FpRoundMode::Dynamic {
                    // Intel SDM Vol. 1 §11.5.3 reports unmasked pre-
                    // computation exceptions before any post-computation
                    // exception. A handler can then mask/fix and restart the
                    // instruction to reach the post-computation boundary.
                    let pre_status = status & 0x07;
                    let reported_status = if Self::x86_simd_fp_unmasked(pre_status, mxcsr) {
                        pre_status
                    } else {
                        status
                    };
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= reported_status;
                    }
                    if Self::x86_simd_fp_unmasked(reported_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: op.guest_pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, fma.dst, result);
            }

            OpKind::X86FP16Fma {
                dst,
                src1,
                src2,
                src3,
                mask,
                kind,
                order,
                round,
                lanes,
            } => {
                if *order == X86FmaOrder::Order123 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let third = Self::read_vec(ctx, *src3);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = match round {
                    FpRoundMode::Dynamic => match (mxcsr >> 13) & 3 {
                        0 => FpRoundMode::RoundNearest,
                        1 => FpRoundMode::RoundDown,
                        2 => FpRoundMode::RoundUp,
                        _ => FpRoundMode::RoundTowardZero,
                    },
                    FpRoundMode::RoundNearest
                    | FpRoundMode::RoundDown
                    | FpRoundMode::RoundUp
                    | FpRoundMode::RoundTowardZero => *round,
                    FpRoundMode::RoundNearestTiesAway => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let sign = 0x8000u64;
                let mut result = [0u64; 16];
                let mut status = 0u32;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        continue;
                    }
                    let sources = [
                        Self::get_lane(&first, lane, 16),
                        Self::get_lane(&second, lane, 16),
                        Self::get_lane(&third, lane, 16),
                    ];
                    let (mut a, b, mut c) = match order {
                        X86FmaOrder::Order123 => (sources[0], sources[1], sources[2]),
                        X86FmaOrder::Order132 => (sources[0], sources[2], sources[1]),
                        X86FmaOrder::Order213 => (sources[1], sources[0], sources[2]),
                        X86FmaOrder::Order231 => (sources[1], sources[2], sources[0]),
                    };
                    let ordered_sources = [a, b, c];
                    let any_snan = ordered_sources
                        .iter()
                        .any(|bits| Self::x86_simd_fp_is_snan(*bits, X86_SIMD_F16));

                    let bits = if let Some(nan) = ordered_sources
                        .iter()
                        .copied()
                        .find(|bits| Self::x86_simd_fp_is_nan(*bits, X86_SIMD_F16))
                    {
                        // NaN handling has precedence over the lower-priority
                        // denormal condition for this lane.
                        status |= u32::from(any_snan);
                        Self::x86_simd_fp_quiet_nan(nan, X86_SIMD_F16)
                    } else {
                        if ordered_sources
                            .iter()
                            .any(|bits| Self::x86_simd_fp_is_denormal(*bits, X86_SIMD_F16))
                        {
                            status |= 1 << 1;
                        }
                        let negate_product = matches!(
                            kind,
                            X86FmaKind::NegativeMultiplyAdd | X86FmaKind::NegativeMultiplySub
                        );
                        let negate_acc = match kind {
                            X86FmaKind::Sub | X86FmaKind::NegativeMultiplySub => true,
                            X86FmaKind::AddSub => lane & 1 == 0,
                            X86FmaKind::SubAdd => lane & 1 != 0,
                            X86FmaKind::Add | X86FmaKind::NegativeMultiplyAdd => false,
                        };
                        if negate_product {
                            a ^= sign;
                        }
                        if negate_acc {
                            c ^= sign;
                        }
                        let computed =
                            Self::x86_simd_fp_fma_non_nan(a, b, c, X86_SIMD_F16, mode, mxcsr);
                        status |= computed.status;
                        computed.bits
                    };
                    Self::set_lane(&mut result, lane, 16, bits);
                }
                if *round == FpRoundMode::Dynamic {
                    let pre_status = status & 0x07;
                    let reported_status = if Self::x86_simd_fp_unmasked(pre_status, mxcsr) {
                        pre_status
                    } else {
                        status
                    };
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= reported_status;
                    }
                    if Self::x86_simd_fp_unmasked(reported_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: op.guest_pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FourFma {
                dst,
                src0,
                src1,
                src2,
                src3,
                mem,
                mask,
                scalar,
                negate_product,
                mask_zeroing,
            } => {
                // Snapshot every architectural input before computing: the
                // destination may be one of the four aligned source-block
                // registers.
                let old_dst = Self::read_vec(ctx, *dst);
                let sources = [
                    Self::read_vec(ctx, *src0),
                    Self::read_vec(ctx, *src1),
                    Self::read_vec(ctx, *src2),
                    Self::read_vec(ctx, *src3),
                ];
                let memory = Self::read_vec(ctx, *mem);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = match (mxcsr >> 13) & 3 {
                    0 => FpRoundMode::RoundNearest,
                    1 => FpRoundMode::RoundDown,
                    2 => FpRoundMode::RoundUp,
                    _ => FpRoundMode::RoundTowardZero,
                };
                let lanes = if *scalar { 1 } else { 16 };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    if active & (1u64 << lane) != 0 || !mask_zeroing {
                        Self::set_lane(&mut result, lane, 32, Self::get_lane(&old_dst, lane, 32));
                    }
                }
                if *scalar {
                    for lane in 1..4u8 {
                        Self::set_lane(&mut result, lane, 32, Self::get_lane(&old_dst, lane, 32));
                    }
                }

                // Intel specifies exception priority by FMA boundary. Compute
                // all lanes in one boundary, commit its sticky status, and
                // stop before the next boundary when any exception is
                // unmasked. The architectural destination is written only
                // after all four boundaries complete.
                for stage in 0..4u8 {
                    let multiplier = Self::get_lane(&memory, stage, 32);
                    let mut stage_status = 0u32;
                    for lane in 0..lanes {
                        if active & (1u64 << lane) == 0 {
                            continue;
                        }
                        let computed = Self::x86_f32_fma_boundary(
                            Self::get_lane(&sources[stage as usize], lane, 32),
                            multiplier,
                            Self::get_lane(&result, lane, 32),
                            *negate_product,
                            mode,
                            mxcsr,
                        );
                        stage_status |= computed.status;
                        Self::set_lane(&mut result, lane, 32, computed.bits);
                    }
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= stage_status;
                    }
                    if Self::x86_simd_fp_unmasked(stage_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FourDotProduct {
                dst,
                src0,
                src1,
                src2,
                src3,
                mem,
                mask,
                saturating,
                mask_zeroing,
            } => {
                // Snapshot the destination and aligned source block before any
                // result is produced because the destination may alias a
                // source register used by a later iteration.
                let old_dst = Self::read_vec(ctx, *dst);
                let sources = [
                    Self::read_vec(ctx, *src0),
                    Self::read_vec(ctx, *src1),
                    Self::read_vec(ctx, *src2),
                    Self::read_vec(ctx, *src3),
                ];
                let memory = Self::read_vec(ctx, *mem);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mut result = [0u64; 16];

                for lane in 0..16u8 {
                    if active & (1u64 << lane) == 0 {
                        if !mask_zeroing {
                            Self::set_lane(
                                &mut result,
                                lane,
                                32,
                                Self::get_lane(&old_dst, lane, 32),
                            );
                        }
                        continue;
                    }

                    let mut accumulator =
                        i64::from(Self::get_lane(&old_dst, lane, 32) as u32 as i32);
                    for stage in 0..4u8 {
                        let memory_word = Self::get_lane(&memory, stage, 32) as u32;
                        let memory_low = i64::from(memory_word as u16 as i16);
                        let memory_high = i64::from((memory_word >> 16) as u16 as i16);
                        let source_low = i64::from(Self::get_lane(
                            &sources[stage as usize],
                            lane * 2,
                            16,
                        ) as u16 as i16);
                        let source_high =
                            i64::from(Self::get_lane(&sources[stage as usize], lane * 2 + 1, 16)
                                as u16 as i16);
                        let sum = accumulator + source_low * memory_low + source_high * memory_high;
                        accumulator = if *saturating {
                            sum.clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                        } else {
                            i64::from(sum as i32)
                        };
                    }
                    Self::set_lane(&mut result, lane, 32, accumulator as u32 as u64);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86FP16Complex {
                dst,
                src1,
                src2,
                mask,
                width: _,
                pairs,
                scalar,
                mask_zeroing,
                accumulate,
                conjugate,
                round,
            } => {
                let old_dst = Self::read_vec(ctx, *dst);
                let left = Self::read_vec(ctx, *src1);
                let right = Self::read_vec(ctx, *src2);
                let active = mask.map_or(u64::MAX, |mask| ctx.read_vreg(mask));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = match round {
                    FpRoundMode::Dynamic => match (mxcsr >> 13) & 3 {
                        0 => FpRoundMode::RoundNearest,
                        1 => FpRoundMode::RoundDown,
                        2 => FpRoundMode::RoundUp,
                        _ => FpRoundMode::RoundTowardZero,
                    },
                    FpRoundMode::RoundNearest
                    | FpRoundMode::RoundDown
                    | FpRoundMode::RoundUp
                    | FpRoundMode::RoundTowardZero => *round,
                    FpRoundMode::RoundNearestTiesAway => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let mut result = [0u64; 16];
                if *scalar {
                    for lane in 2..8 {
                        Self::set_lane(&mut result, lane, 16, Self::get_lane(&left, lane, 16));
                    }
                }

                let mut status = 0u32;
                for pair in 0..*pairs {
                    let real_lane = pair * 2;
                    let imag_lane = real_lane + 1;
                    if active & (1u64 << pair) == 0 {
                        if !mask_zeroing {
                            Self::set_lane(
                                &mut result,
                                real_lane,
                                16,
                                Self::get_lane(&old_dst, real_lane, 16),
                            );
                            Self::set_lane(
                                &mut result,
                                imag_lane,
                                16,
                                Self::get_lane(&old_dst, imag_lane, 16),
                            );
                        }
                        continue;
                    }

                    let ar = Self::get_lane(&left, real_lane, 16);
                    let ai = Self::get_lane(&left, imag_lane, 16);
                    let br = Self::get_lane(&right, real_lane, 16);
                    let bi = Self::get_lane(&right, imag_lane, 16);
                    let (tmp_real, tmp_imag) = if *accumulate {
                        let dr = Self::get_lane(&old_dst, real_lane, 16);
                        let di = Self::get_lane(&old_dst, imag_lane, 16);
                        (
                            Self::x86_fp16_fma_boundary(ar, br, dr, false, mode, mxcsr),
                            Self::x86_fp16_fma_boundary(ai, br, di, false, mode, mxcsr),
                        )
                    } else {
                        (
                            Self::x86_simd_fp_mul(
                                ar,
                                br,
                                X86_SIMD_F16,
                                mode,
                                mxcsr & !((1 << 6) | (1 << 15)),
                            ),
                            Self::x86_simd_fp_mul(
                                ai,
                                br,
                                X86_SIMD_F16,
                                mode,
                                mxcsr & !((1 << 6) | (1 << 15)),
                            ),
                        )
                    };
                    status |= tmp_real.status | tmp_imag.status;
                    let real = Self::x86_fp16_fma_boundary(
                        ai,
                        bi,
                        tmp_real.bits,
                        !*conjugate,
                        mode,
                        mxcsr,
                    );
                    let imag =
                        Self::x86_fp16_fma_boundary(ar, bi, tmp_imag.bits, *conjugate, mode, mxcsr);
                    status |= real.status | imag.status;
                    Self::set_lane(&mut result, real_lane, 16, real.bits);
                    Self::set_lane(&mut result, imag_lane, 16, imag.bits);
                }
                if *round == FpRoundMode::Dynamic {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPermute {
                dst,
                src1,
                src2,
                indices,
                elem,
                width,
                ..
            } => {
                let table1 = Self::read_vec(ctx, *src1);
                let table2 = src2.map(|reg| Self::read_vec(ctx, reg));
                let controls = Self::read_vec(ctx, *indices);
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let table_lanes = u64::from(lanes) * if table2.is_some() { 2 } else { 1 };
                debug_assert!(table_lanes.is_power_of_two());
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let selected = Self::get_lane(&controls, lane, bits) & (table_lanes - 1);
                    let value = if selected < u64::from(lanes) {
                        Self::get_lane(&table1, selected as u8, bits)
                    } else {
                        Self::get_lane(
                            table2.as_ref().expect("second permute table"),
                            (selected - u64::from(lanes)) as u8,
                            bits,
                        )
                    };
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PermuteBytesWords {
                dst,
                table1,
                table2,
                indices,
                mask,
                elem,
                width,
                zeroing,
                ..
            } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *table1);
                let second = table2.map(|reg| Self::read_vec(ctx, reg));
                let controls = Self::read_vec(ctx, *indices);
                let lanes = width.lanes(*elem) as u8;
                let bits = elem.bytes() * 8;
                let table_lanes = u64::from(lanes) * if second.is_some() { 2 } else { 1 };
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let selected = Self::get_lane(&controls, lane, bits) & (table_lanes - 1);
                    let value = if selected < u64::from(lanes) {
                        Self::get_lane(&first, selected as u8, bits)
                    } else {
                        Self::get_lane(
                            second.as_ref().expect("second permute table"),
                            (selected - u64::from(lanes)) as u8,
                            bits,
                        )
                    };
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPopcnt {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(*elem) as u8 {
                    let count = Self::get_lane(&input, lane, bits).count_ones();
                    Self::set_lane(&mut result, lane, bits, u64::from(count));
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VConflict {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let lanes = width.lanes(*elem) as u8;
                let mut result = [0u64; 16];
                for lane in 0..lanes {
                    let value = Self::get_lane(&input, lane, bits);
                    let mut conflicts = 0u64;
                    for previous in 0..lane {
                        if Self::get_lane(&input, previous, bits) == value {
                            conflicts |= 1u64 << previous;
                        }
                    }
                    Self::set_lane(&mut result, lane, bits, conflicts);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLeadingZeros {
                dst,
                src,
                mask,
                elem,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let input = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(*elem) as u8 {
                    let value = Self::get_lane(&input, lane, bits);
                    let count = if bits == 32 {
                        (value as u32).leading_zeros()
                    } else {
                        value.leading_zeros()
                    };
                    Self::set_lane(&mut result, lane, bits, u64::from(count));
                }
                Self::apply_vector_mask(
                    &mut result,
                    &old,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    *elem,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VCvtFP32ToBF16 {
                dst,
                src1,
                src2,
                mask,
                width,
                zeroing,
            } => {
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = src2.map(|reg| Self::read_vec(ctx, reg));
                let fp32_lanes = width.lanes(VecElementType::F32) as u8;
                let mut result = [0u64; 16];
                if let Some(second) = second {
                    for lane in 0..fp32_lanes {
                        let low = Self::get_lane(&second, lane, 32) as u32;
                        let high = Self::get_lane(&first, lane, 32) as u32;
                        Self::set_lane(
                            &mut result,
                            lane,
                            16,
                            u64::from(Self::x86_fp32_to_bf16_bits(low)),
                        );
                        Self::set_lane(
                            &mut result,
                            lane + fp32_lanes,
                            16,
                            u64::from(Self::x86_fp32_to_bf16_bits(high)),
                        );
                    }
                } else {
                    for lane in 0..fp32_lanes {
                        let input = Self::get_lane(&first, lane, 32) as u32;
                        Self::set_lane(
                            &mut result,
                            lane,
                            16,
                            u64::from(Self::x86_fp32_to_bf16_bits(input)),
                        );
                    }
                }
                if let Some(mask_bits) = mask.map(|mask| ctx.read_vreg(mask)) {
                    let result_lanes = if second.is_some() {
                        fp32_lanes * 2
                    } else {
                        fp32_lanes
                    };
                    for lane in 0..result_lanes {
                        if mask_bits & (1u64 << lane) == 0 {
                            let inactive = if *zeroing {
                                0
                            } else {
                                Self::get_lane(&old, lane, 16)
                            };
                            Self::set_lane(&mut result, lane, 16, inactive);
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VDotProductBF16 {
                dst,
                acc,
                src1,
                src2,
                mask,
                width,
                zeroing,
            } => {
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(VecElementType::F32) as u8 {
                    let acc_bits = Self::get_lane(&accumulator, lane, 32) as u32;
                    let a_low = Self::get_lane(&first, lane * 2, 16) as u16;
                    let a_high = Self::get_lane(&first, lane * 2 + 1, 16) as u16;
                    let b_low = Self::get_lane(&second, lane * 2, 16) as u16;
                    let b_high = Self::get_lane(&second, lane * 2 + 1, 16) as u16;

                    // Intel Table 5-4 overrides evaluation order for NaN
                    // propagation: low pair, high pair, then accumulator.
                    let nan = [a_low, b_low, a_high, b_high]
                        .into_iter()
                        .find_map(|value| {
                            Self::x86_bf16_is_nan(value).then(|| Self::x86_bf16_quiet_nan(value))
                        })
                        .or_else(|| {
                            Self::x86_simd_fp_is_nan(u64::from(acc_bits), X86_SIMD_F32).then(|| {
                                Self::x86_simd_fp_quiet_nan(u64::from(acc_bits), X86_SIMD_F32)
                                    as u32
                            })
                        });
                    let value = if let Some(nan) = nan {
                        nan
                    } else {
                        let acc = Self::x86_fp32_ftz(acc_bits);
                        let high = f32::from_bits(Self::x86_bf16_to_fp32_daz(a_high)).mul_add(
                            f32::from_bits(Self::x86_bf16_to_fp32_daz(b_high)),
                            f32::from_bits(acc),
                        );
                        let high_bits = if high.is_nan() {
                            0xFFC0_0000
                        } else {
                            Self::x86_fp32_ftz(high.to_bits())
                        };
                        let low = f32::from_bits(Self::x86_bf16_to_fp32_daz(a_low)).mul_add(
                            f32::from_bits(Self::x86_bf16_to_fp32_daz(b_low)),
                            f32::from_bits(high_bits),
                        );
                        if low.is_nan() {
                            0xFFC0_0000
                        } else {
                            Self::x86_fp32_ftz(low.to_bits())
                        }
                    };
                    Self::set_lane(&mut result, lane, 32, u64::from(value));
                }
                Self::apply_vector_mask(
                    &mut result,
                    &accumulator,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::F32,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VMultiplyAdd52 {
                dst,
                acc,
                src1,
                src2,
                mask,
                width,
                high,
                zeroing,
            } => {
                const MASK52: u64 = (1u64 << 52) - 1;
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for lane in 0..width.lanes(VecElementType::I64) as u8 {
                    let lhs = Self::get_lane(&first, lane, 64) & MASK52;
                    let rhs = Self::get_lane(&second, lane, 64) & MASK52;
                    let product = u128::from(lhs) * u128::from(rhs);
                    let addend = if *high {
                        ((product >> 52) as u64) & MASK52
                    } else {
                        product as u64 & MASK52
                    };
                    let value = Self::get_lane(&accumulator, lane, 64).wrapping_add(addend);
                    Self::set_lane(&mut result, lane, 64, value);
                }
                Self::apply_vector_mask(
                    &mut result,
                    &accumulator,
                    mask.map(|mask| ctx.read_vreg(mask)),
                    *zeroing,
                    *width,
                    VecElementType::I64,
                );
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VDotProductExt {
                dst,
                acc,
                src1,
                src2,
                src_elem,
                acc_elem,
                width,
                src1_signed,
                src2_signed,
                saturate,
            } => {
                debug_assert!(matches!(src_elem, VecElementType::I8 | VecElementType::I16));
                debug_assert_eq!(*acc_elem, VecElementType::I32);

                // Snapshot all operands before the architectural accumulator is
                // overwritten. VEX dot products permit dst to alias either
                // multiplicand as well as the implicit accumulator.
                let accumulator = Self::read_vec(ctx, *acc);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let src_bits = src_elem.bytes() * 8;
                let terms = 32 / src_bits;
                let lanes = width.lanes(VecElementType::I32) as u8;
                let src_mask = (1u64 << src_bits) - 1;
                let sign_extend = |value: u64| -> i128 {
                    let shift = 128 - src_bits;
                    (i128::from(value) << shift) >> shift
                };
                let unsigned_result = !src1_signed && !src2_signed;
                let mut result = [0u64; 16];

                for lane in 0..lanes {
                    let acc_raw = Self::get_lane(&accumulator, lane, 32) as u32;
                    let mut sum = if unsigned_result {
                        i128::from(acc_raw)
                    } else {
                        i128::from(acc_raw as i32)
                    };
                    let first_term = u32::from(lane) * terms;
                    for term in 0..terms {
                        let source_lane = (first_term + term) as u8;
                        let a_raw = Self::get_lane(&first, source_lane, src_bits) & src_mask;
                        let b_raw = Self::get_lane(&second, source_lane, src_bits) & src_mask;
                        let a = if *src1_signed {
                            sign_extend(a_raw)
                        } else {
                            i128::from(a_raw)
                        };
                        let b = if *src2_signed {
                            sign_extend(b_raw)
                        } else {
                            i128::from(b_raw)
                        };
                        sum += a * b;
                    }
                    let value = if *saturate {
                        if unsigned_result {
                            sum.clamp(0, i128::from(u32::MAX)) as u32
                        } else {
                            sum.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32 as u32
                        }
                    } else {
                        sum as u32
                    };
                    Self::set_lane(&mut result, lane, 32, u64::from(value));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VFP16Arith {
                dst,
                src1,
                src2,
                mask,
                op,
                round,
                width,
                lanes,
                zeroing,
            } => {
                if *lanes == 0 || *lanes > width.lanes(VecElementType::F16) as u8 {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let old = Self::read_vec(ctx, *dst);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let rounding = match round {
                    FpRoundMode::Dynamic => match (mxcsr >> 13) & 3 {
                        0 => FpRoundMode::RoundNearest,
                        1 => FpRoundMode::RoundDown,
                        2 => FpRoundMode::RoundUp,
                        _ => FpRoundMode::RoundTowardZero,
                    },
                    FpRoundMode::RoundNearest
                    | FpRoundMode::RoundDown
                    | FpRoundMode::RoundUp
                    | FpRoundMode::RoundTowardZero => *round,
                    FpRoundMode::RoundNearestTiesAway => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let mask_bits = mask.map(|mask| ctx.read_vreg(mask));
                let mut result = [0u64; 16];
                let mut status = 0u32;
                // AVX-512-FP16 arithmetic consumes binary16 denormals and
                // produces gradual-underflow results independently of
                // MXCSR.DAZ/FTZ. Preserve every other MXCSR control bit.
                let fp16_mxcsr = mxcsr & !((1 << 6) | (1 << 15));
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*zeroing {
                            Self::set_lane(&mut result, lane, 16, Self::get_lane(&old, lane, 16));
                        }
                        continue;
                    }
                    let a_bits = Self::get_lane(&first, lane, 16) as u16;
                    let b_bits = Self::get_lane(&second, lane, 16) as u16;
                    let computed = match op {
                        Avx10FP16Op::Min | Avx10FP16Op::Max => {
                            // AVX512-FP16 always handles denormal FP16 inputs;
                            // MXCSR.DAZ is ignored, but the denormal-operand
                            // exception remains architecturally observable.
                            let mut lane_status = 0u32;
                            if Self::x86_simd_fp_is_denormal(u64::from(a_bits), X86_SIMD_F16)
                                || Self::x86_simd_fp_is_denormal(u64::from(b_bits), X86_SIMD_F16)
                            {
                                lane_status |= 1 << 1;
                            }
                            if Self::x86_simd_fp_is_snan(u64::from(a_bits), X86_SIMD_F16)
                                || Self::x86_simd_fp_is_snan(u64::from(b_bits), X86_SIMD_F16)
                            {
                                lane_status |= 1;
                            }
                            // Intel MIN/MAX selects source 2 for unordered or
                            // equal operands. Preserve the selected FP16 bits
                            // exactly, including an SNaN in source 2.
                            let a = Self::x86_fp16_to_f32(a_bits);
                            let b = Self::x86_fp16_to_f32(b_bits);
                            X86SimdFpResult {
                                bits: u64::from(
                                    if (*op == Avx10FP16Op::Min && a < b)
                                        || (*op == Avx10FP16Op::Max && a > b)
                                    {
                                        a_bits
                                    } else {
                                        b_bits
                                    },
                                ),
                                status: lane_status,
                            }
                        }
                        Avx10FP16Op::Add => Self::x86_simd_fp_add(
                            u64::from(a_bits),
                            u64::from(b_bits),
                            X86_SIMD_F16,
                            rounding,
                            fp16_mxcsr,
                        ),
                        Avx10FP16Op::Sub => Self::x86_simd_fp_sub(
                            u64::from(a_bits),
                            u64::from(b_bits),
                            X86_SIMD_F16,
                            rounding,
                            fp16_mxcsr,
                        ),
                        Avx10FP16Op::Mul => Self::x86_simd_fp_mul(
                            u64::from(a_bits),
                            u64::from(b_bits),
                            X86_SIMD_F16,
                            rounding,
                            fp16_mxcsr,
                        ),
                        Avx10FP16Op::Div => Self::x86_simd_fp_div(
                            u64::from(a_bits),
                            u64::from(b_bits),
                            X86_SIMD_F16,
                            rounding,
                            fp16_mxcsr,
                        ),
                        Avx10FP16Op::Sqrt => Self::x86_simd_fp_sqrt(
                            u64::from(b_bits),
                            X86_SIMD_F16,
                            rounding,
                            fp16_mxcsr,
                        ),
                    };
                    status |= computed.status;
                    Self::set_lane(&mut result, lane, 16, computed.bits);
                }
                if *round == FpRoundMode::Dynamic && status != 0 {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VCvtFpToIntSat {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                width,
                signed,
                truncate,
                round,
                zeroing,
                suppress_exceptions,
            } => {
                let widths = x86_sat_fp_to_int_widths(*fp_elem, *int_elem, *width, *truncate);
                let encoded_width = widths.map(|(_, encoded_width)| encoded_width);
                let canonical_rounding = *round != FpRoundMode::RoundNearestTiesAway
                    && if *truncate {
                        *round == FpRoundMode::RoundTowardZero
                            && (!*suppress_exceptions || encoded_width == Some(VecWidth::V512))
                    } else {
                        matches!(
                            (*round, *suppress_exceptions),
                            (FpRoundMode::Dynamic, false)
                        ) || (*round != FpRoundMode::Dynamic
                            && *suppress_exceptions
                            && encoded_width == Some(VecWidth::V512))
                    };
                let canonical_shape =
                    encoded_width.is_some() && canonical_rounding && (!*zeroing || mask.is_some());
                if !canonical_shape || !matches!(ctx.arch_regs, ArchRegState::X86_64(_)) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: op.guest_pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let format = match fp_elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => unreachable!("canonical shape checked above"),
                };
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                result[(width.bytes() / 8) as usize..].fill(0);
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => unreachable!("x86 state checked above"),
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    match (mxcsr >> 13) & 3 {
                        0 => FpRoundMode::RoundNearest,
                        1 => FpRoundMode::RoundDown,
                        2 => FpRoundMode::RoundUp,
                        _ => FpRoundMode::RoundTowardZero,
                    }
                } else {
                    *round
                };
                let src_lane_bits = fp_elem.bytes() * 8;
                let int_bits = int_elem.bytes() * 8;
                let dst_lane_bits = if *int_elem == VecElementType::I8 {
                    32
                } else {
                    int_bits
                };
                let (src_width, _) = widths.expect("canonical conversion shape checked above");
                let lanes = src_width.lanes(*fp_elem) as u8;
                let mut status = 0;
                for lane in 0..lanes {
                    if active & (1u64 << lane) == 0 {
                        if *zeroing {
                            Self::set_lane(&mut result, lane, dst_lane_bits, 0);
                        }
                        continue;
                    }

                    let mut source_bits = Self::get_lane(&source, lane, src_lane_bits);
                    // These instructions report only Invalid and Precision.
                    // DAZ substitutes signed zero; a preserved denormal is not
                    // itself a Denormal exception for this instruction class.
                    if mxcsr & (1 << 6) != 0 && Self::x86_simd_fp_is_denormal(source_bits, format) {
                        let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
                        source_bits &= sign;
                    }
                    let converted =
                        Self::x86_simd_fp_to_int_sat(source_bits, format, int_bits, *signed, mode);
                    status |= converted.status;
                    Self::set_lane(
                        &mut result,
                        lane,
                        dst_lane_bits,
                        converted.bits
                            & if int_bits == 64 {
                                u64::MAX
                            } else {
                                (1u64 << int_bits) - 1
                            },
                    );
                }

                if !*suppress_exceptions {
                    // Invalid is detected in the pre-computation phase;
                    // Precision is post-computation. An unmasked IE therefore
                    // faults before PE from any lane can be accrued.
                    let pre_status = status & 1;
                    let reported_status = if Self::x86_simd_fp_unmasked(pre_status, mxcsr) {
                        pre_status
                    } else {
                        status
                    };
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= reported_status;
                    }
                    if Self::x86_simd_fp_unmasked(reported_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VMin { .. } | OpKind::VCvtBF16ToFP32 { .. } | OpKind::VMinMax { .. } => {
                // AVX10 operations not yet implemented in interpreter
                // These would require full vector register state tracking
                ctx.request_exit(ExitReason::Undefined {
                    addr: ctx.pc,
                    opcode: 0,
                });
            }
            _ => unreachable!("execute_op: unhandled OpKind"),
        }

        Ok(())
    }
}
