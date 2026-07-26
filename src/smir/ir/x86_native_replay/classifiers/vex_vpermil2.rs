//! Register-only AMD XOP `VPERMIL2PS`/`VPERMIL2PD` replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX-encoded VPERMIL2
    /// instruction.
    ///
    /// AMD APM Volume 4, revision 3.26 assigns opcodes 48H and 49H in map
    /// 0F3A with mandatory 66H. VEX.W swaps the ModR/M and SRS source roles,
    /// VEX.L selects 128 or 256 bits, and all VEX.vvvv and immediate values
    /// are legal. Memory forms remain excluded so native replay cannot bypass
    /// guest-memory translation or precise fault handling.
    pub fn is_vex_register_vpermil2(&self) -> bool {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0xC4 {
            return false;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];

        p0 & 0x1F == 3 && p1 & 0x03 == 1 && matches!(opcode, 0x48 | 0x49) && modrm >> 6 == 3
    }

    /// Architectural destination selected by an exact register-only
    /// VPERMIL2 encoding. ModR/M.reg is extended by inverted VEX.R.
    pub(crate) fn vex_vpermil2_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_vpermil2() {
            return None;
        }
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }
}
