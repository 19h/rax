//! Exact EVEX VXORPS/VXORPD scalar-broadcast memory classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// One exact EVEX VXORPS/VXORPD scalar-broadcast memory encoding rewritten to
/// consume the equivalent 4/8-byte scalar from `[rsp]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexBroadcastXorMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate an EVEX VXORPS/VXORPD scalar-broadcast memory encoding and
    /// rewrite only its memory operand to `[rsp]`.
    ///
    /// EVEX.0F.W0 57 /r is VXORPS with m32bcst; EVEX.66.0F.W1 57 /r is
    /// VXORPD with m64bcst. L'L selects 128/256/512 bits, `EVEX.b=1` selects
    /// scalar broadcast, and `aaa=001..111` retains merge/zero masking. The
    /// helper evaluates the complete guest effective address, so the rewrite
    /// removes segment/address-size prefixes and extended address bits.
    pub(crate) fn evex_broadcast_xor_memory_encoding(
        &self,
    ) -> Option<X86EvexBroadcastXorMemoryEncoding> {
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
        if p0 & 0x0F != 1
            || p1 & 0x04 == 0
            || opcode != 0x57
            || p2 & 0x10 == 0
            || p2 & 0x60 == 0x60
            || modrm >> 6 == 3
        {
            return None;
        }

        let (elem, memory_width) = match (p1 & 0x03, p1 & 0x80 != 0) {
            (0, false) => (VecElementType::F32, MemWidth::B4),
            (1, true) => (VecElementType::F64, MemWidth::B8),
            _ => return None,
        };
        let writemask = match p2 & 0x07 {
            0 => None,
            index => Some(index),
        };
        let zeroing = p2 & 0x80 != 0;
        if zeroing && writemask.is_none() {
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
        let needs_avx512vl = width != VecWidth::V512;

        let stack_bytes = [
            0x62,
            // Preserve R/R' and map 0F, select unextended SIB index/base, and
            // clear APX B4 because the rewritten base is architectural RSP.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore the ordinary EVEX.U fixed bit.
            p1 | 0x04,
            // Preserve z, L'L, broadcast, V', and aaa exactly.
            p2,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ];
        let stack_instruction = X86InstructionBytes::new(&stack_bytes).unwrap();

        // A register clone independently validates map, W/pp, opcode, vector
        // length, extensions, and mask legality. Only EVEX.b and r/m change.
        let register_bytes = [
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38),
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
        if register_instruction.evex_register_logic_requirements() != Some((needs_avx512vl, true)) {
            return None;
        }

        Some(X86EvexBroadcastXorMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            memory_width,
            stack_instruction,
            needs_avx512vl,
        })
    }
}
