//! EVEX VALIGND/Q memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact VALIGND/Q memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexVectorAlignMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// An unconditional scalar helper load followed by the original broadcast
    /// operation rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX VALIGND/Q memory encoding and its byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexVectorAlignMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    /// EVEX.vvvv supplies the high half of the aligned concatenation.
    pub(crate) high: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) replay: X86EvexVectorAlignMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one VALIGND/Q full-vector or scalar-broadcast memory source and
    /// select an exact helper-backed native replay.
    ///
    /// VALIGN memory is exception class E4NF: its read is unconditional even
    /// when every applicable opmask bit is clear. Segment/address-size
    /// prefixes and APX B4/X4 extensions remain confined to helper address
    /// evaluation.
    pub(crate) fn evex_vector_align_memory_encoding(
        &self,
    ) -> Option<X86EvexVectorAlignMemoryEncoding> {
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
        let immediate = *bytes.get(operand_end)?;
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        if p0 & 0x07 != 3
            || p1 & 0x03 != 1
            || opcode != 0x03
            || p2 & 0x60 == 0x60
            || (zeroing && mask == 0)
            || operand_end + 1 != bytes.len()
        {
            return None;
        }

        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let elem = if p1 & 0x80 != 0 {
            VecElementType::I64
        } else {
            VecElementType::I32
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | (modrm >> 3) & 7;
        let high = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let broadcast = p2 & 0x10 != 0;
        let needs_avx512vl = width != VecWidth::V512;

        let replay = if broadcast {
            let rewritten = [
                0x62,
                // Preserve R/R' and the map, select unextended SIB
                // index/base, and clear APX B4 for the rewritten RSP base.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
                immediate,
            ];
            X86EvexVectorAlignMemoryReplay::Broadcast {
                stack_instruction: X86InstructionBytes::new(&rewritten).unwrap(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != high)
                .expect("two operands cannot consume every low vector register");
            let rewritten = [
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Clear APX B4 and retain destination extensions.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
                immediate,
            ];
            let register_instruction = X86InstructionBytes::new(&rewritten).unwrap();
            if register_instruction.evex_register_vector_align_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexVectorAlignMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexVectorAlignMemoryEncoding {
            width,
            elem,
            destination,
            high,
            writemask,
            zeroing,
            immediate,
            replay,
            needs_avx512vl,
        })
    }
}
