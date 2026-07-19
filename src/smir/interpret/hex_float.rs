//! Hexagon software floating-point helper routines (hf_*/hr_*/hex_*)

use crate::smir::interpret::*;
use std::cmp::Ordering;
use std::collections::HashMap;

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

// ============================================================================
// Hexagon scalar floating-point evaluation (OpKind::HexFp)
// ============================================================================
//
// Bit-exact port of the `qemu-hexagon` reference semantics in
// `src/isa/hexagon/semantics/float.rs` and `float_ext.rs`. Only the
// RESULT bit pattern is produced here (the SMIR-lift harness compares the
// result register/predicate and USR:OVF (bit0); none of the F2 ops set OVF, and
// the FP exception sticky bits — USR bits 1..5 — are NOT compared, so they are
// intentionally not modeled). NaN results are canonicalised to Hexagon's
// default all-ones NaN, matching QEMU's `default_nan_mode`.

#[inline]
pub(crate) fn hf32_is_nan(b: u32) -> bool {
    (b & 0x7f80_0000) == 0x7f80_0000 && (b & 0x007f_ffff) != 0
}
#[inline]
pub(crate) fn hf64_is_nan(b: u64) -> bool {
    (b & 0x7ff0_0000_0000_0000) == 0x7ff0_0000_0000_0000 && (b & 0x000f_ffff_ffff_ffff) != 0
}

/// Hexagon `fpclassify` category on raw f32 bits.
#[inline]
pub(crate) fn hf32_class_bit(b: u32) -> u32 {
    let exp = (b >> 23) & 0xff;
    let mant = b & 0x007f_ffff;
    if exp == 0 {
        if mant == 0 {
            0 // Zero
        } else {
            2 // Subnormal
        }
    } else if exp == 0xff {
        if mant == 0 {
            3 // Infinite
        } else {
            4 // Nan
        }
    } else {
        1 // Normal
    }
}
#[inline]
pub(crate) fn hf64_class_bit(b: u64) -> u32 {
    let exp = (b >> 52) & 0x7ff;
    let mant = b & 0x000f_ffff_ffff_ffff;
    if exp == 0 {
        if mant == 0 { 0 } else { 2 }
    } else if exp == 0x7ff {
        if mant == 0 { 3 } else { 4 }
    } else {
        1
    }
}

/// Relation result of an ordered IEEE compare (Unordered if either is NaN).
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum HfRel {
    Less,
    Equal,
    Greater,
    Unordered,
}

pub(crate) fn hf_cmp_sf(a: u32, b: u32) -> HfRel {
    if hf32_is_nan(a) || hf32_is_nan(b) {
        return HfRel::Unordered;
    }
    let (fa, fb) = (f32::from_bits(a), f32::from_bits(b));
    if fa < fb {
        HfRel::Less
    } else if fa > fb {
        HfRel::Greater
    } else {
        HfRel::Equal
    }
}
pub(crate) fn hf_cmp_df(a: u64, b: u64) -> HfRel {
    if hf64_is_nan(a) || hf64_is_nan(b) {
        return HfRel::Unordered;
    }
    let (fa, fb) = (f64::from_bits(a), f64::from_bits(b));
    if fa < fb {
        HfRel::Less
    } else if fa > fb {
        HfRel::Greater
    } else {
        HfRel::Equal
    }
}

/// IEEE-754-2019 minimumNumber / maximumNumber on raw f32 bits.
pub(crate) fn hf_sf_minmax(a: u32, b: u32, is_min: bool) -> u32 {
    let an = hf32_is_nan(a);
    let bn = hf32_is_nan(b);
    if an || bn {
        if !(an && bn) {
            return if an { b } else { a };
        }
        return 0xFFFF_FFFF; // both NaN -> default NaN
    }
    let (fa, fb) = (f32::from_bits(a), f32::from_bits(b));
    let pick_a = if fa == fb {
        let sa = (a >> 31) & 1;
        let sb = (b >> 31) & 1;
        if sa == sb {
            true
        } else if is_min {
            sa == 1
        } else {
            sa == 0
        }
    } else if is_min {
        fa < fb
    } else {
        fa > fb
    };
    if pick_a { a } else { b }
}
pub(crate) fn hf_df_minmax(a: u64, b: u64, is_min: bool) -> u64 {
    let an = hf64_is_nan(a);
    let bn = hf64_is_nan(b);
    if an || bn {
        if !(an && bn) {
            return if an { b } else { a };
        }
        return 0xFFFF_FFFF_FFFF_FFFF;
    }
    let (fa, fb) = (f64::from_bits(a), f64::from_bits(b));
    let pick_a = if fa == fb {
        let sa = (a >> 63) & 1;
        let sb = (b >> 63) & 1;
        if sa == sb {
            true
        } else if is_min {
            sa == 1
        } else {
            sa == 0
        }
    } else if is_min {
        fa < fb
    } else {
        fa > fb
    };
    if pick_a { a } else { b }
}

#[inline]
pub(crate) fn hf_round_f32(f: f32, chop: bool) -> f32 {
    if chop { f.trunc() } else { f.round_ties_even() }
}
#[inline]
pub(crate) fn hf_round_f64(f: f64, chop: bool) -> f64 {
    if chop { f.trunc() } else { f.round_ties_even() }
}

/// `float_to_sint` clamp (mirrors sem/float.rs).
pub(crate) fn hf_to_sint(ri: f64, min: i128, max: i128) -> i128 {
    let v = ri as i128;
    if v < min || v > max || !ri.is_finite() {
        if ri.is_sign_negative() { min } else { max }
    } else {
        v
    }
}
pub(crate) fn hf_to_uint(ri: f64, max: u128) -> u128 {
    if !ri.is_finite() {
        return max;
    }
    let v = ri as i128;
    if v < 0 || (v as u128) > max {
        max
    } else {
        v as u128
    }
}

pub(crate) fn hf_sf_to_sint(b: u32, chop: bool, min: i128, max: i128) -> i128 {
    if hf32_is_nan(b) {
        return -1;
    }
    let f = f32::from_bits(b);
    let ri = hf_round_f32(f, chop);
    hf_to_sint(ri as f64, min, max)
}
pub(crate) fn hf_sf_to_uint(b: u32, chop: bool, max: u128) -> u128 {
    if hf32_is_nan(b) {
        return max;
    }
    let f = f32::from_bits(b);
    if (b & 0x8000_0000) != 0 && f != 0.0 {
        return 0;
    }
    let ri = hf_round_f32(f, chop);
    hf_to_uint(ri as f64, max)
}
pub(crate) fn hf_df_to_sint(b: u64, chop: bool, min: i128, max: i128) -> i128 {
    if hf64_is_nan(b) {
        return -1;
    }
    let f = f64::from_bits(b);
    let ri = hf_round_f64(f, chop);
    hf_to_sint(ri, min, max)
}
pub(crate) fn hf_df_to_uint(b: u64, chop: bool, max: u128) -> u128 {
    if hf64_is_nan(b) {
        return max;
    }
    let f = f64::from_bits(b);
    if (b & 0x8000_0000_0000_0000) != 0 && f != 0.0 {
        return 0;
    }
    let ri = hf_round_f64(f, chop);
    hf_to_uint(ri, max)
}

/// `df -> sf` narrowing (sem `df_to_sf`); only the result bits.
pub(crate) fn hf_df_to_sf(b: u64) -> u32 {
    if hf64_is_nan(b) {
        return 0xFFFF_FFFF;
    }
    (f64::from_bits(b) as f32).to_bits()
}

/// Hexagon single-precision fused multiply-add `c {+,-} a*b` with a single IEEE
/// rounding (native `f32::mul_add`) and default-NaN canonicalisation. Mirrors
/// the F2_sffma / F2_sffms reference (`sem/float_ext.rs::sf_fma`) for the result
/// bits: `mul_add` is correctly-rounded (one rounding), so the finite result
/// matches; any NaN result is canonicalised to all-ones (Hexagon default NaN),
/// which also covers the invalid cases (sNaN input, 0*inf, inf-inf).
pub(crate) fn hex_sf_fma(araw: u32, braw: u32, craw: u32, negate_product: bool) -> u32 {
    // sffms computes Rx - Rs*Rt = (-Rs)*Rt + Rx.
    let fa = f32::from_bits(if negate_product {
        araw ^ 0x8000_0000
    } else {
        araw
    });
    let fb = f32::from_bits(braw);
    let fc = f32::from_bits(craw);
    let r = fa.mul_add(fb, fc);
    if r.is_nan() { 0xFFFF_FFFF } else { r.to_bits() }
}

// ============================================================================
// Reciprocal / inverse-sqrt seed + fixup (OpKind::HexFpRecip)
// ============================================================================
//
// Byte-for-byte port of QEMU `target/hexagon/arch.c`:
//   * `arch_sf_recip_common(Rs,Rt,Rd,adjust)` and
//   * `arch_sf_invsqrt_common(Rs,Rd,adjust)`
// plus the idef seed-table lookup, copied verbatim from the rax reference sem
// (`src/isa/hexagon/semantics/float_ext.rs`). The 128-entry seed tables
// are reproduced EXACTLY (they were recovered byte-for-byte from the qemu
// oracle). The Pe `adjust` value lands in the FULL predicate byte (the harness
// compares the whole byte). USR FP sticky flags are NOT modeled here (the
// harness compares only the result + USR:OVF, and these ops never set OVF), so
// the flag-setting side of `float32_scalbn`/`round_exact` is dropped — it does
// not affect the returned bit pattern.

pub(crate) const HR_RECIP_LOOKUP: [u8; 128] = [
    0xfe, 0xfa, 0xf6, 0xf2, 0xef, 0xeb, 0xe7, 0xe4, 0xe0, 0xdd, 0xd9, 0xd6, 0xd2, 0xcf, 0xcc, 0xc9,
    0xc6, 0xc2, 0xbf, 0xbc, 0xb9, 0xb6, 0xb3, 0xb1, 0xae, 0xab, 0xa8, 0xa5, 0xa3, 0xa0, 0x9d, 0x9b,
    0x98, 0x96, 0x93, 0x91, 0x8e, 0x8c, 0x8a, 0x87, 0x85, 0x83, 0x80, 0x7e, 0x7c, 0x7a, 0x78, 0x75,
    0x73, 0x71, 0x6f, 0x6d, 0x6b, 0x69, 0x67, 0x65, 0x63, 0x61, 0x5f, 0x5e, 0x5c, 0x5a, 0x58, 0x56,
    0x54, 0x53, 0x51, 0x4f, 0x4e, 0x4c, 0x4a, 0x49, 0x47, 0x45, 0x44, 0x42, 0x40, 0x3f, 0x3d, 0x3c,
    0x3a, 0x39, 0x37, 0x36, 0x34, 0x33, 0x32, 0x30, 0x2f, 0x2d, 0x2c, 0x2b, 0x29, 0x28, 0x27, 0x25,
    0x24, 0x23, 0x21, 0x20, 0x1f, 0x1e, 0x1c, 0x1b, 0x1a, 0x19, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12,
    0x11, 0x0f, 0x0e, 0x0d, 0x0c, 0x0b, 0x0a, 0x09, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x00,
];

pub(crate) const HR_INVSQRT_LOOKUP: [u8; 128] = [
    0x69, 0x66, 0x63, 0x61, 0x5e, 0x5b, 0x59, 0x57, 0x54, 0x52, 0x50, 0x4d, 0x4b, 0x49, 0x47, 0x45,
    0x43, 0x41, 0x3f, 0x3d, 0x3b, 0x39, 0x37, 0x36, 0x34, 0x32, 0x30, 0x2f, 0x2d, 0x2c, 0x2a, 0x28,
    0x27, 0x25, 0x24, 0x22, 0x21, 0x1f, 0x1e, 0x1d, 0x1b, 0x1a, 0x19, 0x17, 0x16, 0x15, 0x14, 0x12,
    0x11, 0x10, 0x0f, 0x0d, 0x0c, 0x0b, 0x0a, 0x09, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
    0xfe, 0xfa, 0xf6, 0xf3, 0xef, 0xeb, 0xe8, 0xe4, 0xe1, 0xde, 0xdb, 0xd7, 0xd4, 0xd1, 0xce, 0xcb,
    0xc9, 0xc6, 0xc3, 0xc0, 0xbe, 0xbb, 0xb8, 0xb6, 0xb3, 0xb1, 0xaf, 0xac, 0xaa, 0xa8, 0xa5, 0xa3,
    0xa1, 0x9f, 0x9d, 0x9b, 0x99, 0x97, 0x95, 0x93, 0x91, 0x8f, 0x8d, 0x8b, 0x89, 0x87, 0x86, 0x84,
    0x82, 0x80, 0x7f, 0x7d, 0x7b, 0x7a, 0x78, 0x77, 0x75, 0x74, 0x72, 0x71, 0x6f, 0x6e, 0x6c, 0x6b,
];

pub(crate) const HR_SF_BIAS: i32 = 127;
pub(crate) const HR_SF_MANTBITS: i32 = 23;
pub(crate) const HR_SF_MAXEXP: i32 = 254;
pub(crate) const HR_F32_ONE: u32 = 0x3f80_0000;
pub(crate) const HR_F32_NAN: u32 = 0xffff_ffff; // Hexagon default NaN (all ones)

#[inline]
pub(crate) fn hr_is_inf(b: u32) -> bool {
    (b & 0x7fff_ffff) == 0x7f80_0000
}
#[inline]
pub(crate) fn hr_is_zero(b: u32) -> bool {
    (b & 0x7fff_ffff) == 0
}
#[inline]
pub(crate) fn hr_is_neg(b: u32) -> bool {
    (b >> 31) & 1 == 1
}
#[inline]
pub(crate) fn hr_is_normal(b: u32) -> bool {
    let e = (b >> 23) & 0xff;
    e != 0 && e != 0xff
}
#[inline]
pub(crate) fn hr_is_denormal(b: u32) -> bool {
    (b >> 23) & 0xff == 0 && (b & 0x007f_ffff) != 0
}
#[inline]
pub(crate) fn hr_getexp_raw(b: u32) -> i32 {
    ((b >> 23) & 0xff) as i32
}
/// QEMU `float32_getexp`: raw exp for normals; raw+1 for denormals; -1 else.
#[inline]
pub(crate) fn hr_getexp(b: u32) -> i32 {
    let raw = hr_getexp_raw(b);
    if hr_is_normal(b) {
        raw
    } else if hr_is_denormal(b) {
        raw + 1
    } else {
        -1
    }
}
#[inline]
pub(crate) fn hr_infinite(neg: bool) -> u32 {
    if neg { 0xff80_0000 } else { 0x7f80_0000 }
}

/// Exact (sign, m, e) decomposition of a finite f32 (caller excludes NaN/inf).
#[derive(Clone, Copy)]
pub(crate) struct HrSf {
    neg: bool,
    m: u128,
    e: i32,
}
pub(crate) fn hr_decode(b: u32) -> HrSf {
    let neg = (b >> 31) & 1 == 1;
    let exp = ((b >> 23) & 0xff) as i32;
    let frac = (b & 0x007f_ffff) as u128;
    if exp == 0 {
        HrSf {
            neg,
            m: frac,
            e: -149,
        }
    } else {
        HrSf {
            neg,
            m: frac | 0x0080_0000,
            e: exp - 150,
        }
    }
}

/// Round an exact magnitude `m * 2^e` to nearest f32 (no flag side-effects). The
/// default tie-break is to even; `ties_away` (the `:lib` fma forms) rounds an
/// exact half AWAY from zero on a tiny/subnormal result instead — this is the
/// behaviour that native `f32::mul_add` (always ties-to-even) cannot reproduce.
/// Port of `round_exact_to_f32` (result bits only).
pub(crate) fn hr_round_exact_to_f32(
    neg: bool,
    mut m: u128,
    mut e: i32,
    sticky: bool,
    ties_away: bool,
) -> u32 {
    let sign = if neg { 0x8000_0000u32 } else { 0 };
    if m == 0 {
        return sign;
    }
    let msb = 127 - m.leading_zeros() as i32;
    let mut unbiased = msb + e;
    let tiny = unbiased < -126;
    let lowest_exp = if tiny { -149 } else { unbiased - 23 };
    let drop = lowest_exp - e;
    if drop > 0 {
        let drop = drop as u32;
        let dropped_mask = if drop >= 128 {
            u128::MAX
        } else {
            (1u128 << drop) - 1
        };
        let dropped = m & dropped_mask;
        let half = if (1..=128).contains(&drop) {
            1u128 << (drop - 1)
        } else {
            0
        };
        m = if drop >= 128 { 0 } else { m >> drop };
        e += drop as i32;
        let round_bit = dropped & half != 0;
        let rest = (dropped & half.wrapping_sub(1)) != 0 || sticky;
        if round_bit && ((ties_away && tiny) || rest || (m & 1) == 1) {
            m += 1;
        }
    }
    if m == 0 {
        return sign;
    }
    let new_msb = 127 - m.leading_zeros() as i32;
    unbiased = new_msb + e;
    if unbiased > 127 {
        return sign | 0x7f80_0000;
    }
    if unbiased < -126 {
        let frac = if e == -149 {
            m
        } else if e > -149 {
            m << (e + 149)
        } else {
            m >> (-149 - e)
        };
        return sign | (frac as u32 & 0x007f_ffff);
    }
    let extra = new_msb - 23;
    let frac = if extra >= 0 {
        (m >> extra) & 0x007f_ffff
    } else {
        (m << (-extra)) & 0x007f_ffff
    };
    let biased = (unbiased + 127) as u32;
    sign | (biased << 23) | (frac as u32)
}

/// softfloat `float32_scalbn(f, n)` for finite `f` (the only kind reached on the
/// recip/invsqrt normal path). Port of the sem's `f32_scalbn`.
pub(crate) fn hr_scalbn(b: u32, n: i32) -> u32 {
    if hr_is_zero(b) {
        return b;
    }
    let neg = hr_is_neg(b);
    let dec = hr_decode(b); // exact (sign, m, e); m != 0 here
    hr_round_exact_to_f32(neg, dec.m, dec.e + n, false, false)
}

/// Guard width for the exact f32 fma core (48-bit product mantissa + 78 = 126
/// bits, fits i128). Port of the sem's `SF_GUARD`.
pub(crate) const HR_SF_GUARD: i32 = 78;

/// Exactly add two finite scaled magnitudes. Port of the sem's `add_scaled`
/// (result-shaping only; no flag side-effects). Returns `(neg, mag, e, sticky)`
/// where `mag*2^e` is the magnitude truncated toward zero and `sticky` is true
/// iff the true magnitude is strictly larger than `mag*2^e`.
pub(crate) fn hr_add_scaled(
    neg_a: bool,
    ma: u128,
    ea: i32,
    neg_b: bool,
    mb: u128,
    eb: i32,
    guard: i32,
) -> (bool, u128, i32, bool) {
    if ma == 0 {
        return (neg_b, mb, eb, false);
    }
    if mb == 0 {
        return (neg_a, ma, ea, false);
    }
    let ehi = ea.max(eb);
    let ce = ehi - guard;
    let split = |m: u128, e: i32| -> (i128, bool) {
        let shift = e - ce;
        if shift >= 0 {
            ((m << shift) as i128, false)
        } else {
            let s = (-shift) as u32;
            if s >= 128 {
                (0, m != 0)
            } else {
                let kept = (m >> s) as i128;
                let residual = (m & ((1u128 << s) - 1)) != 0;
                (kept, residual)
            }
        }
    };
    let (ka, ra) = split(ma, ea);
    let (kb, rb) = split(mb, eb);
    let sa = if neg_a { -ka } else { ka };
    let sb = if neg_b { -kb } else { kb };
    let res_a = if ra { if neg_a { -1i32 } else { 1 } } else { 0 };
    let res_b = if rb { if neg_b { -1i32 } else { 1 } } else { 0 };
    let res_sign = res_a + res_b;
    let mut sum = sa + sb;
    if sum == 0 {
        if res_sign == 0 {
            return (false, 0, ce, false);
        }
        let neg = res_sign < 0;
        return (neg, 0, ce, true);
    }
    let neg = sum < 0;
    if neg {
        sum = -sum;
    }
    let mag = sum as u128;
    let sticky;
    let final_mag;
    if res_sign == 0 {
        sticky = false;
        final_mag = mag;
    } else {
        let res_neg = res_sign < 0;
        if res_neg == neg {
            sticky = true;
            final_mag = mag;
        } else {
            sticky = true;
            final_mag = mag - 1;
        }
    }
    (neg, final_mag, ce, sticky)
}

/// Exact fused multiply-add `a*b + c` with a single rounding (flag-free port of
/// the sem's `sf_fma`). `ties_away` selects the `:lib` ties-away rounding of a
/// subnormal result; the recip path never uses it.
pub(crate) fn hr_sf_fma(
    araw: u32,
    braw: u32,
    craw: u32,
    negate_prod: bool,
    ties_away: bool,
) -> u32 {
    let a = if negate_prod {
        araw ^ 0x8000_0000
    } else {
        araw
    };
    let b = braw;
    let c = craw;
    let any_nan = hf32_is_nan(araw) || hf32_is_nan(braw) || hf32_is_nan(craw);
    let a_inf = (a & 0x7fff_ffff) == 0x7f80_0000;
    let b_inf = (b & 0x7fff_ffff) == 0x7f80_0000;
    let c_inf = (c & 0x7fff_ffff) == 0x7f80_0000;
    let a_zero = (a & 0x7fff_ffff) == 0;
    let b_zero = (b & 0x7fff_ffff) == 0;
    let prod_invalid = (a_inf && b_zero) || (b_inf && a_zero);
    if any_nan || prod_invalid {
        return 0xFFFF_FFFF;
    }
    if a_inf || b_inf {
        let prod_neg = ((a >> 31) ^ (b >> 31)) & 1 == 1;
        if c_inf {
            let c_neg = (c >> 31) & 1 == 1;
            if prod_neg != c_neg {
                return 0xFFFF_FFFF; // inf - inf
            }
            return if prod_neg { 0xff80_0000 } else { 0x7f80_0000 };
        }
        return if prod_neg { 0xff80_0000 } else { 0x7f80_0000 };
    }
    if c_inf {
        return c;
    }
    let da = hr_decode(a);
    let db = hr_decode(b);
    let dc = hr_decode(c);
    let prod_neg = da.neg ^ db.neg;
    let prod_m = da.m * db.m; // up to 48 bits
    let prod_e = da.e + db.e;
    if prod_m == 0 {
        if dc.m == 0 {
            let neg = prod_neg && dc.neg;
            return if neg { 0x8000_0000 } else { 0 };
        }
        return hr_round_exact_to_f32(dc.neg, dc.m, dc.e, false, ties_away);
    }
    if dc.m == 0 {
        return hr_round_exact_to_f32(prod_neg, prod_m, prod_e, false, ties_away);
    }
    let (neg, mag, e, sticky) =
        hr_add_scaled(prod_neg, prod_m, prod_e, dc.neg, dc.m, dc.e, HR_SF_GUARD);
    if mag == 0 && !sticky {
        return 0;
    }
    hr_round_exact_to_f32(neg, mag, e, sticky, ties_away)
}

#[inline]
pub(crate) fn hr_sf_true_zero_product(rs: u32, rt: u32) -> bool {
    let (frs, frt) = (f32::from_bits(rs), f32::from_bits(rt));
    (frs == 0.0 && frt.is_finite()) || (frt == 0.0 && frs.is_finite())
}

/// `Rx {+,-}= sfmpy(Rs,Rt):lib`. Byte-for-byte port of the sem's `sf_fma_lib`:
/// the exact single-rounding fma (with ties-away subnormal rounding), then the
/// `:lib` post-fixups — preserve a true-zero accumulator's sign, back a
/// spurious-overflow infinity (no infinite input) off to max-finite (bit
/// decrement), and flush inf-minus-inf to +0. Flags are not modeled (the harness
/// compares only the result + USR:OVF, which `:lib` never sets).
pub(crate) fn hex_sf_fma_lib(rs: u32, rt: u32, rx: u32, sub: bool) -> u32 {
    let tmp = hr_sf_fma(rs, rt, rx, sub, true);
    if hf32_is_nan(rs) || hf32_is_nan(rt) || hf32_is_nan(rx) {
        return tmp;
    }
    let frx = f32::from_bits(rx);
    let prod = f32::from_bits(rs) * f32::from_bits(rt); // inf-ness only
    let infinp =
        frx.is_infinite() || f32::from_bits(rt).is_infinite() || f32::from_bits(rs).is_infinite();
    let xor_sign = ((rs >> 31) ^ (rx >> 31) ^ (rt >> 31)) & 1;
    let inf_minus_inf = frx.is_infinite()
        && prod.is_infinite()
        && (if sub { xor_sign == 0 } else { xor_sign != 0 });
    let mut res = if frx == 0.0 && hr_sf_true_zero_product(rs, rt) {
        rx
    } else {
        tmp
    };
    if f32::from_bits(res).is_infinite() && !infinp {
        res = res.wrapping_sub(1);
    }
    if inf_minus_inf {
        res = 0; // +0.0
    }
    res
}

// ============================================================================
// Scaled single-precision fused multiply-add (F2_sffma_sc): `Rx += Rs*Rt` then
// `* 2^Pu`. Pu is a two's-complement signed-8 scale folded into the EXACT
// product BEFORE the single rounding (a hardware scalb), so it routes through
// the exact integer fma core with the scale threaded into the result exponent —
// native `f32::mul_add` followed by a separate scale would double-round.
// Byte-for-byte port of the sem's `sf_fma_scale` (result bits only).
// ============================================================================

/// Exact fused multiply-add `a*b + c`, then `* 2^scale` applied to the exact
/// magnitude before the single rounding. Mirror of `hr_sf_fma` with the sem's
/// `scale` arg threaded into every `hr_round_exact_to_f32` call site.
pub(crate) fn hr_sf_fma_scaled(araw: u32, braw: u32, craw: u32, scale: i32) -> u32 {
    let a = araw;
    let b = braw;
    let c = craw;
    let any_nan = hf32_is_nan(araw) || hf32_is_nan(braw) || hf32_is_nan(craw);
    let a_inf = (a & 0x7fff_ffff) == 0x7f80_0000;
    let b_inf = (b & 0x7fff_ffff) == 0x7f80_0000;
    let c_inf = (c & 0x7fff_ffff) == 0x7f80_0000;
    let a_zero = (a & 0x7fff_ffff) == 0;
    let b_zero = (b & 0x7fff_ffff) == 0;
    let prod_invalid = (a_inf && b_zero) || (b_inf && a_zero);
    if any_nan || prod_invalid {
        return 0xFFFF_FFFF;
    }
    if a_inf || b_inf {
        let prod_neg = ((a >> 31) ^ (b >> 31)) & 1 == 1;
        if c_inf {
            let c_neg = (c >> 31) & 1 == 1;
            if prod_neg != c_neg {
                return 0xFFFF_FFFF; // inf - inf
            }
            return if prod_neg { 0xff80_0000 } else { 0x7f80_0000 };
        }
        return if prod_neg { 0xff80_0000 } else { 0x7f80_0000 };
    }
    if c_inf {
        return c;
    }
    let da = hr_decode(a);
    let db = hr_decode(b);
    let dc = hr_decode(c);
    let prod_neg = da.neg ^ db.neg;
    let prod_m = da.m * db.m;
    let prod_e = da.e + db.e;
    if prod_m == 0 {
        if dc.m == 0 {
            let neg = prod_neg && dc.neg;
            return if neg { 0x8000_0000 } else { 0 };
        }
        return hr_round_exact_to_f32(dc.neg, dc.m, dc.e + scale, false, false);
    }
    if dc.m == 0 {
        return hr_round_exact_to_f32(prod_neg, prod_m, prod_e + scale, false, false);
    }
    let (neg, mag, e, sticky) =
        hr_add_scaled(prod_neg, prod_m, prod_e, dc.neg, dc.m, dc.e, HR_SF_GUARD);
    if mag == 0 && !sticky {
        return 0;
    }
    hr_round_exact_to_f32(neg, mag, e + scale, sticky, false)
}

/// `Rx += sfmpy(Rs,Rt,Pu):scale`. Fused multiply-add then scale by `2^Pu`
/// (Pu read as a two's-complement signed-8 exponent). Byte-for-byte port of the
/// sem's `sf_fma_scale` (result bits only): a true-zero accumulator plus a
/// true-zero product keeps Rx (sign preserved) with no scaling.
pub(crate) fn hex_sf_fma_scale(rs: u32, rt: u32, rx: u32, pu: u8) -> u32 {
    if !hf32_is_nan(rs)
        && !hf32_is_nan(rt)
        && !hf32_is_nan(rx)
        && f32::from_bits(rx) == 0.0
        && hr_sf_true_zero_product(rs, rt)
    {
        return rx;
    }
    let scale = pu as i8 as i32;
    hr_sf_fma_scaled(rs, rt, rx, scale)
}

// ============================================================================
// Double-precision high-half multiply / fixup (F2_dfmpyhh / F2_dfmpyfix).
// ============================================================================
//
// Byte-for-byte port of the reference sem (sem/float_ext.rs::df_mpyhh /
// dfmpyfix). dfmpyhh needs an EXACT 64-bit-mantissa rounding core
// (`hr_round_exact_to_f64`, the f64 analog of `hr_round_exact_to_f32`) because
// native f64 double-rounds; dfmpyfix only ever scales by an exact power of two.
// Result bits only (the harness compares result + USR:OVF; these ops never set
// OVF, so the FP sticky flags are not modeled).

#[inline]
pub(crate) fn hr_df_getexp(b: u64) -> u64 {
    (b >> 52) & 0x7ff
}
#[inline]
pub(crate) fn hr_df_is_normal(b: u64) -> bool {
    let e = hr_df_getexp(b);
    e != 0 && e != 0x7ff
}
#[inline]
pub(crate) fn hr_df_is_denorm(b: u64) -> bool {
    hr_df_getexp(b) == 0 && (b & 0x000f_ffff_ffff_ffff) != 0
}
#[inline]
pub(crate) fn hr_df_is_big(b: u64) -> bool {
    hr_df_getexp(b) >= 512
}
#[inline]
pub(crate) fn hf64_is_snan(b: u64) -> bool {
    hf64_is_nan(b) && (b & 0x0008_0000_0000_0000) == 0
}

/// Exact (sign, m, e) decomposition of a finite f64 (caller excludes NaN/inf).
#[derive(Clone, Copy)]
pub(crate) struct HrDf {
    neg: bool,
    m: u128,
    e: i32,
}
pub(crate) fn hr_df_decode(b: u64) -> HrDf {
    let neg = (b >> 63) & 1 == 1;
    let exp = ((b >> 52) & 0x7ff) as i32;
    let frac = (b & 0x000f_ffff_ffff_ffff) as u128;
    if exp == 0 {
        HrDf {
            neg,
            m: frac,
            e: -1074,
        }
    } else {
        HrDf {
            neg,
            m: frac | 0x0010_0000_0000_0000,
            e: exp - 1075,
        }
    }
}

/// Round an exact magnitude `m * 2^e` to nearest-even f64 (no flag side-effects).
/// Direct f64 analog of `hr_round_exact_to_f32` (bias 1023, 52 mantissa bits,
/// smallest normal exponent -1022, subnormal floor 2^-1074). Port of the sem's
/// `round_exact_to_f64` (result bits only). dfmpyhh never uses `ties_away`.
pub(crate) fn hr_round_exact_to_f64(neg: bool, mut m: u128, mut e: i32, sticky: bool) -> u64 {
    let sign = if neg { 0x8000_0000_0000_0000u64 } else { 0 };
    if m == 0 {
        return sign;
    }
    let msb = 127 - m.leading_zeros() as i32;
    let mut unbiased = msb + e;
    let tiny = unbiased < -1022;
    let lowest_exp = if tiny { -1074 } else { unbiased - 52 };
    let drop = lowest_exp - e;
    if drop > 0 {
        let drop = drop as u32;
        let dropped_mask = if drop >= 128 {
            u128::MAX
        } else {
            (1u128 << drop) - 1
        };
        let dropped = m & dropped_mask;
        let half = if (1..=128).contains(&drop) {
            1u128 << (drop - 1)
        } else {
            0
        };
        m = if drop >= 128 { 0 } else { m >> drop };
        e += drop as i32;
        let round_bit = dropped & half != 0;
        let rest = (dropped & half.wrapping_sub(1)) != 0 || sticky;
        if round_bit && (rest || (m & 1) == 1) {
            m += 1;
        }
    }
    if m == 0 {
        return sign;
    }
    let new_msb = 127 - m.leading_zeros() as i32;
    unbiased = new_msb + e;
    if unbiased > 1023 {
        return sign | 0x7ff0_0000_0000_0000;
    }
    if unbiased < -1022 {
        let frac = if e == -1074 {
            m
        } else if e > -1074 {
            m << (e + 1074)
        } else {
            m >> (-1074 - e)
        };
        return sign | (frac as u64 & 0x000f_ffff_ffff_ffff);
    }
    let extra = new_msb - 52;
    let frac = if extra >= 0 {
        (m >> extra) & 0x000f_ffff_ffff_ffff
    } else {
        (m << (-extra)) & 0x000f_ffff_ffff_ffff
    };
    let biased = (unbiased + 1023) as u64;
    sign | (biased << 52) | (frac as u64)
}

/// `Rxx = dfmpyhh(Rss, Rtt, Rxx)`: high-half multiply + fixed-weight accumulate.
/// Byte-for-byte port of the sem's `df_mpyhh` (result bits only):
///   * each operand's mantissa is masked to its HIGH 32 bits before multiplying;
///   * subnormal inputs are flushed to signed zero;
///   * inf/NaN follow the usual product rules;
///   * the 64-bit accumulator is added at a FIXED weight `acc_e = prod_e + 31`,
///     then rounded once to nearest-even.
pub(crate) fn hr_df_mpyhh(araw: u64, braw: u64, acc: u64) -> u64 {
    if hf64_is_nan(araw) || hf64_is_nan(braw) {
        return 0xFFFF_FFFF_FFFF_FFFF;
    }
    let a_inf = (araw & 0x7fff_ffff_ffff_ffff) == 0x7ff0_0000_0000_0000;
    let b_inf = (braw & 0x7fff_ffff_ffff_ffff) == 0x7ff0_0000_0000_0000;
    let a_zero = (araw & 0x7fff_ffff_ffff_ffff) == 0;
    let b_zero = (braw & 0x7fff_ffff_ffff_ffff) == 0;
    if a_inf || b_inf {
        let neg = ((araw >> 63) ^ (braw >> 63)) & 1 == 1;
        if a_zero || b_zero {
            return 0xFFFF_FFFF_FFFF_FFFF; // inf * 0 -> invalid, default NaN
        }
        return if neg {
            0xfff0_0000_0000_0000
        } else {
            0x7ff0_0000_0000_0000
        };
    }
    let a_sub = (araw >> 52) & 0x7ff == 0 && (araw & 0x000f_ffff_ffff_ffff) != 0;
    let b_sub = (braw >> 52) & 0x7ff == 0 && (braw & 0x000f_ffff_ffff_ffff) != 0;
    let a = if a_sub {
        araw & 0x8000_0000_0000_0000
    } else {
        araw
    };
    let b = if b_sub {
        braw & 0x8000_0000_0000_0000
    } else {
        braw
    };
    let da = hr_df_decode(a & 0xffff_ffff_0000_0000);
    let db = hr_df_decode(b & 0xffff_ffff_0000_0000);
    let neg = da.neg ^ db.neg;
    if da.m == 0 || db.m == 0 {
        return if neg { 0x8000_0000_0000_0000 } else { 0 };
    }
    let prod_m = da.m * db.m;
    let prod_e = da.e + db.e;
    let acc_e = prod_e + 31;
    let lo = prod_e.min(acc_e);
    let total = (prod_m << (prod_e - lo)) + ((acc as u128) << (acc_e - lo));
    hr_round_exact_to_f64(neg, total, lo, false)
}

/// `Rdd = dfmpyfix(Rss, Rtt)`: conditional exact `2^±52` denormal fixup. Port of
/// the sem's `dfmpyfix` arm (the scale is always an exact power of two).
pub(crate) fn hr_df_mpyfix(ss: u64, tt: u64) -> u64 {
    if hr_df_is_denorm(ss) && hr_df_is_big(tt) && hr_df_is_normal(tt) {
        (f64::from_bits(ss) * (2.0f64).powi(52)).to_bits()
    } else if hr_df_is_denorm(tt) && hr_df_is_big(ss) && hr_df_is_normal(ss) {
        (f64::from_bits(ss) * (2.0f64).powi(-52)).to_bits()
    } else {
        ss
    }
}

// ============================================================================
// CABAC binary arithmetic decode (S2_cabacdecbin) + TLB match (A4_tlbmatch).
// ============================================================================
//
// Both are PURE FUNCTIONS of their register inputs (plus, for CABAC, the
// constant H.264 transition tables) — neither reads hidden global state, so both
// are oracle-backed and portable. Tables copied VERBATIM from the reference sem
// (sem/extra2.rs), recovered cell-for-cell against qemu-hexagon.

#[rustfmt::skip]
pub(crate) const HEX_R_LPS_TABLE_64X4: [[u8; 4]; 64] = [
    [128, 176, 208, 240], [128, 167, 197, 227], [128, 158, 187, 216], [123, 150, 178, 205],
    [116, 142, 169, 195], [111, 135, 160, 185], [105, 128, 152, 175], [100, 122, 144, 166],
    [ 95, 116, 137, 158], [ 90, 110, 130, 150], [ 85, 104, 123, 142], [ 81,  99, 117, 135],
    [ 77,  94, 111, 128], [ 73,  89, 105, 122], [ 69,  85, 100, 116], [ 66,  80,  95, 110],
    [ 62,  76,  90, 104], [ 59,  72,  86,  99], [ 56,  69,  81,  94], [ 53,  65,  77,  89],
    [ 51,  62,  73,  85], [ 48,  59,  69,  80], [ 46,  56,  66,  76], [ 43,  53,  63,  72],
    [ 41,  50,  59,  69], [ 39,  48,  56,  65], [ 37,  45,  54,  62], [ 35,  43,  51,  59],
    [ 33,  41,  48,  56], [ 32,  39,  46,  53], [ 30,  37,  43,  50], [ 29,  35,  41,  48],
    [ 27,  33,  39,  45], [ 26,  31,  37,  43], [ 24,  30,  35,  41], [ 23,  28,  33,  39],
    [ 22,  27,  32,  37], [ 21,  26,  30,  35], [ 20,  24,  29,  33], [ 19,  23,  27,  31],
    [ 18,  22,  26,  30], [ 17,  21,  25,  28], [ 16,  20,  23,  27], [ 15,  19,  22,  25],
    [ 14,  18,  21,  24], [ 14,  17,  20,  23], [ 13,  16,  19,  22], [ 12,  15,  18,  21],
    [ 12,  14,  17,  20], [ 11,  14,  16,  19], [ 11,  13,  15,  18], [ 10,  12,  15,  17],
    [ 10,  12,  14,  16], [  9,  11,  13,  15], [  9,  11,  12,  14], [  8,  10,  12,  14],
    [  8,   9,  11,  13], [  7,   9,  11,  12], [  7,   9,  10,  12], [  7,   8,  10,  11],
    [  6,   8,   9,  11], [  6,   7,   9,  10], [  6,   7,   8,   9], [  2,   2,   2,   2],
];

#[rustfmt::skip]
pub(crate) const HEX_AC_NEXT_STATE_MPS_64: [u8; 64] = [
     1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15, 16,
    17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48,
    49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 62, 63,
];

#[rustfmt::skip]
pub(crate) const HEX_AC_NEXT_STATE_LPS_64: [u8; 64] = [
     0,  0,  1,  2,  2,  4,  4,  5,  6,  7,  8,  9,  9, 11, 11, 12,
    13, 13, 15, 15, 16, 16, 18, 18, 19, 19, 21, 21, 22, 22, 23, 24,
    24, 25, 26, 26, 27, 27, 28, 29, 29, 30, 30, 30, 31, 32, 32, 33,
    33, 33, 34, 34, 35, 35, 35, 36, 36, 36, 37, 37, 37, 38, 38, 63,
];

/// `fINSERT_RANGE(reg, hibit, lobit, val)`: replace bits `[hibit:lobit]`.
#[inline]
pub(crate) fn hex_insert_range(reg: u32, hibit: u32, lobit: u32, val: u32) -> u32 {
    let width = hibit - lobit + 1;
    let field_mask = if width >= 32 {
        u32::MAX
    } else {
        (1u32 << width) - 1
    };
    (reg & !(field_mask << lobit)) | ((val & field_mask) << lobit)
}

/// `Rdd = decbin(Rss,Rtt)` (+P0). Byte-for-byte port of the sem's
/// `S2_cabacdecbin`. Returns `(Rdd, P0)`.
pub(crate) fn hex_cabac_decbin(rss: u64, rtt: u64) -> (u64, u8) {
    let rtt_w1 = (rtt >> 32) as u32;
    let rtt_w0 = rtt as u32;
    let state = (rtt_w1 & 0x3f) as usize;
    let val_mps = (rtt_w1 >> 8) & 1;
    let bitpos = rtt_w0 & 0x1f;

    let mut range = rss as u32; // Rss.w0
    let mut offset = (rss >> 32) as u32; // Rss.w1
    range <<= bitpos;
    offset <<= bitpos;

    let r_lps = (HEX_R_LPS_TABLE_64X4[state][((range >> 29) & 3) as usize] as u32) << 23;
    let r_mps = (range & 0xff80_0000).wrapping_sub(r_lps);

    let mut rdd_w0: u32;
    let rdd_w1: u32;
    let p0: u8;
    if offset < r_mps {
        rdd_w0 = HEX_AC_NEXT_STATE_MPS_64[state] as u32;
        rdd_w0 = hex_insert_range(rdd_w0, 8, 8, val_mps);
        rdd_w0 = hex_insert_range(rdd_w0, 31, 23, r_mps >> 23);
        rdd_w1 = offset;
        p0 = val_mps as u8;
    } else {
        rdd_w0 = HEX_AC_NEXT_STATE_LPS_64[state] as u32;
        let mps_bit = if state == 0 { 1 - val_mps } else { val_mps };
        rdd_w0 = hex_insert_range(rdd_w0, 8, 8, mps_bit);
        rdd_w0 = hex_insert_range(rdd_w0, 31, 23, r_lps >> 23);
        rdd_w1 = offset.wrapping_sub(r_mps);
        p0 = (val_mps ^ 1) as u8;
    }
    let rdd = (rdd_w0 as u64) | ((rdd_w1 as u64) << 32);
    (rdd, p0)
}

/// `Pd = tlbmatch(Rss,Rt)`. Byte-for-byte port of the sem's `A4_tlbmatch`. The
/// matched "TLB entry" is the seeded register pair `Rss` itself (no hidden TLB
/// state), so it is a pure function. Returns the 0x00/0xff predicate byte.
pub(crate) fn hex_tlbmatch(rss: u64, rt: u32) -> u8 {
    let tlblo = rss as u32; // Rss.w0
    let tlbhi = (rss >> 32) as u32; // Rss.w1
    let mut mask: u32 = 0x07ff_ffff;
    let v = (!tlblo).reverse_bits();
    let size = v.leading_ones().min(6);
    mask &= 0xffff_ffffu32.wrapping_shl(2 * size);
    let valid = (tlbhi >> 31) & 1 != 0;
    let matched = valid && ((tlbhi & mask) == (rt & mask));
    if matched { 0xff } else { 0x00 }
}

/// Port of `arch_sf_recip_common`. Returns `(ret, RsV, RtV, RdV, PeV)`.
pub(crate) fn hr_recip_common(rsv: u32, rtv: u32) -> (bool, u32, u32, u32, u8) {
    let rs_nan = hf32_is_nan(rsv);
    let rt_nan = hf32_is_nan(rtv);
    if rs_nan && rt_nan {
        return (false, HR_F32_NAN, HR_F32_NAN, HR_F32_NAN, 0);
    }
    if rs_nan {
        return (false, HR_F32_NAN, HR_F32_NAN, HR_F32_NAN, 0);
    }
    if rt_nan {
        return (false, HR_F32_NAN, HR_F32_NAN, HR_F32_NAN, 0);
    }
    if hr_is_inf(rsv) && hr_is_inf(rtv) {
        return (false, HR_F32_NAN, HR_F32_NAN, HR_F32_NAN, 0);
    }
    if hr_is_zero(rsv) && hr_is_zero(rtv) {
        return (false, HR_F32_NAN, HR_F32_NAN, HR_F32_NAN, 0);
    }
    if hr_is_zero(rtv) {
        let sign = hr_is_neg(rsv) ^ hr_is_neg(rtv);
        return (false, hr_infinite(sign), HR_F32_ONE, HR_F32_ONE, 0);
    }
    if hr_is_inf(rtv) {
        let rs = 0x8000_0000 & (rsv ^ rtv);
        return (false, rs, HR_F32_ONE, HR_F32_ONE, 0);
    }
    if hr_is_zero(rsv) {
        let rs = 0x8000_0000 & (rsv ^ rtv);
        return (false, rs, HR_F32_ONE, HR_F32_ONE, 0);
    }
    if hr_is_inf(rsv) {
        let sign = hr_is_neg(rsv) ^ hr_is_neg(rtv);
        return (false, hr_infinite(sign), HR_F32_ONE, HR_F32_ONE, 0);
    }
    // Normal path: adjust extreme exponents, set PeV. Branch order is QEMU's.
    let mut pe: u8 = 0x00;
    let n_exp = hr_getexp_raw(rsv);
    let d_exp = hr_getexp_raw(rtv);
    let (mut rs, mut rt) = (rsv, rtv);
    if (n_exp - d_exp + HR_SF_BIAS) <= HR_SF_MANTBITS {
        pe = 0x80;
        rt = hr_scalbn(rt, -64);
        rs = hr_scalbn(rs, 64);
    } else if (n_exp - d_exp + HR_SF_BIAS) > (HR_SF_MAXEXP - 24) {
        pe = 0x40;
        rt = hr_scalbn(rt, 32);
        rs = hr_scalbn(rs, -32);
    } else if n_exp <= HR_SF_MANTBITS + 2 {
        rt = hr_scalbn(rt, 64);
        rs = hr_scalbn(rs, 64);
    } else if d_exp <= 1 {
        rt = hr_scalbn(rt, 32);
        rs = hr_scalbn(rs, 32);
    } else if d_exp > 252 {
        rt = hr_scalbn(rt, -32);
        rs = hr_scalbn(rs, -32);
    }
    (true, rs, rt, 0, pe)
}

/// Port of `arch_sf_invsqrt_common`. Returns `(ret, RsV, RdV, PeV)`.
pub(crate) fn hr_invsqrt_common(rsv: u32) -> (bool, u32, u32, u8) {
    if hf32_is_nan(rsv) {
        return (false, HR_F32_NAN, HR_F32_NAN, 0);
    }
    if hr_is_neg(rsv) && !hr_is_zero(rsv) {
        return (false, HR_F32_NAN, HR_F32_NAN, 0);
    }
    if hr_is_inf(rsv) {
        return (false, hr_infinite(true), hr_infinite(true), 0);
    }
    if hr_is_zero(rsv) {
        return (false, rsv, HR_F32_ONE, 0);
    }
    let mut pe: u8 = 0x00;
    let mut rs = rsv;
    let r_exp = hr_getexp(rsv);
    if r_exp <= 24 {
        rs = hr_scalbn(rs, 64);
        pe = 0xe0;
    }
    (true, rs, 0, pe)
}

#[inline]
pub(crate) fn hr_make_sf(sign: u32, exp: i32, mant: u32) -> u32 {
    ((sign & 1) << 31) | (((exp as u32) & 0xff) << 23) | (mant & 0x007f_ffff)
}

/// Dispatch a HexFpRecip kind. Returns `(Rd, Pe)`. For the fixup kinds Pe is 0
/// (unused — the lift never wires a predicate output for them).
pub(crate) fn hex_fp_recip_eval(kind: HexFpRecipKind, rsv: u32, rtv: u32) -> (u32, u8) {
    use HexFpRecipKind::*;
    match kind {
        SfRecipa => {
            let (ret, _rs, rt, rd, pe) = hr_recip_common(rsv, rtv);
            if !ret {
                return (rd, pe);
            }
            let idx = ((rt >> 16) & 0x7f) as usize;
            let mant = ((HR_RECIP_LOOKUP[idx] as u32) << 15) | 1;
            let exp = HR_SF_BIAS - (hr_getexp_raw(rt) - HR_SF_BIAS) - 1;
            (hr_make_sf(rt >> 31, exp, mant), pe)
        }
        SfInvSqrtA => {
            let (ret, rs, rd, pe) = hr_invsqrt_common(rsv);
            if !ret {
                return (rd, pe);
            }
            let idx = ((rs >> 17) & 0x7f) as usize;
            let mant = (HR_INVSQRT_LOOKUP[idx] as u32) << 15;
            let exp = HR_SF_BIAS - ((hr_getexp_raw(rs) - HR_SF_BIAS) >> 1) - 1;
            (hr_make_sf(rs >> 31, exp, mant), pe)
        }
        SfFixupN => {
            // Rd = recip_common's adjusted Rs (numerator).
            let (_ret, rs, _rt, _rd, _pe) = hr_recip_common(rsv, rtv);
            (rs, 0)
        }
        SfFixupD => {
            // Rd = recip_common's adjusted Rt (denominator).
            let (_ret, _rs, rt, _rd, _pe) = hr_recip_common(rsv, rtv);
            (rt, 0)
        }
        SfFixupR => {
            // Rd = invsqrt_common's adjusted Rs (radicand).
            let (_ret, rs, _rd, _pe) = hr_invsqrt_common(rsv);
            (rs, 0)
        }
    }
}

/// Evaluate a Hexagon scalar FP sub-op; `a`/`b` are raw operand bits.
pub(crate) fn hex_fp_eval(op: HexFpOp, a: u64, b: u64) -> u64 {
    use HexFpOp::*;
    let a32 = a as u32;
    let b32 = b as u32;
    // Predicate helpers (Hexagon scalar predicate byte: 0x00 / 0xff).
    let pred = |hit: bool| -> u64 { if hit { 0xff } else { 0x00 } };
    match op {
        // ---- single compares ----
        SfCmpEq => pred(hf_cmp_sf(a32, b32) == HfRel::Equal),
        SfCmpGt => pred(hf_cmp_sf(a32, b32) == HfRel::Greater),
        SfCmpGe => {
            let r = hf_cmp_sf(a32, b32);
            pred(r == HfRel::Greater || r == HfRel::Equal)
        }
        SfCmpUo => pred(hf_cmp_sf(a32, b32) == HfRel::Unordered),
        // ---- double compares ----
        DfCmpEq => pred(hf_cmp_df(a, b) == HfRel::Equal),
        DfCmpGt => pred(hf_cmp_df(a, b) == HfRel::Greater),
        DfCmpGe => {
            let r = hf_cmp_df(a, b);
            pred(r == HfRel::Greater || r == HfRel::Equal)
        }
        DfCmpUo => pred(hf_cmp_df(a, b) == HfRel::Unordered),
        // ---- classify (b = class-mask immediate bits) ----
        SfClass => pred((b32 >> hf32_class_bit(a32)) & 1 == 1),
        DfClass => pred((b >> hf64_class_bit(a) as u64) & 1 == 1),
        // ---- min / max ----
        SfMin => hf_sf_minmax(a32, b32, true) as u64,
        SfMax => hf_sf_minmax(a32, b32, false) as u64,
        DfMin => hf_df_minmax(a, b, true),
        DfMax => hf_df_minmax(a, b, false),
        // ---- arithmetic (native round-to-nearest + default-NaN canonicalise) ----
        SfAdd => {
            let r = f32::from_bits(a32) + f32::from_bits(b32);
            if r.is_nan() {
                0xFFFF_FFFF
            } else {
                r.to_bits() as u64
            }
        }
        SfSub => {
            let r = f32::from_bits(a32) - f32::from_bits(b32);
            if r.is_nan() {
                0xFFFF_FFFF
            } else {
                r.to_bits() as u64
            }
        }
        SfMpy => {
            // f32*f32 is exact in f64, re-rounded to f32 (= direct f32 multiply).
            let r = (f32::from_bits(a32) as f64 * f32::from_bits(b32) as f64) as f32;
            if r.is_nan() {
                0xFFFF_FFFF
            } else {
                r.to_bits() as u64
            }
        }
        DfAdd => {
            let r = f64::from_bits(a) + f64::from_bits(b);
            if r.is_nan() {
                0xFFFF_FFFF_FFFF_FFFF
            } else {
                r.to_bits()
            }
        }
        DfSub => {
            let r = f64::from_bits(a) - f64::from_bits(b);
            if r.is_nan() {
                0xFFFF_FFFF_FFFF_FFFF
            } else {
                r.to_bits()
            }
        }
        // ---- conversions ----
        ConvDf2Sf => hf_df_to_sf(a) as u64,
        ConvSf2Df => {
            if hf32_is_nan(a32) {
                0xFFFF_FFFF_FFFF_FFFF
            } else {
                (f32::from_bits(a32) as f64).to_bits()
            }
        }
        // int -> float (result is exact rounding; never NaN). `a` carries the raw
        // source integer (32 or 64 bits) per the variant.
        ConvW2Sf => ((a32 as i32 as f32).to_bits()) as u64,
        ConvUw2Sf => ((a32 as f32).to_bits()) as u64,
        ConvD2Sf => ((a as i64 as f32).to_bits()) as u64,
        ConvUd2Sf => ((a as f32).to_bits()) as u64,
        ConvW2Df => (a32 as i32 as f64).to_bits(),
        ConvUw2Df => (a32 as f64).to_bits(),
        ConvD2Df => (a as i64 as f64).to_bits(),
        ConvUd2Df => (a as f64).to_bits(),
        // float -> int
        ConvSf2W => {
            hf_sf_to_sint(a32, false, i32::MIN as i128, i32::MAX as i128) as i32 as u32 as u64
        }
        ConvSf2WChop => {
            hf_sf_to_sint(a32, true, i32::MIN as i128, i32::MAX as i128) as i32 as u32 as u64
        }
        ConvSf2Uw => hf_sf_to_uint(a32, false, u32::MAX as u128) as u32 as u64,
        ConvSf2UwChop => hf_sf_to_uint(a32, true, u32::MAX as u128) as u32 as u64,
        ConvSf2D => hf_sf_to_sint(a32, false, i64::MIN as i128, i64::MAX as i128) as i64 as u64,
        ConvSf2DChop => hf_sf_to_sint(a32, true, i64::MIN as i128, i64::MAX as i128) as i64 as u64,
        ConvSf2Ud => hf_sf_to_uint(a32, false, u64::MAX as u128) as u64,
        ConvSf2UdChop => hf_sf_to_uint(a32, true, u64::MAX as u128) as u64,
        ConvDf2W => {
            hf_df_to_sint(a, false, i32::MIN as i128, i32::MAX as i128) as i32 as u32 as u64
        }
        ConvDf2WChop => {
            hf_df_to_sint(a, true, i32::MIN as i128, i32::MAX as i128) as i32 as u32 as u64
        }
        ConvDf2Uw => hf_df_to_uint(a, false, u32::MAX as u128) as u32 as u64,
        ConvDf2UwChop => hf_df_to_uint(a, true, u32::MAX as u128) as u32 as u64,
        ConvDf2D => hf_df_to_sint(a, false, i64::MIN as i128, i64::MAX as i128) as i64 as u64,
        ConvDf2DChop => hf_df_to_sint(a, true, i64::MIN as i128, i64::MAX as i128) as i64 as u64,
        ConvDf2Ud => hf_df_to_uint(a, false, u64::MAX as u128) as u64,
        ConvDf2UdChop => hf_df_to_uint(a, true, u64::MAX as u128) as u64,
    }
}

// ============================================================================
// Tests
// ============================================================================
