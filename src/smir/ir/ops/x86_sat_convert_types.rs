//! Canonical AVX10.2 saturating floating-point conversion shapes.

use crate::smir::ir::types::{FpRoundMode, VecElementType, VecWidth};

/// Source floating-point format for x86 AVX10.2 saturating conversions.
///
/// BF16 is intentionally distinct from IEEE binary16. `VecElementType` has no
/// BF16 variant because most generic vector operations cannot interpret BF16
/// lanes; these conversions are one of the instruction families that can.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum X86SatFpFormat {
    F16,
    BF16,
    F32,
    F64,
}

impl X86SatFpFormat {
    /// Source element width in bytes.
    pub const fn bytes(self) -> u32 {
        match self {
            Self::F16 | Self::BF16 => 2,
            Self::F32 => 4,
            Self::F64 => 8,
        }
    }

    /// Raw lane type used to materialize register and memory operands.
    pub(crate) const fn memory_elem(self) -> VecElementType {
        match self {
            Self::F16 => VecElementType::F16,
            Self::BF16 => VecElementType::I16,
            Self::F32 => VecElementType::F32,
            Self::F64 => VecElementType::F64,
        }
    }
}

/// Return the encoded EVEX vector length for a canonical packed saturating
/// conversion shape from its destination payload width. The returned pair is
/// `(source payload width, encoded EVEX vector length)`; a 64-bit payload still
/// resides in an XMM register.
///
/// Returning `None` keeps malformed or unassigned SMIR fail-closed. The shape
/// table is derived from Intel AVX10.2 Architecture Specification revision 7,
/// Chapter 12: FP64-to-I32 narrows, FP32-to-I64 widens, and all other admitted
/// families retain their payload width. The FP16-to-I8 and FP32-to-I8 families
/// have both truncating and MXCSR/embedded-rounding variants. BF16-to-I8 uses
/// fixed RNE or RTZ and never consults MXCSR.
pub(crate) const fn x86_sat_fp_to_int_widths(
    fp_format: X86SatFpFormat,
    int_elem: VecElementType,
    dst_width: VecWidth,
    truncate: bool,
) -> Option<(VecWidth, VecWidth)> {
    use VecElementType::{I8, I32, I64};
    use VecWidth::{V64, V128, V256, V512};
    use X86SatFpFormat::{BF16, F16, F32, F64};

    match (fp_format, int_elem, dst_width, truncate) {
        (F16 | BF16, I8, V128, _) => Some((V128, V128)),
        (F16 | BF16, I8, V256, _) => Some((V256, V256)),
        (F16 | BF16, I8, V512, _) => Some((V512, V512)),
        (F32, I8, V128, _) | (F32, I32, V128, true) | (F64, I64, V128, true) => Some((V128, V128)),
        (F32, I8, V256, _) | (F32, I32, V256, true) | (F64, I64, V256, true) => Some((V256, V256)),
        (F32, I8, V512, _) | (F32, I32, V512, true) | (F64, I64, V512, true) => Some((V512, V512)),
        (F32, I64, V128, true) => Some((V64, V128)),
        (F32, I64, V256, true) => Some((V128, V256)),
        (F32, I64, V512, true) => Some((V256, V512)),
        (F64, I32, V64, true) => Some((V128, V128)),
        (F64, I32, V128, true) => Some((V256, V256)),
        (F64, I32, V256, true) => Some((V512, V512)),
        _ => None,
    }
}

/// Validate the architectural rounding/exception-control shape.
pub(crate) fn x86_sat_fp_to_int_controls(
    fp_format: X86SatFpFormat,
    truncate: bool,
    round: FpRoundMode,
    suppress_exceptions: bool,
    encoded_width: VecWidth,
) -> bool {
    if fp_format == X86SatFpFormat::BF16 {
        return !suppress_exceptions
            && if truncate {
                round == FpRoundMode::RoundTowardZero
            } else {
                round == FpRoundMode::RoundNearest
            };
    }

    if truncate {
        round == FpRoundMode::RoundTowardZero
            && (!suppress_exceptions || encoded_width == VecWidth::V512)
    } else {
        matches!((round, suppress_exceptions), (FpRoundMode::Dynamic, false))
            || (round != FpRoundMode::Dynamic
                && round != FpRoundMode::RoundNearestTiesAway
                && suppress_exceptions
                && encoded_width == VecWidth::V512)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use VecElementType::{I8, I32, I64};
    use VecWidth::{V128, V256, V512};
    use X86SatFpFormat::{BF16, F16, F32, F64};

    #[test]
    fn canonical_shapes_cover_equal_narrowing_and_widening_payloads() {
        for (fp, int, dst, src, encoded) in [
            (F32, I32, V128, V128, V128),
            (F64, I32, V128, V256, V256),
            (F32, I64, V512, V256, V512),
            (F64, I64, V512, V512, V512),
        ] {
            assert_eq!(
                x86_sat_fp_to_int_widths(fp, int, dst, true),
                Some((src, encoded))
            );
        }
        assert_eq!(
            x86_sat_fp_to_int_widths(F32, I8, V256, false),
            Some((V256, V256))
        );
        assert_eq!(x86_sat_fp_to_int_widths(F64, I32, V512, true), None,);
        assert_eq!(x86_sat_fp_to_int_widths(F32, I64, V256, false), None);
        assert_eq!(
            x86_sat_fp_to_int_widths(F16, I8, V512, false),
            Some((V512, V512))
        );
        assert_eq!(
            x86_sat_fp_to_int_widths(BF16, I8, V256, true),
            Some((V256, V256))
        );
    }

    #[test]
    fn canonical_controls_distinguish_bf16_from_ieee_formats() {
        assert!(x86_sat_fp_to_int_controls(
            BF16,
            false,
            FpRoundMode::RoundNearest,
            false,
            V128
        ));
        assert!(!x86_sat_fp_to_int_controls(
            BF16,
            false,
            FpRoundMode::Dynamic,
            false,
            V128
        ));
        assert!(!x86_sat_fp_to_int_controls(
            BF16,
            true,
            FpRoundMode::RoundTowardZero,
            true,
            V512
        ));
        assert!(x86_sat_fp_to_int_controls(
            F16,
            false,
            FpRoundMode::Dynamic,
            false,
            V128
        ));
        assert!(x86_sat_fp_to_int_controls(
            F16,
            false,
            FpRoundMode::RoundUp,
            true,
            V512
        ));
    }
}
