//! EVEX opmask-selector blend memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact EVEX mask-blend memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexMaskBlendMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original broadcast operation
    /// rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane scalar helper loads accumulated in a nonarchitectural
    /// stack vector, followed by the original selector-mask operation
    /// rewritten to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact V[P]BLENDM* memory encoding and its byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexMaskBlendMemoryEncoding {
    pub(crate) opcode: u8,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) selector: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexMaskBlendMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX VBLENDMPS/PD or VPBLENDMB/MW/MD/MQ memory source and
    /// select an exact helper-backed native replay.
    ///
    /// The opmask is a source selector rather than a destination writemask,
    /// but Type E4 suppresses source-memory accesses for zero selector bits.
    /// Segment/address-size prefixes and APX B4/X4 extensions remain confined
    /// to helper address evaluation.
    pub(crate) fn evex_mask_blend_memory_encoding(&self) -> Option<X86EvexMaskBlendMemoryEncoding> {
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
        let selector = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 0x07 != 2
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x64..=0x66)
            || p2 & 0x60 == 0x60
            || (zeroing && selector == 0)
            || (broadcast && opcode == 0x66)
            || operand_end != bytes.len()
        {
            return None;
        }

        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let elem = match (opcode, p1 & 0x80 != 0) {
            (0x64 | 0x65, false) => VecElementType::I32,
            (0x64 | 0x65, true) => VecElementType::I64,
            (0x66, false) => VecElementType::I8,
            (0x66, true) => VecElementType::I16,
            _ => unreachable!("validated EVEX mask-blend opcode"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | (modrm >> 3) & 7;
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let selector = (selector != 0).then_some(selector);
        let needs_avx512vl = width != VecWidth::V512;

        let stack_instruction = || {
            let rewritten = [
                0x62,
                // Preserve R/R' and the map, select unextended SIB
                // index/base, and clear APX B4 for the rewritten RSP base.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, b, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ];
            X86InstructionBytes::new(&rewritten).unwrap()
        };

        let replay = if broadcast {
            X86EvexMaskBlendMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if selector.is_some() {
            X86EvexMaskBlendMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source1)
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
            ];
            let register_instruction = X86InstructionBytes::new(&rewritten).unwrap();
            if register_instruction.evex_register_mask_blend_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexMaskBlendMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexMaskBlendMemoryEncoding {
            opcode,
            width,
            elem,
            destination,
            source1,
            selector,
            zeroing,
            replay,
            needs_avx512vl,
        })
    }
}
