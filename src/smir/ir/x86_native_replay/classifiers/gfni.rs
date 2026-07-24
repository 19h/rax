//! Register-only EVEX GFNI replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX GFNI vector instruction and return
    /// whether its vector length requires AVX-512VL in addition to AVX-512F
    /// and GFNI.
    ///
    /// The admitted set is exactly VGF2P8MULB, VGF2P8AFFINEQB, and
    /// VGF2P8AFFINEINVQB. Memory sources, EVEX.b, reserved vector lengths,
    /// malformed masks, incorrect W/pp/map/opcode combinations, and incomplete
    /// or trailing instruction bytes fail closed.
    pub fn evex_register_gfni_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let w = p1 & 0x80 != 0;
        match (map, opcode, w, bytes.len()) {
            (2, 0xCF, false, 6) => {}
            (3, 0xCE | 0xCF, true, 7) => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || ll == 3 || (zeroing && mask == 0) {
            return None;
        }
        Some(ll != 2)
    }
}
