//! EVEX packed integer interleave full-vector memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Exact EVEX VPUNPCK* Full Mem encoding and its byte-validated
/// register-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexIntegerInterleaveMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) high: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
}

fn interleave_shape(opcode: u8, w: bool) -> Option<(VecElementType, bool)> {
    match opcode {
        // Byte/word forms specify WIG.
        0x60 => Some((VecElementType::I8, false)),
        0x61 => Some((VecElementType::I16, false)),
        0x68 => Some((VecElementType::I8, true)),
        0x69 => Some((VecElementType::I16, true)),
        // Doubleword forms require W0; quadword forms require W1.
        0x62 if !w => Some((VecElementType::I32, false)),
        0x6A if !w => Some((VecElementType::I32, true)),
        0x6C if w => Some((VecElementType::I64, false)),
        0x6D if w => Some((VecElementType::I64, true)),
        _ => None,
    }
}

impl X86InstructionBytes {
    /// Validate one EVEX VPUNPCKLBW/LWD/LDQ/LQDQ/HBW/HWD/HDQ/HQDQ Full Mem
    /// source and select an exact helper-backed register replay.
    ///
    /// Intel assigns these forms Type E4NF/E4NF.nb semantics: the complete
    /// 16/32/64-byte vector tuple is read irrespective of the destination
    /// writemask. Segment/address-size prefixes and APX B4/X4 address
    /// extensions remain confined to helper address evaluation.
    pub(crate) fn evex_integer_interleave_memory_encoding(
        &self,
    ) -> Option<X86EvexIntegerInterleaveMemoryEncoding> {
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
        let w = p1 & 0x80 != 0;
        let (elem, high) = interleave_shape(opcode, w)?;
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        if p0 & 0x07 != 1
            || p1 & 0x03 != 1
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || modrm >> 6 == 3
            || (zeroing && mask == 0)
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
        {
            return None;
        }

        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | (modrm >> 3) & 7;
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination && *candidate != source1)
            .expect("two operands cannot consume every low vector register");
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Register EVEX.X/B encode scratch bits 4/3 with inverted
            // polarity. Scratch is low, so B is one; clear APX B4.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve W/vvvv/66 and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa exactly; EVEX.b remains clear.
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ])
        .unwrap();
        let needs_avx512vl = width != VecWidth::V512;
        if register_instruction.evex_register_integer_interleave_needs_vl() != Some(needs_avx512vl)
        {
            return None;
        }

        Some(X86EvexIntegerInterleaveMemoryEncoding {
            width,
            elem,
            high,
            opcode,
            w,
            destination,
            source1,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            scratch,
            register_instruction,
            memory_size: width.bytes(),
            needs_avx512vl,
        })
    }
}
