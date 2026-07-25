//! Register-only AVX VEX widening doubleword-multiply replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only VEX `VPMULUDQ` or `VPMULDQ` instruction and
    /// report whether its vector length requires AVX2 rather than AVX.
    ///
    /// `VPMULUDQ` uses VEX map 0F opcode F4; `VPMULDQ` uses map 0F38 opcode
    /// 28. Both require mandatory prefix 66 and specify WIG. VEX.128 requires
    /// AVX, while VEX.256 requires AVX2. Memory operands and every malformed
    /// or non-canonical byte shape fail closed.
    pub fn vex_register_widening_dword_multiply_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let (map, p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (1, *p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] => (p0 & 0x1F, *p1, *opcode, *modrm),
            _ => return None,
        };

        (matches!((map, opcode), (1, 0xF4) | (2, 0x28)) && p1 & 0x03 == 1 && modrm >> 6 == 3)
            .then_some(p1 & 0x04 != 0)
    }
}
