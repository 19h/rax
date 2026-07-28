//! AVX VEX widening doubleword-multiply replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

impl X86InstructionBytes {
    /// Validate one register-only VEX `VPMULUDQ` or `VPMULDQ` instruction and
    /// report whether its vector length requires AVX2 rather than AVX.
    ///
    /// `VPMULUDQ` uses VEX map 0F opcode F4; `VPMULDQ` uses map 0F38 opcode
    /// 28. Both require mandatory prefix 66 and specify WIG. VEX.128 requires
    /// AVX, while VEX.256 requires AVX2. Memory operands and every malformed
    /// or non-canonical byte shape fail closed.
    pub fn vex_register_widening_dword_multiply_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let (map, p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (1, *p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] => (p0 & 0x1F, *p1, *opcode, *modrm),
            _ => return None,
        };

        (matches!((map, opcode), (1, 0xF4) | (2, 0x28)) && p1 & 0x03 == 1 && modrm >> 6 == 3)
            .then_some(p1 & 0x04 != 0)
    }

    /// Validate one complete VEX `VPMULUDQ` or `VPMULDQ` instruction whose
    /// second source operand is memory and return
    /// `(destination, first source, signed, width, W)`.
    ///
    /// `VPMULUDQ` uses map 0F and opcode F4H; `VPMULDQ` uses map 0F38 and
    /// opcode 28H. Both require mandatory prefix 66H and specify WIG. The
    /// shared memory parser validates every prefix, ModR/M, SIB, displacement,
    /// and complete-instruction boundary before this semantic classification.
    pub(crate) fn vex_memory_widening_dword_multiply_fields(
        &self,
    ) -> Option<(u8, u8, bool, VecWidth, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.pp != 1 {
            return None;
        }
        let signed = match (fields.map, fields.opcode) {
            (1, 0xF4) => false,
            (2, 0x28) => true,
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            signed,
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
mod memory_tests {
    use super::*;

    fn map_and_opcode(signed: bool) -> (u8, u8) {
        if signed { (2, 0x28) } else { (1, 0xF4) }
    }

    fn vex2_instruction(destination: u8, source1: u8, base: u8, width: VecWidth) -> Vec<u8> {
        assert!(base < 8);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC5,
            (if destination < 8 { 0x80 } else { 0 }) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
            0xF4,
            0x40 | ((destination & 7) << 3) | base,
            0x20,
        ]
    }

    fn vex3_instruction(
        destination: u8,
        source1: u8,
        base: u8,
        signed: bool,
        width: VecWidth,
        w: bool,
    ) -> Vec<u8> {
        let (map, opcode) = map_and_opcode(signed);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if base < 8 { 0x20 } else { 0 })
                | map,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    #[test]
    fn classifies_every_destination_source_signedness_width_form_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for signed in [false, true] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        if !signed {
                            let bytes = vex2_instruction(destination, source1, 3, width);
                            let metadata = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                metadata.vex_memory_widening_dword_multiply_fields(),
                                Some((destination, source1, signed, width, false)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }

                        for base in [3, 11] {
                            for w in [false, true] {
                                let bytes =
                                    vex3_instruction(destination, source1, base, signed, width, w);
                                let metadata = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    metadata.vex_memory_widening_dword_multiply_fields(),
                                    Some((destination, source1, signed, width, w)),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * (5 + 4));
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shapes() {
        // addr32 FS: VPMULDQ ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x02, 0xB5, 0x28, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_widening_dword_multiply_fields(),
            Some((14, 9, true, VecWidth::V256, true))
        );

        // GS: VPMULUDQ xmm14,xmm9,[r14+r15*2+0x44332211]
        let bytes = [
            0x65, 0xC4, 0x01, 0xB1, 0xF4, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_widening_dword_multiply_fields(),
            Some((14, 9, false, VecWidth::V128, true))
        );
    }

    #[test]
    fn malformed_or_semantically_different_memory_encodings_fail_closed() {
        let valid = vex3_instruction(3, 9, 11, true, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x29;
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

        // Map 0F38 cannot be represented by VEX2.
        cases.push(vec![0xC5, 0xF1, 0x28, 0x40, 0x20]);

        for bytes in cases {
            let metadata = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                metadata.vex_memory_widening_dword_multiply_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
