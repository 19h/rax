//! Complete VEX `VMOVSS`/`VMOVSD` scalar floating-point memory classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::MemWidth;

/// Direction of the scalar guest-memory transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86VexScalarFpMemoryKind {
    Load,
    Store,
}

/// Exact fields for one VEX `VMOVSS` or `VMOVSD` memory form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexScalarFpMemoryEncoding {
    pub(crate) kind: X86VexScalarFpMemoryKind,
    pub(crate) vector: u8,
    pub(crate) memory_width: MemWidth,
    /// Exact L bit. `VMOVSD` is LIG; `VMOVSS` is admitted only at L=0 because
    /// Intel documents L=1 as generation-dependent unpredictable behavior.
    pub(crate) width_256: bool,
    /// Exact ignored W bit, retained to bind source metadata to its SMIR hint.
    pub(crate) w: bool,
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
}

impl X86InstructionBytes {
    /// Validate one complete VEX `VMOVSS` or `VMOVSD` memory form.
    ///
    /// Both families use map 0F opcodes 10H/11H, reserve VEX.vvvv as encoded
    /// `1111b`, and treat W as ignored. `VMOVSS` transfers 4 bytes; admission
    /// restricts it to L=0 because Intel documents L=1 as
    /// generation-dependent unpredictable. `VMOVSD` transfers 8 bytes and
    /// accepts both L encodings because it is LIG. These moves are bit
    /// transfers with no SIMD floating-point exceptions. The shared parser
    /// validates the complete
    /// ModR/M/SIB/displacement image and accepts only segment/address-size
    /// legacy prefixes.
    ///
    /// Classification is O(1) time and O(1) space because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn vex_scalar_fp_memory_encoding(&self) -> Option<X86VexScalarFpMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.source1 != 0 || !matches!(fields.opcode, 0x10 | 0x11) {
            return None;
        }

        let memory_width = match fields.pp {
            2 if !fields.width_256 => MemWidth::B4,
            3 => MemWidth::B8,
            _ => return None,
        };
        let kind = if fields.opcode == 0x10 {
            X86VexScalarFpMemoryKind::Load
        } else {
            X86VexScalarFpMemoryKind::Store
        };

        Some(X86VexScalarFpMemoryEncoding {
            kind,
            vector: fields.destination,
            memory_width,
            width_256: fields.width_256,
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

    fn instruction(
        vector: u8,
        base: u8,
        pp: u8,
        opcode: u8,
        width_256: bool,
        form: Form,
    ) -> Vec<u8> {
        assert!(vector < 16 && base < 16);
        let modrm = 0x40 | ((vector & 7) << 3) | (base & 7);
        let mut bytes = match form {
            Form::C5 => {
                assert!(base < 8);
                vec![
                    0xC5,
                    (if vector < 8 { 0x80 } else { 0 }) | 0x78 | (u8::from(width_256) << 2) | pp,
                    opcode,
                    modrm,
                ]
            }
            Form::C4W0 | Form::C4W1 => vec![
                0xC4,
                (if vector < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 1,
                (u8::from(form.w()) << 7) | 0x78 | (u8::from(width_256) << 2) | pp,
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

    #[test]
    fn classifies_all_288_form_vector_width_and_direction_cells() {
        let mut classified = 0usize;
        for form in Form::ALL {
            for vector in 0..16 {
                for (pp, memory_width, lengths) in [
                    (2, MemWidth::B4, &[false][..]),
                    (3, MemWidth::B8, &[false, true][..]),
                ] {
                    for &width_256 in lengths {
                        for opcode in [0x10, 0x11] {
                            let bytes = instruction(
                                vector,
                                if form == Form::C5 { 3 } else { 11 },
                                pp,
                                opcode,
                                width_256,
                                form,
                            );
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_scalar_fp_memory_encoding(),
                                Some(X86VexScalarFpMemoryEncoding {
                                    kind: if opcode == 0x10 {
                                        X86VexScalarFpMemoryKind::Load
                                    } else {
                                        X86VexScalarFpMemoryKind::Store
                                    },
                                    vector,
                                    memory_width,
                                    width_256,
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
            }
        }
        assert_eq!(classified, 3 * 16 * (2 + 4));
    }

    #[test]
    fn complete_addresses_ignored_fields_and_reserved_fields_are_exact() {
        let valid_cases: &[(&[u8], X86VexScalarFpMemoryEncoding)] = &[
            (
                &[0x64, 0xC5, 0x7A, 0x11, 0x4D, 0x20],
                X86VexScalarFpMemoryEncoding {
                    kind: X86VexScalarFpMemoryKind::Store,
                    vector: 9,
                    memory_width: MemWidth::B4,
                    width_256: false,
                    w: false,
                    pp: 2,
                    opcode: 0x11,
                },
            ),
            (
                &[0x65, 0xC4, 0x41, 0xFF, 0x10, 0x74, 0x24, 0x20],
                X86VexScalarFpMemoryEncoding {
                    kind: X86VexScalarFpMemoryKind::Load,
                    vector: 14,
                    memory_width: MemWidth::B8,
                    width_256: true,
                    w: true,
                    pp: 3,
                    opcode: 0x10,
                },
            ),
            (
                &[
                    0x67, 0xC4, 0x61, 0x7B, 0x11, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
                ],
                X86VexScalarFpMemoryEncoding {
                    kind: X86VexScalarFpMemoryKind::Store,
                    vector: 14,
                    memory_width: MemWidth::B8,
                    width_256: false,
                    w: false,
                    pp: 3,
                    opcode: 0x11,
                },
            ),
        ];
        for &(bytes, expected) in valid_cases {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_scalar_fp_memory_encoding(),
                Some(expected),
                "{bytes:02X?}"
            );
        }

        let valid = instruction(9, 11, 2, 0x11, false, Form::C4W1);
        let mut invalid = Vec::new();
        let mut reserved_vvvv = valid.clone();
        reserved_vvvv[2] &= !0x08;
        invalid.push(reserved_vvvv);
        let mut vmovss_l1 = valid.clone();
        vmovss_l1[2] |= 0x04;
        invalid.push(vmovss_l1);
        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        invalid.push(wrong_map);
        for (pp, opcode) in [(0, 0x11), (1, 0x11), (2, 0x12)] {
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
        let mut truncated_sib = instruction(9, 4, 2, 0x10, false, Form::C4W0);
        truncated_sib.pop();
        invalid.push(truncated_sib);
        let mut forbidden_prefix = valid.clone();
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_scalar_fp_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
