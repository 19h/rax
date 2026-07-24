//! Register-only AVX VEX FMA3 replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one canonical five-byte register-only VEX FMA3 instruction.
    ///
    /// Intel SDM Vol. 2 assigns opcodes 96H through 9FH, A6H through AFH,
    /// and B6H through BFH in map 0F38 with mandatory 66H. VEX.W selects
    /// binary32/binary64 elements, VEX.L selects 128/256 bits for packed forms
    /// and is ignored for scalar forms, and VEX.vvvv is an unrestricted second
    /// source. R and B extend the destination and third source; X is ignored
    /// for a register ModR/M operand. Memory forms remain excluded so native
    /// replay cannot bypass guest-memory translation or fault handling.
    pub fn is_vex_register_fma3(&self) -> bool {
        let bytes = self.as_slice();
        if bytes.len() != 5 || bytes[0] != 0xC4 {
            return false;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];

        p0 & 0x1F == 2
            && p1 & 0x03 == 1
            && matches!(opcode, 0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF)
            && modrm >> 6 == 3
    }
}
