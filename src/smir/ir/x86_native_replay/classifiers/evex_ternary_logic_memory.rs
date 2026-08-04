//! EVEX VPTERNLOGD/Q memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact VPTERNLOGD/Q memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexTernaryLogicMemoryReplay {
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

/// Exact VPTERNLOGD/Q memory encoding and its byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexTernaryLogicMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source2: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) replay: X86EvexTernaryLogicMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source2: u8,
    source3: u8,
    writemask: Option<u8>,
    zeroing: bool,
    immediate: u8,
}

impl X86InstructionBytes {
    fn evex_register_ternary_logic_fields(&self) -> Option<RegisterFields> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let modrm = bytes[5];
        let mask = p2 & 0x07;
        if p0 & 0x0F != 3
            || p1 & 0x07 != 5
            || bytes[4] != 0x25
            || modrm >> 6 != 3
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
        {
            return None;
        }
        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        Some(RegisterFields {
            width,
            elem: if p1 & 0x80 == 0 {
                VecElementType::I32
            } else {
                VecElementType::I64
            },
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            source2: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            source3: (modrm & 7)
                | (u8::from(p0 & 0x20 == 0) << 3)
                | (u8::from(p0 & 0x40 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            zeroing: p2 & 0x80 != 0,
            immediate: bytes[6],
        })
    }

    /// Validate one packed AVX-512 ternary-logic memory source and select an
    /// exact helper-backed native replay.
    ///
    /// VPTERNLOGD/Q use map 0F3A opcode 25H with mandatory 66H. W selects
    /// 32-/64-bit elements, `L'L` selects 128/256/512 bits, and memory
    /// `EVEX.b=1` selects an m32bcst/m64bcst source. Segment/address-size
    /// prefixes and APX B4/X4 extensions remain confined to helper address
    /// evaluation; the imm8 truth table is preserved exactly by every replay.
    pub(crate) fn evex_ternary_logic_memory_encoding(
        &self,
    ) -> Option<X86EvexTernaryLogicMemoryEncoding> {
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
        if p0 & 0x07 != 3
            || p1 & 0x03 != 1
            || opcode != 0x25
            || modrm >> 6 == 3
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
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
        let elem = if p1 & 0x80 == 0 {
            VecElementType::I32
        } else {
            VecElementType::I64
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source2 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        let needs_avx512vl = width != VecWidth::V512;

        let stack_instruction = || {
            let rewritten = [
                0x62,
                // Preserve R/R' and map, select unextended SIB index/base,
                // and clear APX B4 for the rewritten RSP base.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, b, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
                immediate,
            ];
            X86InstructionBytes::new(&rewritten).unwrap()
        };

        let replay = if broadcast {
            X86EvexTernaryLogicMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexTernaryLogicMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source2)
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
            let expected = RegisterFields {
                width,
                elem,
                destination,
                source2,
                source3: scratch,
                writemask,
                zeroing,
                immediate,
            };
            if register_instruction.evex_register_ternary_logic_fields() != Some(expected) {
                return None;
            }
            X86EvexTernaryLogicMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexTernaryLogicMemoryEncoding {
            width,
            elem,
            destination,
            source2,
            writemask,
            zeroing,
            immediate,
            replay,
            needs_avx512vl,
        })
    }
}
