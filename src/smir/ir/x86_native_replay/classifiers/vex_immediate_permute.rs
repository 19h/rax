//! AVX/AVX2 VEX immediate-permute replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// One complete immediate-permute memory encoding rewritten to consume the
/// helper-loaded r/m source from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexImmediatePermuteMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) scratch: u8,
    pub(crate) opcode: u8,
    pub(crate) immediate: u8,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx2: bool,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86VexImmediatePermuteMemoryEncoding {
    /// Absolute source-table lane selected for one architectural destination
    /// lane by the instruction's immediate.
    pub(crate) fn source_lane(self, lane: u8) -> u8 {
        match self.opcode {
            0x04 => {
                let domain_base = lane / 4 * 4;
                domain_base + ((self.immediate >> ((lane % 4) * 2)) & 3)
            }
            0x05 => {
                let domain_base = lane / 2 * 2;
                domain_base + ((self.immediate >> lane) & 1)
            }
            0x00 | 0x01 => (self.immediate >> (lane * 2)) & 3,
            _ => unreachable!("validated VEX immediate-permute opcode"),
        }
    }
}

impl X86InstructionBytes {
    /// Validate one register-source VEX immediate permute and report whether
    /// the selected form requires AVX2 rather than AVX.
    ///
    /// This covers VPERMILPS and VPERMILPD at VEX.128/VEX.256 with W0, plus
    /// VPERMQ and VPERMPD at VEX.256 with W1. Every form uses three-byte VEX
    /// map 0F3A, mandatory prefix 66, reserved VEX.vvvv=`1111b`, and an imm8.
    /// VEX.X is ignored for register operands. Memory operands and malformed
    /// byte shapes fail closed.
    pub fn vex_register_immediate_permute_needs_avx2(&self) -> Option<bool> {
        let [0xC4, p0, p1, opcode, modrm, _] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x78 != 0x78 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        let ymm = p1 & 0x04 != 0;
        match (*opcode, w, ymm) {
            (0x04 | 0x05, false, _) => Some(false),
            (0x00 | 0x01, true, true) => Some(true),
            _ => None,
        }
    }

    /// Return the architectural destination after exact validation. Every
    /// covered instruction writes ModR/M.reg. The AVX-only state bridge uses
    /// this result to clear the destination's state-backed ZMM[511:256] after
    /// architectural VEX upper-zeroing.
    pub(crate) fn vex_immediate_permute_destination_index(&self) -> Option<u8> {
        self.vex_register_immediate_permute_needs_avx2()?;
        let [0xC4, p0, _, _, modrm, _] = self.as_slice() else {
            unreachable!("VEX immediate-permute shape was validated")
        };
        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 })
    }

    /// Validate one complete VEX immediate permute whose source is memory and
    /// rewrite only that source to a borrowed low vector register.
    ///
    /// `VPERMILPS` and `VPERMILPD` use map 0F3A, mandatory prefix 66H,
    /// VEX.W=0, reserved VEX.vvvv=`1111b`, an imm8 selector, and either vector
    /// width. `VPERMQ` and `VPERMPD` additionally require VEX.W=1 and
    /// VEX.L=1. The former pair requires AVX; the latter pair requires AVX2.
    /// Segment and address-size prefixes are consumed by guest
    /// effective-address evaluation and omitted from the register rewrite.
    pub(crate) fn vex_immediate_permute_memory_encoding(
        &self,
    ) -> Option<X86VexImmediatePermuteMemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || fields.source1 != 0 {
            return None;
        }
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let (elem, needs_avx2) = match (fields.opcode, fields.w, fields.width_256) {
            (0x04, false, _) => (VecElementType::F32, false),
            (0x05, false, _) => (VecElementType::F64, false),
            (0x00, true, true) => (VecElementType::I64, true),
            (0x01, true, true) => (VecElementType::F64, true),
            _ => return None,
        };
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one destination cannot consume every low vector register");

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
            immediate,
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes)?;
        if register_instruction.vex_register_immediate_permute_needs_avx2() != Some(needs_avx2)
            || register_instruction.vex_immediate_permute_destination_index()
                != Some(fields.destination)
        {
            return None;
        }

        Some(X86VexImmediatePermuteMemoryEncoding {
            width,
            elem,
            destination: fields.destination,
            scratch,
            opcode: fields.opcode,
            immediate,
            memory_size: width.bytes(),
            needs_avx2,
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_encoding(
        destination: u8,
        source1: u8,
        base: u8,
        opcode: u8,
        width: VecWidth,
        immediate: u8,
        w: bool,
    ) -> [u8; 7] {
        assert!(destination < 16 && source1 < 16 && base < 16 && base & 7 != 4);
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7)
                | (((!source1) & 0x0F) << 3)
                | (u8::from(width == VecWidth::V256) << 2)
                | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    fn expected_shape(opcode: u8, width: VecWidth, w: bool) -> Option<(VecElementType, bool)> {
        match (opcode, w, width) {
            (0x04, false, _) => Some((VecElementType::F32, false)),
            (0x05, false, _) => Some((VecElementType::F64, false)),
            (0x00, true, VecWidth::V256) => Some((VecElementType::I64, true)),
            (0x01, true, VecWidth::V256) => Some((VecElementType::F64, true)),
            _ => None,
        }
    }

    fn expected_source_lane(opcode: u8, immediate: u8, lane: u8) -> u8 {
        match opcode {
            0x04 => lane / 4 * 4 + ((immediate >> ((lane % 4) * 2)) & 3),
            0x05 => lane / 2 * 2 + ((immediate >> lane) & 1),
            0x00 | 0x01 => (immediate >> (lane * 2)) & 3,
            _ => unreachable!(),
        }
    }

    #[test]
    fn memory_classifier_exhaustively_covers_1_048_576_encoding_cells() {
        let mut accepted = 0usize;
        let mut rejected = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for opcode in [0x00, 0x01, 0x04, 0x05] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        for w in [false, true] {
                            for immediate in u8::MIN..=u8::MAX {
                                let base = if immediate & 1 == 0 { 3 } else { 11 };
                                let bytes = memory_encoding(
                                    destination,
                                    source1,
                                    base,
                                    opcode,
                                    width,
                                    immediate,
                                    w,
                                );
                                let actual = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_immediate_permute_memory_encoding();
                                let expected = (source1 == 0)
                                    .then(|| expected_shape(opcode, width, w))
                                    .flatten()
                                    .map(|(elem, needs_avx2)| {
                                        let scratch = (0..16)
                                            .find(|candidate| *candidate != destination)
                                            .unwrap();
                                        let register_bytes = [
                                            0xC4,
                                            (bytes[1] & 0x9F)
                                                | 0x40
                                                | if scratch & 8 == 0 { 0x20 } else { 0 },
                                            bytes[2],
                                            opcode,
                                            0xC0 | ((destination & 7) << 3) | (scratch & 7),
                                            immediate,
                                        ];
                                        X86VexImmediatePermuteMemoryEncoding {
                                            width,
                                            elem,
                                            destination,
                                            scratch,
                                            opcode,
                                            immediate,
                                            memory_size: width.bytes(),
                                            needs_avx2,
                                            register_instruction: X86InstructionBytes::new(
                                                &register_bytes,
                                            )
                                            .unwrap(),
                                        }
                                    });
                                assert_eq!(actual, expected, "{bytes:02X?}");
                                if let Some(encoding) = actual {
                                    let lanes = encoding.width.lanes(encoding.elem) as u8;
                                    for lane in 0..lanes {
                                        assert_eq!(
                                            encoding.source_lane(lane),
                                            expected_source_lane(opcode, immediate, lane),
                                            "{bytes:02X?} lane {lane}"
                                        );
                                    }
                                    accepted += 1;
                                } else {
                                    rejected += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 16 * 6 * 256);
        assert_eq!(accepted + rejected, 1_048_576);
    }

    #[test]
    fn complete_prefixed_sib_rip_and_addr32_shapes_rewrite_exactly() {
        #[derive(Clone, Copy)]
        struct Expected {
            width: VecWidth,
            elem: VecElementType,
            destination: u8,
            scratch: u8,
            opcode: u8,
            immediate: u8,
            memory_size: u32,
            needs_avx2: bool,
        }

        let cases: &[(&[u8], Expected, &[u8])] = &[
            (
                &[0x64, 0xC4, 0x43, 0x79, 0x04, 0x4B, 0x20, 0xA5],
                Expected {
                    width: VecWidth::V128,
                    elem: VecElementType::F32,
                    destination: 9,
                    scratch: 0,
                    opcode: 0x04,
                    immediate: 0xA5,
                    memory_size: 16,
                    needs_avx2: false,
                },
                &[0xC4, 0x63, 0x79, 0x04, 0xC8, 0xA5],
            ),
            (
                &[0x65, 0xC4, 0x03, 0xFD, 0x01, 0x74, 0xEC, 0x20, 0x1B],
                Expected {
                    width: VecWidth::V256,
                    elem: VecElementType::F64,
                    destination: 14,
                    scratch: 0,
                    opcode: 0x01,
                    immediate: 0x1B,
                    memory_size: 32,
                    needs_avx2: true,
                },
                &[0xC4, 0x63, 0xFD, 0x01, 0xF0, 0x1B],
            ),
            (
                &[
                    0x67, 0xC4, 0x63, 0x79, 0x05, 0x0C, 0x8D, 0x11, 0x22, 0x33, 0x44, 0x3C,
                ],
                Expected {
                    width: VecWidth::V128,
                    elem: VecElementType::F64,
                    destination: 9,
                    scratch: 0,
                    opcode: 0x05,
                    immediate: 0x3C,
                    memory_size: 16,
                    needs_avx2: false,
                },
                &[0xC4, 0x63, 0x79, 0x05, 0xC8, 0x3C],
            ),
            (
                &[0xC4, 0xE3, 0xFD, 0x00, 0x0D, 0x11, 0x22, 0x33, 0x44, 0xFF],
                Expected {
                    width: VecWidth::V256,
                    elem: VecElementType::I64,
                    destination: 1,
                    scratch: 0,
                    opcode: 0x00,
                    immediate: 0xFF,
                    memory_size: 32,
                    needs_avx2: true,
                },
                &[0xC4, 0xE3, 0xFD, 0x00, 0xC8, 0xFF],
            ),
        ];

        for (bytes, expected_without_instruction, register_bytes) in cases {
            let actual = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_immediate_permute_memory_encoding()
                .unwrap();
            assert_eq!(
                actual.width, expected_without_instruction.width,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.elem, expected_without_instruction.elem,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.destination, expected_without_instruction.destination,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.scratch, expected_without_instruction.scratch,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.opcode, expected_without_instruction.opcode,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.immediate, expected_without_instruction.immediate,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.memory_size, expected_without_instruction.memory_size,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.needs_avx2, expected_without_instruction.needs_avx2,
                "{bytes:02X?}"
            );
            assert_eq!(
                actual.register_instruction,
                X86InstructionBytes::new(register_bytes).unwrap(),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn malformed_or_semantically_different_memory_shapes_fail_closed() {
        let valid = memory_encoding(9, 0, 11, 0x01, VecWidth::V256, 0xA5, true).to_vec();
        let mut cases = Vec::new();
        for (index, value) in [
            (1, (valid[1] & !0x1F) | 2),
            (2, valid[2] & !3),
            (2, valid[2] & !0x80),
            (2, valid[2] & !0x04),
            (2, valid[2] & !0x08),
            (3, 0x02),
        ] {
            let mut bytes = valid.clone();
            bytes[index] = value;
            cases.push(bytes);
        }

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
                    .vex_immediate_permute_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
