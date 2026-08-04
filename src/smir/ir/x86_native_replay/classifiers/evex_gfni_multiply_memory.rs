//! EVEX `VGF2P8MULB` memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

/// Native replay strategy for one exact EVEX `VGF2P8MULB` memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexGfniMultiplyMemoryReplay {
    /// One unconditional complete-vector helper load followed by a
    /// register-source rewrite using a nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// Per-active-byte helper loads accumulated in a nonarchitectural stack
    /// vector, followed by the original writemasked operation using `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX `VGF2P8MULB` Full Mem encoding and its byte-validated replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexGfniMultiplyMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexGfniMultiplyMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX `VGF2P8MULB` Full Mem source and select an exact
    /// helper-backed native replay.
    ///
    /// Intel specifies map 0F38, mandatory 66H, W0, opcode CFH, a Full Mem
    /// tuple, byte-granular writemasking, and Type E4 exceptions. EVEX.b is
    /// reserved. Unmasked forms read one complete 16/32/64-byte tuple;
    /// writemasked forms suppress each inactive byte access independently.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_gfni_multiply_memory_encoding(
        &self,
    ) -> Option<X86EvexGfniMultiplyMemoryEncoding> {
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
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        if p0 & 0x07 != 2
            || p1 & 0x83 != 0x01
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || opcode != 0xCF
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
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let needs_avx512vl = width != VecWidth::V512;

        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | (modrm & 0x38),
        ])
        .unwrap();
        if register_probe.evex_register_gfni_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        let replay = if writemask.is_some() {
            let stack_instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F38, select unextended SIB
                // index/base, and clear APX B4 because the base is RSP.
                (p0 & 0x97) | 0x60,
                // Preserve W0/vvvv/66 and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, V', and aaa exactly; b was rejected.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .unwrap();
            X86EvexGfniMultiplyMemoryReplay::MaskedVector { stack_instruction }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source1)
                .expect("two operands cannot consume every low vector register");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Clear APX B4 and retain destination extensions.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ])
            .unwrap();
            if register_instruction.evex_register_gfni_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexGfniMultiplyMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexGfniMultiplyMemoryEncoding {
            width,
            destination,
            source1,
            writemask,
            zeroing,
            replay,
            needs_avx512vl,
        })
    }
}
