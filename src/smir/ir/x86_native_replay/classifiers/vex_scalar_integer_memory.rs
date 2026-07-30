//! Complete VEX `VMOVD`/`VMOVQ` scalar-integer memory classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::MemWidth;

/// Direction of the scalar guest-memory transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86VexScalarIntegerMemoryKind {
    Load,
    Store,
}

/// Exact fields for one VEX.128 `VMOVD` or `VMOVQ` memory form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexScalarIntegerMemoryEncoding {
    pub(crate) kind: X86VexScalarIntegerMemoryKind,
    pub(crate) vector: u8,
    pub(crate) memory_width: MemWidth,
    /// Exact W bit. It selects width for opcodes 6EH/7EH and is ignored by
    /// the F3.0F.7E and 66.0F.D6 `VMOVQ` aliases.
    pub(crate) w: bool,
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
}

impl X86InstructionBytes {
    /// Validate one complete VEX.128 `VMOVD` or `VMOVQ` memory form.
    ///
    /// The admitted encodings are 66.0F.6E/7E, where W selects a 4- or 8-byte
    /// transfer, plus the WIG F3.0F.7E load and 66.0F.D6 store aliases. Every
    /// form reserves VEX.vvvv as encoded `1111b`, requires L=0, and has no
    /// flag or SIMD floating-point exception behavior. The shared parser
    /// validates the complete ModR/M/SIB/displacement image and accepts only
    /// segment/address-size legacy prefixes.
    ///
    /// Classification is O(1) time and O(1) space because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn vex_scalar_integer_memory_encoding(
        &self,
    ) -> Option<X86VexScalarIntegerMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.width_256 || fields.source1 != 0 {
            return None;
        }

        let (kind, memory_width) = match (fields.pp, fields.opcode, fields.w) {
            (1, 0x6E, false) => (X86VexScalarIntegerMemoryKind::Load, MemWidth::B4),
            (1, 0x6E, true) => (X86VexScalarIntegerMemoryKind::Load, MemWidth::B8),
            (1, 0x7E, false) => (X86VexScalarIntegerMemoryKind::Store, MemWidth::B4),
            (1, 0x7E, true) => (X86VexScalarIntegerMemoryKind::Store, MemWidth::B8),
            (2, 0x7E, _) => (X86VexScalarIntegerMemoryKind::Load, MemWidth::B8),
            (1, 0xD6, _) => (X86VexScalarIntegerMemoryKind::Store, MemWidth::B8),
            _ => return None,
        };

        Some(X86VexScalarIntegerMemoryEncoding {
            kind,
            vector: fields.destination,
            memory_width,
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
        C4W0,
        C4W1,
    }

    impl Form {
        const ALL: [Self; 3] = [Self::C5, Self::C4W0, Self::C4W1];

        const fn w(self) -> bool {
            matches!(self, Self::C4W1)
        }
    }

    fn instruction(vector: u8, base: u8, pp: u8, opcode: u8, form: Form) -> Vec<u8> {
        assert!(vector < 16 && base < 16);
        let modrm = 0x40 | ((vector & 7) << 3) | (base & 7);
        let mut bytes = match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if vector < 8 { 0x80 } else { 0 }) | 0x78 | pp,
                    opcode,
                    modrm,
                ]
            }
            Form::C4W0 | Form::C4W1 => vec![
                0xC4,
                (if vector < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 1,
                (u8::from(form.w()) << 7) | 0x78 | pp,
                opcode,
                modrm,
            ],
        };
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(0x20);
        bytes
    }

    fn expected(form: Form, pp: u8, opcode: u8) -> (X86VexScalarIntegerMemoryKind, MemWidth) {
        match (pp, opcode, form.w()) {
            (1, 0x6E, false) => (X86VexScalarIntegerMemoryKind::Load, MemWidth::B4),
            (1, 0x6E, true) => (X86VexScalarIntegerMemoryKind::Load, MemWidth::B8),
            (1, 0x7E, false) => (X86VexScalarIntegerMemoryKind::Store, MemWidth::B4),
            (1, 0x7E, true) => (X86VexScalarIntegerMemoryKind::Store, MemWidth::B8),
            (2, 0x7E, _) => (X86VexScalarIntegerMemoryKind::Load, MemWidth::B8),
            (1, 0xD6, _) => (X86VexScalarIntegerMemoryKind::Store, MemWidth::B8),
            _ => unreachable!(),
        }
    }

    #[test]
    fn classifies_all_192_form_vector_alias_and_width_cells() {
        let aliases = [(1, 0x6E), (1, 0x7E), (2, 0x7E), (1, 0xD6)];
        let mut classified = 0usize;
        for form in Form::ALL {
            for vector in 0..16 {
                for (pp, opcode) in aliases {
                    let bytes = instruction(
                        vector,
                        if form == Form::C5 { 3 } else { 11 },
                        pp,
                        opcode,
                        form,
                    );
                    let (kind, memory_width) = expected(form, pp, opcode);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_scalar_integer_memory_encoding(),
                        Some(X86VexScalarIntegerMemoryEncoding {
                            kind,
                            vector,
                            memory_width,
                            w: form.w(),
                            pp,
                            opcode,
                        }),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
        assert_eq!(classified, 3 * 16 * 4);
    }

    #[test]
    fn complete_addresses_and_reserved_fields_classify_fail_closed() {
        let valid_cases: &[(&[u8], X86VexScalarIntegerMemoryEncoding)] = &[
            (
                &[0x64, 0xC5, 0x79, 0x7E, 0x4D, 0x20],
                X86VexScalarIntegerMemoryEncoding {
                    kind: X86VexScalarIntegerMemoryKind::Store,
                    vector: 9,
                    memory_width: MemWidth::B4,
                    w: false,
                    pp: 1,
                    opcode: 0x7E,
                },
            ),
            (
                &[0x65, 0xC4, 0x41, 0xF9, 0x6E, 0x74, 0x24, 0x20],
                X86VexScalarIntegerMemoryEncoding {
                    kind: X86VexScalarIntegerMemoryKind::Load,
                    vector: 14,
                    memory_width: MemWidth::B8,
                    w: true,
                    pp: 1,
                    opcode: 0x6E,
                },
            ),
            (
                &[
                    0x67, 0xC4, 0x61, 0x7A, 0x7E, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
                ],
                X86VexScalarIntegerMemoryEncoding {
                    kind: X86VexScalarIntegerMemoryKind::Load,
                    vector: 14,
                    memory_width: MemWidth::B8,
                    w: false,
                    pp: 2,
                    opcode: 0x7E,
                },
            ),
        ];
        for &(bytes, expected) in valid_cases {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_scalar_integer_memory_encoding(),
                Some(expected),
                "{bytes:02X?}"
            );
        }

        let valid = instruction(9, 11, 1, 0xD6, Form::C4W1);
        let mut invalid = Vec::new();
        let mut reserved_vvvv = valid.clone();
        reserved_vvvv[2] &= !0x08;
        invalid.push(reserved_vvvv);
        let mut l1 = valid.clone();
        l1[2] |= 0x04;
        invalid.push(l1);
        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        invalid.push(wrong_map);
        for (pp, opcode) in [(0, 0xD6), (2, 0xD6), (1, 0x6F)] {
            let mut bytes = valid.clone();
            bytes[2] = (bytes[2] & !3) | pp;
            bytes[3] = opcode;
            invalid.push(bytes);
        }
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

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_scalar_integer_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
