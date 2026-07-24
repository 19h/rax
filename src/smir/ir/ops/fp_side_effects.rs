//! Floating-point status/trap side-effect classification.

use super::OpKind;
use crate::smir::ir::types::{FpRoundMode, OpWidth, VecElementType};

impl OpKind {
    /// Return whether execution can update architectural floating-point status
    /// or request a floating-point exception independently of the data result.
    pub(super) fn has_fp_status_side_effects(&self) -> bool {
        if let OpKind::X86Fma(fma) = self {
            // Malformed IR must remain observable so DCE cannot erase the
            // interpreter's fail-closed #UD boundary. Canonical embedded-
            // rounding forms suppress all status/trap effects.
            return !fma.shape_valid() || fma.round == FpRoundMode::Dynamic;
        }

        if let OpKind::X86IntToFp {
            elem,
            int_width,
            round,
            suppress_exceptions,
            ..
        } = self
        {
            let canonical = matches!(
                elem,
                VecElementType::F16 | VecElementType::F32 | VecElementType::F64
            ) && matches!(int_width, OpWidth::W32 | OpWidth::W64)
                && *round != FpRoundMode::RoundNearestTiesAway;
            // Every signed or unsigned 32-bit integer is exactly representable
            // in binary64's 53-bit precision; all other admitted shapes can
            // set Precision, and binary16 can additionally set Overflow.
            let always_exact = *elem == VecElementType::F64 && *int_width == OpWidth::W32;
            return !canonical || (!*suppress_exceptions && !always_exact);
        }

        if let OpKind::X86FpToInt {
            elem,
            int_width,
            truncate,
            round,
            suppress_exceptions,
            ..
        } = self
        {
            let canonical = matches!(
                elem,
                VecElementType::F16 | VecElementType::F32 | VecElementType::F64
            ) && matches!(int_width, OpWidth::W32 | OpWidth::W64)
                && *round != FpRoundMode::RoundNearestTiesAway
                && (!*truncate || *round == FpRoundMode::RoundTowardZero);
            return !*suppress_exceptions || !canonical;
        }

        if let OpKind::X86FpConvert {
            from,
            to,
            round,
            suppress_exceptions,
            ..
        } = self
        {
            let canonical = matches!(
                (from, to),
                (VecElementType::F16, VecElementType::F32)
                    | (VecElementType::F16, VecElementType::F64)
                    | (VecElementType::F32, VecElementType::F16)
                    | (VecElementType::F32, VecElementType::F64)
                    | (VecElementType::F64, VecElementType::F16)
                    | (VecElementType::F64, VecElementType::F32)
            ) && *round != FpRoundMode::RoundNearestTiesAway;
            return !canonical || !*suppress_exceptions;
        }

        matches!(
            self,
            OpKind::X86Round { .. }
                | OpKind::X86DotProduct { .. }
                | OpKind::X86FpBinary {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86FpCompare {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86VectorFpCompare { .. }
                | OpKind::X86GetExponent {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86GetMantissa {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86RoundScale {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86Reduce {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86Range {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86Exp2 {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86Recip28 {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86Rsqrt28 {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86ScaleF {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86Sqrt {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86PackedFpConvert {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86PackedIntToFp {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86PackedFpToInt {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86PackedIntToFp {
                    round: FpRoundMode::RoundNearestTiesAway,
                    ..
                }
                | OpKind::X86PackedFpToInt {
                    round: FpRoundMode::RoundNearestTiesAway,
                    ..
                }
                | OpKind::X86PackedIntToFp16 {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86PackedFp16ToInt {
                    suppress_exceptions: false,
                    ..
                }
                | OpKind::X86PackedIntToFp16 {
                    round: FpRoundMode::RoundNearestTiesAway,
                    ..
                }
                | OpKind::X86PackedFp16ToInt {
                    round: FpRoundMode::RoundNearestTiesAway,
                    ..
                }
                | OpKind::VFP16Arith {
                    round: FpRoundMode::Dynamic,
                    ..
                }
                | OpKind::X86FP16Fma {
                    round: FpRoundMode::Dynamic,
                    ..
                }
                | OpKind::X86FP16Complex {
                    round: FpRoundMode::Dynamic,
                    ..
                }
                | OpKind::X86FourFma { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::types::{ArchReg, OpWidth, VReg, VecElementType, X86Reg};

    fn scalar_conversion(suppress_exceptions: bool) -> OpKind {
        OpKind::X86FpToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            elem: VecElementType::F32,
            int_width: OpWidth::W32,
            signed: true,
            truncate: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions,
        }
    }

    fn scalar_int_to_fp(
        elem: VecElementType,
        int_width: OpWidth,
        suppress_exceptions: bool,
    ) -> OpKind {
        OpKind::X86IntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            elem,
            int_width,
            signed: true,
            round: FpRoundMode::Dynamic,
            suppress_exceptions,
            zero_upper: true,
        }
    }

    fn scalar_fp_convert(
        from: VecElementType,
        to: VecElementType,
        suppress_exceptions: bool,
    ) -> OpKind {
        OpKind::X86FpConvert {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
            mask: None,
            from,
            to,
            mask_zeroing: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions,
            zero_upper: true,
        }
    }

    #[test]
    fn scalar_fp_to_int_is_side_effecting_exactly_without_sae() {
        assert!(scalar_conversion(false).has_side_effects());
        assert!(!scalar_conversion(true).has_side_effects());

        let mut invalid = scalar_conversion(true);
        let OpKind::X86FpToInt { round, .. } = &mut invalid else {
            unreachable!()
        };
        *round = FpRoundMode::RoundNearestTiesAway;
        assert!(invalid.has_side_effects(), "invalid IR must fail closed");
    }

    #[test]
    fn scalar_int_to_fp_classifies_status_exactness_and_invalid_ir() {
        assert!(scalar_int_to_fp(VecElementType::F32, OpWidth::W64, false).has_side_effects());
        assert!(scalar_int_to_fp(VecElementType::F16, OpWidth::W32, false).has_side_effects());
        assert!(!scalar_int_to_fp(VecElementType::F32, OpWidth::W64, true).has_side_effects());
        assert!(
            !scalar_int_to_fp(VecElementType::F64, OpWidth::W32, false).has_side_effects(),
            "binary64 exactly represents the complete 32-bit integer domain"
        );

        let mut invalid = scalar_int_to_fp(VecElementType::F64, OpWidth::W32, true);
        let OpKind::X86IntToFp { round, .. } = &mut invalid else {
            unreachable!()
        };
        *round = FpRoundMode::RoundNearestTiesAway;
        assert!(invalid.has_side_effects(), "invalid IR must fail closed");
    }

    #[test]
    fn scalar_fp_convert_classifies_status_sae_and_invalid_ir() {
        assert!(
            scalar_fp_convert(VecElementType::F16, VecElementType::F64, false).has_side_effects()
        );
        assert!(
            scalar_fp_convert(VecElementType::F64, VecElementType::F16, false).has_side_effects()
        );
        assert!(
            !scalar_fp_convert(VecElementType::F64, VecElementType::F32, true).has_side_effects()
        );

        let invalid_same = scalar_fp_convert(VecElementType::F32, VecElementType::F32, true);
        assert!(
            invalid_same.has_side_effects(),
            "invalid IR must fail closed"
        );
        let mut invalid_round = scalar_fp_convert(VecElementType::F64, VecElementType::F32, true);
        let OpKind::X86FpConvert { round, .. } = &mut invalid_round else {
            unreachable!()
        };
        *round = FpRoundMode::RoundNearestTiesAway;
        assert!(
            invalid_round.has_side_effects(),
            "invalid IR must fail closed"
        );
    }
}
