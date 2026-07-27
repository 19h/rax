//! Register-only AVX/AVX2 VEX immediate-permute replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-source VEX immediate permute and report whether
    /// the selected form requires AVX2 rather than AVX.
    ///
    /// This covers VPERMILPS and VPERMILPD at VEX.128/VEX.256 with W0, plus
    /// VPERMQ and VPERMPD at VEX.256 with W1. Every form uses three-byte VEX
    /// map 0F3A, mandatory prefix 66, reserved VEX.vvvv=`1111b`, and an imm8.
    /// VEX.X is ignored for register operands. Memory operands and malformed
    /// byte shapes fail closed.
    pub fn vex_register_immediate_permute_needs_avx2(&self) -> Option<bool> {
        let [0xC4, p0, p1, opcode, modrm, _] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x78 != 0x78 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        let ymm = p1 & 0x04 != 0;
        match (*opcode, w, ymm) {
            (0x04 | 0x05, false, _) => Some(false),
            (0x00 | 0x01, true, true) => Some(true),
            _ => None,
        }
    }

    /// Return the architectural destination after exact validation. Every
    /// covered instruction writes ModR/M.reg. The AVX-only state bridge uses
    /// this result to clear the destination's state-backed ZMM[511:256] after
    /// architectural VEX upper-zeroing.
    pub(crate) fn vex_immediate_permute_destination_index(&self) -> Option<u8> {
        self.vex_register_immediate_permute_needs_avx2()?;
        let [0xC4, p0, _, _, modrm, _] = self.as_slice() else {
            unreachable!("VEX immediate-permute shape was validated")
        };
        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 })
    }
}
