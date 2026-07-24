//! Register-only EVEX binary16 narrowing-conversion replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX `VCVTPD2PH`, `VCVTPS2PH`, or
    /// `VCVTPS2PHX` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Ordinary 128-bit and
    /// 256-bit source forms require AVX-512VL. `VCVTPD2PH` and `VCVTPS2PHX`
    /// use all four `L'L` values as embedded rounding control when
    /// register-source `EVEX.b=1` implies a 512-bit source. `VCVTPS2PH` uses
    /// its immediate for rounding and admits the canonical `EVEX.b=1,L'L=00`
    /// 512-bit SAE form. The legacy-map `VCVTPS2PH` requires AVX-512F;
    /// `VCVTPD2PH` and `VCVTPS2PHX` require AVX-512-FP16. Memory forms and
    /// every reserved EVEX field fail closed.
    pub fn evex_register_fp16_narrow_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
            || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (needs_fp16, has_immediate) = match (map, pp, w, opcode) {
            // VCVTPS2PH is the AVX-512F conversion retained from F16C.
            (3, 1, false, 0x1D) => (false, true),
            // VCVTPD2PH and VCVTPS2PHX are AVX-512-FP16 conversions.
            (5, 1, true, 0x5A) | (5, 1, false, 0x1D) => (true, false),
            _ => return None,
        };
        if bytes.len() != if has_immediate { 7 } else { 6 } {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        if embedded_control {
            if has_immediate {
                // SAE does not consume L'L as rounding control. LLVM emits
                // the canonical width-implied L'L=00 representation.
                return (ll == 0).then_some((false, false));
            }
            // L'L encodes RN/RD/RU/RZ for the 512-bit ER forms.
            return Some((false, true));
        }
        match ll {
            0 | 1 => Some((true, needs_fp16)),
            2 => Some((false, needs_fp16)),
            _ => None,
        }
    }
}
