//! AVX VEX packed bit-test replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

/// One complete packed bit-test memory encoding rewritten to consume the
/// helper-loaded second source from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexPtestMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) first_source: u8,
    pub(crate) scratch: u8,
    pub(crate) opcode: u8,
    pub(crate) tested_bits: Option<u64>,
    pub(crate) w: bool,
    pub(crate) memory_size: u32,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86InstructionBytes {
    /// Validate a register-only VEX `VPTEST`, `VTESTPS`, or `VTESTPD`.
    ///
    /// All three instructions use map 0F38 with the 66 mandatory prefix,
    /// reserve VEX.vvvv as encoded `1111b`, and accept 128- and 256-bit
    /// vectors. `VPTEST` specifies WIG, whereas `VTESTPS` and `VTESTPD`
    /// require W0. Both vector lengths require AVX. Memory operands and
    /// malformed byte shapes fail closed.
    pub fn is_vex_register_ptest(&self) -> bool {
        matches!(
            self.as_slice(),
            [0xC4, p0, p1, opcode, modrm]
                if p0 & 0x1F == 2
                    && p1 & 0x78 == 0x78
                    && p1 & 0x03 == 1
                    && modrm >> 6 == 3
                    && match opcode {
                        0x17 => true,
                        0x0E | 0x0F => p1 & 0x80 == 0,
                        _ => false,
                }
        )
    }

    /// Validate one complete VEX packed bit test whose second source is memory
    /// and rewrite only that source to a borrowed low vector register.
    ///
    /// `VPTEST`, `VTESTPS`, and `VTESTPD` use three-byte VEX map 0F38,
    /// mandatory prefix 66H, reserved VEX.vvvv=`1111b`, and either vector
    /// width. `VPTEST` specifies WIG; the floating forms require W0. Segment
    /// and address-size prefixes are consumed by guest effective-address
    /// evaluation and omitted from the register rewrite.
    pub(crate) fn vex_ptest_memory_encoding(&self) -> Option<X86VexPtestMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.pp != 1 || fields.source1 != 0 {
            return None;
        }
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let tested_bits = match (fields.opcode, fields.w) {
            (0x17, _) => None,
            (0x0E, false) => Some(0x8000_0000_8000_0000),
            (0x0F, false) => Some(0x8000_0000_0000_0000),
            _ => return None,
        };
        let first_source = fields.destination;
        let scratch = (0..16u8)
            .find(|candidate| *candidate != first_source)
            .expect("one source cannot consume every low vector register");

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
            // Preserve VEX.R and the map, canonicalize X, and encode the
            // borrowed scratch through inverted VEX.B.
            (p0 & 0x9F) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            p1,
            fields.opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes)?;
        let [0xC4, register_p0, _, _, register_modrm] = register_instruction.as_slice() else {
            unreachable!("rewritten VEX packed bit test has a validated shape")
        };
        let rewritten_first =
            ((register_modrm >> 3) & 7) | (u8::from(register_p0 & 0x80 == 0) << 3);
        if !register_instruction.is_vex_register_ptest() || rewritten_first != first_source {
            return None;
        }

        Some(X86VexPtestMemoryEncoding {
            width,
            first_source,
            scratch,
            opcode: fields.opcode,
            tested_bits,
            w: fields.w,
            memory_size: width.bytes(),
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_encoding(
        first_source: u8,
        reserved_source: u8,
        base: u8,
        opcode: u8,
        width: VecWidth,
        w: bool,
    ) -> [u8; 6] {
        assert!(first_source < 16 && reserved_source < 16 && base < 16 && base & 7 != 4);
        [
            0xC4,
            (if first_source < 8 { 0x80 } else { 0 })
                | (if reserved_source & 1 == 0 { 0x40 } else { 0 })
                | (if base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(w) << 7)
                | (((!reserved_source) & 0x0F) << 3)
                | (u8::from(width == VecWidth::V256) << 2)
                | 1,
            opcode,
            0x40 | ((first_source & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    fn expected_tested_bits(opcode: u8, w: bool) -> Option<Option<u64>> {
        match (opcode, w) {
            (0x17, _) => Some(None),
            (0x0E, false) => Some(Some(0x8000_0000_8000_0000)),
            (0x0F, false) => Some(Some(0x8000_0000_0000_0000)),
            _ => None,
        }
    }

    #[test]
    fn memory_classifier_exhaustively_covers_3_072_encoding_cells() {
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        for first_source in 0..16 {
            for reserved_source in 0..16 {
                for opcode in [0x0E, 0x0F, 0x17] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        for w in [false, true] {
                            let base = if first_source & 1 == 0 { 3 } else { 11 };
                            let bytes = memory_encoding(
                                first_source,
                                reserved_source,
                                base,
                                opcode,
                                width,
                                w,
                            );
                            let actual = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_ptest_memory_encoding();
                            let expected = (reserved_source == 0)
                                .then(|| expected_tested_bits(opcode, w))
                                .flatten()
                                .map(|tested_bits| {
                                    let scratch = (0..16)
                                        .find(|candidate| *candidate != first_source)
                                        .unwrap();
                                    let register_bytes = [
                                        0xC4,
                                        (bytes[1] & 0x9F)
                                            | 0x40
                                            | if scratch & 8 == 0 { 0x20 } else { 0 },
                                        bytes[2],
                                        opcode,
                                        0xC0 | ((first_source & 7) << 3) | (scratch & 7),
                                    ];
                                    X86VexPtestMemoryEncoding {
                                        width,
                                        first_source,
                                        scratch,
                                        opcode,
                                        tested_bits,
                                        w,
                                        memory_size: width.bytes(),
                                        register_instruction: X86InstructionBytes::new(
                                            &register_bytes,
                                        )
                                        .unwrap(),
                                    }
                                });
                            assert_eq!(actual, expected, "{bytes:02X?}");
                            if actual.is_some() {
                                accepted += 1;
                            } else {
                                rejected += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 128);
        assert_eq!(accepted + rejected, 3_072);
    }

    #[test]
    fn complete_prefixed_sib_rip_and_addr32_shapes_rewrite_exactly() {
        let cases: &[(&[u8], u8, u8, u8, Option<u64>, bool, VecWidth, &[u8])] = &[
            (
                &[0x64, 0xC4, 0x42, 0x79, 0x0E, 0x4B, 0x20],
                9,
                0,
                0x0E,
                Some(0x8000_0000_8000_0000),
                false,
                VecWidth::V128,
                &[0xC4, 0x62, 0x79, 0x0E, 0xC8],
            ),
            (
                &[0x65, 0xC4, 0x02, 0x7D, 0x0F, 0x74, 0xEC, 0x20],
                14,
                0,
                0x0F,
                Some(0x8000_0000_0000_0000),
                false,
                VecWidth::V256,
                &[0xC4, 0x62, 0x7D, 0x0F, 0xF0],
            ),
            (
                &[
                    0x67, 0xC4, 0x62, 0x79, 0x17, 0x0C, 0x8D, 0x11, 0x22, 0x33, 0x44,
                ],
                9,
                0,
                0x17,
                None,
                false,
                VecWidth::V128,
                &[0xC4, 0x62, 0x79, 0x17, 0xC8],
            ),
            (
                &[0xC4, 0xE2, 0xFD, 0x17, 0x0D, 0x11, 0x22, 0x33, 0x44],
                1,
                0,
                0x17,
                None,
                true,
                VecWidth::V256,
                &[0xC4, 0xE2, 0xFD, 0x17, 0xC8],
            ),
        ];

        for (bytes, first_source, scratch, opcode, tested_bits, w, width, register_bytes) in cases {
            let actual = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_ptest_memory_encoding()
                .unwrap();
            assert_eq!(actual.first_source, *first_source, "{bytes:02X?}");
            assert_eq!(actual.scratch, *scratch, "{bytes:02X?}");
            assert_eq!(actual.opcode, *opcode, "{bytes:02X?}");
            assert_eq!(actual.tested_bits, *tested_bits, "{bytes:02X?}");
            assert_eq!(actual.w, *w, "{bytes:02X?}");
            assert_eq!(actual.width, *width, "{bytes:02X?}");
            assert_eq!(actual.memory_size, width.bytes(), "{bytes:02X?}");
            assert_eq!(
                actual.register_instruction,
                X86InstructionBytes::new(register_bytes).unwrap(),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn malformed_or_semantically_different_memory_shapes_fail_closed() {
        let valid = memory_encoding(9, 0, 11, 0x0E, VecWidth::V256, false).to_vec();
        let mut cases = Vec::new();
        for (index, value) in [
            (1, (valid[1] & !0x1F) | 1),
            (2, valid[2] & !3),
            (2, valid[2] & !0x08),
            (2, valid[2] | 0x80),
            (3, 0x10),
        ] {
            let mut bytes = valid.clone();
            bytes[index] = value;
            cases.push(bytes);
        }

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.pop();
        cases.push(register_source);

        let mut truncated = valid.clone();
        truncated.pop();
        cases.push(truncated);

        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut forbidden_prefix = valid.clone();
        forbidden_prefix.insert(0, 0x66);
        cases.push(forbidden_prefix);

        let mut non_vex = valid;
        non_vex[0] = 0x62;
        cases.push(non_vex);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_ptest_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
