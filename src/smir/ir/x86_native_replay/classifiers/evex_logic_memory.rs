//! Exact unmasked EVEX packed-logical full-vector memory classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// Logical operation selected by one byte-validated EVEX memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexLogicMemoryKind {
    And,
    AndNot,
    Or,
    Xor,
}

/// One exact unmasked EVEX packed-logical full-vector memory encoding rewritten
/// to consume an equivalent nonarchitectural vector scratch register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexLogicMemoryEncoding {
    pub(crate) kind: X86EvexLogicMemoryKind,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
}

/// One exact writemasked EVEX packed-logical full-vector memory encoding
/// rewritten to consume the equivalent staged vector from `[rsp]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexMaskedLogicMemoryEncoding {
    pub(crate) kind: X86EvexLogicMemoryKind,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: u8,
    pub(crate) zeroing: bool,
    pub(crate) memory_width: MemWidth,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
}

impl X86InstructionBytes {
    /// Validate one unmasked, nonbroadcast EVEX VAND*/VANDN*/VOR*/VXOR* or
    /// VPAND*/VPANDN*/VPOR*/VPXOR* full-vector memory encoding and rewrite
    /// only its memory operand to an otherwise unused vector register.
    ///
    /// Floating PS/PD forms require AVX-512DQ; integer D/Q forms require
    /// AVX-512F. EVEX.128/256 additionally require AVX-512VL. Writemasking and
    /// scalar broadcast have distinct fault-suppression graphs and therefore
    /// fail closed here.
    pub(crate) fn evex_logic_memory_encoding(&self) -> Option<X86EvexLogicMemoryEncoding> {
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
            || p2 & 0x97 != 0
            || p2 & 0x60 == 0x60
            || modrm >> 6 == 3
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
        {
            return None;
        }

        let kind = match opcode {
            0x54 | 0xDB => X86EvexLogicMemoryKind::And,
            0x55 | 0xDF => X86EvexLogicMemoryKind::AndNot,
            0x56 | 0xEB => X86EvexLogicMemoryKind::Or,
            0x57 | 0xEF => X86EvexLogicMemoryKind::Xor,
            _ => return None,
        };
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (elem, needs_avx512dq) = match opcode {
            0x54..=0x57 => match (pp, w) {
                (0, false) => (VecElementType::F32, true),
                (1, true) => (VecElementType::F64, true),
                _ => return None,
            },
            0xDB | 0xDF | 0xEB | 0xEF if pp == 1 => (
                if w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                false,
            ),
            _ => return None,
        };
        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let scratch = (0..32u8)
            .find(|candidate| *candidate != destination && *candidate != source1)
            .expect("two EVEX operands leave at least thirty scratch registers");

        let register_bytes = [
            0x62,
            (p0 & 0x97)
                | (u8::from(scratch & 0x10 == 0) << 6)
                | (u8::from(scratch & 0x08 == 0) << 5),
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
        let needs_avx512vl = width != VecWidth::V512;
        if register_instruction.evex_register_logic_requirements()
            != Some((needs_avx512vl, needs_avx512dq))
        {
            return None;
        }

        Some(X86EvexLogicMemoryEncoding {
            kind,
            width,
            elem,
            destination,
            source1,
            scratch,
            register_instruction,
            needs_avx512vl,
            needs_avx512dq,
        })
    }

    /// Validate one writemasked, nonbroadcast EVEX VAND*, VANDN*, VOR*,
    /// VXOR*, VPAND*, VPANDN*, VPOR*, or VPXOR* full-vector memory encoding
    /// and rewrite only its memory operand to `[rsp]`.
    ///
    /// The original `aaa=001..111`, merge/zero control, vector length, and
    /// logical operands are retained exactly. Per-lane MMU helpers stage only
    /// architecturally active lanes before this instruction is emitted, so
    /// this classifier excludes both unmasked and scalar-broadcast forms.
    pub(crate) fn evex_masked_logic_memory_encoding(
        &self,
    ) -> Option<X86EvexMaskedLogicMemoryEncoding> {
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
            || p2 & 0x10 != 0
            || p2 & 0x07 == 0
            || p2 & 0x60 == 0x60
            || modrm >> 6 == 3
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
        {
            return None;
        }

        let kind = match opcode {
            0x54 | 0xDB => X86EvexLogicMemoryKind::And,
            0x55 | 0xDF => X86EvexLogicMemoryKind::AndNot,
            0x56 | 0xEB => X86EvexLogicMemoryKind::Or,
            0x57 | 0xEF => X86EvexLogicMemoryKind::Xor,
            _ => return None,
        };
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (elem, memory_width, needs_avx512dq) = match opcode {
            0x54..=0x57 => match (pp, w) {
                (0, false) => (VecElementType::F32, MemWidth::B4, true),
                (1, true) => (VecElementType::F64, MemWidth::B8, true),
                _ => return None,
            },
            0xDB | 0xDF | 0xEB | 0xEF if pp == 1 => {
                if w {
                    (VecElementType::I64, MemWidth::B8, false)
                } else {
                    (VecElementType::I32, MemWidth::B4, false)
                }
            }
            _ => return None,
        };
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
            // Retain R/R' and map 0F; select unextended SIB index/base RSP.
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            // Retain z, L'L, V', and aaa; EVEX.b was rejected above.
            p2,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ];
        let stack_instruction = X86InstructionBytes::new(&stack_bytes).unwrap();

        // A register clone independently revalidates map, W/pp, opcode,
        // vector length, extensions, and mask legality.
        let register_bytes = [
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | (modrm & 0x38),
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
        if register_instruction.evex_register_logic_requirements()
            != Some((needs_avx512vl, needs_avx512dq))
        {
            return None;
        }

        Some(X86EvexMaskedLogicMemoryEncoding {
            kind,
            width,
            elem,
            destination,
            source1,
            writemask: p2 & 0x07,
            zeroing: p2 & 0x80 != 0,
            memory_width,
            stack_instruction,
            needs_avx512vl,
            needs_avx512dq,
        })
    }
}
