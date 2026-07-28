//! Complete VEX packed-integer rounded-average memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete VEX `VPAVGB` or `VPAVGW` instruction whose second
    /// source is memory and return
    /// `(destination, first source, element type, vector width, W)`.
    ///
    /// These forms use map 0F and mandatory prefix 66H. VEX.W is ignored by
    /// the architecture and is retained so the native rewrite can reproduce
    /// the guest selection. The shared parser validates the complete
    /// ModR/M/SIB/displacement byte shape and permits only
    /// segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_packed_average_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, VecWidth, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.pp != 1 {
            return None;
        }
        let elem = match fields.opcode {
            0xE0 => VecElementType::I8,
            0xE3 => VecElementType::I16,
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            elem,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
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
        form: Form,
    ) -> Vec<u8> {
        let opcode = match elem {
            VecElementType::I8 => 0xE0,
            VecElementType::I16 => 0xE3,
            _ => unreachable!(),
        };
        let l = u8::from(width == VecWidth::V256);
        let modrm = 0x40 | ((destination & 7) << 3) | (base & 7);
        match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if destination < 8 { 0x80 } else { 0 })
                        | (((!source1) & 0x0F) << 3)
                        | (l << 2)
                        | 1,
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
                (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
                opcode,
                modrm,
                0x20,
            ],
        }
    }

    #[test]
    fn classifies_every_c4_c5_destination_source_width_w_and_element_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for elem in [VecElementType::I8, VecElementType::I16] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        let bytes = instruction(destination, source1, 3, elem, width, Form::C5);
                        let metadata = X86InstructionBytes::new(&bytes).unwrap();
                        assert_eq!(
                            metadata.vex_memory_packed_average_fields(),
                            Some((destination, source1, elem, width, false)),
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
                                    Form::C4 { w },
                                );
                                let metadata = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    metadata.vex_memory_packed_average_fields(),
                                    Some((destination, source1, elem, width, w)),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 2 * (1 + 2 * 2));
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shape() {
        // addr32 FS: VPAVGW ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x01, 0xB5, 0xE3, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_packed_average_fields(),
            Some((14, 9, VecElementType::I16, VecWidth::V256, true))
        );
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = instruction(
            3,
            9,
            11,
            VecElementType::I16,
            VecWidth::V128,
            Form::C4 { w: false },
        );
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0xE1;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.truncate(5);
        cases.push(register_source);

        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut truncated = valid.clone();
        truncated.pop();
        cases.push(truncated);

        let mut forbidden_legacy_prefix = valid;
        forbidden_legacy_prefix.insert(0, 0x66);
        cases.push(forbidden_legacy_prefix);

        for bytes in cases {
            let metadata = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                metadata.vex_memory_packed_average_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
