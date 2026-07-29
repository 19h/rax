//! Shared complete-instruction parsing for VEX memory-source replay.

use super::X86InstructionBytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexMemoryFields {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) map: u8,
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
    pub(crate) width_256: bool,
    pub(crate) w: bool,
}

impl X86InstructionBytes {
    /// Parse one complete VEX instruction whose ModR/M r/m operand is memory.
    ///
    /// Only segment/address-size legacy prefixes are accepted before VEX.
    /// The complete ModR/M/SIB/displacement length is checked, so trailing or
    /// truncated bytes fail closed. The parser is O(1) time and O(1) space
    /// because x86 instructions are bounded to 15 bytes.
    pub(crate) fn vex_memory_fields(&self) -> Option<X86VexMemoryFields> {
        let bytes = self.as_slice();
        let mut vex_offset = 0usize;
        while bytes
            .get(vex_offset)
            .is_some_and(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
        {
            vex_offset += 1;
        }

        let (map, p1, opcode_offset, modrm_offset, destination_high, w) =
            match bytes.get(vex_offset) {
                Some(0xC5) => {
                    let p1 = *bytes.get(vex_offset + 1)?;
                    (
                        1,
                        p1,
                        vex_offset + 2,
                        vex_offset + 3,
                        u8::from(p1 & 0x80 == 0) * 8,
                        false,
                    )
                }
                Some(0xC4) => {
                    let p0 = *bytes.get(vex_offset + 1)?;
                    let p1 = *bytes.get(vex_offset + 2)?;
                    let map = p0 & 0x1F;
                    if !matches!(map, 1..=3) {
                        return None;
                    }
                    (
                        map,
                        p1,
                        vex_offset + 3,
                        vex_offset + 4,
                        u8::from(p0 & 0x80 == 0) * 8,
                        p1 & 0x80 != 0,
                    )
                }
                _ => return None,
            };
        let opcode = *bytes.get(opcode_offset)?;
        let modrm = *bytes.get(modrm_offset)?;
        if modrm >> 6 == 3 {
            return None;
        }

        let mode = modrm >> 6;
        let rm = modrm & 7;
        let mut end = modrm_offset + 1;
        if rm == 4 {
            let sib = *bytes.get(end)?;
            end += 1;
            if mode == 0 && sib & 7 == 5 {
                end += 4;
            }
        } else if mode == 0 && rm == 5 {
            end += 4;
        }
        end += match mode {
            1 => 1,
            2 => 4,
            _ => 0,
        };
        if end != bytes.len() {
            return None;
        }

        Some(X86VexMemoryFields {
            destination: destination_high | ((modrm >> 3) & 7),
            source1: (!p1 >> 3) & 0x0F,
            map,
            pp: p1 & 0x03,
            opcode,
            width_256: p1 & 0x04 != 0,
            w,
        })
    }

    /// Parse one complete VEX memory-source instruction followed by an imm8.
    ///
    /// The non-immediate prefix is validated by [`Self::vex_memory_fields`],
    /// so a missing displacement, an extra byte, or a register ModR/M shape
    /// still fails closed. Runtime is O(1) and auxiliary space is O(1) because
    /// architectural x86 instructions are bounded to 15 bytes.
    pub(crate) fn vex_memory_fields_with_imm8(&self) -> Option<(X86VexMemoryFields, u8)> {
        let (immediate, instruction) = self.as_slice().split_last()?;
        let instruction = X86InstructionBytes::new(instruction)?;
        Some((instruction.vex_memory_fields()?, *immediate))
    }
}
