//! Register-only AVX VEX scalar `VMOVQ` aliases.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate a register-only VEX `VMOVQ` encoded as `F3 0F 7E` or
    /// `66 0F D6`.
    ///
    /// Both aliases are VEX.128, require AVX, reserve VEX.vvvv as encoded
    /// `1111b`, and treat VEX.W as ignored. Compact C5 and extended C4
    /// encodings are accepted; C4.X is ignored for register operands. Memory
    /// forms remain at the precise SMIR interpreter frontier.
    pub fn is_vex_register_scalar_vmovq(&self) -> bool {
        self.vex_register_scalar_vmovq_destination_index().is_some()
    }

    /// Return the architectural XMM destination after exact validation.
    ///
    /// Opcode `7Eh` writes ModR/M.reg, while opcode `D6h` writes ModR/M.r/m.
    /// The AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after native replay zeros bits 255:64.
    pub(crate) fn vex_register_scalar_vmovq_destination_index(&self) -> Option<u8> {
        let (reg_extension, rm_extension, p1, opcode, modrm) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (p1 & 0x80 == 0, false, *p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => {
                (p0 & 0x80 == 0, p0 & 0x20 == 0, *p1, *opcode, *modrm)
            }
            _ => return None,
        };
        if p1 & 0x7C != 0x78 || modrm >> 6 != 3 {
            return None;
        }

        let destination = match (p1 & 0x03, opcode) {
            (2, 0x7E) => ((modrm >> 3) & 7) + if reg_extension { 8 } else { 0 },
            (1, 0xD6) => (modrm & 7) + if rm_extension { 8 } else { 0 },
            _ => return None,
        };
        Some(destination)
    }
}
