//! Floating-point op execution

use crate::smir::interpret::*;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::{FlagSet, FlagUpdate, LazyFlagOp, LazyFlags};
use crate::smir::ir::memory::{MemoryError, SmirMemory};
use crate::smir::ir::ops::{
    HexFpOp, HexFpRecipKind, OpKind, RvVectorState, SmirOp, X86AdxKind, X86BlsKind,
    X86CacheControlKind, X86CountKind, X86OpHint, X86ThreeDNowKind, X86X87ArithmeticDestination,
    X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant, X86X87ControlKind, X86X87DataKind,
    X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth, X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{CallTarget, SmirBlock, SmirFunction, Terminator, TrapKind};
use std::cmp::Ordering;
use std::collections::HashMap;

impl SmirInterpreter {
    pub(crate) fn execute_op_fp(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // FLOATING POINT
            // ==================================================================
            OpKind::FAdd {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a + b, *precision);
            }

            OpKind::FSub {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a - b, *precision);
            }

            OpKind::FMul {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a * b, *precision);
            }

            OpKind::FDiv {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a / b, *precision);
            }

            OpKind::FFma {
                dst,
                src1,
                src2,
                src3,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                let c = self.read_fp(ctx, *src3, *precision);
                self.write_fp(ctx, *dst, a.mul_add(b, c), *precision);
            }

            OpKind::FAbs {
                dst,
                src,
                precision,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, a.abs(), *precision);
            }

            OpKind::FNeg {
                dst,
                src,
                precision,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, -a, *precision);
            }

            OpKind::FSqrt {
                dst,
                src,
                precision,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, a.sqrt(), *precision);
            }

            OpKind::FMin {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a.min(b), *precision);
            }

            OpKind::FMax {
                dst,
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                self.write_fp(ctx, *dst, a.max(b), *precision);
            }

            OpKind::FCmp {
                src1,
                src2,
                precision,
            } => {
                let a = self.read_fp(ctx, *src1, *precision);
                let b = self.read_fp(ctx, *src2, *precision);
                // Set flags based on comparison
                let result = if a < b {
                    u64::MAX
                } else if a > b {
                    1
                } else {
                    0
                };
                ctx.flags.set_lazy_sub(
                    if a >= b { 1 } else { 0 },
                    if a <= b { 1 } else { 0 },
                    result,
                    OpWidth::W64,
                );
            }

            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling,
                suppress_exceptions,
            } => {
                self.execute_x86_fp_compare(
                    ctx,
                    *src1,
                    *src2,
                    *elem,
                    *signaling,
                    *suppress_exceptions,
                );
            }

            OpKind::X86VectorFpCompare {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                predicate,
                scalar,
                mask_destination,
                zero_upper,
                suppress_exceptions,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
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
                        return Ok(());
                    }
                };
                let signaling = matches!(
                    *predicate & 0x1F,
                    1 | 2 | 5 | 6 | 9 | 10 | 13 | 14 | 16 | 19 | 20 | 23 | 24 | 27 | 28 | 31
                );
                let mut status = 0u32;
                let mut mask_result = 0u64;
                let mut vector_result = if *scalar {
                    first
                } else {
                    Self::read_vec(ctx, *dst)
                };
                if *zero_upper && !*mask_destination {
                    vector_result[(width.bytes() / 8) as usize..].fill(0);
                }
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        continue;
                    }
                    let first_raw = Self::get_lane(&first, lane, elem.bytes() * 8);
                    let second_raw = Self::get_lane(&second, lane, elem.bytes() * 8);
                    let first_value = Self::x86_simd_fp_apply_daz(first_raw, format, mxcsr);
                    let second_value = Self::x86_simd_fp_apply_daz(second_raw, format, mxcsr);
                    status |= first_value.status | second_value.status;
                    let first_nan = Self::x86_simd_fp_is_nan(first_value.bits, format);
                    let second_nan = Self::x86_simd_fp_is_nan(second_value.bits, format);
                    if Self::x86_simd_fp_is_snan(first_value.bits, format)
                        || Self::x86_simd_fp_is_snan(second_value.bits, format)
                        || (signaling && (first_nan || second_nan))
                    {
                        status |= 1;
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
                        0b0100, 0b0010, 0b0110, 0b1000, 0b1011, 0b1101, 0b1001, 0b0111, 0b1100,
                        0b1010, 0b1110, 0b0000, 0b0011, 0b0101, 0b0001, 0b1111,
                    ];
                    let is_true =
                        TRUTH_TABLES[usize::from(*predicate & 0x0F)] & (1u8 << relation) != 0;
                    if *mask_destination {
                        if is_true {
                            mask_result |= 1u64 << lane;
                        }
                    } else {
                        Self::set_lane(
                            &mut vector_result,
                            lane,
                            elem.bytes() * 8,
                            if is_true {
                                if *elem == VecElementType::F32 {
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
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    if Self::x86_simd_fp_unmasked(status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                if *mask_destination {
                    ctx.write_vreg(*dst, mask_result);
                } else {
                    Self::write_vec(ctx, *dst, vector_result);
                }
            }

            OpKind::X86GetExponent {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_get_exponent(source_bits, format, mxcsr);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86GetMantissa {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_get_mantissa(source_bits, format, mxcsr, *imm);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86RoundScale {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_round_scale(source_bits, format, mxcsr, *imm);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86Reduce {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = Self::x86_simd_reduce(source_bits, format, mxcsr, *imm);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86Range {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *imm > 0x0F
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if *scalar { first } else { old };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted = Self::x86_simd_range(
                        Self::get_lane(&first, lane, elem_bits),
                        Self::get_lane(&second, lane, elem_bits),
                        format,
                        mxcsr,
                        *imm,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86FixupImm {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let first = Self::read_vec(ctx, *src1);
                let table = Self::read_vec(ctx, *src2);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if *scalar { first } else { old };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted = Self::x86_simd_fixup_imm(
                        Self::get_lane(&old, lane, elem_bits),
                        Self::get_lane(&first, lane, elem_bits),
                        Self::get_lane(&table, lane, elem_bits),
                        format,
                        mxcsr,
                        *imm,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                // VFIXUPIMM treats MXCSR exception masks as set: reporting can
                // update the IE/ZE sticky flags but never raises #XM.
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Exp2 {
                dst,
                src,
                mask,
                elem,
                width,
                lanes,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *width != VecWidth::V512
                    || *lanes != width.lanes(*elem) as u8
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        }
                        continue;
                    }
                    let converted =
                        Self::x86_simd_exp2(Self::get_lane(&source, lane, elem_bits), format);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86Recip14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86Rsqrt14 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86Rsqrt14 { .. });
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (!matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                            || *lanes != width.lanes(*elem) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let bits = Self::get_lane(&source, lane, elem_bits);
                    let converted = if rsqrt {
                        Self::x86_simd_rsqrt14(bits, format, mxcsr)
                    } else {
                        Self::x86_simd_recip14(bits, format, mxcsr)
                    };
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86RecipFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
            | OpKind::X86RsqrtFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            } => {
                let rsqrt = matches!(op.kind, OpKind::X86RsqrtFp16 { .. });
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (!matches!(width, VecWidth::V128 | VecWidth::V256 | VecWidth::V512)
                            || *lanes != width.lanes(VecElementType::F16) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        let inactive = if *mask_zeroing {
                            0
                        } else {
                            Self::get_lane(&old, lane, 16)
                        };
                        Self::set_lane(&mut result, lane, 16, inactive);
                        continue;
                    }
                    let bits = Self::get_lane(&source, lane, 16) as u16;
                    Self::set_lane(
                        &mut result,
                        lane,
                        16,
                        u64::from(Self::x86_fp16_approx(bits, rsqrt)),
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86Recip28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (*width != VecWidth::V512 || *lanes != width.lanes(*elem) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted =
                        Self::x86_simd_recip28(Self::get_lane(&source, lane, elem_bits), format);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86Rsqrt28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if *scalar != merge.is_some()
                    || (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar
                        && (*width != VecWidth::V512 || *lanes != width.lanes(*elem) as u8))
                    || (*mask_zeroing && mask.is_none())
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if let Some(merge) = merge {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *scalar {
                    result[2..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let converted =
                        Self::x86_simd_rsqrt28(Self::get_lane(&source, lane, elem_bits), format);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86ScaleF {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                round,
                suppress_exceptions,
            } => {
                let format = match elem {
                    VecElementType::F16 => X86_SIMD_F16,
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if (*scalar && (*width != VecWidth::V128 || *lanes != 1))
                    || (!*scalar && *lanes != width.lanes(*elem) as u8)
                    || (*mask_zeroing && mask.is_none())
                    || (*suppress_exceptions != (*round != FpRoundMode::Dynamic))
                    || matches!(round, FpRoundMode::RoundNearestTiesAway)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }

                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let old = Self::read_vec(ctx, *dst);
                let mut result = if *scalar { first } else { old };
                if *scalar {
                    result[2..].fill(0);
                } else {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let active = mask.map_or(u64::MAX, |reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let elem_bits = elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if active & (1u64 << lane) == 0 {
                        if *mask_zeroing {
                            Self::set_lane(&mut result, lane, elem_bits, 0);
                        } else {
                            Self::set_lane(
                                &mut result,
                                lane,
                                elem_bits,
                                Self::get_lane(&old, lane, elem_bits),
                            );
                        }
                        continue;
                    }
                    let first_bits = Self::get_lane(&first, lane, elem_bits);
                    let second_bits = Self::get_lane(&second, lane, elem_bits);
                    let converted = Self::x86_simd_scale_f(
                        first_bits,
                        second_bits,
                        format,
                        mode,
                        mxcsr,
                        *scalar,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86CheckAlignment { addr, alignment } => {
                debug_assert!(alignment.is_power_of_two());
                let effective_addr = self.compute_address(ctx, addr);
                if effective_addr & (u64::from(*alignment) - 1) != 0 {
                    ctx.request_exit(ExitReason::GeneralProtection {
                        addr: op.guest_pc,
                        error_code: 0,
                    });
                }
            }

            OpKind::FConvert { dst, src, from, to } => {
                let a = self.read_fp(ctx, *src, *from);
                self.write_fp(ctx, *dst, a, *to);
            }

            OpKind::HexFp {
                dst,
                src1,
                src2,
                op,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                let r = hex_fp_eval(*op, a, b);
                ctx.write_vreg(*dst, r);
            }

            OpKind::HexFp3 {
                dst,
                src1,
                src2,
                src3,
                negate_product,
                lib,
            } => {
                let a = ctx.read_vreg(*src1) as u32;
                let b = ctx.read_vreg(*src2) as u32;
                let c = ctx.read_vreg(*src3) as u32;
                let r = if *lib {
                    // `:lib` form: exact-core fma + Hexagon post-fixups.
                    hex_sf_fma_lib(a, b, c, *negate_product)
                } else {
                    hex_sf_fma(a, b, c, *negate_product)
                };
                ctx.write_vreg(*dst, r as u64);
            }

            OpKind::HexFpRecip {
                dst,
                pred,
                src1,
                src2,
                kind,
            } => {
                let rs = ctx.read_vreg(*src1) as u32;
                let rt = ctx.read_vreg(*src2) as u32;
                let (rd, pe) = hex_fp_recip_eval(*kind, rs, rt);
                ctx.write_vreg(*dst, rd as u64);
                if let Some(p) = pred {
                    // The seed ops (sfrecipa/sfinvsqrta) write the FULL Hexagon
                    // predicate byte Pe; the harness compares the whole byte.
                    ctx.write_vreg(*p, pe as u64);
                }
            }

            OpKind::HexFpDf {
                dst,
                src1,
                src2,
                src3,
                op,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                let r = match op {
                    crate::smir::ir::ops::HexDfOp::DfMpyHh => {
                        let acc = ctx.read_vreg(*src3);
                        hr_df_mpyhh(a, b, acc)
                    }
                    crate::smir::ir::ops::HexDfOp::DfMpyFix => hr_df_mpyfix(a, b),
                };
                ctx.write_vreg(*dst, r);
            }

            OpKind::HexFpScFma {
                dst,
                src1,
                src2,
                src3,
                scale,
            } => {
                let rs = ctx.read_vreg(*src1) as u32;
                let rt = ctx.read_vreg(*src2) as u32;
                let rx = ctx.read_vreg(*src3) as u32;
                let pu = ctx.read_vreg(*scale) as u8;
                let r = hex_sf_fma_scale(rs, rt, rx, pu);
                ctx.write_vreg(*dst, r as u64);
            }

            OpKind::HexCabacDecBin {
                dst,
                pred,
                src1,
                src2,
            } => {
                let rss = ctx.read_vreg(*src1);
                let rtt = ctx.read_vreg(*src2);
                let (rdd, p0) = hex_cabac_decbin(rss, rtt);
                ctx.write_vreg(*dst, rdd);
                ctx.write_vreg(*pred, p0 as u64);
            }

            OpKind::HexTlbMatch { dst, src1, src2 } => {
                let rss = ctx.read_vreg(*src1);
                let rt = ctx.read_vreg(*src2) as u32;
                let p = hex_tlbmatch(rss, rt);
                ctx.write_vreg(*dst, p as u64);
            }

            OpKind::RvFp {
                dst,
                fcsr_dst,
                src1,
                src2,
                src3,
                fcsr_src,
                op: fp_op,
                rm_field,
                xlen,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                let c = ctx.read_vreg(*src3);
                let fcsr = ctx.read_vreg(*fcsr_src) as u32;
                let malformed = !matches!(*xlen, 32 | 64)
                    || *rm_field > 7
                    || (*xlen == 32 && crate::isa::riscv::float::fp_requires_rv64(*fp_op));
                // Bit-exact against the qemu-verified RISC-V interpreter.
                let evaluated = if malformed {
                    None
                } else {
                    crate::isa::riscv::float::eval_scalar_fp(*fp_op, *rm_field, fcsr, a, b, c)
                };
                match evaluated {
                    Some((res, new_fcsr)) => {
                        let res =
                            if *xlen == 32 && crate::isa::riscv::float::fp_writes_int_dst(*fp_op) {
                                res & 0xffff_ffff
                            } else {
                                res
                            };
                        ctx.write_vreg(*dst, res);
                        ctx.write_vreg(*fcsr_dst, new_fcsr as u64);
                    }
                    None => {
                        // Illegal rounding-mode fields trap with no architectural
                        // state change.
                        ctx.request_exit(ExitReason::Undefined {
                            addr: op.guest_pc,
                            opcode: 0,
                        });
                    }
                }
            }

            OpKind::RvIntCrypto {
                dst,
                src1,
                src2,
                op,
                imm,
                xlen,
            } => {
                let a = ctx.read_vreg(*src1);
                let b = ctx.read_vreg(*src2);
                if let Some(res) =
                    crate::isa::riscv::crypto::eval_int_crypto(*op, a, b, *imm, *xlen as u32)
                {
                    ctx.write_vreg(*dst, res);
                }
            }

            OpKind::RvVector {
                insn, xlen, state, ..
            } => {
                exec_rv_vector(ctx, memory, *insn, *xlen, op.guest_pc, state);
            }

            OpKind::IntToFp {
                dst,
                src,
                int_width,
                fp_precision,
                signed,
            } => {
                let val = ctx.read_vreg(*src) & int_width.mask();
                let f = if *signed {
                    self.sign_extend(val, *int_width) as i64 as f64
                } else {
                    val as f64
                };
                self.write_fp(ctx, *dst, f, *fp_precision);
            }

            OpKind::FpToInt {
                dst,
                src,
                fp_precision,
                int_width,
                signed,
                round,
            } => {
                let f = self.read_fp(ctx, *src, *fp_precision);
                let rounded = self.round_fp_value(ctx, f, *round);
                let val = if *signed {
                    (rounded as i64) as u64
                } else {
                    rounded as u64
                };
                ctx.write_vreg(*dst, val & int_width.mask());
            }

            OpKind::X86FpToInt {
                dst,
                src,
                elem,
                int_width,
                signed,
                truncate,
                round,
                suppress_exceptions,
            } => {
                self.execute_x86_fp_to_int(
                    ctx,
                    *dst,
                    *src,
                    *elem,
                    *int_width,
                    *signed,
                    *truncate,
                    *round,
                    *suppress_exceptions,
                );
            }

            OpKind::X86IntToFp {
                dst,
                merge,
                src,
                elem,
                int_width,
                signed,
                round,
                suppress_exceptions,
                zero_upper,
            } => {
                self.execute_x86_int_to_fp(
                    ctx,
                    *dst,
                    *merge,
                    *src,
                    *elem,
                    *int_width,
                    *signed,
                    *round,
                    *suppress_exceptions,
                    *zero_upper,
                );
            }

            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask,
                from,
                to,
                mask_zeroing,
                round,
                suppress_exceptions,
                zero_upper,
            } => {
                let mut result = Self::read_vec(ctx, *merge);
                if *zero_upper {
                    result[2..].fill(0);
                }
                let active = mask.map_or(true, |reg| ctx.read_vreg(reg) & 1 != 0);
                if !active {
                    let scalar_bits = if *mask_zeroing {
                        0
                    } else {
                        Self::get_lane(&Self::read_vec(ctx, *dst), 0, to.bytes() * 8)
                    };
                    Self::set_lane(&mut result, 0, to.bytes() * 8, scalar_bits);
                    Self::write_vec(ctx, *dst, result);
                    return Ok(());
                }

                let source = if matches!(src, VReg::Virtual(_)) {
                    ctx.read_vreg(*src)
                } else {
                    Self::get_lane(&Self::read_vec(ctx, *src), 0, from.bytes() * 8)
                };
                let (from_format, to_format) = match (*from, *to) {
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
                        return Ok(());
                    }
                };
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let converted = Self::x86_simd_fp_convert_precision(
                    source,
                    from_format,
                    to_format,
                    mode,
                    mxcsr,
                    true,
                );
                if !*suppress_exceptions {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= converted.status;
                    }
                    if Self::x86_simd_fp_unmasked(converted.status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        return Ok(());
                    }
                }
                Self::set_lane(&mut result, 0, to.bytes() * 8, converted.bits);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from,
                to,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                report_fp16_denormal,
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let (from_format, to_format) = match (*from, *to) {
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
                        return Ok(());
                    }
                };
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, to.bytes() * 8);
                            Self::set_lane(&mut result, lane, to.bytes() * 8, preserved);
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, from.bytes() * 8);
                    let converted = Self::x86_simd_fp_convert_precision(
                        source_bits,
                        from_format,
                        to_format,
                        mode,
                        if *to == VecElementType::F16 {
                            // VCVTPD2PH/VCVTPS2PHX produce FP16 denormals
                            // regardless of MXCSR.FTZ.
                            mxcsr & !(1 << 15)
                        } else {
                            mxcsr
                        },
                        *report_fp16_denormal,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, to.bytes() * 8, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem,
                fp_elem,
                signed,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let format = match fp_elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if !matches!(int_elem, VecElementType::I32 | VecElementType::I64) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let int_bits = int_elem.bytes() * 8;
                let int_mask = if int_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << int_bits) - 1
                };
                let fp_bits = fp_elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, fp_bits);
                            Self::set_lane(&mut result, lane, fp_bits, preserved);
                        }
                        continue;
                    }
                    let raw = Self::get_lane(&source, lane, int_bits) & int_mask;
                    let value = if *signed {
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
                    let converted = Self::x86_simd_fp_round_exact(
                        negative, magnitude, 0, false, format, mode, mxcsr,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, fp_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                signed,
                truncate,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let format = match fp_elem {
                    VecElementType::F32 => X86_SIMD_F32,
                    VecElementType::F64 => X86_SIMD_F64,
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                if !matches!(int_elem, VecElementType::I32 | VecElementType::I64) {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway
                    || (*truncate && mode != FpRoundMode::RoundTowardZero)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let fp_bits = fp_elem.bytes() * 8;
                let int_bits = int_elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, int_bits);
                            Self::set_lane(&mut result, lane, int_bits, preserved);
                        }
                        continue;
                    }
                    let mut source_bits = Self::get_lane(&source, lane, fp_bits);
                    // Intel specifies only Invalid and Precision for these
                    // conversions. DAZ still substitutes a signed zero, but a
                    // preserved denormal does not independently raise DE.
                    if mxcsr & (1 << 6) != 0 && Self::x86_simd_fp_is_denormal(source_bits, format) {
                        let (sign, _, _, _) = Self::x86_simd_fp_masks(format);
                        source_bits &= sign;
                    }
                    let converted =
                        Self::x86_simd_fp_to_int(source_bits, format, int_bits, *signed, mode);
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, int_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86PackedIntToFp16 {
                dst,
                src,
                mask,
                int_elem,
                signed,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let elem_bits = int_elem.bytes() * 8;
                let elem_mask = if elem_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            Self::set_lane(&mut result, lane, 16, Self::get_lane(&old, lane, 16));
                        }
                        continue;
                    }
                    let raw = Self::get_lane(&source, lane, elem_bits) & elem_mask;
                    let value = if *signed {
                        let shift = 128 - elem_bits;
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
                    let converted = Self::x86_simd_fp_round_exact(
                        negative,
                        magnitude,
                        0,
                        false,
                        X86_SIMD_F16,
                        mode,
                        mxcsr,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, 16, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86PackedFp16ToInt {
                dst,
                src,
                mask,
                int_elem,
                signed,
                truncate,
                lanes,
                dst_width,
                mask_zeroing,
                zero_upper,
                round,
                suppress_exceptions,
                ..
            } => {
                let source = Self::read_vec(ctx, *src);
                let old = Self::read_vec(ctx, *dst);
                let mut result = old;
                if *zero_upper {
                    result[(dst_width.bytes() / 8) as usize..].fill(0);
                }
                result[..(dst_width.bytes() / 8) as usize].fill(0);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                if mode == FpRoundMode::RoundNearestTiesAway
                    || (*truncate && mode != FpRoundMode::RoundTowardZero)
                {
                    ctx.request_exit(ExitReason::Undefined {
                        addr: ctx.pc,
                        opcode: 0,
                    });
                    return Ok(());
                }
                let elem_bits = int_elem.bytes() * 8;
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        if !*mask_zeroing {
                            let preserved = Self::get_lane(&old, lane, elem_bits);
                            Self::set_lane(&mut result, lane, elem_bits, preserved);
                        }
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, 16);
                    let converted = Self::x86_simd_fp_to_int(
                        source_bits,
                        X86_SIMD_F16,
                        elem_bits,
                        *signed,
                        mode,
                    );
                    status |= converted.status;
                    Self::set_lane(&mut result, lane, elem_bits, converted.bits);
                }
                if !*suppress_exceptions {
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

            OpKind::X86PackedFpConvertStore {
                addr,
                src,
                mask,
                lanes,
                round,
            } => {
                let effective_addr = self.compute_address(ctx, addr);
                let source = Self::read_vec(ctx, *src);
                let mask_bits = mask.map(|reg| ctx.read_vreg(reg));

                // E11 fault suppression is per destination element. Probe all
                // active lanes before conversion so a late address fault cannot
                // leave either partial stores or premature MXCSR status updates.
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        continue;
                    }
                    memory.probe(effective_addr.wrapping_add(u64::from(lane) * 2), 2, true)?;
                }

                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let mode = if *round == FpRoundMode::Dynamic {
                    self.dynamic_fp_round_mode(ctx)
                } else {
                    *round
                };
                let mut converted_lanes = [0u16; 16];
                let mut status = 0;
                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        continue;
                    }
                    let source_bits = Self::get_lane(&source, lane, 32);
                    let converted = Self::x86_simd_fp_convert_precision(
                        source_bits,
                        X86_SIMD_F32,
                        X86_SIMD_F16,
                        mode,
                        // VCVTPS2PH produces FP16 denormals even when FTZ is set.
                        mxcsr & !(1 << 15),
                        false,
                    );
                    status |= converted.status;
                    converted_lanes[usize::from(lane)] = converted.bits as u16;
                }
                if Self::x86_simd_fp_unmasked(status, mxcsr) {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= status;
                    }
                    ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                    return Ok(());
                }

                for lane in 0..*lanes {
                    if mask_bits.is_some_and(|bits| bits & (1u64 << lane) == 0) {
                        continue;
                    }
                    memory.write(
                        effective_addr.wrapping_add(u64::from(lane) * 2),
                        &converted_lanes[usize::from(lane)].to_le_bytes(),
                    )?;
                }
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.mxcsr |= status;
                }
            }

            OpKind::FRound {
                dst,
                src,
                precision,
                mode,
            } => {
                let a = self.read_fp(ctx, *src, *precision);
                self.write_fp(ctx, *dst, self.round_fp_value(ctx, a, *mode), *precision);
            }

            OpKind::X86Round {
                dst,
                merge,
                src,
                elem,
                width,
                lanes,
                scalar_source,
                zero_upper,
                mode,
                suppress_precision,
            } => {
                let source = if *scalar_source && matches!(src, VReg::Virtual(_)) {
                    let mut value = [0u64; 16];
                    value[0] = ctx.read_vreg(*src);
                    value
                } else {
                    Self::read_vec(ctx, *src)
                };
                let old = Self::read_vec(ctx, *dst);
                let mut result = if u32::from(*lanes) * elem.bytes() < width.bytes() {
                    Self::read_vec(ctx, *merge)
                } else {
                    old
                };
                if *zero_upper {
                    result[(width.bytes() / 8) as usize..].fill(0);
                }
                let mxcsr = match &ctx.arch_regs {
                    ArchRegState::X86_64(x86) => x86.mxcsr,
                    _ => 0x1F80,
                };
                let daz = mxcsr & (1 << 6) != 0;
                let mut status = 0u32;
                for lane in 0..*lanes {
                    let raw = Self::get_lane(&source, lane, elem.bytes() * 8);
                    let rounded = match elem {
                        VecElementType::F32 => {
                            let mut bits = raw as u32;
                            let exponent = bits & 0x7F80_0000;
                            let fraction = bits & 0x007F_FFFF;
                            if exponent == 0 && fraction != 0 && daz {
                                bits &= 0x8000_0000;
                            }
                            let exponent = bits & 0x7F80_0000;
                            let fraction = bits & 0x007F_FFFF;
                            if exponent == 0x7F80_0000 && fraction != 0 {
                                if fraction & 0x0040_0000 == 0 {
                                    status |= 1;
                                    bits |= 0x0040_0000;
                                }
                                u64::from(bits)
                            } else {
                                let value = f32::from_bits(bits);
                                let rounded =
                                    self.round_fp_value(ctx, f64::from(value), *mode) as f32;
                                let rounded_bits = rounded.to_bits();
                                if rounded_bits != bits && !*suppress_precision {
                                    status |= 1 << 5;
                                }
                                u64::from(rounded_bits)
                            }
                        }
                        VecElementType::F64 => {
                            let mut bits = raw;
                            let exponent = bits & 0x7FF0_0000_0000_0000;
                            let fraction = bits & 0x000F_FFFF_FFFF_FFFF;
                            if exponent == 0 && fraction != 0 && daz {
                                bits &= 0x8000_0000_0000_0000;
                            }
                            let exponent = bits & 0x7FF0_0000_0000_0000;
                            let fraction = bits & 0x000F_FFFF_FFFF_FFFF;
                            if exponent == 0x7FF0_0000_0000_0000 && fraction != 0 {
                                if fraction & 0x0008_0000_0000_0000 == 0 {
                                    status |= 1;
                                    bits |= 0x0008_0000_0000_0000;
                                }
                                bits
                            } else {
                                let value = f64::from_bits(bits);
                                let rounded = self.round_fp_value(ctx, value, *mode);
                                let rounded_bits = rounded.to_bits();
                                if rounded_bits != bits && !*suppress_precision {
                                    status |= 1 << 5;
                                }
                                rounded_bits
                            }
                        }
                        _ => {
                            ctx.request_exit(ExitReason::Undefined {
                                addr: ctx.pc,
                                opcode: 0,
                            });
                            return Ok(());
                        }
                    };
                    Self::set_lane(&mut result, lane, elem.bytes() * 8, rounded);
                }
                if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                    x86.mxcsr |= status;
                }
                let invalid_unmasked = status & 1 != 0 && mxcsr & (1 << 7) == 0;
                let precision_unmasked = status & (1 << 5) != 0 && mxcsr & (1 << 12) == 0;
                if invalid_unmasked || precision_unmasked {
                    ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                } else {
                    Self::write_vec(ctx, *dst, result);
                }
            }

            OpKind::X86DotProduct {
                dst,
                src1,
                src2,
                elem,
                width,
                imm,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
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
                let (format, lanes_per_group, groups) = match (*elem, *width) {
                    (VecElementType::F32, VecWidth::V128) => (X86_SIMD_F32, 4u8, 1u8),
                    (VecElementType::F32, VecWidth::V256) => (X86_SIMD_F32, 4, 2),
                    (VecElementType::F64, VecWidth::V128) => (X86_SIMD_F64, 2, 1),
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                        return Ok(());
                    }
                };
                let commit_stage = |ctx: &mut SmirContext, stage_status: u32| -> bool {
                    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
                        x86.mxcsr |= stage_status;
                    }
                    if Self::x86_simd_fp_unmasked(stage_status, mxcsr) {
                        ctx.request_exit(ExitReason::SimdFloatingPoint { addr: ctx.pc });
                        true
                    } else {
                        false
                    }
                };

                let mut products = vec![[0u64; 4]; groups as usize];
                let mut stage_status = 0u32;
                for group in 0..groups {
                    for lane in 0..lanes_per_group {
                        if imm & (1 << (lane + 4)) == 0 {
                            continue;
                        }
                        let vector_lane = group * lanes_per_group + lane;
                        let a = Self::get_lane(&first, vector_lane, elem.bytes() * 8);
                        let b = Self::get_lane(&second, vector_lane, elem.bytes() * 8);
                        let product = Self::x86_simd_fp_mul(a, b, format, mode, mxcsr);
                        products[group as usize][lane as usize] = product.bits;
                        stage_status |= product.status;
                    }
                }
                if commit_stage(ctx, stage_status) {
                    return Ok(());
                }

                let mut totals = vec![0u64; groups as usize];
                if *elem == VecElementType::F64 {
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            products[group as usize][0],
                            products[group as usize][1],
                            format,
                            mode,
                            mxcsr,
                        );
                        totals[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                } else {
                    let mut low = vec![0u64; groups as usize];
                    let mut high = vec![0u64; groups as usize];
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            products[group as usize][0],
                            products[group as usize][1],
                            format,
                            mode,
                            mxcsr,
                        );
                        low[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            products[group as usize][2],
                            products[group as usize][3],
                            format,
                            mode,
                            mxcsr,
                        );
                        high[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                    stage_status = 0;
                    for group in 0..groups {
                        let sum = Self::x86_simd_fp_add(
                            low[group as usize],
                            high[group as usize],
                            format,
                            mode,
                            mxcsr,
                        );
                        totals[group as usize] = sum.bits;
                        stage_status |= sum.status;
                    }
                    if commit_stage(ctx, stage_status) {
                        return Ok(());
                    }
                }

                let mut result = [0u64; 16];
                for group in 0..groups {
                    for lane in 0..lanes_per_group {
                        if imm & (1 << lane) != 0 {
                            Self::set_lane(
                                &mut result,
                                group * lanes_per_group + lane,
                                elem.bytes() * 8,
                                totals[group as usize],
                            );
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            _ => return self.execute_op_simd(ctx, memory, op),
        }

        Ok(())
    }
}
