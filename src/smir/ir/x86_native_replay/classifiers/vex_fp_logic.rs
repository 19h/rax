//! Register-only AVX VEX floating logical replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only VEX VANDPS/PD, VANDNPS/PD, VORPS/PD, or
    /// VXORPS/PD instruction.
    ///
    /// Intel SDM Vol. 2 assigns opcodes 54H through 57H in map 0F. No mandatory
    /// prefix selects packed binary32 lanes and 66H selects packed binary64
    /// lanes. VEX.L selects 128 or 256 bits, VEX.W is ignored, and VEX.vvvv is
    /// the unrestricted first source. The C5H form supplies the implicit map,
    /// W, X, and B fields; the C4H form may encode either W value. X is ignored
    /// for a register ModR/M operand. Memory forms remain excluded so native
    /// replay cannot bypass guest-memory translation or fault handling.
    pub fn is_vex_register_fp_logic(&self) -> bool {
        let bytes = self.as_slice();
        let (pp, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (p1 & 0x03, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (p1 & 0x03, *opcode, *modrm),
            _ => return false,
        };

        matches!(pp, 0 | 1) && matches!(opcode, 0x54..=0x57) && modrm >> 6 == 3
    }
}
