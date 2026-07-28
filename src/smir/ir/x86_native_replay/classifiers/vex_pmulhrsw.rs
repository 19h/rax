//! Complete VEX packed rounded-high word-multiply memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

impl X86InstructionBytes {
    /// Validate one complete VEX `VPMULHRSW` instruction whose second source
    /// operand is memory and return `(destination, first source, width, W)`.
    ///
    /// This form uses map 0F38, mandatory prefix 66H, and opcode 0BH. VEX.W is
    /// ignored by the architecture and retained here so callers can prove that
    /// both W selections were classified before canonicalizing native replay
    /// to W=0. The shared parser validates the complete
    /// ModR/M/SIB/displacement byte shape and permits only
    /// segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_pmulhrsw_fields(&self) -> Option<(u8, u8, VecWidth, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.pp != 1 || fields.opcode != 0x0B {
            return None;
        }
        Some((
            fields.destination,
            fields.source1,
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

    fn instruction(destination: u8, source1: u8, base: u8, width: VecWidth, w: bool) -> Vec<u8> {
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 2,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (l << 2) | 1,
            0x0B,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    #[test]
    fn classifies_every_destination_source_width_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for width in [VecWidth::V128, VecWidth::V256] {
                    for base in [3, 11] {
                        for w in [false, true] {
                            let bytes = instruction(destination, source1, base, width, w);
                            let metadata = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                metadata.vex_memory_pmulhrsw_fields(),
                                Some((destination, source1, width, w)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 2 * 2);
    }

    #[test]
    fn accepts_complete_prefixed_sib_and_displacement_shape() {
        // addr32 FS: VPMULHRSW ymm14,ymm9,[r14d+r15d*2+0x44332211]
        let bytes = [
            0x64, 0x67, 0xC4, 0x02, 0xB5, 0x0B, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
        ];
        let metadata = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_pmulhrsw_fields(),
            Some((14, 9, VecWidth::V256, true))
        );
    }

    #[test]
    fn malformed_or_semantically_different_encodings_fail_closed() {
        let valid = instruction(3, 9, 11, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x0A;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.truncate(5);
        cases.push(register_source);

        cases.push(vec![0xC5, 0x31, 0x0B, 0x5B, 0x20]);

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
            assert_eq!(metadata.vex_memory_pmulhrsw_fields(), None, "{bytes:02X?}");
        }
    }
}
