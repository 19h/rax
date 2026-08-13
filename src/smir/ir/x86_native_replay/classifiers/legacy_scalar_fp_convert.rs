//! Register-only legacy SSE/SSE2 scalar floating-point conversion replay.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{ArchReg, FpRoundMode, OpWidth, VReg, VecElementType, X86Reg};

/// Exact scalar conversion selected by mandatory F2/F3, opcode 0F 2A, 0F
/// 2C, 0F 2D, or 0F 5A, and the effective REX.W bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86LegacyScalarFpConvertKind {
    IntToFp {
        elem: VecElementType,
        int_width: OpWidth,
    },
    FpToInt {
        elem: VecElementType,
        int_width: OpWidth,
        truncate: bool,
    },
    FpConvert {
        from: VecElementType,
        to: VecElementType,
    },
}

impl X86LegacyScalarFpConvertKind {
    fn mandatory_prefix(self) -> X86SsePrefix {
        let elem = match self {
            Self::IntToFp { elem, .. } | Self::FpToInt { elem, .. } => elem,
            Self::FpConvert { from, .. } => from,
        };
        if elem == VecElementType::F32 {
            X86SsePrefix::Rep
        } else {
            X86SsePrefix::Repne
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::IntToFp { .. } => 0x2A,
            Self::FpToInt { truncate: true, .. } => 0x2C,
            Self::FpToInt {
                truncate: false, ..
            } => 0x2D,
            Self::FpConvert { .. } => 0x5A,
        }
    }
}

/// Decoded architectural operands of one canonical register-only legacy
/// scalar conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyScalarFpConvertReplay {
    pub(crate) kind: X86LegacyScalarFpConvertKind,
    pub(crate) destination: u8,
    pub(crate) source: u8,
}

impl X86LegacyScalarFpConvertReplay {
    /// Architectural GPR destination for scalar FP-to-integer forms.
    pub(crate) fn gpr_destination(self) -> Option<u8> {
        matches!(self.kind, X86LegacyScalarFpConvertKind::FpToInt { .. })
            .then_some(self.destination)
    }

    /// Architectural GPR source for scalar integer-to-FP forms.
    pub(crate) fn gpr_source(self) -> Option<u8> {
        matches!(self.kind, X86LegacyScalarFpConvertKind::IntToFp { .. }).then_some(self.source)
    }
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

/// Validate the complete one-operation semantic graph emitted for one
/// register-only legacy scalar conversion. Source-byte replay is admitted only
/// when operands, widths, rounding, exception policy, merge behavior, and the
/// exact SSE provenance hint all agree with the decoded instruction.
pub(crate) fn x86_legacy_scalar_fp_convert_shape_matches(
    ops: &[SmirOp],
    replay: X86LegacyScalarFpConvertReplay,
) -> bool {
    let [operation] = ops else {
        return false;
    };
    if operation.x86_hint
        != Some(X86OpHint::SseOp {
            prefix: replay.kind.mandatory_prefix(),
            opcode: replay.kind.opcode(),
        })
    {
        return false;
    }

    match (replay.kind, &operation.kind) {
        (
            X86LegacyScalarFpConvertKind::IntToFp { elem, int_width },
            OpKind::X86IntToFp {
                dst,
                merge,
                src,
                elem: actual_elem,
                int_width: actual_int_width,
                signed: true,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: false,
            },
        ) => {
            *dst == xmm(replay.destination)
                && *merge == *dst
                && *src == gpr(replay.source)
                && *actual_elem == elem
                && *actual_int_width == int_width
        }
        (
            X86LegacyScalarFpConvertKind::FpToInt {
                elem,
                int_width,
                truncate,
            },
            OpKind::X86FpToInt {
                dst,
                src,
                elem: actual_elem,
                int_width: actual_int_width,
                signed: true,
                truncate: actual_truncate,
                round,
                suppress_exceptions: false,
            },
        ) => {
            let expected_round = if truncate {
                FpRoundMode::RoundTowardZero
            } else {
                FpRoundMode::Dynamic
            };
            *dst == gpr(replay.destination)
                && *src == xmm(replay.source)
                && *actual_elem == elem
                && *actual_int_width == int_width
                && *actual_truncate == truncate
                && *round == expected_round
        }
        (
            X86LegacyScalarFpConvertKind::FpConvert { from, to },
            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask: None,
                from: actual_from,
                to: actual_to,
                mask_zeroing: false,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: false,
            },
        ) => {
            *dst == xmm(replay.destination)
                && *merge == *dst
                && *src == xmm(replay.source)
                && *actual_from == from
                && *actual_to == to
        }
        _ => false,
    }
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy scalar conversion.
    ///
    /// F3 selects binary32 and F2 selects binary64. The mandatory prefix may
    /// be followed by one final REX prefix. REX.W selects 64-bit integer input
    /// or output for opcodes 2A/2C/2D and is ignored by opcode 5A. REX.R/B
    /// extend ModR/M.reg and ModR/M.r/m respectively; REX.X is ignored. Other
    /// prefix orders, duplicate prefixes, REX2/VEX/EVEX, memory, truncated,
    /// and trailing-byte forms fail closed.
    pub(crate) fn legacy_register_scalar_fp_convert_replay(
        &self,
    ) -> Option<X86LegacyScalarFpConvertReplay> {
        let (elem, rex, tail) = match self.as_slice() {
            [0xF3, rex @ 0x40..=0x4F, tail @ ..] => (VecElementType::F32, Some(*rex), tail),
            [0xF3, tail @ ..] => (VecElementType::F32, None, tail),
            [0xF2, rex @ 0x40..=0x4F, tail @ ..] => (VecElementType::F64, Some(*rex), tail),
            [0xF2, tail @ ..] => (VecElementType::F64, None, tail),
            _ => return None,
        };
        let [0x0F, opcode @ (0x2A | 0x2C | 0x2D | 0x5A), modrm] = tail else {
            return None;
        };
        if modrm >> 6 != 3 {
            return None;
        }

        let rex = rex.unwrap_or(0);
        let int_width = if rex & 0x08 != 0 {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        let kind = match *opcode {
            0x2A => X86LegacyScalarFpConvertKind::IntToFp { elem, int_width },
            0x2C | 0x2D => X86LegacyScalarFpConvertKind::FpToInt {
                elem,
                int_width,
                truncate: *opcode == 0x2C,
            },
            0x5A => X86LegacyScalarFpConvertKind::FpConvert {
                from: elem,
                to: if elem == VecElementType::F32 {
                    VecElementType::F64
                } else {
                    VecElementType::F32
                },
            },
            _ => unreachable!("scalar conversion opcode was pattern-validated"),
        };
        Some(X86LegacyScalarFpConvertReplay {
            kind,
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
        })
    }

    /// Rewrite a validated legacy scalar FP-to-integer destination to RAX/EAX
    /// while preserving all non-destination source bits, including ignored
    /// REX.W/X images. This supports the state-backed guest RSP/RBP wrapper.
    pub(crate) fn legacy_scalar_fp_to_int_with_destination_rax(&self) -> Option<Self> {
        let replay = self.legacy_register_scalar_fp_convert_replay()?;
        replay.gpr_destination()?;
        let mut rewritten = *self;
        let modrm_index = self.as_slice().len() - 1;
        rewritten.bytes[modrm_index] &= !0x38;
        if self.as_slice().len() == 5 {
            rewritten.bytes[1] &= !0x04;
        }
        debug_assert_eq!(
            rewritten
                .legacy_register_scalar_fp_convert_replay()
                .and_then(X86LegacyScalarFpConvertReplay::gpr_destination),
            Some(0)
        );
        Some(rewritten)
    }

    /// Rewrite a validated legacy scalar integer-to-FP source to RAX/EAX
    /// while preserving all non-source bits, including ignored REX.X images
    /// and the operand-width-selecting REX.W bit.
    pub(crate) fn legacy_scalar_int_to_fp_with_source_rax(&self) -> Option<Self> {
        let replay = self.legacy_register_scalar_fp_convert_replay()?;
        replay.gpr_source()?;
        let mut rewritten = *self;
        let modrm_index = self.as_slice().len() - 1;
        rewritten.bytes[modrm_index] &= !0x07;
        if self.as_slice().len() == 5 {
            rewritten.bytes[1] &= !0x01;
        }
        debug_assert_eq!(
            rewritten
                .legacy_register_scalar_fp_convert_replay()
                .and_then(X86LegacyScalarFpConvertReplay::gpr_source),
            Some(0)
        );
        Some(rewritten)
    }
}
