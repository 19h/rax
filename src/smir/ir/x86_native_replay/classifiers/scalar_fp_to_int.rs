//! Register-only VEX/EVEX scalar floating-point-to-integer replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only AVX VEX signed scalar floating-point-to-
    /// integer conversion and return its architectural GPR destination.
    ///
    /// The admitted set is `VCVTSS2SI`, `VCVTSD2SI`, `VCVTTSS2SI`, and
    /// `VCVTTSD2SI`, with W0/W1 selecting 32-/64-bit results in the C4 form.
    /// `VEX.vvvv` must be encoded as `1111b`; F3/F2 select binary32/binary64.
    /// Intel documents `VEX.L=1` as generation-dependent unpredictable, so
    /// only `VEX.L=0` is admitted. C4 register-form `VEX.X` is ignored.
    /// Memory sources and every non-exact byte string fail closed.
    pub(crate) fn vex_scalar_fp_to_int_destination_index(&self) -> Option<u8> {
        let (encoded_r, p1, opcode, modrm) = match self.as_slice() {
            &[0xC5, p1, opcode, modrm] => (p1 & 0x80 != 0, p1, opcode, modrm),
            &[0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (p0 & 0x80 != 0, p1, opcode, modrm),
            _ => return None,
        };
        if p1 & 0x7C != 0x78
            || !matches!(p1 & 0x03, 2 | 3)
            || !matches!(opcode, 0x2C | 0x2D)
            || modrm >> 6 != 3
        {
            return None;
        }
        Some(((modrm >> 3) & 7) | (u8::from(!encoded_r) << 3))
    }

    /// Rewrite a validated VEX scalar FP-to-integer destination while
    /// retaining every non-destination bit, including ignored W/X bits.
    pub(crate) fn vex_scalar_fp_to_int_with_destination(&self, destination: u8) -> Option<Self> {
        if destination >= 16 || self.vex_scalar_fp_to_int_destination_index().is_none() {
            return None;
        }

        let mut rewritten = *self;
        match self.as_slice() {
            [0xC5, _p1, _opcode, _modrm] => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[3] = (rewritten.bytes[3] & !0x38) | ((destination & 7) << 3);
            }
            [0xC4, _p0, _p1, _opcode, _modrm] => {
                if destination < 8 {
                    rewritten.bytes[1] |= 0x80;
                } else {
                    rewritten.bytes[1] &= !0x80;
                }
                rewritten.bytes[4] = (rewritten.bytes[4] & !0x38) | ((destination & 7) << 3);
            }
            _ => unreachable!("VEX scalar FP-to-integer shape was validated"),
        }
        debug_assert_eq!(
            rewritten.vex_scalar_fp_to_int_destination_index(),
            Some(destination)
        );
        Some(rewritten)
    }

    /// Validate one register-only EVEX scalar floating-point-to-integer
    /// conversion and return whether it requires AVX-512-FP16.
    ///
    /// The admitted set is `VCVT{SS,SD,SH}2{SI,USI}` plus the corresponding
    /// truncating `VCVTT*` forms, for 32-bit and 64-bit integer destinations.
    /// Map-1 binary32/binary64 forms require AVX-512F; map-5 binary16 forms
    /// require AVX-512-FP16. `L'L` is ignored when `EVEX.b=0`, selects embedded
    /// rounding for non-truncating `EVEX.b=1` forms, and remains LLIG for the
    /// truncating SAE forms. Consequently `L'L=11` is valid only for the
    /// non-truncating embedded-rounding form.
    ///
    /// Memory sources, reserved vvvv/V'/mask/zeroing fields, fabricated GPR bit
    /// 4, and RSP/RBP destinations fail closed. RSP/RBP are unsafe because raw
    /// native replay identity-maps guest GPRs while the trampoline owns the host
    /// stack and frame registers.
    pub fn evex_register_scalar_fp_to_int_requires_fp16(&self) -> Option<bool> {
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
            || p2 & 0x8F != 0x08
            || modrm >> 6 != 3
            || p0 & 0x10 == 0
            || !matches!(opcode, 0x2C | 0x2D | 0x78 | 0x79)
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let needs_fp16 = match (map, pp) {
            // EVEX.F3/F2.0F forms convert binary32/binary64 respectively.
            (1, 2 | 3) => false,
            // EVEX.F3.MAP5 forms convert binary16.
            (5, 2) => true,
            _ => return None,
        };

        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let truncating = matches!(opcode, 0x2C | 0x78);
        if ll == 3 && !(embedded_control && !truncating) {
            return None;
        }

        // EVEX.R' is reserved above. EVEX.R selects GPR0-7/GPR8-15; reject
        // guest RSP/RBP only in the low bank. The r/m extension channels name
        // XMM0-XMM31 and are all safe.
        let low_gpr_bank = p0 & 0x80 != 0;
        let gpr_low = (modrm >> 3) & 0x07;
        if low_gpr_bank && matches!(gpr_low, 4 | 5) {
            return None;
        }

        Some(needs_fp16)
    }
}
