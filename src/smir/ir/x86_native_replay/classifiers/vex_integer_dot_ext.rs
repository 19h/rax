//! Register-only AVX-VNNI-INT8/INT16 VEX dot-product replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one exact register-only AVX-VNNI-INT8 or AVX-VNNI-INT16
    /// extended integer dot product and return whether it is a word variant.
    ///
    /// Intel SDM Volume 2 assigns the byte variants to VEX map 0F38 opcodes
    /// 50H/51H with NP, F3, or F2 and the word variants to opcodes D2H/D3H
    /// with NP, 66H, or F3. Every defined form is W0 and admits VEX.128 or
    /// VEX.256. VEX.X is ignored for a register ModR/M operand. Memory sources
    /// remain excluded so replay cannot bypass guest translation or faults.
    pub fn vex_register_integer_dot_ext_is_int16(&self) -> Option<bool> {
        let [0xC4, p0, p1, opcode, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 2 || p1 & 0x80 != 0 || modrm >> 6 != 3 {
            return None;
        }

        let pp = p1 & 0x03;
        match (opcode, pp) {
            (0x50 | 0x51, 0 | 2 | 3) => Some(false),
            (0xD2 | 0xD3, 0 | 1 | 2) => Some(true),
            _ => None,
        }
    }

    /// Architectural XMM/YMM destination selected by an exact extended
    /// integer dot product. The ModR/M.reg field is extended by inverted
    /// VEX.R.
    pub(crate) fn vex_integer_dot_ext_destination_index(&self) -> Option<u8> {
        self.vex_register_integer_dot_ext_is_int16()?;
        let [0xC4, p0, _p1, _opcode, modrm] = self.as_slice() else {
            unreachable!()
        };
        Some((u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAPES: [(u8, u8, bool); 12] = [
        (3, 0x50, false),
        (3, 0x51, false),
        (2, 0x50, false),
        (2, 0x51, false),
        (0, 0x50, false),
        (0, 0x51, false),
        (2, 0xD2, true),
        (2, 0xD3, true),
        (1, 0xD2, true),
        (1, 0xD3, true),
        (0, 0xD2, true),
        (0, 0xD3, true),
    ];

    fn encoding(
        extension_bits: u8,
        encoded_vvvv: u8,
        ymm: bool,
        pp: u8,
        opcode: u8,
        modrm: u8,
    ) -> [u8; 5] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        assert!(pp < 4);
        [
            0xC4,
            extension_bits | 2,
            (encoded_vvvv << 3) | (u8::from(ymm) << 2) | pp,
            opcode,
            modrm,
        ]
    }

    #[test]
    fn classifier_exhaustively_covers_786_432_extension_vvvv_l_and_modrm_shapes() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for (pp, opcode, int16) in SHAPES {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        for modrm in u8::MIN..=u8::MAX {
                            let bytes =
                                encoding(extension_bits, encoded_vvvv, ymm, pp, opcode, modrm);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let expected = (modrm >> 6 == 3).then_some(int16);
                            assert_eq!(
                                instruction.vex_register_integer_dot_ext_is_int16(),
                                expected,
                                "{bytes:02X?}"
                            );
                            let destination_extension = u8::from(extension_bits & 0x80 == 0) << 3;
                            assert_eq!(
                                instruction.vex_integer_dot_ext_destination_index(),
                                expected.map(|_| destination_extension | ((modrm >> 3) & 7)),
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected.is_some());
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 196_608);
        assert_eq!(tested, 786_432);
    }

    #[test]
    fn classifier_exhaustively_rejects_wrong_map_pp_opcode_w_and_length() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for map in 0u8..32 {
            for pp in 0u8..4 {
                for opcode in u8::MIN..=u8::MAX {
                    for w in [false, true] {
                        for has_modrm in [false, true] {
                            let mut bytes = vec![
                                0xC4,
                                0xE0 | map,
                                (u8::from(w) << 7) | (0x0D << 3) | 0x04 | pp,
                                opcode,
                            ];
                            if has_modrm {
                                bytes.push(0xCA);
                            }
                            let expected = if has_modrm && !w && map == 2 {
                                SHAPES.iter().find_map(|&(shape_pp, shape_opcode, int16)| {
                                    (pp == shape_pp && opcode == shape_opcode).then_some(int16)
                                })
                            } else {
                                None
                            };
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                instruction.vex_register_integer_dot_ext_is_int16(),
                                expected,
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected.is_some());
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 12);
        assert_eq!(tested, 131_072);
    }

    #[test]
    fn classifier_accepts_llvm_encodings_and_ignored_x_but_rejects_other_forms() {
        // Independently assembled by LLVM 23 with +avxvnniint8 or
        // +avxvnniint16.
        for (bytes, int16, destination) in [
            (&[0xC4, 0x42, 0x2B, 0x50, 0xCB][..], false, 9),
            (&[0xC4, 0x42, 0x0E, 0x51, 0xFD][..], false, 15),
            (&[0xC4, 0xE2, 0x68, 0x51, 0xCB][..], false, 1),
            (&[0xC4, 0x42, 0x2A, 0xD2, 0xCB][..], true, 9),
            (&[0xC4, 0x42, 0x0D, 0xD3, 0xFD][..], true, 15),
            (&[0xC4, 0xE2, 0x68, 0xD3, 0xCB][..], true, 1),
        ] {
            for clear_ignored_x in [false, true] {
                let mut bytes = bytes.to_vec();
                if clear_ignored_x {
                    bytes[1] &= !0x40;
                }
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                assert_eq!(
                    instruction.vex_register_integer_dot_ext_is_int16(),
                    Some(int16),
                    "{bytes:02X?}"
                );
                assert_eq!(
                    instruction.vex_integer_dot_ext_destination_index(),
                    Some(destination),
                    "{bytes:02X?}"
                );
            }
        }

        for bytes in [
            vec![0xC4, 0xE2, 0x68, 0x50],
            vec![0xC4, 0xE2, 0x68, 0x50, 0xCB, 0],
            vec![0xC4, 0xE2, 0xE8, 0x50, 0xCB],
            vec![0xC4, 0xE3, 0x68, 0x50, 0xCB],
            vec![0xC4, 0xE2, 0x69, 0x50, 0xCB],
            vec![0xC4, 0xE2, 0x6B, 0xD2, 0xCB],
            vec![0xC4, 0xE2, 0x68, 0x50, 0x0B],
            vec![0x62, 0xF2, 0x6B, 0x08, 0x50, 0xCB],
            vec![0xC5, 0xE8, 0x50, 0xCB],
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_integer_dot_ext_is_int16(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
