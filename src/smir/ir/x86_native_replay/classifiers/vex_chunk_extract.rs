//! Register-destination AVX/AVX2 VEX 128-bit chunk extraction replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-destination VEXTRACTF128/VEXTRACTI128 and report
    /// whether the selected form requires AVX2 rather than AVX.
    ///
    /// Both forms are VEX.256.66.0F3A.W0, reserve VEX.vvvv=`1111b`, and carry
    /// an imm8 whose bit 0 selects the 128-bit source lane. VEX.X is ignored
    /// for register destinations. Memory destinations and malformed byte
    /// shapes fail closed.
    pub fn vex_register_chunk_extract_needs_avx2(&self) -> Option<bool> {
        let [0xC4, p0, 0x7D, opcode, modrm, _] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 3 || modrm >> 6 != 3 {
            return None;
        }
        match *opcode {
            0x19 => Some(false),
            0x39 => Some(true),
            _ => None,
        }
    }

    /// Return the architectural XMM destination after exact validation.
    /// VEXTRACT uses ModR/M.r/m as its register destination.
    pub(crate) fn vex_chunk_extract_destination_index(&self) -> Option<u8> {
        self.vex_register_chunk_extract_needs_avx2()?;
        let [0xC4, p0, _, _, modrm, _] = self.as_slice() else {
            unreachable!("VEX chunk-extract shape was validated")
        };
        Some((modrm & 7) + if p0 & 0x20 == 0 { 8 } else { 0 })
    }
}
