//! EVEX vector-chunk insert memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Exact EVEX VINSERTF*/VINSERTI* memory encoding and its byte-validated
/// helper-backed register replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexChunkInsertMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) chunk_width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChunkInsertFields {
    width: VecWidth,
    chunk_width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source1: u8,
    writemask: Option<u8>,
    zeroing: bool,
    immediate: u8,
    opcode: u8,
    w: bool,
}

fn operation(opcode: u8, w: bool) -> Option<(VecElementType, VecWidth)> {
    Some(match (opcode, w) {
        (0x18, false) => (VecElementType::F32, VecWidth::V128),
        (0x18, true) => (VecElementType::F64, VecWidth::V128),
        (0x1A, false) => (VecElementType::F32, VecWidth::V256),
        (0x1A, true) => (VecElementType::F64, VecWidth::V256),
        (0x38, false) => (VecElementType::I32, VecWidth::V128),
        (0x38, true) => (VecElementType::I64, VecWidth::V128),
        (0x3A, false) => (VecElementType::I32, VecWidth::V256),
        (0x3A, true) => (VecElementType::I64, VecWidth::V256),
        _ => return None,
    })
}

fn memory_fields(bytes: &[u8]) -> Option<(ChunkInsertFields, u8, u8, u8)> {
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
    let operand_end = memory_operand_end(bytes, modrm_index)?;
    let immediate = *bytes.get(operand_end)?;
    let w = p1 & 0x80 != 0;
    let (elem, chunk_width) = operation(opcode, w)?;
    let width = match (p2 >> 5) & 3 {
        1 if chunk_width == VecWidth::V128 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };
    let mask = p2 & 7;
    let zeroing = p2 & 0x80 != 0;
    if p0 & 7 != 3
        || p1 & 3 != 1
        || p2 & 0x10 != 0
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || operand_end.checked_add(1)? != bytes.len()
    {
        return None;
    }
    Some((
        ChunkInsertFields {
            width,
            chunk_width,
            elem,
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            zeroing,
            immediate,
            opcode,
            w,
        },
        p0,
        p1,
        p2,
    ))
}

fn register_rewrite_matches(
    instruction: X86InstructionBytes,
    expected: ChunkInsertFields,
    scratch: u8,
) -> bool {
    let [0x62, p0, p1, p2, opcode, modrm, immediate] = instruction.as_slice() else {
        return false;
    };
    let Some((elem, chunk_width)) = operation(*opcode, p1 & 0x80 != 0) else {
        return false;
    };
    let width = match (p2 >> 5) & 3 {
        1 if chunk_width == VecWidth::V128 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return false,
    };
    let destination =
        (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
    let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
    let source2 = (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4) | (modrm & 7);
    let mask = p2 & 7;
    instruction.evex_register_chunk_insert_requirements()
        == Some((
            width != VecWidth::V512,
            expected.w != (chunk_width == VecWidth::V256),
        ))
        && p0 & 0x0F == 3
        && p1 & 7 == 5
        && p2 & 0x10 == 0
        && modrm >> 6 == 3
        && elem == expected.elem
        && chunk_width == expected.chunk_width
        && width == expected.width
        && destination == expected.destination
        && source1 == expected.source1
        && source2 == scratch
        && (mask != 0).then_some(mask) == expected.writemask
        && (p2 & 0x80 != 0) == expected.zeroing
        && *immediate == expected.immediate
}

impl X86InstructionBytes {
    /// Validate one EVEX VINSERTF32X4/VINSERTF64X2/VINSERTI32X4/
    /// VINSERTI64X2 or VINSERTF32X8/VINSERTF64X4/VINSERTI32X8/
    /// VINSERTI64X4 memory source and select an exact native replay.
    ///
    /// Intel assigns every memory form Type E6NF semantics: the complete
    /// 16/32-byte tuple is read irrespective of the destination writemask.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_chunk_insert_memory_encoding(
        &self,
    ) -> Option<X86EvexChunkInsertMemoryEncoding> {
        let (fields, p0, p1, p2) = memory_fields(self.as_slice())?;
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination && *candidate != fields.source1)
            .expect("two operands leave a low vector scratch register");
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F3A. Register X/B encode scratch bits
            // 4/3 with inverted polarity; clear address-only APX B4.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve W/vvvv/66 and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa; EVEX.b is already rejected.
            p2,
            fields.opcode,
            0xC0 | ((fields.destination & 7) << 3) | (scratch & 7),
            fields.immediate,
        ])?;
        if !register_rewrite_matches(register_instruction, fields, scratch) {
            return None;
        }
        let needs_avx512vl = fields.width != VecWidth::V512;
        let needs_avx512dq = fields.w != (fields.chunk_width == VecWidth::V256);

        Some(X86EvexChunkInsertMemoryEncoding {
            width: fields.width,
            chunk_width: fields.chunk_width,
            elem: fields.elem,
            destination: fields.destination,
            source1: fields.source1,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            immediate: fields.immediate,
            scratch,
            register_instruction,
            memory_size: fields.chunk_width.bytes(),
            needs_avx512vl,
            needs_avx512dq,
        })
    }
}
