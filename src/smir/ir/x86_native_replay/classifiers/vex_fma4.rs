//! Register-only AMD AVX VEX FMA4 replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX FMA4 instruction.
    ///
    /// AMD APM Volume 4, revision 3.26 assigns opcodes 5CH through 5FH,
    /// 68H through 6FH, and 78H through 7FH in map 0F3A with mandatory 66H.
    /// VEX.W swaps the ModR/M and `/is4` source roles, VEX.L selects 128/256
    /// bits for packed forms and is ignored for scalar forms, and VEX.vvvv is
    /// an unrestricted first source. Bits 7:4 of the final byte select the
    /// `/is4` register; bits 3:0 do not select an operand. Memory forms remain
    /// excluded so native replay cannot bypass guest-memory translation or
    /// precise fault handling.
    pub fn is_vex_register_fma4(&self) -> bool {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0xC4 {
            return false;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];

        p0 & 0x1F == 3
            && p1 & 0x03 == 1
            && matches!(opcode, 0x5C..=0x5F | 0x68..=0x6F | 0x78..=0x7F)
            && modrm >> 6 == 3
    }

    /// Architectural destination register selected by an exact register-only
    /// FMA4 encoding. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_fma4_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_fma4() {
            return None;
        }
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }
}
