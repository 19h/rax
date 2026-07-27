//! Register-only VEX packed move replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    fn is_vex_register_packed_move_with_opcodes_and_prefixes(
        &self,
        load_opcode: u8,
        store_opcode: u8,
        prefixes: [u8; 2],
    ) -> bool {
        let bytes = self.as_slice();
        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return false,
        };

        p1 & 0x78 == 0x78
            && prefixes.contains(&(p1 & 0x03))
            && (opcode == load_opcode || opcode == store_opcode)
            && modrm >> 6 == 3
    }

    fn vex_register_packed_move_destination_index(
        &self,
        load_opcode: u8,
        store_opcode: u8,
        prefixes: [u8; 2],
    ) -> Option<u8> {
        if !self.is_vex_register_packed_move_with_opcodes_and_prefixes(
            load_opcode,
            store_opcode,
            prefixes,
        ) {
            return None;
        }
        let (reg_extension, rm_extension, opcode, modrm) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (p1 & 0x80 == 0, false, *opcode, *modrm),
            [0xC4, p0, _, opcode, modrm] => (p0 & 0x80 == 0, p0 & 0x20 == 0, *opcode, *modrm),
            _ => unreachable!("VEX packed move shape was validated"),
        };
        let destination = if opcode == load_opcode {
            ((modrm >> 3) & 7) + if reg_extension { 8 } else { 0 }
        } else {
            debug_assert_eq!(opcode, store_opcode);
            (modrm & 7) + if rm_extension { 8 } else { 0 }
        };
        Some(destination)
    }

    /// Validate one register-only VEX `VMOVAPS` or `VMOVAPD` instruction in
    /// either opcode direction.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. C5 and C4 encodings are
    /// accepted; C4.W and C4.X are ignored for register operands. VEX.vvvv is
    /// reserved and must be encoded as `1111b`. Memory forms and every
    /// noncanonical byte shape fail closed.
    pub fn is_vex_register_aligned_packed_fp_move(&self) -> bool {
        self.is_vex_register_packed_move_with_opcodes_and_prefixes(0x28, 0x29, [0, 1])
    }

    /// Return the architectural destination register after exact validation.
    /// Opcode `28h` writes ModR/M.reg; opcode `29h` writes ModR/M.r/m. The
    /// AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after the replayed VEX instruction zeros its
    /// architectural upper state.
    pub(crate) fn vex_aligned_packed_fp_move_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_move_destination_index(0x28, 0x29, [0, 1])
    }

    /// Validate one register-only VEX `VMOVUPS` or `VMOVUPD` instruction in
    /// either opcode direction.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. C5 and C4 encodings are
    /// accepted; C4.W and C4.X are ignored for register operands. VEX.vvvv is
    /// reserved and must be encoded as `1111b`. Memory forms and every
    /// noncanonical byte shape fail closed.
    pub fn is_vex_register_unaligned_packed_fp_move(&self) -> bool {
        self.is_vex_register_packed_move_with_opcodes_and_prefixes(0x10, 0x11, [0, 1])
    }

    /// Return the architectural destination register after exact validation.
    /// Opcode `10h` writes ModR/M.reg; opcode `11h` writes ModR/M.r/m. The
    /// AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after the replayed VEX instruction zeros its
    /// architectural upper state.
    pub(crate) fn vex_unaligned_packed_fp_move_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_move_destination_index(0x10, 0x11, [0, 1])
    }

    /// Validate one register-only VEX `VMOVDQA` or `VMOVDQU` instruction in
    /// either opcode direction.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. C5 and C4 encodings are
    /// accepted; C4.W and C4.X are ignored for register operands. VEX.vvvv is
    /// reserved and must be encoded as `1111b`. Memory forms and every
    /// noncanonical byte shape fail closed.
    pub fn is_vex_register_packed_integer_move(&self) -> bool {
        self.is_vex_register_packed_move_with_opcodes_and_prefixes(0x6F, 0x7F, [1, 2])
    }

    /// Return the architectural destination register after exact validation.
    /// Opcode `6Fh` writes ModR/M.reg; opcode `7Fh` writes ModR/M.r/m. The
    /// AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after the replayed VEX instruction zeros its
    /// architectural upper state.
    pub(crate) fn vex_packed_integer_move_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_move_destination_index(0x6F, 0x7F, [1, 2])
    }
}
