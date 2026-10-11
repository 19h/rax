//! Width-bounded integer lane kernels and saturation detection.

use crate::smir::interpret::SmirInterpreter;
use crate::smir::ir::types::VLaneOp;

impl SmirInterpreter {
    /// Returns true iff the saturating lane op `op` clamps `(a, b)` out of the
    /// target `elem_bits` range — i.e. the Hexagon `ctx.sat_n`/`ctx.satu_n`
    /// overflow condition. Only `AddSat`/`SubSat` saturate; all other ops never
    /// clamp (return false). Mirrors `apply_lane_op`'s i128 arithmetic exactly so
    /// the clamp detection matches the value path bit-for-bit.
    pub(crate) fn lane_sat_clamped(
        op: VLaneOp,
        a: u64,
        b: u64,
        elem_bits: u32,
        signed: bool,
    ) -> bool {
        let mask: u64 = if elem_bits >= 64 {
            u64::MAX
        } else {
            (1u64 << elem_bits) - 1
        };
        let sx = |v: u64| -> i64 {
            if elem_bits >= 64 {
                v as i64
            } else {
                let shift = 64 - elem_bits;
                ((v << shift) as i64) >> shift
            }
        };
        let smin: i128 = if signed {
            -(1i128 << (elem_bits - 1))
        } else {
            0
        };
        let smax: i128 = if signed {
            (1i128 << (elem_bits - 1)) - 1
        } else {
            mask as i128
        };
        match op {
            VLaneOp::AddSat => {
                if signed {
                    let s = sx(a) as i128 + sx(b) as i128;
                    s < smin || s > smax
                } else {
                    let s = (a & mask) as u128 + (b & mask) as u128;
                    s > mask as u128
                }
            }
            VLaneOp::SubSat => {
                if signed {
                    let s = sx(a) as i128 - sx(b) as i128;
                    s < smin || s > smax
                } else {
                    // Unsigned sub clamps to 0 (matches `ctx.satu_n` on negatives;
                    // an unsigned diff can never exceed `mask`).
                    (a & mask) < (b & mask)
                }
            }
            _ => false,
        }
    }

    /// Apply a width-bounded integer operation to two lane values.
    pub(crate) fn apply_lane_op(op: VLaneOp, a: u64, b: u64, elem_bits: u32, signed: bool) -> u64 {
        let mask: u64 = if elem_bits >= 64 {
            u64::MAX
        } else {
            (1u64 << elem_bits) - 1
        };
        // Sign-extend a zero-extended `elem_bits` value to i64.
        let sx = |v: u64| -> i64 {
            if elem_bits >= 64 {
                v as i64
            } else {
                let shift = 64 - elem_bits;
                ((v << shift) as i64) >> shift
            }
        };
        let smin: i64 = if elem_bits >= 64 {
            i64::MIN
        } else {
            -(1i64 << (elem_bits - 1))
        };
        let smax: i64 = if elem_bits >= 64 {
            i64::MAX
        } else {
            (1i64 << (elem_bits - 1)) - 1
        };
        let umax: u64 = mask;
        let res: u64 = match op {
            VLaneOp::Add => a.wrapping_add(b),
            VLaneOp::Sub => a.wrapping_sub(b),
            VLaneOp::Mul => a.wrapping_mul(b),
            VLaneOp::And => a & b,
            VLaneOp::Or => a | b,
            VLaneOp::Xor => a ^ b,
            VLaneOp::AndNot => a & !b,
            VLaneOp::OrNot => a | !b,
            VLaneOp::Not => !a,
            VLaneOp::Min => {
                if signed {
                    sx(a).min(sx(b)) as u64
                } else {
                    (a & mask).min(b & mask)
                }
            }
            VLaneOp::Max => {
                if signed {
                    sx(a).max(sx(b)) as u64
                } else {
                    (a & mask).max(b & mask)
                }
            }
            VLaneOp::AddSat => {
                if signed {
                    (sx(a) as i128 + sx(b) as i128).clamp(smin as i128, smax as i128) as u64
                } else {
                    ((a & mask) as u128 + (b & mask) as u128).min(umax as u128) as u64
                }
            }
            VLaneOp::SubSat => {
                if signed {
                    (sx(a) as i128 - sx(b) as i128).clamp(smin as i128, smax as i128) as u64
                } else {
                    (a & mask).saturating_sub(b & mask)
                }
            }
            VLaneOp::Avg => {
                if signed {
                    ((sx(a) as i128 + sx(b) as i128) >> 1) as u64
                } else {
                    (((a & mask) as u128 + (b & mask) as u128) >> 1) as u64
                }
            }
            VLaneOp::AvgRnd => {
                if signed {
                    ((sx(a) as i128 + sx(b) as i128 + 1) >> 1) as u64
                } else {
                    (((a & mask) as u128 + (b & mask) as u128 + 1) >> 1) as u64
                }
            }
            VLaneOp::Sign => {
                if b & mask == 0 {
                    0
                } else if sx(b) < 0 {
                    0u64.wrapping_sub(a)
                } else {
                    a
                }
            }
            VLaneOp::AbsDiff => {
                if signed {
                    (sx(a) as i128 - sx(b) as i128).unsigned_abs() as u64
                } else {
                    (a & mask).abs_diff(b & mask)
                }
            }
            VLaneOp::ShiftSigned8 | VLaneOp::ShiftSigned8Rnd => {
                let value = a & mask;
                let rounding = op == VLaneOp::ShiftSigned8Rnd;
                Self::shift_lane_signed_byte(value, b, elem_bits, signed, rounding)
            }
        };
        res & mask
    }

    /// AArch64 SSHL/SRSHL lane semantics. The count uses only bits [7:0];
    /// the rounding addition needs one extra bit even for a 64-bit lane.
    fn shift_lane_signed_byte(
        value: u64,
        count: u64,
        elem_bits: u32,
        signed: bool,
        rounding: bool,
    ) -> u64 {
        let count = i32::from(count as u8 as i8);
        if count >= 0 {
            let amount = count as u32;
            return if amount >= elem_bits {
                0
            } else {
                value.wrapping_shl(amount)
            };
        }

        let amount = (-count) as u32;
        let extended = if signed {
            let extension = 64 - elem_bits;
            (((value << extension) as i64) >> extension) as i128
        } else {
            i128::from(value)
        };
        if amount > elem_bits {
            return if signed && !rounding && extended < 0 {
                u64::MAX
            } else {
                0
            };
        }

        let mut rounded = extended;
        if rounding {
            rounded += 1i128 << (amount - 1);
        }
        (rounded >> amount) as u64
    }
}
