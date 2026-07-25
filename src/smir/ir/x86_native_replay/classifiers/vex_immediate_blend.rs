//! Register-only AVX VEX immediate blends.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX immediate blend and
    /// report whether the selected vector width requires AVX2 rather than AVX.
    ///
    /// Intel SDM Volume 2 assigns `VPBLENDD`, `VBLENDPS`, `VBLENDPD`, and
    /// `VPBLENDW` to map 0F3A with mandatory 66H and opcodes 02H/0CH/0DH/0EH.
    /// `VPBLENDD` requires AVX2 and VEX.W=0. `VBLENDPS` and `VBLENDPD` require
    /// AVX for both widths. `VPBLENDW` requires AVX for 128 bits and AVX2 for
    /// 256 bits. The latter three opcodes are WIG. Memory forms remain excluded
    /// so replay cannot bypass guest translation or precise fault handling.
    pub fn vex_register_immediate_blend_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, opcode, modrm, _imm] = bytes else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        match opcode {
            0x02 if p1 & 0x80 == 0 => Some(true),
            0x0C | 0x0D => Some(false),
            0x0E => Some(p1 & 0x04 != 0),
            _ => None,
        }
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX immediate blend. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_immediate_blend_destination_index(&self) -> Option<u8> {
        self.vex_register_immediate_blend_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }
}
