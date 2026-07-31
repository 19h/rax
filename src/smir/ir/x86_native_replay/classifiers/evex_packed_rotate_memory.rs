//! EVEX packed doubleword/quadword rotate memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact VPROL[DQ]/VPROR[DQ] or
/// VPROLV[DQ]/VPRORV[DQ] memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedRotateMemoryReplay {
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

/// Exact EVEX packed rotate memory encoding and its byte-validated native
/// replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedRotateMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    /// Architectural data source for variable-count forms. Immediate forms
    /// consume memory as their only data source and therefore carry `None`.
    pub(crate) source: Option<u8>,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) left: bool,
    pub(crate) immediate: Option<u8>,
    pub(crate) replay: X86EvexPackedRotateMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one packed AVX-512 doubleword/quadword rotate memory source
    /// and select an exact helper-backed native replay.
    ///
    /// Immediate VPROL[DQ]/VPROR[DQ] use map 0F opcode 72H with ModR/M
    /// extensions `/1` and `/0`; variable VPROLV[DQ]/VPRORV[DQ] use map
    /// 0F38 opcodes 15H and 14H. W selects 32- or 64-bit elements, `L'L`
    /// selects 128/256/512 bits, and memory `EVEX.b=1` selects a scalar
    /// broadcast. Segment/address-size prefixes and APX B4/X4 extensions
    /// remain confined to helper address evaluation.
    pub(crate) fn evex_packed_rotate_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedRotateMemoryEncoding> {
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
        let group = (modrm >> 3) & 0x07;
        let (left, immediate) = match (map, opcode, group) {
            (1, 0x72, 0) => (false, Some(*bytes.get(operand_end)?)),
            (1, 0x72, 1) => (true, Some(*bytes.get(operand_end)?)),
            (2, 0x14, _) => (false, None),
            (2, 0x15, _) => (true, None),
            _ => return None,
        };
        let expected_end = operand_end + usize::from(immediate.is_some());
        let mask = p2 & 0x07;
        if p1 & 0x03 != 1
            || modrm >> 6 == 3
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
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
        let elem = if p1 & 0x80 != 0 {
            VecElementType::I64
        } else {
            VecElementType::I32
        };
        let variable = immediate.is_none();
        let source_or_immediate_destination = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let destination = if variable {
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7)
        } else {
            source_or_immediate_destination
        };
        let source = variable.then_some(source_or_immediate_destination);
        let writemask = (mask != 0).then_some(mask);
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
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
            X86EvexPackedRotateMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexPackedRotateMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| {
                    *candidate != destination && source.is_none_or(|source| *candidate != source)
                })
                .expect("at most two operands cannot consume every low vector register");
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
            if register_instruction.evex_register_packed_rotate_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexPackedRotateMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedRotateMemoryEncoding {
            width,
            elem,
            destination,
            source,
            writemask,
            zeroing,
            left,
            immediate,
            replay,
            needs_avx512vl,
        })
    }
}
