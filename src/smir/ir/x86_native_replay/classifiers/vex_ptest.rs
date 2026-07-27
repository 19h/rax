//! Register-only AVX VEX packed bit tests.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate a register-only VEX `VPTEST`, `VTESTPS`, or `VTESTPD`.
    ///
    /// All three instructions use map 0F38 with the 66 mandatory prefix,
    /// reserve VEX.vvvv as encoded `1111b`, and accept 128- and 256-bit
    /// vectors. `VPTEST` specifies WIG, whereas `VTESTPS` and `VTESTPD`
    /// require W0. Both vector lengths require AVX. Memory-source forms remain
    /// at the precise SMIR interpreter frontier.
    pub fn is_vex_register_ptest(&self) -> bool {
        matches!(
            self.as_slice(),
            [0xC4, p0, p1, opcode, modrm]
                if p0 & 0x1F == 2
                    && p1 & 0x78 == 0x78
                    && p1 & 0x03 == 1
                    && modrm >> 6 == 3
                    && match opcode {
                        0x17 => true,
                        0x0E | 0x0F => p1 & 0x80 == 0,
                        _ => false,
                    }
        )
    }
}
