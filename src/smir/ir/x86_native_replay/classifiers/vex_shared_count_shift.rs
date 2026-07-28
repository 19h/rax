//! Complete VEX packed shared-count shift memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{ShiftOp, VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete VEX packed shift whose shared count operand is
    /// memory and return `(destination, source, element type, shift, width,
    /// opcode, W)`.
    ///
    /// Every form uses map 0F, mandatory prefix 66H, a 128-bit count source,
    /// and specifies WIG. The shared parser validates every prefix, ModR/M,
    /// SIB, displacement, and complete-instruction boundary before this
    /// semantic classification.
    pub(crate) fn vex_memory_shared_count_shift_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, ShiftOp, VecWidth, u8, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || fields.pp != 1 {
            return None;
        }
        let (elem, shift) = match fields.opcode {
            0xD1 => (VecElementType::I16, ShiftOp::Lsr),
            0xD2 => (VecElementType::I32, ShiftOp::Lsr),
            0xD3 => (VecElementType::I64, ShiftOp::Lsr),
            0xE1 => (VecElementType::I16, ShiftOp::Asr),
            0xE2 => (VecElementType::I32, ShiftOp::Asr),
            0xF1 => (VecElementType::I16, ShiftOp::Lsl),
            0xF2 => (VecElementType::I32, ShiftOp::Lsl),
            0xF3 => (VecElementType::I64, ShiftOp::Lsl),
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            elem,
            shift,
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

    const KINDS: [(u8, VecElementType, ShiftOp); 8] = [
        (0xD1, VecElementType::I16, ShiftOp::Lsr),
        (0xD2, VecElementType::I32, ShiftOp::Lsr),
        (0xD3, VecElementType::I64, ShiftOp::Lsr),
        (0xE1, VecElementType::I16, ShiftOp::Asr),
        (0xE2, VecElementType::I32, ShiftOp::Asr),
        (0xF1, VecElementType::I16, ShiftOp::Lsl),
        (0xF2, VecElementType::I32, ShiftOp::Lsl),
        (0xF3, VecElementType::I64, ShiftOp::Lsl),
    ];

    fn vex2_instruction(
        destination: u8,
        source: u8,
        base: u8,
        opcode: u8,
        width: VecWidth,
    ) -> Vec<u8> {
        assert!(base < 8);
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC5,
            (if destination < 8 { 0x80 } else { 0 }) | (((!source) & 0x0F) << 3) | (l << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | base,
            0x20,
        ]
    }

    fn vex3_instruction(
        destination: u8,
        source: u8,
        base: u8,
        opcode: u8,
        width: VecWidth,
        w: bool,
    ) -> Vec<u8> {
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 1,
            (u8::from(w) << 7) | (((!source) & 0x0F) << 3) | (l << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    #[test]
    fn classifies_every_destination_source_kind_width_form_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source in 0..16 {
                for (opcode, elem, shift) in KINDS {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        let bytes = vex2_instruction(destination, source, 3, opcode, width);
                        let metadata = X86InstructionBytes::new(&bytes).unwrap();
                        assert_eq!(
                            metadata.vex_memory_shared_count_shift_fields(),
                            Some((destination, source, elem, shift, width, opcode, false)),
                            "{bytes:02X?}"
                        );
                        classified += 1;

                        for base in [3, 11] {
                            for w in [false, true] {
                                let bytes =
                                    vex3_instruction(destination, source, base, opcode, width, w);
                                let metadata = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    metadata.vex_memory_shared_count_shift_fields(),
                                    Some((destination, source, elem, shift, width, opcode, w)),
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
        // addr32 FS: VPSRAD ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x01, 0xB5, 0xE2, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_shared_count_shift_fields(),
            Some((
                14,
                9,
                VecElementType::I32,
                ShiftOp::Asr,
                VecWidth::V256,
                0xE2,
                true,
            ))
        );

        // GS: VPSLLW xmm14,xmm9,[r14+r15*2+0x44332211]
        let bytes = [
            0x65, 0xC4, 0x01, 0x31, 0xF1, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_shared_count_shift_fields(),
            Some((
                14,
                9,
                VecElementType::I16,
                ShiftOp::Lsl,
                VecWidth::V128,
                0xF1,
                false,
            ))
        );
    }

    #[test]
    fn malformed_or_semantically_different_memory_encodings_fail_closed() {
        let valid = vex3_instruction(3, 9, 11, 0xE2, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0xE3;
        cases.push(wrong_opcode);

        let mut register_count = valid.clone();
        register_count[4] |= 0xC0;
        register_count.truncate(5);
        cases.push(register_count);

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
                metadata.vex_memory_shared_count_shift_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
