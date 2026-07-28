//! EVEX packed FMA3 memory-source replay classification.

use super::super::X86EvexPackedFma3MemoryEncoding;
use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

fn memory_operand_end(bytes: &[u8], modrm_index: usize) -> Option<usize> {
    let modrm = *bytes.get(modrm_index)?;
    let mode = modrm >> 6;
    let rm = modrm & 7;
    if mode == 3 {
        return None;
    }

    let mut end = modrm_index + 1;
    let sib_base = if rm == 4 {
        let sib = *bytes.get(end)?;
        end += 1;
        Some(sib & 7)
    } else {
        None
    };
    let displacement = match mode {
        0 if rm == 5 || sib_base == Some(5) => 4,
        0 => 0,
        1 => 1,
        2 => 4,
        _ => unreachable!("register mode rejected"),
    };
    end.checked_add(displacement)
        .filter(|operand_end| *operand_end <= bytes.len())
}

fn vector_legacy_prefix_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
        .count()
}

impl X86InstructionBytes {
    /// Validate one unmasked, non-broadcast EVEX packed binary32/binary64
    /// FMA3 memory encoding and rewrite it to an exact register-source
    /// instruction using a low scratch register distinct from both
    /// architectural register operands.
    ///
    /// Intel SDM Vol. 2 assigns these forms to map 0F38, mandatory prefix
    /// 66H, and opcode low nibbles 6H, 7H, 8H, AH, CH, or EH. W selects
    /// binary32/binary64 and L'L selects 128/256/512 bits. The admitted subset
    /// has `EVEX.b=0`, `aaa=000`, and `z=0`, so both the memory and rewritten
    /// register form use MXCSR rounding and update every active-width lane.
    /// Segment/address-size prefixes and APX extended memory address bits are
    /// consumed only by the helper-computed guest address and are therefore
    /// removed from the register-source rewrite.
    pub(crate) fn evex_packed_fma3_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFma3MemoryEncoding> {
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
        if p0 & 0x07 != 2
            || p1 & 0x03 != 1
            || p2 & 0x90 != 0
            || p2 & 0x07 != 0
            || p2 & 0x60 == 0x60
            || modrm >> 6 == 3
            || !matches!(
                opcode,
                0x96 | 0x97
                    | 0x98
                    | 0x9A
                    | 0x9C
                    | 0x9E
                    | 0xA6
                    | 0xA7
                    | 0xA8
                    | 0xAA
                    | 0xAC
                    | 0xAE
                    | 0xB6
                    | 0xB7
                    | 0xB8
                    | 0xBA
                    | 0xBC
                    | 0xBE
            )
        {
            return None;
        }
        if memory_operand_end(bytes, modrm_index)? != bytes.len() {
            return None;
        }

        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination && *candidate != source1)
            .expect("two operands cannot consume every low vector register");

        let mut register_bytes = [0x62, p0, p1, p2, opcode, 0];
        // Register-source EVEX.X/B encode scratch bits 4/3 with inverted
        // polarity. Clear APX B4, restore the fixed U bit, and retain R/R',
        // V/V', W, L'L, opcode, and the destination ModR/M field.
        register_bytes[1] =
            (register_bytes[1] & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 };
        register_bytes[2] |= 0x04;
        register_bytes[5] = 0xC0 | (modrm & 0x38) | (scratch & 7);
        let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
        let needs_avx512vl = width != VecWidth::V512;
        if register_instruction.evex_register_packed_fma_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        Some(X86EvexPackedFma3MemoryEncoding {
            width,
            destination,
            source1,
            scratch,
            opcode,
            w: p1 & 0x80 != 0,
            register_instruction,
            needs_avx512vl,
        })
    }
}
