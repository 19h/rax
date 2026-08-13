//! Register-destination legacy MMX/SSE scalar-extract replay.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86X87ControlKind};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, X86Reg,
};

/// Exact legacy scalar-extract operation selected by the mandatory prefix,
/// opcode map, opcode byte, and effective REX.W bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86LegacyScalarExtractKind {
    ExtractPs,
    PextrB,
    PextrD,
    PextrQ,
    PextrWMap1Mmx,
    PextrWMap1Xmm,
    PextrWMap3,
}

impl X86LegacyScalarExtractKind {
    pub(crate) fn touches_mmx(self) -> bool {
        self == Self::PextrWMap1Mmx
    }

    pub(crate) fn requires_sse41(self) -> bool {
        matches!(
            self,
            Self::ExtractPs | Self::PextrB | Self::PextrD | Self::PextrQ | Self::PextrWMap3
        )
    }

    fn uses_map1_destination(self) -> bool {
        matches!(self, Self::PextrWMap1Mmx | Self::PextrWMap1Xmm)
    }

    fn element(self) -> VecElementType {
        match self {
            Self::PextrB => VecElementType::I8,
            Self::PextrWMap1Mmx | Self::PextrWMap1Xmm | Self::PextrWMap3 => VecElementType::I16,
            Self::ExtractPs | Self::PextrD => VecElementType::I32,
            Self::PextrQ => VecElementType::I64,
        }
    }

    fn lane_mask(self) -> u8 {
        match self {
            Self::PextrB => 0x0F,
            Self::PextrWMap1Mmx => 0x03,
            Self::PextrWMap1Xmm | Self::PextrWMap3 => 0x07,
            Self::ExtractPs | Self::PextrD => 0x03,
            Self::PextrQ => 0x01,
        }
    }

    fn destination_width(self) -> OpWidth {
        if self == Self::PextrQ {
            OpWidth::W64
        } else {
            OpWidth::W32
        }
    }
}

/// Decoded architectural operands of one canonical register-destination
/// legacy scalar extract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyScalarExtractReplay {
    pub(crate) kind: X86LegacyScalarExtractKind,
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) lane: u8,
}

/// Expected block-wide definition/use counts for a temporary elided by exact
/// legacy scalar-extract replay.
pub(crate) type X86LegacyScalarExtractVirtualRequirement = (VReg, usize, usize);

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn mm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Mm(index)))
}

/// Validate the complete stable SMIR graph emitted for one register-only
/// legacy scalar extract. The returned virtual-register counts prove that an
/// elided XMM extraction temporary does not escape its source instruction.
/// MMX `PEXTRW` has no virtual temporary and retains its independently lowered
/// leading `EnterMmx` marker.
pub(crate) fn x86_legacy_scalar_extract_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyScalarExtractReplay,
) -> Option<Vec<X86LegacyScalarExtractVirtualRequirement>> {
    if replay.kind.touches_mmx() {
        let [enter_mmx, extract] = ops else {
            return None;
        };
        if enter_mmx.x86_hint.is_some()
            || !matches!(
                enter_mmx.kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                }
            )
            || extract.x86_hint
                != Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xC5,
                })
            || !matches!(
                extract.kind,
                OpKind::VExtractLane {
                    dst,
                    vec,
                    lane,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                } if dst == gpr(replay.destination)
                    && vec == mm(replay.source)
                    && lane == replay.lane
            )
        {
            return None;
        }
        return Some(Vec::new());
    }

    let [extract, move_result] = ops else {
        return None;
    };
    if extract.x86_hint.is_some() || move_result.x86_hint.is_some() {
        return None;
    }
    let temporary = match extract.kind {
        OpKind::VExtractLane {
            dst: temporary @ VReg::Virtual(_),
            vec,
            lane,
            elem,
            sign: SignExtend::Zero,
        } if vec == xmm(replay.source) && lane == replay.lane && elem == replay.kind.element() => {
            temporary
        }
        _ => return None,
    };
    if !matches!(
        move_result.kind,
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(source),
            width,
        } if dst == gpr(replay.destination)
            && source == temporary
            && width == replay.kind.destination_width()
    ) {
        return None;
    }
    Some(vec![(temporary, 1, 1)])
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-destination legacy `EXTRACTPS` or
    /// `PEXTRB/D/Q/W` instruction.
    ///
    /// The XMM forms require mandatory 66H; the MMX map-0F `PEXTRW` form has
    /// no mandatory prefix. One optional final REX prefix extends the encoded
    /// GPR/XMM fields. REX.W selects `PEXTRQ` at opcode 0F3A 16H and is ignored
    /// for the other forms; MMX indices remain three bits. Memory destinations,
    /// other or repeated prefixes, non-final or duplicate REX, REX2, VEX/EVEX,
    /// truncated instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_scalar_extract_replay(
        &self,
    ) -> Option<X86LegacyScalarExtractReplay> {
        let (operand_size, rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (true, Some(*rex), tail),
            [0x66, tail @ ..] => (true, None, tail),
            [rex @ 0x40..=0x4F, tail @ ..] => (false, Some(*rex), tail),
            tail => (false, None, tail),
        };
        let rex = rex.unwrap_or(0);
        let (kind, modrm, immediate) = match tail {
            [0x0F, 0xC5, modrm, immediate] => (
                if operand_size {
                    X86LegacyScalarExtractKind::PextrWMap1Xmm
                } else {
                    X86LegacyScalarExtractKind::PextrWMap1Mmx
                },
                *modrm,
                *immediate,
            ),
            [0x0F, 0x3A, opcode, modrm, immediate] if operand_size => {
                let kind = match *opcode {
                    0x14 => X86LegacyScalarExtractKind::PextrB,
                    0x15 => X86LegacyScalarExtractKind::PextrWMap3,
                    0x16 if rex & 0x08 != 0 => X86LegacyScalarExtractKind::PextrQ,
                    0x16 => X86LegacyScalarExtractKind::PextrD,
                    0x17 => X86LegacyScalarExtractKind::ExtractPs,
                    _ => return None,
                };
                (kind, *modrm, *immediate)
            }
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }

        let reg = (modrm >> 3) & 7;
        let rm = modrm & 7;
        let rex_r = (rex & 0x04) << 1;
        let rex_b = (rex & 0x01) << 3;
        let (destination, source) = if kind.uses_map1_destination() {
            (
                reg | rex_r,
                if kind.touches_mmx() { rm } else { rm | rex_b },
            )
        } else {
            (rm | rex_b, reg | rex_r)
        };
        Some(X86LegacyScalarExtractReplay {
            kind,
            destination,
            source,
            lane: immediate & kind.lane_mask(),
        })
    }

    /// Rewrite a validated scalar-extract destination to RAX/EAX while
    /// retaining the exact source, operation, width selector, and immediate.
    /// The x86-64 lowerer uses this for guest RSP/RBP destinations, which must
    /// commit through state rather than execute against the host stack/frame.
    pub(crate) fn legacy_scalar_extract_with_destination_rax(&self) -> Option<Self> {
        let replay = self.legacy_register_scalar_extract_replay()?;
        let mut rewritten = *self;
        let modrm_index = self.as_slice().len().checked_sub(2)?;
        let (modrm_mask, rex_mask) = if replay.kind.uses_map1_destination() {
            (0x38, 0x04)
        } else {
            (0x07, 0x01)
        };
        rewritten.bytes[modrm_index] &= !modrm_mask;
        let opcode_index = self.as_slice().iter().position(|byte| *byte == 0x0F)?;
        if let Some(rex_index) = self.as_slice()[..opcode_index]
            .iter()
            .position(|byte| (0x40..=0x4F).contains(byte))
        {
            rewritten.bytes[rex_index] &= !rex_mask;
        }
        debug_assert_eq!(
            rewritten
                .legacy_register_scalar_extract_replay()
                .map(|actual| (actual.kind, actual.destination, actual.source, actual.lane)),
            Some((replay.kind, 0, replay.source, replay.lane))
        );
        Some(rewritten)
    }
}
