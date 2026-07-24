//! Canonical AVX10.2 saturating floating-point conversion shapes.

use crate::smir::ir::types::{VecElementType, VecWidth};

/// Return the encoded EVEX vector length for a canonical packed saturating
/// conversion shape from its destination payload width. The returned pair is
/// `(source payload width, encoded EVEX vector length)`; a 64-bit payload still
/// resides in an XMM register.
///
/// Returning `None` keeps malformed or unassigned SMIR fail-closed. The shape
/// table is derived from Intel AVX10.2 Architecture Specification revision 7,
/// Chapter 12: FP64-to-I32 narrows, FP32-to-I64 widens, and all other admitted
/// families retain their payload width. Only the FP32-to-I8 family has both
/// truncating and MXCSR/embedded-rounding variants.
pub(crate) const fn x86_sat_fp_to_int_widths(
    fp_elem: VecElementType,
    int_elem: VecElementType,
    dst_width: VecWidth,
    truncate: bool,
) -> Option<(VecWidth, VecWidth)> {
    use VecElementType::{F32, F64, I8, I32, I64};
    use VecWidth::{V64, V128, V256, V512};

    match (fp_elem, int_elem, dst_width, truncate) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use VecElementType::{F32, F64, I8, I32, I64};
    use VecWidth::{V128, V256, V512};

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
    }
}
