//! Exact register-only legacy scalar-XMM MOVQ replay classification and
//! semantic graph validation.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, X86Reg,
};

/// Decoded architectural operands of one canonical register-only legacy
/// scalar-XMM MOVQ instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyScalarXmmMovqReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
}

/// Expected block-wide definition/use counts for one virtual register elided
/// by exact native replay.
pub(crate) type X86LegacyScalarXmmMovqVirtualRequirement = (VReg, usize, usize);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn exact_extract(op: &SmirOp, vector: VReg, lane: u8) -> Option<VReg> {
    if op.x86_hint.is_some() {
        return None;
    }
    match op.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane: actual_lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } if vec == vector && actual_lane == lane => Some(scalar),
        _ => None,
    }
}

fn exact_insert(op: &SmirOp, vector: VReg, scalar: VReg, lane: u8) -> bool {
    op.x86_hint.is_none()
        && matches!(
            op.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane: actual_lane,
                elem: VecElementType::I64,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && actual_lane == lane
        )
}

/// Validate the complete eight-operation low-qword transfer and high-qword
/// zeroing graph emitted by both legacy register-only MOVQ opcode directions.
/// Each returned tuple is `(virtual register, definitions, uses)` so the
/// grouping layer can prove that no elided temporary escapes the instruction.
pub(crate) fn x86_legacy_scalar_xmm_movq_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyScalarXmmMovqReplay,
) -> Option<[X86LegacyScalarXmmMovqVirtualRequirement; 5]> {
    let [
        extract_source,
        zero,
        broadcast,
        insert_source,
        extract_low,
        extract_high,
        insert_low,
        insert_high,
    ] = ops
    else {
        return None;
    };

    let scalar = exact_extract(extract_source, xmm(replay.source), 0)?;
    let zero_scalar = match zero.kind {
        OpKind::Mov {
            dst: zero_scalar @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } if zero.x86_hint.is_none() => zero_scalar,
        _ => return None,
    };
    let result = match broadcast.kind {
        OpKind::VBroadcast {
            dst: result @ VReg::Virtual(_),
            scalar,
            elem: VecElementType::I64,
            lanes: 1,
        } if scalar == zero_scalar && broadcast.x86_hint.is_none() => result,
        _ => return None,
    };
    if !exact_insert(insert_source, result, scalar, 0) {
        return None;
    }
    let low = exact_extract(extract_low, result, 0)?;
    let high = exact_extract(extract_high, result, 1)?;
    let destination = xmm(replay.destination);
    if !exact_insert(insert_low, destination, low, 0)
        || !exact_insert(insert_high, destination, high, 1)
    {
        return None;
    }

    let requirements = [
        (scalar, 1, 1),
        (zero_scalar, 1, 1),
        (result, 2, 3),
        (low, 1, 1),
        (high, 1, 1),
    ];
    let mut registers = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| registers.insert(*register))
        .then_some(requirements)
}

impl X86InstructionBytes {
    /// Decode an exact canonical register-only legacy scalar-XMM MOVQ.
    ///
    /// `66 0F D6 /r` encodes the destination in ModR/M.r/m and source in
    /// ModR/M.reg; `F3 0F 7E /r` encodes the reverse field direction. An
    /// optional final REX prefix extends both fields, while REX.W and REX.X
    /// are ignored. Memory, malformed prefix, VEX/EVEX, REX2, truncated, and
    /// trailing-byte forms fail closed.
    pub(crate) fn legacy_scalar_xmm_movq_replay(&self) -> Option<X86LegacyScalarXmmMovqReplay> {
        let (prefix, rex, opcode, modrm) = match self.as_slice() {
            [prefix @ (0x66 | 0xF3), 0x0F, opcode, modrm] => (*prefix, None, *opcode, *modrm),
            [
                prefix @ (0x66 | 0xF3),
                rex @ 0x40..=0x4F,
                0x0F,
                opcode,
                modrm,
            ] => (*prefix, Some(*rex), *opcode, *modrm),
            _ => return None,
        };
        if !matches!((prefix, opcode), (0x66, 0xD6) | (0xF3, 0x7E)) || modrm >> 6 != 3 {
            return None;
        }

        let rex = rex.unwrap_or(0);
        let reg = ((rex & 0x04) << 1) | ((modrm >> 3) & 7);
        let rm = ((rex & 0x01) << 3) | (modrm & 7);
        let (destination, source) = if opcode == 0xD6 { (rm, reg) } else { (reg, rm) };
        Some(X86LegacyScalarXmmMovqReplay {
            destination,
            source,
        })
    }
}
