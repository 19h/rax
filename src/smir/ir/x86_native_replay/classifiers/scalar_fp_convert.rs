//! Register-only EVEX scalar floating-point precision-conversion replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only AVX VEX `VCVTSS2SD` or `VCVTSD2SS`
    /// instruction and return its architectural destination.
    ///
    /// Both forms use map 0F, opcode 5A, and consume `VEX.vvvv` as the
    /// upper-lane merge source. F3 selects binary32-to-binary64 and F2 selects
    /// binary64-to-binary32. `VEX.W` and register-form `VEX.X` are ignored.
    /// Intel documents `VEX.L=1` as generation-dependent unpredictable, so
    /// only `VEX.L=0` register forms are admitted. Memory and non-exact source
    /// byte strings fail closed.
    pub fn vex_scalar_fp_convert_destination_index(&self) -> Option<u8> {
        let (encoded_r, p1, opcode, modrm) = match self.as_slice() {
            &[0xC5, p1, opcode, modrm] => (p1 & 0x80 != 0, p1, opcode, modrm),
            &[0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (p0 & 0x80 != 0, p1, opcode, modrm),
            _ => return None,
        };
        if p1 & 0x04 != 0 || !matches!(p1 & 0x03, 2 | 3) || opcode != 0x5A || modrm >> 6 != 3 {
            return None;
        }
        Some(((modrm >> 3) & 7) | (u8::from(!encoded_r) << 3))
    }

    /// Validate one register-only EVEX scalar floating-point precision
    /// conversion and return whether it requires AVX-512-FP16.
    ///
    /// The admitted set is `VCVTSD2SS`, `VCVTSS2SD`, `VCVTSD2SH`,
    /// `VCVTSH2SD`, `VCVTSS2SH`, and `VCVTSH2SS`. Every family is LLIG.
    /// Register-source `EVEX.b=1` selects embedded rounding plus SAE for the
    /// narrowing forms and SAE for the exact widening forms. The register-
    /// source control makes all four L'L bit images defined; without it, LLIG
    /// accepts the three defined EVEX vector-length encodings. EVEX.vvvv/V'
    /// supplies the upper-lane merge source.
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
            // VCVTSD2SH, VCVTSH2SD, VCVTSS2SH, and VCVTSH2SS respectively.
            (5, 0x5A, 3, true)
            | (5, 0x5A, 2, false)
            | (5, 0x1D, 0, false)
            | (6, 0x13, 0, false) => true,
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if (zeroing && mask == 0) || (ll == 3 && !embedded_control) {
            return None;
        }
        Some(needs_fp16)
    }
}
