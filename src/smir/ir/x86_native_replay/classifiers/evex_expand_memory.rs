//! EVEX packed expand memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay selected for one exact VEXPAND*/VPEXPAND* memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexExpandMemoryReplay {
    /// One precise full-vector helper read followed by a register-source
    /// rewrite using a nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// Precise scalar helper reads reconstruct the dense selected prefix on
    /// the native stack before the original masked operation reads `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX packed expand memory encoding and byte-validated replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexExpandMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexExpandMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512vbmi2: bool,
}

impl X86InstructionBytes {
    /// Validate one Type-E4 packed expand memory source and select an exact
    /// helper-backed native replay.
    ///
    /// VEXPANDPS/PD use opcode 88H, VPEXPANDD/Q use 89H, and
    /// VPEXPANDB/W use 62H in map 0F38 with mandatory prefix 66H. W selects
    /// the element width inside each opcode pair. `L'L` selects
    /// 128/256/512 bits; EVEX.b and EVEX.vvvv/V' are reserved. Segment,
    /// address-size, and APX B4/X4 address extensions remain confined to the
    /// precise guest-memory helper and are intentionally absent from the
    /// rewritten register/stack instruction.
    pub(crate) fn evex_expand_memory_encoding(&self) -> Option<X86EvexExpandMemoryEncoding> {
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
            (0x62, false) => VecElementType::I8,
            (0x62, true) => VecElementType::I16,
            (0x88, false) => VecElementType::F32,
            (0x88, true) => VecElementType::F64,
            (0x89, false) => VecElementType::I32,
            (0x89, true) => VecElementType::I64,
            _ => return None,
        };
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 3;
        if p0 & 0x07 != 2
            || p1 & 0x03 != 1
            || p1 & 0x78 != 0x78
            || p2 & 0x10 != 0
            || p2 & 0x08 == 0
            || ll == 3
            || (zeroing && mask == 0)
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
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let writemask = (mask != 0).then_some(mask);

        let replay = if writemask.is_some() {
            let rewritten = [
                0x62,
                // Preserve R/R' and map 0F38, select unextended RSP as the
                // reconstructed source, and clear APX B4/X4 address state.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore ordinary EVEX.U.
                p1 | 0x04,
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ];
            X86EvexExpandMemoryReplay::MaskedVector {
                stack_instruction: X86InstructionBytes::new(&rewritten).unwrap(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination)
                .expect("one destination cannot consume every low vector register");
            let rewritten = [
                0x62,
                // EVEX.X/B encode scratch bits 4/3 with inverted polarity.
                // The selected scratch is low, so X is fixed to encoded one.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ];
            X86EvexExpandMemoryReplay::Vector {
                scratch,
                register_instruction: X86InstructionBytes::new(&rewritten).unwrap(),
            }
        };

        Some(X86EvexExpandMemoryEncoding {
            width,
            elem,
            destination,
            writemask,
            zeroing,
            replay,
            needs_avx512vl: width != VecWidth::V512,
            needs_avx512vbmi2: matches!(elem, VecElementType::I8 | VecElementType::I16),
        })
    }
}
