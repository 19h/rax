//! Exact register-only legacy SHA-NI replay classification and semantic graph
//! validation.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86Sha32Op};
use crate::smir::ir::types::{ArchReg, SignExtend, VReg, VecElementType, X86Reg};

/// Decoded architectural operands of one register-only legacy SHA-NI
/// instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyShaReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) op: X86Sha32Op,
    pub(crate) immediate: u8,
}

fn is_xmm(register: VReg, expected: u8) -> bool {
    matches!(
        register,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(actual))) if actual == expected
    )
}

fn exact_extract(op: &SmirOp, raw: VReg, expected_lane: u8) -> Option<VReg> {
    if op.x86_hint.is_some() {
        return None;
    }
    match &op.kind {
        OpKind::VExtractLane {
            dst: scalar @ VReg::Virtual(_),
            vec,
            lane,
            elem: VecElementType::I32,
            sign: SignExtend::Zero,
        } if *vec == raw && *lane == expected_lane => Some(*scalar),
        _ => None,
    }
}

fn exact_insert(op: &SmirOp, destination: u8, scalar: VReg, expected_lane: u8) -> bool {
    op.x86_hint.is_none()
        && matches!(
            &op.kind,
            OpKind::VInsertLane {
                dst,
                vec,
                scalar: actual_scalar,
                lane,
                elem: VecElementType::I32,
            } if is_xmm(*dst, destination)
                && is_xmm(*vec, destination)
                && *actual_scalar == scalar
                && *lane == expected_lane
        )
}

fn exact_wk(op: X86Sha32Op, wk: Option<VReg>) -> bool {
    match (op, wk) {
        (X86Sha32Op::Sha256Rounds2, Some(register)) => is_xmm(register, 0),
        (X86Sha32Op::Sha256Rounds2, None) => false,
        (_, None) => true,
        (_, Some(_)) => false,
    }
}

/// Validate the exact nine-operation temporary-plus-four-lane merge emitted
/// by the legacy SHA-NI lifter. The returned
/// `(temporary, expected-use-count)` pairs let the grouping layer prove that
/// no elided value escapes this instruction.
pub(crate) fn x86_legacy_sha_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyShaReplay,
) -> Option<[(VReg, usize); 5]> {
    let [
        sha,
        extract0,
        extract1,
        extract2,
        extract3,
        insert0,
        insert1,
        insert2,
        insert3,
    ] = ops
    else {
        return None;
    };
    if sha.x86_hint.is_some() {
        return None;
    }
    let raw = match &sha.kind {
        OpKind::X86Sha32 {
            dst: raw @ VReg::Virtual(_),
            src1,
            src2,
            wk,
            op,
            imm,
        } if *op == replay.op
            && *imm == replay.immediate
            && is_xmm(*src1, replay.destination)
            && is_xmm(*src2, replay.source)
            && exact_wk(replay.op, *wk) =>
        {
            *raw
        }
        _ => return None,
    };

    let lane0 = exact_extract(extract0, raw, 0)?;
    let lane1 = exact_extract(extract1, raw, 1)?;
    let lane2 = exact_extract(extract2, raw, 2)?;
    let lane3 = exact_extract(extract3, raw, 3)?;
    if !exact_insert(insert0, replay.destination, lane0, 0)
        || !exact_insert(insert1, replay.destination, lane1, 1)
        || !exact_insert(insert2, replay.destination, lane2, 2)
        || !exact_insert(insert3, replay.destination, lane3, 3)
    {
        return None;
    }
    Some([(raw, 4), (lane0, 1), (lane1, 1), (lane2, 1), (lane3, 1)])
}

impl X86InstructionBytes {
    /// Decode an exact register-only legacy SHA-NI instruction. The specified
    /// no-mandatory-prefix encoding may carry at most one inert FS, GS, or
    /// address-size prefix followed by an optional final REX prefix. Memory,
    /// 66/F2/F3/LOCK, VEX/EVEX, REX2, duplicate/reordered prefixes, truncated
    /// instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_sha_replay(&self) -> Option<X86LegacyShaReplay> {
        let bytes = self.as_slice();
        let mut cursor = usize::from(
            bytes
                .first()
                .is_some_and(|byte| matches!(*byte, 0x64 | 0x65 | 0x67)),
        );
        let rex = bytes
            .get(cursor)
            .copied()
            .filter(|byte| (0x40..=0x4F).contains(byte));
        cursor += usize::from(rex.is_some());
        let tail = bytes.get(cursor..)?;
        let extension = rex.unwrap_or(0);
        let decode_registers = |modrm: u8| {
            (modrm >> 6 == 3).then_some((
                ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
                (modrm & 7) | ((extension & 0x01) << 3),
            ))
        };

        let (destination, source, op, immediate) = match tail {
            [0x0F, 0x38, opcode @ 0xC8..=0xCD, modrm] => {
                let (destination, source) = decode_registers(*modrm)?;
                let op = match opcode {
                    0xC8 => X86Sha32Op::Sha1Nexte,
                    0xC9 => X86Sha32Op::Sha1Msg1,
                    0xCA => X86Sha32Op::Sha1Msg2,
                    0xCB => X86Sha32Op::Sha256Rounds2,
                    0xCC => X86Sha32Op::Sha256Msg1,
                    0xCD => X86Sha32Op::Sha256Msg2,
                    _ => unreachable!(),
                };
                (destination, source, op, 0)
            }
            [0x0F, 0x3A, 0xCC, modrm, immediate] => {
                let (destination, source) = decode_registers(*modrm)?;
                (destination, source, X86Sha32Op::Sha1Rounds4, *immediate)
            }
            _ => return None,
        };
        Some(X86LegacyShaReplay {
            destination,
            source,
            op,
            immediate,
        })
    }
}
