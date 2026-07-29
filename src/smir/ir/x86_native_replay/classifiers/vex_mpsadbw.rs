//! Complete VEX VMPSADBW memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

impl X86InstructionBytes {
    /// Validate one complete VEX `VMPSADBW` instruction whose second source is
    /// memory and return `(destination, first source, vector width, imm8, W)`.
    ///
    /// Intel SDM Volume 2 assigns VMPSADBW to map 0F3A, mandatory prefix 66H,
    /// opcode 42H. VEX.128 requires AVX and VEX.256 requires AVX2. VEX.W is
    /// ignored and retained so helper-backed replay preserves source-byte
    /// provenance. The shared parser validates the complete
    /// ModR/M/SIB/displacement plus imm8 shape and accepts only
    /// segment/address-size legacy prefixes.
    ///
    /// Runtime and auxiliary space are O(1) because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn vex_memory_mpsadbw_fields(&self) -> Option<(u8, u8, VecWidth, u8, bool)> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || fields.opcode != 0x42 {
            return None;
        }
        Some((
            fields.destination,
            fields.source1,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            immediate,
            fields.w,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(
        destination: u8,
        source1: u8,
        base: u8,
        width: VecWidth,
        immediate: u8,
        w: bool,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
            0x42,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    #[test]
    fn classifies_all_262_144_destination_source_width_w_and_immediate_cells() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for width in [VecWidth::V128, VecWidth::V256] {
                    for w in [false, true] {
                        for immediate in u8::MIN..=u8::MAX {
                            let bytes = instruction(destination, source1, 11, width, immediate, w);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_mpsadbw_fields(),
                                Some((destination, source1, width, immediate, w)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 2 * 256);
    }

    #[test]
    fn llvm_23_and_complete_prefixed_address_shapes_classify_exactly() {
        // Independently assembled with LLVM 23. W1 aliases are obtained by
        // toggling the architecturally ignored VEX.W bit.
        let cases: &[(&[u8], (u8, u8, VecWidth, u8, bool))] = &[
            (
                &[0xC4, 0x43, 0x2D, 0x42, 0x4B, 0x20, 0xA5],
                (9, 10, VecWidth::V256, 0xA5, false),
            ),
            (
                &[0xC4, 0xE3, 0x69, 0x42, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x3C],
                (1, 2, VecWidth::V128, 0x3C, false),
            ),
            (
                &[
                    0x64, 0xC4, 0xE3, 0x75, 0x42, 0x04, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x5A,
                ],
                (0, 1, VecWidth::V256, 0x5A, false),
            ),
            (
                &[0x65, 0xC4, 0x03, 0x29, 0x42, 0x4C, 0xEC, 0x20, 0x03],
                (9, 10, VecWidth::V128, 0x03, false),
            ),
            (
                &[
                    0x64, 0x67, 0xC4, 0x03, 0xAD, 0x42, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xFF,
                ],
                (14, 10, VecWidth::V256, 0xFF, true),
            ),
        ];
        for (bytes, expected) in cases {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_memory_mpsadbw_fields(),
                Some(*expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = instruction(9, 10, 11, VecWidth::V256, 0xA5, true);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] ^= 1;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.remove(5);
        cases.push(register_source);

        let mut missing_immediate = valid.clone();
        missing_immediate.pop();
        cases.push(missing_immediate);

        let mut truncated_displacement = valid.clone();
        truncated_displacement.remove(5);
        cases.push(truncated_displacement);

        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut forbidden_legacy_prefix = valid.clone();
        forbidden_legacy_prefix.insert(0, 0x66);
        cases.push(forbidden_legacy_prefix);

        let mut non_vex = valid;
        non_vex[0] = 0x62;
        cases.push(non_vex);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_mpsadbw_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
