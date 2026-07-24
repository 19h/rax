//! Register-only EVEX packed-single high/low move replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX `VMOVHLPS` or `VMOVLHPS` instruction.
    ///
    /// Both instructions are fixed at EVEX.128, map 0F, NP, W0, use all three
    /// vector-register extension channels, and forbid masking and EVEX.b. They
    /// require AVX-512F but, despite their 128-bit width, do not require
    /// AVX-512VL. Memory ModR/M forms are architecturally invalid. The returned
    /// value is therefore always `false` for a valid instruction.
    pub fn evex_register_high_low_move_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0f != 1
            || p1 & 0x04 == 0
            || p1 & 0x83 != 0
            || p2 & !0x08 != 0
            || !matches!(opcode, 0x12 | 0x16)
            || modrm >> 6 != 3
        {
            return None;
        }

        Some(false)
    }
}
