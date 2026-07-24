//! Register-only AVX VEX packed-string comparison replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only VEX.128 `VPCMPxSTRx` instruction.
    ///
    /// Intel SDM Vol. 2B assigns opcodes 60H through 63H in map 0F3A with
    /// mandatory 66H, VEX.L=0, and reserved VEX.vvvv=1111b. Both VEX.W values
    /// are valid: W selects 32- versus 64-bit explicit lengths and is ignored
    /// by implicit-length forms. R and B may select XMM0 through XMM15; X is
    /// ignored for a register ModR/M operand. Memory forms remain excluded so
    /// native replay cannot bypass guest-memory translation or fault handling.
    pub fn is_vex_register_packed_string_compare(&self) -> bool {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0xC4 {
            return false;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];

        p0 & 0x1F == 3 && p1 & 0x7F == 0x79 && matches!(opcode, 0x60..=0x63) && modrm >> 6 == 3
    }
}
