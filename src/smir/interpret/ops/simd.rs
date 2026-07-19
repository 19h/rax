//! SIMD/vector op execution

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
    pub(crate) fn execute_op_simd(
        &self,
        ctx: &mut SmirContext,
        memory: &mut dyn SmirMemory,
        op: &SmirOp,
    ) -> Result<(), MemoryError> {
        let x86_hint = op.x86_hint;
        match &op.kind {
            // ==================================================================
            // SIMD / VECTOR (simplified)
            // ==================================================================
            OpKind::VAdd {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a + b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a + b);
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            a.wrapping_add(b)
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a - b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a - b);
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            a.wrapping_sub(b)
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    // VMax is architectural vector FMAX: NaN-PROPAGATING (a lone
                    // quiet NaN wins), distinct from the numeric VFMinMaxNm. Rust's
                    // `a.max(b)` is numeric (drops a lone NaN), so propagate
                    // explicitly. (#159)
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if a.is_nan() {
                                a
                            } else if b.is_nan() {
                                b
                            } else {
                                a.max(b)
                            }
                        });
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if a.is_nan() {
                                a
                            } else if b.is_nan() {
                                b
                            } else {
                                a.max(b)
                            }
                        });
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| a.max(b));
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VX86MinMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
                min,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if (*min && a < b) || (!*min && a > b) {
                                a
                            } else {
                                b
                            }
                        });
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if (*min && a < b) || (!*min && a > b) {
                                a
                            } else {
                                b
                            }
                        });
                    }
                    _ => {
                        ctx.request_exit(ExitReason::Undefined {
                            addr: ctx.pc,
                            opcode: 0,
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VAddSubSat {
                dst,
                src1,
                src2,
                elem,
                lanes,
                subtract,
                signed,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let lhs = Self::read_vec(ctx, *src1);
                let rhs = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let mask = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let a = Self::get_lane(&lhs, lane, bits);
                    let b = Self::get_lane(&rhs, lane, bits);
                    let value = if *signed {
                        let shift = 64 - bits;
                        let a = ((a << shift) as i64 >> shift) as i128;
                        let b = ((b << shift) as i64 >> shift) as i128;
                        let raw = if *subtract { a - b } else { a + b };
                        let min = -(1i128 << (bits - 1));
                        let max = (1i128 << (bits - 1)) - 1;
                        raw.clamp(min, max) as u64 & mask
                    } else if *subtract {
                        a.saturating_sub(b)
                    } else {
                        (u128::from(a) + u128::from(b)).min(u128::from(mask)) as u64
                    };
                    Self::set_lane(&mut result, lane, bits, value);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VMul {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a * b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a * b);
                    }
                    _ => {
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            a.wrapping_mul(b)
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VDiv {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| a / b);
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| a / b);
                    }
                    _ => {
                        // Integer vector divide is not a NEON op; guard against
                        // division-by-zero in case a malformed op reaches here.
                        self.vec_binary_op(ctx, *dst, *src1, *src2, *elem, *lanes, |a, b| {
                            if b == 0 { 0 } else { a.wrapping_div(b) }
                        });
                    }
                }
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VReduce {
                dst,
                src,
                elem,
                lanes,
                op,
            } => {
                let a = Self::read_vec(ctx, *src);
                let bits = elem.bytes() * 8;
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let lane = |i: u8| Self::get_lane(&a, i, bits) & mask;
                let sext = |v: u64| {
                    let shift = 64 - bits;
                    ((v << shift) as i64) >> shift
                };
                let n = *lanes;
                let value = match op {
                    VecReduceOp::Add => {
                        let mut acc = 0u64;
                        for i in 0..n {
                            acc = acc.wrapping_add(lane(i));
                        }
                        acc & mask
                    }
                    VecReduceOp::SMax => {
                        let mut acc = sext(lane(0));
                        for i in 1..n {
                            acc = acc.max(sext(lane(i)));
                        }
                        acc as u64 & mask
                    }
                    VecReduceOp::SMin => {
                        let mut acc = sext(lane(0));
                        for i in 1..n {
                            acc = acc.min(sext(lane(i)));
                        }
                        acc as u64 & mask
                    }
                    VecReduceOp::UMax => {
                        let mut acc = lane(0);
                        for i in 1..n {
                            acc = acc.max(lane(i));
                        }
                        acc
                    }
                    VecReduceOp::UMin => {
                        let mut acc = lane(0);
                        for i in 1..n {
                            acc = acc.min(lane(i));
                        }
                        acc
                    }
                    // FP reductions. NaN-quiet (FMaxNm/FMinNm) use Rust min/max
                    // (maxNum/minNum); NaN-propagating (FMax/FMin) yield NaN if
                    // any lane is NaN.
                    VecReduceOp::FMax
                    | VecReduceOp::FMin
                    | VecReduceOp::FMaxNm
                    | VecReduceOp::FMinNm => {
                        let nm = matches!(op, VecReduceOp::FMaxNm | VecReduceOp::FMinNm);
                        let is_min = matches!(op, VecReduceOp::FMin | VecReduceOp::FMinNm);
                        if bits == 32 {
                            let lf = |i: u8| f32::from_bits(Self::get_lane(&a, i, 32) as u32);
                            let mut acc = lf(0);
                            for i in 1..n {
                                let x = lf(i);
                                acc = if !nm && (acc.is_nan() || x.is_nan()) {
                                    f32::NAN
                                } else if is_min {
                                    acc.min(x)
                                } else {
                                    acc.max(x)
                                };
                            }
                            acc.to_bits() as u64
                        } else {
                            let lf = |i: u8| f64::from_bits(Self::get_lane(&a, i, 64));
                            let mut acc = lf(0);
                            for i in 1..n {
                                let x = lf(i);
                                acc = if !nm && (acc.is_nan() || x.is_nan()) {
                                    f64::NAN
                                } else if is_min {
                                    acc.min(x)
                                } else {
                                    acc.max(x)
                                };
                            }
                            acc.to_bits()
                        }
                    }
                    // Widening add: sum sign/zero-extended lanes; result is 2x
                    // the element width.
                    VecReduceOp::SAddLong => {
                        let mut acc = 0i128;
                        for i in 0..n {
                            acc += i128::from(sext(lane(i)));
                        }
                        acc as u64
                    }
                    VecReduceOp::UAddLong => {
                        let mut acc = 0u128;
                        for i in 0..n {
                            acc += u128::from(lane(i));
                        }
                        acc as u64
                    }
                };
                // Widening reductions write a result 2x the element width.
                let result_bits = if matches!(op, VecReduceOp::SAddLong | VecReduceOp::UAddLong) {
                    (bits * 2).min(64)
                } else {
                    bits
                };
                let rmask = if result_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << result_bits) - 1
                };
                let mut result = [0u64; 16];
                Self::set_lane(&mut result, 0, result_bits, value & rmask);
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VFMinMaxNm {
                dst,
                src1,
                src2,
                elem,
                lanes,
                min,
            } => {
                // IEEE maxNum/minNum: Rust f32/f64 max/min return the numeric
                // operand when one is NaN, matching FMAXNM/FMINNM.
                match elem {
                    VecElementType::F32 => {
                        self.vec_binary_op_f32(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if *min { a.min(b) } else { a.max(b) }
                        });
                    }
                    VecElementType::F64 => {
                        self.vec_binary_op_f64(ctx, *dst, *src1, *src2, *lanes, |a, b| {
                            if *min { a.min(b) } else { a.max(b) }
                        });
                    }
                    _ => {
                        // FMAXNM/FMINNM are FP-only; ignore otherwise.
                    }
                }
            }

            OpKind::VPermute2 {
                dst,
                src1,
                src2,
                elem,
                lanes,
                kind,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let n = *lanes as usize;
                let half = n / 2;
                let geta = |i: usize| Self::get_lane(&a, i as u8, bits);
                let getb = |i: usize| Self::get_lane(&b, i as u8, bits);
                let mut result = [0u64; 16];
                for d in 0..n {
                    let v = match kind {
                        VecPermuteKind::Zip1 => {
                            if d % 2 == 0 {
                                geta(d / 2)
                            } else {
                                getb(d / 2)
                            }
                        }
                        VecPermuteKind::Zip2 => {
                            if d % 2 == 0 {
                                geta(half + d / 2)
                            } else {
                                getb(half + d / 2)
                            }
                        }
                        VecPermuteKind::Uzp1 => {
                            let idx = 2 * d;
                            if idx < n { geta(idx) } else { getb(idx - n) }
                        }
                        VecPermuteKind::Uzp2 => {
                            let idx = 2 * d + 1;
                            if idx < n { geta(idx) } else { getb(idx - n) }
                        }
                        VecPermuteKind::Trn1 => {
                            if d % 2 == 0 {
                                geta(d)
                            } else {
                                getb(d - 1)
                            }
                        }
                        VecPermuteKind::Trn2 => {
                            if d % 2 == 0 {
                                geta(d + 1)
                            } else {
                                getb(d)
                            }
                        }
                    };
                    Self::set_lane(&mut result, d as u8, bits, v);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VTableLookup {
                dst,
                table,
                num_tables,
                index,
                lanes,
                is_tbx,
            } => {
                // Build the byte table from `num_tables` consecutive registers
                // (table, table+1, ... mod 32).
                let base = match table {
                    VReg::Arch(ArchReg::Arm(ArmReg::V(n))) => u32::from(*n),
                    _ => 0,
                };
                let mut tbl = [0u8; 64];
                for t in 0..u32::from(*num_tables) {
                    let reg = VReg::Arch(ArchReg::Arm(ArmReg::V(((base + t) % 32) as u8)));
                    let rv = Self::read_vec(ctx, reg);
                    for byte in 0..16u8 {
                        tbl[(t * 16 + u32::from(byte)) as usize] =
                            Self::get_lane(&rv, byte, 8) as u8;
                    }
                }
                let table_size = usize::from(*num_tables) * 16;
                let idx_v = Self::read_vec(ctx, *index);
                let mut out = [0u8; 16];
                if *is_tbx {
                    let cur = Self::read_vec(ctx, *dst);
                    for byte in 0..16u8 {
                        out[byte as usize] = Self::get_lane(&cur, byte, 8) as u8;
                    }
                }
                let n = *lanes as usize;
                for byte in 0..n {
                    let idx = Self::get_lane(&idx_v, byte as u8, 8) as usize;
                    if idx < table_size {
                        out[byte] = tbl[idx];
                    } else if !*is_tbx {
                        out[byte] = 0;
                    }
                }
                // Q==0 (8 lanes) zeroes the upper 64 bits.
                if n == 8 {
                    for byte in &mut out[8..16] {
                        *byte = 0;
                    }
                }
                let mut result = [0u64; 16];
                for byte in 0..16u8 {
                    Self::set_lane(&mut result, byte, 8, u64::from(out[byte as usize]));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = a[i] & b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = !a[i] & b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = a[i] | b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, op.x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = a[i] ^ b[i];
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VBitSelect {
                dst,
                mask,
                src_true,
                src_false,
                width,
            } => {
                let m = Self::read_vec(ctx, *mask);
                let t = Self::read_vec(ctx, *src_true);
                let f = Self::read_vec(ctx, *src_false);
                let mut result = [0u64; 16];
                let word_count = (width.bytes() / 8) as usize;
                for i in 0..word_count {
                    result[i] = (t[i] & m[i]) | (f[i] & !m[i]);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op,
                signed,
                set_ovf,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let elem_bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                let mut ovf = false;
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, elem_bits);
                    let bv = Self::get_lane(&b, lane, elem_bits);
                    let rv = Self::apply_lane_op(*op, av, bv, elem_bits, *signed);
                    // For the saturating VLane opcodes whose sem uses
                    // `ctx.sat_n`/`ctx.satu_n` (e.g. `vsubuwsat`), flag USR:OVF
                    // on any lane whose add/sub clamped out of the target range.
                    if *set_ovf {
                        ovf |= Self::lane_sat_clamped(*op, av, bv, elem_bits, *signed);
                    }
                    Self::set_lane(&mut result, lane, elem_bits, rv);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VWidenMul {
                dst_lo,
                dst_hi,
                src1,
                src2,
                src_elem,
                signed1,
                signed2,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let wide_lanes = (1024 / nbits as usize) / 2; // wide lanes per output vector
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // Sign- or zero-extend an `nbits` zero-extended lane value to i64.
                let ext = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let shift = 64 - nbits;
                        ((v << shift) as i64) >> shift
                    } else {
                        v as i64
                    }
                };
                for i in 0..wide_lanes {
                    let even = i as u8 * 2;
                    let odd = even + 1;
                    let pe = ext(Self::get_lane(&a, even, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, even, nbits), *signed2));
                    let po = ext(Self::get_lane(&a, odd, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, odd, nbits), *signed2));
                    let ae = if *acc {
                        Self::get_lane(&lo, i as u8, wbits) as i64
                    } else {
                        0
                    };
                    let ao = if *acc {
                        Self::get_lane(&hi, i as u8, wbits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(&mut lo, i as u8, wbits, ae.wrapping_add(pe) as u64);
                    Self::set_lane(&mut hi, i as u8, wbits, ao.wrapping_add(po) as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VWidenAddSub {
                dst_lo,
                dst_hi,
                src1,
                src2,
                src_elem,
                signed1,
                signed2,
                sub,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let wide_lanes = (1024 / nbits as usize) / 2; // wide lanes per output vector
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // Sign- or zero-extend an `nbits` zero-extended lane value to i64.
                let ext = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let shift = 64 - nbits;
                        ((v << shift) as i64) >> shift
                    } else {
                        v as i64
                    }
                };
                let combine = |x: i64, y: i64| -> i64 {
                    if *sub {
                        x.wrapping_sub(y)
                    } else {
                        x.wrapping_add(y)
                    }
                };
                for i in 0..wide_lanes {
                    let even = i as u8 * 2;
                    let odd = even + 1;
                    let re = combine(
                        ext(Self::get_lane(&a, even, nbits), *signed1),
                        ext(Self::get_lane(&b, even, nbits), *signed2),
                    );
                    let ro = combine(
                        ext(Self::get_lane(&a, odd, nbits), *signed1),
                        ext(Self::get_lane(&b, odd, nbits), *signed2),
                    );
                    let ae = if *acc {
                        // sign-extend the existing wide lane so accumulate wraps signed
                        let v = Self::get_lane(&lo, i as u8, wbits);
                        let s = 64 - wbits;
                        ((v << s) as i64) >> s
                    } else {
                        0
                    };
                    let ao = if *acc {
                        let v = Self::get_lane(&hi, i as u8, wbits);
                        let s = 64 - wbits;
                        ((v << s) as i64) >> s
                    } else {
                        0
                    };
                    Self::set_lane(&mut lo, i as u8, wbits, ae.wrapping_add(re) as u64);
                    Self::set_lane(&mut hi, i as u8, wbits, ao.wrapping_add(ro) as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VLaneUnary {
                dst,
                src,
                elem,
                lanes,
                op,
                signed,
            } => {
                let a = Self::read_vec(ctx, *src);
                let elem_bits = elem.bytes() * 8;
                let mask: u64 = if elem_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                // Sign-extend a zero-extended `elem_bits` lane value to i64.
                let sx = |v: u64| -> i64 {
                    if elem_bits >= 64 {
                        v as i64
                    } else {
                        let shift = 64 - elem_bits;
                        ((v << shift) as i64) >> shift
                    }
                };
                let smax: i64 = if elem_bits >= 64 {
                    i64::MAX
                } else {
                    (1i64 << (elem_bits - 1)) - 1
                };
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, elem_bits);
                    let rv: u64 = match op {
                        // Not
                        0 => !av,
                        // Abs (wrapping: MIN -> MIN)
                        1 => (sx(av).wrapping_abs()) as u64,
                        // AbsSat: clamp |a| to the signed max (MIN -> MAX)
                        2 => {
                            let s = sx(av);
                            // wrapping_abs of MIN stays MIN (negative); clamp via i128
                            ((s as i128).abs().min(smax as i128)) as u64
                        }
                        // Clz within the elem-wide lane
                        3 => {
                            let v = av & mask;
                            (v << (64 - elem_bits)).leading_zeros().min(elem_bits) as u64
                        }
                        // Popcount of the elem-wide lane
                        4 => (av & mask).count_ones() as u64,
                        // NormAmt: max(clz(a), clz(!a)) - 1 within the lane
                        5 => {
                            let v = (av & mask) << (64 - elem_bits);
                            let nv = (!av & mask) << (64 - elem_bits);
                            let n = v
                                .leading_zeros()
                                .min(elem_bits)
                                .max(nv.leading_zeros().min(elem_bits));
                            (n - 1) as u64
                        }
                        // Neg (two's complement)
                        6 => sx(av).wrapping_neg() as u64,
                        // Clb: count leading sign bits = max(clz, clo) capped at
                        // the element width, on the left-justified lane value.
                        7 => {
                            let lj = (av & mask) << (64 - elem_bits);
                            let zeros = lj.leading_zeros().min(elem_bits);
                            let ones = lj.leading_ones().min(elem_bits);
                            zeros.max(ones) as u64
                        }
                        _ => av,
                    };
                    let _ = signed;
                    Self::set_lane(&mut result, lane, elem_bits, rv & mask);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VNavg {
                dst,
                src1,
                src2,
                elem,
                lanes,
                signed,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let elem_bits = elem.bytes() * 8;
                let mask: u64 = if elem_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let ext = |v: u64| -> i64 {
                    if *signed {
                        if elem_bits >= 64 {
                            v as i64
                        } else {
                            let shift = 64 - elem_bits;
                            ((v << shift) as i64) >> shift
                        }
                    } else {
                        (v & mask) as i64
                    }
                };
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let av = ext(Self::get_lane(&a, lane, elem_bits));
                    let bv = ext(Self::get_lane(&b, lane, elem_bits));
                    let r = (av.wrapping_sub(bv)) >> 1; // arithmetic, like sem `>> 1`
                    Self::set_lane(&mut result, lane, elem_bits, (r as u64) & mask);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShiftAcc {
                dst,
                src,
                amount,
                shift,
                elem,
                lanes,
            } => {
                let amt = match amount {
                    SrcOperand::Imm(val) => *val as u32,
                    SrcOperand::Reg(reg) => ctx.read_vreg(*reg) as u32,
                    _ => 0,
                };
                let elem_bits = elem.bytes() * 8;
                let mask = if elem_bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let sh = amt % elem_bits;
                let src_val = Self::read_vec(ctx, *src);
                let mut result = Self::read_vec(ctx, *dst);
                for lane in 0..*lanes {
                    let val = Self::get_lane(&src_val, lane, elem_bits);
                    let shifted = match shift {
                        ShiftOp::Lsl => (val << sh) & mask,
                        ShiftOp::Lsr => (val >> sh) & mask,
                        ShiftOp::Asr => {
                            let sv = if elem_bits >= 64 {
                                val as i64
                            } else {
                                let s = 64 - elem_bits;
                                ((val << s) as i64) >> s
                            };
                            ((sv >> sh) as u64) & mask
                        }
                        _ => val & mask,
                    };
                    let prev = Self::get_lane(&result, lane, elem_bits);
                    Self::set_lane(
                        &mut result,
                        lane,
                        elem_bits,
                        prev.wrapping_add(shifted) & mask,
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLut16 {
                dst_lo,
                dst_hi,
                src_idx,
                table,
                sel,
                nomatch,
                oracc,
            } => {
                let vu = Self::read_vec(ctx, *src_idx);
                let vv = Self::read_vec(ctx, *table);
                let sel_v = match sel {
                    SrcOperand::Imm(v) => *v as u32,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as u32,
                    _ => 0,
                };
                let matchval = (sel_v & 0xF) as u8;
                let oh = ((sel_v >> 1) & 0x1) as u8;
                let mut lo = if *oracc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *oracc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                let look = |idx: u8| -> u16 {
                    if *nomatch {
                        let k = ((idx & 0x0F) | (matchval << 4)) as usize;
                        Self::get_lane(&vv, ((k % 32) * 2) as u8 + oh, 16) as u16
                    } else if (idx & 0xF0) == (matchval << 4) {
                        let k = idx as usize;
                        Self::get_lane(&vv, ((k % 32) * 2) as u8 + oh, 16) as u16
                    } else {
                        0
                    }
                };
                for i in 0..64u8 {
                    let v_lo = look(Self::get_lane(&vu, i * 2, 8) as u8);
                    let v_hi = look(Self::get_lane(&vu, i * 2 + 1, 8) as u8);
                    if *oracc {
                        let plo = Self::get_lane(&lo, i, 16) as u16;
                        let phi = Self::get_lane(&hi, i, 16) as u16;
                        Self::set_lane(&mut lo, i, 16, (plo | v_lo) as u64);
                        Self::set_lane(&mut hi, i, 16, (phi | v_hi) as u64);
                    } else {
                        Self::set_lane(&mut lo, i, 16, v_lo as u64);
                        Self::set_lane(&mut hi, i, 16, v_hi as u64);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VLut {
                dst,
                src_idx,
                table,
                sel,
                nomatch,
                oracc,
            } => {
                let vu = Self::read_vec(ctx, *src_idx);
                let vv = Self::read_vec(ctx, *table);
                let sel_v = match sel {
                    SrcOperand::Imm(v) => *v as u32,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as u32,
                    _ => 0,
                };
                let matchval = (sel_v & 0x7) as u8;
                let oh = ((sel_v >> 1) & 0x1) as u8;
                let mut out = if *oracc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                for i in 0..128u8 {
                    let idx = Self::get_lane(&vu, i, 8) as u8;
                    let val: u8 = if *nomatch {
                        let lut_idx = ((idx & 0x1f) | (matchval << 5)) as usize;
                        Self::get_lane(&vv, ((lut_idx % 64) * 2) as u8 + oh, 8) as u8
                    } else if (idx & 0xe0) == (matchval << 5) {
                        let lut_idx = idx as usize;
                        Self::get_lane(&vv, ((lut_idx % 64) * 2) as u8 + oh, 8) as u8
                    } else {
                        0
                    };
                    if *oracc {
                        let prev = Self::get_lane(&out, i, 8) as u8;
                        Self::set_lane(&mut out, i, 8, (prev | val) as u64);
                    } else {
                        Self::set_lane(&mut out, i, 8, val as u64);
                    }
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VDelta {
                dst,
                src,
                control,
                ascending,
            } => {
                let mut cur = Self::read_vec(ctx, *src);
                let ctrl = Self::read_vec(ctx, *control);
                let mut offsets = [1u8, 2, 4, 8, 16, 32, 64];
                if !*ascending {
                    offsets.reverse();
                }
                for &offset in offsets.iter() {
                    let off = offset as usize;
                    let prev = cur;
                    for k in 0..128usize {
                        let cb = Self::get_lane(&ctrl, k as u8, 8);
                        let src_k = if cb & (off as u64) != 0 {
                            (k ^ off) as u8
                        } else {
                            k as u8
                        };
                        Self::set_lane(&mut cur, k as u8, 8, Self::get_lane(&prev, src_k, 8));
                    }
                }
                Self::write_vec(ctx, *dst, cur);
            }

            OpKind::VShuffVdd {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                amount,
            } => {
                let mut lo = Self::read_vec(ctx, *src_lo);
                let mut hi = Self::read_vec(ctx, *src_hi);
                let rt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                let mut offset = 1usize;
                while offset < 128 {
                    if rt & offset != 0 {
                        for k in 0..128usize {
                            if k & offset == 0 {
                                let a = Self::get_lane(&hi, k as u8, 8);
                                let b = Self::get_lane(&lo, (k + offset) as u8, 8);
                                Self::set_lane(&mut hi, k as u8, 8, b);
                                Self::set_lane(&mut lo, (k + offset) as u8, 8, a);
                            }
                        }
                    }
                    offset <<= 1;
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VDealB4W { dst, src1, src2 } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for i in 0..32u8 {
                    Self::set_lane(&mut result, i, 8, Self::get_lane(&v, i * 4, 8));
                    Self::set_lane(&mut result, 32 + i, 8, Self::get_lane(&v, i * 4 + 2, 8));
                    Self::set_lane(&mut result, 64 + i, 8, Self::get_lane(&u, i * 4, 8));
                    Self::set_lane(&mut result, 96 + i, 8, Self::get_lane(&u, i * 4 + 2, 8));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VAlign {
                dst,
                src1,
                src2,
                amount,
                left,
            } => {
                let amt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                let shift = if *left { 128 - (amt & 127) } else { amt & 127 };
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut result = [0u64; 16];
                for i in 0..128u8 {
                    let j = i as usize + shift;
                    let byte = if j < 128 {
                        Self::get_lane(&v, j as u8, 8)
                    } else {
                        Self::get_lane(&u, (j - 128) as u8, 8)
                    };
                    Self::set_lane(&mut result, i, 8, byte);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShuffle2 {
                dst,
                src,
                elem,
                deal,
            } => {
                let s = Self::read_vec(ctx, *src);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let mut result = [0u64; 16];
                for i in 0..half {
                    if *deal {
                        Self::set_lane(&mut result, i, nbits, Self::get_lane(&s, i * 2, nbits));
                        Self::set_lane(
                            &mut result,
                            i + half,
                            nbits,
                            Self::get_lane(&s, i * 2 + 1, nbits),
                        );
                    } else {
                        Self::set_lane(&mut result, i * 2, nbits, Self::get_lane(&s, i, nbits));
                        Self::set_lane(
                            &mut result,
                            i * 2 + 1,
                            nbits,
                            Self::get_lane(&s, i + half, nbits),
                        );
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShuffleEO {
                dst,
                src1,
                src2,
                elem,
                odd,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let parity = if *odd { 1 } else { 0 };
                let mut result = [0u64; 16];
                for i in 0..half {
                    let sel = i * 2 + parity;
                    Self::set_lane(&mut result, i * 2, nbits, Self::get_lane(&v, sel, nbits));
                    Self::set_lane(
                        &mut result,
                        i * 2 + 1,
                        nbits,
                        Self::get_lane(&u, sel, nbits),
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPack {
                dst,
                src1,
                src2,
                elem,
                odd,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let parity = if *odd { 1 } else { 0 };
                let mut result = [0u64; 16];
                for i in 0..half {
                    let sel = i * 2 + parity;
                    Self::set_lane(&mut result, i, nbits, Self::get_lane(&v, sel, nbits));
                    Self::set_lane(&mut result, i + half, nbits, Self::get_lane(&u, sel, nbits));
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VPackSat {
                dst,
                src1,
                src2,
                src_elem,
                to_unsigned,
                src_lanes,
                block_lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let wbits = src_elem.bytes() * 8;
                let nbits = wbits / 2;
                let (lo_b, hi_b) = if *to_unsigned {
                    (0i64, ((1i64 << nbits) - 1))
                } else {
                    (-(1i64 << (nbits - 1)), (1i64 << (nbits - 1)) - 1)
                };
                let sat = |raw: u64| -> u64 {
                    let sh = 64 - wbits;
                    let sv = ((raw << sh) as i64) >> sh; // sign-extend wide source
                    sv.clamp(lo_b, hi_b) as u64
                };
                let mut result = [0u64; 16];
                debug_assert!(*block_lanes != 0 && *src_lanes % *block_lanes == 0);
                for block_base in (0..*src_lanes).step_by(*block_lanes as usize) {
                    let output_base = block_base * 2;
                    for i in 0..*block_lanes {
                        let source_lane = block_base + i;
                        Self::set_lane(
                            &mut result,
                            output_base + i,
                            nbits,
                            sat(Self::get_lane(&v, source_lane, wbits)),
                        );
                        Self::set_lane(
                            &mut result,
                            output_base + *block_lanes + i,
                            nbits,
                            sat(Self::get_lane(&u, source_lane, wbits)),
                        );
                    }
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VWidenExt {
                dst_lo,
                dst_hi,
                src,
                src_elem,
                signed,
                interleave,
            } => {
                let s = Self::read_vec(ctx, *src);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let wide_lanes = (1024 / wbits) as u8; // wide lanes per output vector
                let ext = |raw: u64| -> u64 {
                    if *signed {
                        let sh = 64 - nbits;
                        (((raw << sh) as i64) >> sh) as u64
                    } else {
                        raw
                    }
                };
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for i in 0..wide_lanes {
                    let (lo_idx, hi_idx) = if *interleave {
                        (i * 2, i * 2 + 1)
                    } else {
                        (i, i + wide_lanes)
                    };
                    Self::set_lane(&mut lo, i, wbits, ext(Self::get_lane(&s, lo_idx, nbits)));
                    Self::set_lane(&mut hi, i, wbits, ext(Self::get_lane(&s, hi_idx, nbits)));
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VCmpToQ {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
                accumulate,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let ebytes = elem.bytes() as usize;
                let sext = |v: u64| -> i64 {
                    let sh = 64 - nbits;
                    ((v << sh) as i64) >> sh
                };
                let mut q = [0u64; 16];
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, nbits);
                    let bv = Self::get_lane(&b, lane, nbits);
                    let t = match cond {
                        VecCmpCond::Eq => av == bv,
                        VecCmpCond::Ne => av != bv,
                        VecCmpCond::Gt => sext(av) > sext(bv),
                        VecCmpCond::Ge => sext(av) >= sext(bv),
                        VecCmpCond::Lt => sext(av) < sext(bv),
                        VecCmpCond::Le => sext(av) <= sext(bv),
                        VecCmpCond::Gtu => av > bv,
                        VecCmpCond::Geu => av >= bv,
                        VecCmpCond::Ltu => av < bv,
                        VecCmpCond::Leu => av <= bv,
                    };
                    if t {
                        for byte in 0..ebytes {
                            let bit = lane as usize * ebytes + byte;
                            q[bit >> 6] |= 1u64 << (bit & 63);
                        }
                    }
                }
                // Accumulating compares combine the new mask into the existing Q.
                if let Some(combine) = accumulate {
                    let prev = Self::read_vec(ctx, *dst);
                    for w in 0..2 {
                        q[w] = match combine {
                            VLaneOp::And => prev[w] & q[w],
                            VLaneOp::Or => prev[w] | q[w],
                            VLaneOp::Xor => prev[w] ^ q[w],
                            _ => q[w],
                        };
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            OpKind::VQFromVAndR {
                dst,
                src1,
                src2,
                oracc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                // vandvrt_acc OR-accumulates into the existing dst Q; otherwise
                // overwrite (start from a clean Q).
                let mut q = if *oracc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                for byte in 0..128usize {
                    let av = Self::get_lane(&a, byte as u8, 8);
                    let bv = Self::get_lane(&b, byte as u8, 8);
                    if (av & bv) != 0 {
                        q[byte >> 6] |= 1u64 << (byte & 63);
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            OpKind::VMaskZero {
                dst,
                mask_q,
                src,
                negate,
                oracc,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let s = Self::read_vec(ctx, *src);
                // vandqrt_acc OR-accumulates the gated bytes into the existing
                // dst; the plain forms overwrite (unselected bytes -> 0).
                let mut result = if *oracc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                for byte in 0..128usize {
                    let bit = (m[byte >> 6] >> (byte & 63)) & 1 != 0;
                    if bit ^ *negate {
                        let sv = Self::get_lane(&s, byte as u8, 8);
                        if *oracc {
                            let prev = Self::get_lane(&result, byte as u8, 8);
                            Self::set_lane(&mut result, byte as u8, 8, prev | sv);
                        } else {
                            Self::set_lane(&mut result, byte as u8, 8, sv);
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VLaneCond {
                dst,
                src,
                mask_q,
                elem,
                lanes,
                sub,
                negate,
            } => {
                let x = Self::read_vec(ctx, *dst);
                let u = Self::read_vec(ctx, *src);
                let m = Self::read_vec(ctx, *mask_q);
                let elem_bits = elem.bytes() * 8;
                let ebytes = elem.bytes() as usize;
                let mut result = x;
                for lane in 0..*lanes {
                    let a = Self::get_lane(&x, lane, elem_bits);
                    let b = Self::get_lane(&u, lane, elem_bits);
                    let r = if *sub {
                        a.wrapping_sub(b)
                    } else {
                        a.wrapping_add(b)
                    };
                    let rb = r.to_le_bytes();
                    let base = lane as usize * ebytes;
                    // Per-byte select: each Q bit covering this lane's bytes
                    // chooses op-result vs unchanged dst (fCONDMASK{8,16,32}).
                    for byte in 0..ebytes {
                        let bidx = base + byte;
                        let qb = (m[bidx >> 6] >> (bidx & 63)) & 1 != 0;
                        if qb ^ *negate {
                            Self::set_lane(&mut result, bidx as u8, 8, rb[byte] as u64);
                        }
                    }
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VCarry {
                dst,
                src1,
                src2,
                q_inout,
                sub,
                has_cin,
                cin0,
                has_cout,
                sat,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let qin = if *has_cin {
                    Self::read_vec(ctx, *q_inout)
                } else {
                    [0u64; 16]
                };
                let mut out = [0u64; 16];
                let mut qout = [0u64; 16];
                // vaddcarrysat (sat=true) is the only carry form that saturates;
                // its sem (hvx_carry.rs) clamps via `ctx.sat_n(s, 32)`, setting
                // USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..32usize {
                    let av = Self::get_lane(&a, i as u8, 32) as u32;
                    let bv0 = Self::get_lane(&b, i as u8, 32) as u32;
                    let bv = if *sub { !bv0 } else { bv0 };
                    let cin = if *has_cin {
                        let bit = i * 4;
                        ((qin[bit >> 6] >> (bit & 63)) & 1) as u32
                    } else {
                        *cin0 as u32
                    };
                    if *sat {
                        // vaddcarrysat: signed sat_32 of Vu + Vv + cin (no
                        // carry-out). `sub` is never set for the sat form.
                        let s = av as i32 as i64 + bv0 as i32 as i64 + cin as i64;
                        if s < i32::MIN as i64 || s > i32::MAX as i64 {
                            ovf = true;
                        }
                        let clamped = s.clamp(i32::MIN as i64, i32::MAX as i64) as u32;
                        Self::set_lane(&mut out, i as u8, 32, clamped as u64);
                    } else {
                        let full = av as u64 + bv as u64 + cin as u64;
                        Self::set_lane(&mut out, i as u8, 32, full & 0xffff_ffff);
                        let carry = (full >> 32) != 0;
                        if *has_cout {
                            for byte in 0..4 {
                                let bit = i * 4 + byte;
                                if carry {
                                    qout[bit >> 6] |= 1u64 << (bit & 63);
                                }
                            }
                        }
                    }
                }
                Self::write_vec(ctx, *dst, out);
                if *has_cout {
                    Self::write_vec(ctx, *q_inout, qout);
                }
                if *sat && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VSwap {
                dst_lo,
                dst_hi,
                mask_q,
                src1,
                src2,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for byte in 0..128usize {
                    let qb = (m[byte >> 6] >> (byte & 63)) & 1 != 0;
                    let uv = Self::get_lane(&u, byte as u8, 8);
                    let vv = Self::get_lane(&v, byte as u8, 8);
                    if qb {
                        Self::set_lane(&mut lo, byte as u8, 8, uv);
                        Self::set_lane(&mut hi, byte as u8, 8, vv);
                    } else {
                        Self::set_lane(&mut lo, byte as u8, 8, vv);
                        Self::set_lane(&mut hi, byte as u8, 8, uv);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX vshufoeb/vshufoeh: even shuffle -> dst_lo, odd shuffle -> dst_hi.
            // out_lo[2i]=src2[2i], out_lo[2i+1]=src1[2i]; out_hi uses sub-lane 2i+1.
            OpKind::VShuffleEOPair {
                dst_lo,
                dst_hi,
                src1,
                src2,
                elem,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let nbits = elem.bytes() * 8;
                let total = (1024 / nbits) as u8;
                let half = total / 2;
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for i in 0..half {
                    let e = i * 2;
                    let o = i * 2 + 1;
                    Self::set_lane(&mut lo, i * 2, nbits, Self::get_lane(&v, e, nbits));
                    Self::set_lane(&mut lo, i * 2 + 1, nbits, Self::get_lane(&u, e, nbits));
                    Self::set_lane(&mut hi, i * 2, nbits, Self::get_lane(&v, o, nbits));
                    Self::set_lane(&mut hi, i * 2 + 1, nbits, Self::get_lane(&u, o, nbits));
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX in-place dual-register byte shuffle/deal: swap Vy.b[k] <-> Vx.b[k+offset].
            OpKind::VShuffleDeal {
                dst_y,
                dst_x,
                amount,
                deal,
            } => {
                let mut vy = Self::read_vec(ctx, *dst_y);
                let mut vx = Self::read_vec(ctx, *dst_x);
                let rt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                // shuffle: offset ascending 1..64; deal: descending 64..1.
                let offsets: [usize; 7] = if *deal {
                    [64, 32, 16, 8, 4, 2, 1]
                } else {
                    [1, 2, 4, 8, 16, 32, 64]
                };
                for &offset in offsets.iter() {
                    if rt & offset != 0 {
                        for k in 0..128usize {
                            if k & offset == 0 {
                                let a = Self::get_lane(&vy, k as u8, 8);
                                let b = Self::get_lane(&vx, (k + offset) as u8, 8);
                                Self::set_lane(&mut vy, k as u8, 8, b);
                                Self::set_lane(&mut vx, (k + offset) as u8, 8, a);
                            }
                        }
                    }
                }
                Self::write_vec(ctx, *dst_y, vy);
                Self::write_vec(ctx, *dst_x, vx);
            }

            // HVX vdealvdd: deal-direction byte swap network over a pair (lo=Vv, hi=Vu).
            OpKind::VDealVdd {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                amount,
            } => {
                let mut lo = Self::read_vec(ctx, *src_lo);
                let mut hi = Self::read_vec(ctx, *src_hi);
                let rt = match amount {
                    SrcOperand::Imm(v) => *v as usize,
                    SrcOperand::Reg(r) => ctx.read_vreg(*r) as usize,
                    _ => 0,
                };
                let mut offset = 64usize;
                while offset > 0 {
                    if rt & offset != 0 {
                        for k in 0..128usize {
                            if k & offset == 0 {
                                let a = Self::get_lane(&hi, k as u8, 8);
                                let b = Self::get_lane(&lo, (k + offset) as u8, 8);
                                Self::set_lane(&mut hi, k as u8, 8, b);
                                Self::set_lane(&mut lo, (k + offset) as u8, 8, a);
                            }
                        }
                    }
                    offset >>= 1;
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX vunpackob/oh: Vxx.<2w>[i] |= ZE(Vu.<w>[i]) << nbits (sequential split).
            OpKind::VUnpackOAcc {
                dst_lo,
                dst_hi,
                src,
                src_elem,
            } => {
                let s = Self::read_vec(ctx, *src);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let total = (1024 / nbits as usize); // narrow lanes total
                let half = (total / 2) as u8;
                let mut lo = Self::read_vec(ctx, *dst_lo);
                let mut hi = Self::read_vec(ctx, *dst_hi);
                for i in 0..total as u8 {
                    let add = Self::get_lane(&s, i, nbits) << nbits;
                    if i < half {
                        let cur = Self::get_lane(&lo, i, wbits);
                        Self::set_lane(&mut lo, i, wbits, cur | add);
                    } else {
                        let cur = Self::get_lane(&hi, i - half, wbits);
                        Self::set_lane(&mut hi, i - half, wbits, cur | add);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            // HVX vinsertwr: Vx.w[0] = Rt (other words preserved).
            OpKind::VInsertWordR { dst, scalar } => {
                let mut v = Self::read_vec(ctx, *dst);
                let rt = ctx.read_vreg(*scalar) as u32 as u64;
                Self::set_lane(&mut v, 0, 32, rt);
                Self::write_vec(ctx, *dst, v);
            }

            // HVX extractw: Rd = Vu.uw[(Rs & 127) >> 2].
            OpKind::VExtractWord { dst, src, sel } => {
                let v = Self::read_vec(ctx, *src);
                let rs = ctx.read_vreg(*sel) as u32;
                let idx = ((rs & 127) >> 2) as u8;
                let word = Self::get_lane(&v, idx, 32);
                ctx.write_vreg(*dst, word & 0xffff_ffff);
            }

            // HVX vlut4: Vd.h[i] = Rtt.h[(Vu.uh[i] >> 14) & 3].
            OpKind::VLut4 { dst, src, table } => {
                let u = Self::read_vec(ctx, *src);
                let rtt = ctx.read_vreg(*table);
                let mut out = [0u64; 16];
                for i in 0..64u8 {
                    let sel = (Self::get_lane(&u, i, 16) >> 14) & 3;
                    let entry = (rtt >> (sel * 16)) & 0xffff;
                    Self::set_lane(&mut out, i, 16, entry);
                }
                Self::write_vec(ctx, *dst, out);
            }

            // HVX vrotr: Vd.uw[i] = rotate_right(Vu.uw[i], Vv.uw[i] & 0x1f).
            OpKind::VRotr { dst, src, amount } => {
                let u = Self::read_vec(ctx, *src);
                let v = Self::read_vec(ctx, *amount);
                let mut out = [0u64; 16];
                for i in 0..32u8 {
                    let amt = (Self::get_lane(&v, i, 32) & 0x1f) as u32;
                    let val = Self::get_lane(&u, i, 32) as u32;
                    Self::set_lane(&mut out, i, 32, val.rotate_right(amt) as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            // HVX vaddububb_sat/vsubububb_sat: Vd.ub = sat_u8(Vu.ub +/- Vv.b).
            OpKind::VAddSubMixedSat {
                dst,
                src1,
                src2,
                sub,
            } => {
                let u = Self::read_vec(ctx, *src1);
                let v = Self::read_vec(ctx, *src2);
                let mut out = [0u64; 16];
                // vaddububb_sat/vsubububb_sat are dedicated; their sem
                // (hvx_addsub.rs) clamps via `ctx.satu_n(r, 8)`, setting USR:OVF
                // on any clamped lane.
                let mut ovf = false;
                for i in 0..128u8 {
                    let a = Self::get_lane(&u, i, 8) as i32; // unsigned byte
                    let b = Self::get_lane(&v, i, 8) as u8 as i8 as i32; // signed byte
                    let r = if *sub { a - b } else { a + b };
                    if r < 0 || r > 255 {
                        ovf = true;
                    }
                    let s = r.clamp(0, 255) as u64;
                    Self::set_lane(&mut out, i, 8, s);
                }
                Self::write_vec(ctx, *dst, out);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            // HVX vsetq / vsetq2: build a Q vector predicate from a scalar length.
            OpKind::VSetPredQ { dst, scalar, v2 } => {
                let rt = ctx.read_vreg(*scalar) as u32;
                let mut q = [0u64; 16];
                if *v2 {
                    // vsetq2: set bits 0..=((Rt-1) & 127) (Rt==0 -> all 128).
                    let last = (rt.wrapping_sub(1) & 127) as usize;
                    for i in 0..=last {
                        q[i >> 6] |= 1u64 << (i & 63);
                    }
                } else {
                    // vsetq: set the low (Rt & 127) bits.
                    let n = (rt & 127) as usize;
                    for i in 0..n {
                        q[i >> 6] |= 1u64 << (i & 63);
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            // HVX shuffeqh/shuffeqw: Q-predicate shrink/shuffle.
            OpKind::VShuffEqQ {
                dst,
                src1,
                src2,
                stride,
            } => {
                let qs = Self::read_vec(ctx, *src1);
                let qt = Self::read_vec(ctx, *src2);
                let qbit = |q: &VecValue, i: usize| (q[i >> 6] >> (i & 63)) & 1 != 0;
                let st = *stride as usize;
                let mut q = [0u64; 16];
                for i in 0..128usize {
                    let bit = if i & st != 0 {
                        qbit(&qs, i - st)
                    } else {
                        qbit(&qt, i)
                    };
                    if bit {
                        q[i >> 6] |= 1u64 << (i & 63);
                    }
                }
                Self::write_vec(ctx, *dst, q);
            }

            // HVX vmpahhsat/vmpauhuhsat/vmpsuhuhsat: saturating halfword mpa pair-scalar.
            OpKind::VMpaHhSat {
                dst,
                src,
                table,
                signed_u,
                signed_t,
                shl,
                sub,
            } => {
                let vx = Self::read_vec(ctx, *dst);
                let vu = Self::read_vec(ctx, *src);
                let rtt = ctx.read_vreg(*table);
                let mut out = [0u64; 16];
                // vmpahhsat/vmpauhuhsat/vmpsuhuhsat are dedicated; their sem
                // (hvx_mpys.rs) clamps via `ctx.sat_n(prod >> 16, 16)`, setting
                // USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..64u8 {
                    let x = Self::get_lane(&vx, i, 16) as u16 as i16 as i64; // Vx.h signed
                    let raw = Self::get_lane(&vu, i, 16) as u16;
                    let u = if *signed_u {
                        raw as i16 as i64
                    } else {
                        raw as i64
                    };
                    let idx = ((raw >> 14) & 3) as u64;
                    let t_raw = ((rtt >> (idx * 16)) & 0xffff) as u16;
                    let t = if *signed_t {
                        t_raw as i16 as i64
                    } else {
                        t_raw as i64
                    };
                    let addend = t << 15;
                    // vmps subtracts the scalar term; vmpa adds it.
                    let prod = ((x * u) << *shl) + if *sub { -addend } else { addend };
                    let v = prod >> 16;
                    if v < -(1i64 << 15) || v > (1i64 << 15) - 1 {
                        ovf = true;
                    }
                    let r = v.clamp(-(1i64 << 15), (1i64 << 15) - 1);
                    Self::set_lane(&mut out, i, 16, r as u64 & 0xffff);
                }
                Self::write_vec(ctx, *dst, out);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            // HVX vmpyhsat_acc: Vxx.w[i] += sat32(Vu.h[2i/2i+1] * Rt.h[0/1]).
            OpKind::VMpyHsatAcc {
                dst_lo,
                dst_hi,
                src,
                scalar,
            } => {
                let vu = Self::read_vec(ctx, *src);
                let rt = ctx.read_vreg(*scalar) as u32;
                let rt0 = (rt & 0xffff) as u16 as i16 as i64;
                let rt1 = ((rt >> 16) & 0xffff) as u16 as i16 as i64;
                let mut lo = Self::read_vec(ctx, *dst_lo);
                let mut hi = Self::read_vec(ctx, *dst_hi);
                let smin = -(1i64 << 31);
                let smax = (1i64 << 31) - 1;
                // vmpyhsat_acc is dedicated; its sem (hvx_mpyv.rs) clamps via
                // `ctx.sat_n(.., 32)`, setting USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..32u8 {
                    let p0 = (Self::get_lane(&vu, 2 * i, 16) as u16 as i16 as i64) * rt0;
                    let p1 = (Self::get_lane(&vu, 2 * i + 1, 16) as u16 as i16 as i64) * rt1;
                    let a0 = Self::get_lane(&lo, i, 32) as u32 as i32 as i64;
                    let a1 = Self::get_lane(&hi, i, 32) as u32 as i32 as i64;
                    let r0 = a0 + p0;
                    let r1 = a1 + p1;
                    if r0 < smin || r0 > smax || r1 < smin || r1 > smax {
                        ovf = true;
                    }
                    let s0 = r0.clamp(smin, smax);
                    let s1 = r1.clamp(smin, smax);
                    Self::set_lane(&mut lo, i, 32, s0 as u64 & 0xffff_ffff);
                    Self::set_lane(&mut hi, i, 32, s1 as u64 & 0xffff_ffff);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            // HVX vasr_into: shift Vu.w into the running accumulator pair Vxx.
            OpKind::VAsrInto {
                dst_lo,
                dst_hi,
                src,
                amount,
            } => {
                let vu = Self::read_vec(ctx, *src);
                let vv = Self::read_vec(ctx, *amount);
                let mut x0 = Self::read_vec(ctx, *dst_lo); // Vxx.v[0]
                let mut x1 = Self::read_vec(ctx, *dst_hi); // Vxx.v[1]
                for i in 0..32u8 {
                    // fSE32_64(Vu.w[i]) << 32 — Vu.w is SIGN-extended in the sem.
                    let shift = ((Self::get_lane(&vu, i, 32) as u32 as i32 as i64) << 32) as i64;
                    let xlo = Self::get_lane(&x0, i, 32) as u32 as i64; // ZE lo
                    // SE hi: (fSE32_64(x0.w[i]) << 32) | ZE lo (matches sem's get_w<<32).
                    let xhi = (Self::get_lane(&x0, i, 32) as u32 as i32 as i64) << 32;
                    let mask = xhi | xlo;
                    let lomask: i64 = (1i64 << 32) - 1;
                    let vvw = Self::get_lane(&vv, i, 32) as u32 as i32;
                    let count = -(0x40 & vvw) + (vvw & 0x3f);
                    let result: i64 = if count == -0x40 {
                        0
                    } else if count < 0 {
                        let n = (-count) as u32;
                        (shift << n) | (mask & (lomask << n))
                    } else {
                        let n = count as u32;
                        (shift >> n) | (mask & ((lomask as u64 >> n) as i64))
                    };
                    Self::set_lane(&mut x1, i, 32, ((result >> 32) & 0xffff_ffff) as u64);
                    Self::set_lane(&mut x0, i, 32, (result & 0xffff_ffff) as u64);
                }
                Self::write_vec(ctx, *dst_lo, x0);
                Self::write_vec(ctx, *dst_hi, x1);
            }

            // HVX v6mpy: V69 byte-matrix multiply with packed signed-10-bit coeffs.
            OpKind::V6Mpy {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2_lo,
                src2_hi,
                horizontal,
                phase,
                acc,
            } => {
                let u0 = Self::read_vec(ctx, *src_lo); // Vuu.v[0]
                let u1 = Self::read_vec(ctx, *src_hi); // Vuu.v[1]
                let cv0 = Self::read_vec(ctx, *src2_lo); // Vvv.v[0] -> c0j
                let cv1 = Self::read_vec(ctx, *src2_hi); // Vvv.v[1] -> c1j
                // unsigned byte k (0..3) of word lane i.
                let ub = |b: &VecValue, i: u8, k: u8| -> i64 {
                    (Self::get_lane(b, i * 4 + k, 8) & 0xff) as i64
                };
                // signed 10-bit coeff j (0..2) of word lane i: lo8 from ub[j], hi2 from ub[3]>>(2j).
                let coeff = |b: &VecValue, i: u8, j: u8| -> i64 {
                    let hi2 = (ub(b, i, 3) >> (2 * j)) & 3;
                    let lo8 = ub(b, i, j);
                    let v10 = (hi2 << 8) | lo8;
                    ((v10 & 0x3ff) << 54) >> 54
                };
                let terms = Self::v6mpy_terms(*horizontal, *phase);
                let mut o0 = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut o1 = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                for i in 0..32u8 {
                    let c = [
                        coeff(&cv0, i, 0),
                        coeff(&cv0, i, 1),
                        coeff(&cv0, i, 2),
                        coeff(&cv1, i, 0),
                        coeff(&cv1, i, 1),
                        coeff(&cv1, i, 2),
                    ];
                    let mut s0 = if *acc {
                        Self::get_lane(&o0, i, 32) as u32 as i32 as i64
                    } else {
                        0
                    };
                    let mut s1 = if *acc {
                        Self::get_lane(&o1, i, 32) as u32 as i32 as i64
                    } else {
                        0
                    };
                    for &(vsel, byte, ci, osel) in terms {
                        let uv = if vsel == 0 { &u0 } else { &u1 };
                        let prod = ub(uv, i, byte) * c[ci as usize];
                        if osel == 0 {
                            s0 = s0.wrapping_add(prod);
                        } else {
                            s1 = s1.wrapping_add(prod);
                        }
                    }
                    Self::set_lane(&mut o0, i, 32, s0 as u64 & 0xffff_ffff);
                    Self::set_lane(&mut o1, i, 32, s1 as u64 & 0xffff_ffff);
                }
                Self::write_vec(ctx, *dst_lo, o0);
                Self::write_vec(ctx, *dst_hi, o1);
            }

            OpKind::VCondMove {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                pred,
                negate,
            } => {
                let p = ctx.read_vreg(*pred) & 1;
                let take = if *negate { p == 0 } else { p != 0 };
                if take {
                    let lo = Self::read_vec(ctx, *src_lo);
                    Self::write_vec(ctx, *dst_lo, lo);
                    if let Some(hi) = dst_hi {
                        let hv = Self::read_vec(ctx, *src_hi);
                        Self::write_vec(ctx, *hi, hv);
                    }
                }
                // CANCEL (no write) when the condition is false.
            }

            OpKind::VPrefixSumQ {
                dst,
                mask_q,
                elem,
                lanes,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let elem_bits = elem.bytes() * 8;
                let ebytes = elem.bytes() as usize;
                let mut result = [0u64; 16];
                let mut acc: u64 = 0;
                for lane in 0..*lanes {
                    let base = lane as usize * ebytes;
                    for byte in 0..ebytes {
                        let bidx = base + byte;
                        acc = acc.wrapping_add((m[bidx >> 6] >> (bidx & 63)) & 1);
                    }
                    Self::set_lane(&mut result, lane, elem_bits, acc);
                }
                Self::write_vec(ctx, *dst, result);
            }

            // HVX histogram family. Read-modify-writes the WHOLE V0..V31 register
            // file (treated as a 32 x 128-byte bin matrix), tallying values from
            // the 128-byte input vector (re-read from the `.tmp` load's address in
            // guest memory). Ported exactly from sem/hvx_hist.rs.
            OpKind::VHist {
                input,
                aligned,
                mask_q,
                use_q,
                imm_match,
                sat,
                kind,
            } => {
                // 1) Read the 128 input bytes from memory at the .tmp address.
                let mut ea = self.compute_address(ctx, input);
                if *aligned {
                    ea &= !127u64;
                }
                let mut inp = [0u8; 128];
                memory.read(ea, &mut inp)?;

                // 2) Read the WHOLE V file into a 32 x 128-byte bin matrix.
                let mut file = [[0u8; 128]; 32];
                for r in 0..32u8 {
                    let v = Self::read_vec(ctx, VReg::Arch(ArchReg::Hexagon(HexagonReg::V(r))));
                    for w in 0..16usize {
                        file[r as usize][w * 8..w * 8 + 8].copy_from_slice(&v[w].to_le_bytes());
                    }
                }

                // q-mask (vector-byte predicate bits) for the q-forms.
                let qv = if *use_q {
                    Some(Self::read_vec(ctx, *mask_q))
                } else {
                    None
                };
                // Q layout in a VecValue: bit i lives in lane (i>>6), bit (i&63).
                let qbit = |q: &VecValue, i: usize| -> bool { (q[i >> 6] >> (i & 63)) & 1 != 0 };
                let get_uh = |f: &[[u8; 128]; 32], reg: usize, i: usize| -> u32 {
                    u16::from_le_bytes([f[reg][i * 2], f[reg][i * 2 + 1]]) as u32
                };
                let set_uh = |f: &mut [[u8; 128]; 32], reg: usize, i: usize, val: u32| {
                    f[reg][i * 2..i * 2 + 2].copy_from_slice(&(val as u16).to_le_bytes());
                };
                let get_uw = |f: &[[u8; 128]; 32], reg: usize, i: usize| -> u32 {
                    u32::from_le_bytes([
                        f[reg][i * 4],
                        f[reg][i * 4 + 1],
                        f[reg][i * 4 + 2],
                        f[reg][i * 4 + 3],
                    ])
                };
                let set_uw = |f: &mut [[u8; 128]; 32], reg: usize, i: usize, val: u32| {
                    f[reg][i * 4..i * 4 + 4].copy_from_slice(&val.to_le_bytes());
                };

                // 3) Run the bin-update loop for this family.
                match *kind {
                    // vhist / vhistq: 8 lanes x 16 bytes -> uh bins, += 1.
                    0 => {
                        for lane in 0..8usize {
                            for i in 0..16usize {
                                if let Some(ref q) = qv {
                                    if !qbit(q, 16 * lane + i) {
                                        continue;
                                    }
                                }
                                let value = inp[16 * lane + i] as usize;
                                let regno = value >> 3;
                                let element = value & 7;
                                let idx = 8 * lane + element;
                                let cur = get_uh(&file, regno, idx);
                                set_uh(&mut file, regno, idx, cur.wrapping_add(1) & 0xffff);
                            }
                        }
                    }
                    // vwhist128 family: 64 halfwords -> uw bins, += weight.
                    1 => {
                        for i in 0..64usize {
                            let bucket = inp[2 * i] as usize;
                            let weight = inp[2 * i + 1] as u32;
                            let vindex = (bucket >> 3) & 0x1f;
                            let elindex = ((i >> 1) & !3) | ((bucket >> 1) & 3);
                            let mut cond = true;
                            if let Some(u) = imm_match {
                                cond &= (bucket & 1) as u8 == *u;
                            }
                            if let Some(ref q) = qv {
                                cond &= qbit(q, 2 * i);
                            }
                            if cond {
                                let cur = get_uw(&file, vindex, elindex);
                                set_uw(&mut file, vindex, elindex, cur.wrapping_add(weight));
                            }
                        }
                    }
                    // vwhist256 family: 64 halfwords -> uh bins, += weight (opt sat).
                    _ => {
                        for i in 0..64usize {
                            let bucket = inp[2 * i] as usize;
                            let weight = inp[2 * i + 1] as u32;
                            let vindex = (bucket >> 3) & 0x1f;
                            let elindex = (i & !7) | (bucket & 7);
                            let cond = match qv {
                                Some(ref q) => qbit(q, 2 * i),
                                None => true,
                            };
                            if cond {
                                let sum = get_uh(&file, vindex, elindex).wrapping_add(weight);
                                let val = if *sat { sum.min(0xffff) } else { sum & 0xffff };
                                set_uh(&mut file, vindex, elindex, val);
                            }
                        }
                    }
                }

                // 4) Write the WHOLE V file back.
                for r in 0..32u8 {
                    let mut v = [0u64; 16];
                    for w in 0..16usize {
                        v[w] = u64::from_le_bytes([
                            file[r as usize][w * 8],
                            file[r as usize][w * 8 + 1],
                            file[r as usize][w * 8 + 2],
                            file[r as usize][w * 8 + 3],
                            file[r as usize][w * 8 + 4],
                            file[r as usize][w * 8 + 5],
                            file[r as usize][w * 8 + 6],
                            file[r as usize][w * 8 + 7],
                        ]);
                    }
                    Self::write_vec(ctx, VReg::Arch(ArchReg::Hexagon(HexagonReg::V(r))), v);
                }
            }

            OpKind::VBlend {
                dst,
                mask_q,
                src_true,
                src_false,
            } => {
                let m = Self::read_vec(ctx, *mask_q);
                let t = Self::read_vec(ctx, *src_true);
                let f = Self::read_vec(ctx, *src_false);
                let mut result = [0u64; 16];
                for byte in 0..128usize {
                    let bit_set = (m[byte >> 6] >> (byte & 63)) & 1 != 0;
                    let src = if bit_set { &t } else { &f };
                    Self::set_lane(
                        &mut result,
                        byte as u8,
                        8,
                        Self::get_lane(src, byte as u8, 8),
                    );
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VShiftV {
                dst,
                src,
                amount,
                elem,
                lanes,
                kind,
            } => {
                let s = Self::read_vec(ctx, *src);
                let amt = Self::read_vec(ctx, *amount);
                let nbits = elem.bytes() * 8;
                let n_amt = nbits.trailing_zeros() + 1; // 16->5, 32->6
                let mut result = [0u64; 16];
                for i in 0..*lanes {
                    let raw = Self::get_lane(&s, i, nbits);
                    // sign-extend the low n_amt bits of the amount lane.
                    let araw = Self::get_lane(&amt, i, nbits) & ((1u64 << n_amt) - 1);
                    let sh = 64 - n_amt;
                    let shamt = (((araw << sh) as i64) >> sh) as i32;
                    let sext = |v: u64| -> i64 {
                        let sh = 64 - nbits;
                        ((v << sh) as i64) >> sh
                    };
                    let out: u64 = match kind {
                        VShiftVKind::AshiftL => {
                            let sa = sext(raw);
                            if shamt >= 0 {
                                (sa << shamt) as u64
                            } else {
                                (sa >> (-shamt)) as u64
                            }
                        }
                        VShiftVKind::AshiftR => {
                            let sa = sext(raw);
                            if shamt >= 0 {
                                (sa >> shamt) as u64
                            } else {
                                (sa << (-shamt)) as u64
                            }
                        }
                        VShiftVKind::LshiftR => {
                            if shamt >= 0 {
                                raw >> shamt
                            } else {
                                raw << (-shamt)
                            }
                        }
                    };
                    Self::set_lane(&mut result, i, nbits, out);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VMulShiftSat {
                dst,
                src1,
                src2,
                src_elem,
                lanes,
                signed1,
                signed2,
                shift_left,
                round,
                sat_bits,
                out_shift,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let ext = |raw: u64, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - nbits;
                        ((raw << sh) as i64) >> sh
                    } else {
                        raw as i64
                    }
                };
                let mut result = [0u64; 16];
                for i in 0..*lanes {
                    let mut p = ext(Self::get_lane(&a, i, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, i, nbits), *signed2));
                    p <<= *shift_left;
                    if *round {
                        p += 1i64 << (*out_shift - 1);
                    }
                    if *sat_bits != 0 {
                        let lo = -(1i64 << (*sat_bits - 1));
                        let hi = (1i64 << (*sat_bits - 1)) - 1;
                        p = p.clamp(lo, hi);
                    }
                    Self::set_lane(&mut result, i, nbits, (p >> *out_shift) as u64);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VNarrowShiftSat {
                dst,
                src_lo,
                src_hi,
                src_elem,
                amount,
                arith,
                round,
                sat,
                set_ovf,
            } => {
                let lo_src = Self::read_vec(ctx, *src_lo);
                let hi_src = Self::read_vec(ctx, *src_hi);
                let wbits = src_elem.bytes() * 8; // wide source element bits
                let nbits = wbits / 2; // narrow output element bits
                let wide_lanes = (1024 / wbits) as u8;
                // Rt-sourced shift amounts are masked to narrow_bits-1 bits
                // (sem: `rt & 0xF` for word->half, `rt & 0x7` for half->byte);
                // immediates (vround/vsat) are used verbatim.
                let shamt: u32 = match amount {
                    SrcOperand::Reg(r) => (ctx.read_vreg(*r) as u32) & (nbits - 1),
                    SrcOperand::Imm(v) | SrcOperand::Imm64(v) => *v as u32,
                    _ => 0,
                };
                // Extend a wide lane to i64 per signedness.
                let ext = |raw: u64| -> i64 {
                    if *arith {
                        let sh = 64 - wbits;
                        ((raw << sh) as i64) >> sh
                    } else {
                        raw as i64
                    }
                };
                // Shift-round one wide lane and saturate to the narrow width.
                // Returns (narrowed value, clamped?) where `clamped` mirrors the
                // sem's `ctx.sat_n`/`ctx.satu_n` overflow flag (value outside the
                // target range BEFORE clamping).
                let narrow = |raw: u64| -> (u64, bool) {
                    let mut v = ext(raw);
                    if *round && shamt > 0 {
                        v += 1i64 << (shamt - 1);
                    }
                    v >>= shamt;
                    match sat {
                        // signed narrow
                        1 => {
                            let lo = -(1i64 << (nbits - 1));
                            let hi = (1i64 << (nbits - 1)) - 1;
                            let c = v < lo || v > hi;
                            ((v.clamp(lo, hi) as u64) & ((1u64 << nbits) - 1), c)
                        }
                        // unsigned narrow
                        2 => {
                            let hi = (1i64 << nbits) - 1;
                            let c = v < 0 || v > hi;
                            ((v.clamp(0, hi) as u64) & ((1u64 << nbits) - 1), c)
                        }
                        // truncate
                        _ => ((v as u64) & ((1u64 << nbits) - 1), false),
                    }
                };
                let mut result = [0u64; 16];
                let mut ovf = false;
                for i in 0..wide_lanes {
                    // even/low sub-lane <- src_lo (Vv); odd/high <- src_hi (Vu)
                    let (lv, lc) = narrow(Self::get_lane(&lo_src, i, wbits));
                    Self::set_lane(&mut result, 2 * i, nbits, lv);
                    let (hv, hc) = narrow(Self::get_lane(&hi_src, i, wbits));
                    Self::set_lane(&mut result, 2 * i + 1, nbits, hv);
                    ovf |= lc | hc;
                }
                Self::write_vec(ctx, *dst, result);
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VSatDW {
                dst,
                src_lo,
                src_hi,
            } => {
                let lo = Self::read_vec(ctx, *src_lo);
                let hi = Self::read_vec(ctx, *src_hi);
                let mut result = [0u64; 16];
                // vsatdw is dedicated; its sem (hvx_round.rs) clamps via
                // `ctx.sat_n(val, 32)`, which sets USR:OVF on any clamped lane.
                let mut ovf = false;
                for i in 0..32u8 {
                    let h = Self::get_lane(&hi, i, 32) as i32 as i64; // sign-extended high word
                    let l = Self::get_lane(&lo, i, 32); // zero-extended low word
                    let val = (h << 32) | (l as i64);
                    if val < i32::MIN as i64 || val > i32::MAX as i64 {
                        ovf = true;
                    }
                    let s = val.clamp(i32::MIN as i64, i32::MAX as i64) as i32 as u32;
                    Self::set_lane(&mut result, i, 32, s as u64);
                }
                Self::write_vec(ctx, *dst, result);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VNarrowShiftV {
                dst,
                src_lo,
                src_hi,
                amount,
                src_elem,
                arith,
                round,
            } => {
                let lo_src = Self::read_vec(ctx, *src_lo);
                let hi_src = Self::read_vec(ctx, *src_hi);
                let amt = Self::read_vec(ctx, *amount);
                let wbits = src_elem.bytes() * 8;
                let nbits = wbits / 2;
                let wide_lanes = (1024 / wbits) as u8;
                let ext = |raw: u64| -> i64 {
                    if *arith {
                        let sh = 64 - wbits;
                        ((raw << sh) as i64) >> sh
                    } else {
                        raw as i64
                    }
                };
                // amount sub-lanes are narrow-width; mask to log2(narrow_bits).
                let amask = nbits - 1;
                // vasrv* always saturate to the unsigned narrow range via
                // `ctx.satu_n` (hvx_round.rs), so every clamped lane sets USR:OVF.
                let narrow = |raw: u64, s: u32| -> (u64, bool) {
                    let mut v = ext(raw);
                    if *round && s > 0 {
                        v += 1i64 << (s - 1);
                    }
                    v >>= s;
                    let hi = (1i64 << nbits) - 1;
                    let c = v < 0 || v > hi;
                    ((v.clamp(0, hi) as u64) & ((1u64 << nbits) - 1), c)
                };
                let mut result = [0u64; 16];
                let mut ovf = false;
                for i in 0..wide_lanes {
                    let s0 = (Self::get_lane(&amt, 2 * i, nbits) as u32) & amask;
                    let (v0, c0) = narrow(Self::get_lane(&lo_src, i, wbits), s0);
                    Self::set_lane(&mut result, 2 * i, nbits, v0);
                    let s1 = (Self::get_lane(&amt, 2 * i + 1, nbits) as u32) & amask;
                    let (v1, c1) = narrow(Self::get_lane(&hi_src, i, wbits), s1);
                    Self::set_lane(&mut result, 2 * i + 1, nbits, v1);
                    ovf |= c0 | c1;
                }
                Self::write_vec(ctx, *dst, result);
                if ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VPairPairReduceMul {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2_lo,
                src2_hi,
                narrow_elem,
                out_elem,
                signed1,
                signed2,
            } => {
                let u0 = Self::read_vec(ctx, *src_lo);
                let u1 = Self::read_vec(ctx, *src_hi);
                let v0 = Self::read_vec(ctx, *src2_lo);
                let v1 = Self::read_vec(ctx, *src2_hi);
                let nbits = narrow_elem.bytes() * 8;
                let obits = out_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ex = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - nbits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                for i in 0..olanes {
                    let plo = ex(Self::get_lane(&u0, i * 2, nbits), *signed1)
                        * ex(Self::get_lane(&v0, i * 2, nbits), *signed2)
                        + ex(Self::get_lane(&u1, i * 2, nbits), *signed1)
                            * ex(Self::get_lane(&v1, i * 2, nbits), *signed2);
                    let phi = ex(Self::get_lane(&u0, i * 2 + 1, nbits), *signed1)
                        * ex(Self::get_lane(&v0, i * 2 + 1, nbits), *signed2)
                        + ex(Self::get_lane(&u1, i * 2 + 1, nbits), *signed1)
                            * ex(Self::get_lane(&v1, i * 2 + 1, nbits), *signed2);
                    Self::set_lane(&mut lo, i, obits, plo as u64);
                    Self::set_lane(&mut hi, i, obits, phi as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VPairReduceMul {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2,
                pair_elem,
                rt_elem,
                out_elem,
                signed1,
                signed2,
                acc,
            } => {
                let u0 = Self::read_vec(ctx, *src_lo);
                let u1 = Self::read_vec(ctx, *src_hi);
                let r = Self::read_vec(ctx, *src2);
                let pbits = pair_elem.bytes() * 8;
                let rbits = rt_elem.bytes() * 8;
                let obits = out_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                let exg = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let rt = |k: u8| exg(Self::get_lane(&r, k, rbits), rbits, *signed2);
                for i in 0..olanes {
                    let plo = exg(Self::get_lane(&u0, i * 2, pbits), pbits, *signed1) * rt(0)
                        + exg(Self::get_lane(&u1, i * 2, pbits), pbits, *signed1) * rt(1);
                    let phi = exg(Self::get_lane(&u0, i * 2 + 1, pbits), pbits, *signed1) * rt(2)
                        + exg(Self::get_lane(&u1, i * 2 + 1, pbits), pbits, *signed1) * rt(3);
                    let alo = if *acc {
                        Self::get_lane(&lo, i, obits) as i64
                    } else {
                        0
                    };
                    let ahi = if *acc {
                        Self::get_lane(&hi, i, obits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(&mut lo, i, obits, alo.wrapping_add(plo) as u64);
                    Self::set_lane(&mut hi, i, obits, ahi.wrapping_add(phi) as u64);
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VSlideReduceMul {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2,
                src_elem,
                rt_elem,
                out_elem,
                mode,
                signed1,
                signed2,
                sat,
                set_ovf,
                acc,
            } => {
                let v0 = Self::read_vec(ctx, *src_lo);
                let v1 = Self::read_vec(ctx, *src_hi);
                let r = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8; // multiplicand width
                let rbits = rt_elem.bytes() * 8; // Rt sub-lane width
                let obits = out_elem.bytes() * 8; // output width
                let olanes = (1024 / obits) as u8;
                let ext = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                // narrow multiplicand lane reader
                let m = |vec: &VecValue, lane: u8| {
                    ext(Self::get_lane(vec, lane, nbits), nbits, *signed1)
                };
                // Rt sub-lane reader (from the I32-broadcast `src2`)
                let rt = |lane: u8| ext(Self::get_lane(&r, lane, rbits), rbits, *signed2);
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc && *mode != 2 {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // Returns (saturated value, clamped?). Only mode 2 saturates; its
                // sem (hvx_rmpy.rs) clamps via `ctx.sat_n`, flagging USR:OVF.
                let satn = |s: i64| -> (i64, bool) {
                    if *sat && obits < 64 {
                        let l = -(1i64 << (obits - 1));
                        let h = (1i64 << (obits - 1)) - 1;
                        (s.clamp(l, h), s < l || s > h)
                    } else {
                        (s, false)
                    }
                };
                let mut ovf = false;
                for i in 0..olanes {
                    let n0 = (2 * i) as u8; // narrow lane 2i
                    let n1 = (2 * i + 1) as u8; // narrow lane 2i+1
                    let rb0 = rt(n0); // Rt[(2i)%subs] via broadcast
                    let rb1 = rt(n1); // Rt[(2i+1)%subs]
                    match *mode {
                        0 => {
                            // _dv 2-tap sliding (pair -> pair)
                            let alo = if *acc {
                                Self::get_lane(&lo, i, obits) as i64
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(m(&v0, n0).wrapping_mul(rb0))
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb1));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                Self::get_lane(&hi, i, obits) as i64
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb0))
                                .wrapping_add(m(&v1, n0).wrapping_mul(rb1));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                        1 => {
                            // vtmpy 3-tap sliding with a free (un-multiplied) addend tap
                            let alo = if *acc {
                                Self::get_lane(&lo, i, obits) as i64
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(m(&v0, n0).wrapping_mul(rb0))
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb1))
                                .wrapping_add(m(&v1, n0));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                Self::get_lane(&hi, i, obits) as i64
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(m(&v0, n1).wrapping_mul(rb0))
                                .wrapping_add(m(&v1, n0).wrapping_mul(rb1))
                                .wrapping_add(m(&v1, n1));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                        _ => {
                            // mode 2: pair -> single, straddle, saturated. Rt taps are
                            // fixed sub-lanes 0/1 (Rt.h[0], Rt.h[1]) read from the
                            // I32-broadcast src2.
                            let acc_v = if *acc {
                                ext(Self::get_lane(&lo, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s = acc_v
                                .wrapping_add(m(&v0, n1).wrapping_mul(rt(0)))
                                .wrapping_add(m(&v1, n0).wrapping_mul(rt(1)));
                            let (sv, c) = satn(s);
                            ovf |= c;
                            Self::set_lane(&mut lo, i, obits, sv as u64);
                        }
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                if *mode != 2 {
                    Self::write_vec(ctx, *dst_hi, hi);
                }
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VRotReduceMulPair {
                dst_lo,
                dst_hi,
                src_lo,
                src_hi,
                src2,
                src_elem,
                rt_elem,
                out_elem,
                imm,
                mode,
                signed1,
                signed2,
                acc,
                abs_diff,
            } => {
                let v0 = Self::read_vec(ctx, *src_lo);
                let v1 = Self::read_vec(ctx, *src_hi);
                let r = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8; // multiplicand width
                let rbits = rt_elem.bytes() * 8; // Rt sub-lane width
                let obits = out_elem.bytes() * 8; // output width (I32)
                let olanes = (1024 / obits) as u8;
                let ext = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                // narrow multiplicand lane reader
                let m = |vec: &VecValue, lane: u8| {
                    ext(Self::get_lane(vec, lane, nbits), nbits, *signed1)
                };
                // Rt sub-lane reader (from the I32-broadcast `src2`)
                let rt = |lane: u8| ext(Self::get_lane(&r, lane, rbits), rbits, *signed2);
                let mut lo = if *acc {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let mut hi = if *acc {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                // per-tap kernel: mul (a*b) or sum-of-abs-diff (|a-b|).
                let kern = |a: i64, b: i64| -> i64 {
                    if *abs_diff {
                        (a - b).abs()
                    } else {
                        a.wrapping_mul(b)
                    }
                };
                let im = (*imm as usize) & 1;
                for i in 0..olanes {
                    match *mode {
                        0 => {
                            // byte window, #u1 source-select + Rt byte rotate by -imm.
                            let base = (i as u8) * 4;
                            // sel = imm ? src_hi : src_lo (taps 0 and 2 of dst_lo/hi)
                            let sel: &VecValue = if im != 0 { &v1 } else { &v0 };
                            // rb(n) = Rt.byte[(n - imm) & 3]
                            let rb = |n: usize| rt(((n.wrapping_sub(im)) & 3) as u8);
                            let alo = if *acc {
                                ext(Self::get_lane(&lo, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(kern(m(sel, base), rb(0)))
                                .wrapping_add(kern(m(&v0, base + 1), rb(1)))
                                .wrapping_add(kern(m(&v0, base + 2), rb(2)))
                                .wrapping_add(kern(m(&v0, base + 3), rb(3)));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                ext(Self::get_lane(&hi, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(kern(m(&v1, base), rb(2)))
                                .wrapping_add(kern(m(&v1, base + 1), rb(3)))
                                .wrapping_add(kern(m(sel, base + 2), rb(0)))
                                .wrapping_add(kern(m(&v0, base + 3), rb(1)));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                        _ => {
                            // mode 1: vdsaduh halfword window (imm ignored).
                            // r0 = Rt.uh[0] = t.h[0]; r1 = Rt.uh[1] = t.h[1].
                            let r0 = rt(0);
                            let r1 = rt(1);
                            let n0 = (i as u8) * 2; // halfword lane 2i
                            let n1 = (i as u8) * 2 + 1; // halfword lane 2i+1
                            let alo = if *acc {
                                ext(Self::get_lane(&lo, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s0 = alo
                                .wrapping_add(kern(m(&v0, n0), r0))
                                .wrapping_add(kern(m(&v0, n1), r1));
                            Self::set_lane(&mut lo, i, obits, s0 as u64);
                            let ahi = if *acc {
                                ext(Self::get_lane(&hi, i, obits), obits, true)
                            } else {
                                0
                            };
                            let s1 = ahi
                                .wrapping_add(kern(m(&v0, n1), r0))
                                .wrapping_add(kern(m(&v1, n0), r1));
                            Self::set_lane(&mut hi, i, obits, s1 as u64);
                        }
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VMulSubLane {
                dst,
                src1,
                src2,
                out_elem,
                sub_elem,
                odd,
                signed1,
                signed2,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let obits = out_elem.bytes() * 8;
                let sbits = sub_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ratio = (obits / sbits) as u8;
                let mut out = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let exts = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                for i in 0..olanes {
                    let s1 = exts(Self::get_lane(&a, i, obits), obits, *signed1);
                    let sub_idx = i * ratio + if *odd { 1 } else { 0 };
                    let s2 = exts(Self::get_lane(&b, sub_idx, sbits), sbits, *signed2);
                    let accv = if *acc {
                        Self::get_lane(&out, i, obits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(
                        &mut out,
                        i,
                        obits,
                        accv.wrapping_add(s1.wrapping_mul(s2)) as u64,
                    );
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VMulSubLaneFrac {
                dst,
                src1,
                src2,
                out_elem,
                sub_elem,
                odd,
                signed1,
                signed2,
                shl1,
                rnd,
                shift,
                sat,
                acc,
                rnd2,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let d = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let obits = out_elem.bytes() * 8;
                let sbits = sub_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ratio = (obits / sbits) as u8;
                let exf = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let mut out = [0u64; 16];
                for i in 0..olanes {
                    let s1 = exf(Self::get_lane(&a, i, obits), obits, *signed1);
                    let sub_idx = i * ratio + if *odd { 1 } else { 0 };
                    let s2 = exf(Self::get_lane(&b, sub_idx, sbits), sbits, *signed2);
                    let mut p = s1.wrapping_mul(s2);
                    if *shl1 {
                        p <<= 1;
                    }
                    if *acc {
                        // sacc: add the existing full-precision dst lane before shifting.
                        p += exf(Self::get_lane(&d, i, obits), obits, true);
                    }
                    if *rnd2 {
                        p = ((p >> (*shift - 1)) + 1) >> 1;
                    } else {
                        if *rnd && *shift > 0 {
                            p += 1i64 << (*shift - 1);
                        }
                        p >>= *shift;
                    }
                    if *sat && obits < 64 {
                        let lo = -(1i64 << (obits - 1));
                        let hi = (1i64 << (obits - 1)) - 1;
                        p = p.clamp(lo, hi);
                    }
                    Self::set_lane(&mut out, i, obits, p as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VMulSubLaneSh {
                dst,
                src1,
                src2,
                out_elem,
                sub_elem,
                odd1,
                odd2,
                signed1,
                signed2,
                shl,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let obits = out_elem.bytes() * 8;
                let sbits = sub_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let ratio = (obits / sbits) as u8;
                let exts = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - bits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                let mut out = [0u64; 16];
                for i in 0..olanes {
                    let i1 = i * ratio + if *odd1 { 1 } else { 0 };
                    let i2 = i * ratio + if *odd2 { 1 } else { 0 };
                    let s1 = exts(Self::get_lane(&a, i1, sbits), sbits, *signed1);
                    let s2 = exts(Self::get_lane(&b, i2, sbits), sbits, *signed2);
                    let p = s1.wrapping_mul(s2).wrapping_shl(*shl as u32);
                    Self::set_lane(&mut out, i, obits, p as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VMulWord64Pair {
                dst_lo,
                dst_hi,
                src1,
                src2,
                mode,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                // word i: 32-bit lane; src2 sub-halfwords at 2i (even/uh0) and 2i+1 (odd/h1).
                let mut lo = [0u64; 16];
                let mut hi = [0u64; 16];
                let old_lo = if *mode == 1 {
                    Self::read_vec(ctx, *dst_lo)
                } else {
                    [0u64; 16]
                };
                let old_hi = if *mode == 1 {
                    Self::read_vec(ctx, *dst_hi)
                } else {
                    [0u64; 16]
                };
                for i in 0..32u8 {
                    let uw = Self::get_lane(&a, i, 32) as u32 as i32 as i64;
                    if *mode == 0 {
                        // vmpyewuh_64: src2.uh[2i] (low, unsigned).
                        let uh0 = (Self::get_lane(&b, i, 32) as u32 & 0xffff) as i64;
                        let prod = uw * uh0;
                        Self::set_lane(&mut hi, i, 32, (prod >> 16) as u32 as u64);
                        Self::set_lane(&mut lo, i, 32, (prod << 16) as u32 as u64);
                    } else {
                        // vmpyowh_64_acc: src2.h[2i+1] (high, signed), accumulate dst_hi.
                        let h1 = ((Self::get_lane(&b, i, 32) as u32) >> 16) as u16 as i16 as i64;
                        let acc_hi = Self::get_lane(&old_hi, i, 32) as u32 as i32 as i64;
                        let prod = uw * h1 + acc_hi;
                        Self::set_lane(&mut hi, i, 32, (prod >> 16) as u32 as u64);
                        let lo_h0 = ((Self::get_lane(&old_lo, i, 32) as u32) >> 16) & 0xffff;
                        let lo_h1 = (prod as u32) & 0xffff;
                        Self::set_lane(&mut lo, i, 32, ((lo_h1 << 16) | lo_h0) as u64);
                    }
                }
                Self::write_vec(ctx, *dst_lo, lo);
                Self::write_vec(ctx, *dst_hi, hi);
            }

            OpKind::VMulEvenWiden {
                dst,
                src1,
                src2,
                src_elem,
                signed1,
                signed2,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let nbits = src_elem.bytes() * 8;
                let wbits = nbits * 2;
                let olanes = (1024 / wbits) as u8;
                let mut out = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let ext = |v: u64, signed: bool| -> i64 {
                    if signed {
                        let sh = 64 - nbits;
                        ((v << sh) as i64) >> sh
                    } else {
                        v as i64
                    }
                };
                for i in 0..olanes {
                    let p = ext(Self::get_lane(&a, i * 2, nbits), *signed1)
                        .wrapping_mul(ext(Self::get_lane(&b, i * 2, nbits), *signed2));
                    let acc_v = if *acc {
                        Self::get_lane(&out, i, wbits) as i64
                    } else {
                        0
                    };
                    Self::set_lane(&mut out, i, wbits, acc_v.wrapping_add(p) as u64);
                }
                Self::write_vec(ctx, *dst, out);
            }

            OpKind::VReduceMul {
                dst,
                src1,
                src2,
                src1_elem,
                src2_elem,
                out_elem,
                taps,
                signed1,
                signed2,
                sat,
                set_ovf,
                acc,
            } => {
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let n1 = src1_elem.bytes() * 8;
                let n2 = src2_elem.bytes() * 8;
                let obits = out_elem.bytes() * 8;
                let olanes = (1024 / obits) as u8;
                let mut out = if *acc {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let ext = |v: u64, bits: u32, signed: bool| -> i64 {
                    if signed {
                        let shift = 64 - bits;
                        ((v << shift) as i64) >> shift
                    } else {
                        v as i64
                    }
                };
                let mut ovf = false;
                for i in 0..olanes {
                    let mut s: i64 = if *acc {
                        // accumulator low `obits` bits, sign-extended for saturating sum.
                        ext(Self::get_lane(&out, i, obits), obits, true)
                    } else {
                        0
                    };
                    for k in 0..*taps {
                        let idx = i * *taps + k;
                        s = s.wrapping_add(
                            ext(Self::get_lane(&a, idx, n1), n1, *signed1).wrapping_mul(ext(
                                Self::get_lane(&b, idx, n2),
                                n2,
                                *signed2,
                            )),
                        );
                    }
                    if *sat && obits < 64 {
                        let lo = -(1i64 << (obits - 1));
                        let hi = (1i64 << (obits - 1)) - 1;
                        // The saturating reduce opcodes clamp via `ctx.sat_n`,
                        // which flags USR:OVF on any clamped lane.
                        if s < lo || s > hi {
                            ovf = true;
                        }
                        s = s.clamp(lo, hi);
                    }
                    Self::set_lane(&mut out, i, obits, s as u64);
                }
                Self::write_vec(ctx, *dst, out);
                if *set_ovf && ovf {
                    Self::set_hex_ovf(ctx);
                }
            }

            OpKind::VMov { dst, src, width } => {
                let val = Self::read_vec(ctx, *src);
                if matches!(op.x86_hint, Some(X86OpHint::SseMov { .. })) {
                    let mut result = Self::read_vec(ctx, *dst);
                    let words = width.bytes() as usize / 8;
                    result[..words].copy_from_slice(&val[..words]);
                    Self::write_vec(ctx, *dst, result);
                } else if matches!(
                    op.x86_hint,
                    Some(X86OpHint::VexOp { .. } | X86OpHint::EvexOp { .. })
                ) && matches!(dst, VReg::Arch(ArchReg::X86(_)))
                {
                    let mut result = [0; 16];
                    let words = width.bytes() as usize / 8;
                    result[..words].copy_from_slice(&val[..words]);
                    Self::write_vec(ctx, *dst, result);
                } else {
                    Self::write_vec(ctx, *dst, val);
                }
            }

            OpKind::VShift {
                dst,
                src,
                amount,
                shift,
                elem,
                lanes,
            } => {
                let amt = match amount {
                    SrcOperand::Imm(val) => *val as u32,
                    SrcOperand::Reg(reg) => ctx.read_vreg(*reg) as u32,
                    _ => 0,
                };
                let elem_bits = elem.bytes() * 8;
                let mask = if elem_bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << elem_bits) - 1
                };
                let src_val = Self::read_vec(ctx, *src);
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let val = Self::get_lane(&src_val, lane, elem_bits);
                    let shifted = match shift {
                        ShiftOp::Lsl => (val << (amt % elem_bits)) & mask,
                        ShiftOp::Lsr => (val >> (amt % elem_bits)) & mask,
                        ShiftOp::Asr => {
                            // Sign-extend the element to i64 before the arithmetic
                            // shift (get_lane zero-extends), so high lanes are
                            // replicated with the element's sign bit, not 0.
                            let sv = if elem_bits >= 64 {
                                val as i64
                            } else {
                                let sh = 64 - elem_bits;
                                ((val << sh) as i64) >> sh
                            };
                            ((sv >> (amt % elem_bits)) as u64) & mask
                        }
                        _ => val,
                    };
                    Self::set_lane(&mut result, lane, elem_bits, shifted);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VBroadcast {
                dst,
                scalar,
                elem,
                lanes,
            } => {
                // Splat the low `elem` bits of the scalar register into every lane.
                let elem_bits = elem.bytes() * 8;
                let val = ctx.read_vreg(*scalar);
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    Self::set_lane(&mut result, lane, elem_bits, val);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VInsertLane {
                dst,
                vec,
                scalar,
                lane,
                elem,
            } => {
                let mut value = Self::read_vec(ctx, *vec);
                Self::set_lane(&mut value, *lane, elem.bytes() * 8, ctx.read_vreg(*scalar));
                Self::write_vec(ctx, *dst, value);
            }

            OpKind::VExtractLane {
                dst,
                vec,
                lane,
                elem,
                sign,
            } => {
                let bits = elem.bytes() * 8;
                let raw = Self::get_lane(&Self::read_vec(ctx, *vec), *lane, bits);
                let value = if *sign == SignExtend::Sign && bits < 64 {
                    (((raw << (64 - bits)) as i64) >> (64 - bits)) as u64
                } else {
                    raw
                };
                ctx.write_vreg(*dst, value);
            }

            OpKind::VCmp {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
            } => {
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let a = Self::read_vec(ctx, *src1);
                let b = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let signed = |value: u64| -> i64 {
                    if bits == 64 {
                        value as i64
                    } else {
                        let shift = 64 - bits;
                        ((value << shift) as i64) >> shift
                    }
                };
                let int_cmp = |av: u64, bv: u64| match cond {
                    VecCmpCond::Eq => av == bv,
                    VecCmpCond::Ne => av != bv,
                    VecCmpCond::Lt => signed(av) < signed(bv),
                    VecCmpCond::Le => signed(av) <= signed(bv),
                    VecCmpCond::Gt => signed(av) > signed(bv),
                    VecCmpCond::Ge => signed(av) >= signed(bv),
                    VecCmpCond::Ltu => av < bv,
                    VecCmpCond::Leu => av <= bv,
                    VecCmpCond::Gtu => av > bv,
                    VecCmpCond::Geu => av >= bv,
                };
                let fp_cmp = |av: f64, bv: f64| match cond {
                    VecCmpCond::Eq => av == bv,
                    VecCmpCond::Ne => av != bv,
                    VecCmpCond::Lt | VecCmpCond::Ltu => av < bv,
                    VecCmpCond::Le | VecCmpCond::Leu => av <= bv,
                    VecCmpCond::Gt | VecCmpCond::Gtu => av > bv,
                    VecCmpCond::Ge | VecCmpCond::Geu => av >= bv,
                };
                let f16_to_f64 = |raw: u16| -> f64 {
                    let sign = (u32::from(raw & 0x8000)) << 16;
                    let exp = (raw >> 10) & 0x1f;
                    let frac = raw & 0x03ff;
                    let bits32 = if exp == 0 {
                        if frac == 0 {
                            sign
                        } else {
                            let shift = frac.leading_zeros() - 6;
                            let normalized = (u32::from(frac) << (shift + 1)) & 0x03ff;
                            sign | ((112 - shift) << 23) | (normalized << 13)
                        }
                    } else if exp == 0x1f {
                        sign | 0x7f80_0000 | (u32::from(frac) << 13)
                    } else {
                        sign | ((u32::from(exp) + 112) << 23) | (u32::from(frac) << 13)
                    };
                    f64::from(f32::from_bits(bits32))
                };

                let mut result = [0u64; 16];
                let true_value = if bits == 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                for lane in 0..*lanes {
                    let av = Self::get_lane(&a, lane, bits);
                    let bv = Self::get_lane(&b, lane, bits);
                    let matched = match elem {
                        VecElementType::I8
                        | VecElementType::I16
                        | VecElementType::I32
                        | VecElementType::I64 => int_cmp(av, bv),
                        VecElementType::F16 => fp_cmp(f16_to_f64(av as u16), f16_to_f64(bv as u16)),
                        VecElementType::F32 => fp_cmp(
                            f64::from(f32::from_bits(av as u32)),
                            f64::from(f32::from_bits(bv as u32)),
                        ),
                        VecElementType::F64 => fp_cmp(f64::from_bits(av), f64::from_bits(bv)),
                    };
                    if matched {
                        Self::set_lane(&mut result, lane, bits, true_value);
                    }
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VShuffle {
                dst,
                src1,
                src2,
                indices,
                elem,
                lanes,
            } => {
                let first = Self::read_vec(ctx, *src1);
                let second = src2.map(|reg| Self::read_vec(ctx, reg));
                let selectors = Self::read_vec(ctx, *indices);
                let bits = elem.bytes() * 8;
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let index = Self::get_lane(&selectors, lane, bits);
                    let selected = if index < u64::from(*lanes) {
                        Self::get_lane(&first, index as u8, bits)
                    } else if let Some(second) = &second {
                        let second_index = index - u64::from(*lanes);
                        if second_index < u64::from(*lanes) {
                            Self::get_lane(second, second_index as u8, bits)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    Self::set_lane(&mut result, lane, bits, selected);
                }
                Self::write_vec(ctx, *dst, result);
            }

            OpKind::VInterleave {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                high,
            } => {
                debug_assert!(*block_lanes != 0 && *block_lanes % 2 == 0);
                debug_assert!(*lanes % *block_lanes == 0);
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let half = *block_lanes / 2;
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let within_block = lane % *block_lanes;
                    let block_base = lane - within_block;
                    let source_lane = block_base + if *high { half } else { 0 } + within_block / 2;
                    let source = if within_block & 1 == 0 {
                        &first
                    } else {
                        &second
                    };
                    let selected = Self::get_lane(source, source_lane, bits);
                    Self::set_lane(&mut result, lane, bits, selected);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VByteShuffle {
                dst,
                src,
                control,
                lanes,
                block_lanes,
            } => {
                debug_assert!(block_lanes.is_power_of_two());
                debug_assert!(*block_lanes != 0 && *lanes % *block_lanes == 0);
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let source = Self::read_vec(ctx, *src);
                let selectors = Self::read_vec(ctx, *control);
                let mut result = [0u64; 16];
                for lane in 0..*lanes {
                    let selector = Self::get_lane(&selectors, lane, 8) as u8;
                    let selected = if selector & 0x80 != 0 {
                        0
                    } else {
                        let block_base = (lane / *block_lanes) * *block_lanes;
                        let source_lane = block_base + (selector & (*block_lanes - 1));
                        Self::get_lane(&source, source_lane, 8)
                    };
                    Self::set_lane(&mut result, lane, 8, selected);
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VHorizontalBin {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                subtract,
                saturating,
            } => {
                debug_assert!(*block_lanes != 0 && *block_lanes % 2 == 0);
                debug_assert!(*lanes % *block_lanes == 0);
                debug_assert!(matches!(elem, VecElementType::I16 | VecElementType::I32));
                debug_assert!(!*saturating || *elem == VecElementType::I16);
                let old = Self::legacy_xmm_snapshot(ctx, *dst, x86_hint);
                let first = Self::read_vec(ctx, *src1);
                let second = Self::read_vec(ctx, *src2);
                let bits = elem.bytes() * 8;
                let mask = (1u64 << bits) - 1;
                let calculate = |a: u64, b: u64| -> u64 {
                    if *saturating {
                        let shift = 64 - bits;
                        let lhs = ((a << shift) as i64) >> shift;
                        let rhs = ((b << shift) as i64) >> shift;
                        let value = if *subtract { lhs - rhs } else { lhs + rhs };
                        let low = -(1i64 << (bits - 1));
                        let high = (1i64 << (bits - 1)) - 1;
                        value.clamp(low, high) as u64 & mask
                    } else if *subtract {
                        a.wrapping_sub(b) & mask
                    } else {
                        a.wrapping_add(b) & mask
                    }
                };
                let mut result = [0u64; 16];
                let half = *block_lanes / 2;
                for block_base in (0..*lanes).step_by(*block_lanes as usize) {
                    for pair in 0..half {
                        let lhs_lane = block_base + pair * 2;
                        let rhs_lane = lhs_lane + 1;
                        Self::set_lane(
                            &mut result,
                            block_base + pair,
                            bits,
                            calculate(
                                Self::get_lane(&first, lhs_lane, bits),
                                Self::get_lane(&first, rhs_lane, bits),
                            ),
                        );
                        Self::set_lane(
                            &mut result,
                            block_base + half + pair,
                            bits,
                            calculate(
                                Self::get_lane(&second, lhs_lane, bits),
                                Self::get_lane(&second, rhs_lane, bits),
                            ),
                        );
                    }
                }
                Self::write_vec(ctx, *dst, result);
                Self::restore_legacy_xmm_upper(ctx, *dst, old);
            }

            OpKind::VLoad { dst, addr, width } => {
                let effective_addr = self.compute_address(ctx, addr);
                let mut buf = [0u8; 64];
                let size = width.bytes() as usize;
                memory.read(effective_addr, &mut buf[..size])?;

                let mut vec = if matches!(op.x86_hint, Some(X86OpHint::SseMov { .. })) {
                    Self::read_vec(ctx, *dst)
                } else {
                    [0u64; 16]
                };
                let words = (size + 7) / 8;
                for i in 0..words {
                    let start = i * 8;
                    let end = start + 8;
                    vec[i] = u64::from_le_bytes(buf[start..end].try_into().unwrap());
                }

                Self::write_vec(ctx, *dst, vec);
            }

            OpKind::PredVLoad {
                dst,
                cond,
                addr,
                width,
            } => {
                if ctx.read_vreg(*cond) & 1 != 0 {
                    let effective_addr = self.compute_address(ctx, addr);
                    let mut buf = [0u8; 64];
                    let size = width.bytes() as usize;
                    memory.read(effective_addr, &mut buf[..size])?;

                    let mut vec = [0u64; 16];
                    for (word, chunk) in buf[..size].chunks_exact(8).enumerate() {
                        vec[word] = u64::from_le_bytes(chunk.try_into().unwrap());
                    }
                    Self::write_vec(ctx, *dst, vec);
                }
            }

            OpKind::VStore { src, addr, width } => {
                let effective_addr = self.compute_address(ctx, addr);
                let val = Self::read_vec(ctx, *src);

                let size = width.bytes() as usize;
                let mut buf = [0u8; 64];
                let words = (size + 7) / 8;
                for i in 0..words {
                    let start = i * 8;
                    let end = start + 8;
                    buf[start..end].copy_from_slice(&val[i].to_le_bytes());
                }

                memory.write(effective_addr, &buf[..size])?;
            }

            _ => return self.execute_op_unary(ctx, memory, op),
        }

        Ok(())
    }
}
