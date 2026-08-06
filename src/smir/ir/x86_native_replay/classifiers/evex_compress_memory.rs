//! EVEX packed compress memory-destination classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Exact Type-E4 VCOMPRESS*/VPCOMPRESS* memory destination and its
/// byte-validated private-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexCompressMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) source: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512vbmi2: bool,
}

impl X86InstructionBytes {
    /// Validate one Type-E4 packed compress memory destination and rewrite
    /// only its address operand to unextended `[rsp]`.
    ///
    /// VCOMPRESSPS/PD use opcode 8AH, VPCOMPRESSD/Q use 8BH, and
    /// VPCOMPRESSB/W use 63H in map 0F38 with mandatory prefix 66H. W selects
    /// the element width inside each opcode pair. `L'L` selects
    /// 128/256/512 bits; EVEX.b, EVEX.z, and EVEX.vvvv/V' are reserved for a
    /// memory destination. Segment, address-size, and APX B4/X4 address
    /// extensions remain confined to guest-memory helpers.
    pub(crate) fn evex_compress_memory_encoding(&self) -> Option<X86EvexCompressMemoryEncoding> {
        let bytes = self.as_slice();
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
        let w = p1 & 0x80 != 0;
        let elem = match (opcode, w) {
            (0x63, false) => VecElementType::I8,
            (0x63, true) => VecElementType::I16,
            (0x8A, false) => VecElementType::F32,
            (0x8A, true) => VecElementType::F64,
            (0x8B, false) => VecElementType::I32,
            (0x8B, true) => VecElementType::I64,
            _ => return None,
        };
        let mask = p2 & 0x07;
        let ll = (p2 >> 5) & 3;
        if p0 & 0x07 != 2
            || p1 & 0x03 != 1
            || p1 & 0x78 != 0x78
            || p2 & 0x90 != 0
            || p2 & 0x08 == 0
            || ll == 3
            || operand_end != bytes.len()
        {
            return None;
        }

        let width = match ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let source =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let rewritten = [
            0x62,
            // Preserve R/R' and map 0F38, select unextended RSP, and clear
            // APX B4/X4 address state.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            p2,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ];

        Some(X86EvexCompressMemoryEncoding {
            width,
            elem,
            source,
            writemask: (mask != 0).then_some(mask),
            stack_instruction: X86InstructionBytes::new(&rewritten).unwrap(),
            needs_avx512vl: width != VecWidth::V512,
            needs_avx512vbmi2: matches!(elem, VecElementType::I8 | VecElementType::I16),
        })
    }
}
