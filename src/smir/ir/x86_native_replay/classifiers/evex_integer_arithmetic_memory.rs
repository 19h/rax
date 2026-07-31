//! EVEX packed integer arithmetic and VNNI memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::ops::X86VecMap;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact EVEX packed integer arithmetic memory
/// encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexIntegerArithmeticMemoryReplay {
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

/// Exact EVEX packed integer arithmetic or VNNI memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexIntegerArithmeticMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) map: X86VecMap,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) replay: X86EvexIntegerArithmeticMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86EvexIntegerArithmeticMemoryEncoding {
    pub(crate) fn is_dot_product(self) -> bool {
        self.map == X86VecMap::Map0F38 && matches!(self.opcode, 0x50..=0x53)
    }

    pub(crate) fn is_ifma52(self) -> bool {
        self.map == X86VecMap::Map0F38 && matches!(self.opcode, 0xB4 | 0xB5)
    }
}

fn integer_arithmetic_elem(map: X86VecMap, opcode: u8, w: bool) -> Option<VecElementType> {
    match (map, opcode, w) {
        (X86VecMap::Map0F, 0xD8 | 0xDC | 0xE8 | 0xEC | 0xF8 | 0xFC, _) => Some(VecElementType::I8),
        (X86VecMap::Map0F, 0xD9 | 0xDD | 0xE9 | 0xED | 0xF9 | 0xFD, _) => Some(VecElementType::I16),
        (X86VecMap::Map0F, 0xE0, _) => Some(VecElementType::I8),
        (X86VecMap::Map0F, 0xE3, _) => Some(VecElementType::I16),
        (X86VecMap::Map0F, 0xFA | 0xFE, false) => Some(VecElementType::I32),
        (X86VecMap::Map0F, 0xD4 | 0xFB, true) => Some(VecElementType::I64),
        (X86VecMap::Map0F38, 0x50..=0x53, false) => Some(VecElementType::I32),
        (X86VecMap::Map0F38, 0xB4 | 0xB5, true) => Some(VecElementType::I64),
        _ => None,
    }
}

fn register_arithmetic_needs_vl(instruction: &X86InstructionBytes) -> Option<bool> {
    instruction
        .evex_register_integer_arithmetic_needs_vl()
        .or_else(|| instruction.evex_register_packed_average_needs_vl())
        .or_else(|| instruction.evex_register_integer_dot_needs_vl())
        .or_else(|| instruction.evex_register_ifma52_needs_vl())
}

impl X86InstructionBytes {
    /// Validate one exact register-only EVEX VPDPBUSD, VPDPBUSDS, VPDPWSSD,
    /// or VPDPWSSDS and return whether its vector length requires AVX-512VL.
    ///
    /// The family uses map 0F38, mandatory prefix 66H, W=0, opcodes 50H
    /// through 53H, and AVX-512VNNI. Memory, EVEX.b, reserved vector lengths,
    /// and malformed masks fail closed.
    pub fn evex_register_integer_dot_needs_vl(&self) -> Option<bool> {
        let [0x62, p0, p1, p2, 0x50..=0x53, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x0F != 2
            || p1 & 0x87 != 0x05
            || modrm >> 6 != 3
            || p2 & 0x10 != 0
            || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        {
            return None;
        }
        match (p2 >> 5) & 3 {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate one exact register-only EVEX VPMADD52LUQ or VPMADD52HUQ and
    /// return whether its vector length requires AVX-512VL.
    ///
    /// The family uses map 0F38, mandatory prefix 66H, W=1, opcodes B4H/B5H,
    /// and AVX-512IFMA. Memory, EVEX.b, reserved vector lengths, and malformed
    /// masks fail closed.
    pub fn evex_register_ifma52_needs_vl(&self) -> Option<bool> {
        let [0x62, p0, p1, p2, 0xB4 | 0xB5, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x0F != 2
            || p1 & 0x87 != 0x85
            || modrm >> 6 != 3
            || p2 & 0x10 != 0
            || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        {
            return None;
        }
        match (p2 >> 5) & 3 {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate one EVEX packed wrapping/saturating integer add/subtract or
    /// rounded unsigned average, integer VNNI dot product, or IFMA52
    /// multiply-add whose source is memory, and select an exact helper-backed
    /// native replay.
    ///
    /// The 24-instruction family uses Type E4/E4.nb exception semantics:
    /// inactive writemask lanes suppress their corresponding 1/2/4/8-byte
    /// access. VPADDD/Q and VPSUBD/Q additionally accept m32bcst/m64bcst;
    /// VPDPBUSD/S and VPDPWSSD/S accept m32bcst; VPMADD52LUQ/HUQ accept
    /// m64bcst; VPAVGB/W use only full-vector memory sources.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_integer_arithmetic_memory_encoding(
        &self,
    ) -> Option<X86EvexIntegerArithmeticMemoryEncoding> {
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
        let map = match p0 & 0x07 {
            1 => X86VecMap::Map0F,
            2 => X86VecMap::Map0F38,
            _ => return None,
        };
        let elem = integer_arithmetic_elem(map, opcode, w)?;
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        let broadcast_allowed = matches!(elem, VecElementType::I32 | VecElementType::I64);
        if p1 & 0x03 != 1
            || modrm >> 6 == 3
            || (zeroing && mask == 0)
            || (broadcast && !broadcast_allowed)
            || p2 & 0x60 == 0x60
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
        let writemask = (mask != 0).then_some(mask);
        let needs_avx512vl = width != VecWidth::V512;

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and the opcode map, select unextended SIB
                // index/base, and clear APX B4 because the rewritten base is
                // RSP.
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

        // Independently validate the operation, W/WIG field, operands,
        // vector length, and writemask through the register-only classifier.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38),
        ])
        .unwrap();
        if register_arithmetic_needs_vl(&register_probe) != Some(needs_avx512vl) {
            return None;
        }

        let replay = if broadcast {
            X86EvexIntegerArithmeticMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexIntegerArithmeticMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
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
            if register_arithmetic_needs_vl(&register_instruction) != Some(needs_avx512vl) {
                return None;
            }
            X86EvexIntegerArithmeticMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexIntegerArithmeticMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            map,
            opcode,
            w,
            replay,
            needs_avx512vl,
        })
    }
}
