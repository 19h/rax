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
    pub(crate) stack_segment: bool,
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
        let mut segment_override = None;
        while bytes
            .get(vex_offset)
            .is_some_and(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
        {
            if matches!(bytes[vex_offset], 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65) {
                segment_override = Some(bytes[vex_offset]);
            }
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
        let default_stack_segment;
        if rm == 4 {
            let sib = *bytes.get(end)?;
            end += 1;
            let base = sib & 7;
            let has_base = !(mode == 0 && base == 5);
            default_stack_segment = has_base && matches!(base, 4 | 5);
            if !has_base {
                end += 4;
            }
        } else if mode == 0 && rm == 5 {
            default_stack_segment = false;
            end += 4;
        } else {
            default_stack_segment = matches!(rm, 4 | 5);
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
            stack_segment: match segment_override {
                Some(0x36) => true,
                Some(_) => false,
                None => default_stack_segment,
            },
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

    /// Rewrite only the ModR/M memory source of one fully validated VEX
    /// instruction to an architectural vector or GPR register index.
    ///
    /// Segment and address-size prefixes, SIB, and displacement bytes belong
    /// only to the already-evaluated guest address and are omitted. C5 has no
    /// r/m extension channel and therefore accepts only indices 0-7; C4 uses
    /// inverted `VEX.B` for indices 0-15. Every other VEX field is preserved,
    /// including ignored W/X bits. The returned instruction is a complete
    /// register-form byte string with no immediate operand.
    pub(crate) fn vex_memory_with_register_source(&self, source: u8) -> Option<Self> {
        if source >= 16 {
            return None;
        }
        self.vex_memory_fields()?;

        let bytes = self.as_slice();
        let vex_offset = bytes
            .iter()
            .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
            .count();
        match *bytes.get(vex_offset)? {
            0xC5 => {
                if source >= 8 {
                    return None;
                }
                let p1 = *bytes.get(vex_offset + 1)?;
                let opcode = *bytes.get(vex_offset + 2)?;
                let modrm = *bytes.get(vex_offset + 3)?;
                Self::new(&[0xC5, p1, opcode, 0xC0 | (modrm & 0x38) | source])
            }
            0xC4 => {
                let mut p0 = *bytes.get(vex_offset + 1)?;
                let p1 = *bytes.get(vex_offset + 2)?;
                let opcode = *bytes.get(vex_offset + 3)?;
                let modrm = *bytes.get(vex_offset + 4)?;
                if source < 8 {
                    p0 |= 0x20;
                } else {
                    p0 &= !0x20;
                }
                Self::new(&[0xC4, p0, p1, opcode, 0xC0 | (modrm & 0x38) | (source & 7)])
            }
            _ => None,
        }
    }
}
