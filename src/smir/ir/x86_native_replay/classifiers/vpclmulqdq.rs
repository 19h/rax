//! Register-only EVEX VPCLMULQDQ replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX VPCLMULQDQ instruction and return
    /// whether its vector length requires AVX-512VL in addition to AVX-512F
    /// and VPCLMULQDQ.
    ///
    /// W is ignored architecturally and therefore both values are admitted.
    /// Memory sources, masking, zeroing, EVEX.b, reserved vector lengths,
    /// incorrect pp/map/opcode combinations, and incomplete or trailing bytes
    /// fail closed.
    pub fn evex_register_vpclmulqdq_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 3 || p1 & 0x04 == 0 || p1 & 0x03 != 1 || opcode != 0x44 || modrm >> 6 != 3 {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        if embedded_control || zeroing || mask != 0 || ll == 3 {
            return None;
        }
        Some(ll != 2)
    }
}
