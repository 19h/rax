//! Register-only legacy MMX/SSE packed floating-point conversion replay.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86X87ControlKind};
use crate::smir::ir::types::{ArchReg, FpRoundMode, VReg, VecElementType, VecWidth, X86Reg};

/// Exact legacy packed conversion selected by opcodes 0F 2A, 0F 2C, 0F 2D,
/// or 0F 5A without a mandatory prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86LegacyPackedFpConvertKind {
    Cvtpi2ps,
    Cvttps2pi,
    Cvtps2pi,
    Cvtps2pd,
}

impl X86LegacyPackedFpConvertKind {
    /// Whether this instruction consumes or produces MM0-MM7 and therefore
    /// must retain the lifter's precise trailing `EnterMmx` state commit.
    pub(crate) fn touches_mmx(self) -> bool {
        self != Self::Cvtps2pd
    }

    fn opcode(self) -> u8 {
        match self {
            Self::Cvtpi2ps => 0x2A,
            Self::Cvttps2pi => 0x2C,
            Self::Cvtps2pi => 0x2D,
            Self::Cvtps2pd => 0x5A,
        }
    }
}

/// Decoded architectural operands of one canonical register-only legacy
/// packed floating-point conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyPackedFpConvertReplay {
    pub(crate) kind: X86LegacyPackedFpConvertKind,
    pub(crate) destination: u8,
    pub(crate) source: u8,
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn mm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Mm(index)))
}

fn exact_sse_hint(op: &SmirOp, opcode: u8) -> bool {
    op.x86_hint
        == Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        })
}

/// Validate the complete stable semantic graph emitted for one register-only
/// legacy packed conversion. MMX-crossing forms include the exact trailing
/// state marker here even though replay deliberately leaves that marker for
/// independent lowering.
pub(crate) fn x86_legacy_packed_fp_convert_shape_matches(
    ops: &[SmirOp],
    replay: X86LegacyPackedFpConvertReplay,
) -> bool {
    let expected_len = if replay.kind.touches_mmx() { 2 } else { 1 };
    if ops.len() != expected_len || !exact_sse_hint(&ops[0], replay.kind.opcode()) {
        return false;
    }

    let operation_matches = match replay.kind {
        X86LegacyPackedFpConvertKind::Cvtpi2ps => matches!(
            &ops[0].kind,
            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask: None,
                int_elem: VecElementType::I32,
                fp_elem: VecElementType::F32,
                signed: true,
                lanes: 2,
                src_width: VecWidth::V64,
                dst_width: VecWidth::V64,
                mask_zeroing: false,
                zero_upper: false,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
            } if *dst == xmm(replay.destination) && *src == mm(replay.source)
        ),
        X86LegacyPackedFpConvertKind::Cvttps2pi | X86LegacyPackedFpConvertKind::Cvtps2pi => {
            let truncate = replay.kind == X86LegacyPackedFpConvertKind::Cvttps2pi;
            let round = if truncate {
                FpRoundMode::RoundTowardZero
            } else {
                FpRoundMode::Dynamic
            };
            matches!(
                &ops[0].kind,
                OpKind::X86PackedFpToInt {
                    dst,
                    src,
                    mask: None,
                    fp_elem: VecElementType::F32,
                    int_elem: VecElementType::I32,
                    signed: true,
                    truncate: actual_truncate,
                    lanes: 2,
                    src_width: VecWidth::V64,
                    dst_width: VecWidth::V64,
                    mask_zeroing: false,
                    zero_upper: false,
                    round: actual_round,
                    suppress_exceptions: false,
                } if *dst == mm(replay.destination)
                    && *src == xmm(replay.source)
                    && *actual_truncate == truncate
                    && *actual_round == round
            )
        }
        X86LegacyPackedFpConvertKind::Cvtps2pd => matches!(
            &ops[0].kind,
            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask: None,
                from: VecElementType::F32,
                to: VecElementType::F64,
                lanes: 2,
                dst_width: VecWidth::V128,
                mask_zeroing: false,
                zero_upper: false,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                report_fp16_denormal: false,
            } if *dst == xmm(replay.destination) && *src == xmm(replay.source)
        ),
    };
    operation_matches
        && (!replay.kind.touches_mmx()
            || matches!(
                ops[1].kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                }
            ) && ops[1].x86_hint.is_none())
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy packed conversion.
    /// Only an optional final REX prefix is accepted. REX.R extends an XMM
    /// ModR/M.reg operand, REX.B extends an XMM ModR/M.r/m operand, and every
    /// extension bit naming an MMX operand is ignored. REX.W/X are ignored.
    /// Mandatory, segment/address-size, repeat/lock, REX2, VEX/EVEX, memory,
    /// truncated, and trailing-byte forms fail closed.
    pub(crate) fn legacy_register_packed_fp_convert_replay(
        &self,
    ) -> Option<X86LegacyPackedFpConvertReplay> {
        let (rex, tail) = match self.as_slice() {
            [rex @ 0x40..=0x4F, tail @ ..] => (Some(*rex), tail),
            tail => (None, tail),
        };
        let (kind, modrm) = match tail {
            [0x0F, 0x2A, modrm] => (X86LegacyPackedFpConvertKind::Cvtpi2ps, *modrm),
            [0x0F, 0x2C, modrm] => (X86LegacyPackedFpConvertKind::Cvttps2pi, *modrm),
            [0x0F, 0x2D, modrm] => (X86LegacyPackedFpConvertKind::Cvtps2pi, *modrm),
            [0x0F, 0x5A, modrm] => (X86LegacyPackedFpConvertKind::Cvtps2pd, *modrm),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }

        let rex = rex.unwrap_or(0);
        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;
        let rex_r = (rex & 0x04) << 1;
        let rex_b = (rex & 0x01) << 3;
        let (destination, source) = match kind {
            X86LegacyPackedFpConvertKind::Cvtpi2ps => (reg | rex_r, rm),
            X86LegacyPackedFpConvertKind::Cvttps2pi | X86LegacyPackedFpConvertKind::Cvtps2pi => {
                (reg, rm | rex_b)
            }
            X86LegacyPackedFpConvertKind::Cvtps2pd => (reg | rex_r, rm | rex_b),
        };
        Some(X86LegacyPackedFpConvertReplay {
            kind,
            destination,
            source,
        })
    }
}
