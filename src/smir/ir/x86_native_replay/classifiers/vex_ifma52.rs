//! AVX-IFMA VEX `VPMADD52LUQ`/`VPMADD52HUQ` replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

impl X86InstructionBytes {
    /// Validate one exact register-only AVX-IFMA multiply-add and return
    /// `(destination, source1, source2, width, high)`.
    ///
    /// Intel SDM Volume 2 assigns VPMADD52LUQ/HUQ to VEX map 0F38 opcodes
    /// B4H/B5H. Both forms require mandatory prefix 66H, W=1, and either a
    /// 128-bit or 256-bit vector length. VEX.X is ignored for a register
    /// ModR/M operand. Memory forms remain excluded so source replay cannot
    /// bypass guest translation or precise faults.
    pub(crate) fn vex_register_ifma52_fields(&self) -> Option<(u8, u8, u8, VecWidth, bool)> {
        let [0xC4, p0, p1, opcode @ (0xB4 | 0xB5), modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 2 || p1 & 0x83 != 0x81 || modrm >> 6 != 3 {
            return None;
        }

        Some((
            (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            (!p1 >> 3) & 0x0F,
            (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
            if p1 & 0x04 != 0 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            *opcode == 0xB5,
        ))
    }

    /// Architectural XMM/YMM destination selected by an exact register-only
    /// AVX-IFMA multiply-add.
    pub(crate) fn vex_ifma52_destination_index(&self) -> Option<u8> {
        self.vex_register_ifma52_fields().map(|fields| fields.0)
    }

    /// Validate one complete AVX-IFMA multiply-add whose third operand is
    /// memory and return `(destination, source1, width, high, opcode)`.
    ///
    /// The shared parser validates optional segment/address-size prefixes and
    /// the complete ModR/M, SIB, displacement, and instruction boundary. This
    /// semantic layer then admits only the two Intel-defined AVX-IFMA shapes.
    pub(crate) fn vex_memory_ifma52_fields(&self) -> Option<(u8, u8, VecWidth, bool, u8)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.pp != 1 || !fields.w || !matches!(fields.opcode, 0xB4 | 0xB5) {
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
            fields.opcode == 0xB5,
            fields.opcode,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding(extension_bits: u8, encoded_vvvv: u8, ymm: bool, opcode: u8, modrm: u8) -> [u8; 5] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        assert!(matches!(opcode, 0xB4 | 0xB5));
        [
            0xC4,
            extension_bits | 2,
            0x80 | (encoded_vvvv << 3) | (u8::from(ymm) << 2) | 1,
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
    fn register_classifier_exhaustively_covers_all_131_072_prefix_and_modrm_cells() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for opcode in [0xB4, 0xB5] {
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
                                    if ymm { VecWidth::V256 } else { VecWidth::V128 },
                                    opcode == 0xB5,
                                )
                            });
                            assert_eq!(
                                instruction.vex_register_ifma52_fields(),
                                expected,
                                "{bytes:02X?}"
                            );
                            assert_eq!(
                                instruction.vex_ifma52_destination_index(),
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
        assert_eq!(accepted, 32_768);
        assert_eq!(tested, 131_072);
    }

    #[test]
    fn memory_classifier_exhaustively_covers_all_98_304_defined_address_cells() {
        let mut accepted = 0usize;
        for opcode in [0xB4, 0xB5] {
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
                                instruction.vex_memory_ifma52_fields(),
                                Some((
                                    (u8::from(extension_bits & 0x80 == 0) << 3)
                                        | ((modrm >> 3) & 7),
                                    (!encoded_vvvv) & 0x0F,
                                    if ymm { VecWidth::V256 } else { VecWidth::V128 },
                                    opcode == 0xB5,
                                    opcode,
                                )),
                                "{bytes:02X?}"
                            );
                            accepted += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 98_304);
    }

    #[test]
    fn classifiers_reject_wrong_map_prefix_w_opcode_operand_and_boundaries() {
        let valid_register = encoding(0xE0, 13, true, 0xB5, 0xFD);
        let valid_memory = complete_memory_encoding(0xE0, 13, true, 0xB5, 0x45);

        let mut register_cases = Vec::new();
        let mut wrong_map = valid_register;
        wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
        register_cases.push(wrong_map.to_vec());
        let mut wrong_prefix = valid_register;
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        register_cases.push(wrong_prefix.to_vec());
        let mut wrong_w = valid_register;
        wrong_w[2] &= !0x80;
        register_cases.push(wrong_w.to_vec());
        let mut wrong_opcode = valid_register;
        wrong_opcode[3] = 0xB3;
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
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_ifma52_fields(),
                None,
                "{bytes:02X?}"
            );
        }

        let mut memory_cases = Vec::new();
        let mut wrong_map = valid_memory.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 3;
        memory_cases.push(wrong_map);
        let mut wrong_prefix = valid_memory.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 0;
        memory_cases.push(wrong_prefix);
        let mut wrong_w = valid_memory.clone();
        wrong_w[2] &= !0x80;
        memory_cases.push(wrong_w);
        let mut wrong_opcode = valid_memory.clone();
        wrong_opcode[3] = 0xB6;
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

        for bytes in memory_cases {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(instruction.vex_memory_ifma52_fields(), None, "{bytes:02X?}");
        }
    }

    #[test]
    fn memory_classifier_accepts_segment_address_size_sib_and_displacements() {
        let cases = [
            vec![0x64, 0xC4, 0xE2, 0xE9, 0xB4, 0x00],
            vec![0x67, 0xC4, 0x62, 0x85, 0xB5, 0x44, 0x8B, 0x20],
            vec![
                0x65, 0x67, 0xC4, 0x22, 0x81, 0xB4, 0x84, 0x8D, 0x44, 0x33, 0x22, 0x11,
            ],
        ];
        for bytes in cases {
            assert!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_ifma52_fields()
                    .is_some(),
                "{bytes:02X?}"
            );
        }
    }
}
