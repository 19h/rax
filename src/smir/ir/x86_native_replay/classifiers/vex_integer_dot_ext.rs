//! AVX-VNNI-INT8/INT16 VEX dot-product replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::ops::X86SsePrefix;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Architectural fields for one complete AVX-VNNI-INT8/INT16 VEX
/// dot-product instruction whose third operand is memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexIntegerDotExtMemoryFields {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) src_elem: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) src1_signed: bool,
    pub(crate) src2_signed: bool,
    pub(crate) saturate: bool,
    pub(crate) prefix: X86SsePrefix,
    pub(crate) opcode: u8,
}

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

    /// Validate one complete AVX-VNNI-INT8 or AVX-VNNI-INT16 VEX dot
    /// product whose third operand is memory.
    ///
    /// Intel SDM Volume 2 assigns the byte variants to map 0F38 opcodes
    /// 50H/51H with F2, F3, or no mandatory prefix and the word variants to
    /// opcodes D2H/D3H with F3, 66H, or no mandatory prefix. All forms are
    /// W=0 and admit VEX.128 or VEX.256. The shared parser checks the complete
    /// prefix/ModR/M/SIB/displacement boundary and rejects register sources.
    pub(crate) fn vex_memory_integer_dot_ext_fields(
        &self,
    ) -> Option<X86VexIntegerDotExtMemoryFields> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.w {
            return None;
        }
        let prefix = match fields.pp {
            0 => X86SsePrefix::None,
            1 => X86SsePrefix::OpSize,
            2 => X86SsePrefix::Rep,
            3 => X86SsePrefix::Repne,
            _ => unreachable!("VEX.pp is two bits"),
        };
        let (src_elem, src1_signed, src2_signed) = match (fields.opcode, prefix) {
            (0x50 | 0x51, X86SsePrefix::Repne) => (VecElementType::I8, true, true),
            (0x50 | 0x51, X86SsePrefix::Rep) => (VecElementType::I8, true, false),
            (0x50 | 0x51, X86SsePrefix::None) => (VecElementType::I8, false, false),
            (0xD2 | 0xD3, X86SsePrefix::Rep) => (VecElementType::I16, true, false),
            (0xD2 | 0xD3, X86SsePrefix::OpSize) => (VecElementType::I16, false, true),
            (0xD2 | 0xD3, X86SsePrefix::None) => (VecElementType::I16, false, false),
            _ => return None,
        };
        Some(X86VexIntegerDotExtMemoryFields {
            destination: fields.destination,
            source1: fields.source1,
            src_elem,
            width: if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            src1_signed,
            src2_signed,
            saturate: fields.opcode & 1 != 0,
            prefix,
            opcode: fields.opcode,
        })
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

    fn memory_semantics(pp: u8, opcode: u8) -> Option<(VecElementType, bool, bool, X86SsePrefix)> {
        match (opcode, pp) {
            (0x50 | 0x51, 3) => Some((VecElementType::I8, true, true, X86SsePrefix::Repne)),
            (0x50 | 0x51, 2) => Some((VecElementType::I8, true, false, X86SsePrefix::Rep)),
            (0x50 | 0x51, 0) => Some((VecElementType::I8, false, false, X86SsePrefix::None)),
            (0xD2 | 0xD3, 2) => Some((VecElementType::I16, true, false, X86SsePrefix::Rep)),
            (0xD2 | 0xD3, 1) => Some((VecElementType::I16, false, true, X86SsePrefix::OpSize)),
            (0xD2 | 0xD3, 0) => Some((VecElementType::I16, false, false, X86SsePrefix::None)),
            _ => None,
        }
    }

    fn complete_memory_encoding(
        extension_bits: u8,
        encoded_vvvv: u8,
        ymm: bool,
        pp: u8,
        opcode: u8,
        modrm: u8,
    ) -> Vec<u8> {
        assert!(modrm >> 6 != 3);
        let mut bytes = encoding(extension_bits, encoded_vvvv, ymm, pp, opcode, modrm).to_vec();
        let mode = modrm >> 6;
        let rm = modrm & 7;
        if rm == 4 {
            // Scale=1, no index, base=5 exercises the no-base disp32 case
            // when Mod=00 and the ordinary SIB base otherwise.
            bytes.push(0x25);
            if mode == 0 {
                bytes.extend_from_slice(&0x4433_2211u32.to_le_bytes());
            }
        } else if mode == 0 && rm == 5 {
            bytes.extend_from_slice(&0x4433_2211u32.to_le_bytes());
        }
        match mode {
            1 => bytes.push(0x20),
            2 => bytes.extend_from_slice(&0x4433_2211u32.to_le_bytes()),
            _ => {}
        }
        bytes
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
    fn memory_classifier_exhaustively_covers_589_824_defined_prefix_vvvv_l_and_modrm_cells() {
        let mut classified = 0usize;
        for (pp, opcode, _) in SHAPES {
            let (src_elem, src1_signed, src2_signed, prefix) =
                memory_semantics(pp, opcode).unwrap();
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        for modrm in 0u8..=0xBF {
                            let bytes = complete_memory_encoding(
                                extension_bits,
                                encoded_vvvv,
                                ymm,
                                pp,
                                opcode,
                                modrm,
                            );
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let destination =
                                (u8::from(extension_bits & 0x80 == 0) << 3) | ((modrm >> 3) & 7);
                            assert_eq!(
                                instruction.vex_memory_integer_dot_ext_fields(),
                                Some(X86VexIntegerDotExtMemoryFields {
                                    destination,
                                    source1: (!encoded_vvvv) & 0x0F,
                                    src_elem,
                                    width: if ymm { VecWidth::V256 } else { VecWidth::V128 },
                                    src1_signed,
                                    src2_signed,
                                    saturate: opcode & 1 != 0,
                                    prefix,
                                    opcode,
                                }),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 589_824);
    }

    #[test]
    fn memory_classifier_exhaustively_rejects_wrong_map_pp_opcode_w_and_length() {
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
                                bytes.push(0x02);
                            }
                            let expected = if has_modrm && !w && map == 2 {
                                memory_semantics(pp, opcode).map(
                                    |(src_elem, src1_signed, src2_signed, prefix)| {
                                        X86VexIntegerDotExtMemoryFields {
                                            destination: 0,
                                            source1: 2,
                                            src_elem,
                                            width: VecWidth::V256,
                                            src1_signed,
                                            src2_signed,
                                            saturate: opcode & 1 != 0,
                                            prefix,
                                            opcode,
                                        }
                                    },
                                )
                            } else {
                                None
                            };
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                instruction.vex_memory_integer_dot_ext_fields(),
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
    fn memory_classifier_accepts_all_llvm_23_encodings_and_complete_address_prefixes() {
        // Independently assembled by LLVM 23 with +avxvnniint8 and
        // +avxvnniint16.
        let encodings: [(&[u8], VecElementType, bool, bool, bool, VecWidth); 12] = [
            (
                &[0xC4, 0x42, 0x2B, 0x50, 0x4B, 0x20],
                VecElementType::I8,
                true,
                true,
                false,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0x0F, 0x51, 0x7D, 0x20],
                VecElementType::I8,
                true,
                true,
                true,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x42, 0x2A, 0x50, 0x4B, 0x20],
                VecElementType::I8,
                true,
                false,
                false,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0x0E, 0x51, 0x7D, 0x20],
                VecElementType::I8,
                true,
                false,
                true,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x42, 0x28, 0x50, 0x4B, 0x20],
                VecElementType::I8,
                false,
                false,
                false,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0x0C, 0x51, 0x7D, 0x20],
                VecElementType::I8,
                false,
                false,
                true,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x42, 0x2A, 0xD2, 0x4B, 0x20],
                VecElementType::I16,
                true,
                false,
                false,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0x0E, 0xD3, 0x7D, 0x20],
                VecElementType::I16,
                true,
                false,
                true,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x42, 0x29, 0xD2, 0x4B, 0x20],
                VecElementType::I16,
                false,
                true,
                false,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0x0D, 0xD3, 0x7D, 0x20],
                VecElementType::I16,
                false,
                true,
                true,
                VecWidth::V256,
            ),
            (
                &[0xC4, 0x42, 0x28, 0xD2, 0x4B, 0x20],
                VecElementType::I16,
                false,
                false,
                false,
                VecWidth::V128,
            ),
            (
                &[0xC4, 0x42, 0x0C, 0xD3, 0x7D, 0x20],
                VecElementType::I16,
                false,
                false,
                true,
                VecWidth::V256,
            ),
        ];
        for (bytes, src_elem, src1_signed, src2_signed, saturate, width) in encodings {
            let fields = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_memory_integer_dot_ext_fields()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(
                fields.destination,
                if width == VecWidth::V128 { 9 } else { 15 }
            );
            assert_eq!(
                fields.source1,
                if width == VecWidth::V128 { 10 } else { 14 }
            );
            assert_eq!(fields.src_elem, src_elem);
            assert_eq!(fields.width, width);
            assert_eq!(fields.src1_signed, src1_signed);
            assert_eq!(fields.src2_signed, src2_signed);
            assert_eq!(fields.saturate, saturate);
        }

        let mut prefixed = vec![0x64, 0x67];
        prefixed.extend_from_slice(encodings[0].0);
        assert!(
            X86InstructionBytes::new(&prefixed)
                .unwrap()
                .vex_memory_integer_dot_ext_fields()
                .is_some()
        );
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
