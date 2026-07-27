//! Register-only VEX one-source lane-shuffle replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only VEX one-source lane-shuffle instruction and
    /// return whether its 256-bit form requires AVX2.
    ///
    /// This covers VMOVSLDUP, VMOVSHDUP, VMOVDDUP, VPSHUFD, VPSHUFHW, and
    /// VPSHUFLW. The duplicate moves require AVX at both vector lengths.
    /// VEX.128 packed immediate shuffles require AVX; their VEX.256 forms
    /// require AVX2. Every form uses map 0F, reserves VEX.vvvv as `1111b`, and
    /// is WIG. Memory operands and malformed byte shapes fail closed.
    pub fn vex_register_lane_shuffle_needs_avx2(&self) -> Option<bool> {
        let (p1, opcode, modrm, has_immediate) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm, false),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm, false),
            [0xC5, p1, opcode, modrm, _] => (*p1, *opcode, *modrm, true),
            [0xC4, p0, p1, opcode, modrm, _] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm, true),
            _ => return None,
        };
        if p1 & 0x78 != 0x78 || modrm >> 6 != 3 {
            return None;
        }

        match (has_immediate, opcode, p1 & 0x03) {
            (false, 0x12, 2 | 3) | (false, 0x16, 2) => Some(false),
            (true, 0x70, 1 | 2 | 3) => Some(p1 & 0x04 != 0),
            _ => None,
        }
    }

    /// Return the architectural destination after exact validation. Every
    /// covered instruction writes ModR/M.reg. The AVX-only state bridge uses
    /// the result to clear the destination's state-backed ZMM[511:256] after
    /// architectural VEX upper-zeroing.
    pub(crate) fn vex_lane_shuffle_destination_index(&self) -> Option<u8> {
        self.vex_register_lane_shuffle_needs_avx2()?;
        let (reg_extension, modrm) = match self.as_slice() {
            [0xC5, p1, _, modrm] | [0xC5, p1, _, modrm, _] => (p1 & 0x80 == 0, *modrm),
            [0xC4, p0, _, _, modrm] | [0xC4, p0, _, _, modrm, _] => (p0 & 0x80 == 0, *modrm),
            _ => unreachable!("VEX lane-shuffle shape was validated"),
        };
        Some(((modrm >> 3) & 7) + if reg_extension { 8 } else { 0 })
    }
}
