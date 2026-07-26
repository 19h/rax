//! Exact x86 AVX10.2 saturating floating-point-to-integer conversions.

use super::*;

impl SmirInterpreter {
    /// Convert one x86 SIMD floating-point lane to a saturated integer.
    /// Invalid conversions retain IE but replace the ordinary indefinite
    /// result with the closest endpoint; NaNs map to 0. The FP16/F32-to-I8
    /// families use AVX10.2's pre-round representability thresholds, which
    /// intentionally differ from a generic conversion followed by clamping near
    /// 0 and 255.
    pub(crate) fn x86_simd_fp_to_int_sat(
        bits: u64,
        format: X86SimdFpFormat,
        int_bits: u32,
        signed: bool,
        mode: FpRoundMode,
    ) -> X86SimdFpResult {
        debug_assert!(!matches!(
            mode,
            FpRoundMode::Dynamic | FpRoundMode::RoundNearestTiesAway
        ));

        if matches!(format.total_bits, 16 | 32)
            && int_bits == 8
            && !Self::x86_simd_fp_is_nan(bits, format)
            && !Self::x86_simd_fp_is_infinite(bits, format)
        {
            let source = if format.total_bits == 16 {
                Self::x86_fp16_to_f32(bits as u16)
            } else {
                f32::from_bits(bits as u32)
            };
            let out_of_range = if signed {
                match mode {
                    FpRoundMode::RoundNearest => source < -128.5 || source >= 127.5,
                    FpRoundMode::RoundDown => source < -128.0 || source >= 128.0,
                    FpRoundMode::RoundUp => source <= -129.0 || source > 127.0,
                    FpRoundMode::RoundTowardZero => source <= -129.0 || source >= 128.0,
                    _ => unreachable!("rounding mode validated above"),
                }
            } else if format.total_bits == 16 {
                match mode {
                    FpRoundMode::RoundNearest => source < -0.5 || source >= 255.5,
                    FpRoundMode::RoundDown => source < 0.0 || source >= 256.0,
                    FpRoundMode::RoundUp => source <= -1.0 || source > 255.0,
                    FpRoundMode::RoundTowardZero => source <= -1.0 || source >= 256.0,
                    _ => unreachable!("rounding mode validated above"),
                }
            } else {
                source <= -1.0 || source >= 256.0
            };
            if out_of_range {
                let saturated = if signed {
                    if source.is_sign_negative() {
                        0x80
                    } else {
                        0x7F
                    }
                } else if source.is_sign_negative() {
                    0
                } else {
                    0xFF
                };
                return X86SimdFpResult {
                    bits: saturated,
                    status: 1,
                };
            }

            let saturated_without_invalid = if signed {
                (source >= 127.0)
                    .then_some(0x7F)
                    .or_else(|| if source <= -128.0 { Some(0x80) } else { None })
            } else if source > 255.0 {
                Some(0xFF)
            } else if source < 0.0 {
                Some(0)
            } else {
                None
            };
            if let Some(result) = saturated_without_invalid {
                let exact = if signed && result == 0x80 {
                    source == -128.0
                } else {
                    source == result as f32
                };
                return X86SimdFpResult {
                    bits: result,
                    status: if exact { 0 } else { 1 << 5 },
                };
            }
        }

        let converted = Self::x86_simd_fp_to_int(bits, format, int_bits, signed, mode);
        if converted.status & 1 == 0 {
            return converted;
        }

        let saturated = if Self::x86_simd_fp_is_nan(bits, format) {
            0
        } else {
            let (sign_mask, _, _, _) = Self::x86_simd_fp_masks(format);
            let negative = bits & sign_mask != 0;
            match (signed, negative, int_bits) {
                (true, true, 64) => 1u64 << 63,
                (true, true, _) => 1u64 << (int_bits - 1),
                (true, false, 64) => i64::MAX as u64,
                (true, false, _) => (1u64 << (int_bits - 1)) - 1,
                (false, true, _) => 0,
                (false, false, 64) => u64::MAX,
                (false, false, _) => (1u64 << int_bits) - 1,
            }
        };
        X86SimdFpResult {
            bits: saturated,
            // Invalid conversions do not additionally report Precision.
            status: 1,
        }
    }
}
