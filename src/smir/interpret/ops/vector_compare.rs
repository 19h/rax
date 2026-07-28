//! Architecture-neutral vector comparison execution.

use crate::smir::interpret::SmirInterpreter;
use crate::smir::ir::context::SmirContext;
use crate::smir::ir::ops::X86OpHint;
use crate::smir::ir::types::{VLaneOp, VReg, VecCmpCond, VecElementType};

impl SmirInterpreter {
    pub(super) fn execute_vec_compare_to_q(
        &self,
        ctx: &mut SmirContext,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        cond: VecCmpCond,
        elem: VecElementType,
        lanes: u8,
        accumulate: Option<VLaneOp>,
    ) {
        let a = Self::read_vec(ctx, src1);
        let b = Self::read_vec(ctx, src2);
        let nbits = elem.bytes() * 8;
        let ebytes = elem.bytes() as usize;
        let sext = |value: u64| -> i64 {
            let shift = 64 - nbits;
            ((value << shift) as i64) >> shift
        };
        let mut q = [0_u64; 16];
        for lane in 0..lanes {
            let av = Self::get_lane(&a, lane, nbits);
            let bv = Self::get_lane(&b, lane, nbits);
            let matched = match cond {
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
                VecCmpCond::False => false,
                VecCmpCond::True => true,
            };
            if matched {
                for byte in 0..ebytes {
                    let bit = lane as usize * ebytes + byte;
                    q[bit >> 6] |= 1_u64 << (bit & 63);
                }
            }
        }
        // Accumulating compares combine the new mask into the existing Q.
        if let Some(combine) = accumulate {
            let previous = Self::read_vec(ctx, dst);
            for word in 0..2 {
                q[word] = match combine {
                    VLaneOp::And => previous[word] & q[word],
                    VLaneOp::Or => previous[word] | q[word],
                    VLaneOp::Xor => previous[word] ^ q[word],
                    _ => q[word],
                };
            }
        }
        Self::write_vec(ctx, dst, q);
    }

    pub(super) fn execute_vec_compare(
        &self,
        ctx: &mut SmirContext,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        cond: VecCmpCond,
        elem: VecElementType,
        lanes: u8,
        x86_hint: Option<X86OpHint>,
    ) {
        let old = Self::legacy_xmm_snapshot(ctx, dst, x86_hint);
        let a = Self::read_vec(ctx, src1);
        let b = Self::read_vec(ctx, src2);
        let bits = elem.bytes() * 8;
        let signed = |value: u64| -> i64 {
            if bits == 64 {
                value as i64
            } else {
                let shift = 64 - bits;
                ((value << shift) as i64) >> shift
            }
        };
        let integer_compare = |av: u64, bv: u64| match cond {
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
            VecCmpCond::False => false,
            VecCmpCond::True => true,
        };
        let float_compare = |av: f64, bv: f64| match cond {
            VecCmpCond::Eq => av == bv,
            VecCmpCond::Ne => av != bv,
            VecCmpCond::Lt | VecCmpCond::Ltu => av < bv,
            VecCmpCond::Le | VecCmpCond::Leu => av <= bv,
            VecCmpCond::Gt | VecCmpCond::Gtu => av > bv,
            VecCmpCond::Ge | VecCmpCond::Geu => av >= bv,
            VecCmpCond::False => false,
            VecCmpCond::True => true,
        };
        let f16_to_f64 = |raw: u16| -> f64 {
            let sign = (u32::from(raw & 0x8000)) << 16;
            let exp = (raw >> 10) & 0x1f;
            let fraction = raw & 0x03ff;
            let bits32 = if exp == 0 {
                if fraction == 0 {
                    sign
                } else {
                    let shift = fraction.leading_zeros() - 6;
                    let normalized = (u32::from(fraction) << (shift + 1)) & 0x03ff;
                    sign | ((112 - shift) << 23) | (normalized << 13)
                }
            } else if exp == 0x1f {
                sign | 0x7f80_0000 | (u32::from(fraction) << 13)
            } else {
                sign | ((u32::from(exp) + 112) << 23) | (u32::from(fraction) << 13)
            };
            f64::from(f32::from_bits(bits32))
        };

        let mut result = [0_u64; 16];
        let true_value = if bits == 64 {
            u64::MAX
        } else {
            (1_u64 << bits) - 1
        };
        for lane in 0..lanes {
            let av = Self::get_lane(&a, lane, bits);
            let bv = Self::get_lane(&b, lane, bits);
            let matched = match elem {
                VecElementType::I8
                | VecElementType::I16
                | VecElementType::I32
                | VecElementType::I64 => integer_compare(av, bv),
                VecElementType::F16 => float_compare(f16_to_f64(av as u16), f16_to_f64(bv as u16)),
                VecElementType::F32 => float_compare(
                    f64::from(f32::from_bits(av as u32)),
                    f64::from(f32::from_bits(bv as u32)),
                ),
                VecElementType::F64 => float_compare(f64::from_bits(av), f64::from_bits(bv)),
            };
            if matched {
                Self::set_lane(&mut result, lane, bits, true_value);
            }
        }
        Self::write_vec(ctx, dst, result);
        Self::restore_legacy_xmm_upper(ctx, dst, old);
    }
}
