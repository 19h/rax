//! AVX VEX floating-point dot-product replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one exact register-only VEX `VDPPS` or `VDPPD` instruction
    /// and return whether its destination is 256-bit.
    ///
    /// Intel SDM Volume 2 assigns both instructions to map 0F3A with mandatory
    /// prefix 66H and architecturally ignored VEX.W. `VDPPS` admits VEX.128
    /// and VEX.256; `VDPPD` admits only VEX.128 and raises #UD for VEX.L=1.
    /// VEX.X is ignored for a register ModR/M operand. Memory sources remain
    /// excluded so replay cannot bypass guest translation or precise faults.
    pub fn vex_register_fp_dot_product_uses_ymm(&self) -> Option<bool> {
        let [0xC4, p0, p1, opcode, modrm, _imm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let ymm = p1 & 0x04 != 0;
        match opcode {
            0x40 => Some(ymm),
            0x41 if !ymm => Some(false),
            _ => None,
        }
    }

    /// Architectural XMM/YMM destination selected by an exact VEX dot
    /// product. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_fp_dot_product_destination_index(&self) -> Option<u8> {
        self.vex_register_fp_dot_product_uses_ymm()?;
        let [0xC4, p0, _p1, _opcode, modrm, _imm] = self.as_slice() else {
            unreachable!()
        };
        Some((u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7))
    }

    /// Validate one complete AVX VEX `VDPPS` or `VDPPD` instruction whose
    /// second source is memory and return
    /// `(destination, source1, element, width, immediate, W)`.
    ///
    /// Both instructions use map 0F3A with mandatory prefix 66H and define
    /// VEX.W as ignored. `VDPPS` admits VEX.128 and VEX.256; `VDPPD` admits
    /// only VEX.128. The shared parser accepts only segment/address-size
    /// legacy prefixes and validates the complete ModR/M/SIB/displacement
    /// plus imm8 shape.
    pub(crate) fn vex_memory_fp_dot_product_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, VecWidth, u8, bool)> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 {
            return None;
        }
        let (elem, width) = match (fields.opcode, fields.width_256) {
            (0x40, false) => (VecElementType::F32, VecWidth::V128),
            (0x40, true) => (VecElementType::F32, VecWidth::V256),
            (0x41, false) => (VecElementType::F64, VecWidth::V128),
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            elem,
            width,
            immediate,
            fields.w,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding(
        extension_bits: u8,
        w: bool,
        encoded_vvvv: u8,
        ymm: bool,
        opcode: u8,
        modrm: u8,
        immediate: u8,
    ) -> [u8; 6] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        [
            0xC4,
            extension_bits | 3,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(ymm) << 2) | 1,
            opcode,
            modrm,
            immediate,
        ]
    }

    #[test]
    fn classifier_exhaustively_covers_262_144_extension_w_vvvv_l_opcode_modrm_shapes() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for opcode in [0x40, 0x41] {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for w in [false, true] {
                    for encoded_vvvv in 0u8..16 {
                        for ymm in [false, true] {
                            for modrm in u8::MIN..=u8::MAX {
                                let bytes = encoding(
                                    extension_bits,
                                    w,
                                    encoded_vvvv,
                                    ymm,
                                    opcode,
                                    modrm,
                                    0xA5,
                                );
                                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                                let expected =
                                    (modrm >> 6 == 3 && (opcode == 0x40 || !ymm)).then_some(ymm);
                                assert_eq!(
                                    instruction.vex_register_fp_dot_product_uses_ymm(),
                                    expected,
                                    "{bytes:02X?}"
                                );
                                let destination_extension =
                                    u8::from(extension_bits & 0x80 == 0) << 3;
                                assert_eq!(
                                    instruction.vex_fp_dot_product_destination_index(),
                                    expected
                                        .map(|_| { destination_extension | ((modrm >> 3) & 7) }),
                                    "{bytes:02X?}"
                                );
                                accepted += usize::from(expected.is_some());
                                tested += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 49_152);
        assert_eq!(tested, 262_144);
    }

    #[test]
    fn classifier_exhaustively_rejects_wrong_map_pp_opcode_length_and_vdppd_l1() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for map in 0u8..32 {
            for pp in 0u8..4 {
                for opcode in u8::MIN..=u8::MAX {
                    for ymm in [false, true] {
                        for has_immediate in [false, true] {
                            let mut bytes = vec![
                                0xC4,
                                0xE0 | map,
                                0x80 | (0x0D << 3) | (u8::from(ymm) << 2) | pp,
                                opcode,
                                0xCA,
                            ];
                            if has_immediate {
                                bytes.push(0x5A);
                            }
                            let expected = map == 3
                                && pp == 1
                                && has_immediate
                                && (opcode == 0x40 || (opcode == 0x41 && !ymm));
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                instruction.vex_register_fp_dot_product_uses_ymm(),
                                expected.then_some(ymm),
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected);
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 3);
        assert_eq!(tested, 131_072);
    }

    #[test]
    fn classifier_accepts_all_immediates_wig_ignored_x_and_llvm_encodings() {
        for immediate in u8::MIN..=u8::MAX {
            for extension_bits in [0xE0, 0xA0] {
                for w in [false, true] {
                    for (opcode, ymm) in [(0x40, false), (0x40, true), (0x41, false)] {
                        let bytes = encoding(extension_bits, w, 0x0D, ymm, opcode, 0xCA, immediate);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_register_fp_dot_product_uses_ymm(),
                            Some(ymm),
                            "{bytes:02X?}"
                        );
                    }
                }
            }
        }

        // Independently assembled by LLVM 23. W1 aliases are obtained by
        // toggling architecturally ignored VEX.W in the corresponding result.
        for (bytes, ymm, destination) in [
            (&[0xC4, 0x43, 0x29, 0x40, 0xCB, 0xA5][..], false, 9),
            (&[0xC4, 0x43, 0xA9, 0x40, 0xCB, 0xA5][..], false, 9),
            (&[0xC4, 0x43, 0x0D, 0x40, 0xFD, 0x5A][..], true, 15),
            (&[0xC4, 0x43, 0x8D, 0x40, 0xFD, 0x5A][..], true, 15),
            (&[0xC4, 0x43, 0x29, 0x41, 0xCB, 0x3C][..], false, 9),
            (&[0xC4, 0x43, 0xA9, 0x41, 0xCB, 0x3C][..], false, 9),
        ] {
            let instruction = X86InstructionBytes::new(bytes).unwrap();
            assert_eq!(
                instruction.vex_register_fp_dot_product_uses_ymm(),
                Some(ymm),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_fp_dot_product_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_rejects_memory_incomplete_trailing_non_vex_and_invalid_l1() {
        let register = encoding(0xE0, true, 0x0D, true, 0x40, 0xCA, 0xA5);
        let mut memory = register;
        memory[4] &= 0x3F;
        let invalid_l1 = encoding(0xE0, false, 0x0D, true, 0x41, 0xCA, 0xA5);
        for bytes in [
            register[..5].to_vec(),
            register.iter().copied().chain([0]).collect(),
            memory.to_vec(),
            invalid_l1.to_vec(),
            vec![0x66, 0x0F, 0x3A, 0x40, 0xCA, 0xA5],
            vec![0x62, 0xF3, 0x75, 0x08, 0x40, 0xCA, 0xA5],
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_fp_dot_product_uses_ymm(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    fn memory_encoding(
        destination: u8,
        source1: u8,
        base: u8,
        elem: VecElementType,
        width: VecWidth,
        immediate: u8,
        w: bool,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        assert!(
            (elem, width) != (VecElementType::F64, VecWidth::V256),
            "VDPPD has no VEX.256 encoding"
        );
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7)
                | (((!source1) & 0x0F) << 3)
                | (u8::from(width == VecWidth::V256) << 2)
                | 1,
            if elem == VecElementType::F32 {
                0x40
            } else {
                0x41
            },
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    #[test]
    fn memory_classifier_covers_all_393_216_register_width_w_and_immediate_cells() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for (elem, width) in [
                    (VecElementType::F32, VecWidth::V128),
                    (VecElementType::F32, VecWidth::V256),
                    (VecElementType::F64, VecWidth::V128),
                ] {
                    for w in [false, true] {
                        for immediate in u8::MIN..=u8::MAX {
                            let bytes =
                                memory_encoding(destination, source1, 3, elem, width, immediate, w);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_fp_dot_product_fields(),
                                Some((destination, source1, elem, width, immediate, w)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 3 * 2 * 256);
    }

    #[test]
    fn memory_classifier_accepts_llvm_23_and_complete_prefixed_address_shapes() {
        for (bytes, expected) in [
            (
                vec![0xC4, 0x43, 0x29, 0x40, 0x4B, 0x20, 0xA5],
                (9, 10, VecElementType::F32, VecWidth::V128, 0xA5, false),
            ),
            (
                vec![0xC4, 0x43, 0x0D, 0x40, 0x7B, 0x20, 0x5A],
                (15, 14, VecElementType::F32, VecWidth::V256, 0x5A, false),
            ),
            (
                vec![0xC4, 0x43, 0x29, 0x41, 0x4B, 0x20, 0x3C],
                (9, 10, VecElementType::F64, VecWidth::V128, 0x3C, false),
            ),
            (
                vec![
                    0x64, 0x67, 0xC4, 0x63, 0xAD, 0x40, 0xB4, 0x75, 0x11, 0x22, 0x33, 0x44, 0xA5,
                ],
                (14, 10, VecElementType::F32, VecWidth::V256, 0xA5, true),
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_fp_dot_product_fields(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn memory_classifier_rejects_semantically_different_or_malformed_encodings() {
        let valid = memory_encoding(9, 10, 11, VecElementType::F32, VecWidth::V256, 0xA5, true);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x42;
        cases.push(wrong_opcode);

        let mut invalid_vdppd_l1 = valid.clone();
        invalid_vdppd_l1[3] = 0x41;
        cases.push(invalid_vdppd_l1);

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

        let mut forbidden_prefix = valid;
        forbidden_prefix.insert(0, 0x66);
        cases.push(forbidden_prefix);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_fp_dot_product_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
