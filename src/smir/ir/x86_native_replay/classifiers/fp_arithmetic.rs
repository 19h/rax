//! Register-only x86 binary floating-point arithmetic replay classification.

use super::X86InstructionBytes;

const OPCODES: [u8; 6] = [0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

impl X86InstructionBytes {
    /// Validate one complete AVX VEX scalar binary32/binary64 arithmetic
    /// instruction with a memory source and return
    /// `(destination, source1, pp, opcode, W)`.
    ///
    /// This classifier accepts optional x86-64 segment/address-size prefixes,
    /// validates the complete ModR/M/SIB/displacement length, and requires
    /// `VEX.L=0`. Intel documents `VEX.L=1` for every scalar member of this
    /// family as generation-dependent unpredictable behavior.
    pub(crate) fn vex_scalar_memory_fp_arithmetic_fields(&self) -> Option<(u8, u8, u8, u8, bool)> {
        let bytes = self.as_slice();
        let mut vex_offset = 0usize;
        while bytes
            .get(vex_offset)
            .is_some_and(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
        {
            vex_offset += 1;
        }

        let (p1, opcode_offset, modrm_offset, destination_high, w) = match bytes.get(vex_offset) {
            Some(0xC5) => {
                let p1 = *bytes.get(vex_offset + 1)?;
                (
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
                if p0 & 0x1F != 1 {
                    return None;
                }
                (
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
        let pp = p1 & 0x03;
        if !matches!(pp, 2 | 3) || p1 & 0x04 != 0 || !OPCODES.contains(&opcode) || modrm >> 6 == 3 {
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

        let destination = destination_high | ((modrm >> 3) & 7);
        let source1 = (!p1 >> 3) & 0x0F;
        Some((destination, source1, pp, opcode, w))
    }

    /// Validate one register-only legacy SSE or AVX VEX
    /// `ADD`/`MUL`/`SUB`/`MIN`/`DIV`/`MAX` instruction over packed or scalar
    /// binary32/binary64 elements and report whether it requires AVX.
    ///
    /// Memory forms remain at the precise interpreter boundary. Scalar
    /// `VEX.L=1` is excluded because Intel documents generation-dependent
    /// unpredictable behavior for those encodings.
    pub fn legacy_vex_register_fp_arithmetic_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy_modrm = match bytes {
            [0x0F, opcode, modrm] if OPCODES.contains(opcode) => Some(*modrm),
            [0x66 | 0xF2 | 0xF3, 0x0F, opcode, modrm] if OPCODES.contains(opcode) => Some(*modrm),
            [0x40..=0x4F, 0x0F, opcode, modrm] if OPCODES.contains(opcode) => Some(*modrm),
            [0x66 | 0xF2 | 0xF3, 0x40..=0x4F, 0x0F, opcode, modrm] if OPCODES.contains(opcode) => {
                Some(*modrm)
            }
            _ => None,
        };
        if let Some(modrm) = legacy_modrm {
            return (modrm >> 6 == 3).then_some(false);
        }

        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return None,
        };
        if !OPCODES.contains(&opcode) || modrm >> 6 != 3 {
            return None;
        }

        let scalar = matches!(p1 & 0x03, 2 | 3);
        (!scalar || p1 & 0x04 == 0).then_some(true)
    }

    /// Validate a register-only EVEX binary floating-point arithmetic form
    /// and return whether it requires AVX-512VL in addition to AVX-512F.
    ///
    /// Packed and scalar binary32/binary64 forms are accepted with exact mask
    /// validation. Register `EVEX.b=1` admits all four embedded rounding modes
    /// for ADD/MUL/SUB/DIV. For packed MIN/MAX, `EVEX.b=1` also implies a
    /// 512-bit operation and SAE; all four L'L bit patterns are legal because
    /// vector length is implied and the encoded RC is immaterial. Memory and
    /// reserved encodings fail closed.
    pub fn evex_register_fp_arithmetic_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0x62, p0, p1, p2, opcode, modrm] = bytes else {
            return None;
        };

        if p0 & 0x0F != 1 || p1 & 0x04 == 0 || !OPCODES.contains(opcode) || modrm >> 6 != 3 {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        if w != matches!(pp, 1 | 3) {
            return None;
        }
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if zeroing && mask == 0 {
            return None;
        }

        let scalar = matches!(pp, 2 | 3);
        if scalar {
            return (embedded_control || ll != 3).then_some(false);
        }
        if embedded_control {
            return Some(false);
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }
}
