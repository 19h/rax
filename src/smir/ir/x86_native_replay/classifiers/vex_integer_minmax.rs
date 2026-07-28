//! Complete VEX packed-integer minimum/maximum memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete VEX packed signed/unsigned integer minimum or
    /// maximum instruction whose second source operand is memory and return
    /// `(destination, first source, element type, minimum, signed, width,
    /// map, opcode, W)`.
    ///
    /// Original unsigned-byte and signed-word forms use map 0F. Signed-byte,
    /// signed-doubleword, unsigned-word, and unsigned-doubleword forms use map
    /// 0F38. Every form requires mandatory prefix 66H and specifies WIG. The
    /// shared parser validates every prefix, ModR/M, SIB, displacement, and
    /// complete-instruction boundary before this semantic classification.
    pub(crate) fn vex_memory_integer_minmax_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, bool, bool, VecWidth, u8, u8, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.pp != 1 {
            return None;
        }
        let (elem, minimum, signed) = match (fields.map, fields.opcode) {
            (1, 0xDA) => (VecElementType::I8, true, false),
            (1, 0xDE) => (VecElementType::I8, false, false),
            (1, 0xEA) => (VecElementType::I16, true, true),
            (1, 0xEE) => (VecElementType::I16, false, true),
            (2, 0x38) => (VecElementType::I8, true, true),
            (2, 0x39) => (VecElementType::I32, true, true),
            (2, 0x3A) => (VecElementType::I16, true, false),
            (2, 0x3B) => (VecElementType::I32, true, false),
            (2, 0x3C) => (VecElementType::I8, false, true),
            (2, 0x3D) => (VecElementType::I32, false, true),
            (2, 0x3E) => (VecElementType::I16, false, false),
            (2, 0x3F) => (VecElementType::I32, false, false),
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            elem,
            minimum,
            signed,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            fields.map,
            fields.opcode,
            fields.w,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [(u8, u8, VecElementType, bool, bool); 12] = [
        (1, 0xDA, VecElementType::I8, true, false),
        (1, 0xDE, VecElementType::I8, false, false),
        (1, 0xEA, VecElementType::I16, true, true),
        (1, 0xEE, VecElementType::I16, false, true),
        (2, 0x38, VecElementType::I8, true, true),
        (2, 0x39, VecElementType::I32, true, true),
        (2, 0x3A, VecElementType::I16, true, false),
        (2, 0x3B, VecElementType::I32, true, false),
        (2, 0x3C, VecElementType::I8, false, true),
        (2, 0x3D, VecElementType::I32, false, true),
        (2, 0x3E, VecElementType::I16, false, false),
        (2, 0x3F, VecElementType::I32, false, false),
    ];

    fn vex2_instruction(
        destination: u8,
        source1: u8,
        base: u8,
        opcode: u8,
        width: VecWidth,
    ) -> Vec<u8> {
        assert!(base < 8);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC5,
            (if destination < 8 { 0x80 } else { 0 }) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | base,
            0x20,
        ]
    }

    fn vex3_instruction(
        destination: u8,
        source1: u8,
        base: u8,
        map: u8,
        opcode: u8,
        width: VecWidth,
        w: bool,
    ) -> Vec<u8> {
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
    fn classifies_every_destination_source_kind_width_form_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for (map, opcode, elem, minimum, signed) in KINDS {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        if map == 1 {
                            let bytes = vex2_instruction(destination, source1, 3, opcode, width);
                            let metadata = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                metadata.vex_memory_integer_minmax_fields(),
                                Some((
                                    destination,
                                    source1,
                                    elem,
                                    minimum,
                                    signed,
                                    width,
                                    map,
                                    opcode,
                                    false,
                                )),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }

                        for base in [3, 11] {
                            for w in [false, true] {
                                let bytes = vex3_instruction(
                                    destination,
                                    source1,
                                    base,
                                    map,
                                    opcode,
                                    width,
                                    w,
                                );
                                let metadata = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    metadata.vex_memory_integer_minmax_fields(),
                                    Some((
                                        destination,
                                        source1,
                                        elem,
                                        minimum,
                                        signed,
                                        width,
                                        map,
                                        opcode,
                                        w,
                                    )),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * (4 * 5 + 8 * 4));
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shapes() {
        // addr32 FS: VPMINSD ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x02, 0xB5, 0x39, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_integer_minmax_fields(),
            Some((
                14,
                9,
                VecElementType::I32,
                true,
                true,
                VecWidth::V256,
                2,
                0x39,
                true,
            ))
        );

        // GS: VPMAXUB xmm14,xmm9,[r14+r15*2+0x44332211]
        let bytes = [
            0x65, 0xC4, 0x01, 0xB1, 0xDE, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_integer_minmax_fields(),
            Some((
                14,
                9,
                VecElementType::I8,
                false,
                false,
                VecWidth::V128,
                1,
                0xDE,
                true,
            ))
        );
    }

    #[test]
    fn malformed_or_semantically_different_memory_encodings_fail_closed() {
        let valid = vex3_instruction(3, 9, 11, 2, 0x39, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 3;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x37;
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
        cases.push(vec![0xC5, 0xF1, 0x39, 0x40, 0x20]);

        for bytes in cases {
            let metadata = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                metadata.vex_memory_integer_minmax_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
