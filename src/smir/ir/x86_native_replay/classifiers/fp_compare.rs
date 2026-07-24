//! Register-only EVEX floating-point comparison replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX `VCOMISH` or `VUCOMISH` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Both instructions are
    /// scalar LLIG forms, require AVX-512-FP16 but not AVX-512VL, admit SAE
    /// through EVEX.b, and accept the three defined EVEX vector-length
    /// encodings. They reserve EVEX.vvvv/V'/z/aaa and reject memory forms.
    pub fn evex_register_fp16_flag_compare_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        let ll = (p2 >> 5) & 0x03;
        if p0 & 0x0F != 5
            || p1 != 0x7C
            || p2 & 0x8F != 0x08
            || ll == 3
            || !matches!(opcode, 0x2E | 0x2F)
            || modrm >> 6 != 3
        {
            return None;
        }
        Some((false, true))
    }

    /// Validate one register-only EVEX `VCMPPS`, `VCMPPD`, `VCMPSS`,
    /// `VCMPSD`, `VCMPPH`, or `VCMPSH` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Packed 128-bit and
    /// 256-bit forms require AVX-512VL. Register-source packed `EVEX.b=1`
    /// selects the 512-bit SAE form and requires `L'L=00`; scalar forms are
    /// LLIG and never require AVX-512VL. Binary16 forms require AVX-512-FP16.
    /// The destination must use the canonical K0-K7 encoding, EVEX.z and
    /// immediate bits 7:5 are reserved, and every memory form fails closed.
    pub fn evex_register_fp_compare_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let immediate = bytes[6];

        if p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || p2 & 0x80 != 0
            || opcode != 0xC2
            || modrm >> 6 != 3
            || immediate & !0x1F != 0
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (scalar, needs_fp16) = match (map, pp, w) {
            // VCMPPS, VCMPPD, VCMPSS, and VCMPSD.
            (1, 0, false) | (1, 1, true) => (false, false),
            (1, 2, false) | (1, 3, true) => (true, false),
            // VCMPPH and VCMPSH.
            (3, 0, false) => (false, true),
            (3, 2, false) => (true, true),
            _ => return None,
        };

        let ll = (p2 >> 5) & 0x03;
        let suppress_exceptions = p2 & 0x10 != 0;
        if scalar {
            // EVEX.b selects SAE, not embedded rounding, so L'L remains LLIG:
            // its three defined vector-length encodings are ignored, while
            // the reserved 11b vector-length encoding remains invalid.
            return (ll != 3).then_some((false, needs_fp16));
        }
        if suppress_exceptions {
            // Packed register-source SAE is defined only for VL=512 and uses
            // the canonical L'L=00 encoding.
            return (ll == 0).then_some((false, needs_fp16));
        }
        match ll {
            0 | 1 => Some((true, needs_fp16)),
            2 => Some((false, needs_fp16)),
            _ => None,
        }
    }
}
