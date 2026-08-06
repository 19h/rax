//! EVEX scalar floating-point and integer move memory classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType};

/// Direction of one exact EVEX scalar move memory transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexScalarMoveMemoryKind {
    Load,
    Store,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarMoveMemoryFields {
    kind: X86EvexScalarMoveMemoryKind,
    elem: VecElementType,
    vector: u8,
    writemask: Option<u8>,
    zeroing: bool,
    map: u8,
    pp: u8,
    w: bool,
    ll: u8,
    opcode: u8,
    memory_width: MemWidth,
}

/// Exact EVEX scalar move memory encoding and its byte-validated `[rsp]`
/// replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarMoveMemoryEncoding {
    pub(crate) kind: X86EvexScalarMoveMemoryKind,
    pub(crate) elem: VecElementType,
    pub(crate) vector: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) map: u8,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) ll: u8,
    pub(crate) opcode: u8,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512fp16: bool,
}

fn scalar_move_memory_fields(bytes: &[u8]) -> Option<(u8, u8, u8, u8, ScalarMoveMemoryFields)> {
    let start = vector_legacy_prefix_len(bytes);
    if bytes.get(start) != Some(&0x62) {
        return None;
    }

    let p0 = *bytes.get(start + 1)?;
    let p1 = *bytes.get(start + 2)?;
    let p2 = *bytes.get(start + 3)?;
    let opcode = *bytes.get(start + 4)?;
    let modrm_index = start + 5;
    let modrm = *bytes.get(modrm_index)?;
    let map = p0 & 0x07;
    let pp = p1 & 0x03;
    let w = p1 & 0x80 != 0;
    let (kind, elem) = match (map, pp, w, opcode) {
        (1, 2, false, 0x10) => (X86EvexScalarMoveMemoryKind::Load, VecElementType::F32),
        (1, 2, false, 0x11) => (X86EvexScalarMoveMemoryKind::Store, VecElementType::F32),
        (1, 3, true, 0x10) => (X86EvexScalarMoveMemoryKind::Load, VecElementType::F64),
        (1, 3, true, 0x11) => (X86EvexScalarMoveMemoryKind::Store, VecElementType::F64),
        (5, 2, false, 0x10) => (X86EvexScalarMoveMemoryKind::Load, VecElementType::F16),
        (5, 2, false, 0x11) => (X86EvexScalarMoveMemoryKind::Store, VecElementType::F16),
        (1, 1, false, 0x6E) => (X86EvexScalarMoveMemoryKind::Load, VecElementType::I32),
        (1, 1, false, 0x7E) => (X86EvexScalarMoveMemoryKind::Store, VecElementType::I32),
        (1, 1, true, 0x6E) => (X86EvexScalarMoveMemoryKind::Load, VecElementType::I64),
        (1, 1, true, 0x7E) => (X86EvexScalarMoveMemoryKind::Store, VecElementType::I64),
        (1, 2, true, 0x7E) => (X86EvexScalarMoveMemoryKind::Load, VecElementType::I64),
        (1, 1, true, 0xD6) => (X86EvexScalarMoveMemoryKind::Store, VecElementType::I64),
        (5, 1, _, 0x6E) => (X86EvexScalarMoveMemoryKind::Load, VecElementType::I16),
        (5, 1, _, 0x7E) => (X86EvexScalarMoveMemoryKind::Store, VecElementType::I16),
        _ => return None,
    };
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    let integer = matches!(
        elem,
        VecElementType::I16 | VecElementType::I32 | VecElementType::I64
    );
    if p1 & 0x78 != 0x78
        || p2 & 0x08 == 0
        || p2 & 0x10 != 0
        || ll == 3
        || (integer && (ll != 0 || mask != 0 || zeroing))
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || (kind == X86EvexScalarMoveMemoryKind::Store && zeroing)
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let vector =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    let memory_width = match elem {
        VecElementType::F16 | VecElementType::I16 => MemWidth::B2,
        VecElementType::F32 | VecElementType::I32 => MemWidth::B4,
        VecElementType::F64 | VecElementType::I64 => MemWidth::B8,
        _ => unreachable!("validated scalar move element"),
    };
    Some((
        p0,
        p1,
        p2,
        modrm,
        ScalarMoveMemoryFields {
            kind,
            elem,
            vector,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            map,
            pp,
            w,
            ll,
            opcode,
            memory_width,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one supported EVEX scalar move memory form and synthesize its
    /// exact `[rsp]` replay.
    ///
    /// Intel specifies a Tuple1 Scalar 2/4/8-byte transfer. `VMOVSH`,
    /// `VMOVSS`, and `VMOVSD` may use a writemask; for their loads, only
    /// writemask bit 0 controls the access and an inactive lane merges or
    /// zeroes the low scalar while every destination bit above it is cleared.
    /// For their stores, an inactive bit 0 suppresses the complete memory
    /// access; zeroing is not encoded for a memory destination. `VMOVD`, both
    /// `VMOVQ` aliases, and `VMOVW` are unmasked and fixed at EVEX.128; their
    /// loads zero every destination bit above the transferred integer.
    /// EVEX.vvvv/V' and EVEX.b are reserved throughout. The three defined LLIG
    /// images are retained only for the floating-point forms and do not
    /// require AVX-512VL. Segment/address-size prefixes and APX B4/X4
    /// extensions remain confined to helper address evaluation.
    ///
    /// Classification is O(1) time and O(1) space because x86 instructions
    /// are bounded to 15 bytes.
    pub(crate) fn evex_scalar_move_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarMoveMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = scalar_move_memory_fields(self.as_slice())?;
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and the opcode map, select ordinary unextended
            // SIB index/base, and clear APX B4 for the RSP rewrite.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, LLIG, reserved V', and aaa; b was rejected.
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, rewritten_fields) =
            scalar_move_memory_fields(stack_instruction.as_slice())?;
        if rewritten_fields != fields {
            return None;
        }

        Some(X86EvexScalarMoveMemoryEncoding {
            kind: fields.kind,
            elem: fields.elem,
            vector: fields.vector,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            map: fields.map,
            pp: fields.pp,
            w: fields.w,
            ll: fields.ll,
            opcode: fields.opcode,
            memory_width: fields.memory_width,
            stack_instruction,
            needs_avx512fp16: fields.map == 5,
        })
    }
}
