//! EVEX 128-bit-chunk shuffle memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// Native replay selected for one exact EVEX VSHUFF32X4/VSHUFF64X2 or
/// VSHUFI32X4/VSHUFI64X2 memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexChunkShuffleMemoryReplay {
    /// A complete vector tuple staged in an otherwise unused low vector
    /// register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar broadcast tuple staged in a 16-byte stack slot.
    Broadcast {
        memory_width: MemWidth,
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX 128-bit-chunk shuffle memory encoding and its byte-validated
/// native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexChunkShuffleMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) replay: X86EvexChunkShuffleMemoryReplay,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChunkShuffleFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source1: u8,
    writemask: Option<u8>,
    zeroing: bool,
    immediate: u8,
    broadcast: bool,
}

fn memory_fields(bytes: &[u8]) -> Option<(ChunkShuffleFields, usize, usize)> {
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
    let elem = match (opcode, w) {
        (0x23, false) => VecElementType::F32,
        (0x23, true) => VecElementType::F64,
        (0x43, false) => VecElementType::I32,
        (0x43, true) => VecElementType::I64,
        _ => return None,
    };
    let mask = p2 & 0x07;
    let zeroing = p2 & 0x80 != 0;
    if p0 & 0x07 != 3
        || p1 & 0x03 != 1
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || operand_end.checked_add(1)? != bytes.len()
    {
        return None;
    }
    let width = match (p2 >> 5) & 3 {
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => return None,
    };

    Some((
        ChunkShuffleFields {
            width,
            elem,
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            zeroing,
            immediate,
            broadcast: p2 & 0x10 != 0,
        },
        start,
        modrm_index,
    ))
}

impl X86InstructionBytes {
    /// Validate one EVEX VSHUFF32X4/VSHUFF64X2 or VSHUFI32X4/VSHUFI64X2
    /// full-vector or scalar-broadcast memory source and select an exact
    /// helper-backed native replay.
    ///
    /// Intel assigns these forms Type E4NF semantics: the complete 32/64-byte
    /// tuple or one 4/8-byte broadcast scalar is read irrespective of the
    /// destination writemask. Segment/address-size prefixes and APX B4/X4
    /// address extensions remain confined to helper address evaluation.
    pub(crate) fn evex_chunk_shuffle_memory_encoding(
        &self,
    ) -> Option<X86EvexChunkShuffleMemoryEncoding> {
        let bytes = self.as_slice();
        let (fields, start, modrm_index) = memory_fields(bytes)?;
        let p0 = bytes[start + 1];
        let p1 = bytes[start + 2];
        let p2 = bytes[start + 3];
        let opcode = bytes[start + 4];
        let modrm = bytes[modrm_index];
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination && *candidate != fields.source1)
            .expect("two operands leave a low vector scratch register");
        let needs_avx512vl = fields.width == VecWidth::V256;

        let replay = if fields.broadcast {
            let stack_instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F3A, select unextended RSP, and
                // clear APX B4 because the rewritten address is architectural.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/66 and restore ordinary EVEX.U.
                p1 | 0x04,
                // Preserve z, L'L, broadcast, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
                fields.immediate,
            ])
            .unwrap();
            let (rewritten, _, rewritten_modrm) = memory_fields(stack_instruction.as_slice())?;
            if rewritten != fields || stack_instruction.as_slice()[rewritten_modrm] & 7 != 4 {
                return None;
            }
            X86EvexChunkShuffleMemoryReplay::Broadcast {
                memory_width: match fields.elem {
                    VecElementType::F32 | VecElementType::I32 => MemWidth::B4,
                    VecElementType::F64 | VecElementType::I64 => MemWidth::B8,
                    _ => unreachable!("validated chunk-shuffle element width"),
                },
                stack_instruction,
            }
        } else {
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Scratch is low, so X is one; clear APX B4.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                // Register replay preserves mask control but clears EVEX.b.
                p2 & !0x10,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
                fields.immediate,
            ])
            .unwrap();
            if register_instruction.evex_register_chunk_shuffle_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexChunkShuffleMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        let memory_size = match replay {
            X86EvexChunkShuffleMemoryReplay::Vector { .. } => fields.width.bytes(),
            X86EvexChunkShuffleMemoryReplay::Broadcast { memory_width, .. } => memory_width.bytes(),
        };

        Some(X86EvexChunkShuffleMemoryEncoding {
            width: fields.width,
            elem: fields.elem,
            destination: fields.destination,
            source1: fields.source1,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            immediate: fields.immediate,
            replay,
            memory_size,
            needs_avx512vl,
        })
    }
}
