//! Register-only EVEX scalar-move replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX `VMOVSH`, `VMOVSS`, or `VMOVSD`
    /// instruction in either opcode direction.
    ///
    /// Returns whether AVX-512-FP16 is required. All three scalar families are
    /// LLIG, accept the three defined EVEX vector-length encodings, and require
    /// neither AVX-512VL nor AVX-512DQ. `VMOVSS` and `VMOVSD` require AVX-512F;
    /// `VMOVSH` requires AVX-512-FP16. Register forms consume EVEX.vvvv/V' as
    /// the upper-XMM merge source. EVEX.b, malformed zeroing with k0, memory
    /// forms, and every non-family opcode field fail closed.
    pub fn evex_register_scalar_move_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0 || !matches!(opcode, 0x10 | 0x11) || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let needs_fp16 = match (map, pp, w) {
            // VMOVSS and VMOVSD.
            (1, 2, false) | (1, 3, true) => false,
            // VMOVSH.
            (5, 2, false) => true,
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || ll == 3 || (zeroing && mask == 0) {
            return None;
        }

        Some(needs_fp16)
    }
}
