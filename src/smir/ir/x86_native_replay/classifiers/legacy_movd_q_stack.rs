//! Guest-stack-register legacy MMX/SSE2 MOVD/MOVQ replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86X87ControlKind};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

/// Exact architectural fields for one legacy register-only MOVD/MOVQ whose
/// GPR operand is guest RSP or RBP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86LegacyMovdQStackReplay {
    pub(crate) gpr: u8,
    pub(crate) vector: u8,
    pub(crate) width: OpWidth,
    pub(crate) vector_destination: bool,
    pub(crate) mmx: bool,
    pub(crate) hint: X86OpHint,
}

impl X86LegacyMovdQStackReplay {
    pub(crate) fn touches_mmx(self) -> bool {
        self.mmx
    }

    pub(crate) fn gpr_is_destination(self) -> bool {
        !self.vector_destination
    }
}

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn vector(index: u8, mmx: bool) -> VReg {
    VReg::Arch(ArchReg::X86(if mmx {
        X86Reg::Mm(index)
    } else {
        X86Reg::Xmm(index)
    }))
}

/// Validate the complete stable SMIR graph for an admitted stack-GPR legacy
/// MOVD/MOVQ. MMX forms contain an unhinted leading `EnterMmx`; XMM forms
/// contain only the exact `X86MovdQ` operation.
pub(crate) fn x86_legacy_movd_q_stack_shape_matches(
    ops: &[SmirOp],
    replay: X86LegacyMovdQStackReplay,
) -> bool {
    let operation = if replay.mmx {
        let [marker, operation] = ops else {
            return false;
        };
        if marker.guest_pc != operation.guest_pc
            || marker.x86_hint.is_some()
            || !matches!(
                marker.kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                }
            )
        {
            return false;
        }
        operation
    } else {
        let [operation] = ops else {
            return false;
        };
        operation
    };

    let vector = vector(replay.vector, replay.mmx);
    let gpr = gpr(replay.gpr);
    let (expected_dst, expected_src) = if replay.vector_destination {
        (vector, gpr)
    } else {
        (gpr, vector)
    };
    operation.x86_hint == Some(replay.hint)
        && matches!(
            &operation.kind,
            OpKind::X86MovdQ {
                dst,
                src,
                width,
                zero_upper,
            } if *dst == expected_dst
                && *src == expected_src
                && *width == replay.width
                && !*zero_upper
        )
}

impl X86InstructionBytes {
    /// Decode one exact canonical legacy register-only MOVD/MOVQ whose GPR
    /// operand is guest RSP or RBP.
    ///
    /// Prefix-free forms address MM0-MM7; `66H` forms address XMM0-XMM15.
    /// REX.W selects the 64-bit transfer. REX.R extends only the XMM register
    /// and is ignored for MMX, REX.B extends the GPR operand, and REX.X is
    /// ignored for this register form. The optional REX prefix must be final.
    /// Memory forms, non-stack GPRs, LOCK/repeat, duplicate/reordered prefixes,
    /// trailing bytes, REX2, VEX, and EVEX fail closed.
    pub(crate) fn legacy_movd_q_stack_replay(&self) -> Option<X86LegacyMovdQStackReplay> {
        let (xmm, rex, opcode, modrm) = match self.as_slice() {
            [0x0F, opcode @ (0x6E | 0x7E), modrm] => (false, None, *opcode, *modrm),
            [rex @ 0x40..=0x4F, 0x0F, opcode @ (0x6E | 0x7E), modrm] => {
                (false, Some(*rex), *opcode, *modrm)
            }
            [0x66, 0x0F, opcode @ (0x6E | 0x7E), modrm] => (true, None, *opcode, *modrm),
            [0x66, rex @ 0x40..=0x4F, 0x0F, opcode @ (0x6E | 0x7E), modrm] => {
                (true, Some(*rex), *opcode, *modrm)
            }
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }

        let rex = rex.unwrap_or(0);
        let gpr = ((rex & 1) << 3) | (modrm & 7);
        if !matches!(gpr, 4 | 5) {
            return None;
        }
        let encoded_vector = (modrm >> 3) & 7;
        let vector = if xmm {
            ((rex >> 2) & 1) << 3 | encoded_vector
        } else {
            encoded_vector
        };
        Some(X86LegacyMovdQStackReplay {
            gpr,
            vector,
            width: if rex & 8 == 0 {
                OpWidth::W32
            } else {
                OpWidth::W64
            },
            vector_destination: opcode == 0x6E,
            mmx: !xmm,
            hint: X86OpHint::SseOp {
                prefix: if xmm {
                    X86SsePrefix::OpSize
                } else {
                    X86SsePrefix::None
                },
                opcode,
            },
        })
    }

    /// Rewrite a validated guest RSP/RBP GPR operand to RAX while preserving
    /// direction, width, vector index, state plane, and ignored REX bits.
    pub(crate) fn legacy_movd_q_stack_with_gpr_rax(&self) -> Option<Self> {
        self.legacy_movd_q_stack_replay()?;
        let mut rewritten = *self;
        let modrm = usize::from(rewritten.len.checked_sub(1)?);
        rewritten.bytes[modrm] &= !0x07;
        Some(rewritten)
    }
}
