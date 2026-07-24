//! Register-only EVEX scalar floating-point precision-conversion replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX scalar floating-point precision
    /// conversion and return whether it requires AVX-512-FP16.
    ///
    /// The admitted set is `VCVTSD2SS`, `VCVTSS2SD`, `VCVTSD2SH`,
    /// `VCVTSH2SD`, `VCVTSS2SH`, and `VCVTSH2SS`. Every family is LLIG.
    /// Register-source `EVEX.b=1` selects embedded rounding plus SAE for the
    /// narrowing forms and SAE for the exact widening forms, so all four L'L
    /// values are valid. EVEX.vvvv/V' supplies the upper-lane merge source.
    ///
    /// Memory forms, malformed zeroing with k0, absent EVEX fixed-one, and
    /// every non-family map/opcode/prefix/W combination fail closed.
    pub fn evex_register_scalar_fp_convert_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let needs_fp16 = match (map, opcode, pp, w) {
            // VCVTSD2SS and VCVTSS2SD.
            (1, 0x5A, 3, true) | (1, 0x5A, 2, false) => false,
            // VCVTSD2SH, VCVTSH2SD, VCVTSS2SH, and VCVTSH2SS.
            (5, 0x5A, 3, true)
            | (5, 0x5A, 2, false)
            | (5, 0x1D, 0, false)
            | (6, 0x13, 0, false) => true,
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        if zeroing && mask == 0 {
            return None;
        }
        Some(needs_fp16)
    }
}
