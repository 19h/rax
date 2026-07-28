//! AVX VEX FMA3 replay.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

impl X86InstructionBytes {
    /// Validate one complete scalar VEX FMA3 instruction with a memory third
    /// source and return `(destination, source2, opcode, W)`.
    ///
    /// Intel SDM Vol. 2 assigns the scalar binary32/binary64 forms to map
    /// 0F38, mandatory prefix 66H, and opcode low nibbles 9H, BH, DH, or FH.
    /// `VEX.L` is ignored for these forms, so both encoded values are accepted.
    /// The shared memory parser accepts only segment/address-size legacy
    /// prefixes and validates the complete ModR/M/SIB/displacement length.
    pub(crate) fn vex_memory_scalar_fma3_fields(&self) -> Option<(u8, u8, u8, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2
            || fields.pp != 1
            || !matches!(
                fields.opcode,
                0x99 | 0x9B | 0x9D | 0x9F | 0xA9 | 0xAB | 0xAD | 0xAF | 0xB9 | 0xBB | 0xBD | 0xBF
            )
        {
            return None;
        }
        Some((fields.destination, fields.source1, fields.opcode, fields.w))
    }

    /// Validate one complete packed VEX FMA3 instruction with a memory third
    /// source and return `(destination, source2, opcode, width, W)`.
    ///
    /// Intel SDM Vol. 2 assigns the packed binary32/binary64 forms to map
    /// 0F38, mandatory prefix 66H, and opcode low nibbles 6H, 7H, 8H, AH, CH,
    /// or EH. The shared memory parser accepts only segment/address-size
    /// legacy prefixes and validates the complete ModR/M/SIB/displacement
    /// length.
    pub(crate) fn vex_memory_packed_fma3_fields(&self) -> Option<(u8, u8, u8, VecWidth, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2
            || fields.pp != 1
            || !matches!(
                fields.opcode,
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
        Some((
            fields.destination,
            fields.source1,
            fields.opcode,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            fields.w,
        ))
    }

    /// Validate one canonical five-byte register-only VEX FMA3 instruction.
    ///
    /// Intel SDM Vol. 2 assigns opcodes 96H through 9FH, A6H through AFH,
    /// and B6H through BFH in map 0F38 with mandatory 66H. VEX.W selects
    /// binary32/binary64 elements, VEX.L selects 128/256 bits for packed forms
    /// and is ignored for scalar forms, and VEX.vvvv is an unrestricted second
    /// source. R and B extend the destination and third source; X is ignored
    /// for a register ModR/M operand. Memory forms remain excluded so native
    /// replay cannot bypass guest-memory translation or fault handling.
    pub fn is_vex_register_fma3(&self) -> bool {
        let bytes = self.as_slice();
        if bytes.len() != 5 || bytes[0] != 0xC4 {
            return false;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];

        p0 & 0x1F == 2
            && p1 & 0x03 == 1
            && matches!(opcode, 0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF)
            && modrm >> 6 == 3
    }
}
