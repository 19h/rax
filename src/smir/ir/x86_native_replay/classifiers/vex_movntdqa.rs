//! Complete VEX `VMOVNTDQA` memory-source classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

/// One complete VEX `VMOVNTDQA` memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexMovntdqaMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) width: VecWidth,
    pub(crate) w: bool,
}

impl X86InstructionBytes {
    /// Validate one complete `VEX.128/256.66.0F38.WIG 2A /r`
    /// `VMOVNTDQA` instruction whose source is memory.
    ///
    /// VEX.vvvv is reserved as encoded `1111b`; both vector lengths are
    /// defined, and W is architecturally ignored but retained for provenance
    /// checks. The shared parser validates the complete
    /// ModR/M/SIB/displacement shape and accepts only segment/address-size
    /// legacy prefixes.
    ///
    /// Runtime and auxiliary space are O(1) because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn vex_movntdqa_memory_encoding(&self) -> Option<X86VexMovntdqaMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.source1 != 0 || fields.map != 2 || fields.pp != 1 || fields.opcode != 0x2A {
            return None;
        }
        Some(X86VexMovntdqaMemoryEncoding {
            destination: fields.destination,
            width: if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            w: fields.w,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(destination: u8, base: u8, width: VecWidth, w: bool) -> Vec<u8> {
        assert!(destination < 16 && base < 16);
        assert!(matches!(width, VecWidth::V128 | VecWidth::V256));
        let mut bytes = vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | (if (destination ^ base ^ u8::from(w)) & 1 == 0 {
                    0x40
                } else {
                    0
                })
                | (if base < 8 { 0x20 } else { 0 })
                | 2,
            (u8::from(w) << 7) | 0x78 | (if width == VecWidth::V256 { 0x04 } else { 0 }) | 1,
            0x2A,
            0x40 | ((destination & 7) << 3) | (base & 7),
        ];
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(0x20);
        bytes
    }

    #[test]
    fn classifies_every_destination_base_width_wig_and_ignored_x_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for base in 0..16 {
                for width in [VecWidth::V128, VecWidth::V256] {
                    for w in [false, true] {
                        let bytes = instruction(destination, base, width, w);
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_movntdqa_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(
                            encoding,
                            X86VexMovntdqaMemoryEncoding {
                                destination,
                                width,
                                w,
                            }
                        );
                        classified += 1;
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 2);
    }

    #[test]
    fn llvm_23_and_complete_prefixed_address_shapes_classify_exactly() {
        for (bytes, expected) in [
            (
                &[0xC4, 0x42, 0x79, 0x2A, 0x4B, 0x20][..],
                X86VexMovntdqaMemoryEncoding {
                    destination: 9,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            (
                &[0xC4, 0x42, 0x7D, 0x2A, 0x4B, 0x20][..],
                X86VexMovntdqaMemoryEncoding {
                    destination: 9,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            (
                &[
                    0x64, 0x67, 0xC4, 0x02, 0xF9, 0x2A, 0xB4, 0x7E, 0x44, 0x33, 0x22, 0x11,
                ][..],
                X86VexMovntdqaMemoryEncoding {
                    destination: 14,
                    width: VecWidth::V128,
                    w: true,
                },
            ),
            (
                &[
                    0x65, 0x67, 0xC4, 0x02, 0xFD, 0x2A, 0xB4, 0x7E, 0x40, 0x33, 0x22, 0x11,
                ][..],
                X86VexMovntdqaMemoryEncoding {
                    destination: 14,
                    width: VecWidth::V256,
                    w: true,
                },
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .vex_movntdqa_memory_encoding(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn reserved_register_and_nonexact_encodings_fail_closed() {
        let valid = instruction(9, 11, VecWidth::V256, true);
        let mut invalid = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 3;
        invalid.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        invalid.push(wrong_prefix);

        let mut nonreserved_vvvv = valid.clone();
        nonreserved_vvvv[2] &= !0x08;
        invalid.push(nonreserved_vvvv);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x2B;
        invalid.push(wrong_opcode);

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

        let mut forbidden_prefix = valid.clone();
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);

        let mut evex = valid;
        evex[0] = 0x62;
        invalid.push(evex);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_movntdqa_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
