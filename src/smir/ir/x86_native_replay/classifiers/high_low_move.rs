//! Register-only EVEX packed-single high/low move replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only legacy SSE or AVX VEX `MOVHLPS`/`MOVLHPS`
    /// instruction and report whether it requires AVX.
    ///
    /// Canonical legacy encodings have no mandatory prefix and may carry one
    /// final REX prefix. VEX encodings use map 0F, no mandatory prefix,
    /// `VEX.L=0`, and WIG. Both opcode forms require `ModRM.mod=0b11`;
    /// architecturally invalid memory forms and every non-canonical byte shape
    /// fail closed.
    pub fn legacy_vex_register_high_low_move_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy_modrm = match bytes {
            [0x0F, 0x12 | 0x16, modrm] | [0x40..=0x4F, 0x0F, 0x12 | 0x16, modrm] => Some(*modrm),
            _ => None,
        };
        if let Some(modrm) = legacy_modrm {
            return (modrm >> 6 == 3).then_some(false);
        }

        let (p1, modrm) = match bytes {
            [0xC5, p1, 0x12 | 0x16, modrm] => (*p1, *modrm),
            [0xC4, p0, p1, 0x12 | 0x16, modrm] if p0 & 0x1F == 1 => (*p1, *modrm),
            _ => return None,
        };
        (p1 & 0x07 == 0 && modrm >> 6 == 3).then_some(true)
    }

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
