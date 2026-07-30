//! Complete original-VEX `CMPccXADD` memory classification.

use super::X86InstructionBytes;
use crate::smir::ir::X86VexCmpccxaddMemoryEncoding;
use crate::smir::ir::types::MemWidth;

impl X86InstructionBytes {
    /// Validate one complete original
    /// `VEX.128.66.0F38.W{0,1} E0+cc /r` `CMPccXADD` instruction.
    ///
    /// APX-promoted EVEX forms intentionally fail this classifier. The shared
    /// parser additionally enforces a memory ModR/M operand, complete
    /// SIB/displacement consumption, and the absence of forbidden legacy
    /// prefixes or trailing bytes. Runtime and auxiliary space are O(1)
    /// because architectural x86 instructions are at most 15 bytes.
    pub(crate) fn vex_cmpccxadd_memory_encoding(&self) -> Option<X86VexCmpccxaddMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2
            || fields.pp != 1
            || fields.width_256
            || !matches!(fields.opcode, 0xE0..=0xEF)
        {
            return None;
        }
        Some(X86VexCmpccxaddMemoryEncoding {
            cmp: fields.destination,
            add: fields.source1,
            condition_code: fields.opcode & 0x0F,
            width: if fields.w { MemWidth::B8 } else { MemWidth::B4 },
            stack_segment: fields.stack_segment,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(cmp: u8, add: u8, base: u8, width: MemWidth, cc: u8) -> Vec<u8> {
        assert!(cmp < 16 && add < 16 && base < 16 && cc < 16);
        assert!(matches!(width, MemWidth::B4 | MemWidth::B8));
        let mut bytes = vec![
            0xC4,
            (if cmp < 8 { 0x80 } else { 0 })
                | (if (cmp ^ add ^ base ^ cc) & 1 == 0 {
                    0x40
                } else {
                    0
                })
                | (if base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(width == MemWidth::B8) << 7) | ((!add & 0x0F) << 3) | 1,
            0xE0 | cc,
            0x40 | ((cmp & 7) << 3) | (base & 7),
        ];
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(0x20);
        bytes
    }

    #[test]
    fn classifies_every_condition_width_comparison_and_addend_register_cell() {
        let mut classified = 0usize;
        for cc in 0..16 {
            for width in [MemWidth::B4, MemWidth::B8] {
                for cmp in 0..16 {
                    for add in 0..16 {
                        for base in [3, 12] {
                            let bytes = instruction(cmp, add, base, width, cc);
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_cmpccxadd_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(
                                encoding,
                                X86VexCmpccxaddMemoryEncoding {
                                    cmp,
                                    add,
                                    condition_code: cc,
                                    width,
                                    stack_segment: matches!(base & 7, 4 | 5),
                                }
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 2 * 16 * 16 * 2);
    }

    #[test]
    fn complete_prefixed_address_shapes_classify_exactly() {
        for (bytes, expected) in [
            (
                &[0x64, 0x67, 0xC4, 0x42, 0x29, 0xE2, 0x4C, 0x24, 0x20][..],
                X86VexCmpccxaddMemoryEncoding {
                    cmp: 9,
                    add: 10,
                    condition_code: 2,
                    width: MemWidth::B4,
                    stack_segment: false,
                },
            ),
            (
                &[
                    0x65, 0xC4, 0x02, 0xF1, 0xEF, 0xB4, 0x7E, 0x44, 0x33, 0x22, 0x11,
                ][..],
                X86VexCmpccxaddMemoryEncoding {
                    cmp: 14,
                    add: 1,
                    condition_code: 15,
                    width: MemWidth::B8,
                    stack_segment: false,
                },
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_cmpccxadd_memory_encoding(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn segment_override_and_default_base_select_the_exact_fault_class() {
        for (name, base, prefixes, expected_stack_segment) in [
            ("default RBX uses DS", 3, &[][..], false),
            ("default RBP uses SS", 5, &[][..], true),
            ("explicit SS overrides RBX", 3, &[0x36][..], true),
            ("explicit DS overrides RBP", 5, &[0x3E][..], false),
            ("explicit FS overrides RBP", 5, &[0x64][..], false),
            ("last segment override wins", 5, &[0x36, 0x65][..], false),
        ] {
            let mut bytes = instruction(9, 10, base, MemWidth::B8, 7);
            for prefix in prefixes.iter().rev() {
                bytes.insert(0, *prefix);
            }
            let encoding = X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_cmpccxadd_memory_encoding()
                .unwrap_or_else(|| panic!("{name}: {bytes:02X?}"));
            assert_eq!(
                encoding.stack_segment, expected_stack_segment,
                "{name}: {bytes:02X?}"
            );
        }
    }

    #[test]
    fn nonexact_vex_and_every_evex_frontier_fail_closed() {
        let valid = instruction(9, 10, 12, MemWidth::B8, 7);
        let mut invalid = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 3;
        invalid.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        invalid.push(wrong_prefix);

        let mut wrong_length = valid.clone();
        wrong_length[2] |= 4;
        invalid.push(wrong_length);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0xD2;
        invalid.push(wrong_opcode);

        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.truncate(5);
        invalid.push(register);

        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);

        let mut truncated = valid.clone();
        truncated.pop();
        invalid.push(truncated);

        for prefix in [0xF0, 0xF2, 0xF3, 0x66, 0x40, 0x48] {
            let mut forbidden = valid.clone();
            forbidden.insert(0, prefix);
            invalid.push(forbidden);
        }

        invalid.push(vec![0x62, 0xEA, 0x61, 0x00, 0xE2, 0x08]);
        invalid.push(vec![0x62, 0xEA, 0x65, 0x08, 0xE2, 0x08]);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_cmpccxadd_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
