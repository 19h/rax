//! Register-only VEX/EVEX scalar integer-to-floating-point replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only AVX VEX signed integer-to-scalar-FP
    /// conversion and return its architectural XMM destination.
    ///
    /// The admitted set is `VCVTSI2SS` and `VCVTSI2SD`. F3/F2 select the
    /// binary32/binary64 destination; C4 W0/W1 select a 32-/64-bit GPR source.
    /// `VEX.vvvv` supplies the upper-lane merge source. Intel documents
    /// `VEX.L=1` as generation-dependent unpredictable, so only `VEX.L=0` is
    /// admitted. C4 register-form `VEX.X` is ignored. Memory sources and every
    /// non-exact byte string fail closed.
    pub(crate) fn vex_scalar_int_to_fp_destination_index(&self) -> Option<u8> {
        let (encoded_r, p1, modrm) = match self.as_slice() {
            &[0xC5, p1, 0x2A, modrm] => (p1 & 0x80 != 0, p1, modrm),
            &[0xC4, p0, p1, 0x2A, modrm] if p0 & 0x1F == 1 => (p0 & 0x80 != 0, p1, modrm),
            _ => return None,
        };
        if p1 & 0x04 != 0 || !matches!(p1 & 0x03, 2 | 3) || modrm >> 6 != 3 {
            return None;
        }
        Some(((modrm >> 3) & 7) | (u8::from(!encoded_r) << 3))
    }

    /// Return the architectural GPR source of a validated VEX signed
    /// integer-to-scalar-FP conversion.
    pub(crate) fn vex_scalar_int_to_fp_source_index(&self) -> Option<u8> {
        self.vex_scalar_int_to_fp_destination_index()?;
        match self.as_slice() {
            [0xC5, _p1, 0x2A, modrm] => Some(modrm & 7),
            [0xC4, p0, _p1, 0x2A, modrm] => Some((modrm & 7) | (u8::from(p0 & 0x20 == 0) << 3)),
            _ => unreachable!("VEX scalar integer-to-FP shape was validated"),
        }
    }

    /// Rewrite a validated VEX scalar integer source while retaining every
    /// non-source bit, including ignored X and merge-source fields.
    pub(crate) fn vex_scalar_int_to_fp_with_source(&self, source: u8) -> Option<Self> {
        if source >= 16 || self.vex_scalar_int_to_fp_source_index().is_none() {
            return None;
        }

        let mut rewritten = *self;
        match self.as_slice() {
            [0xC5, _p1, 0x2A, _modrm] => {
                if source >= 8 {
                    return None;
                }
                rewritten.bytes[3] = (rewritten.bytes[3] & !0x07) | source;
            }
            [0xC4, _p0, _p1, 0x2A, _modrm] => {
                if source < 8 {
                    rewritten.bytes[1] |= 0x20;
                } else {
                    rewritten.bytes[1] &= !0x20;
                }
                rewritten.bytes[4] = (rewritten.bytes[4] & !0x07) | (source & 7);
            }
            _ => unreachable!("VEX scalar integer-to-FP shape was validated"),
        }
        debug_assert_eq!(rewritten.vex_scalar_int_to_fp_source_index(), Some(source));
        Some(rewritten)
    }

    /// Validate one register-only EVEX scalar integer-to-floating-point
    /// conversion and return whether it requires AVX-512-FP16.
    ///
    /// The admitted set is `VCVT{,U}SI2{SS,SD,SH}` for 32-bit and 64-bit GPR
    /// sources. Map-1 binary32/binary64 forms require AVX-512F; map-5 binary16
    /// forms require AVX-512-FP16. `L'L` is ignored when `EVEX.b=0` and selects
    /// embedded rounding/SAE when `EVEX.b=1`, except that binary64 W0 is exact
    /// for every input and architecturally ignores the selected rounding mode.
    /// `L'L=11` is valid only when `EVEX.b=1` repurposes the field as embedded
    /// rounding; otherwise L'L remains LLIG and accepts the three defined EVEX
    /// vector-length encodings.
    ///
    /// Memory sources, masks, zeroing, fabricated GPR bit 4, and RSP/RBP
    /// sources fail closed. RSP/RBP are unsafe because raw native replay
    /// identity-maps guest GPRs while the trampoline owns the host stack and
    /// frame registers.
    pub fn evex_register_scalar_int_to_fp_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // EVEX.X would fabricate a fifth GPR source bit. EVEX.R/R' select the
        // 32 architectural XMM destinations and are both meaningful here.
        if p1 & 0x04 == 0
            || p2 & 0x87 != 0
            || modrm >> 6 != 3
            || p0 & 0x40 == 0
            || !matches!(opcode, 0x2A | 0x7B)
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let needs_fp16 = match (map, pp) {
            // EVEX.F3/F2.0F forms produce binary32/binary64 respectively.
            (1, 2 | 3) => false,
            // EVEX.F3.MAP5 forms produce binary16.
            (5, 2) => true,
            _ => return None,
        };

        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        if ll == 3 && !embedded_control {
            return None;
        }

        // EVEX.B selects GPR0-7/GPR8-15. Reject guest RSP/RBP only in the low
        // bank; R12/R13 are ordinary identity-mapped source registers.
        let low_gpr_bank = p0 & 0x20 != 0;
        let gpr_low = modrm & 0x07;
        if low_gpr_bank && matches!(gpr_low, 4 | 5) {
            return None;
        }

        Some(needs_fp16)
    }
}
