//! AVX/AVX2 VEX variable-blend replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// One complete variable-blend memory encoding rewritten to consume the
/// helper-loaded second source from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexVariableBlendMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) mask: u8,
    pub(crate) scratch: u8,
    pub(crate) opcode: u8,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx2: bool,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX variable blend and report
    /// whether the selected form requires AVX2 rather than AVX.
    ///
    /// Intel SDM Volume 2 assigns `VBLENDVPS`, `VBLENDVPD`, and `VPBLENDVB`
    /// to map 0F3A with mandatory 66H, VEX.W=0, and opcodes 4AH/4BH/4CH.
    /// Both floating forms require AVX at either vector width. `VPBLENDVB`
    /// requires AVX for 128 bits and AVX2 for 256 bits. The explicit mask
    /// register occupies imm8[7:4], while imm8[3:0] is ignored. Memory forms
    /// remain excluded so replay cannot bypass guest translation or precise
    /// fault handling.
    pub fn vex_register_variable_blend_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, opcode, modrm, _is4] = bytes else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x83 != 1 || modrm >> 6 != 3 || !matches!(opcode, 0x4A..=0x4C) {
            return None;
        }

        Some(*opcode == 0x4C && p1 & 0x04 != 0)
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX variable blend. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_variable_blend_destination_index(&self) -> Option<u8> {
        self.vex_register_variable_blend_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }

    /// Validate one complete VEX variable blend whose second source is memory
    /// and rewrite only that source to a borrowed low vector register.
    ///
    /// Intel SDM Volume 2 specifies `VBLENDVPS`, `VBLENDVPD`, and `VPBLENDVB`
    /// with 16-/32-byte memory sources. The explicit mask remains encoded in
    /// imm8[7:4], and the ignored low nibble is retained exactly. VEX.W=1 is
    /// reserved and rejected. Segment and address-size prefixes are consumed
    /// by guest effective-address evaluation and removed from the register
    /// rewrite. The shared parser validates the complete bounded
    /// ModR/M/SIB/displacement plus imm8 shape.
    pub(crate) fn vex_variable_blend_memory_encoding(
        &self,
    ) -> Option<X86VexVariableBlendMemoryEncoding> {
        let (fields, is4_byte) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || fields.w {
            return None;
        }
        let elem = match fields.opcode {
            0x4A => VecElementType::I32,
            0x4B => VecElementType::I64,
            0x4C => VecElementType::I8,
            _ => return None,
        };
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let mask = is4_byte >> 4;
        let scratch = (0..16u8)
            .find(|candidate| {
                *candidate != fields.destination
                    && *candidate != fields.source1
                    && *candidate != mask
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
        if register_instruction.vex_register_variable_blend_needs_avx2()
            != Some(fields.opcode == 0x4C && fields.width_256)
            || register_instruction.vex_variable_blend_destination_index()
                != Some(fields.destination)
        {
            return None;
        }

        Some(X86VexVariableBlendMemoryEncoding {
            width,
            elem,
            destination: fields.destination,
            source1: fields.source1,
            mask,
            scratch,
            opcode: fields.opcode,
            memory_size: width.bytes(),
            needs_avx2: fields.opcode == 0x4C && fields.width_256,
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_encoding(
        opcode: u8,
        width_256: bool,
        destination: u8,
        source1: u8,
        base: u8,
        is4_byte: u8,
    ) -> [u8; 7] {
        assert!(destination < 16 && source1 < 16 && base < 16);
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (((!source1) & 0x0F) << 3) | (u8::from(width_256) << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            is4_byte,
        ]
    }

    #[test]
    fn memory_classifier_exhaustively_covers_393_216_operand_mask_and_ignored_nibble_cells() {
        let mut classified = 0usize;
        for opcode in 0x4A..=0x4C {
            for width_256 in [false, true] {
                for destination in 0..16 {
                    for source1 in 0..16 {
                        for mask in 0..16 {
                            for ignored_low in 0..16 {
                                let is4_byte = (mask << 4) | ignored_low;
                                let base = if is4_byte & 1 == 0 { 3 } else { 11 };
                                let bytes = memory_encoding(
                                    opcode,
                                    width_256,
                                    destination,
                                    source1,
                                    base,
                                    is4_byte,
                                );
                                let encoding = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_variable_blend_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                let width = if width_256 {
                                    VecWidth::V256
                                } else {
                                    VecWidth::V128
                                };
                                let elem = match opcode {
                                    0x4A => VecElementType::I32,
                                    0x4B => VecElementType::I64,
                                    0x4C => VecElementType::I8,
                                    _ => unreachable!(),
                                };
                                let scratch = (0..16u8)
                                    .find(|candidate| {
                                        *candidate != destination
                                            && *candidate != source1
                                            && *candidate != mask
                                    })
                                    .unwrap();
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.mask, mask, "{bytes:02X?}");
                                assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                                assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                                assert_eq!(encoding.memory_size, width.bytes(), "{bytes:02X?}");
                                assert_eq!(
                                    encoding.needs_avx2,
                                    opcode == 0x4C && width_256,
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
        assert_eq!(classified, 3 * 2 * 16 * 16 * 16 * 16);
    }

    #[test]
    fn memory_classifier_matches_llvm_23_and_complete_prefixed_address_shapes() {
        for (bytes, expected) in [
            (
                &[0xC4, 0xE3, 0x69, 0x4A, 0x4F, 0x20, 0x3F][..],
                (VecWidth::V128, VecElementType::I32, 1, 2, 3, 0, false),
            ),
            (
                &[0xC4, 0x43, 0x2D, 0x4B, 0x4B, 0x20, 0xC5][..],
                (VecWidth::V256, VecElementType::I64, 9, 10, 12, 0, false),
            ),
            (
                &[0xC4, 0x43, 0x01, 0x4C, 0x7E, 0x20, 0xD0][..],
                (VecWidth::V128, VecElementType::I8, 15, 15, 13, 0, false),
            ),
            (
                &[
                    0x64, 0x67, 0xC4, 0x03, 0x2D, 0x4C, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xC5,
                ][..],
                (VecWidth::V256, VecElementType::I8, 14, 10, 12, 0, true),
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_variable_blend_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(
                (
                    encoding.width,
                    encoding.elem,
                    encoding.destination,
                    encoding.source1,
                    encoding.mask,
                    encoding.scratch,
                    encoding.needs_avx2,
                ),
                expected,
                "{bytes:02X?}"
            );
            assert_eq!(encoding.register_instruction.as_slice().len(), 6);
        }
    }

    #[test]
    fn memory_classifier_fails_closed_at_every_structural_frontier() {
        let valid = memory_encoding(0x4C, true, 9, 10, 11, 0xC5).to_vec();
        let mut invalid = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        invalid.push(wrong_map);
        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        invalid.push(wrong_prefix);
        let mut w1 = valid.clone();
        w1[2] |= 0x80;
        invalid.push(w1);
        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x49;
        invalid.push(wrong_opcode);
        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.remove(5);
        invalid.push(register);
        invalid.push(valid[..valid.len() - 1].to_vec());
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);
        let mut forbidden_prefix = valid;
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_variable_blend_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
