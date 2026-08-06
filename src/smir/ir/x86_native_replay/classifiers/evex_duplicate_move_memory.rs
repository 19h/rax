//! EVEX VMOVSLDUP/VMOVSHDUP/VMOVDDUP memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Exact EVEX duplicate-move memory encoding and its byte-validated
/// register-source replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexDuplicateMoveMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) high: bool,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) memory_size: u32,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one complete EVEX VMOVSLDUP/VMOVSHDUP/VMOVDDUP memory source
    /// and synthesize its exact register-source replay.
    ///
    /// Intel SDM revision 092 assigns VMOVSHDUP/VMOVSLDUP Type E4NF.nb and
    /// VMOVDDUP Type E5NF semantics. Their memory tuple is therefore read
    /// irrespective of the destination writemask: 8 bytes for EVEX.128
    /// VMOVDDUP and 16/32/64 bytes for every other width/kind. The native
    /// replay retains destination masking and zeroing exactly, while segment,
    /// address-size, and APX B4/X4 controls remain helper-owned.
    pub(crate) fn evex_duplicate_move_memory_encoding(
        &self,
    ) -> Option<X86EvexDuplicateMoveMemoryEncoding> {
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
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (elem, high) = match (opcode, pp, w) {
            (0x12, 2, false) => (VecElementType::F32, false),
            (0x16, 2, false) => (VecElementType::F32, true),
            (0x12, 3, true) => (VecElementType::F64, false),
            _ => return None,
        };
        let ll = (p2 >> 5) & 0x03;
        let width = match ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => return None,
        };
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;

        // Map 0F, reserved vvvv/V'=11111b, b=0, and a complete memory
        // ModR/M operand are invariant. P0.B4 and P1.X4 remain admissible APX
        // address extensions and are removed only from the register replay.
        if p0 & 0x07 != 1
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || p2 & 0x10 != 0
            || modrm >> 6 == 3
            || (zeroing && mask == 0)
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
        {
            return None;
        }

        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination)
            .expect("one destination cannot consume every low vector register");
        let needs_avx512vl = width != VecWidth::V512;
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve destination R/R' and map 0F. Register EVEX.X/B select
            // scratch bits 4/3 with inverted polarity; scratch is below 16.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve fixed W/pp and reserved vvvv, and restore ordinary U.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa; register replay clears EVEX.b.
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ])?;
        if register_instruction.evex_register_lane_shuffle_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        let memory_size = if elem == VecElementType::F64 && width == VecWidth::V128 {
            8
        } else {
            width.bytes()
        };
        Some(X86EvexDuplicateMoveMemoryEncoding {
            width,
            elem,
            high,
            destination,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            memory_size,
            scratch,
            register_instruction,
            needs_avx512vl,
        })
    }
}
