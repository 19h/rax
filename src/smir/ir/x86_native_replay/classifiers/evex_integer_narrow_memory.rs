//! EVEX integer-narrowing memory-destination classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth, X86NarrowMode};

/// Exact Type-E6 VPMOV*/VPMOVS*/VPMOVUS* memory destination and its
/// byte-validated unmasked private-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexIntegerNarrowMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) src_elem: VecElementType,
    pub(crate) dst_elem: VecElementType,
    pub(crate) mode: X86NarrowMode,
    pub(crate) source: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512bw: bool,
}

fn operation(opcode: u8) -> Option<(VecElementType, VecElementType, X86NarrowMode)> {
    let mode = match opcode >> 4 {
        1 => X86NarrowMode::UnsignedSaturate,
        2 => X86NarrowMode::SignedSaturate,
        3 => X86NarrowMode::Truncate,
        _ => return None,
    };
    let (src_elem, dst_elem) = match opcode & 0x0F {
        0 => (VecElementType::I16, VecElementType::I8),
        1 => (VecElementType::I32, VecElementType::I8),
        2 => (VecElementType::I64, VecElementType::I8),
        3 => (VecElementType::I32, VecElementType::I16),
        4 => (VecElementType::I64, VecElementType::I16),
        5 => (VecElementType::I64, VecElementType::I32),
        _ => return None,
    };
    Some((src_elem, dst_elem, mode))
}

impl X86InstructionBytes {
    /// Validate one Type-E6 integer-narrowing memory destination and rewrite
    /// only its address to unextended `[rsp]`, clearing the architectural
    /// writemask for the private replay.
    ///
    /// Opcodes 10H..15H, 20H..25H, and 30H..35H select unsigned saturation,
    /// signed saturation, and truncation respectively. The low nibble selects
    /// the 16/32/64-bit source and 8/16/32-bit destination element pair.
    /// Every form uses map 0F38, mandatory prefix F3H, W0, and a reserved
    /// EVEX.vvvv/V'. EVEX.b and EVEX.z are reserved for a memory destination.
    /// Segment, address-size, and APX B4/X4 controls remain confined to guest
    /// helper address evaluation.
    pub(crate) fn evex_integer_narrow_memory_encoding(
        &self,
    ) -> Option<X86EvexIntegerNarrowMemoryEncoding> {
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
        let (src_elem, dst_elem, mode) = operation(opcode)?;
        let ll = (p2 >> 5) & 3;
        if p0 & 0x07 != 2
            || p1 & 0x83 != 2
            || p1 & 0x78 != 0x78
            || p2 & 0x98 != 0x08
            || ll == 3
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
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
        let mask = p2 & 7;
        let rewritten = [
            0x62,
            // Preserve R/R' and map 0F38, select unextended RSP, and clear
            // APX B4/X4 address state.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // The stack replay computes every narrowed lane. Guest helpers
            // apply the architectural mask to fixed destination positions.
            p2 & !0x87,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ];

        Some(X86EvexIntegerNarrowMemoryEncoding {
            width,
            src_elem,
            dst_elem,
            mode,
            source,
            writemask: (mask != 0).then_some(mask),
            stack_instruction: X86InstructionBytes::new(&rewritten).unwrap(),
            needs_avx512vl: width != VecWidth::V512,
            needs_avx512bw: src_elem == VecElementType::I16,
        })
    }
}
