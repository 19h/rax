//! EVEX packed and scalar FMA3 memory-source replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native source-replay strategy for one exact packed FMA3 memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFma3MemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original broadcast form rewritten
    /// to consume the helper-staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane scalar helper loads accumulated in a nonarchitectural
    /// stack vector, followed by the original writemasked packed operation
    /// rewritten to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX packed FMA3 memory encoding and its byte-validated native replay
/// strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedFma3MemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) replay: X86EvexPackedFma3MemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

/// Exact EVEX scalar FMA3 memory encoding rewritten to consume an equivalent
/// 2/4/8-byte operand from a nonarchitectural host-stack slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarFma3MemoryEncoding {
    pub(crate) hint_width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) stack_instruction: X86InstructionBytes,
}

impl X86InstructionBytes {
    /// Validate one EVEX scalar binary16/binary32/binary64 FMA3 memory
    /// encoding and rewrite only its memory operand to `[rsp]`.
    ///
    /// Intel SDM Vol. 2 assigns binary32/binary64 to map 0F38 with W selecting
    /// the element width, and binary16 to MAP6.W0. All use mandatory prefix
    /// 66H, scalar opcode low nibbles 9H, BH, DH, or FH, and LLIG. The admitted
    /// subset has `EVEX.b=0`; `aaa=000` requires `z=0`, while
    /// `aaa=001..111` retains merge/zero masking. MXCSR supplies the rounding
    /// mode. The rewritten instruction canonicalizes LLIG to L'L=0 and
    /// consumes the helper-staged scalar from a 16-byte host-stack slot.
    /// Segment/address-size prefixes and APX extended address bits are removed
    /// because the helper has already evaluated the complete guest effective
    /// address.
    pub(crate) fn evex_scalar_fma3_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarFma3MemoryEncoding> {
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
        let elem = match (p0 & 0x07, p1 & 0x80 != 0) {
            (2, false) => VecElementType::F32,
            (2, true) => VecElementType::F64,
            (6, false) => VecElementType::F16,
            _ => return None,
        };
        let writemask = match p2 & 0x07 {
            0 => None,
            index => Some(index),
        };
        let zeroing = p2 & 0x80 != 0;
        if p1 & 0x03 != 1
            || p2 & 0x10 != 0
            || (zeroing && writemask.is_none())
            || modrm >> 6 == 3
            || !matches!(
                opcode,
                0x99 | 0x9B | 0x9D | 0x9F | 0xA9 | 0xAB | 0xAD | 0xAF | 0xB9 | 0xBB | 0xBD | 0xBF
            )
        {
            return None;
        }
        if memory_operand_end(bytes, modrm_index)? != bytes.len() {
            return None;
        }

        let hint_width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 | 3 => VecWidth::V512,
            _ => unreachable!("two-bit EVEX vector length"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);

        let stack_bytes = [
            0x62,
            // Preserve R/R' and the map, select unextended SIB index/base, and
            // clear APX B4 because the rewritten base is architectural RSP.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv/pp and restore the ordinary EVEX.U fixed bit.
            p1 | 0x04,
            // Preserve z/V'/aaa while canonicalizing LLIG and b.
            p2 & 0x8F,
            opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ];
        let stack_instruction = X86InstructionBytes::new(&stack_bytes).unwrap();

        // Reuse the independent register-form validator as a second semantic
        // oracle for map/W/opcode/operand-extension fields. Register r/m=0 is
        // arbitrary; only classification, not execution, consumes this clone.
        let register_bytes = [
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2 & 0x8F,
            opcode,
            0xC0 | (modrm & 0x38),
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
        let rewritten_needs_vl = match elem {
            VecElementType::F16 => register_instruction.evex_register_scalar_fp16_fma_needs_vl(),
            VecElementType::F32 | VecElementType::F64 => {
                register_instruction.evex_register_scalar_fma_needs_vl()
            }
            _ => unreachable!("validated EVEX scalar FMA3 element"),
        };
        if rewritten_needs_vl != Some(false) {
            return None;
        }

        Some(X86EvexScalarFma3MemoryEncoding {
            hint_width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            opcode,
            w: p1 & 0x80 != 0,
            stack_instruction,
        })
    }

    /// Validate one EVEX packed binary16/binary32/binary64 FMA3 memory
    /// encoding and select an exact native replay.
    ///
    /// Intel SDM Vol. 2 assigns binary32/binary64 to map 0F38 with W selecting
    /// the element width, and binary16 to MAP6.W0. All use mandatory prefix
    /// 66H, opcode low nibbles 6H, 7H, 8H, AH, CH, or EH, and L'L to select
    /// 128/256/512 bits. With `EVEX.b=0`, an unmasked complete vector helper
    /// load is replayed through a low scratch register, while a writemasked
    /// vector is accumulated from per-active-lane 2/4/8-byte helper loads in a
    /// nonarchitectural stack payload. With `EVEX.b=1`, merge/zero
    /// writemasking is retained: the helper performs at most one scalar
    /// 2/4/8-byte load and the broadcast memory form is rewritten to consume
    /// `[rsp]` with the exact original opmask controls. Every replay retains
    /// MXCSR rounding.
    /// Segment/address-size prefixes and APX extended memory address bits are
    /// consumed only by the helper-computed guest address and are therefore
    /// removed from either rewrite.
    pub(crate) fn evex_packed_fma3_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFma3MemoryEncoding> {
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
        let elem = match (p0 & 0x07, p1 & 0x80 != 0) {
            (2, false) => VecElementType::F32,
            (2, true) => VecElementType::F64,
            (6, false) => VecElementType::F16,
            _ => return None,
        };
        let writemask = match p2 & 0x07 {
            0 => None,
            index => Some(index),
        };
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        if p1 & 0x03 != 1
            || (zeroing && writemask.is_none())
            || p2 & 0x60 == 0x60
            || modrm >> 6 == 3
            || !matches!(
                opcode,
                0x96 | 0x97
                    | 0x98
                    | 0x9A
                    | 0x9C
                    | 0x9E
                    | 0xA6
                    | 0xA7
                    | 0xA8
                    | 0xAA
                    | 0xAC
                    | 0xAE
                    | 0xB6
                    | 0xB7
                    | 0xB8
                    | 0xBA
                    | 0xBC
                    | 0xBE
            )
        {
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
        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and the map, select unextended SIB
                // index/base, and clear APX B4 because the rewritten base is
                // architectural RSP.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, b, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .unwrap()
        };
        let replay = if broadcast {
            X86EvexPackedFma3MemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            // Validate every non-address semantic field through the
            // independent register-form classifier before retaining the exact
            // opmask controls on the stack-memory rewrite.
            let register_probe = X86InstructionBytes::new(&[
                0x62,
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38),
            ])
            .unwrap();
            let rewritten_needs_vl = match elem {
                VecElementType::F16 => register_probe.evex_register_packed_fp16_fma_needs_vl(),
                VecElementType::F32 | VecElementType::F64 => {
                    register_probe.evex_register_packed_fma_needs_vl()
                }
                _ => unreachable!("validated EVEX packed FMA3 element"),
            };
            if rewritten_needs_vl != Some(needs_avx512vl) {
                return None;
            }
            X86EvexPackedFma3MemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source1)
                .expect("two operands cannot consume every low vector register");

            let mut register_bytes = [0x62, p0, p1, p2, opcode, 0];
            // Register-source EVEX.X/B encode scratch bits 4/3 with inverted
            // polarity. Clear APX B4, restore the fixed U bit, and retain R/R',
            // V/V', W, L'L, opcode, and the destination ModR/M field.
            register_bytes[1] =
                (register_bytes[1] & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 };
            register_bytes[2] |= 0x04;
            register_bytes[5] = 0xC0 | (modrm & 0x38) | (scratch & 7);
            let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
            let rewritten_needs_vl = match elem {
                VecElementType::F16 => {
                    register_instruction.evex_register_packed_fp16_fma_needs_vl()
                }
                VecElementType::F32 | VecElementType::F64 => {
                    register_instruction.evex_register_packed_fma_needs_vl()
                }
                _ => unreachable!("validated EVEX packed FMA3 element"),
            };
            if rewritten_needs_vl != Some(needs_avx512vl) {
                return None;
            }
            X86EvexPackedFma3MemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedFma3MemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            opcode,
            w: p1 & 0x80 != 0,
            replay,
            needs_avx512vl,
        })
    }
}
