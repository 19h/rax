//! Deterministic host replay for architecturally ignored scalar EVEX L'L bits.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate a register-only scalar EVEX FMA3 whose `EVEX.b=0` makes L'L
    /// architecturally ignored, and return the equivalent L'L=00 host image.
    /// Embedded-rounding forms retain L'L because those bits select EVEX.RC.
    pub(crate) fn evex_scalar_fma_llig_canonical_ll0(&self) -> Option<Self> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 || bytes[3] & 0x10 != 0 {
            return None;
        }
        let valid = self.evex_register_scalar_fma_needs_vl() == Some(false)
            || self.evex_register_scalar_fp16_fma_needs_vl() == Some(false);
        if !valid {
            return None;
        }

        let mut canonical = *self;
        canonical.bytes[3] &= !0x60;
        let canonical_valid = canonical.evex_register_scalar_fma_needs_vl() == Some(false)
            || canonical.evex_register_scalar_fp16_fma_needs_vl() == Some(false);
        canonical_valid.then_some(canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_only_dynamic_scalar_fma_llig() {
        for map_w in [(2, false), (2, true), (6, false)] {
            for ll in 0..4 {
                let p0 = 0xE0 | map_w.0;
                let p1 = (u8::from(map_w.1) << 7) | 0x75;
                let dynamic =
                    X86InstructionBytes::new(&[0x62, p0, p1, 0x09 | (ll << 5), 0x99, 0xC2])
                        .unwrap();
                let canonical = dynamic
                    .evex_scalar_fma_llig_canonical_ll0()
                    .expect("dynamic scalar FMA3");
                assert_eq!(canonical.as_slice()[3], 0x09);

                let embedded =
                    X86InstructionBytes::new(&[0x62, p0, p1, 0x19 | (ll << 5), 0x99, 0xC2])
                        .unwrap();
                assert_eq!(
                    embedded.evex_scalar_fma_llig_canonical_ll0(),
                    None,
                    "embedded rounding must retain RC={ll}"
                );
            }
        }
    }

    #[test]
    fn rejects_packed_memory_and_non_fma_shapes() {
        for bytes in [
            [0x62, 0xE2, 0x75, 0x09, 0x98, 0xC2],
            [0x62, 0xE2, 0x75, 0x09, 0x99, 0x02],
            [0x62, 0xE2, 0x75, 0x09, 0x58, 0xC2],
        ] {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(instruction.evex_scalar_fma_llig_canonical_ll0(), None);
        }
    }
}
