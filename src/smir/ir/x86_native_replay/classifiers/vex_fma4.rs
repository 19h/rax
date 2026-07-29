//! AMD AVX VEX FMA4 replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth, X86FmaKind};

/// One complete FMA4 memory encoding rewritten to consume the helper-loaded
/// value from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexFma4MemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) encoded_256: bool,
    pub(crate) elem: VecElementType,
    pub(crate) kind: X86FmaKind,
    pub(crate) scalar: bool,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) is4: u8,
    pub(crate) scratch: u8,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) memory_size: u32,
    pub(crate) register_instruction: X86InstructionBytes,
}

fn fma4_spec(opcode: u8) -> Option<(VecElementType, X86FmaKind, bool)> {
    let elem = if opcode & 1 == 0 {
        VecElementType::F32
    } else {
        VecElementType::F64
    };
    let (kind, scalar) = match opcode {
        0x5C | 0x5D => (X86FmaKind::AddSub, false),
        0x5E | 0x5F => (X86FmaKind::SubAdd, false),
        0x68 | 0x69 => (X86FmaKind::Add, false),
        0x6A | 0x6B => (X86FmaKind::Add, true),
        0x6C | 0x6D => (X86FmaKind::Sub, false),
        0x6E | 0x6F => (X86FmaKind::Sub, true),
        0x78 | 0x79 => (X86FmaKind::NegativeMultiplyAdd, false),
        0x7A | 0x7B => (X86FmaKind::NegativeMultiplyAdd, true),
        0x7C | 0x7D => (X86FmaKind::NegativeMultiplySub, false),
        0x7E | 0x7F => (X86FmaKind::NegativeMultiplySub, true),
        _ => return None,
    };
    Some((elem, kind, scalar))
}

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX FMA4 instruction.
    ///
    /// AMD APM Volume 4, revision 3.26 assigns opcodes 5CH through 5FH,
    /// 68H through 6FH, and 78H through 7FH in map 0F3A with mandatory 66H.
    /// VEX.W swaps the ModR/M and `/is4` source roles, VEX.L selects 128/256
    /// bits for packed forms and is ignored for scalar forms, and VEX.vvvv is
    /// an unrestricted first source. Bits 7:4 of the final byte select the
    /// `/is4` register; bits 3:0 do not select an operand. Memory forms remain
    /// excluded so native replay cannot bypass guest-memory translation or
    /// precise fault handling.
    pub fn is_vex_register_fma4(&self) -> bool {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0xC4 {
            return false;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];

        p0 & 0x1F == 3
            && p1 & 0x03 == 1
            && matches!(opcode, 0x5C..=0x5F | 0x68..=0x6F | 0x78..=0x7F)
            && modrm >> 6 == 3
    }

    /// Architectural destination register selected by an exact register-only
    /// FMA4 encoding. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_fma4_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_fma4() {
            return None;
        }
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }

    /// Validate one complete FMA4 memory source and rewrite only that source
    /// to a borrowed low vector register.
    ///
    /// AMD APM Volume 4, revision 3.26 specifies 4-/8-byte scalar and
    /// 16-/32-byte packed memory operands. VEX.W selects whether memory is
    /// source 2 or source 3 without changing its encoded ModR/M location.
    /// Segment and address-size prefixes are consumed by guest effective
    /// address evaluation and removed from the register rewrite. The `/is4`
    /// byte, including its architecturally ignored low nibble, is retained
    /// exactly. The shared VEX parser validates the entire bounded
    /// ModR/M/SIB/displacement plus `/is4` shape.
    pub(crate) fn vex_fma4_memory_encoding(&self) -> Option<X86VexFma4MemoryEncoding> {
        let (fields, is4_byte) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 {
            return None;
        }
        let (elem, kind, scalar) = fma4_spec(fields.opcode)?;
        let width = if scalar {
            VecWidth::V128
        } else if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let memory_size = if scalar {
            elem.bytes() as u32
        } else {
            width.bytes()
        };
        let is4 = is4_byte >> 4;
        let scratch = (0..16u8)
            .find(|candidate| {
                *candidate != fields.destination
                    && *candidate != fields.source1
                    && *candidate != is4
            })
            .expect("three operands cannot consume every low vector register");

        let bytes = self.as_slice();
        let start = bytes
            .iter()
            .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
            .count();
        if bytes.get(start) != Some(&0xC4) {
            return None;
        }
        let p0 = *bytes.get(start + 1)?;
        let p1 = *bytes.get(start + 2)?;
        let modrm = *bytes.get(start + 4)?;
        let register_bytes = [
            0xC4,
            // Preserve VEX.R and the map, canonicalize the ignored X bit, and
            // encode the borrowed scratch through inverted VEX.B.
            (p0 & 0x9F) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            p1,
            fields.opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
            is4_byte,
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes).unwrap();
        if !register_instruction.is_vex_register_fma4()
            || register_instruction.vex_fma4_destination_index() != Some(fields.destination)
        {
            return None;
        }

        Some(X86VexFma4MemoryEncoding {
            width,
            encoded_256: fields.width_256,
            elem,
            kind,
            scalar,
            destination: fields.destination,
            source1: fields.source1,
            is4,
            scratch,
            opcode: fields.opcode,
            w: fields.w,
            memory_size,
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPCODES: [u8; 20] = [
        0x5C, 0x5D, 0x5E, 0x5F, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x78, 0x79, 0x7A,
        0x7B, 0x7C, 0x7D, 0x7E, 0x7F,
    ];

    fn memory_encoding(
        opcode: u8,
        w: bool,
        encoded_256: bool,
        destination: u8,
        source1: u8,
        base: u8,
        is4_byte: u8,
    ) -> [u8; 6] {
        assert!(destination < 16 && source1 < 16 && base < 16);
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (u8::from(encoded_256) << 2) | 1,
            opcode,
            ((destination & 7) << 3) | (base & 7),
            is4_byte,
        ]
    }

    #[test]
    fn memory_classifier_exhaustively_covers_655_360_operand_and_is4_register_cells() {
        let mut classified = 0usize;
        for opcode in OPCODES {
            let (elem, kind, scalar) = fma4_spec(opcode).unwrap();
            for w in [false, true] {
                for encoded_256 in [false, true] {
                    for destination in 0..16 {
                        for source1 in 0..16 {
                            for is4 in 0..16 {
                                let ignored_low = (opcode ^ destination ^ source1 ^ is4) & 0x0F;
                                let is4_byte = (is4 << 4) | ignored_low;
                                let base = if is4_byte & 1 == 0 { 3 } else { 11 };
                                let bytes = memory_encoding(
                                    opcode,
                                    w,
                                    encoded_256,
                                    destination,
                                    source1,
                                    base,
                                    is4_byte,
                                );
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_fma4_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                let width = if scalar || !encoded_256 {
                                    VecWidth::V128
                                } else {
                                    VecWidth::V256
                                };
                                let scratch = (0..16u8)
                                    .find(|candidate| {
                                        *candidate != destination
                                            && *candidate != source1
                                            && *candidate != is4_byte >> 4
                                    })
                                    .unwrap();
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.encoded_256, encoded_256, "{bytes:02X?}");
                                assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                assert_eq!(encoding.kind, kind, "{bytes:02X?}");
                                assert_eq!(encoding.scalar, scalar, "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.is4, is4_byte >> 4, "{bytes:02X?}");
                                assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                                assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                                assert_eq!(encoding.w, w, "{bytes:02X?}");
                                assert_eq!(
                                    encoding.memory_size,
                                    if scalar {
                                        elem.bytes() as u32
                                    } else {
                                        width.bytes()
                                    },
                                    "{bytes:02X?}"
                                );
                                let rewritten = encoding.register_instruction.as_slice();
                                assert_eq!(rewritten[5], is4_byte, "{bytes:02X?}");
                                assert_eq!(rewritten[4] >> 6, 3, "{bytes:02X?}");
                                assert_eq!(rewritten[4] & 7, scratch & 7, "{bytes:02X?}");
                                assert_eq!(rewritten[1] & 0x20 == 0, scratch >= 8, "{bytes:02X?}");
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 20 * 2 * 2 * 16 * 16 * 16);
    }

    #[test]
    fn memory_classifier_preserves_every_ignored_is4_low_nibble() {
        for opcode in OPCODES {
            for w in [false, true] {
                for encoded_256 in [false, true] {
                    for ignored_low in 0..16 {
                        let bytes =
                            memory_encoding(opcode, w, encoded_256, 9, 10, 11, 0xC0 | ignored_low);
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_fma4_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(
                            encoding.register_instruction.as_slice()[5],
                            0xC0 | ignored_low,
                            "{bytes:02X?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn memory_classifier_matches_llvm_23_and_complete_prefixed_address_shapes() {
        for (bytes, expected) in [
            (
                &[0xC4, 0xE3, 0x69, 0x68, 0x4F, 0x20, 0x40][..],
                (VecWidth::V128, false, 1, 2, 4, 0, 16),
            ),
            (
                &[0xC4, 0xE3, 0xE9, 0x68, 0x4F, 0x20, 0x30][..],
                (VecWidth::V128, true, 1, 2, 3, 0, 16),
            ),
            (
                &[0xC4, 0x43, 0x29, 0x6B, 0x4B, 0x20, 0xC0][..],
                (VecWidth::V128, false, 9, 10, 12, 0, 8),
            ),
            (
                &[0xC4, 0x43, 0x89, 0x7E, 0x7C, 0x24, 0x20, 0xD0][..],
                (VecWidth::V128, true, 15, 14, 13, 0, 4),
            ),
            (
                &[
                    0x64, 0x67, 0xC4, 0x03, 0xAD, 0x5D, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xC5,
                ][..],
                (VecWidth::V256, true, 14, 10, 12, 0, 32),
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_fma4_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(
                (
                    encoding.width,
                    encoding.w,
                    encoding.destination,
                    encoding.source1,
                    encoding.is4,
                    encoding.scratch,
                    encoding.memory_size,
                ),
                expected,
                "{bytes:02X?}"
            );
            assert_eq!(encoding.register_instruction.as_slice().len(), 6);
        }
    }

    #[test]
    fn memory_classifier_fails_closed_at_every_structural_frontier() {
        let valid = memory_encoding(0x68, false, true, 9, 10, 11, 0xC5).to_vec();
        let mut invalid = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        invalid.push(wrong_map);
        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        invalid.push(wrong_prefix);
        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x67;
        invalid.push(wrong_opcode);
        let mut register = valid.clone();
        register[4] |= 0xC0;
        invalid.push(register);
        invalid.push(valid[..valid.len() - 1].to_vec());
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);
        for prefix in [0x40, 0x66, 0xF0, 0xF2, 0xF3] {
            let mut bytes = valid.clone();
            bytes.insert(0, prefix);
            invalid.push(bytes);
        }

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_fma4_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
