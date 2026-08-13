//! Register-only legacy/VEX/EVEX packed sign/zero-extension replay.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecElementType, VecWidth, X86Reg,
};

/// Decoded architectural operands and element shape of one canonical
/// register-only legacy SSE4.1 packed sign/zero-extension instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyPackedExtendReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) source_element: VecElementType,
    pub(crate) destination_element: VecElementType,
    pub(crate) signed: bool,
}

/// Expected block-wide definition/use counts for one virtual register elided
/// by exact legacy packed-extension replay.
pub(crate) type X86LegacyPackedExtendVirtualRequirement = (VReg, usize, usize);

fn exact_extract(
    op: &SmirOp,
    vector: VReg,
    expected_lane: u8,
    element: VecElementType,
    sign: SignExtend,
) -> Option<VReg> {
    if op.x86_hint.is_some() {
        return None;
    }
    match op.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane,
            elem,
            sign: actual_sign,
        } if vec == vector && lane == expected_lane && elem == element && actual_sign == sign => {
            Some(scalar)
        }
        _ => None,
    }
}

fn exact_zero_scalar(op: &SmirOp) -> Option<VReg> {
    if op.x86_hint.is_some() {
        return None;
    }
    match op.kind {
        OpKind::Mov {
            dst: scalar @ VReg::Virtual(_),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        } => Some(scalar),
        _ => None,
    }
}

fn exact_insert(
    op: &SmirOp,
    vector: VReg,
    scalar: VReg,
    expected_lane: u8,
    element: VecElementType,
) -> bool {
    op.x86_hint.is_none()
        && matches!(
            op.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane,
                elem,
            } if dst == vector
                && vec == vector
                && actual_scalar == scalar
                && lane == expected_lane
                && elem == element
        )
}

/// Validate the complete extract/extend/build/legacy-merge graph emitted for
/// one register-only legacy PMOVSX*/PMOVZX*. Each returned tuple is
/// `(virtual register, definitions, uses)` so the grouping layer proves that
/// no elided temporary escapes this source instruction.
pub(crate) fn x86_legacy_packed_extend_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyPackedExtendReplay,
) -> Option<Vec<X86LegacyPackedExtendVirtualRequirement>> {
    let lanes = VecWidth::V128.lanes(replay.destination_element) as usize;
    if ops.len() != 4 * lanes + 2 {
        return None;
    }

    let source = VReg::Arch(ArchReg::X86(X86Reg::Xmm(replay.source)));
    let destination = VReg::Arch(ArchReg::X86(X86Reg::Xmm(replay.destination)));
    let source_sign = if replay.signed {
        SignExtend::Sign
    } else {
        SignExtend::Zero
    };
    let mut requirements = Vec::with_capacity(2 * lanes + 2);
    let mut extended = Vec::with_capacity(lanes);
    for lane in 0..lanes {
        let scalar = exact_extract(
            &ops[lane],
            source,
            lane as u8,
            replay.source_element,
            source_sign,
        )?;
        requirements.push((scalar, 1, 1));
        extended.push(scalar);
    }

    let zero = exact_zero_scalar(&ops[lanes])?;
    let raw = match ops[lanes + 1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem,
            lanes: actual_lanes,
        } if scalar == zero
            && elem == replay.destination_element
            && usize::from(actual_lanes) == lanes =>
        {
            vector
        }
        _ => return None,
    };
    if ops[lanes + 1].x86_hint.is_some() {
        return None;
    }
    requirements.push((zero, 1, 1));
    requirements.push((raw, lanes + 1, 2 * lanes));

    for lane in 0..lanes {
        if !exact_insert(
            &ops[lanes + 2 + lane],
            raw,
            extended[lane],
            lane as u8,
            replay.destination_element,
        ) {
            return None;
        }
    }

    let result_extract_start = 2 * lanes + 2;
    let destination_insert_start = 3 * lanes + 2;
    for lane in 0..lanes {
        let scalar = exact_extract(
            &ops[result_extract_start + lane],
            raw,
            lane as u8,
            replay.destination_element,
            SignExtend::Zero,
        )?;
        if !exact_insert(
            &ops[destination_insert_start + lane],
            destination,
            scalar,
            lane as u8,
            replay.destination_element,
        ) {
            return None;
        }
        requirements.push((scalar, 1, 1));
    }

    let mut unique = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| unique.insert(*register))
        .then_some(requirements)
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy SSE4.1
    /// PMOVSXBW/BD/BQ/WD/WQ/DQ or PMOVZXBW/BD/BQ/WD/WQ/DQ instruction.
    /// Only mandatory 66H followed by an optional final REX prefix is
    /// accepted. REX.R/B extend the XMM operands; fixed-width REX.W and the
    /// register-form REX.X bit are ignored. Memory, other or reordered
    /// prefixes, VEX/EVEX, REX2, truncation, and trailing bytes fail closed.
    pub(crate) fn legacy_register_packed_extend_replay(
        &self,
    ) -> Option<X86LegacyPackedExtendReplay> {
        let (rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (Some(*rex), tail),
            [0x66, tail @ ..] => (None, tail),
            _ => return None,
        };
        let &[0x0F, 0x38, opcode, modrm] = tail else {
            return None;
        };
        if modrm >> 6 != 3 || !matches!(opcode, 0x20..=0x25 | 0x30..=0x35) {
            return None;
        }

        let (source_element, destination_element) = match opcode & 0x0F {
            0x00 => (VecElementType::I8, VecElementType::I16),
            0x01 => (VecElementType::I8, VecElementType::I32),
            0x02 => (VecElementType::I8, VecElementType::I64),
            0x03 => (VecElementType::I16, VecElementType::I32),
            0x04 => (VecElementType::I16, VecElementType::I64),
            0x05 => (VecElementType::I32, VecElementType::I64),
            _ => unreachable!("legacy packed-extension opcode was validated"),
        };
        let extension = rex.unwrap_or(0);
        Some(X86LegacyPackedExtendReplay {
            destination: ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
            source: (modrm & 7) | ((extension & 0x01) << 3),
            source_element,
            destination_element,
            signed: opcode < 0x30,
        })
    }

    /// Validate one register-only AVX/AVX2 VEX packed sign/zero-extension
    /// instruction and return whether its 256-bit destination requires AVX2.
    ///
    /// This covers VPMOVSXBW/BD/BQ/WD/WQ/DQ and
    /// VPMOVZXBW/BD/BQ/WD/WQ/DQ. Every form uses the three-byte VEX prefix,
    /// map 0F38, mandatory 66, reserved `VEX.vvvv=1111b`, and WIG. `VEX.L=0`
    /// forms require AVX; `VEX.L=1` forms require AVX2. Memory and malformed
    /// byte shapes fail closed.
    pub fn vex_register_packed_extend_needs_avx2(&self) -> Option<bool> {
        let &[0xC4, p0, p1, opcode, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 2
            || p1 & 0x78 != 0x78
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x20..=0x25 | 0x30..=0x35)
            || modrm >> 6 != 3
        {
            return None;
        }
        Some(p1 & 0x04 != 0)
    }

    /// Return the architectural VEX packed-extension destination after exact
    /// validation. The AVX-only state bridge uses this to clear the
    /// destination's state-backed ZMM[511:256] after architectural VEX
    /// upper-zeroing.
    pub(crate) fn vex_packed_extend_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_extend_needs_avx2()?;
        let &[_, p0, _, _, modrm] = self.as_slice() else {
            unreachable!("VEX packed-extension shape was validated")
        };
        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 })
    }

    /// Validate one complete AVX/AVX2 VEX packed sign/zero-extension
    /// instruction whose sole source is memory and return
    /// `(destination, source element, destination element, vector width,
    /// signed, opcode, W)`.
    ///
    /// All twelve forms use map 0F38, mandatory prefix 66H, reserve VEX.vvvv
    /// as encoded `1111b`, and define VEX.W as ignored. The shared parser
    /// validates the complete ModR/M/SIB/displacement shape and permits only
    /// segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_packed_extend_fields(
        &self,
    ) -> Option<(u8, VecElementType, VecElementType, VecWidth, bool, u8, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.source1 != 0 || fields.map != 2 || fields.pp != 1 {
            return None;
        }
        let signed = fields.opcode < 0x30;
        let (source_element, destination_element) = match fields.opcode & 0x0F {
            0x00 => (VecElementType::I8, VecElementType::I16),
            0x01 => (VecElementType::I8, VecElementType::I32),
            0x02 => (VecElementType::I8, VecElementType::I64),
            0x03 => (VecElementType::I16, VecElementType::I32),
            0x04 => (VecElementType::I16, VecElementType::I64),
            0x05 => (VecElementType::I32, VecElementType::I64),
            _ => return None,
        };
        if !matches!(fields.opcode, 0x20..=0x25 | 0x30..=0x35) {
            return None;
        }
        Some((
            fields.destination,
            source_element,
            destination_element,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            signed,
            fields.opcode,
            fields.w,
        ))
    }

    /// Validate register-only EVEX packed sign/zero-extension moves and return
    /// whether the destination vector length requires AVX-512VL. This covers
    /// VPMOVSXBW/BD/BQ/WD/WQ/DQ and VPMOVZXBW/BD/BQ/WD/WQ/DQ. W is ignored
    /// for every form except the fixed-W0 DQ forms. Reserved EVEX.vvvv/V',
    /// EVEX.b, vector length, masking, and memory forms fail closed.
    pub fn evex_register_packed_extend_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Every admitted form uses map 0F38, mandatory 66, reserved
        // EVEX.vvvv=1111b and EVEX.V'=1, and a register ModR/M source.
        if p0 & 0x0F != 2
            || p1 & 0x07 != 0x05
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
            || !matches!(opcode, 0x20..=0x25 | 0x30..=0x35)
        {
            return None;
        }
        if matches!(opcode, 0x25 | 0x35) && p1 & 0x80 != 0 {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }
}
