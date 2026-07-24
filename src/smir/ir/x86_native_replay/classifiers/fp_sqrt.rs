//! Register-only x86 floating-point square-root replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only legacy SSE or AVX VEX
    /// `SQRTPS`/`SQRTPD`/`SQRTSS`/`SQRTSD` instruction and report whether it
    /// requires AVX.
    ///
    /// Legacy forms accept the canonical mandatory-prefix position, an
    /// optional REX prefix, and a register ModR/M source. VEX forms require map
    /// 0F and a register source. Packed VEX forms reserve `vvvv`, while scalar
    /// forms use it as the upper-lane merge source. Scalar `VEX.L=1` is kept at
    /// the interpreter boundary because Intel documents generation-dependent
    /// unpredictable behavior for that encoding. Memory forms remain excluded
    /// so replay cannot bypass guest translation or fault handling.
    pub fn legacy_vex_register_fp_sqrt_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy_modrm = match bytes {
            [0x0F, 0x51, modrm] => Some(*modrm),
            [0x66 | 0xF2 | 0xF3, 0x0F, 0x51, modrm] => Some(*modrm),
            [0x40..=0x4F, 0x0F, 0x51, modrm] => Some(*modrm),
            [0x66 | 0xF2 | 0xF3, 0x40..=0x4F, 0x0F, 0x51, modrm] => Some(*modrm),
            _ => None,
        };
        if let Some(modrm) = legacy_modrm {
            return (modrm >> 6 == 3).then_some(false);
        }

        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return None,
        };
        if opcode != 0x51 || modrm >> 6 != 3 {
            return None;
        }

        let pp = p1 & 0x03;
        let packed = pp <= 1;
        if (packed && p1 & 0x78 != 0x78) || (!packed && p1 & 0x04 != 0) {
            return None;
        }
        Some(true)
    }

    /// Validate one register-only EVEX `VSQRTPS`, `VSQRTPD`, `VSQRTSS`,
    /// `VSQRTSD`, or `VSQRTPH` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Packed 128-bit and
    /// 256-bit forms require AVX-512VL, except that register-source
    /// `EVEX.b=1` selects a 512-bit operation and uses `L'L` as embedded
    /// rounding control. Scalar forms are LLIG and never require AVX-512VL;
    /// without embedded rounding, they accept only the three defined EVEX
    /// vector-length encodings.
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
            // Scalar forms consume vvvv/V' as source 1. L'L is LLIG when b=0,
            // where the reserved 11b vector-length encoding remains invalid,
            // and selects one of four rounding controls when b=1.
            return (embedded_control || ll != 3).then_some((false, false));
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
