//! Exact register-only legacy AES-NI replay classification and semantic graph
//! validation.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    ArchReg, SignExtend, VReg, VecElementType, VecWidth, X86AesOp, X86Reg,
};

/// Decoded architectural operands of one canonical register-only legacy
/// AES-NI instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyAesReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) op: X86AesOp,
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
            elem: VecElementType::I64,
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
                elem: VecElementType::I64,
            } if is_xmm(*dst, destination)
                && is_xmm(*vec, destination)
                && *actual_scalar == scalar
                && *lane == expected_lane
        )
}

/// Validate the exact five-operation temporary-plus-two-lane merge emitted by
/// the legacy AES lifter. The returned `(temporary, expected-use-count)` pairs
/// let the grouping layer prove that no elided value escapes this instruction.
pub(crate) fn x86_legacy_aes_shape_virtual_requirements(
    ops: &[SmirOp],
    replay: X86LegacyAesReplay,
) -> Option<[(VReg, usize); 3]> {
    let [aes, extract0, extract1, insert0, insert1] = ops else {
        return None;
    };
    if aes.x86_hint.is_some() {
        return None;
    }
    let raw = match &aes.kind {
        OpKind::X86Aes {
            dst: raw @ VReg::Virtual(_),
            src1,
            src2,
            width: VecWidth::V128,
            op,
            imm,
        } if *op == replay.op && *imm == replay.immediate => {
            let operands_match = match replay.op {
                X86AesOp::Enc | X86AesOp::EncLast | X86AesOp::Dec | X86AesOp::DecLast => {
                    is_xmm(*src1, replay.destination)
                        && src2.is_some_and(|source| is_xmm(source, replay.source))
                }
                X86AesOp::InvMixColumns | X86AesOp::KeygenAssist => {
                    is_xmm(*src1, replay.source) && src2.is_none()
                }
            };
            operands_match.then_some(*raw)?
        }
        _ => return None,
    };
    let lane0 = exact_extract(extract0, raw, 0)?;
    let lane1 = exact_extract(extract1, raw, 1)?;
    if !exact_insert(insert0, replay.destination, lane0, 0)
        || !exact_insert(insert1, replay.destination, lane1, 1)
    {
        return None;
    }
    Some([(raw, 2), (lane0, 1), (lane1, 1)])
}

impl X86InstructionBytes {
    /// Decode an exact canonical register-only legacy AES-NI instruction.
    /// Only a mandatory 66H prefix followed by an optional final REX prefix is
    /// accepted. Memory, VEX/EVEX, REX2, duplicate/reordered prefixes,
    /// truncated instructions, and trailing bytes fail closed.
    pub(crate) fn legacy_register_aes_replay(&self) -> Option<X86LegacyAesReplay> {
        let bytes = self.as_slice();
        let (rex, tail) = match bytes {
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
        match tail {
            [0x0F, 0x38, opcode @ 0xDB..=0xDF, modrm] => {
                let (destination, source) = decode_registers(*modrm)?;
                let op = match opcode {
                    0xDB => X86AesOp::InvMixColumns,
                    0xDC => X86AesOp::Enc,
                    0xDD => X86AesOp::EncLast,
                    0xDE => X86AesOp::Dec,
                    0xDF => X86AesOp::DecLast,
                    _ => unreachable!(),
                };
                Some(X86LegacyAesReplay {
                    destination,
                    source,
                    op,
                    immediate: 0,
                })
            }
            [0x0F, 0x3A, 0xDF, modrm, immediate] => {
                let (destination, source) = decode_registers(*modrm)?;
                Some(X86LegacyAesReplay {
                    destination,
                    source,
                    op: X86AesOp::KeygenAssist,
                    immediate: *immediate,
                })
            }
            _ => None,
        }
    }
}
