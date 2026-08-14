//! Guest-stack-destination legacy SSE and AVX VEX vector sign-mask extracts.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, VecElementType, VecWidth, X86Reg};

/// Architectural fields that must agree between exact MOVMSK source bytes and
/// the stable one-operation SMIR graph replaced by native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86MovMaskStackReplay {
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) elem: VecElementType,
    pub(crate) lanes: u8,
    pub(crate) dst_width: OpWidth,
    pub(crate) vector_width: VecWidth,
    pub(crate) hint: X86OpHint,
    pub(crate) needs_avx2: bool,
}

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V64 | VecWidth::V512 => {
            unreachable!("MOVMSK replay accepts only XMM/YMM sources")
        }
    }))
}

/// Validate the complete stable one-operation graph emitted for an admitted
/// legacy or VEX MOVMSK source instruction. Exact replay is rejected if byte
/// provenance and semantic operands, widths, lane layout, or encoding hint do
/// not agree.
pub(crate) fn x86_mov_mask_stack_shape_matches(
    ops: &[SmirOp],
    replay: X86MovMaskStackReplay,
) -> bool {
    let [operation] = ops else {
        return false;
    };
    operation.x86_hint == Some(replay.hint)
        && matches!(
            &operation.kind,
            OpKind::X86MovMask {
                dst,
                src,
                elem,
                lanes,
                dst_width,
            } if *dst == gpr(replay.destination)
                && *src == vector(replay.source, replay.vector_width)
                && *elem == replay.elem
                && *lanes == replay.lanes
                && *dst_width == replay.dst_width
        )
}

fn opcode_matches_prefix(p1: u8, opcode: u8) -> bool {
    matches!((opcode, p1 & 0x03), (0x50, 0 | 1) | (0xD7, 1))
}

impl X86InstructionBytes {
    /// Decode an exact canonical register-only legacy `MOVMSKPS` or
    /// `MOVMSKPD` whose architectural destination is guest RSP or RBP.
    ///
    /// The optional REX prefix must be final, REX.R must select a low GPR,
    /// REX.B may select XMM8-XMM15, REX.W selects the architectural r64 write,
    /// and ignored REX.X is retained byte-for-byte. Segment and address-size
    /// prefixes are removed by the shared non-memory canonicalizer before this
    /// strict classifier runs. LOCK, repeat, reordered or duplicate prefixes,
    /// memory forms, trailing bytes, REX2, VEX, and EVEX fail closed.
    pub(crate) fn legacy_mov_mask_stack_destination_replay(&self) -> Option<X86MovMaskStackReplay> {
        let (operand_size, rex, modrm) = match self.as_slice() {
            [0x0F, 0x50, modrm] => (false, None, *modrm),
            [rex @ 0x40..=0x4F, 0x0F, 0x50, modrm] => (false, Some(*rex), *modrm),
            [0x66, 0x0F, 0x50, modrm] => (true, None, *modrm),
            [0x66, rex @ 0x40..=0x4F, 0x0F, 0x50, modrm] => (true, Some(*rex), *modrm),
            _ => return None,
        };
        if modrm >> 6 != 3 {
            return None;
        }
        let rex = rex.unwrap_or(0);
        let destination = (((rex >> 2) & 1) << 3) | ((modrm >> 3) & 7);
        if !matches!(destination, 4 | 5) {
            return None;
        }
        let source = ((rex & 1) << 3) | (modrm & 7);
        let (elem, lanes, prefix) = if operand_size {
            (VecElementType::F64, 2, X86SsePrefix::OpSize)
        } else {
            (VecElementType::F32, 4, X86SsePrefix::None)
        };
        Some(X86MovMaskStackReplay {
            destination,
            source,
            elem,
            lanes,
            dst_width: if rex & 8 != 0 {
                OpWidth::W64
            } else {
                OpWidth::W32
            },
            vector_width: VecWidth::V128,
            hint: X86OpHint::SseOp {
                prefix,
                opcode: 0x50,
            },
            needs_avx2: false,
        })
    }

    /// Return the validated legacy guest RSP/RBP destination index.
    pub(crate) fn legacy_mov_mask_stack_destination_index(&self) -> Option<u8> {
        Some(self.legacy_mov_mask_stack_destination_replay()?.destination)
    }

    /// Rewrite a validated legacy guest RSP/RBP destination to RAX while
    /// retaining the mandatory prefix, REX.W/B/X bits, source, and opcode.
    pub(crate) fn legacy_mov_mask_stack_destination_with_destination_rax(&self) -> Option<Self> {
        self.legacy_mov_mask_stack_destination_replay()?;
        let mut rewritten = *self;
        let modrm = usize::from(rewritten.len.checked_sub(1)?);
        rewritten.bytes[modrm] &= !0x38;
        Some(rewritten)
    }

    /// Decode the complete architectural fields of a validated VEX MOVMSK
    /// guest-stack-destination instruction for semantic-graph comparison.
    pub(crate) fn vex_mov_mask_stack_destination_replay(&self) -> Option<X86MovMaskStackReplay> {
        let (p1, opcode, modrm, destination, source, w) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (
                *p1,
                *opcode,
                *modrm,
                (u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
                modrm & 7,
                false,
            ),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (
                *p1,
                *opcode,
                *modrm,
                (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
                (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
                p1 & 0x80 != 0,
            ),
            _ => return None,
        };
        if p1 & 0x78 != 0x78
            || modrm >> 6 != 3
            || !matches!(destination, 4 | 5)
            || !opcode_matches_prefix(p1, opcode)
        {
            return None;
        }
        let width = if p1 & 0x04 == 0 {
            VecWidth::V128
        } else {
            VecWidth::V256
        };
        let (elem, lanes, pp) = match (opcode, p1 & 0x03, width) {
            (0x50, 0, VecWidth::V128) => (VecElementType::F32, 4, X86SsePrefix::None),
            (0x50, 0, VecWidth::V256) => (VecElementType::F32, 8, X86SsePrefix::None),
            (0x50, 1, VecWidth::V128) => (VecElementType::F64, 2, X86SsePrefix::OpSize),
            (0x50, 1, VecWidth::V256) => (VecElementType::F64, 4, X86SsePrefix::OpSize),
            (0xD7, 1, VecWidth::V128) => (VecElementType::I8, 16, X86SsePrefix::OpSize),
            (0xD7, 1, VecWidth::V256) => (VecElementType::I8, 32, X86SsePrefix::OpSize),
            _ => return None,
        };
        Some(X86MovMaskStackReplay {
            destination,
            source,
            elem,
            lanes,
            dst_width: OpWidth::W32,
            vector_width: width,
            hint: X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp,
                opcode,
                width,
                w,
            },
            needs_avx2: opcode == 0xD7 && width == VecWidth::V256,
        })
    }

    /// Validate a register-only VEX `VMOVMSKPS`, `VMOVMSKPD`, or `VPMOVMSKB`
    /// whose architectural r32 destination is guest RSP or RBP.
    ///
    /// The exact replay path is intentionally limited to these two
    /// destinations: every other GPR is already handled by canonical
    /// `X86MovMask` lowering. Both VEX.128 and VEX.256 are valid, VEX.W is
    /// ignored, VEX.vvvv must be encoded as `1111b`, and the source must be
    /// XMM0-XMM15 or YMM0-YMM15. `VPMOVMSKB` requires AVX2 only at 256 bits;
    /// every other admitted form requires AVX.
    pub fn vex_mov_mask_stack_destination_needs_avx2(&self) -> Option<bool> {
        Some(self.vex_mov_mask_stack_destination_replay()?.needs_avx2)
    }

    /// Return the validated guest RSP/RBP destination index.
    pub(crate) fn vex_mov_mask_stack_destination_index(&self) -> Option<u8> {
        Some(self.vex_mov_mask_stack_destination_replay()?.destination)
    }

    /// Rewrite a validated guest RSP/RBP destination to another GPR while
    /// retaining every non-destination bit, including ignored W/X bits.
    pub(crate) fn vex_mov_mask_stack_destination_with_destination(
        &self,
        destination: u8,
    ) -> Option<Self> {
        if destination >= 16 || self.vex_mov_mask_stack_destination_needs_avx2().is_none() {
            return None;
        }

        let mut rewritten = *self;
        match self.as_slice() {
            [0xC5, _p1, _opcode, _modrm] => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[3] = (rewritten.bytes[3] & !0x38) | ((destination & 7) << 3);
            }
            [0xC4, _p0, _p1, _opcode, _modrm] => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[4] = (rewritten.bytes[4] & !0x38) | ((destination & 7) << 3);
            }
            _ => unreachable!("VEX MOVMSK stack-destination shape was validated"),
        }
        Some(rewritten)
    }
}
