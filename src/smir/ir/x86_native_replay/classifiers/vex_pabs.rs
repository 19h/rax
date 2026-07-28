//! Complete VEX packed-integer absolute-value memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

impl X86InstructionBytes {
    /// Validate one complete VEX `VPABSB`, `VPABSW`, or `VPABSD` instruction
    /// whose sole source is memory and return
    /// `(destination, element type, vector width, W)`.
    ///
    /// These forms use map 0F38, mandatory prefix 66H, and reserve VEX.vvvv as
    /// encoded `1111b`. VEX.W is ignored by the architecture and is retained
    /// so the native rewrite can reproduce the exact W selection. The shared
    /// parser validates the complete ModR/M/SIB/displacement byte shape and
    /// permits only segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_packed_abs_fields(
        &self,
    ) -> Option<(u8, VecElementType, VecWidth, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.source1 != 0 || fields.map != 2 || fields.pp != 1 {
            return None;
        }
        let elem = match fields.opcode {
            0x1C => VecElementType::I8,
            0x1D => VecElementType::I16,
            0x1E => VecElementType::I32,
            _ => return None,
        };
        Some((
            fields.destination,
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

    fn instruction(
        destination: u8,
        base: u8,
        elem: VecElementType,
        width: VecWidth,
        w: bool,
    ) -> Vec<u8> {
        let opcode = match elem {
            VecElementType::I8 => 0x1C,
            VecElementType::I16 => 0x1D,
            VecElementType::I32 => 0x1E,
            _ => unreachable!(),
        };
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | 0x40
                | (if base < 8 { 0x20 } else { 0 })
                | 0x02,
            (u8::from(w) << 7) | 0x78 | (u8::from(width == VecWidth::V256) << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    #[test]
    fn classifies_every_destination_width_w_and_element_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for base in [3, 11] {
                for elem in [VecElementType::I8, VecElementType::I16, VecElementType::I32] {
                    for width in [VecWidth::V128, VecWidth::V256] {
                        for w in [false, true] {
                            let bytes = instruction(destination, base, elem, width, w);
                            let metadata = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                metadata.vex_memory_packed_abs_fields(),
                                Some((destination, elem, width, w)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 2 * 3 * 2 * 2);
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shapes() {
        // addr32 FS: VPABSD ymm14,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x02, 0xFD, 0x1E, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_packed_abs_fields(),
            Some((14, VecElementType::I32, VecWidth::V256, true))
        );
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = instruction(3, 11, VecElementType::I16, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut nonreserved_vvvv = valid.clone();
        nonreserved_vvvv[2] &= !0x08;
        cases.push(nonreserved_vvvv);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x1F;
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
        forbidden_legacy_prefix.insert(0, 0xF3);
        cases.push(forbidden_legacy_prefix);

        for bytes in cases {
            let metadata = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                metadata.vex_memory_packed_abs_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
