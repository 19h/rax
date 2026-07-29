//! Base AVX-VNNI VEX integer dot-product replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Architectural fields for one complete base AVX-VNNI VEX dot-product
/// instruction whose third operand is memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexIntegerDotMemoryFields {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) src_elem: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) src1_unsigned: bool,
    pub(crate) saturate: bool,
    pub(crate) opcode: u8,
}

impl X86InstructionBytes {
    /// Validate one exact register-only base AVX-VNNI integer dot product.
    ///
    /// Intel SDM Volume 2 assigns VPDPBUSD, VPDPBUSDS, VPDPWSSD, and
    /// VPDPWSSDS to VEX map 0F38 opcodes 50H through 53H. Every form requires
    /// mandatory prefix 66H, W=0, and VEX.128 or VEX.256. VEX.X is ignored
    /// for a register ModR/M operand. Memory sources remain excluded so source
    /// replay cannot bypass guest translation or precise faults.
    pub(crate) fn vex_register_integer_dot_fields(
        &self,
    ) -> Option<(u8, u8, u8, VecElementType, VecWidth, bool, bool)> {
        let [0xC4, p0, p1, opcode @ 0x50..=0x53, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 2 || p1 & 0x83 != 1 || modrm >> 6 != 3 {
            return None;
        }

        Some((
            (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            (!p1 >> 3) & 0x0F,
            (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
            if *opcode < 0x52 {
                VecElementType::I8
            } else {
                VecElementType::I16
            },
            if p1 & 0x04 != 0 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            *opcode < 0x52,
            *opcode & 1 != 0,
        ))
    }

    /// Architectural XMM/YMM destination selected by an exact register-only
    /// base AVX-VNNI dot product.
    pub(crate) fn vex_integer_dot_destination_index(&self) -> Option<u8> {
        self.vex_register_integer_dot_fields()
            .map(|fields| fields.0)
    }

    /// Validate one complete base AVX-VNNI integer dot product whose third
    /// operand is memory.
    ///
    /// The shared parser verifies the complete prefix, ModR/M, SIB,
    /// displacement, and instruction boundary. This semantic layer then
    /// requires map 0F38, mandatory prefix 66H, W=0, and opcode 50H through
    /// 53H.
    pub(crate) fn vex_memory_integer_dot_fields(&self) -> Option<X86VexIntegerDotMemoryFields> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.pp != 1 || fields.w || !matches!(fields.opcode, 0x50..=0x53) {
            return None;
        }
        Some(X86VexIntegerDotMemoryFields {
            destination: fields.destination,
            source1: fields.source1,
            src_elem: if fields.opcode < 0x52 {
                VecElementType::I8
            } else {
                VecElementType::I16
            },
            width: if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            src1_unsigned: fields.opcode < 0x52,
            saturate: fields.opcode & 1 != 0,
            opcode: fields.opcode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding(extension_bits: u8, encoded_vvvv: u8, ymm: bool, opcode: u8, modrm: u8) -> [u8; 5] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        assert!(matches!(opcode, 0x50..=0x53));
        [
            0xC4,
            extension_bits | 2,
            (encoded_vvvv << 3) | (u8::from(ymm) << 2) | 1,
            opcode,
            modrm,
        ]
    }

    fn complete_memory_encoding(
        extension_bits: u8,
        encoded_vvvv: u8,
        ymm: bool,
        opcode: u8,
        modrm: u8,
    ) -> Vec<u8> {
        assert!(modrm >> 6 != 3);
        let mut bytes = encoding(extension_bits, encoded_vvvv, ymm, opcode, modrm).to_vec();
        let mode = modrm >> 6;
        let rm = modrm & 7;
        if rm == 4 {
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
    fn register_classifier_exhaustively_covers_all_262_144_prefix_and_modrm_cells() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for opcode in 0x50u8..=0x53 {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        for modrm in u8::MIN..=u8::MAX {
                            let bytes = encoding(extension_bits, encoded_vvvv, ymm, opcode, modrm);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let expected = (modrm >> 6 == 3).then(|| {
                                (
                                    (u8::from(extension_bits & 0x80 == 0) << 3)
                                        | ((modrm >> 3) & 7),
                                    (!encoded_vvvv) & 0x0F,
                                    (u8::from(extension_bits & 0x20 == 0) << 3) | (modrm & 7),
                                    if opcode < 0x52 {
                                        VecElementType::I8
                                    } else {
                                        VecElementType::I16
                                    },
                                    if ymm { VecWidth::V256 } else { VecWidth::V128 },
                                    opcode < 0x52,
                                    opcode & 1 != 0,
                                )
                            });
                            assert_eq!(
                                instruction.vex_register_integer_dot_fields(),
                                expected,
                                "{bytes:02X?}"
                            );
                            assert_eq!(
                                instruction.vex_integer_dot_destination_index(),
                                expected.map(|fields| fields.0),
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected.is_some());
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 65_536);
        assert_eq!(tested, 262_144);
    }

    #[test]
    fn memory_classifier_exhaustively_covers_all_196_608_defined_address_cells() {
        let mut accepted = 0usize;
        for opcode in 0x50u8..=0x53 {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        for modrm in u8::MIN..=u8::MAX {
                            if modrm >> 6 == 3 {
                                continue;
                            }
                            let bytes = complete_memory_encoding(
                                extension_bits,
                                encoded_vvvv,
                                ymm,
                                opcode,
                                modrm,
                            );
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                instruction.vex_memory_integer_dot_fields(),
                                Some(X86VexIntegerDotMemoryFields {
                                    destination: (u8::from(extension_bits & 0x80 == 0) << 3)
                                        | ((modrm >> 3) & 7),
                                    source1: (!encoded_vvvv) & 0x0F,
                                    src_elem: if opcode < 0x52 {
                                        VecElementType::I8
                                    } else {
                                        VecElementType::I16
                                    },
                                    width: if ymm { VecWidth::V256 } else { VecWidth::V128 },
                                    src1_unsigned: opcode < 0x52,
                                    saturate: opcode & 1 != 0,
                                    opcode,
                                }),
                                "{bytes:02X?}"
                            );
                            accepted += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 196_608);
    }

    #[test]
    fn classifiers_reject_wrong_map_prefix_w_opcode_operand_and_boundaries() {
        let valid_register = encoding(0xE0, 13, true, 0x53, 0xFD);
        let valid_memory = complete_memory_encoding(0xE0, 13, true, 0x53, 0x45);

        let mut register_cases = Vec::new();
        let mut wrong_map = valid_register;
        wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
        register_cases.push(wrong_map.to_vec());
        let mut wrong_prefix = valid_register;
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        register_cases.push(wrong_prefix.to_vec());
        let mut wrong_w = valid_register;
        wrong_w[2] |= 0x80;
        register_cases.push(wrong_w.to_vec());
        let mut wrong_opcode = valid_register;
        wrong_opcode[3] = 0x54;
        register_cases.push(wrong_opcode.to_vec());
        let mut memory_operand = valid_register;
        memory_operand[4] &= 0x3F;
        register_cases.push(memory_operand.to_vec());
        let mut trailing = valid_register.to_vec();
        trailing.push(0);
        register_cases.push(trailing);
        register_cases.push(valid_register[..4].to_vec());
        let mut legacy_prefix = valid_register.to_vec();
        legacy_prefix.insert(0, 0x66);
        register_cases.push(legacy_prefix);
        for bytes in register_cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_integer_dot_fields(),
                None,
                "{bytes:02X?}"
            );
        }

        let mut memory_cases = Vec::new();
        let mut wrong_map = valid_memory.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
        memory_cases.push(wrong_map);
        let mut wrong_prefix = valid_memory.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        memory_cases.push(wrong_prefix);
        let mut wrong_w = valid_memory.clone();
        wrong_w[2] |= 0x80;
        memory_cases.push(wrong_w);
        let mut wrong_opcode = valid_memory.clone();
        wrong_opcode[3] = 0x54;
        memory_cases.push(wrong_opcode);
        let mut register_operand = valid_memory.clone();
        register_operand[4] |= 0xC0;
        register_operand.truncate(5);
        memory_cases.push(register_operand);
        let mut trailing = valid_memory.clone();
        trailing.push(0);
        memory_cases.push(trailing);
        let mut truncated = valid_memory.clone();
        truncated.pop();
        memory_cases.push(truncated);
        let mut forbidden_prefix = valid_memory;
        forbidden_prefix.insert(0, 0x66);
        memory_cases.push(forbidden_prefix);
        for bytes in memory_cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_integer_dot_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn matches_independent_llvm_23_vex_register_and_memory_encodings() {
        let cases = [
            (
                &[0xC4, 0xE2, 0x69, 0x50, 0xCB][..],
                VecElementType::I8,
                VecWidth::V128,
                true,
                false,
            ),
            (
                &[0xC4, 0xE2, 0x4D, 0x51, 0xFD][..],
                VecElementType::I8,
                VecWidth::V256,
                true,
                true,
            ),
            (
                &[0xC4, 0xE2, 0x69, 0x52, 0xCB][..],
                VecElementType::I16,
                VecWidth::V128,
                false,
                false,
            ),
            (
                &[0xC4, 0xE2, 0x4D, 0x53, 0xFD][..],
                VecElementType::I16,
                VecWidth::V256,
                false,
                true,
            ),
        ];
        for (bytes, elem, width, unsigned, saturate) in cases {
            let fields = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_register_integer_dot_fields()
                .unwrap();
            assert_eq!(fields.3, elem, "{bytes:02X?}");
            assert_eq!(fields.4, width, "{bytes:02X?}");
            assert_eq!(fields.5, unsigned, "{bytes:02X?}");
            assert_eq!(fields.6, saturate, "{bytes:02X?}");
        }

        let cases = [
            (
                &[0xC4, 0xC2, 0x69, 0x50, 0x4B, 0x20][..],
                VecElementType::I8,
                VecWidth::V128,
                true,
                false,
            ),
            (
                &[0xC4, 0xC2, 0x4D, 0x51, 0x7D, 0x20][..],
                VecElementType::I8,
                VecWidth::V256,
                true,
                true,
            ),
            (
                &[0xC4, 0xC2, 0x69, 0x52, 0x4B, 0x20][..],
                VecElementType::I16,
                VecWidth::V128,
                false,
                false,
            ),
            (
                &[0xC4, 0xC2, 0x4D, 0x53, 0x7D, 0x20][..],
                VecElementType::I16,
                VecWidth::V256,
                false,
                true,
            ),
        ];
        for (bytes, elem, width, unsigned, saturate) in cases {
            let fields = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_memory_integer_dot_fields()
                .unwrap();
            assert_eq!(fields.src_elem, elem, "{bytes:02X?}");
            assert_eq!(fields.width, width, "{bytes:02X?}");
            assert_eq!(fields.src1_unsigned, unsigned, "{bytes:02X?}");
            assert_eq!(fields.saturate, saturate, "{bytes:02X?}");
        }
    }
}
