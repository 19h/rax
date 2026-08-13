//! Deterministic canonicalization of non-memory legacy prefix groups.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Remove at most one segment-override prefix and one address-size prefix
    /// from the complete leading legacy-prefix sequence.
    ///
    /// Intel classifies these prefixes on legacy instructions whose operands
    /// do not reside in memory as reserved and potentially unpredictable. RAX
    /// already gives decoder-accepted register forms the same deterministic
    /// SMIR semantics as their unprefixed encoding. Native replay therefore
    /// emits the canonical byte string rather than executing the reserved
    /// source image on the host.
    ///
    /// This primitive establishes only exact prefix syntax. The caller must
    /// independently prove that the canonical bytes identify a supported
    /// register-only replay family and that the complete SMIR group has no
    /// memory or alignment effects. Other prefix groups are preserved byte for
    /// byte so the strict family classifier can reject LOCK, conflicting
    /// mandatory prefixes, non-final REX, malformed lengths, and unrelated
    /// instructions. Runtime is O(L) and auxiliary space is O(1), where
    /// `L <= 15` bytes.
    pub(crate) fn non_memory_prefix_canonical(&self) -> Option<Self> {
        let bytes = self.as_slice();
        let mut canonical = [0u8; 15];
        let mut input = 0usize;
        let mut output = 0usize;
        let mut segment_seen = false;
        let mut address_seen = false;

        while let Some(&byte) = bytes.get(input) {
            match byte {
                0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 => {
                    if segment_seen {
                        return None;
                    }
                    segment_seen = true;
                }
                0x67 => {
                    if address_seen {
                        return None;
                    }
                    address_seen = true;
                }
                0xF0 | 0xF2 | 0xF3 | 0x66 => {
                    canonical[output] = byte;
                    output += 1;
                }
                _ => break,
            }
            input += 1;
        }

        if !segment_seen && !address_seen {
            return None;
        }
        let suffix = &bytes[input..];
        if matches!(suffix.first(), Some(0x62 | 0xC4 | 0xC5 | 0xD5)) {
            return None;
        }
        canonical[output..output + suffix.len()].copy_from_slice(suffix);
        X86InstructionBytes::new(&canonical[..output + suffix.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizer_removes_each_segment_and_address_group_in_either_order() {
        const SEGMENTS: [u8; 6] = [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65];
        let canonical = [0x66, 0x45, 0x0F, 0x58, 0xCA];
        let mut checked = 0usize;

        for prefix in SEGMENTS.into_iter().chain([0x67]) {
            let mut source = vec![prefix];
            source.extend(canonical);
            assert_eq!(
                X86InstructionBytes::new(&source)
                    .unwrap()
                    .non_memory_prefix_canonical()
                    .unwrap()
                    .as_slice(),
                canonical,
                "{source:02X?}"
            );
            checked += 1;
        }

        for segment in SEGMENTS {
            for prefixes in [[segment, 0x67], [0x67, segment]] {
                let mut source = prefixes.to_vec();
                source.extend(canonical);
                assert_eq!(
                    X86InstructionBytes::new(&source)
                        .unwrap()
                        .non_memory_prefix_canonical()
                        .unwrap()
                        .as_slice(),
                    canonical,
                    "{source:02X?}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 7 + 2 * SEGMENTS.len());
    }

    #[test]
    fn canonicalizer_preserves_all_other_bytes_and_prefix_order() {
        for (source, expected) in [
            (
                &[0x67, 0xF3, 0x4D, 0x0F, 0x58, 0xCA][..],
                &[0xF3, 0x4D, 0x0F, 0x58, 0xCA][..],
            ),
            (
                &[0xF2, 0x64, 0x67, 0x4F, 0x0F, 0x58, 0xCA][..],
                &[0xF2, 0x4F, 0x0F, 0x58, 0xCA][..],
            ),
            (
                &[0x67, 0xF0, 0x0F, 0x58, 0xCA][..],
                &[0xF0, 0x0F, 0x58, 0xCA][..],
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(source)
                    .unwrap()
                    .non_memory_prefix_canonical()
                    .unwrap()
                    .as_slice(),
                expected,
                "{source:02X?}"
            );
        }
    }

    #[test]
    fn canonicalizer_rejects_absence_duplicates_and_prefix_only_images() {
        for bytes in [
            &[0x0F, 0x58, 0xC0][..],
            &[0x0F, 0xC6, 0xC0, 0x67][..],
            &[0x64, 0x65, 0x0F, 0x58, 0xC0][..],
            &[0x67, 0x67, 0x0F, 0x58, 0xC0][..],
            &[0x64, 0xC5, 0xF8, 0x58, 0xC0][..],
            &[0x67, 0xC4, 0xE1, 0x78, 0x58, 0xC0][..],
            &[0x65, 0x62, 0xF1, 0x7C, 0x08, 0x58, 0xC0][..],
            &[0x67, 0xD5, 0x00, 0x01, 0xC0][..],
            &[0x64][..],
            &[0x67][..],
            &[0x64, 0x67][..],
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .non_memory_prefix_canonical(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
