//! VEX one-source lane-shuffle replay and memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one register-only VEX one-source lane-shuffle instruction and
    /// return whether its 256-bit form requires AVX2.
    ///
    /// This covers VMOVSLDUP, VMOVSHDUP, VMOVDDUP, VPSHUFD, VPSHUFHW, and
    /// VPSHUFLW. The duplicate moves require AVX at both vector lengths.
    /// VEX.128 packed immediate shuffles require AVX; their VEX.256 forms
    /// require AVX2. Every form uses map 0F, reserves VEX.vvvv as `1111b`, and
    /// is WIG. Memory operands and malformed byte shapes fail closed.
    pub fn vex_register_lane_shuffle_needs_avx2(&self) -> Option<bool> {
        let (p1, opcode, modrm, has_immediate) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm, false),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm, false),
            [0xC5, p1, opcode, modrm, _] => (*p1, *opcode, *modrm, true),
            [0xC4, p0, p1, opcode, modrm, _] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm, true),
            _ => return None,
        };
        if p1 & 0x78 != 0x78 || modrm >> 6 != 3 {
            return None;
        }

        match (has_immediate, opcode, p1 & 0x03) {
            (false, 0x12, 2 | 3) | (false, 0x16, 2) => Some(false),
            (true, 0x70, 1 | 2 | 3) => Some(p1 & 0x04 != 0),
            _ => None,
        }
    }

    /// Return the architectural destination after exact validation. Every
    /// covered instruction writes ModR/M.reg. The AVX-only state bridge uses
    /// the result to clear the destination's state-backed ZMM[511:256] after
    /// architectural VEX upper-zeroing.
    pub(crate) fn vex_lane_shuffle_destination_index(&self) -> Option<u8> {
        self.vex_register_lane_shuffle_needs_avx2()?;
        let (reg_extension, modrm) = match self.as_slice() {
            [0xC5, p1, _, modrm] | [0xC5, p1, _, modrm, _] => (p1 & 0x80 == 0, *modrm),
            [0xC4, p0, _, _, modrm] | [0xC4, p0, _, _, modrm, _] => (p0 & 0x80 == 0, *modrm),
            _ => unreachable!("VEX lane-shuffle shape was validated"),
        };
        Some(((modrm >> 3) & 7) + if reg_extension { 8 } else { 0 })
    }

    /// Validate one complete VEX `VMOVSLDUP`, `VMOVSHDUP`, or `VMOVDDUP`
    /// instruction whose source is memory and return
    /// `(destination, width, element, high, memory size, W)`.
    ///
    /// Every form uses map 0F, reserves VEX.vvvv as `1111b`, defines VEX.W
    /// as ignored, and requires only AVX at both vector lengths. `VMOVDDUP`
    /// with a 128-bit destination is the architectural exception that reads
    /// only 8 bytes from memory; the other forms read the full vector width.
    /// Runtime and auxiliary space are O(1).
    pub(crate) fn vex_memory_duplicate_move_fields(
        &self,
    ) -> Option<(u8, VecWidth, VecElementType, bool, u32, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.source1 != 0 {
            return None;
        }
        let (element, high) = match (fields.opcode, fields.pp) {
            (0x12, 2) => (VecElementType::F32, false),
            (0x16, 2) => (VecElementType::F32, true),
            (0x12, 3) => (VecElementType::F64, false),
            _ => return None,
        };
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let memory_size = if element == VecElementType::F64 && width == VecWidth::V128 {
            8
        } else {
            width.bytes()
        };
        Some((
            fields.destination,
            width,
            element,
            high,
            memory_size,
            fields.w,
        ))
    }

    /// Validate one complete VEX `VPSHUFD`, `VPSHUFHW`, or `VPSHUFLW`
    /// instruction whose source is memory and return
    /// `(destination, width, element, high-words selector, imm8, W)`.
    ///
    /// All three instructions use map 0F opcode 70H, reserve VEX.vvvv as
    /// `1111b`, and define VEX.W as ignored. `high_words` is `None` for
    /// `VPSHUFD`, `Some(true)` for `VPSHUFHW`, and `Some(false)` for
    /// `VPSHUFLW`. Both 128- and 256-bit encodings are admitted; runtime
    /// feature gating separately requires AVX2 for the 256-bit forms.
    /// Runtime and auxiliary space are O(1).
    pub(crate) fn vex_memory_lane_shuffle_fields(
        &self,
    ) -> Option<(u8, VecWidth, VecElementType, Option<bool>, u8, bool)> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 1
            || fields.opcode != 0x70
            || fields.source1 != 0
            || !matches!(fields.pp, 1..=3)
        {
            return None;
        }
        let (element, high_words) = match fields.pp {
            1 => (VecElementType::I32, None),
            2 => (VecElementType::I16, Some(true)),
            3 => (VecElementType::I16, Some(false)),
            _ => unreachable!("validated VEX lane-shuffle prefix"),
        };
        Some((
            fields.destination,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            element,
            high_words,
            immediate,
            fields.w,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    enum Form {
        C5,
        C4 { w: bool },
    }

    fn instruction(
        destination: u8,
        base: u8,
        width: VecWidth,
        pp: u8,
        immediate: u8,
        form: Form,
    ) -> Vec<u8> {
        let l = u8::from(width == VecWidth::V256);
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 }) | 0x78 | (l << 2) | pp,
                    0x70,
                    modrm,
                    0x20,
                    immediate,
                ]
            }
            Form::C4 { w } => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(w) << 7) | 0x78 | (l << 2) | pp,
                0x70,
                modrm,
                0x20,
                immediate,
            ],
        }
    }

    fn duplicate_instruction(
        destination: u8,
        base: u8,
        width: VecWidth,
        opcode: u8,
        pp: u8,
        form: Form,
    ) -> Vec<u8> {
        let l = u8::from(width == VecWidth::V256);
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 }) | 0x78 | (l << 2) | pp,
                    opcode,
                    modrm,
                    0x20,
                ]
            }
            Form::C4 { w } => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(w) << 7) | 0x78 | (l << 2) | pp,
                opcode,
                modrm,
                0x20,
            ],
        }
    }

    fn expected_kind(pp: u8) -> (VecElementType, Option<bool>) {
        match pp {
            1 => (VecElementType::I32, None),
            2 => (VecElementType::I16, Some(true)),
            3 => (VecElementType::I16, Some(false)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn classifies_all_480_duplicate_destination_width_kind_w_and_base_cells() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for width in [VecWidth::V128, VecWidth::V256] {
                for (opcode, pp, element, high) in [
                    (0x12, 2, VecElementType::F32, false),
                    (0x16, 2, VecElementType::F32, true),
                    (0x12, 3, VecElementType::F64, false),
                ] {
                    let memory_size = if element == VecElementType::F64 && width == VecWidth::V128 {
                        8
                    } else {
                        width.bytes()
                    };
                    let bytes = duplicate_instruction(destination, 3, width, opcode, pp, Form::C5);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_memory_duplicate_move_fields(),
                        Some((destination, width, element, high, memory_size, false)),
                        "{bytes:02X?}"
                    );
                    classified += 1;

                    for base in [3, 11] {
                        for w in [false, true] {
                            let bytes = duplicate_instruction(
                                destination,
                                base,
                                width,
                                opcode,
                                pp,
                                Form::C4 { w },
                            );
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_duplicate_move_fields(),
                                Some((destination, width, element, high, memory_size, w)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 2 * 3 * (1 + 2 * 2));
    }

    #[test]
    fn malformed_reserved_or_different_duplicate_encodings_fail_closed() {
        let valid = duplicate_instruction(9, 11, VecWidth::V256, 0x16, 2, Form::C4 { w: true });
        let mut cases = Vec::new();

        let mut nonreserved_vvvv = valid.clone();
        nonreserved_vvvv[2] &= !0x08;
        cases.push(nonreserved_vvvv);

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 1;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x17;
        cases.push(wrong_opcode);

        let mut invalid_opcode_prefix_pair = valid.clone();
        invalid_opcode_prefix_pair[2] = (invalid_opcode_prefix_pair[2] & !3) | 3;
        cases.push(invalid_opcode_prefix_pair);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.remove(5);
        cases.push(register_source);

        let mut truncated_displacement = valid.clone();
        truncated_displacement[4] = (truncated_displacement[4] & 0x3F) | 0x80;
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
                    .vex_memory_duplicate_move_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifies_all_122880_destination_width_kind_immediate_w_and_base_cells() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for width in [VecWidth::V128, VecWidth::V256] {
                for pp in 1..=3 {
                    let (element, high_words) = expected_kind(pp);
                    for immediate in u8::MIN..=u8::MAX {
                        let bytes = instruction(destination, 3, width, pp, immediate, Form::C5);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_memory_lane_shuffle_fields(),
                            Some((destination, width, element, high_words, immediate, false,)),
                            "{bytes:02X?}"
                        );
                        classified += 1;

                        for base in [3, 11] {
                            for w in [false, true] {
                                let bytes = instruction(
                                    destination,
                                    base,
                                    width,
                                    pp,
                                    immediate,
                                    Form::C4 { w },
                                );
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_memory_lane_shuffle_fields(),
                                    Some((destination, width, element, high_words, immediate, w,)),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 2 * 3 * 256 * (1 + 2 * 2));
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shape() {
        // addr32 FS: VPSHUFHW ymm14,[r14d+r15d*2+0x44332211],0xA5
        let bytes = [
            0x64, 0x67, 0xC4, 0x01, 0xFE, 0x70, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xA5,
        ];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_lane_shuffle_fields(),
            Some((
                14,
                VecWidth::V256,
                VecElementType::I16,
                Some(true),
                0xA5,
                true,
            ))
        );
    }

    #[test]
    fn malformed_reserved_or_semantically_different_encodings_fail_closed() {
        let valid = instruction(9, 11, VecWidth::V256, 3, 0xA5, Form::C4 { w: true });
        let mut cases = Vec::new();

        let mut nonreserved_vvvv = valid.clone();
        nonreserved_vvvv[2] &= !0x08;
        cases.push(nonreserved_vvvv);

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut missing_prefix = valid.clone();
        missing_prefix[2] &= !3;
        cases.push(missing_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x71;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.remove(5);
        cases.push(register_source);

        let mut missing_immediate = valid.clone();
        missing_immediate.pop();
        cases.push(missing_immediate);

        let mut truncated_displacement = valid.clone();
        truncated_displacement[4] = (truncated_displacement[4] & 0x3F) | 0x80;
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
                    .vex_memory_lane_shuffle_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
