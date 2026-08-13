//! Register-source legacy MMX/SSE scalar-insert replay.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86X87ControlKind};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

/// Exact legacy scalar-insert operation selected by the mandatory prefix,
/// opcode map, opcode byte, and effective REX.W bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86LegacyScalarInsertKind {
    PinsB,
    PinsD,
    PinsQ,
    PinsWMap1Mmx,
    PinsWMap1Xmm,
}

impl X86LegacyScalarInsertKind {
    pub(crate) fn touches_mmx(self) -> bool {
        self == Self::PinsWMap1Mmx
    }

    pub(crate) fn requires_sse41(self) -> bool {
        matches!(self, Self::PinsB | Self::PinsD | Self::PinsQ)
    }

    fn element(self) -> VecElementType {
        match self {
            Self::PinsB => VecElementType::I8,
            Self::PinsWMap1Mmx | Self::PinsWMap1Xmm => VecElementType::I16,
            Self::PinsD => VecElementType::I32,
            Self::PinsQ => VecElementType::I64,
        }
    }

    fn lanes(self) -> u8 {
        match self {
            Self::PinsB => 16,
            Self::PinsWMap1Mmx => 4,
            Self::PinsWMap1Xmm => 8,
            Self::PinsD => 4,
            Self::PinsQ => 2,
        }
    }

    fn lane_mask(self) -> u8 {
        self.lanes() - 1
    }
}

/// Decoded architectural operands of one canonical register-source legacy
/// scalar insert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyScalarInsertReplay {
    pub(crate) kind: X86LegacyScalarInsertKind,
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) lane: u8,
}

/// Expected block-wide definition/use counts for a temporary elided by exact
/// legacy scalar-insert replay.
pub(crate) type X86LegacyScalarInsertVirtualRequirement = (VReg, usize, usize);

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn mm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Mm(index)))
}

fn exact_zero_scalar(operation: &SmirOp) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::Mov {
            dst: scalar @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => Some(scalar),
        _ => None,
    }
}

fn exact_extract(
    operation: &SmirOp,
    vector: VReg,
    lane: u8,
    element: VecElementType,
) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane && elem == element => Some(scalar),
        _ => None,
    }
}

fn exact_insert(
    operation: &SmirOp,
    vector: VReg,
    scalar: VReg,
    lane: u8,
    element: VecElementType,
) -> bool {
    operation.x86_hint.is_none()
        && matches!(
            operation.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && actual_lane == lane
                && elem == element
        )
}

/// Validate the complete stable SMIR graph emitted for one register-source
/// legacy scalar insert. The returned virtual-register counts prove that every
/// elided reconstruction temporary is confined to the source instruction.
/// MMX `PINSRW` has no virtual temporary and retains its independently lowered
/// leading `EnterMmx` marker.
pub(crate) fn x86_legacy_scalar_insert_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyScalarInsertReplay,
) -> Option<Vec<X86LegacyScalarInsertVirtualRequirement>> {
    if replay.kind.touches_mmx() {
        let [enter_mmx, insert] = ops else {
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
            || insert.x86_hint
                != Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xC4,
                })
            || !matches!(
                insert.kind,
                OpKind::VInsertLane {
                    dst,
                    vec,
                    scalar,
                    lane,
                    elem: VecElementType::I16,
                } if dst == mm(replay.destination)
                    && vec == mm(replay.destination)
                    && scalar == gpr(replay.source)
                    && lane == replay.lane
            )
        {
            return None;
        }
        return Some(Vec::new());
    }

    let destination = xmm(replay.destination);
    let source = gpr(replay.source);
    let element = replay.kind.element();
    let lanes = replay.kind.lanes();
    let mut index = 0usize;
    let mut requirements = Vec::with_capacity(usize::from(2 * lanes + 2));
    let mut merge_scalars = [None; 16];

    for lane in 0..lanes {
        if lane == replay.lane {
            continue;
        }
        let scalar = exact_extract(ops.get(index)?, destination, lane, element)?;
        index += 1;
        requirements.push((scalar, 1, 1));
        merge_scalars[usize::from(lane)] = Some(scalar);
    }

    let zero = exact_zero_scalar(ops.get(index)?)?;
    index += 1;
    let output = match ops.get(index)? {
        SmirOp {
            kind:
                OpKind::VBroadcast {
                    dst: output @ VReg::Virtual(_),
                    scalar,
                    elem,
                    lanes: actual_lanes,
                },
            x86_hint: None,
            ..
        } if *scalar == zero && *elem == element && *actual_lanes == lanes => *output,
        _ => return None,
    };
    index += 1;
    requirements.push((zero, 1, 1));
    requirements.push((output, usize::from(lanes) + 1, usize::from(lanes) + 1));

    for lane in 0..lanes {
        let scalar = if lane == replay.lane {
            source
        } else {
            merge_scalars[usize::from(lane)]?
        };
        if !exact_insert(ops.get(index)?, output, scalar, lane, element) {
            return None;
        }
        index += 1;
    }

    let raw = match ops.get(index)? {
        SmirOp {
            kind:
                OpKind::VMov {
                    dst: raw @ VReg::Virtual(_),
                    src,
                    width: VecWidth::V128,
                },
            x86_hint: None,
            ..
        } if *src == output => *raw,
        _ => return None,
    };
    index += 1;
    requirements.push((raw, 1, usize::from(lanes)));

    let mut result_scalars = [None; 16];
    for lane in 0..lanes {
        let scalar = exact_extract(ops.get(index)?, raw, lane, element)?;
        index += 1;
        requirements.push((scalar, 1, 1));
        result_scalars[usize::from(lane)] = Some(scalar);
    }
    for lane in 0..lanes {
        if !exact_insert(
            ops.get(index)?,
            destination,
            result_scalars[usize::from(lane)]?,
            lane,
            element,
        ) {
            return None;
        }
        index += 1;
    }
    if index != ops.len() {
        return None;
    }

    let mut unique = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| unique.insert(*register))
        .then_some(requirements)
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-source legacy `PINSRB/D/Q/W`.
    ///
    /// XMM forms require mandatory 66H; the MMX map-0F `PINSRW` form has no
    /// mandatory prefix. One optional final REX prefix extends the encoded GPR
    /// and XMM fields. REX.W selects `PINSRQ` at opcode 0F3A 22H and is ignored
    /// for `PINSRB` and both `PINSRW` forms; MMX indices remain three bits.
    /// Memory sources, other or repeated prefixes, non-final or duplicate REX,
    /// REX2, VEX/EVEX, truncated instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_scalar_insert_replay(
        &self,
    ) -> Option<X86LegacyScalarInsertReplay> {
        let (operand_size, rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (true, Some(*rex), tail),
            [0x66, tail @ ..] => (true, None, tail),
            [rex @ 0x40..=0x4F, tail @ ..] => (false, Some(*rex), tail),
            tail => (false, None, tail),
        };
        let rex = rex.unwrap_or(0);
        let (kind, modrm, immediate) = match tail {
            [0x0F, 0xC4, modrm, immediate] => (
                if operand_size {
                    X86LegacyScalarInsertKind::PinsWMap1Xmm
                } else {
                    X86LegacyScalarInsertKind::PinsWMap1Mmx
                },
                *modrm,
                *immediate,
            ),
            [0x0F, 0x3A, opcode, modrm, immediate] if operand_size => {
                let kind = match *opcode {
                    0x20 => X86LegacyScalarInsertKind::PinsB,
                    0x22 if rex & 0x08 != 0 => X86LegacyScalarInsertKind::PinsQ,
                    0x22 => X86LegacyScalarInsertKind::PinsD,
                    _ => return None,
                };
                (kind, *modrm, *immediate)
            }
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }

        let destination = (modrm >> 3) & 7;
        Some(X86LegacyScalarInsertReplay {
            kind,
            destination: if kind.touches_mmx() {
                destination
            } else {
                destination | ((rex & 0x04) << 1)
            },
            source: (modrm & 7) | ((rex & 0x01) << 3),
            lane: immediate & kind.lane_mask(),
        })
    }

    /// Rewrite a validated scalar-insert source to RAX/EAX while retaining the
    /// exact destination, operation, width selector, and immediate. The x86-64
    /// lowerer uses this for guest RSP/RBP sources, which must be loaded from
    /// guest state rather than read from the host stack/frame registers.
    pub(crate) fn legacy_scalar_insert_with_source_rax(&self) -> Option<Self> {
        let replay = self.legacy_register_scalar_insert_replay()?;
        let mut rewritten = *self;
        let modrm_index = self.as_slice().len().checked_sub(2)?;
        rewritten.bytes[modrm_index] &= !0x07;
        let opcode_index = self.as_slice().iter().position(|byte| *byte == 0x0F)?;
        if let Some(rex_index) = self.as_slice()[..opcode_index]
            .iter()
            .position(|byte| (0x40..=0x4F).contains(byte))
        {
            rewritten.bytes[rex_index] &= !0x01;
        }
        debug_assert_eq!(
            rewritten
                .legacy_register_scalar_insert_replay()
                .map(|actual| (actual.kind, actual.destination, actual.source, actual.lane)),
            Some((replay.kind, replay.destination, 0, replay.lane))
        );
        Some(rewritten)
    }
}
