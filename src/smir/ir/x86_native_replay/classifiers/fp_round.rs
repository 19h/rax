use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only AVX VEX `VROUNDPS`, `VROUNDPD`, `VROUNDSS`,
    /// or `VROUNDSD` instruction and return its architectural destination.
    ///
    /// All four instructions use map 0F3A and mandatory prefix 66. Packed
    /// forms reserve `VEX.vvvv=1111b` and use `VEX.L` to select 128 or 256
    /// bits. Scalar forms consume `VEX.vvvv` as their merge source and define
    /// `VEX.L` as ignored. `VEX.W` and register-form `VEX.X` are ignored, and
    /// all immediate-byte values are defined through their low control bits.
    /// Memory forms and non-exact source byte strings fail closed.
    pub fn vex_round_destination_index(&self) -> Option<u8> {
        let &[0xC4, p0, p1, opcode, modrm, _imm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 3
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x08..=0x0B)
            || modrm >> 6 != 3
            || (matches!(opcode, 0x08 | 0x09) && p1 & 0x78 != 0x78)
        {
            return None;
        }
        Some(((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3))
    }
}
