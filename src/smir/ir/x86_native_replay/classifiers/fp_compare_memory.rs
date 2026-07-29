//! AVX VEX floating-point comparison memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete AVX VEX `VCMPPS` or `VCMPPD` instruction whose
    /// second source is memory and return
    /// `(destination, source1, element, width, predicate, W)`.
    ///
    /// Both packed comparison instructions use map 0F opcode C2H, admit
    /// 128- and 256-bit vector lengths, define VEX.W as ignored, and reserve
    /// immediate bits 7:5. Scalar `VCMPSS`/`VCMPSD` encodings are deliberately
    /// excluded. The shared parser accepts only segment/address-size legacy
    /// prefixes and validates the complete ModR/M/SIB/displacement plus imm8
    /// shape. Runtime and auxiliary space are O(1) because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn vex_memory_packed_fp_compare_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, VecWidth, u8, bool)> {
        let (fields, predicate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 1
            || fields.opcode != 0xC2
            || !matches!(fields.pp, 0 | 1)
            || predicate & !0x1F != 0
        {
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
            predicate,
            fields.w,
        ))
    }

    /// Validate one complete AVX VEX `VCMPSS` or `VCMPSD` instruction whose
    /// second source is memory and return
    /// `(destination, source1, element, predicate, W)`.
    ///
    /// Both scalar comparison instructions use map 0F opcode C2H, define
    /// VEX.W as ignored, and reserve immediate bits 7:5. Although the opcode
    /// table labels VEX.L as ignored, Intel documents VEX.L=1 behavior as
    /// generation-dependent unpredictable; native admission therefore accepts
    /// only VEX.L=0. Packed `VCMPPS`/`VCMPPD` encodings are deliberately
    /// excluded. Runtime and auxiliary space are O(1).
    pub(crate) fn vex_memory_scalar_fp_compare_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, u8, bool)> {
        let (fields, predicate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 1
            || fields.opcode != 0xC2
            || !matches!(fields.pp, 2 | 3)
            || fields.width_256
            || predicate & !0x1F != 0
        {
            return None;
        }
        Some((
            fields.destination,
            fields.source1,
            if fields.pp == 2 {
                VecElementType::F32
            } else {
                VecElementType::F64
            },
            predicate,
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
        source1: u8,
        base: u8,
        elem: VecElementType,
        width: VecWidth,
        predicate: u8,
        form: Form,
    ) -> Vec<u8> {
        let l = u8::from(width == VecWidth::V256);
        let pp = u8::from(elem == VecElementType::F64);
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 })
                        | (((!source1) & 0x0F) << 3)
                        | (l << 2)
                        | pp,
                    0xC2,
                    modrm,
                    0x20,
                    predicate,
                ]
            }
            Form::C4 { w } => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | pp,
                0xC2,
                modrm,
                0x20,
                predicate,
            ],
        }
    }

    #[test]
    fn classifies_every_c4_c5_register_format_width_predicate_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for elem in [VecElementType::F32, VecElementType::F64] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        for predicate in 0..32 {
                            let bytes = instruction(
                                destination,
                                source1,
                                3,
                                elem,
                                width,
                                predicate,
                                Form::C5,
                            );
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_packed_fp_compare_fields(),
                                Some((destination, source1, elem, width, predicate, false,)),
                                "{bytes:02X?}"
                            );
                            classified += 1;

                            for base in [3, 11] {
                                for w in [false, true] {
                                    let bytes = instruction(
                                        destination,
                                        source1,
                                        base,
                                        elem,
                                        width,
                                        predicate,
                                        Form::C4 { w },
                                    );
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .vex_memory_packed_fp_compare_fields(),
                                        Some((destination, source1, elem, width, predicate, w,)),
                                        "{bytes:02X?}"
                                    );
                                    classified += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 2 * 32 * (1 + 2 * 2));
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shape() {
        // addr32 FS: VCMPPD ymm14,ymm9,[r14d+r15d*2+0x44332211],31
        let bytes = [
            0x64, 0x67, 0xC4, 0x01, 0xB5, 0xC2, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0x1F,
        ];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_packed_fp_compare_fields(),
            Some((14, 9, VecElementType::F64, VecWidth::V256, 31, true))
        );
    }

    #[test]
    fn malformed_reserved_scalar_or_semantically_different_encodings_fail_closed() {
        let valid = instruction(
            9,
            10,
            11,
            VecElementType::F64,
            VecWidth::V256,
            31,
            Form::C4 { w: true },
        );
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        for scalar_prefix in [2, 3] {
            let mut scalar = valid.clone();
            scalar[2] = (scalar[2] & !3) | scalar_prefix;
            cases.push(scalar);
        }

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0xC3;
        cases.push(wrong_opcode);

        let mut reserved_predicate = valid.clone();
        *reserved_predicate.last_mut().unwrap() = 0x20;
        cases.push(reserved_predicate);

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
                    .vex_memory_packed_fp_compare_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifies_every_scalar_c4_c5_register_format_predicate_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for elem in [VecElementType::F32, VecElementType::F64] {
                    for predicate in 0..32 {
                        let bytes = instruction(
                            destination,
                            source1,
                            3,
                            elem,
                            VecWidth::V128,
                            predicate,
                            Form::C5,
                        );
                        let scalar_prefix = if elem == VecElementType::F32 { 2 } else { 3 };
                        let mut bytes = bytes;
                        bytes[1] = (bytes[1] & !3) | scalar_prefix;
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_memory_scalar_fp_compare_fields(),
                            Some((destination, source1, elem, predicate, false)),
                            "{bytes:02X?}"
                        );
                        classified += 1;

                        for base in [3, 11] {
                            for w in [false, true] {
                                let mut bytes = instruction(
                                    destination,
                                    source1,
                                    base,
                                    elem,
                                    VecWidth::V128,
                                    predicate,
                                    Form::C4 { w },
                                );
                                bytes[2] = (bytes[2] & !3) | scalar_prefix;
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_memory_scalar_fp_compare_fields(),
                                    Some((destination, source1, elem, predicate, w)),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 32 * (1 + 2 * 2));
    }

    #[test]
    fn scalar_classifier_accepts_complete_shape_and_rejects_unpredictable_l1() {
        // addr32 FS: VCMPSD xmm14,xmm9,[r14d+r15d*2+0x44332211],31
        let bytes = [
            0x64, 0x67, 0xC4, 0x01, 0xB3, 0xC2, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0x1F,
        ];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_scalar_fp_compare_fields(),
            Some((14, 9, VecElementType::F64, 31, true))
        );

        let mut l1 = bytes;
        l1[4] |= 0x04;
        assert_eq!(
            X86InstructionBytes::new(&l1)
                .unwrap()
                .vex_memory_scalar_fp_compare_fields(),
            None
        );
    }

    #[test]
    fn scalar_classifier_rejects_packed_reserved_and_malformed_encodings() {
        let mut valid = instruction(
            9,
            10,
            11,
            VecElementType::F64,
            VecWidth::V128,
            31,
            Form::C4 { w: true },
        );
        valid[2] = (valid[2] & !3) | 3;
        let mut cases = Vec::new();

        for packed_prefix in [0, 1] {
            let mut packed = valid.clone();
            packed[2] = (packed[2] & !3) | packed_prefix;
            cases.push(packed);
        }

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0xC3;
        cases.push(wrong_opcode);

        let mut reserved_predicate = valid.clone();
        *reserved_predicate.last_mut().unwrap() = 0x20;
        cases.push(reserved_predicate);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.remove(5);
        cases.push(register_source);

        let mut missing_immediate = valid.clone();
        missing_immediate.pop();
        cases.push(missing_immediate);

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
                    .vex_memory_scalar_fp_compare_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
