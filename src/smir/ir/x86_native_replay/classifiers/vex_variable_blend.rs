//! Register-only AVX VEX variable blends.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX variable blend and report
    /// whether the selected form requires AVX2 rather than AVX.
    ///
    /// Intel SDM Volume 2 assigns `VBLENDVPS`, `VBLENDVPD`, and `VPBLENDVB`
    /// to map 0F3A with mandatory 66H, VEX.W=0, and opcodes 4AH/4BH/4CH.
    /// Both floating forms require AVX at either vector width. `VPBLENDVB`
    /// requires AVX for 128 bits and AVX2 for 256 bits. The explicit mask
    /// register occupies imm8[7:4], while imm8[3:0] is ignored. Memory forms
    /// remain excluded so replay cannot bypass guest translation or precise
    /// fault handling.
    pub fn vex_register_variable_blend_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, opcode, modrm, _is4] = bytes else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x83 != 1 || modrm >> 6 != 3 || !matches!(opcode, 0x4A..=0x4C) {
            return None;
        }

        Some(*opcode == 0x4C && p1 & 0x04 != 0)
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX variable blend. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_variable_blend_destination_index(&self) -> Option<u8> {
        self.vex_register_variable_blend_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }
}
