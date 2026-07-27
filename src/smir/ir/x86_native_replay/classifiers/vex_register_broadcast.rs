//! Register-only AVX2 VEX scalar-broadcast replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-source VEX scalar broadcast and return its
    /// architectural element width in bits.
    ///
    /// This covers VBROADCASTSS, VBROADCASTSD, VPBROADCASTB,
    /// VPBROADCASTW, VPBROADCASTD, and VPBROADCASTQ. Every covered form uses
    /// three-byte VEX map 0F38, mandatory prefix 66, W0, reserved
    /// VEX.vvvv=`1111b`, and requires AVX2. VBROADCASTSD is valid only at
    /// VEX.256. Memory operands and malformed byte shapes fail closed.
    pub fn vex_register_broadcast_element_bits(&self) -> Option<u8> {
        let [0xC4, p0, p1, opcode, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 2 || p1 & 0xF8 != 0x78 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let ymm = p1 & 0x04 != 0;
        match (*opcode, ymm) {
            (0x18 | 0x58, _) => Some(32),
            (0x19, true) | (0x59, _) => Some(64),
            (0x78, _) => Some(8),
            (0x79, _) => Some(16),
            _ => None,
        }
    }

    /// Return the architectural destination after exact validation. Every
    /// covered instruction writes ModR/M.reg. The AVX-only state bridge uses
    /// this result to clear the destination's state-backed ZMM[511:256] after
    /// architectural VEX upper-zeroing.
    pub(crate) fn vex_register_broadcast_destination_index(&self) -> Option<u8> {
        self.vex_register_broadcast_element_bits()?;
        let [0xC4, p0, _, _, modrm] = self.as_slice() else {
            unreachable!("VEX register-broadcast shape was validated")
        };
        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 })
    }
}
