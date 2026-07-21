//! Deterministic binary80 x87 transcendental approximations.
//!
//! Intel specifies bounded-error approximations rather than a uniquely rounded
//! bit pattern for these instructions.  The implementation below retains the
//! complete 64-bit binary80 significand, uses double-double intermediates with
//! at least 96 useful bits through the final FCW rounding decision, and uses
//! Intel's documented 68-bit internal approximation of pi for trigonometric
//! argument reduction.  No source operand is narrowed to IEEE binary64.

use super::*;

use std::cmp::Ordering;

const X87_ONE: [u8; 10] = [0, 0, 0, 0, 0, 0, 0, 0x80, 0xFF, 0x3F];
const X87_NEG_ONE: [u8; 10] = [0, 0, 0, 0, 0, 0, 0, 0x80, 0xFF, 0xBF];
const X87_NEG_HALF: [u8; 10] = [0, 0, 0, 0, 0, 0, 0, 0x80, 0xFE, 0xBF];

#[derive(Clone, Copy, Debug)]
struct Dd {
    hi: f64,
    lo: f64,
}

#[derive(Clone, Copy, Debug)]
struct ScaledDd {
    value: Dd,
    binary_scale: i32,
}

#[derive(Clone, Copy, Debug)]
struct LogApprox {
    scaled: ScaledDd,
    exact_raw: Option<[u8; 10]>,
}

impl Dd {
    const ZERO: Self = Self { hi: 0.0, lo: 0.0 };
    const ONE: Self = Self { hi: 1.0, lo: 0.0 };

    fn from_f64(value: f64) -> Self {
        Self { hi: value, lo: 0.0 }
    }

    fn from_u64(value: u64) -> Self {
        let high = (value >> 32) as f64 * 4_294_967_296.0;
        let low = (value & 0xFFFF_FFFF) as f64;
        Self::renormalize(high, low)
    }

    fn from_i64(value: i64) -> Self {
        let magnitude = Self::from_u64(value.unsigned_abs());
        if value < 0 {
            magnitude.neg()
        } else {
            magnitude
        }
    }

    fn renormalize(high: f64, low: f64) -> Self {
        let sum = high + low;
        let error = low - (sum - high);
        Self { hi: sum, lo: error }
    }

    fn add(self, rhs: Self) -> Self {
        let sum = self.hi + rhs.hi;
        let virtual_rhs = sum - self.hi;
        let error = (self.hi - (sum - virtual_rhs)) + (rhs.hi - virtual_rhs);
        Self::renormalize(sum, error + self.lo + rhs.lo)
    }

    fn sub(self, rhs: Self) -> Self {
        self.add(rhs.neg())
    }

    fn neg(self) -> Self {
        Self {
            hi: -self.hi,
            lo: -self.lo,
        }
    }

    fn abs(self) -> Self {
        if self.is_negative() { self.neg() } else { self }
    }

    fn mul(self, rhs: Self) -> Self {
        let product = self.hi * rhs.hi;
        let error = self.hi.mul_add(rhs.hi, -product)
            + self.hi * rhs.lo
            + self.lo * rhs.hi
            + self.lo * rhs.lo;
        Self::renormalize(product, error)
    }

    fn mul_f64(self, rhs: f64) -> Self {
        self.mul(Self::from_f64(rhs))
    }

    fn div(self, rhs: Self) -> Self {
        let q0 = self.hi / rhs.hi;
        let r0 = self.sub(rhs.mul_f64(q0));
        let q1 = r0.hi / rhs.hi;
        let q01 = Self::renormalize(q0, q1);
        let r1 = self.sub(rhs.mul(q01));
        Self::renormalize(q01.hi, q01.lo + r1.hi / rhs.hi)
    }

    fn div_f64(self, rhs: f64) -> Self {
        self.div(Self::from_f64(rhs))
    }

    fn scale_pow2(self, exponent: i32) -> Self {
        debug_assert!((-1022..=1023).contains(&exponent));
        let factor = f64::from_bits(((exponent + 1023) as u64) << 52);
        Self {
            hi: self.hi * factor,
            lo: self.lo * factor,
        }
    }

    fn is_zero(self) -> bool {
        self.hi == 0.0 && self.lo == 0.0
    }

    fn is_negative(self) -> bool {
        self.hi < 0.0 || (self.hi == 0.0 && self.lo < 0.0)
    }

    fn cmp_f64(self, rhs: f64) -> Ordering {
        match self.hi.partial_cmp(&rhs).unwrap() {
            Ordering::Equal => self.lo.partial_cmp(&0.0).unwrap(),
            ordering => ordering,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TranscendentalResult {
    raw: [u8; 10],
    invalid: bool,
    denormal: bool,
    zero_divide: bool,
    overflow: bool,
    underflow: bool,
    inexact: bool,
    rounded_up: bool,
}

impl TranscendentalResult {
    fn exact(raw: [u8; 10]) -> Self {
        Self {
            raw,
            invalid: false,
            denormal: false,
            zero_divide: false,
            overflow: false,
            underflow: false,
            inexact: false,
            rounded_up: false,
        }
    }

    fn invalid(raw: [u8; 10]) -> Self {
        Self {
            raw,
            invalid: true,
            ..Self::exact(raw)
        }
    }

    fn from_multiply(result: X87MultiplyResult) -> Self {
        Self {
            raw: result.raw,
            invalid: result.invalid,
            denormal: result.denormal,
            zero_divide: false,
            overflow: result.overflow,
            underflow: result.underflow,
            inexact: result.inexact,
            rounded_up: result.rounded_up,
        }
    }

    fn merge_flags(mut self, other: Self) -> Self {
        self.invalid |= other.invalid;
        self.denormal |= other.denormal;
        self.zero_divide |= other.zero_divide;
        self.overflow |= other.overflow;
        self.underflow |= other.underflow;
        self.inexact |= other.inexact;
        self.rounded_up |= other.rounded_up;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct U192([u64; 3]);

impl U192 {
    const ZERO: Self = Self([0; 3]);

    fn from_u64_shift(value: u64, shift: u32) -> Self {
        let mut limbs = [0u64; 3];
        let word = (shift / 64) as usize;
        let bits = shift % 64;
        limbs[word] = value << bits;
        if bits != 0 {
            limbs[word + 1] = value >> (64 - bits);
        }
        Self(limbs)
    }

    fn bit(self, bit: u32) -> bool {
        self.0[(bit / 64) as usize] & (1u64 << (bit % 64)) != 0
    }

    fn bit_len(self) -> u32 {
        for index in (0..3).rev() {
            if self.0[index] != 0 {
                return index as u32 * 64 + (64 - self.0[index].leading_zeros());
            }
        }
        0
    }

    fn shl_one(&mut self) {
        let mut carry = 0;
        for limb in &mut self.0 {
            let next = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
    }

    fn sub(self, rhs: Self) -> Self {
        let mut output = [0u64; 3];
        let mut borrow = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (partial, first_borrow) = self.0[index].overflowing_sub(rhs.0[index]);
            let (value, second_borrow) = partial.overflowing_sub(u64::from(borrow));
            *slot = value;
            borrow = first_borrow || second_borrow;
        }
        debug_assert!(!borrow);
        Self(output)
    }

    fn to_dd_scaled_2_neg_67(self) -> Dd {
        debug_assert_eq!(self.0[2], 0);
        Dd::from_u64(self.0[0])
            .scale_pow2(-67)
            .add(Dd::from_u64(self.0[1]).scale_pow2(-3))
    }
}

impl Ord for U192 {
    fn cmp(&self, rhs: &Self) -> Ordering {
        self.0.iter().rev().cmp(rhs.0.iter().rev())
    }
}

impl PartialOrd for U192 {
    fn partial_cmp(&self, rhs: &Self) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl SmirInterpreter {
    pub(crate) fn x86_x87_execute_transcendental(
        original: &crate::smir::X86X87State,
        next: &mut crate::smir::X86X87State,
        kind: X86X87TranscendentalKind,
    ) {
        match kind {
            X86X87TranscendentalKind::Exp2MinusOne => {
                Self::x86_x87_transcendental_unary(original, next, Self::x86_x87_f2xm1);
            }
            X86X87TranscendentalKind::Sine | X86X87TranscendentalKind::Cosine => {
                Self::x86_x87_trig_unary(original, next, kind);
            }
            X86X87TranscendentalKind::Tangent | X86X87TranscendentalKind::SineCosine => {
                Self::x86_x87_trig_push(original, next, kind);
            }
            X86X87TranscendentalKind::YLog2X => {
                Self::x86_x87_transcendental_binary_pop(original, next, Self::x86_x87_fyl2x);
            }
            X86X87TranscendentalKind::YLog2Xp1 => {
                Self::x86_x87_transcendental_binary_pop(original, next, Self::x86_x87_fyl2xp1);
            }
            X86X87TranscendentalKind::Arctangent => {
                Self::x86_x87_transcendental_binary_pop(original, next, Self::x86_x87_fpatan);
            }
        }
    }

    fn x86_x87_transcendental_unary(
        original: &crate::smir::X86X87State,
        next: &mut crate::smir::X86X87State,
        operation: fn(&[u8; 10], u16) -> TranscendentalResult,
    ) {
        let physical = original.physical_index(0);
        if original.physical_tag(physical) == 3 {
            if next.signal_stack_fault(false) {
                next.set_logical_raw_tagged(0, crate::smir::X86X87State::INDEFINITE, 2);
            }
            return;
        }
        let result = operation(&original.regs[physical], original.control_word);
        Self::x86_x87_commit_transcendental(next, 0, result);
    }

    fn x86_x87_transcendental_binary_pop(
        original: &crate::smir::X86X87State,
        next: &mut crate::smir::X86X87State,
        operation: fn(&[u8; 10], &[u8; 10], u16) -> TranscendentalResult,
    ) {
        let p0 = original.physical_index(0);
        let p1 = original.physical_index(1);
        if original.physical_tag(p0) == 3 || original.physical_tag(p1) == 3 {
            if next.signal_stack_fault(false) {
                next.set_logical_raw_tagged(1, crate::smir::X86X87State::INDEFINITE, 2);
                next.pop();
            }
            return;
        }
        let result = operation(
            &original.regs[p0],
            &original.regs[p1],
            original.control_word,
        );
        if Self::x86_x87_commit_transcendental(next, 1, result) {
            next.pop();
        }
    }

    fn x86_x87_trig_unary(
        original: &crate::smir::X86X87State,
        next: &mut crate::smir::X86X87State,
        kind: X86X87TranscendentalKind,
    ) {
        let physical = original.physical_index(0);
        if original.physical_tag(physical) == 3 {
            if next.signal_stack_fault(false) {
                next.set_logical_raw_tagged(0, crate::smir::X86X87State::INDEFINITE, 2);
            }
            return;
        }
        let raw = &original.regs[physical];
        let info = Self::x86_x87_raw_info(raw);
        if !info.unsupported && !info.nan && !Self::x86_x87_is_infinite(raw) {
            if Self::x86_x87_trig_out_of_range(raw) {
                next.status_word |= 0x0400; // C2=1; operand remains unchanged.
                return;
            }
            next.status_word &= !0x0400; // C2=0 for every completed in-range form.
        }
        let (sine, cosine) = Self::x86_x87_sine_cosine(raw, original.control_word);
        let result = if kind == X86X87TranscendentalKind::Sine {
            sine
        } else {
            cosine
        };
        Self::x86_x87_commit_transcendental(next, 0, result);
    }

    fn x86_x87_trig_push(
        original: &crate::smir::X86X87State,
        next: &mut crate::smir::X86X87State,
        kind: X86X87TranscendentalKind,
    ) {
        let source_physical = original.physical_index(0);
        if original.physical_tag(source_physical) == 3 {
            Self::x86_x87_masked_transcendental_push_fault(original, next, false);
            return;
        }
        let raw = &original.regs[source_physical];
        let info = Self::x86_x87_raw_info(raw);
        if !info.unsupported && !info.nan && !Self::x86_x87_is_infinite(raw) {
            if Self::x86_x87_trig_out_of_range(raw) {
                next.status_word |= 0x0400;
                return;
            }
            next.status_word &= !0x0400;
        }

        let new_top = original.top().wrapping_sub(1) & 7;
        if original.physical_tag(new_top as usize) != 3 {
            Self::x86_x87_masked_transcendental_push_fault(original, next, true);
            return;
        }

        let (source_result, pushed_result) = if kind == X86X87TranscendentalKind::Tangent {
            (
                Self::x86_x87_tangent(raw, original.control_word),
                TranscendentalResult::exact(X87_ONE),
            )
        } else {
            let (sine, cosine) = Self::x86_x87_sine_cosine(raw, original.control_word);
            (sine.merge_flags(cosine), cosine)
        };

        if !Self::x86_x87_commit_transcendental(next, 0, source_result) {
            return;
        }
        next.set_top(new_top);
        next.set_logical_raw(0, pushed_result.raw);
    }

    fn x86_x87_masked_transcendental_push_fault(
        original: &crate::smir::X86X87State,
        next: &mut crate::smir::X86X87State,
        overflow: bool,
    ) {
        if !next.signal_stack_fault(overflow) {
            return;
        }
        let source_physical = original.physical_index(0);
        next.regs[source_physical] = crate::smir::X86X87State::INDEFINITE;
        next.set_physical_tag(source_physical, 2);
        let new_top = original.top().wrapping_sub(1) & 7;
        next.set_top(new_top);
        next.set_logical_raw_tagged(0, crate::smir::X86X87State::INDEFINITE, 2);
    }

    /// Apply pre-computation exception priority and post-computation exception
    /// commitment.  A false return suppresses the architectural result/pop.
    fn x86_x87_commit_transcendental(
        next: &mut crate::smir::X86X87State,
        destination: u8,
        result: TranscendentalResult,
    ) -> bool {
        next.status_word &= !0x0200; // C1=0 unless magnitude rounding increments.
        if result.invalid {
            next.status_word |= 0x0001;
            if next.control_word & 0x0001 == 0 {
                next.status_word |= 0x8080;
                return false;
            }
        } else {
            if result.denormal {
                next.status_word |= 0x0002;
                if next.control_word & 0x0002 == 0 {
                    next.status_word |= 0x8080;
                    return false;
                }
            }
            if result.zero_divide {
                next.status_word |= 0x0004;
                if next.control_word & 0x0004 == 0 {
                    next.status_word |= 0x8080;
                    return false;
                }
            }
        }

        if result.overflow {
            next.status_word |= 0x0008;
        }
        if result.underflow {
            next.status_word |= 0x0010;
        }
        if result.inexact {
            next.status_word |= 0x0020;
        }
        if result.rounded_up {
            next.status_word |= 0x0200;
        }
        if (result.overflow && next.control_word & 0x0008 == 0)
            || (result.underflow && next.control_word & 0x0010 == 0)
            || (result.inexact && next.control_word & 0x0020 == 0)
        {
            next.status_word |= 0x8080;
        }
        next.set_logical_raw(destination, result.raw);
        true
    }

    fn x86_x87_f2xm1(raw: &[u8; 10], control_word: u16) -> TranscendentalResult {
        let info = Self::x86_x87_raw_info(raw);
        if let Some(result) = Self::x86_x87_unary_nonfinite(raw, info, false) {
            return result;
        }
        if info.zero || *raw == X87_ONE || *raw == X87_NEG_ONE {
            let result = if *raw == X87_ONE {
                X87_ONE
            } else if *raw == X87_NEG_ONE {
                X87_NEG_HALF
            } else {
                *raw
            };
            return TranscendentalResult {
                denormal: info.denormal,
                ..TranscendentalResult::exact(result)
            };
        }

        let (sign, significand, exponent) = Self::x86_x87_finite_parts(raw);
        if exponent > 0 || (exponent == 0 && significand > 0x8000_0000_0000_0000) {
            // Intel defines the result outside [-1,+1] as undefined.  Preserve
            // the operand as RAX's deterministic, non-faulting profile.
            return TranscendentalResult {
                raw: *raw,
                denormal: info.denormal,
                ..TranscendentalResult::exact(*raw)
            };
        }

        let magnitude = Self::x86_x87_dd_from_significand(significand);
        let value = if exponent < -900 {
            magnitude.mul(Self::x86_x87_ln2())
        } else {
            let x = magnitude.scale_pow2(exponent);
            let signed_x = if sign { x.neg() } else { x };
            let z = signed_x.mul(Self::x86_x87_ln2());
            let mut term = z;
            let mut sum = z;
            for n in 2..=52 {
                term = term.mul(z).div_f64(n as f64);
                sum = sum.add(term);
            }
            return Self::x86_x87_round_dd(sum, 0, control_word, true, info.denormal);
        };
        Self::x86_x87_round_dd(
            if sign { value.neg() } else { value },
            exponent,
            control_word,
            true,
            info.denormal,
        )
    }

    fn x86_x87_fyl2x(x: &[u8; 10], y: &[u8; 10], control_word: u16) -> TranscendentalResult {
        let x_info = Self::x86_x87_raw_info(x);
        let (logarithm, zero_divide) = if x_info.unsupported {
            (
                TranscendentalResult::invalid(crate::smir::X86X87State::INDEFINITE),
                false,
            )
        } else if x_info.nan {
            (
                TranscendentalResult {
                    invalid: x_info.signaling_nan,
                    raw: if x_info.signaling_nan {
                        Self::x86_x87_quiet_nan(x)
                    } else {
                        *x
                    },
                    ..TranscendentalResult::exact(*x)
                },
                false,
            )
        } else if x_info.sign && !x_info.zero {
            (
                TranscendentalResult::invalid(crate::smir::X86X87State::INDEFINITE),
                false,
            )
        } else if x_info.zero {
            (
                TranscendentalResult::exact(Self::x86_x87_infinity(true)),
                true,
            )
        } else if Self::x86_x87_is_infinite(x) {
            (
                TranscendentalResult::exact(Self::x86_x87_infinity(false)),
                false,
            )
        } else {
            return Self::x86_x87_multiply_log_approx(
                y,
                Self::x86_x87_log2_approx(x),
                control_word,
                x_info.denormal,
            );
        };

        let multiplied = Self::x86_x87_multiply(
            y,
            &logarithm.raw,
            x_info.signaling_nan,
            x_info.denormal,
            control_word,
        );
        let mut result = TranscendentalResult::from_multiply(multiplied);
        result.invalid |= logarithm.invalid;
        result.denormal |= logarithm.denormal;
        result.zero_divide = zero_divide && !result.invalid;
        result
    }

    fn x86_x87_fyl2xp1(x: &[u8; 10], y: &[u8; 10], control_word: u16) -> TranscendentalResult {
        let x_info = Self::x86_x87_raw_info(x);
        if !x_info.unsupported && !x_info.nan && !x_info.zero && !Self::x86_x87_is_infinite(x) {
            return match Self::x86_x87_log2p1_approx(x) {
                Ok(logarithm) => {
                    Self::x86_x87_multiply_log_approx(y, logarithm, control_word, x_info.denormal)
                }
                Err(result) => result,
            };
        }

        let logarithm = if x_info.unsupported {
            TranscendentalResult::invalid(crate::smir::X86X87State::INDEFINITE)
        } else if x_info.nan {
            TranscendentalResult {
                invalid: x_info.signaling_nan,
                raw: if x_info.signaling_nan {
                    Self::x86_x87_quiet_nan(x)
                } else {
                    *x
                },
                ..TranscendentalResult::exact(*x)
            }
        } else if x_info.zero {
            TranscendentalResult::exact(*x)
        } else if x_info.sign {
            TranscendentalResult::invalid(crate::smir::X86X87State::INDEFINITE)
        } else {
            TranscendentalResult::exact(Self::x86_x87_infinity(false))
        };

        let multiplied = Self::x86_x87_multiply(
            y,
            &logarithm.raw,
            x_info.signaling_nan,
            x_info.denormal,
            control_word,
        );
        let mut result = TranscendentalResult::from_multiply(multiplied);
        result.invalid |= logarithm.invalid;
        result.denormal |= logarithm.denormal;
        result
    }

    fn x86_x87_fpatan(x: &[u8; 10], y: &[u8; 10], control_word: u16) -> TranscendentalResult {
        let x_info = Self::x86_x87_raw_info(x);
        let y_info = Self::x86_x87_raw_info(y);
        if x_info.unsupported || y_info.unsupported {
            return TranscendentalResult::invalid(crate::smir::X86X87State::INDEFINITE);
        }
        if x_info.nan || y_info.nan {
            let (raw, invalid) = Self::x86_x87_binary_nan(x, y, x_info, y_info);
            return TranscendentalResult {
                raw,
                invalid,
                ..TranscendentalResult::exact(raw)
            };
        }

        let x_infinite = Self::x86_x87_is_infinite(x);
        let y_infinite = Self::x86_x87_is_infinite(y);
        let denormal = x_info.denormal || y_info.denormal;
        let signed_angle = |magnitude: Dd, sign: bool, inexact: bool| {
            let value = if sign { magnitude.neg() } else { magnitude };
            let mut result = Self::x86_x87_round_dd(value, 0, control_word, inexact, denormal);
            result.denormal |= denormal;
            result
        };

        if y_info.zero {
            if x_info.sign {
                return signed_angle(Self::x86_x87_pi(), y_info.sign, true);
            }
            return TranscendentalResult {
                raw: Self::x86_x87_signed_zero(y_info.sign),
                denormal,
                ..TranscendentalResult::exact(Self::x86_x87_signed_zero(y_info.sign))
            };
        }
        if x_info.zero {
            return signed_angle(Self::x86_x87_pi().scale_pow2(-1), y_info.sign, true);
        }
        if x_infinite && y_infinite {
            let magnitude = if x_info.sign {
                Self::x86_x87_pi().mul_f64(0.75)
            } else {
                Self::x86_x87_pi().scale_pow2(-2)
            };
            return signed_angle(magnitude, y_info.sign, true);
        }
        if y_infinite {
            return signed_angle(Self::x86_x87_pi().scale_pow2(-1), y_info.sign, true);
        }
        if x_infinite {
            if x_info.sign {
                return signed_angle(Self::x86_x87_pi(), y_info.sign, true);
            }
            return TranscendentalResult {
                raw: Self::x86_x87_signed_zero(y_info.sign),
                denormal,
                ..TranscendentalResult::exact(Self::x86_x87_signed_zero(y_info.sign))
            };
        }

        let (_, x_sig, x_exp) = Self::x86_x87_finite_parts(x);
        let (_, y_sig, y_exp) = Self::x86_x87_finite_parts(y);
        let x_magnitude = Self::x86_x87_dd_from_significand(x_sig);
        let y_magnitude = Self::x86_x87_dd_from_significand(y_sig);
        let compare = (y_exp, y_sig).cmp(&(x_exp, x_sig));
        let (numerator, denominator, exponent_delta, reciprocal) = if compare != Ordering::Greater {
            (y_magnitude, x_magnitude, y_exp - x_exp, false)
        } else {
            (x_magnitude, y_magnitude, x_exp - y_exp, true)
        };

        let base = if exponent_delta < -900 {
            if !reciprocal && !x_info.sign {
                let ratio = numerator.div(denominator);
                let signed = if y_info.sign { ratio.neg() } else { ratio };
                return Self::x86_x87_round_dd(
                    signed,
                    exponent_delta,
                    control_word,
                    true,
                    denormal,
                );
            }
            Dd::ZERO
        } else {
            let ratio = numerator.div(denominator).scale_pow2(exponent_delta);
            Self::x86_x87_atan_unit(ratio)
        };
        let mut magnitude = if reciprocal {
            Self::x86_x87_pi().scale_pow2(-1).sub(base)
        } else {
            base
        };
        if x_info.sign {
            magnitude = Self::x86_x87_pi().sub(magnitude);
        }
        signed_angle(magnitude, y_info.sign, true)
    }

    fn x86_x87_sine_cosine(
        raw: &[u8; 10],
        control_word: u16,
    ) -> (TranscendentalResult, TranscendentalResult) {
        let info = Self::x86_x87_raw_info(raw);
        if let Some(result) = Self::x86_x87_unary_nonfinite(raw, info, true) {
            return (result, result);
        }
        if info.zero {
            return (
                TranscendentalResult::exact(*raw),
                TranscendentalResult::exact(X87_ONE),
            );
        }

        let (sine, cosine) = Self::x86_x87_sine_cosine_approx(raw);
        (
            Self::x86_x87_round_dd(
                sine.value,
                sine.binary_scale,
                control_word,
                true,
                info.denormal,
            ),
            Self::x86_x87_round_dd(
                cosine.value,
                cosine.binary_scale,
                control_word,
                true,
                info.denormal,
            ),
        )
    }

    fn x86_x87_sine_cosine_approx(raw: &[u8; 10]) -> (ScaledDd, ScaledDd) {
        let (sign, significand, exponent) = Self::x86_x87_finite_parts(raw);
        if exponent < -40 {
            // At this scale the first omitted term is far below one PC64 ulp,
            // but its direction still resolves exact and halfway cases for
            // directed rounding.  A 2^-90 relative decrement is larger than
            // the DD guard tail and smaller than every target quantum.
            let magnitude = Self::x86_x87_dd_from_significand(significand);
            let sine_magnitude = magnitude.sub(magnitude.scale_pow2(-90));
            let sine = if sign {
                sine_magnitude.neg()
            } else {
                sine_magnitude
            };
            let cosine = Dd::ONE.sub(Dd::ONE.scale_pow2(-90));
            return (
                ScaledDd {
                    value: sine,
                    binary_scale: exponent,
                },
                ScaledDd {
                    value: cosine,
                    binary_scale: 0,
                },
            );
        }

        let (reduced, quadrant) = Self::x86_x87_reduce_trig(significand, exponent, sign);
        let x2 = reduced.mul(reduced);
        let mut sine_term = reduced;
        let mut sine = reduced;
        let mut cosine_term = Dd::ONE;
        let mut cosine = Dd::ONE;
        for n in 1..=30 {
            let sine_denominator = (2 * n * (2 * n + 1)) as f64;
            sine_term = sine_term.mul(x2).div_f64(-sine_denominator);
            sine = sine.add(sine_term);

            let cosine_denominator = ((2 * n - 1) * (2 * n)) as f64;
            cosine_term = cosine_term.mul(x2).div_f64(-cosine_denominator);
            cosine = cosine.add(cosine_term);
        }

        let (sine, cosine) = match quadrant.rem_euclid(4) {
            0 => (sine, cosine),
            1 => (cosine, sine.neg()),
            2 => (sine.neg(), cosine.neg()),
            3 => (cosine.neg(), sine),
            _ => unreachable!(),
        };
        (
            ScaledDd {
                value: sine,
                binary_scale: 0,
            },
            ScaledDd {
                value: cosine,
                binary_scale: 0,
            },
        )
    }

    fn x86_x87_tangent(raw: &[u8; 10], control_word: u16) -> TranscendentalResult {
        let info = Self::x86_x87_raw_info(raw);
        if let Some(result) = Self::x86_x87_unary_nonfinite(raw, info, true) {
            return result;
        }
        if info.zero {
            return TranscendentalResult::exact(*raw);
        }
        let (sine, cosine) = Self::x86_x87_sine_cosine_approx(raw);
        if cosine.value.is_zero() {
            return TranscendentalResult {
                raw: Self::x86_x87_infinity(sine.value.is_negative()),
                denormal: info.denormal,
                inexact: true,
                ..TranscendentalResult::exact(Self::x86_x87_infinity(sine.value.is_negative()))
            };
        }
        Self::x86_x87_round_dd(
            sine.value.div(cosine.value),
            sine.binary_scale - cosine.binary_scale,
            control_word,
            true,
            info.denormal,
        )
    }

    fn x86_x87_log2_approx(raw: &[u8; 10]) -> LogApprox {
        let (_, significand, exponent) = Self::x86_x87_finite_parts(raw);
        if significand == 0x8000_0000_0000_0000 {
            return LogApprox {
                scaled: ScaledDd {
                    value: Dd::from_i64(exponent as i64),
                    binary_scale: 0,
                },
                exact_raw: Some(Self::x86_x87_from_i64(exponent as i64)),
            };
        }
        let mantissa = Self::x86_x87_dd_from_significand(significand);
        let logarithm = Self::x86_x87_ln_mantissa(mantissa)
            .div(Self::x86_x87_ln2())
            .add(Dd::from_i64(exponent as i64));
        LogApprox {
            scaled: ScaledDd {
                value: logarithm,
                binary_scale: 0,
            },
            exact_raw: None,
        }
    }

    fn x86_x87_log2p1_approx(raw: &[u8; 10]) -> Result<LogApprox, TranscendentalResult> {
        let (sign, significand, exponent) = Self::x86_x87_finite_parts(raw);
        if exponent > 1_022 {
            // Outside the architecturally defined FYL2XP1 input interval.  A
            // log2(x) profile avoids overflowing the bounded double-double
            // scaling primitive while remaining permitted for this undefined
            // operand interval.
            if sign {
                return Err(TranscendentalResult::invalid(
                    crate::smir::X86X87State::INDEFINITE,
                ));
            }
            return Ok(Self::x86_x87_log2_approx(raw));
        }
        if exponent < -900 {
            let magnitude = Self::x86_x87_dd_from_significand(significand).div(Self::x86_x87_ln2());
            return Ok(LogApprox {
                scaled: ScaledDd {
                    value: if sign { magnitude.neg() } else { magnitude },
                    binary_scale: exponent,
                },
                exact_raw: None,
            });
        }
        let magnitude = Self::x86_x87_dd_from_significand(significand).scale_pow2(exponent);
        let x = if sign { magnitude.neg() } else { magnitude };
        let one_plus = Dd::ONE.add(x);
        if one_plus.is_zero() || one_plus.is_negative() {
            return Err(TranscendentalResult::invalid(
                crate::smir::X86X87State::INDEFINITE,
            ));
        }
        let (mantissa, log_exponent) = Self::x86_x87_normalize_dd(one_plus);
        if mantissa.sub(Dd::ONE).is_zero() {
            return Ok(LogApprox {
                scaled: ScaledDd {
                    value: Dd::from_i64(log_exponent as i64),
                    binary_scale: 0,
                },
                exact_raw: Some(Self::x86_x87_from_i64(log_exponent as i64)),
            });
        }
        let logarithm = Self::x86_x87_ln_mantissa(mantissa)
            .div(Self::x86_x87_ln2())
            .add(Dd::from_i64(log_exponent as i64));
        Ok(LogApprox {
            scaled: ScaledDd {
                value: logarithm,
                binary_scale: 0,
            },
            exact_raw: None,
        })
    }

    fn x86_x87_multiply_log_approx(
        y: &[u8; 10],
        logarithm: LogApprox,
        control_word: u16,
        source_denormal: bool,
    ) -> TranscendentalResult {
        if let Some(raw) = logarithm.exact_raw {
            return TranscendentalResult::from_multiply(Self::x86_x87_multiply(
                y,
                &raw,
                false,
                source_denormal,
                control_word,
            ));
        }

        let y_info = Self::x86_x87_raw_info(y);
        if y_info.unsupported {
            return TranscendentalResult::invalid(crate::smir::X86X87State::INDEFINITE);
        }
        if y_info.nan {
            let raw = if y_info.signaling_nan {
                Self::x86_x87_quiet_nan(y)
            } else {
                *y
            };
            return TranscendentalResult {
                raw,
                invalid: y_info.signaling_nan,
                ..TranscendentalResult::exact(raw)
            };
        }

        let logarithm_sign = logarithm.scaled.value.is_negative();
        if y_info.zero {
            let raw = Self::x86_x87_signed_zero(y_info.sign ^ logarithm_sign);
            return TranscendentalResult {
                raw,
                denormal: source_denormal,
                ..TranscendentalResult::exact(raw)
            };
        }
        if Self::x86_x87_is_infinite(y) {
            let raw = Self::x86_x87_infinity(y_info.sign ^ logarithm_sign);
            return TranscendentalResult {
                raw,
                denormal: source_denormal,
                ..TranscendentalResult::exact(raw)
            };
        }

        let (_, y_significand, y_exponent) = Self::x86_x87_finite_parts(y);
        let mut product = logarithm
            .scaled
            .value
            .mul(Self::x86_x87_dd_from_significand(y_significand));
        if y_info.sign {
            product = product.neg();
        }
        Self::x86_x87_round_dd(
            product,
            logarithm.scaled.binary_scale + y_exponent,
            control_word,
            true,
            source_denormal || y_info.denormal,
        )
    }

    fn x86_x87_ln_mantissa(mantissa: Dd) -> Dd {
        let z = mantissa.sub(Dd::ONE).div(mantissa.add(Dd::ONE));
        let z2 = z.mul(z);
        let mut term = z;
        let mut sum = z;
        for n in 1..=48 {
            term = term.mul(z2);
            sum = sum.add(term.div_f64((2 * n + 1) as f64));
        }
        sum.mul_f64(2.0)
    }

    fn x86_x87_ln2() -> Dd {
        Self::x86_x87_ln_mantissa(Dd::from_f64(2.0))
    }

    fn x86_x87_atan_unit(value: Dd) -> Dd {
        let threshold = Self::x86_x87_sqrt_two_minus_one();
        let (argument, offset) =
            if value.hi > threshold.hi || (value.hi == threshold.hi && value.lo > threshold.lo) {
                (
                    value.sub(Dd::ONE).div(value.add(Dd::ONE)),
                    Self::x86_x87_pi().scale_pow2(-2),
                )
            } else {
                (value, Dd::ZERO)
            };
        let square = argument.mul(argument);
        let mut term = argument;
        let mut sum = argument;
        for n in 1..=64 {
            term = term.mul(square).neg();
            sum = sum.add(term.div_f64((2 * n + 1) as f64));
        }
        offset.add(sum)
    }

    fn x86_x87_sqrt_two_minus_one() -> Dd {
        // Newton iteration starts from binary64 and converges quadratically to
        // the double-double square root; five iterations exceed 100 bits.
        let mut root = Dd::from_f64(std::f64::consts::SQRT_2);
        for _ in 0..5 {
            root = root.add(Dd::from_f64(2.0).div(root)).scale_pow2(-1);
        }
        root.sub(Dd::ONE)
    }

    /// Intel's internal Pi is the exact 68-bit fraction
    /// `0.C90FDAA22168C234C * 2^2`.
    fn x86_x87_pi() -> Dd {
        Dd::from_u64(0xC90F_DAA2_2168_C234)
            .scale_pow2(-62)
            .add(Dd::from_u64(0xC).scale_pow2(-66))
    }

    fn x86_x87_reduce_trig(significand: u64, exponent: i32, sign: bool) -> (Dd, i64) {
        if exponent < -4 {
            let mut value = Self::x86_x87_dd_from_significand(significand).scale_pow2(exponent);
            if sign {
                value = value.neg();
            }
            return (value, 0);
        }

        const PI_OVER_TWO_INTEGER: U192 = U192([0x90FD_AA22_168C_234C, 0x0000_0000_0000_000C, 0]);
        let numerator = U192::from_u64_shift(significand, (exponent + 4) as u32);
        let mut remainder = U192::ZERO;
        let mut quotient = 0u64;
        for bit in (0..numerator.bit_len()).rev() {
            remainder.shl_one();
            if numerator.bit(bit) {
                remainder.0[0] |= 1;
            }
            if remainder >= PI_OVER_TWO_INTEGER {
                remainder = remainder.sub(PI_OVER_TWO_INTEGER);
                assert!(bit < 64, "x87 trigonometric quotient exceeds u64");
                quotient |= 1u64 << bit;
            }
        }

        let mut doubled = remainder;
        doubled.shl_one();
        let increment =
            doubled > PI_OVER_TWO_INTEGER || (doubled == PI_OVER_TWO_INTEGER && quotient & 1 != 0);
        let (remainder, remainder_negative) = if increment {
            (PI_OVER_TWO_INTEGER.sub(remainder), true)
        } else {
            (remainder, false)
        };
        quotient += u64::from(increment);

        let mut reduced = remainder.to_dd_scaled_2_neg_67();
        if remainder_negative {
            reduced = reduced.neg();
        }
        let mut signed_quotient = quotient as i64;
        if sign {
            reduced = reduced.neg();
            signed_quotient = -signed_quotient;
        }
        (reduced, signed_quotient)
    }

    fn x86_x87_round_dd(
        value: Dd,
        binary_scale: i32,
        control_word: u16,
        force_inexact: bool,
        denormal_operand: bool,
    ) -> TranscendentalResult {
        if value.is_zero() {
            return TranscendentalResult {
                denormal: denormal_operand,
                inexact: force_inexact,
                ..TranscendentalResult::exact(Self::x86_x87_signed_zero(false))
            };
        }
        let sign = value.is_negative();
        let (normalized, local_exponent) = Self::x86_x87_normalize_dd(value.abs());
        let exact_exponent = local_exponent + binary_scale;
        let mut rounded_exponent = exact_exponent;

        // Extract 96 normalized bits.  Double-double carries approximately
        // 106 bits, leaving ten guard bits beyond this integer image.
        let mut fraction = normalized;
        let mut fixed = 0u128;
        for _ in 0..96 {
            let bit = fraction.cmp_f64(1.0) != Ordering::Less;
            fixed = (fixed << 1) | u128::from(bit);
            if bit {
                fraction = fraction.sub(Dd::ONE);
            }
            fraction = fraction.mul_f64(2.0);
        }
        let sticky = !fraction.is_zero();
        let precision = match (control_word >> 8) & 3 {
            0 => 24u32,
            2 => 53,
            1 | 3 => 64,
            _ => unreachable!(),
        };
        let rounding = (control_word >> 10) & 3;
        let normal_shift = 96 - precision;
        let (mut rounded, normal_inexact, normal_rounded_up) =
            Self::x86_x87_round_fixed(fixed, normal_shift, sticky, rounding, sign);
        if rounded == 1u128 << precision {
            rounded >>= 1;
            rounded_exponent += 1;
        }
        let normal_significand = (rounded as u64) << (64 - precision);
        let inexact = force_inexact || normal_inexact;

        if rounded_exponent > 16_383 {
            let overflow_masked = control_word & 0x0008 != 0;
            let infinity = match rounding {
                0 => true,
                1 => sign,
                2 => !sign,
                3 => false,
                _ => unreachable!(),
            };
            let raw = if !overflow_masked {
                Self::x86_x87_from_raw_parts(
                    normal_significand,
                    ((rounded_exponent - 24_576 + 16_383) as u16) | ((sign as u16) << 15),
                )
            } else if infinity {
                Self::x86_x87_infinity(sign)
            } else {
                Self::x86_x87_from_raw_parts(
                    u64::MAX << (64 - precision),
                    0x7FFE | ((sign as u16) << 15),
                )
            };
            return TranscendentalResult {
                raw,
                denormal: denormal_operand,
                overflow: true,
                inexact: true,
                rounded_up: if overflow_masked {
                    infinity
                } else {
                    normal_rounded_up
                },
                ..TranscendentalResult::exact(raw)
            };
        }

        if exact_exponent < -16_382 {
            let denormal_shift = normal_shift + (-16_382 - exact_exponent) as u32;
            let (denormal_rounded, denormal_inexact, denormal_rounded_up) =
                Self::x86_x87_round_fixed(fixed, denormal_shift, sticky, rounding, sign);
            // `round_fixed` already handles shifts beyond the 128-bit image:
            // directed rounding can still produce one destination quantum.
            // Preserve that increment instead of collapsing every such result
            // to signed zero.
            let denormal_significand = (denormal_rounded << (64 - precision)) as u64;
            let underflow = force_inexact || denormal_inexact;
            let underflow_masked = control_word & 0x0010 != 0;
            let raw = if !underflow || underflow_masked {
                if denormal_significand == 1u64 << 63 {
                    Self::x86_x87_from_raw_parts(denormal_significand, 1 | ((sign as u16) << 15))
                } else {
                    Self::x86_x87_from_raw_parts(denormal_significand, (sign as u16) << 15)
                }
            } else {
                let biased = rounded_exponent + 24_576;
                if biased >= -16_382 {
                    Self::x86_x87_from_raw_parts(
                        normal_significand,
                        (biased + 16_383) as u16 | ((sign as u16) << 15),
                    )
                } else {
                    Self::x86_x87_signed_zero(sign)
                }
            };
            return TranscendentalResult {
                raw,
                denormal: denormal_operand,
                underflow,
                inexact: underflow,
                rounded_up: if underflow_masked {
                    denormal_rounded_up
                } else {
                    normal_rounded_up
                },
                ..TranscendentalResult::exact(raw)
            };
        }

        let raw = Self::x86_x87_from_raw_parts(
            normal_significand,
            (rounded_exponent + 16_383) as u16 | ((sign as u16) << 15),
        );
        TranscendentalResult {
            raw,
            denormal: denormal_operand,
            inexact,
            rounded_up: normal_rounded_up,
            ..TranscendentalResult::exact(raw)
        }
    }

    fn x86_x87_round_fixed(
        value: u128,
        shift: u32,
        sticky: bool,
        rounding: u16,
        sign: bool,
    ) -> (u128, bool, bool) {
        let (truncated, remainder, half_cmp) = if shift == 0 {
            (value, 0, Ordering::Less)
        } else if shift >= 128 {
            let half_cmp = if shift == 128 {
                value.cmp(&(1u128 << 127))
            } else {
                Ordering::Less
            };
            (0, value, half_cmp)
        } else {
            let remainder = value & ((1u128 << shift) - 1);
            let half = 1u128 << (shift - 1);
            let mut comparison = remainder.cmp(&half);
            if comparison == Ordering::Equal && sticky {
                comparison = Ordering::Greater;
            }
            (value >> shift, remainder, comparison)
        };
        let inexact = remainder != 0 || sticky;
        let increment = inexact
            && match rounding & 3 {
                0 => {
                    half_cmp == Ordering::Greater
                        || (half_cmp == Ordering::Equal && truncated & 1 != 0)
                }
                1 => sign,
                2 => !sign,
                3 => false,
                _ => unreachable!(),
            };
        (truncated + u128::from(increment), inexact, increment)
    }

    fn x86_x87_normalize_dd(mut value: Dd) -> (Dd, i32) {
        debug_assert!(!value.is_negative() && !value.is_zero());
        let bits = value.hi.to_bits();
        let field = ((bits >> 52) & 0x7FF) as i32;
        let mut exponent = if field == 0 { -1022 } else { field - 1023 };
        value = value.scale_pow2(-exponent);
        if value.cmp_f64(1.0) == Ordering::Less {
            value = value.mul_f64(2.0);
            exponent -= 1;
        } else if value.cmp_f64(2.0) != Ordering::Less {
            value = value.mul_f64(0.5);
            exponent += 1;
        }
        (value, exponent)
    }

    fn x86_x87_dd_from_significand(significand: u64) -> Dd {
        Dd::from_u64(significand).scale_pow2(-63)
    }

    fn x86_x87_finite_parts(raw: &[u8; 10]) -> (bool, u64, i32) {
        let info = Self::x86_x87_raw_info(raw);
        let significand = u64::from_le_bytes(raw[..8].try_into().unwrap());
        let exponent = u16::from_le_bytes(raw[8..].try_into().unwrap()) & 0x7FFF;
        if exponent == 0 {
            let highest = 63 - significand.leading_zeros();
            (
                info.sign,
                significand << (63 - highest),
                highest as i32 - 16_445,
            )
        } else {
            (info.sign, significand, exponent as i32 - 16_383)
        }
    }

    fn x86_x87_unary_nonfinite(
        raw: &[u8; 10],
        info: X87RawInfo,
        infinity_invalid: bool,
    ) -> Option<TranscendentalResult> {
        if info.unsupported {
            return Some(TranscendentalResult::invalid(
                crate::smir::X86X87State::INDEFINITE,
            ));
        }
        if info.nan {
            let output = if info.signaling_nan {
                Self::x86_x87_quiet_nan(raw)
            } else {
                *raw
            };
            return Some(TranscendentalResult {
                raw: output,
                invalid: info.signaling_nan,
                ..TranscendentalResult::exact(output)
            });
        }
        if infinity_invalid && Self::x86_x87_is_infinite(raw) {
            return Some(TranscendentalResult::invalid(
                crate::smir::X86X87State::INDEFINITE,
            ));
        }
        None
    }

    fn x86_x87_binary_nan(
        source: &[u8; 10],
        destination: &[u8; 10],
        source_info: X87RawInfo,
        destination_info: X87RawInfo,
    ) -> ([u8; 10], bool) {
        let raw = if destination_info.signaling_nan {
            Self::x86_x87_quiet_nan(destination)
        } else if source_info.signaling_nan {
            Self::x86_x87_quiet_nan(source)
        } else if destination_info.nan {
            *destination
        } else {
            *source
        };
        (
            raw,
            source_info.signaling_nan || destination_info.signaling_nan,
        )
    }

    fn x86_x87_is_infinite(raw: &[u8; 10]) -> bool {
        u16::from_le_bytes(raw[8..].try_into().unwrap()) & 0x7FFF == 0x7FFF
            && u64::from_le_bytes(raw[..8].try_into().unwrap()) == 0x8000_0000_0000_0000
    }

    fn x86_x87_trig_out_of_range(raw: &[u8; 10]) -> bool {
        if Self::x86_x87_raw_info(raw).zero {
            return false;
        }
        let (_, _, exponent) = Self::x86_x87_finite_parts(raw);
        exponent >= 63
    }

    fn x86_x87_infinity(sign: bool) -> [u8; 10] {
        Self::x86_x87_from_raw_parts(0x8000_0000_0000_0000, 0x7FFF | ((sign as u16) << 15))
    }

    fn x86_x87_signed_zero(sign: bool) -> [u8; 10] {
        Self::x86_x87_from_raw_parts(0, (sign as u16) << 15)
    }
}
