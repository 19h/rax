//! math::bf16 tests

use super::*;
use crate::isa::arm::aarch64::cpu::*;

// ---- BFloat16 (bf16) helpers (FEAT_BF16) ----

/// Widen a bf16 to f32 — exact (bf16 is the top 16 bits of an f32).
#[inline]
pub(crate) fn bf16_to_f32(b: u16) -> f32 {
    f32::from_bits((b as u32) << 16)
}
/// Convert an f32 (raw bits) to bf16 with round-to-nearest-even (the rounding
/// used by BFCVT/BFCVTN; FPCR rounding mode is ignored). NaN is quieted.
pub(crate) fn f32_to_bf16(x: u32) -> u16 {
    if (x & 0x7F80_0000) == 0x7F80_0000 {
        // Inf or NaN.
        if (x & 0x007F_FFFF) != 0 {
            // NaN: quiet it (set bf16 quiet bit), preserve sign.
            return ((x >> 16) as u16) | 0x0040;
        }
        return (x >> 16) as u16; // +/- Inf -> 0x7F80 / 0xFF80
    }
    // Round-to-nearest-even on the dropped low 16 mantissa bits. The add-bias
    // trick also carries correctly into the exponent (overflow -> bf16 Inf) and
    // handles subnormals/zero.
    let lsb = (x >> 16) & 1;
    let rounded = x.wrapping_add(0x7FFF + lsb);
    (rounded >> 16) as u16
}
pub(crate) fn f32_to_bf16_round(x: u32, rmode: u32) -> u16 {
    if (x & 0x7F80_0000) == 0x7F80_0000 {
        // Inf or NaN.
        if (x & 0x007F_FFFF) != 0 {
            return ((x >> 16) as u16) | 0x0040;
        }
        return (x >> 16) as u16;
    }

    let high = (x >> 16) as u16;
    let low = x & 0xFFFF;
    let sign = (x >> 31) & 1;
    let increment = match rmode & 0x3 {
        0 => low > 0x8000 || (low == 0x8000 && (high & 1) != 0), // nearest-even
        1 => sign == 0 && low != 0,                              // +Inf
        2 => sign != 0 && low != 0,                              // -Inf
        _ => false,                                              // zero
    };
    high.wrapping_add(increment as u16)
}
pub(crate) fn f32_to_bf16_with_fpcr(x: u32, fpcr: u32) -> u16 {
    let x = if fpcr & FPCR_AH != 0 && fp32_is_tiny(x) {
        x & 0x8000_0000
    } else {
        fp32_flush_input_with_fpcr(x, fpcr)
    };
    f32_to_bf16_round(x, (fpcr >> 22) & 0x3)
}
/// FEAT_SVE_B16B16 bf16 binary op (BFADD/BFSUB/BFMUL/BFMAX/BFMIN/BFMAXNM/
/// BFMINNM), ARM-correct at FPCR defaults. bf16 widens to f32 exactly (16-bit
/// left shift). NaN/Inf operands and the MUL/MAX/MIN family resolve through the
/// verified ARM f32 op then narrow round-to-nearest-even (a bf16 product is
/// exact in f32, and MAX/MIN return an operand, so a single narrow is exact).
/// Finite ADD/SUB accumulate the exact sum in f64 and narrow once (round-to-odd
/// to f32 then RNE to bf16) to avoid the f32-then-bf16 double rounding.
pub(crate) fn bf16_binop(kind: FpKind, a: u16, b: u16) -> u16 {
    let af = (a as u32) << 16;
    let bf = (b as u32) << 16;
    let non_finite = |x: u32| x & 0x7F80_0000 == 0x7F80_0000;
    if !matches!(kind, FpKind::Add | FpKind::Sub) || non_finite(af) || non_finite(bf) {
        return f32_to_bf16(fp_three_same_f32(kind, af, bf, 0));
    }
    let x = f32::from_bits(af) as f64;
    let y = f32::from_bits(bf) as f64;
    let s = if matches!(kind, FpKind::Sub) {
        x - y
    } else {
        x + y
    };
    f32_to_bf16(round_odd_f64_to_f32(s))
}
/// FEAT_SVE_B16B16 bf16 fused multiply-add (BFMLA: Zda+Zn*Zm, BFMLS: negate Zn
/// first). Finite operands accumulate the exact product in f64 then narrow once
/// (the bf16*bf16 product is exact in f64); NaN/Inf use the verified f32 fused
/// multiply-add. The negate-input form matches the FPCR.AH=0 oracle behaviour.
pub(crate) fn bf16_fma(a: u16, n: u16, m: u16, sub: bool) -> u16 {
    let af = (a as u32) << 16;
    let nf = ((n ^ if sub { 0x8000 } else { 0 }) as u32) << 16;
    let mf = (m as u32) << 16;
    let non_finite = |x: u32| x & 0x7F80_0000 == 0x7F80_0000;
    if non_finite(af) || non_finite(nf) || non_finite(mf) {
        return f32_to_bf16(fp_muladd_bits(af as u64, nf as u64, mf as u64, 32) as u32);
    }
    let p = (f32::from_bits(nf) as f64) * (f32::from_bits(mf) as f64);
    let s = (f32::from_bits(af) as f64) + p;
    f32_to_bf16(round_odd_f64_to_f32(s))
}
/// One lane of the FEAT_SVE2p1 FP 2-way dot product FDOT (f16 -> f32): the f32
/// accumulator `sum` plus the two f16 products of the 32-bit groups e1, e2,
/// computed with a single rounding (ARM FPDot) then a separate (non-fused) f32
/// accumulate. Faithful port of qemu f16_dotadd: NaN inputs follow
/// FPProcessNaNs4 (first signalling, else first quiet, widened+quieted); finite
/// products are exact in f64, summed via a Knuth 2Sum and rounded once to f32
/// (round-to-odd of the exact sum -> RNE-to-f32 is double-rounding-safe).
pub(crate) fn f16_dotadd(sum: u32, e1: u32, e2: u32) -> u32 {
    let lanes = [e1 as u16, (e1 >> 16) as u16, e2 as u16, (e2 >> 16) as u16];
    let is_nan = |h: u16| (h & 0x7C00) == 0x7C00 && (h & 0x3FF) != 0;
    let is_snan = |h: u16| is_nan(h) && (h & 0x0200) == 0; // f16 quiet bit = bit 9
    let t32: u32 = if lanes.iter().any(|&h| is_nan(h)) {
        let pick = lanes
            .iter()
            .copied()
            .find(|&h| is_snan(h))
            .or_else(|| lanes.iter().copied().find(|&h| is_nan(h)))
            .unwrap();
        AArch64Cpu::fp16_to_f32(pick).to_bits() | 0x0040_0000 // quieted
    } else {
        let f = |h: u16| fp16_to_f64(h);
        let p1 = f(lanes[0]) * f(lanes[2]); // h1r*h2r (exact in f64)
        let p2 = f(lanes[1]) * f(lanes[3]); // h1c*h2c (exact in f64)
        // Knuth 2Sum: hi + lo == p1 + p2 exactly.
        let hi = p1 + p2;
        let v = hi - p1;
        let lo = (p1 - (hi - v)) + (p2 - v);
        // Round the exact sum once to f32: round-to-odd in f64 (force the
        // mantissa odd when inexact) then RNE narrow is double-rounding-safe.
        let s_odd = if lo != 0.0 {
            f64::from_bits(hi.to_bits() | 1)
        } else {
            hi
        };
        (s_odd as f32).to_bits()
    };
    fp_three_same_f32(FpKind::Add, sum, t32, 0)
}
/// One round-to-odd f32 add step (`a + b` rounded once to f32, returned widened
/// to f64 for chaining). The BF16 dot/matrix instructions accumulate as a
/// sequence of these per-pair round-to-odd adds (matching the hardware), NOT a
/// single round of the exact multi-term sum.
#[inline]
pub(crate) fn bf_odd_add(a: f64, b: f64) -> f64 {
    f32::from_bits(round_odd_f64_to_f32(a + b)) as f64
}
/// One f32 multiply for the EBF==0 BF16 dot path: flush-to-zero inputs, exact
/// product in f64 (the widened bf16 operands have <=8-bit significands so the
/// product is exact), round-to-odd-INF to f32, flush-to-zero result. Mirrors
/// qemu float32_mul under the is_ebf(EBF=0) float_status.
pub(crate) fn bf_f32_mul(a: u32, b: u32) -> u32 {
    let af = f32::from_bits(ftz_f32_bits(a)) as f64;
    let bf = f32::from_bits(ftz_f32_bits(b)) as f64;
    ftz_f32_bits(round_odd_inf_f64_to_f32(af * bf))
}
/// One f32 add for the EBF==0 BF16 dot path: flush-to-zero inputs, EXACT sum via
/// 2Sum, round-to-odd at f64, then round-to-odd-INF to f32, flush-to-zero
/// result. The 2Sum + f64 round-to-odd captures the sticky bit even when the
/// operands differ by more than f64 precision (where a plain f64 add would lose
/// it), so the final f32 round-to-odd is exact. Mirrors qemu float32_add.
pub(crate) fn bf_f32_add(a: u32, b: u32) -> u32 {
    let af = f32::from_bits(ftz_f32_bits(a)) as f64;
    let bf = f32::from_bits(ftz_f32_bits(b)) as f64;
    let hi = af + bf;
    if hi.is_nan() {
        return 0x7FC0_0000;
    }
    if hi.is_infinite() {
        return round_odd_inf_f64_to_f32(hi);
    }
    // Knuth 2Sum: lo is the exact rounding error, hi + lo == af + bf exactly.
    let bb = hi - af;
    let lo = (af - (hi - bb)) + (bf - bb);
    let v64 = if lo == 0.0 {
        hi
    } else if hi.to_bits() & 1 == 1 {
        hi // hi already has an odd mantissa LSB
    } else {
        nextafter_f64(hi, lo > 0.0)
    };
    ftz_f32_bits(round_odd_inf_f64_to_f32(v64))
}
/// BFloat16 2-way dot product accumulate (FPCR.EBF==0), the qemu-user default.
/// Mirrors qemu bfdotadd: two products and two adds, each an f32 op under the
/// round-to-odd-INF / flush-to-zero / default-NaN float_status. `e1`/`e2` are
/// the 32-bit source slots (two bf16 each); `sum_bits` is the f32 accumulator.
pub(crate) fn bfdotadd_ebf0(sum_bits: u32, e1: u32, e2: u32) -> u32 {
    let t1 = bf_f32_mul(e1 << 16, e2 << 16);
    let t2 = bf_f32_mul(e1 & 0xFFFF_0000, e2 & 0xFFFF_0000);
    let t = bf_f32_add(t1, t2);
    bf_f32_add(sum_bits, t)
}
#[inline]
pub(crate) fn bf16_dot_result_with_fpcr(bits: u32, fpcr: u32) -> u32 {
    if fpcr & FPCR_AH != 0 && bits == 0x7fc0_0000 {
        0xffc0_0000
    } else {
        bits
    }
}
#[inline]
pub(crate) fn bfmlal_f32_input_with_fpcr(bits: u32, fpcr: u32) -> u32 {
    if fpcr & FPCR_AH != 0 && fp32_is_tiny(bits) {
        bits & 0x8000_0000
    } else {
        fp32_flush_input_with_fpcr(bits, fpcr)
    }
}
#[inline]
pub(crate) fn bfmlal_ah_result(addend: u32, op1: u32, op2: u32, fpcr: u32) -> Option<u32> {
    if fpcr & FPCR_AH == 0 {
        return None;
    }
    for bits in [op1, op2, addend] {
        if is_nan32(bits) {
            return Some(if is_snan32(bits) {
                bits | 0x0040_0000
            } else {
                bits
            });
        }
    }
    let op1 = bfmlal_f32_input_with_fpcr(op1, fpcr);
    let op2 = bfmlal_f32_input_with_fpcr(op2, fpcr);
    if (fp32_is_zero(op1) && fp32_is_inf(op2)) || (fp32_is_inf(op1) && fp32_is_zero(op2)) {
        return Some(0xffc0_0000);
    }
    None
}
#[inline]
pub(crate) fn fmlal_ah_result(addend: u32, op1: u32, op2: u32, fpcr: u32) -> Option<u32> {
    if fpcr & FPCR_AH == 0 {
        return None;
    }
    for bits in [op1, op2, addend] {
        if is_nan32(bits) {
            return Some(if is_snan32(bits) {
                bits | 0x0040_0000
            } else {
                bits
            });
        }
    }
    None
}
#[inline]
pub(crate) fn fmlal_default_invalid_result(
    addend: u32,
    op1: u32,
    op2: u32,
    fpcr: u32,
) -> Option<u32> {
    if fpcr & FPCR_AH != 0 || !is_nan32(addend) || is_snan32(addend) {
        return None;
    }
    if fp_invalid_fma_default_nan(4, addend as u64, op1 as u64, op2 as u64) {
        Some(0x7fc0_0000)
    } else {
        None
    }
}
#[inline]
pub(crate) fn bfmlal_f32_input_status(bits: u64, fpcr: u32) -> u32 {
    if fpcr & FPCR_AH != 0 {
        0
    } else {
        fp_fz_input_status(4, bits, fpcr)
    }
}
