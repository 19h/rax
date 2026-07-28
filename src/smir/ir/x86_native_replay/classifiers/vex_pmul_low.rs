//! Complete VEX packed low-product multiply memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete VEX `VPMULLW` or `VPMULLD` instruction whose
    /// second source operand is memory and return
    /// `(destination, first source, element type, width, W)`.
    ///
    /// `VPMULLW` uses map 0F, mandatory prefix 66H, and opcode D5H.
    /// `VPMULLD` uses map 0F38, mandatory prefix 66H, and opcode 40H. VEX.W is
    /// ignored by both instructions and retained here so callers can prove the
    /// guest byte provenance before canonicalizing native replay to W=0. The
    /// shared parser validates the complete ModR/M/SIB/displacement byte shape
    /// and permits only segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_pmul_low_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, VecWidth, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.pp != 1 {
            return None;
        }
        let elem = match (fields.map, fields.opcode) {
            (1, 0xD5) => VecElementType::I16,
            (2, 0x40) => VecElementType::I32,
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            elem,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            fields.w,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_and_opcode(elem: VecElementType) -> (u8, u8) {
        match elem {
            VecElementType::I16 => (1, 0xD5),
            VecElementType::I32 => (2, 0x40),
            _ => unreachable!("packed low-product test element"),
        }
    }

    fn vex2_instruction(destination: u8, source1: u8, base: u8, width: VecWidth) -> Vec<u8> {
        assert!(base < 8);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC5,
            (if destination < 8 { 0x80 } else { 0 }) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
            0xD5,
            0x40 | ((destination & 7) << 3) | base,
            0x20,
        ]
    }

    fn vex3_instruction(
        destination: u8,
        source1: u8,
        base: u8,
        elem: VecElementType,
        width: VecWidth,
        w: bool,
    ) -> Vec<u8> {
        let (map, opcode) = map_and_opcode(elem);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if base < 8 { 0x20 } else { 0 })
                | map,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    #[test]
    fn classifies_every_destination_source_element_width_form_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for elem in [VecElementType::I16, VecElementType::I32] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        if elem == VecElementType::I16 {
                            let bytes = vex2_instruction(destination, source1, 3, width);
                            let metadata = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                metadata.vex_memory_pmul_low_fields(),
                                Some((destination, source1, elem, width, false)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }

                        for base in [3, 11] {
                            for w in [false, true] {
                                let bytes =
                                    vex3_instruction(destination, source1, base, elem, width, w);
                                let metadata = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    metadata.vex_memory_pmul_low_fields(),
                                    Some((destination, source1, elem, width, w)),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * (5 + 4));
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shapes() {
        // addr32 FS: VPMULLD ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x02, 0xB5, 0x40, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_pmul_low_fields(),
            Some((14, 9, VecElementType::I32, VecWidth::V256, true))
        );

        // GS: VPMULLW xmm14,xmm9,[r14+r15*2+0x44332211]
        let bytes = [
            0x65, 0xC4, 0x01, 0xB1, 0xD5, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_pmul_low_fields(),
            Some((14, 9, VecElementType::I16, VecWidth::V128, true))
        );
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = vex3_instruction(3, 9, 11, VecElementType::I32, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x41;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.truncate(5);
        cases.push(register_source);

        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut truncated = valid.clone();
        truncated.pop();
        cases.push(truncated);

        let mut forbidden_legacy_prefix = valid;
        forbidden_legacy_prefix.insert(0, 0x66);
        cases.push(forbidden_legacy_prefix);

        // Map 0F38 cannot be represented by VEX2.
        cases.push(vec![0xC5, 0xF1, 0x40, 0x40, 0x20]);

        for bytes in cases {
            let metadata = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(metadata.vex_memory_pmul_low_fields(), None, "{bytes:02X?}");
        }
    }
}
