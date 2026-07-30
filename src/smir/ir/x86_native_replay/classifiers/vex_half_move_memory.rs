//! VEX.128 high/low 64-bit lane move memory-source classification.

use super::X86InstructionBytes;

/// Exact fields for one deterministic VEX.128 `VMOVLPS`, `VMOVLPD`,
/// `VMOVHPS`, or `VMOVHPD` memory load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexHalfMoveMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    /// Destination qword lane populated by the 8-byte memory operand.
    pub(crate) memory_lane: u8,
    /// Exact WIG bit retained for source-provenance validation.
    pub(crate) w: bool,
    /// `0` selects packed-single naming; `1` selects packed-double naming.
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
}

impl X86InstructionBytes {
    /// Validate one complete VEX.128 high/low 64-bit lane memory load.
    ///
    /// Map 0F opcode 12H loads memory into destination bits 63:0 while
    /// preserving source1 bits 127:64. Opcode 16H loads memory into bits
    /// 127:64 while preserving source1 bits 63:0. Mandatory-prefix values
    /// `00b` and `01b` select the packed-single and packed-double names but
    /// have identical bit-transfer semantics. W is ignored.
    ///
    /// Intel specifies `VEX.L=1` as #UD for all four forms, so only L=0 is
    /// admitted. Register sources and store opcodes 13H/17H remain disjoint.
    /// The shared parser validates the complete ModR/M/SIB/displacement image
    /// and permits only segment and address-size legacy prefixes.
    pub(crate) fn vex_half_move_memory_encoding(&self) -> Option<X86VexHalfMoveMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1
            || fields.width_256
            || !matches!(fields.pp, 0 | 1)
            || !matches!(fields.opcode, 0x12 | 0x16)
        {
            return None;
        }

        Some(X86VexHalfMoveMemoryEncoding {
            destination: fields.destination,
            source1: fields.source1,
            memory_lane: u8::from(fields.opcode == 0x16),
            w: fields.w,
            pp: fields.pp,
            opcode: fields.opcode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Form {
        C5,
        C4 { w: bool },
    }

    fn instruction(
        destination: u8,
        source1: u8,
        base: u8,
        pp: u8,
        opcode: u8,
        form: Form,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        let p1 =
            (u8::from(matches!(form, Form::C4 { w: true })) << 7) | (((!source1) & 15) << 3) | pp;
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 }) | (p1 & 0x7F),
                    opcode,
                    modrm,
                    0x20,
                ]
            }
            Form::C4 { .. } => vec![
                0xC4,
                (if destination < 8 { 0x80 } else { 0 })
                    | 0x40
                    | (if base < 8 { 0x20 } else { 0 })
                    | 1,
                p1,
                opcode,
                modrm,
                0x20,
            ],
        }
    }

    #[test]
    fn classifies_all_3072_destination_source_format_opcode_and_form_cells() {
        let forms = [Form::C5, Form::C4 { w: false }, Form::C4 { w: true }];
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for pp in 0..=1 {
                    for opcode in [0x12, 0x16] {
                        for form in forms {
                            let base = if form == Form::C5 { 3 } else { 11 };
                            let bytes = instruction(destination, source1, base, pp, opcode, form);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_half_move_memory_encoding(),
                                Some(X86VexHalfMoveMemoryEncoding {
                                    destination,
                                    source1,
                                    memory_lane: u8::from(opcode == 0x16),
                                    w: matches!(form, Form::C4 { w: true }),
                                    pp,
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
        assert_eq!(classified, 16 * 16 * 2 * 2 * 3);
    }

    #[test]
    fn complete_prefixed_rip_sib_addr32_and_ignored_w_shapes_classify() {
        let cases: &[(&[u8], X86VexHalfMoveMemoryEncoding)] = &[
            (
                &[0x64, 0xC5, 0xE8, 0x12, 0x4D, 0x20],
                X86VexHalfMoveMemoryEncoding {
                    destination: 1,
                    source1: 2,
                    memory_lane: 0,
                    w: false,
                    pp: 0,
                    opcode: 0x12,
                },
            ),
            (
                &[0x65, 0xC4, 0x01, 0xA1, 0x16, 0x4C, 0xEC, 0x20],
                X86VexHalfMoveMemoryEncoding {
                    destination: 9,
                    source1: 11,
                    memory_lane: 1,
                    w: true,
                    pp: 1,
                    opcode: 0x16,
                },
            ),
            (
                &[
                    0x67, 0xC4, 0x61, 0x01, 0x12, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
                ],
                X86VexHalfMoveMemoryEncoding {
                    destination: 14,
                    source1: 15,
                    memory_lane: 0,
                    w: false,
                    pp: 1,
                    opcode: 0x12,
                },
            ),
            (
                &[0xC4, 0xC1, 0x80, 0x16, 0x05, 0x11, 0x22, 0x33, 0x44],
                X86VexHalfMoveMemoryEncoding {
                    destination: 0,
                    source1: 15,
                    memory_lane: 1,
                    w: true,
                    pp: 0,
                    opcode: 0x16,
                },
            ),
        ];
        for &(bytes, expected) in cases {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_half_move_memory_encoding(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn l1_stores_register_sources_reserved_prefixes_and_nonexact_images_fail_closed() {
        let valid = instruction(9, 10, 11, 1, 0x16, Form::C4 { w: true });
        let mut invalid = Vec::new();

        let mut l1 = valid.clone();
        l1[2] |= 0x04;
        invalid.push(l1);
        for opcode in [0x13, 0x17, 0x10, 0x14] {
            let mut bytes = valid.clone();
            bytes[3] = opcode;
            invalid.push(bytes);
        }
        for pp in [2, 3] {
            let mut bytes = valid.clone();
            bytes[2] = (bytes[2] & !3) | pp;
            invalid.push(bytes);
        }
        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        invalid.push(wrong_map);
        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.pop();
        invalid.push(register);
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);
        let mut forbidden_prefix = valid.clone();
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);
        for end in 0..valid.len() {
            invalid.push(valid[..end].to_vec());
        }

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_half_move_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
