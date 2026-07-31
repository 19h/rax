//! EVEX packed integer minimum/maximum memory-source classification.

use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use super::{X86EvexIntegerArithmeticMemoryReplay, X86InstructionBytes};
use crate::smir::ir::ops::X86VecMap;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Exact EVEX packed integer minimum/maximum memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexIntegerMinMaxMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) map: X86VecMap,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) minimum: bool,
    pub(crate) signed: bool,
    pub(crate) replay: X86EvexIntegerArithmeticMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

fn integer_minmax_shape(
    map: X86VecMap,
    opcode: u8,
    w: bool,
) -> Option<(VecElementType, bool, bool)> {
    match (map, opcode) {
        (X86VecMap::Map0F, 0xDA) => Some((VecElementType::I8, true, false)),
        (X86VecMap::Map0F, 0xDE) => Some((VecElementType::I8, false, false)),
        (X86VecMap::Map0F, 0xEA) => Some((VecElementType::I16, true, true)),
        (X86VecMap::Map0F, 0xEE) => Some((VecElementType::I16, false, true)),
        (X86VecMap::Map0F38, 0x38) => Some((VecElementType::I8, true, true)),
        (X86VecMap::Map0F38, 0x39) => Some((
            if w {
                VecElementType::I64
            } else {
                VecElementType::I32
            },
            true,
            true,
        )),
        (X86VecMap::Map0F38, 0x3A) => Some((VecElementType::I16, true, false)),
        (X86VecMap::Map0F38, 0x3B) => Some((
            if w {
                VecElementType::I64
            } else {
                VecElementType::I32
            },
            true,
            false,
        )),
        (X86VecMap::Map0F38, 0x3C) => Some((VecElementType::I8, false, true)),
        (X86VecMap::Map0F38, 0x3D) => Some((
            if w {
                VecElementType::I64
            } else {
                VecElementType::I32
            },
            false,
            true,
        )),
        (X86VecMap::Map0F38, 0x3E) => Some((VecElementType::I16, false, false)),
        (X86VecMap::Map0F38, 0x3F) => Some((
            if w {
                VecElementType::I64
            } else {
                VecElementType::I32
            },
            false,
            false,
        )),
        _ => None,
    }
}

impl X86InstructionBytes {
    /// Validate one EVEX packed signed/unsigned integer minimum/maximum memory
    /// source and select an exact helper-backed native replay.
    ///
    /// The 16-instruction family uses Type E4/E4.nb exception semantics:
    /// inactive writemask lanes suppress their corresponding 1/2/4/8-byte
    /// access. Dword/qword forms additionally accept m32bcst/m64bcst.
    /// Segment/address-size prefixes and APX B4/X4 address extensions remain
    /// confined to helper address evaluation.
    pub(crate) fn evex_integer_minmax_memory_encoding(
        &self,
    ) -> Option<X86EvexIntegerMinMaxMemoryEncoding> {
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
        let map = match p0 & 0x07 {
            1 => X86VecMap::Map0F,
            2 => X86VecMap::Map0F38,
            _ => return None,
        };
        let w = p1 & 0x80 != 0;
        let (elem, minimum, signed) = integer_minmax_shape(map, opcode, w)?;
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
                // index/base, and clear APX B4 for the rewritten RSP base.
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

        // Independently validate the operation, W/WIG interpretation,
        // operands, vector length, and writemask through the register-only
        // classifier.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38),
        ])
        .unwrap();
        if register_probe.evex_register_integer_minmax_needs_vl() != Some(needs_avx512vl) {
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
            if register_instruction.evex_register_integer_minmax_needs_vl() != Some(needs_avx512vl)
            {
                return None;
            }
            X86EvexIntegerArithmeticMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexIntegerMinMaxMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            map,
            opcode,
            w,
            minimum,
            signed,
            replay,
            needs_avx512vl,
        })
    }
}
