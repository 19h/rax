//! AVX VEX floating-point shuffle memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete AVX VEX `VSHUFPS` or `VSHUFPD` instruction whose
    /// second source is memory and return
    /// `(destination, source1, element, width, immediate, W)`.
    ///
    /// Both instructions use map 0F opcode C6, admit 128- and 256-bit vector
    /// lengths, and define VEX.W as ignored. The shared parser accepts only
    /// segment/address-size legacy prefixes and validates the complete
    /// ModR/M/SIB/displacement plus imm8 byte shape.
    pub(crate) fn vex_memory_fp_shuffle_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, VecWidth, u8, bool)> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 1 || fields.opcode != 0xC6 || !matches!(fields.pp, 0 | 1) {
            return None;
        }
        Some((
            fields.destination,
            fields.source1,
            if fields.pp == 0 {
                VecElementType::F32
            } else {
                VecElementType::F64
            },
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

    fn c4_instruction(
        destination: u8,
        source1: u8,
        base: u8,
        elem: VecElementType,
        width: VecWidth,
        immediate: u8,
        w: bool,
    ) -> Vec<u8> {
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 1,
            (u8::from(w) << 7)
                | (((!source1) & 0x0F) << 3)
                | (u8::from(width == VecWidth::V256) << 2)
                | u8::from(elem == VecElementType::F64),
            0xC6,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    #[test]
    fn classifies_every_register_format_width_w_and_immediate_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for elem in [VecElementType::F32, VecElementType::F64] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        for w in [false, true] {
                            for immediate in u8::MIN..=u8::MAX {
                                let bytes = c4_instruction(
                                    destination,
                                    source1,
                                    3,
                                    elem,
                                    width,
                                    immediate,
                                    w,
                                );
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_memory_fp_shuffle_fields(),
                                    Some((destination, source1, elem, width, immediate, w,)),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 2 * 2 * 256);
    }

    #[test]
    fn accepts_c5_and_complete_prefixed_sib_displacement_shapes() {
        for (bytes, expected) in [
            (
                vec![0xC5, 0xE8, 0xC6, 0x48, 0x20, 0x1B],
                (1, 2, VecElementType::F32, VecWidth::V128, 0x1B, false),
            ),
            (
                vec![
                    0x64, 0x67, 0xC4, 0x01, 0xAD, 0xC6, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xA5,
                ],
                (14, 10, VecElementType::F64, VecWidth::V256, 0xA5, true),
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_fp_shuffle_fields(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = c4_instruction(9, 10, 11, VecElementType::F64, VecWidth::V256, 0xA5, true);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0xC5;
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

        let mut forbidden_prefix = valid;
        forbidden_prefix.insert(0, 0x66);
        cases.push(forbidden_prefix);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_fp_shuffle_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
