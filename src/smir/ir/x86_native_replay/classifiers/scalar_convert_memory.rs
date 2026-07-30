//! AVX VEX scalar conversion memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{FpRoundMode, OpWidth, VecElementType};

/// Semantic family encoded by one deterministic VEX.L=0 scalar conversion
/// whose final source is memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86VexScalarConvertMemoryKind {
    FpConvert {
        from: VecElementType,
        to: VecElementType,
    },
    IntToFp {
        elem: VecElementType,
        int_width: OpWidth,
    },
    FpToInt {
        elem: VecElementType,
        int_width: OpWidth,
        truncate: bool,
    },
}

/// One complete VEX scalar-conversion memory encoding rewritten to consume a
/// precise-helper result from RAX or a borrowed low XMM register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexScalarConvertMemoryEncoding {
    pub(crate) kind: X86VexScalarConvertMemoryKind,
    pub(crate) destination: u8,
    pub(crate) merge: Option<u8>,
    pub(crate) vector_scratch: Option<u8>,
    pub(crate) memory_size: u32,
    pub(crate) w: bool,
    pub(crate) pp: u8,
    pub(crate) opcode: u8,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86VexScalarConvertMemoryEncoding {
    pub(crate) fn fp_to_int_round(self) -> Option<FpRoundMode> {
        match self.kind {
            X86VexScalarConvertMemoryKind::FpToInt { truncate, .. } => Some(if truncate {
                FpRoundMode::RoundTowardZero
            } else {
                FpRoundMode::Dynamic
            }),
            _ => None,
        }
    }
}

impl X86InstructionBytes {
    /// Validate and rewrite one AVX VEX scalar conversion memory source.
    ///
    /// The admitted family is:
    ///
    /// - `VCVTSS2SD` and `VCVTSD2SS`;
    /// - `VCVTSI2SS` and `VCVTSI2SD`;
    /// - `VCVTSS2SI`, `VCVTSD2SI`, `VCVTTSS2SI`, and `VCVTTSD2SI`.
    ///
    /// Every form uses map 0F and F3/F2 to select binary32/binary64. W is
    /// ignored by FP-to-FP conversion and selects 32-/64-bit integer width for
    /// the other families. FP-to-integer reserves VEX.vvvv as `1111b`; the
    /// vector-destination families consume it as the upper-lane merge source.
    ///
    /// Intel documents VEX.L=1 for every listed scalar instruction as
    /// generation-dependent unpredictable behavior. Only VEX.L=0 is admitted.
    /// The complete memory instruction must contain only segment/address-size
    /// legacy prefixes and an exact ModR/M/SIB/displacement shape.
    pub(crate) fn vex_scalar_convert_memory_encoding(
        &self,
    ) -> Option<X86VexScalarConvertMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.width_256 || !matches!(fields.pp, 2 | 3) {
            return None;
        }

        let fp_elem = if fields.pp == 2 {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        let int_width = if fields.w { OpWidth::W64 } else { OpWidth::W32 };
        let (kind, merge, vector_scratch, memory_size, register_source) = match fields.opcode {
            0x5A => {
                let (from, to) = if fp_elem == VecElementType::F32 {
                    (VecElementType::F32, VecElementType::F64)
                } else {
                    (VecElementType::F64, VecElementType::F32)
                };
                let scratch = (0..8u8)
                    .find(|candidate| {
                        *candidate != fields.destination && *candidate != fields.source1
                    })
                    .expect("two scalar conversion operands leave six low XMM scratch registers");
                (
                    X86VexScalarConvertMemoryKind::FpConvert { from, to },
                    Some(fields.source1),
                    Some(scratch),
                    from.bytes(),
                    scratch,
                )
            }
            0x2A => (
                X86VexScalarConvertMemoryKind::IntToFp {
                    elem: fp_elem,
                    int_width,
                },
                Some(fields.source1),
                None,
                int_width.bytes(),
                0,
            ),
            opcode @ (0x2C | 0x2D) => {
                if fields.source1 != 0 {
                    return None;
                }
                (
                    X86VexScalarConvertMemoryKind::FpToInt {
                        elem: fp_elem,
                        int_width,
                        truncate: opcode == 0x2C,
                    },
                    None,
                    Some(0),
                    fp_elem.bytes(),
                    0,
                )
            }
            _ => return None,
        };

        let register_instruction = self.vex_memory_with_register_source(register_source)?;
        let valid_rewrite = match kind {
            X86VexScalarConvertMemoryKind::FpConvert { .. } => {
                register_instruction.vex_scalar_fp_convert_destination_index()
                    == Some(fields.destination)
            }
            X86VexScalarConvertMemoryKind::IntToFp { .. } => {
                register_instruction.vex_scalar_int_to_fp_destination_index()
                    == Some(fields.destination)
                    && register_instruction.vex_scalar_int_to_fp_source_index() == Some(0)
            }
            X86VexScalarConvertMemoryKind::FpToInt { .. } => {
                register_instruction.vex_scalar_fp_to_int_destination_index()
                    == Some(fields.destination)
            }
        };
        if !valid_rewrite {
            return None;
        }

        Some(X86VexScalarConvertMemoryEncoding {
            kind,
            destination: fields.destination,
            merge,
            vector_scratch,
            memory_size,
            w: fields.w,
            pp: fields.pp,
            opcode: fields.opcode,
            register_instruction,
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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Case {
        opcode: u8,
        pp: u8,
        destination: u8,
        merge: u8,
        base: u8,
        form: Form,
    }

    impl Case {
        fn w(self) -> bool {
            matches!(self.form, Form::C4 { w: true })
        }

        fn memory_bytes(self) -> Vec<u8> {
            let encoded_vvvv = if matches!(self.opcode, 0x2C | 0x2D) {
                0x78
            } else {
                ((!self.merge) & 15) << 3
            };
            let modrm = 0x40 | ((self.destination & 7) << 3) | (self.base & 7);
            match self.form {
                Form::C5 => {
                    assert!(self.base < 8);
                    vec![
                        0xC5,
                        (if self.destination < 8 { 0x80 } else { 0 }) | encoded_vvvv | self.pp,
                        self.opcode,
                        modrm,
                        0x20,
                    ]
                }
                Form::C4 { w } => vec![
                    0xC4,
                    (if self.destination < 8 { 0x80 } else { 0 })
                        | 0x40
                        | (if self.base < 8 { 0x20 } else { 0 })
                        | 1,
                    (u8::from(w) << 7) | encoded_vvvv | self.pp,
                    self.opcode,
                    modrm,
                    0x20,
                ],
            }
        }

        fn expected(self) -> X86VexScalarConvertMemoryEncoding {
            let elem = if self.pp == 2 {
                VecElementType::F32
            } else {
                VecElementType::F64
            };
            let int_width = if self.w() { OpWidth::W64 } else { OpWidth::W32 };
            let (kind, merge, vector_scratch, memory_size, register_source) = match self.opcode {
                0x5A => {
                    let scratch = (0..8)
                        .find(|candidate| {
                            *candidate != self.destination && *candidate != self.merge
                        })
                        .unwrap();
                    (
                        X86VexScalarConvertMemoryKind::FpConvert {
                            from: elem,
                            to: if elem == VecElementType::F32 {
                                VecElementType::F64
                            } else {
                                VecElementType::F32
                            },
                        },
                        Some(self.merge),
                        Some(scratch),
                        elem.bytes(),
                        scratch,
                    )
                }
                0x2A => (
                    X86VexScalarConvertMemoryKind::IntToFp { elem, int_width },
                    Some(self.merge),
                    None,
                    int_width.bytes(),
                    0,
                ),
                opcode @ (0x2C | 0x2D) => (
                    X86VexScalarConvertMemoryKind::FpToInt {
                        elem,
                        int_width,
                        truncate: opcode == 0x2C,
                    },
                    None,
                    Some(0),
                    elem.bytes(),
                    0,
                ),
                _ => unreachable!(),
            };
            let register_instruction = X86InstructionBytes::new(&self.memory_bytes())
                .unwrap()
                .vex_memory_with_register_source(register_source)
                .unwrap();
            X86VexScalarConvertMemoryEncoding {
                kind,
                destination: self.destination,
                merge,
                vector_scratch,
                memory_size,
                w: self.w(),
                pp: self.pp,
                opcode: self.opcode,
                register_instruction,
            }
        }
    }

    #[test]
    fn classifier_covers_all_families_forms_destinations_and_merge_sources() {
        let mut classified = 0usize;
        for opcode in [0x5A, 0x2A, 0x2C, 0x2D] {
            for pp in [2, 3] {
                for form in [Form::C5, Form::C4 { w: false }, Form::C4 { w: true }] {
                    for destination in 0..16 {
                        let merges: &[u8] = if matches!(opcode, 0x2C | 0x2D) {
                            &[0]
                        } else {
                            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
                        };
                        for &merge in merges {
                            let case = Case {
                                opcode,
                                pp,
                                destination,
                                merge,
                                base: if form == Form::C5 { 2 } else { 10 },
                                form,
                            };
                            let actual = X86InstructionBytes::new(&case.memory_bytes())
                                .unwrap()
                                .vex_scalar_convert_memory_encoding();
                            assert_eq!(actual, Some(case.expected()), "{case:?}");
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 3_264);
    }

    #[test]
    fn complete_address_shapes_and_ignored_fields_are_preserved_exactly() {
        let cases: &[(&[u8], u8, Option<u8>, u32, &[u8])] = &[
            (
                &[0x64, 0xC5, 0x6A, 0x5A, 0x4D, 0x20],
                9,
                Some(2),
                4,
                &[0xC5, 0x6A, 0x5A, 0xC8],
            ),
            (
                &[0x65, 0xC4, 0x01, 0xEB, 0x2A, 0x4C, 0xEC, 0x20],
                9,
                Some(2),
                8,
                &[0xC4, 0x21, 0xEB, 0x2A, 0xC8],
            ),
            (
                &[
                    0x67, 0xC4, 0x61, 0x7A, 0x2D, 0x34, 0x75, 0x11, 0x22, 0x33, 0x44,
                ],
                14,
                None,
                4,
                &[0xC4, 0x61, 0x7A, 0x2D, 0xF0],
            ),
            (
                &[0xC4, 0xC1, 0xFB, 0x2C, 0x45, 0x00],
                0,
                None,
                8,
                &[0xC4, 0xE1, 0xFB, 0x2C, 0xC0],
            ),
        ];
        for &(bytes, destination, merge, memory_size, register) in cases {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_scalar_convert_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(encoding.destination, destination, "{bytes:02X?}");
            assert_eq!(encoding.merge, merge, "{bytes:02X?}");
            assert_eq!(encoding.memory_size, memory_size, "{bytes:02X?}");
            assert_eq!(
                encoding.register_instruction.as_slice(),
                register,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn generation_dependent_reserved_and_nonexact_shapes_fail_closed() {
        let valid = Case {
            opcode: 0x5A,
            pp: 2,
            destination: 9,
            merge: 10,
            base: 11,
            form: Form::C4 { w: true },
        }
        .memory_bytes();
        let mut invalid = Vec::new();

        let mut l1 = valid.clone();
        l1[2] |= 0x04;
        invalid.push(l1);
        for (index, xor) in [(1, 0x03), (2, 0x02), (3, 0x10), (4, 0xC0)] {
            let mut bytes = valid.clone();
            bytes[index] ^= xor;
            invalid.push(bytes);
        }
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);
        for end in 0..valid.len() {
            invalid.push(valid[..end].to_vec());
        }
        let mut legacy_prefix = valid.clone();
        legacy_prefix.insert(0, 0x66);
        invalid.push(legacy_prefix);

        let mut reserved_vvvv = Case {
            opcode: 0x2D,
            pp: 2,
            destination: 9,
            merge: 0,
            base: 11,
            form: Form::C4 { w: false },
        }
        .memory_bytes();
        reserved_vvvv[2] &= !0x08;
        invalid.push(reserved_vvvv);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_scalar_convert_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
