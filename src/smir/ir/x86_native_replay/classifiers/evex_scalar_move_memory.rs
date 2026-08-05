//! EVEX `VMOVSH`/`VMOVSS`/`VMOVSD` scalar memory classification.

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
    let elem = match (map, pp, w) {
        (1, 2, false) => VecElementType::F32,
        (1, 3, true) => VecElementType::F64,
        (5, 2, false) => VecElementType::F16,
        _ => return None,
    };
    let kind = match opcode {
        0x10 => X86EvexScalarMoveMemoryKind::Load,
        0x11 => X86EvexScalarMoveMemoryKind::Store,
        _ => return None,
    };
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    let ll = (p2 >> 5) & 3;
    if p1 & 0x78 != 0x78
        || p2 & 0x08 == 0
        || p2 & 0x10 != 0
        || ll == 3
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
        VecElementType::F16 => MemWidth::B2,
        VecElementType::F32 => MemWidth::B4,
        VecElementType::F64 => MemWidth::B8,
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
    /// Validate one EVEX `VMOVSH`, `VMOVSS`, or `VMOVSD` memory form and
    /// synthesize its exact `[rsp]` replay.
    ///
    /// Intel specifies a Tuple1 Scalar 2/4/8-byte transfer. For loads, only
    /// writemask bit 0 controls the access and an inactive lane merges or
    /// zeroes the low scalar while every destination bit above it is cleared.
    /// For stores, an inactive bit 0 suppresses the complete memory access;
    /// zeroing is not encoded for a memory destination. EVEX.vvvv/V' and
    /// EVEX.b are reserved. All three defined LLIG images are retained exactly
    /// and do not require AVX-512VL. Segment/address-size prefixes and APX
    /// B4/X4 extensions remain confined to helper address evaluation.
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
            needs_avx512fp16: fields.elem == VecElementType::F16,
        })
    }
}
