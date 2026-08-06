//! EVEX.128 high/low 64-bit lane move memory classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HalfMoveMemoryFields {
    destination: u8,
    source1: u8,
    memory_lane: u8,
    w: bool,
    pp: u8,
    opcode: u8,
}

/// Exact EVEX.128 `VMOVLPS`, `VMOVLPD`, `VMOVHPS`, or `VMOVHPD` memory
/// encoding and its byte-validated `[rsp]` replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexHalfMoveMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    /// Destination qword lane populated by the 8-byte memory operand.
    pub(crate) memory_lane: u8,
    pub(crate) w: bool,
    /// `0` selects packed-single naming; `1` selects packed-double naming.
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
    pub(crate) stack_instruction: X86InstructionBytes,
}

/// Exact EVEX.128 `VMOVLPS`, `VMOVLPD`, `VMOVHPS`, or `VMOVHPD` memory-store
/// encoding and its byte-validated `[rsp]` replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexHalfMoveStoreEncoding {
    pub(crate) source: u8,
    /// Source qword lane written by the 8-byte memory operand.
    pub(crate) memory_lane: u8,
    pub(crate) w: bool,
    /// `0` selects packed-single naming; `1` selects packed-double naming.
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
    pub(crate) stack_instruction: X86InstructionBytes,
}

fn half_move_memory_fields(bytes: &[u8]) -> Option<(u8, u8, u8, u8, HalfMoveMemoryFields)> {
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
    let pp = p1 & 3;
    let w = p1 & 0x80 != 0;
    if p0 & 7 != 1
        || !matches!((pp, w), (0, false) | (1, true))
        || p2 & !0x08 != 0
        || !matches!(opcode, 0x12 | 0x13 | 0x16 | 0x17)
        || modrm >> 6 == 3
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    let destination =
        ((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4);
    let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
    Some((
        p0,
        p1,
        p2,
        modrm,
        HalfMoveMemoryFields {
            destination,
            source1,
            memory_lane: u8::from(matches!(opcode, 0x16 | 0x17)),
            w,
            pp,
            opcode,
        },
    ))
}

impl X86InstructionBytes {
    /// Validate one complete EVEX.128 high/low 64-bit lane memory load and
    /// synthesize an exact host-stack replay.
    ///
    /// Intel SDM revision 092 defines opcode 12H as loading destination bits
    /// 63:0 while preserving source1 bits 127:64, and opcode 16H as loading
    /// bits 127:64 while preserving source1 bits 63:0. Each form performs one
    /// unconditional 8-byte Type-E9NF access. Packed-single uses W0/NP;
    /// packed-double uses W1/66. EVEX.L'L, z, b, and aaa are reserved, while
    /// destination and source1 may independently select XMM0-XMM31. The
    /// fixed EVEX.128 forms do not require AVX-512VL.
    ///
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation. Classification is O(1) time and
    /// O(1) space because an x86 instruction is at most 15 bytes.
    pub(crate) fn evex_half_move_memory_encoding(&self) -> Option<X86EvexHalfMoveMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = half_move_memory_fields(self.as_slice())?;
        if !matches!(fields.opcode, 0x12 | 0x16) {
            return None;
        }
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F, select ordinary unextended SIB
            // index/base, and remove APX B4 from the helper-owned address.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U after removing
            // APX X4 from the helper-owned address.
            p1 | 0x04,
            // Preserve the source1 V' extension. Every other P2 field was
            // rejected above.
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, rewritten) = half_move_memory_fields(stack_instruction.as_slice())?;
        if rewritten != fields {
            return None;
        }

        Some(X86EvexHalfMoveMemoryEncoding {
            destination: fields.destination,
            source1: fields.source1,
            memory_lane: fields.memory_lane,
            w: fields.w,
            pp: fields.pp,
            opcode: fields.opcode,
            stack_instruction,
        })
    }

    /// Validate one complete EVEX.128 high/low 64-bit lane memory store and
    /// synthesize its exact host-stack replay.
    ///
    /// Intel SDM revision 092 defines opcode 13H as storing source bits 63:0
    /// and opcode 17H as storing source bits 127:64. Each form performs one
    /// unconditional 8-byte Type-E9NF access. Packed-single uses W0/NP;
    /// packed-double uses W1/66. EVEX.vvvv/V', L'L, z, b, and aaa are
    /// reserved, while the ModR/M.reg source may select XMM0-XMM31. These
    /// fixed EVEX.128 forms do not require AVX-512VL.
    ///
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation. Classification is O(1) time and
    /// O(1) space because an x86 instruction is at most 15 bytes.
    pub(crate) fn evex_half_move_store_encoding(&self) -> Option<X86EvexHalfMoveStoreEncoding> {
        let (p0, p1, p2, modrm, fields) = half_move_memory_fields(self.as_slice())?;
        if !matches!(fields.opcode, 0x13 | 0x17) || fields.source1 != 0 {
            return None;
        }
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F, select ordinary unextended SIB
            // index/base, and remove APX B4 from the helper-owned address.
            (p0 & 0x97) | 0x60,
            // Preserve W/reserved vvvv/pp and restore ordinary EVEX.U after
            // removing APX X4 from the helper-owned address.
            p1 | 0x04,
            // Preserve reserved V'. Every other P2 field was rejected above.
            p2,
            fields.opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, rewritten) = half_move_memory_fields(stack_instruction.as_slice())?;
        if rewritten != fields {
            return None;
        }

        Some(X86EvexHalfMoveStoreEncoding {
            source: fields.destination,
            memory_lane: fields.memory_lane,
            w: fields.w,
            pp: fields.pp,
            opcode: fields.opcode,
            stack_instruction,
        })
    }
}
