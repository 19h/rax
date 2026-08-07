//! Register-only x86 floating-point comparison replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{ArchReg, VReg, VecElementType, X86Reg};

/// Decoded operands and semantic controls of one canonical register-only
/// legacy `COMISS`, `UCOMISS`, `COMISD`, or `UCOMISD` instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyFpFlagCompareReplay {
    pub(crate) first: u8,
    pub(crate) second: u8,
    pub(crate) element: VecElementType,
    pub(crate) signaling: bool,
}

fn is_xmm(register: VReg, expected: u8) -> bool {
    matches!(
        register,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(actual))) if actual == expected
    )
}

/// Validate the complete one-operation SMIR graph emitted for a legacy scalar
/// flag compare. Exact source replay must not replace a fabricated compare
/// whose operands, element type, exception policy, or encoding provenance do
/// not match the captured instruction.
pub(crate) fn x86_legacy_fp_flag_compare_shape_matches(
    ops: &[SmirOp],
    replay: X86LegacyFpFlagCompareReplay,
) -> bool {
    let [compare] = ops else {
        return false;
    };
    let expected_prefix = if replay.element == VecElementType::F64 {
        X86SsePrefix::OpSize
    } else {
        X86SsePrefix::None
    };
    compare.x86_hint
        == Some(X86OpHint::SseOp {
            prefix: expected_prefix,
            opcode: if replay.signaling { 0x2F } else { 0x2E },
        })
        && matches!(
            compare.kind,
            OpKind::X86FpCompare {
                src1,
                src2,
                elem,
                signaling,
                suppress_exceptions: false,
            } if is_xmm(src1, replay.first)
                && is_xmm(src2, replay.second)
                && elem == replay.element
                && signaling == replay.signaling
        )
}

#[inline]
fn evex_llig_sae_control_is_valid(p2: u8) -> bool {
    let ll = (p2 >> 5) & 0x03;
    let suppress_exceptions = p2 & 0x10 != 0;
    suppress_exceptions || ll != 3
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy `COMISS`, `UCOMISS`,
    /// `COMISD`, or `UCOMISD` instruction.
    ///
    /// Binary32 uses no mandatory prefix; binary64 requires 66H. An optional
    /// REX prefix must be final, immediately before 0FH. REX.R/B extend the
    /// two XMM operands while REX.W/X are ignored but retained in the exact
    /// source-byte universe. Memory, other or duplicate/reordered prefixes,
    /// VEX/EVEX, REX2, truncated instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_fp_flag_compare_replay(
        &self,
    ) -> Option<X86LegacyFpFlagCompareReplay> {
        let (element, rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (VecElementType::F64, Some(*rex), tail),
            [0x66, tail @ ..] => (VecElementType::F64, None, tail),
            [rex @ 0x40..=0x4F, tail @ ..] => (VecElementType::F32, Some(*rex), tail),
            tail => (VecElementType::F32, None, tail),
        };
        let [0x0F, opcode @ (0x2E | 0x2F), modrm] = tail else {
            return None;
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let extension = rex.unwrap_or(0);
        Some(X86LegacyFpFlagCompareReplay {
            first: ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
            second: (modrm & 7) | ((extension & 0x01) << 3),
            element,
            signaling: *opcode == 0x2F,
        })
    }

    /// Validate one register-only AVX VEX `VCOMISS`, `VUCOMISS`, `VCOMISD`,
    /// or `VUCOMISD` instruction.
    ///
    /// The defined deterministic replay surface requires map 0F,
    /// `VEX.vvvv=1111b`, `VEX.L=0`, and `pp=NP/66` for binary32/binary64.
    /// VEX.W and VEX.X are ignored but retained in the exact source-byte
    /// universe. Intel documents `VEX.L=1` as generation-dependent
    /// unpredictable behavior, so those encodings and all memory forms remain
    /// at the precise interpreter frontier.
    pub fn is_vex_register_fp_flag_compare(&self) -> bool {
        let (p1, opcode, modrm) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return false,
        };
        p1 & 0x7E == 0x78 && matches!(opcode, 0x2E | 0x2F) && modrm >> 6 == 3
    }

    /// Validate one register-only legacy SSE or AVX VEX `CMPPS`, `CMPPD`,
    /// `CMPSS`, or `CMPSD` instruction and report whether it requires AVX.
    ///
    /// Legacy forms admit predicates 0 through 7; VEX forms admit predicates
    /// 0 through 31. The remaining immediate bits are reserved. Canonical
    /// legacy mandatory-prefix placement and an optional final REX prefix are
    /// accepted. VEX map 0F accepts both C5 and C4 encodings and treats W as
    /// ignored. Scalar `VEX.L=1` is excluded because Intel documents
    /// generation-dependent unpredictable behavior for those encodings.
    /// Memory operands and every non-canonical or reserved byte shape fail
    /// closed.
    pub fn legacy_vex_register_fp_compare_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy = match bytes {
            [0x0F, 0xC2, modrm, immediate] => Some((*modrm, *immediate)),
            [0x66 | 0xF2 | 0xF3, 0x0F, 0xC2, modrm, immediate] => Some((*modrm, *immediate)),
            [0x40..=0x4F, 0x0F, 0xC2, modrm, immediate] => Some((*modrm, *immediate)),
            [
                0x66 | 0xF2 | 0xF3,
                0x40..=0x4F,
                0x0F,
                0xC2,
                modrm,
                immediate,
            ] => Some((*modrm, *immediate)),
            _ => None,
        };
        if let Some((modrm, immediate)) = legacy {
            return (modrm >> 6 == 3 && immediate & !0x07 == 0).then_some(false);
        }

        let (p1, opcode, modrm, immediate) = match bytes {
            [0xC5, p1, opcode, modrm, immediate] => (*p1, *opcode, *modrm, *immediate),
            [0xC4, p0, p1, opcode, modrm, immediate] if p0 & 0x1F == 1 => {
                (*p1, *opcode, *modrm, *immediate)
            }
            _ => return None,
        };
        let scalar_l1 = matches!(p1 & 0x03, 2 | 3) && p1 & 0x04 != 0;
        (opcode == 0xC2 && modrm >> 6 == 3 && immediate & !0x1F == 0 && !scalar_l1).then_some(true)
    }

    /// Validate one register-only EVEX `VCOMISH` or `VUCOMISH` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Both instructions are
    /// scalar LLIG forms, require AVX-512-FP16 but not AVX-512VL, and admit
    /// SAE through EVEX.b. Without SAE, the three defined EVEX.L'L values are
    /// ignored and `11b` is reserved; with SAE, all four control values are
    /// valid. They reserve EVEX.vvvv/V'/z/aaa and reject memory forms.
    pub fn evex_register_fp16_flag_compare_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 5
            || p1 != 0x7C
            || p2 & 0x8F != 0x08
            || !evex_llig_sae_control_is_valid(p2)
            || !matches!(opcode, 0x2E | 0x2F)
            || modrm >> 6 != 3
        {
            return None;
        }
        Some((false, true))
    }

    /// Validate one register-only EVEX `VCOMISS`, `VCOMISD`, `VUCOMISS`, or
    /// `VUCOMISD` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`, which is always
    /// `(false, false)`: these scalar LLIG forms require AVX-512F but neither
    /// AVX-512VL nor AVX-512-FP16. EVEX.b selects SAE. Without SAE, the three
    /// defined EVEX.L'L values are ignored and `11b` is reserved; with SAE,
    /// all four control values are valid. EVEX.vvvv/V'/z/aaa are reserved;
    /// all memory forms fail closed.
    pub fn evex_register_fp32_fp64_flag_compare_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 1
            || !matches!(p1, 0x7C | 0xFD)
            || p2 & 0x8F != 0x08
            || !evex_llig_sae_control_is_valid(p2)
            || !matches!(opcode, 0x2E | 0x2F)
            || modrm >> 6 != 3
        {
            return None;
        }
        Some((false, false))
    }

    /// Validate one register-only EVEX `VCMPPS`, `VCMPPD`, `VCMPSS`,
    /// `VCMPSD`, `VCMPPH`, or `VCMPSH` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Packed 128-bit and
    /// 256-bit forms require AVX-512VL. Register-source packed `EVEX.b=1`
    /// selects the 512-bit SAE form and ignores all four `L'L` values; scalar
    /// forms are LLIG and never require AVX-512VL. For both packed and scalar
    /// forms, no-SAE `L'L=11b` is reserved, while SAE admits all four control
    /// values. Binary16 forms require AVX-512-FP16. The destination must use
    /// the canonical K0-K7 encoding, EVEX.z and immediate bits 7:5 are
    /// reserved, and every memory form fails closed.
    pub fn evex_register_fp_compare_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let immediate = bytes[6];

        if p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || p2 & 0x80 != 0
            || opcode != 0xC2
            || modrm >> 6 != 3
            || immediate & !0x1F != 0
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (scalar, needs_fp16) = match (map, pp, w) {
            // VCMPPS, VCMPPD, VCMPSS, and VCMPSD.
            (1, 0, false) | (1, 1, true) => (false, false),
            (1, 2, false) | (1, 3, true) => (true, false),
            // VCMPPH and VCMPSH.
            (3, 0, false) => (false, true),
            (3, 2, false) => (true, true),
            _ => return None,
        };

        let ll = (p2 >> 5) & 0x03;
        let suppress_exceptions = p2 & 0x10 != 0;
        if scalar {
            return evex_llig_sae_control_is_valid(p2).then_some((false, needs_fp16));
        }
        if suppress_exceptions {
            // Packed register-source SAE has implied VL=512 and ignores L'L.
            return Some((false, needs_fp16));
        }
        match ll {
            0 | 1 => Some((true, needs_fp16)),
            2 => Some((false, needs_fp16)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding(element: VecElementType, opcode: u8, rex: Option<u8>, modrm: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        if element == VecElementType::F64 {
            bytes.push(0x66);
        }
        bytes.extend(rex);
        bytes.extend([0x0F, opcode, modrm]);
        bytes
    }

    fn expected(
        element: VecElementType,
        opcode: u8,
        rex: Option<u8>,
        modrm: u8,
    ) -> Option<X86LegacyFpFlagCompareReplay> {
        (modrm >> 6 == 3).then(|| {
            let extension = rex.unwrap_or(0);
            X86LegacyFpFlagCompareReplay {
                first: ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
                second: (modrm & 7) | ((extension & 0x01) << 3),
                element,
                signaling: opcode == 0x2F,
            }
        })
    }

    #[test]
    fn legacy_classifier_exhaustively_accepts_4352_canonical_register_encodings() {
        let mut classified = 0usize;
        for element in [VecElementType::F32, VecElementType::F64] {
            for opcode in [0x2Eu8, 0x2F] {
                for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
                    for modrm in u8::MIN..=u8::MAX {
                        let bytes = encoding(element, opcode, rex, modrm);
                        let actual = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_register_fp_flag_compare_replay();
                        assert_eq!(
                            actual,
                            expected(element, opcode, rex, modrm),
                            "{bytes:02X?}"
                        );
                        classified += usize::from(actual.is_some());
                    }
                }
            }
        }
        assert_eq!(classified, 2 * 2 * 17 * 64);

        // Independently assembled by LLVM 23.0.0git.
        for (bytes, replay) in [
            (
                &[0x0F, 0x2F, 0xCB][..],
                X86LegacyFpFlagCompareReplay {
                    first: 1,
                    second: 3,
                    element: VecElementType::F32,
                    signaling: true,
                },
            ),
            (
                &[0x45, 0x0F, 0x2E, 0xCB][..],
                X86LegacyFpFlagCompareReplay {
                    first: 9,
                    second: 11,
                    element: VecElementType::F32,
                    signaling: false,
                },
            ),
            (
                &[0x66, 0x0F, 0x2F, 0xCB][..],
                X86LegacyFpFlagCompareReplay {
                    first: 1,
                    second: 3,
                    element: VecElementType::F64,
                    signaling: true,
                },
            ),
            (
                &[0x66, 0x44, 0x0F, 0x2E, 0xFA][..],
                X86LegacyFpFlagCompareReplay {
                    first: 15,
                    second: 2,
                    element: VecElementType::F64,
                    signaling: false,
                },
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .legacy_register_fp_flag_compare_replay(),
                Some(replay),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn legacy_classifier_rejects_every_prefix_opcode_operand_and_length_frontier() {
        let invalid: &[&[u8]] = &[
            &[0x0F, 0x2F],
            &[0x0F, 0x2F, 0xCB, 0],
            &[0x66, 0x0F, 0x2E],
            &[0x66, 0x0F, 0x2E, 0xCB, 0],
            &[0xF2, 0x0F, 0x2F, 0xCB],
            &[0xF3, 0x0F, 0x2E, 0xCB],
            &[0xF0, 0x0F, 0x2F, 0xCB],
            &[0x67, 0x0F, 0x2F, 0xCB],
            &[0x64, 0x0F, 0x2F, 0xCB],
            &[0x66, 0x66, 0x0F, 0x2F, 0xCB],
            &[0x48, 0x66, 0x0F, 0x2F, 0xCB],
            &[0x66, 0x40, 0x40, 0x0F, 0x2F, 0xCB],
            &[0xD5, 0x00, 0x0F, 0x2F, 0xCB],
            &[0xC5, 0xF8, 0x2F, 0xCB],
            &[0x62, 0xF1, 0x7C, 0x08, 0x2F, 0xCB],
            &[0x0E, 0x2F, 0xCB],
            &[0x0F, 0x2D, 0xCB],
            &[0x0F, 0x30, 0xCB],
            &[0x0F, 0x2F, 0x01],
            &[0x66, 0x0F, 0x2E, 0x01],
        ];
        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .legacy_register_fp_flag_compare_replay(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
