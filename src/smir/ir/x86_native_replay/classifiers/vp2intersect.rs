//! Register-only EVEX VP2INTERSECTD/Q replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX VP2INTERSECTD/Q instruction and return
    /// whether its vector length requires AVX-512VL in addition to AVX-512F
    /// and AVX512_VP2INTERSECT.
    ///
    /// ModR/M.reg addresses K0-K7 and therefore both EVEX destination-extension
    /// bits are reserved. W selects dword or qword elements. Memory sources,
    /// masking, zeroing, EVEX.b, reserved vector lengths, malformed fixed
    /// fields, and incomplete or trailing bytes fail closed.
    pub fn evex_register_vp2intersect_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Map 2 (0F38), both non-inverted K-destination extension bits,
        // EVEX.P1's fixed-one bit, mandatory F2, and a register source.
        if p0 & 0x0F != 2
            || p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || p1 & 0x03 != 3
            || opcode != 0x68
            || modrm >> 6 != 3
        {
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
