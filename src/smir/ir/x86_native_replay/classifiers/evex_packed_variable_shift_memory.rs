//! EVEX packed per-element variable-shift memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{ShiftOp, VecElementType, VecWidth};

/// Native replay strategy for one exact VPSLLV*, VPSRAV*, or VPSRLV*
/// memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedVariableShiftMemoryReplay {
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

/// Exact EVEX packed per-element variable-shift memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedVariableShiftMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) shift: ShiftOp,
    pub(crate) destination: u8,
    pub(crate) source: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexPackedVariableShiftMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterFields {
    width: VecWidth,
    elem: VecElementType,
    shift: ShiftOp,
    destination: u8,
    source: u8,
    count: u8,
    writemask: Option<u8>,
    zeroing: bool,
}

fn variable_shift_kind(opcode: u8, w: bool) -> Option<(VecElementType, ShiftOp)> {
    match (opcode, w) {
        (0x10, true) => Some((VecElementType::I16, ShiftOp::Lsr)),
        (0x11, true) => Some((VecElementType::I16, ShiftOp::Asr)),
        (0x12, true) => Some((VecElementType::I16, ShiftOp::Lsl)),
        (0x45, false) => Some((VecElementType::I32, ShiftOp::Lsr)),
        (0x45, true) => Some((VecElementType::I64, ShiftOp::Lsr)),
        (0x46, false) => Some((VecElementType::I32, ShiftOp::Asr)),
        (0x46, true) => Some((VecElementType::I64, ShiftOp::Asr)),
        (0x47, false) => Some((VecElementType::I32, ShiftOp::Lsl)),
        (0x47, true) => Some((VecElementType::I64, ShiftOp::Lsl)),
        _ => None,
    }
}

impl X86InstructionBytes {
    fn evex_register_packed_variable_shift_fields(&self) -> Option<RegisterFields> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let mask = p2 & 0x07;
        if p0 & 0x0F != 2
            || p1 & 0x07 != 5
            || modrm >> 6 != 3
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
        {
            return None;
        }
        let (elem, shift) = variable_shift_kind(opcode, p1 & 0x80 != 0)?;
        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        Some(RegisterFields {
            width,
            elem,
            shift,
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            source: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            count: (modrm & 7) | (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            zeroing: p2 & 0x80 != 0,
        })
    }

    /// Validate one packed AVX-512 per-element variable-shift memory source
    /// and select an exact helper-backed native replay.
    ///
    /// Word forms use map 0F38 opcodes 10H-12H with W=1 and forbid
    /// broadcasting. Doubleword/quadword forms use opcodes 45H-47H with W
    /// selecting 32-/64-bit elements and permit scalar memory broadcast.
    /// Segment/address-size prefixes and APX B4/X4 extensions remain confined
    /// to helper address evaluation.
    pub(crate) fn evex_packed_variable_shift_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedVariableShiftMemoryEncoding> {
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
        let mask = p2 & 0x07;
        if p0 & 0x07 != 2
            || p1 & 0x03 != 1
            || modrm >> 6 == 3
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && mask == 0)
            || operand_end != bytes.len()
        {
            return None;
        }

        let (elem, shift) = variable_shift_kind(opcode, p1 & 0x80 != 0)?;
        let broadcast = p2 & 0x10 != 0;
        if broadcast && elem == VecElementType::I16 {
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
        let source = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let zeroing = p2 & 0x80 != 0;
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
            X86EvexPackedVariableShiftMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexPackedVariableShiftMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source)
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
            let expected = RegisterFields {
                width,
                elem,
                shift,
                destination,
                source,
                count: scratch,
                writemask,
                zeroing,
            };
            if register_instruction.evex_register_packed_variable_shift_fields() != Some(expected) {
                return None;
            }
            X86EvexPackedVariableShiftMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedVariableShiftMemoryEncoding {
            width,
            elem,
            shift,
            destination,
            source,
            writemask,
            zeroing,
            replay,
            needs_avx512vl,
        })
    }
}
