//! EVEX packed funnel-shift register and memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact VPSHLD*/VPSHRD* memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFunnelShiftMemoryReplay {
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
    /// stack vector, followed by the original writemasked operation rewritten
    /// to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX packed funnel-shift memory encoding and its byte-validated
/// native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedFunnelShiftMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    /// EVEX.vvvv architectural source. Immediate forms use it as `src`;
    /// variable forms use it as `fill`.
    pub(crate) source: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) left: bool,
    pub(crate) immediate: Option<u8>,
    pub(crate) replay: X86EvexPackedFunnelShiftMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

fn funnel_element(opcode: u8, w: bool) -> Option<VecElementType> {
    match (opcode & 1, w) {
        (0, true) => Some(VecElementType::I16),
        (1, false) => Some(VecElementType::I32),
        (1, true) => Some(VecElementType::I64),
        _ => None,
    }
}

impl X86InstructionBytes {
    /// Validate a register-only AVX-512 VBMI2 packed funnel shift and return
    /// whether its destination vector length additionally requires AVX-512VL.
    ///
    /// Immediate forms use map 0F3A and variable-count forms use map 0F38;
    /// opcodes 70H/71H shift left and 72H/73H shift right. Even opcodes are
    /// W1 word forms, while odd opcodes use W0/W1 for doublewords/quadwords.
    pub fn evex_register_packed_funnel_shift_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let map = p0 & 0x07;
        let immediate = map == 3 && bytes.len() == 7;
        let variable = map == 2 && bytes.len() == 6;
        if (!immediate && !variable)
            || p0 & 0x08 != 0
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || modrm >> 6 != 3
            || !matches!(opcode, 0x70..=0x73)
            || funnel_element(opcode, p1 & 0x80 != 0).is_none()
        {
            return None;
        }
        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        if p2 & 0x10 != 0 || (zeroing && mask == 0) {
            return None;
        }
        match (p2 >> 5) & 3 {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate one packed AVX-512 VBMI2 word/doubleword/quadword funnel-shift
    /// memory source and select an exact helper-backed native replay.
    ///
    /// Segment/address-size prefixes and APX B4/X4 extensions remain confined
    /// to helper address evaluation. Memory `EVEX.b=1` is valid only for
    /// doubleword/quadword scalar broadcasts.
    pub(crate) fn evex_packed_funnel_shift_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFunnelShiftMemoryEncoding> {
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
        let map = p0 & 0x07;
        let immediate = match map {
            2 => None,
            3 => Some(*bytes.get(operand_end)?),
            _ => return None,
        };
        let expected_end = operand_end + usize::from(immediate.is_some());
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        let elem = funnel_element(opcode, p1 & 0x80 != 0)?;
        if p1 & 0x03 != 1
            || !matches!(opcode, 0x70..=0x73)
            || p2 & 0x60 == 0x60
            || (zeroing && mask == 0)
            || (broadcast && elem == VecElementType::I16)
            || expected_end != bytes.len()
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
        let source = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let needs_avx512vl = width != VecWidth::V512;

        let stack_instruction = || {
            let mut rewritten = [0u8; 8];
            rewritten[..7].copy_from_slice(&[
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
            ]);
            if let Some(amount) = immediate {
                rewritten[7] = amount;
            }
            X86InstructionBytes::new(&rewritten[..7 + usize::from(immediate.is_some())]).unwrap()
        };

        let replay = if broadcast {
            X86EvexPackedFunnelShiftMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexPackedFunnelShiftMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source)
                .expect("two operands cannot consume every low vector register");
            let mut rewritten = [0u8; 7];
            rewritten[..6].copy_from_slice(&[
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Clear APX B4 and retain destination extensions.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ]);
            if let Some(amount) = immediate {
                rewritten[6] = amount;
            }
            let register_instruction =
                X86InstructionBytes::new(&rewritten[..6 + usize::from(immediate.is_some())])
                    .unwrap();
            if register_instruction.evex_register_packed_funnel_shift_needs_vl()
                != Some(needs_avx512vl)
            {
                return None;
            }
            X86EvexPackedFunnelShiftMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedFunnelShiftMemoryEncoding {
            width,
            elem,
            destination,
            source,
            writemask,
            zeroing,
            left: opcode <= 0x71,
            immediate,
            replay,
            needs_avx512vl,
        })
    }
}
