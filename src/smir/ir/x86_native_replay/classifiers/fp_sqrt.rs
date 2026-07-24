//! Register-only EVEX floating-point square-root replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX `VSQRTPS`, `VSQRTPD`, `VSQRTSS`,
    /// `VSQRTSD`, or `VSQRTPH` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Packed 128-bit and
    /// 256-bit forms require AVX-512VL, except that register-source
    /// `EVEX.b=1` selects a 512-bit operation and uses `L'L` as embedded
    /// rounding control. Scalar forms are LLIG and never require AVX-512VL.
    /// Binary16 packed forms require AVX-512-FP16. `VSQRTSH` remains owned by
    /// the disjoint scalar-FP16 arithmetic replay classifier. Memory forms and
    /// every reserved EVEX field fail closed.
    pub fn evex_register_fp_sqrt_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0 || opcode != 0x51 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (scalar, needs_fp16) = match (map, pp, w) {
            // VSQRTPS, VSQRTPD, VSQRTSS, and VSQRTSD.
            (1, 0, false) | (1, 1, true) => (false, false),
            (1, 2, false) | (1, 3, true) => (true, false),
            // VSQRTPH. MAP5/F3 is VSQRTSH and is deliberately classified by
            // evex_register_scalar_fp16_arithmetic_needs_vl instead.
            (5, 0, false) => (false, true),
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if zeroing && mask == 0 {
            return None;
        }

        if scalar {
            // Scalar forms consume vvvv/V' as source 1. L'L is LLIG when b=0
            // and selects embedded rounding when b=1.
            return Some((false, false));
        }

        // Packed forms reserve vvvv/V' to their all-ones encodings.
        if p1 & 0x78 != 0x78 || p2 & 0x08 == 0 {
            return None;
        }
        if embedded_control {
            // Register-source EVEX.b implies VL=512 and all four L'L values
            // are valid rounding controls.
            Some((false, needs_fp16))
        } else {
            match ll {
                0 | 1 => Some((true, needs_fp16)),
                2 => Some((false, needs_fp16)),
                _ => None,
            }
        }
    }
}
