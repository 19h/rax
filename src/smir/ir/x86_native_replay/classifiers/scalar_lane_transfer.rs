//! Register-only scalar lane-transfer replay classification.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

/// Decoded architectural operands and controls of one exact register-only
/// legacy SSE4.1 `INSERTPS` instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyInsertpsReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) immediate: u8,
}

/// Expected block-wide definition/use counts for one temporary elided by
/// exact native replay.
pub(crate) type X86LegacyInsertpsVirtualRequirement = (VReg, usize, usize);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
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

fn exact_i32_extract(operation: &SmirOp, vector: VReg, lane: u8) -> Option<VReg> {
    if operation.x86_hint.is_some() {
        return None;
    }
    match operation.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem: VecElementType::I32,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane => Some(scalar),
        _ => None,
    }
}

fn exact_i32_insert(operation: &SmirOp, vector: VReg, scalar: VReg, lane: u8) -> bool {
    operation.x86_hint.is_none()
        && matches!(
            operation.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: VecElementType::I32,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && actual_lane == lane
        )
}

/// Validate the complete stable semantic graph emitted for one register-only
/// legacy `INSERTPS`. O1/O2 may remove either dead O0 temporary: the selected
/// source lane when the destination lane is zero-masked, or the mask-zero
/// scalar when no lane is zero-masked. Each returned tuple is `(virtual
/// register, definitions, uses)` so the grouping layer can prove that no
/// elided temporary escapes the source instruction.
pub(crate) fn x86_legacy_insertps_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyInsertpsReplay,
) -> Option<Vec<X86LegacyInsertpsVirtualRequirement>> {
    let destination = xmm(replay.destination);
    let source = xmm(replay.source);
    let source_lane = replay.immediate >> 6;
    let destination_lane = (replay.immediate >> 4) & 3;
    let zero_mask = replay.immediate & 0x0F;
    let source_live = zero_mask & (1 << destination_lane) == 0;
    let mut index = 0usize;
    let mut requirements = Vec::with_capacity(13);

    let source_scalar = if source_live {
        let scalar = exact_i32_extract(ops.get(index)?, source, source_lane)?;
        index += 1;
        requirements.push((scalar, 1, 1));
        Some(scalar)
    } else if let Some(scalar) = ops
        .get(index)
        .and_then(|operation| exact_i32_extract(operation, source, source_lane))
    {
        index += 1;
        requirements.push((scalar, 1, 0));
        Some(scalar)
    } else {
        None
    };

    let mask_zero = if zero_mask != 0 {
        let scalar = exact_zero_scalar(ops.get(index)?)?;
        index += 1;
        requirements.push((scalar, 1, zero_mask.count_ones() as usize));
        Some(scalar)
    } else if let Some(scalar) = ops
        .get(index)
        .and_then(|operation| exact_zero_scalar(operation))
    {
        index += 1;
        requirements.push((scalar, 1, 0));
        Some(scalar)
    } else {
        None
    };

    let mut merge_scalars = [None; 4];
    for lane in 0..4u8 {
        if zero_mask & (1 << lane) == 0 && lane != destination_lane {
            let scalar = exact_i32_extract(ops.get(index)?, destination, lane)?;
            index += 1;
            requirements.push((scalar, 1, 1));
            merge_scalars[usize::from(lane)] = Some(scalar);
        }
    }

    let broadcast_zero = exact_zero_scalar(ops.get(index)?)?;
    index += 1;
    let output = match ops.get(index)? {
        SmirOp {
            kind:
                OpKind::VBroadcast {
                    dst: output @ VReg::Virtual(_),
                    scalar,
                    elem: VecElementType::I32,
                    lanes: 4,
                },
            x86_hint: None,
            ..
        } if *scalar == broadcast_zero => *output,
        _ => return None,
    };
    index += 1;
    requirements.push((broadcast_zero, 1, 1));
    requirements.push((output, 5, 5));

    for lane in 0..4u8 {
        let scalar = if zero_mask & (1 << lane) != 0 {
            mask_zero?
        } else if lane == destination_lane {
            source_scalar?
        } else {
            merge_scalars[usize::from(lane)]?
        };
        if !exact_i32_insert(ops.get(index)?, output, scalar, lane) {
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
    requirements.push((raw, 1, 4));

    let mut result_scalars = [None; 4];
    for lane in 0..4u8 {
        let scalar = exact_i32_extract(ops.get(index)?, raw, lane)?;
        index += 1;
        requirements.push((scalar, 1, 1));
        result_scalars[usize::from(lane)] = Some(scalar);
    }
    for lane in 0..4u8 {
        if !exact_i32_insert(
            ops.get(index)?,
            destination,
            result_scalars[usize::from(lane)]?,
            lane,
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

#[derive(Clone, Copy)]
enum GprField {
    None,
    Reg,
    Rm,
}

impl X86InstructionBytes {
    /// Decode one exact register-only legacy SSE4.1 `INSERTPS`.
    ///
    /// The mandatory 66H prefix may be followed by one final REX byte.
    /// REX.R/B extend the XMM operands; REX.W/X are ignored by execution but
    /// retained in the replay bytes. Memory, other or reordered prefixes,
    /// non-final or duplicate REX, REX2/VEX/EVEX, truncated, and trailing-byte
    /// forms fail closed.
    pub(crate) fn legacy_register_insertps_replay(&self) -> Option<X86LegacyInsertpsReplay> {
        let (rex, modrm, immediate) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, 0x0F, 0x3A, 0x21, modrm, immediate] => {
                (Some(*rex), *modrm, *immediate)
            }
            [0x66, 0x0F, 0x3A, 0x21, modrm, immediate] => (None, *modrm, *immediate),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let rex = rex.unwrap_or(0);
        Some(X86LegacyInsertpsReplay {
            destination: ((modrm >> 3) & 7) | ((rex & 0x04) << 1),
            source: (modrm & 7) | ((rex & 0x01) << 3),
            immediate,
        })
    }

    /// Validate one register-only EVEX scalar lane transfer that is not already
    /// directly lowerable from its semantic SMIR operations.
    ///
    /// The admitted set is `VEXTRACTPS`, `VINSERTPS`, `VPEXTRB/D/Q/W`, and
    /// `VPINSRB/D/Q/W`. Dword/qword integer forms require AVX-512DQ and return
    /// `true`; the remaining forms require AVX-512F or AVX-512BW. Every form is
    /// fixed at EVEX.128 and forbids masking, zeroing, and EVEX.b. Memory forms,
    /// fabricated GPR bit 4, and GPR operands using RSP/RBP fail closed.
    pub fn evex_register_scalar_lane_transfer_requires_dq(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || p2 & !0x08 != 0 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0f;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        if pp != 1 {
            return None;
        }

        let (needs_dq, reserved_vvvv, gpr_field) = match (map, opcode, w) {
            // VPINSRW and VPEXTRW reg,xmm aliases. W is ignored.
            (1, 0xC4, _) => (false, false, GprField::Rm),
            (1, 0xC5, _) => (false, true, GprField::Reg),

            // VPEXTRB/W/D/Q and VEXTRACTPS. W is ignored for B/W/PS.
            (3, 0x14 | 0x15 | 0x17, _) => (false, true, GprField::Rm),
            (3, 0x16, _) => (true, true, GprField::Rm),

            // VPINSRB/D/Q and VINSERTPS. W is ignored for VPINSRB; VINSERTPS
            // requires W0, while both VPINSRD and VPINSRQ require AVX-512DQ.
            (3, 0x20, _) => (false, false, GprField::Rm),
            (3, 0x21, false) => (false, false, GprField::None),
            (3, 0x22, _) => (true, false, GprField::Rm),
            _ => return None,
        };

        if reserved_vvvv && (p1 & 0x78 != 0x78 || p2 & 0x08 == 0) {
            return None;
        }

        let (extension_valid, low_gpr_bank, gpr_low) = match gpr_field {
            GprField::None => (true, false, 0),
            // EVEX.R' cannot name a 17th GPR. EVEX.R selects GPR0-7/8-15.
            GprField::Reg => (p0 & 0x10 != 0, p0 & 0x80 != 0, (modrm >> 3) & 0x07),
            // EVEX.X' cannot name a 17th GPR. EVEX.B selects GPR0-7/8-15.
            GprField::Rm => (p0 & 0x40 != 0, p0 & 0x20 != 0, modrm & 0x07),
        };
        if !extension_valid || (low_gpr_bank && matches!(gpr_low, 4 | 5)) {
            return None;
        }

        Some(needs_dq)
    }
}
