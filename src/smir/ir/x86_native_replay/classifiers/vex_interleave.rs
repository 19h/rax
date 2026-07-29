//! Complete VEX packed interleave memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete VEX packed low/high interleave instruction whose
    /// second source operand is memory and return `(destination, first source,
    /// element type, high half, width, opcode, W)`.
    ///
    /// Every form uses map 0F and specifies WIG. Packed-integer and packed
    /// binary64 forms use mandatory prefix 66H; packed binary32 uses no
    /// mandatory prefix. The shared parser validates every prefix, ModR/M,
    /// SIB, displacement, and complete-instruction boundary before this
    /// semantic classification.
    pub(crate) fn vex_memory_interleave_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, bool, VecWidth, u8, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || !matches!(fields.pp, 0 | 1) {
            return None;
        }
        let (elem, high) = match (fields.pp, fields.opcode) {
            (0, 0x14) => (VecElementType::F32, false),
            (0, 0x15) => (VecElementType::F32, true),
            (1, 0x14) => (VecElementType::F64, false),
            (1, 0x15) => (VecElementType::F64, true),
            (1, 0x60) => (VecElementType::I8, false),
            (1, 0x61) => (VecElementType::I16, false),
            (1, 0x62) => (VecElementType::I32, false),
            (1, 0x6C) => (VecElementType::I64, false),
            (1, 0x68) => (VecElementType::I8, true),
            (1, 0x69) => (VecElementType::I16, true),
            (1, 0x6A) => (VecElementType::I32, true),
            (1, 0x6D) => (VecElementType::I64, true),
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            elem,
            high,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            fields.opcode,
            fields.w,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [(u8, u8, VecElementType, bool); 12] = [
        (0, 0x14, VecElementType::F32, false),
        (0, 0x15, VecElementType::F32, true),
        (1, 0x14, VecElementType::F64, false),
        (1, 0x15, VecElementType::F64, true),
        (1, 0x60, VecElementType::I8, false),
        (1, 0x61, VecElementType::I16, false),
        (1, 0x62, VecElementType::I32, false),
        (1, 0x6C, VecElementType::I64, false),
        (1, 0x68, VecElementType::I8, true),
        (1, 0x69, VecElementType::I16, true),
        (1, 0x6A, VecElementType::I32, true),
        (1, 0x6D, VecElementType::I64, true),
    ];

    fn vex2_instruction(
        destination: u8,
        source1: u8,
        base: u8,
        pp: u8,
        opcode: u8,
        width: VecWidth,
    ) -> Vec<u8> {
        assert!(base < 8);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC5,
            (if destination < 8 { 0x80 } else { 0 }) | (((!source1) & 0x0F) << 3) | (l << 2) | pp,
            opcode,
            0x40 | ((destination & 7) << 3) | base,
            0x20,
        ]
    }

    fn vex3_instruction(
        destination: u8,
        source1: u8,
        base: u8,
        pp: u8,
        opcode: u8,
        width: VecWidth,
        w: bool,
    ) -> Vec<u8> {
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 1,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | pp,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    #[test]
    fn classifies_every_destination_source_kind_width_form_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for (pp, opcode, elem, high) in KINDS {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        let bytes = vex2_instruction(destination, source1, 3, pp, opcode, width);
                        let metadata = X86InstructionBytes::new(&bytes).unwrap();
                        assert_eq!(
                            metadata.vex_memory_interleave_fields(),
                            Some((destination, source1, elem, high, width, opcode, false)),
                            "{bytes:02X?}"
                        );
                        classified += 1;

                        for base in [3, 11] {
                            for w in [false, true] {
                                let bytes = vex3_instruction(
                                    destination,
                                    source1,
                                    base,
                                    pp,
                                    opcode,
                                    width,
                                    w,
                                );
                                let metadata = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    metadata.vex_memory_interleave_fields(),
                                    Some((destination, source1, elem, high, width, opcode, w)),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * KINDS.len() * 2 * 5);
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shapes() {
        // addr32 FS: VPUNPCKHDQ ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x01, 0xB5, 0x6A, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_interleave_fields(),
            Some((14, 9, VecElementType::I32, true, VecWidth::V256, 0x6A, true,))
        );

        // GS: VPUNPCKLBW xmm14,xmm9,[r14+r15*2+0x44332211]
        let bytes = [
            0x65, 0xC4, 0x01, 0x31, 0x60, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_interleave_fields(),
            Some((
                14,
                9,
                VecElementType::I8,
                false,
                VecWidth::V128,
                0x60,
                false,
            ))
        );

        // addr32 FS: VUNPCKHPS ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x01, 0xB4, 0x15, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_interleave_fields(),
            Some((14, 9, VecElementType::F32, true, VecWidth::V256, 0x15, true,))
        );

        // GS: VUNPCKLPD xmm14,xmm9,[r14+r15*2+0x44332211]
        let bytes = [
            0x65, 0xC4, 0x01, 0x31, 0x14, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_interleave_fields(),
            Some((
                14,
                9,
                VecElementType::F64,
                false,
                VecWidth::V128,
                0x14,
                false,
            ))
        );
    }

    #[test]
    fn malformed_or_semantically_different_memory_encodings_fail_closed() {
        let valid = vex3_instruction(3, 9, 11, 1, 0x6A, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x63;
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
                metadata.vex_memory_interleave_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
