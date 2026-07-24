//! Register-only EVEX binary16 widening-conversion replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Whether replay needs to restore MXCSR.DE to its pre-instruction value.
    ///
    /// Current Intel SDM semantics for register-source `VCVTPH2PSX` do not
    /// report a denormal-operand exception. Some hosts implement the earlier
    /// AVX-512-FP16 behavior and set MXCSR.DE, so native replay must neutralize
    /// that host-specific status change. The other widening conversions expose
    /// their host status directly.
    pub(crate) fn evex_register_fp16_widen_preserves_mxcsr_de(&self) -> bool {
        self.evex_register_fp16_widen_requirements().is_some() && self.as_slice()[1] & 0x0F == 6
    }

    /// Validate one register-only EVEX `VCVTPH2PD`, `VCVTPH2PS`, or
    /// `VCVTPH2PSX` instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512fp16)`. Ordinary 128-bit and
    /// 256-bit forms require AVX-512VL. Register-source `EVEX.b=1` selects the
    /// 512-bit SAE form and requires the canonical `L'L=00` encoding. The
    /// legacy-map `VCVTPH2PS` form requires AVX-512F, whereas `VCVTPH2PD` and
    /// `VCVTPH2PSX` require AVX-512-FP16. Memory forms and every reserved EVEX
    /// field fail closed.
    pub fn evex_register_fp16_widen_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
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
        let needs_fp16 = match (map, pp, w, opcode) {
            // VCVTPH2PS is an AVX-512F conversion retained from F16C.
            (2, 1, false, 0x13) => false,
            // VCVTPH2PD and VCVTPH2PSX are AVX-512-FP16 conversions.
            (5, 0, false, 0x5A) | (6, 1, false, 0x13) => true,
            _ => return None,
        };

        let ll = (p2 >> 5) & 0x03;
        let suppress_exceptions = p2 & 0x10 != 0;
        if suppress_exceptions {
            // Register-source SAE implies VL=512 and has one canonical
            // encoding; L'L=01/10/11 are reserved.
            return (ll == 0).then_some((false, needs_fp16));
        }
        match ll {
            0 | 1 => Some((true, needs_fp16)),
            2 => Some((false, needs_fp16)),
            _ => None,
        }
    }
}
