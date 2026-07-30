//! Complete VEX masked-memory load/store classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// One complete `VMASKMOVPS/PD` or `VPMASKMOVD/Q` VEX memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexMaskedMemoryEncoding {
    pub(crate) load: bool,
    pub(crate) elem: VecElementType,
    pub(crate) width: VecWidth,
    pub(crate) memory_width: MemWidth,
    pub(crate) mask: u8,
    /// Load destination or store data source selected by ModR/M.reg.
    pub(crate) vector: u8,
}

impl X86InstructionBytes {
    /// Validate one complete VEX.128/256.66.0F38 masked-memory instruction.
    ///
    /// The shared parser rejects register operands and validates the complete
    /// ModR/M/SIB/displacement shape. Floating forms require W=0; integer W
    /// selects D versus Q elements. Runtime and auxiliary space are O(1)
    /// because architectural x86 instructions are bounded to 15 bytes.
    pub(crate) fn vex_masked_memory_encoding(&self) -> Option<X86VexMaskedMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.pp != 1 {
            return None;
        }
        let (load, elem) = match (fields.opcode, fields.w) {
            (0x2C, false) => (true, VecElementType::F32),
            (0x2D, false) => (true, VecElementType::F64),
            (0x2E, false) => (false, VecElementType::F32),
            (0x2F, false) => (false, VecElementType::F64),
            (0x8C, false) => (true, VecElementType::I32),
            (0x8C, true) => (true, VecElementType::I64),
            (0x8E, false) => (false, VecElementType::I32),
            (0x8E, true) => (false, VecElementType::I64),
            _ => return None,
        };
        Some(X86VexMaskedMemoryEncoding {
            load,
            elem,
            width: if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            memory_width: if elem.bytes() == 4 {
                MemWidth::B4
            } else {
                MemWidth::B8
            },
            mask: fields.source1,
            vector: fields.destination,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(
        opcode: u8,
        w: bool,
        width: VecWidth,
        mask: u8,
        vector: u8,
        base: u8,
    ) -> Vec<u8> {
        assert!(matches!(width, VecWidth::V128 | VecWidth::V256));
        assert!(mask < 16 && vector < 16 && base < 16);
        let mut bytes = vec![
            0xC4,
            (if vector < 8 { 0x80 } else { 0 })
                | (if (mask ^ vector ^ base) & 1 == 0 {
                    0x40
                } else {
                    0
                })
                | (if base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(w) << 7)
                | ((!mask & 0x0F) << 3)
                | (if width == VecWidth::V256 { 0x04 } else { 0 })
                | 1,
            opcode,
            0x40 | ((vector & 7) << 3) | (base & 7),
        ];
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(0x20);
        bytes
    }

    #[test]
    fn classifies_every_family_width_mask_vector_and_base_cell() {
        let families = [
            (0x2C, false, true, VecElementType::F32),
            (0x2D, false, true, VecElementType::F64),
            (0x2E, false, false, VecElementType::F32),
            (0x2F, false, false, VecElementType::F64),
            (0x8C, false, true, VecElementType::I32),
            (0x8C, true, true, VecElementType::I64),
            (0x8E, false, false, VecElementType::I32),
            (0x8E, true, false, VecElementType::I64),
        ];
        let mut classified = 0usize;
        for (opcode, w, load, elem) in families {
            for width in [VecWidth::V128, VecWidth::V256] {
                for mask in 0..16 {
                    for vector in 0..16 {
                        for base in 0..16 {
                            let bytes = instruction(opcode, w, width, mask, vector, base);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_masked_memory_encoding(),
                                Some(X86VexMaskedMemoryEncoding {
                                    load,
                                    elem,
                                    width,
                                    memory_width: if elem.bytes() == 4 {
                                        MemWidth::B4
                                    } else {
                                        MemWidth::B8
                                    },
                                    mask,
                                    vector,
                                }),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 8 * 2 * 16 * 16 * 16);
    }

    #[test]
    fn complete_prefixed_addresses_classify_and_structural_frontiers_fail_closed() {
        for bytes in [
            &[0xC4, 0xE2, 0x71, 0x2C, 0x17][..],
            &[0x65, 0xC4, 0x22, 0x75, 0x2F, 0x94, 0x58, 0x20, 0, 0, 0][..],
            &[0x64, 0x67, 0xC4, 0x82, 0xF1, 0x8E, 0x54, 0x5A, 0x20][..],
        ] {
            assert!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_masked_memory_encoding()
                    .is_some(),
                "{bytes:02X?}"
            );
        }

        let valid = instruction(0x8C, true, VecWidth::V256, 11, 13, 12);
        let mut invalid = Vec::new();
        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 3;
        invalid.push(wrong_map);
        let mut wrong_pp = valid.clone();
        wrong_pp[2] = (wrong_pp[2] & !3) | 2;
        invalid.push(wrong_pp);
        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x8D;
        invalid.push(wrong_opcode);
        let mut float_w1 = instruction(0x2C, false, VecWidth::V128, 1, 2, 3);
        float_w1[2] |= 0x80;
        invalid.push(float_w1);
        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.pop();
        invalid.push(register);
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);
        let mut truncated = valid.clone();
        truncated.pop();
        invalid.push(truncated);
        let mut forbidden_prefix = valid;
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_masked_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
