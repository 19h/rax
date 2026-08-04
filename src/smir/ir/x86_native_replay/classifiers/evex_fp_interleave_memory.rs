//! EVEX floating-point interleave memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// Native replay selected for one exact EVEX VUNPCKLPS/LPD/HPS/HPD memory
/// source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexFpInterleaveMemoryReplay {
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

/// Exact EVEX VUNPCKLPS/LPD/HPS/HPD memory encoding and its byte-validated
/// native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexFpInterleaveMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) high: bool,
    pub(crate) opcode: u8,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexFpInterleaveMemoryReplay,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX VUNPCKLPS/LPD/HPS/HPD memory source and select an
    /// exact helper-backed native replay.
    ///
    /// Intel assigns these instructions Type E4NF semantics and a Full tuple.
    /// Full-vector sources therefore read 16/32/64 bytes and broadcast sources
    /// read one 4/8-byte scalar irrespective of the destination writemask.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_fp_interleave_memory_encoding(
        &self,
    ) -> Option<X86EvexFpInterleaveMemoryEncoding> {
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
        let pp = p1 & 0x03;
        let elem = match (pp, w) {
            (0, false) => VecElementType::F32,
            (1, true) => VecElementType::F64,
            _ => return None,
        };
        let high = match opcode {
            0x14 => false,
            0x15 => true,
            _ => return None,
        };
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 0x07 != 1
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
            // Preserve W/vvvv/pp and restore ordinary EVEX.U.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa; register replay clears EVEX.b.
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ])
        .unwrap();
        let needs_avx512vl = width != VecWidth::V512;
        if register_instruction.evex_register_fp_shuffle_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        let replay = if broadcast {
            let stack_instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F, select unextended SIB
                // index/base, and clear APX B4 for rewritten RSP.
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                // Preserve z, L'L, broadcast, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .unwrap();
            X86EvexFpInterleaveMemoryReplay::Broadcast {
                memory_width: if elem == VecElementType::F32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                },
                stack_instruction,
            }
        } else {
            X86EvexFpInterleaveMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        let memory_size = match replay {
            X86EvexFpInterleaveMemoryReplay::Vector { .. } => width.bytes(),
            X86EvexFpInterleaveMemoryReplay::Broadcast { memory_width, .. } => memory_width.bytes(),
        };

        Some(X86EvexFpInterleaveMemoryEncoding {
            width,
            elem,
            high,
            opcode,
            destination,
            source1,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            replay,
            memory_size,
            needs_avx512vl,
        })
    }
}
