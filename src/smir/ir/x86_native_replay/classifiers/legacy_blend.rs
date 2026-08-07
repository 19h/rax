//! Exact register-only legacy SSE4.1 blend replay classification and semantic
//! graph validation.

use std::collections::HashSet;

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, OpWidth, SignExtend, SrcOperand, VReg, VecCmpCond, VecElementType, VecWidth, X86Reg,
};

/// Decoded architectural operands of one canonical register-only legacy
/// SSE4.1 blend instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyBlendReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) element: VecElementType,
    pub(crate) immediate: Option<u8>,
}

/// Expected block-wide definition/use counts for one virtual register elided
/// by exact native replay.
pub(crate) type X86LegacyBlendVirtualRequirement = (VReg, usize, usize);

fn is_xmm(register: VReg, expected: u8) -> bool {
    matches!(
        register,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(actual))) if actual == expected
    )
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

fn exact_extract(
    op: &SmirOp,
    vector: VReg,
    expected_lane: u8,
    element: VecElementType,
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
            sign: SignExtend::Zero,
        } if vec == vector && lane == expected_lane && elem == element => Some(scalar),
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

fn exact_destination_insert(
    op: &SmirOp,
    destination: u8,
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
            } if is_xmm(dst, destination)
                && is_xmm(vec, destination)
                && actual_scalar == scalar
                && lane == expected_lane
                && elem == element
        )
}

fn unique_requirements(
    requirements: Vec<X86LegacyBlendVirtualRequirement>,
) -> Option<Vec<X86LegacyBlendVirtualRequirement>> {
    let mut registers = HashSet::with_capacity(requirements.len());
    requirements
        .iter()
        .all(|(register, _, _)| registers.insert(*register))
        .then_some(requirements)
}

fn immediate_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyBlendReplay,
    immediate: u8,
) -> Option<Vec<X86LegacyBlendVirtualRequirement>> {
    let lanes = VecWidth::V128.lanes(replay.element) as usize;
    if ops.len() != 4 * lanes + 3 {
        return None;
    }

    let destination = VReg::Arch(ArchReg::X86(X86Reg::Xmm(replay.destination)));
    let source = VReg::Arch(ArchReg::X86(X86Reg::Xmm(replay.source)));
    let mut requirements = Vec::with_capacity(2 * lanes + 3);
    let mut selected = Vec::with_capacity(lanes);
    for lane in 0..lanes {
        let vector = if immediate >> lane & 1 != 0 {
            source
        } else {
            destination
        };
        let scalar = exact_extract(&ops[lane], vector, lane as u8, replay.element)?;
        requirements.push((scalar, 1, 1));
        selected.push(scalar);
    }

    let zero_scalar = exact_zero_scalar(&ops[lanes])?;
    let output = match ops[lanes + 1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem,
            lanes: actual_lanes,
        } if scalar == zero_scalar
            && elem == replay.element
            && usize::from(actual_lanes) == lanes =>
        {
            vector
        }
        _ => return None,
    };
    if ops[lanes + 1].x86_hint.is_some() {
        return None;
    }
    requirements.push((zero_scalar, 1, 1));
    requirements.push((output, lanes + 1, lanes + 1));

    for lane in 0..lanes {
        if !exact_insert(
            &ops[lanes + 2 + lane],
            output,
            selected[lane],
            lane as u8,
            replay.element,
        ) {
            return None;
        }
    }

    let raw_index = 2 * lanes + 2;
    let raw = match ops[raw_index].kind {
        OpKind::VMov {
            dst: raw @ VReg::Virtual(_),
            src,
            width: VecWidth::V128,
        } if src == output => raw,
        _ => return None,
    };
    if ops[raw_index].x86_hint.is_some() {
        return None;
    }
    requirements.push((raw, 1, lanes));

    for lane in 0..lanes {
        let scalar = exact_extract(&ops[raw_index + 1 + lane], raw, lane as u8, replay.element)?;
        if !exact_destination_insert(
            &ops[raw_index + 1 + lanes + lane],
            replay.destination,
            scalar,
            lane as u8,
            replay.element,
        ) {
            return None;
        }
        requirements.push((scalar, 1, 1));
    }
    unique_requirements(requirements)
}

fn variable_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyBlendReplay,
) -> Option<Vec<X86LegacyBlendVirtualRequirement>> {
    let lanes = VecWidth::V128.lanes(replay.element) as usize;
    if ops.len() != 2 * lanes + 4 {
        return None;
    }

    let zero_scalar = exact_zero_scalar(&ops[0])?;
    let zero_vector = match ops[1].kind {
        OpKind::VBroadcast {
            dst: vector @ VReg::Virtual(_),
            scalar,
            elem,
            lanes: actual_lanes,
        } if scalar == zero_scalar
            && elem == replay.element
            && usize::from(actual_lanes) == lanes =>
        {
            vector
        }
        _ => return None,
    };
    if ops[1].x86_hint.is_some() {
        return None;
    }
    let select = match ops[2].kind {
        OpKind::VCmp {
            dst: select @ VReg::Virtual(_),
            src1,
            src2,
            cond: VecCmpCond::Lt,
            elem,
            lanes: actual_lanes,
        } if is_xmm(src1, 0)
            && src2 == zero_vector
            && elem == replay.element
            && usize::from(actual_lanes) == lanes =>
        {
            select
        }
        _ => return None,
    };
    if ops[2].x86_hint.is_some() {
        return None;
    }
    let raw = match ops[3].kind {
        OpKind::VBitSelect {
            dst: raw @ VReg::Virtual(_),
            mask,
            src_true,
            src_false,
            width: VecWidth::V128,
        } if mask == select
            && is_xmm(src_true, replay.source)
            && is_xmm(src_false, replay.destination) =>
        {
            raw
        }
        _ => return None,
    };
    if ops[3].x86_hint.is_some() {
        return None;
    }

    let mut requirements = Vec::with_capacity(lanes + 4);
    requirements.extend([
        (zero_scalar, 1, 1),
        (zero_vector, 1, 1),
        (select, 1, 1),
        (raw, 1, lanes),
    ]);
    for lane in 0..lanes {
        let scalar = exact_extract(&ops[4 + lane], raw, lane as u8, replay.element)?;
        if !exact_destination_insert(
            &ops[4 + lanes + lane],
            replay.destination,
            scalar,
            lane as u8,
            replay.element,
        ) {
            return None;
        }
        requirements.push((scalar, 1, 1));
    }
    unique_requirements(requirements)
}

/// Validate the complete temporary graph emitted by the legacy SSE4.1 blend
/// lifters. Each returned tuple is `(virtual register, definitions, uses)` so
/// the grouping layer can prove that no elided temporary escapes the source
/// instruction.
pub(crate) fn x86_legacy_blend_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyBlendReplay,
) -> Option<Vec<X86LegacyBlendVirtualRequirement>> {
    match replay.immediate {
        Some(immediate) => immediate_shape_virtual_requirements(ops, replay, immediate),
        None => variable_shape_virtual_requirements(ops, replay),
    }
}

impl X86InstructionBytes {
    /// Decode one exact canonical register-only legacy SSE4.1 blend. Only the
    /// mandatory 66H prefix followed by an optional final REX prefix is
    /// accepted. Memory, other or duplicate/reordered prefixes, VEX/EVEX,
    /// REX2, truncated instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_blend_replay(&self) -> Option<X86LegacyBlendReplay> {
        let (rex, tail) = match self.as_slice() {
            [0x66, rex @ 0x40..=0x4F, tail @ ..] => (Some(*rex), tail),
            [0x66, tail @ ..] => (None, tail),
            _ => return None,
        };
        let extension = rex.unwrap_or(0);
        let decode_registers = |modrm: u8| {
            (modrm >> 6 == 3).then_some((
                ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
                (modrm & 7) | ((extension & 0x01) << 3),
            ))
        };

        let (modrm, element, immediate) = match tail {
            [0x0F, 0x38, 0x10, modrm] => (*modrm, VecElementType::I8, None),
            [0x0F, 0x38, 0x14, modrm] => (*modrm, VecElementType::I32, None),
            [0x0F, 0x38, 0x15, modrm] => (*modrm, VecElementType::I64, None),
            [0x0F, 0x3A, 0x0C, modrm, immediate] => (*modrm, VecElementType::I32, Some(*immediate)),
            [0x0F, 0x3A, 0x0D, modrm, immediate] => (*modrm, VecElementType::I64, Some(*immediate)),
            [0x0F, 0x3A, 0x0E, modrm, immediate] => (*modrm, VecElementType::I16, Some(*immediate)),
            _ => return None,
        };
        let (destination, source) = decode_registers(modrm)?;
        Some(X86LegacyBlendReplay {
            destination,
            source,
            element,
            immediate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMMEDIATE: [(u8, VecElementType); 3] = [
        (0x0C, VecElementType::I32),
        (0x0D, VecElementType::I64),
        (0x0E, VecElementType::I16),
    ];
    const VARIABLE: [(u8, VecElementType); 3] = [
        (0x10, VecElementType::I8),
        (0x14, VecElementType::I32),
        (0x15, VecElementType::I64),
    ];

    fn encoding(map: u8, opcode: u8, rex: Option<u8>, modrm: u8, imm: Option<u8>) -> Vec<u8> {
        let mut bytes = vec![0x66];
        bytes.extend(rex);
        bytes.extend([0x0F, map, opcode, modrm]);
        bytes.extend(imm);
        bytes
    }

    fn expected(
        rex: Option<u8>,
        modrm: u8,
        element: VecElementType,
        immediate: Option<u8>,
    ) -> Option<X86LegacyBlendReplay> {
        (modrm >> 6 == 3).then(|| {
            let extension = rex.unwrap_or(0);
            X86LegacyBlendReplay {
                destination: ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
                source: (modrm & 7) | ((extension & 0x01) << 3),
                element,
                immediate,
            }
        })
    }

    #[test]
    fn classifier_exhaustively_accepts_838_848_safe_register_encodings() {
        let mut classified = 0usize;
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for (opcode, element) in VARIABLE {
                for modrm in u8::MIN..=u8::MAX {
                    let bytes = encoding(0x38, opcode, rex, modrm, None);
                    let actual = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_blend_replay();
                    assert_eq!(actual, expected(rex, modrm, element, None), "{bytes:02X?}");
                    classified += usize::from(actual.is_some());
                }
            }
            for (opcode, element) in IMMEDIATE {
                for reg_rm in 0u8..=0x3F {
                    let modrm = 0xC0 | reg_rm;
                    for immediate in u8::MIN..=u8::MAX {
                        let bytes = encoding(0x3A, opcode, rex, modrm, Some(immediate));
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .legacy_register_blend_replay(),
                            expected(rex, modrm, element, Some(immediate)),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
        assert_eq!(classified, 3 * 17 * 64 + 3 * 17 * 64 * 256);

        // Independently assembled by LLVM 23.
        for (bytes, replay) in [
            (
                &[0x66, 0x0F, 0x3A, 0x0C, 0xCB, 0xA5][..],
                X86LegacyBlendReplay {
                    destination: 1,
                    source: 3,
                    element: VecElementType::I32,
                    immediate: Some(0xA5),
                },
            ),
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x0D, 0xCB, 0x5A][..],
                X86LegacyBlendReplay {
                    destination: 9,
                    source: 11,
                    element: VecElementType::I64,
                    immediate: Some(0x5A),
                },
            ),
            (
                &[0x66, 0x45, 0x0F, 0x3A, 0x0E, 0xF8, 0x81][..],
                X86LegacyBlendReplay {
                    destination: 15,
                    source: 8,
                    element: VecElementType::I16,
                    immediate: Some(0x81),
                },
            ),
            (
                &[0x66, 0x45, 0x0F, 0x38, 0x14, 0xCB][..],
                X86LegacyBlendReplay {
                    destination: 9,
                    source: 11,
                    element: VecElementType::I32,
                    immediate: None,
                },
            ),
            (
                &[0x66, 0x45, 0x0F, 0x38, 0x15, 0xF8][..],
                X86LegacyBlendReplay {
                    destination: 15,
                    source: 8,
                    element: VecElementType::I64,
                    immediate: None,
                },
            ),
            (
                &[0x66, 0x45, 0x0F, 0x38, 0x10, 0xCB][..],
                X86LegacyBlendReplay {
                    destination: 9,
                    source: 11,
                    element: VecElementType::I8,
                    immediate: None,
                },
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .legacy_register_blend_replay(),
                Some(replay),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_rejects_every_prefix_map_opcode_operand_and_length_frontier() {
        let invalid: &[&[u8]] = &[
            &[0x66, 0x0F, 0x3A, 0x0C, 0xCB],
            &[0x66, 0x0F, 0x3A, 0x0C, 0xCB, 0xA5, 0],
            &[0x66, 0x0F, 0x38, 0x14],
            &[0x66, 0x0F, 0x38, 0x14, 0xCB, 0],
            &[0x0F, 0x3A, 0x0C, 0xCB, 0xA5],
            &[0xF2, 0x0F, 0x3A, 0x0C, 0xCB, 0xA5],
            &[0xF3, 0x0F, 0x38, 0x14, 0xCB],
            &[0xF0, 0x66, 0x0F, 0x38, 0x14, 0xCB],
            &[0x67, 0x66, 0x0F, 0x38, 0x14, 0xCB],
            &[0x64, 0x66, 0x0F, 0x38, 0x14, 0xCB],
            &[0x66, 0x66, 0x0F, 0x38, 0x14, 0xCB],
            &[0x48, 0x66, 0x0F, 0x38, 0x14, 0xCB],
            &[0x66, 0xD5, 0x00, 0x0F, 0x38, 0x14, 0xCB],
            &[0xC4, 0xE3, 0x69, 0x0C, 0xCB, 0xA5],
            &[0x62, 0xF3, 0x6D, 0x08, 0x0C, 0xCB, 0xA5],
            &[0x66, 0x0F, 0x39, 0x0C, 0xCB, 0xA5],
            &[0x66, 0x0F, 0x3A, 0x0B, 0xCB, 0xA5],
            &[0x66, 0x0F, 0x3A, 0x0F, 0xCB, 0xA5],
            &[0x66, 0x0F, 0x38, 0x11, 0xCB],
            &[0x66, 0x0F, 0x38, 0x13, 0xCB],
            &[0x66, 0x0F, 0x38, 0x16, 0xCB],
            &[0x66, 0x0F, 0x3A, 0x0C, 0x01, 0xA5],
            &[0x66, 0x0F, 0x38, 0x14, 0x01],
        ];
        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .legacy_register_blend_replay(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
