//! Register-only VEX packed floating-point move replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only VEX `VMOVUPS` or `VMOVUPD` instruction in
    /// either opcode direction.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. C5 and C4 encodings are
    /// accepted; C4.W and C4.X are ignored for register operands. VEX.vvvv is
    /// reserved and must be encoded as `1111b`. Memory forms and every
    /// noncanonical byte shape fail closed.
    pub fn is_vex_register_unaligned_packed_fp_move(&self) -> bool {
        let bytes = self.as_slice();
        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return false,
        };

        p1 & 0x78 == 0x78
            && matches!(p1 & 0x03, 0 | 1)
            && matches!(opcode, 0x10 | 0x11)
            && modrm >> 6 == 3
    }

    /// Return the architectural destination register after exact validation.
    /// Opcode `10h` writes ModR/M.reg; opcode `11h` writes ModR/M.r/m. The
    /// AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after the replayed VEX instruction zeros its
    /// architectural upper state.
    pub(crate) fn vex_unaligned_packed_fp_move_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_unaligned_packed_fp_move() {
            return None;
        }
        let (reg_extension, rm_extension, opcode, modrm) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (p1 & 0x80 == 0, false, *opcode, *modrm),
            [0xC4, p0, _, opcode, modrm] => (p0 & 0x80 == 0, p0 & 0x20 == 0, *opcode, *modrm),
            _ => unreachable!("VEX packed move shape was validated"),
        };
        let destination = if opcode == 0x10 {
            ((modrm >> 3) & 7) + if reg_extension { 8 } else { 0 }
        } else {
            (modrm & 7) + if rm_extension { 8 } else { 0 }
        };
        Some(destination)
    }
}
