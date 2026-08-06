//! Complete EVEX `VMOVNTDQA` memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

/// One complete EVEX `VMOVNTDQA` memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexMovntdqaMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) width: VecWidth,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one complete `EVEX.128/256/512.66.0F38.W0 2A /r`
    /// `VMOVNTDQA` instruction whose source is memory.
    ///
    /// Intel SDM revision 092 defines a Full Mem tuple, a reserved
    /// `vvvv/V' = 11111b` source field, no writemask, no zeroing, no EVEX.b,
    /// and three defined vector lengths. P0.B4 and P1.X4 remain admissible
    /// APX memory-address extensions; their dynamic APX requirement is bound
    /// by the exact SMIR sequence matcher. The shared parser validates the
    /// complete ModR/M/SIB/displacement shape and accepts only segment and
    /// address-size legacy prefixes.
    ///
    /// Runtime and auxiliary space are O(1) because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn evex_movntdqa_memory_encoding(&self) -> Option<X86EvexMovntdqaMemoryEncoding> {
        let bytes = self.as_slice();
        let start = vector_legacy_prefix_len(bytes);
        if bytes.get(start) != Some(&0x62) {
            return None;
        }

        let p0 = *bytes.get(start + 1)?;
        let p1 = *bytes.get(start + 2)?;
        let p2 = *bytes.get(start + 3)?;
        let opcode = *bytes.get(start + 4)?;
        let modrm_index = start + 5;
        let modrm = *bytes.get(modrm_index)?;
        let ll = (p2 >> 5) & 3;
        let width = match ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => return None,
        };

        if p0 & 7 != 2
            || p1 & 0x80 != 0
            || p1 & 0x78 != 0x78
            || p1 & 3 != 1
            || p2 & 0x9F != 0x08
            || opcode != 0x2A
            || modrm >> 6 == 3
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
        {
            return None;
        }

        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        Some(X86EvexMovntdqaMemoryEncoding {
            destination,
            width,
            needs_avx512vl: width != VecWidth::V512,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(destination: u8, base: u8, width: VecWidth) -> Vec<u8> {
        assert!(destination < 32 && base < 32);
        let ll = match width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        };
        let mut bytes = vec![
            0x62,
            0x42 | (u8::from(destination & 8 == 0) << 7)
                | (u8::from(base & 8 == 0) << 5)
                | (u8::from(destination < 16) << 4)
                | (u8::from(base >= 16) << 3),
            0x7D,
            (ll << 5) | 0x08,
            0x2A,
            0x40 | ((destination & 7) << 3) | (base & 7),
        ];
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(1);
        bytes
    }

    #[test]
    fn classifies_every_destination_base_and_width_cell() {
        let mut classified = 0usize;
        for destination in 0..32 {
            for base in 0..32 {
                for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
                    let bytes = instruction(destination, base, width);
                    let encoding = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_movntdqa_memory_encoding()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(
                        encoding,
                        X86EvexMovntdqaMemoryEncoding {
                            destination,
                            width,
                            needs_avx512vl: width != VecWidth::V512,
                        }
                    );
                    classified += 1;
                }
            }
        }
        assert_eq!(classified, 32 * 32 * 3);
    }

    #[test]
    fn exhausts_map_opcode_prefix_w_and_vector_length_selector_space() {
        let mut classified = 0usize;
        for map in 0u8..=7 {
            for opcode in u8::MIN..=u8::MAX {
                for pp in 0u8..=3 {
                    for w in [false, true] {
                        for ll in 0u8..=3 {
                            let bytes = [
                                0x62,
                                0xF0 | map,
                                (u8::from(w) << 7) | 0x7C | pp,
                                (ll << 5) | 0x08,
                                opcode,
                                0x08,
                            ];
                            let actual = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_movntdqa_memory_encoding();
                            let expected = map == 2 && opcode == 0x2A && pp == 1 && !w && ll < 3;
                            assert_eq!(actual.is_some(), expected, "{bytes:02X?}");
                            classified += usize::from(expected);
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 3);
    }

    #[test]
    fn llvm_23_and_apx_address_anchors_classify_exactly() {
        for (bytes, expected) in [
            (
                &[0x62, 0x52, 0x7D, 0x08, 0x2A, 0x4B, 0x04][..],
                X86EvexMovntdqaMemoryEncoding {
                    destination: 9,
                    width: VecWidth::V128,
                    needs_avx512vl: true,
                },
            ),
            (
                &[0x62, 0xC2, 0x7D, 0x28, 0x2A, 0x4B, 0x04][..],
                X86EvexMovntdqaMemoryEncoding {
                    destination: 17,
                    width: VecWidth::V256,
                    needs_avx512vl: true,
                },
            ),
            (
                &[0x62, 0x42, 0x7D, 0x48, 0x2A, 0x7B, 0x04][..],
                X86EvexMovntdqaMemoryEncoding {
                    destination: 31,
                    width: VecWidth::V512,
                    needs_avx512vl: false,
                },
            ),
            (
                &[0x62, 0xEA, 0x79, 0x48, 0x2A, 0x0C, 0xAA][..],
                X86EvexMovntdqaMemoryEncoding {
                    destination: 17,
                    width: VecWidth::V512,
                    needs_avx512vl: false,
                },
            ),
            (
                &[0x65, 0x62, 0xEA, 0x7D, 0x28, 0x2A, 0x4A, 0x04][..],
                X86EvexMovntdqaMemoryEncoding {
                    destination: 17,
                    width: VecWidth::V256,
                    needs_avx512vl: true,
                },
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .evex_movntdqa_memory_encoding(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn reserved_register_and_nonexact_encodings_fail_closed() {
        let valid = instruction(17, 18, VecWidth::V512);
        let mut invalid = Vec::new();

        for (index, mask) in [(1usize, 7u8), (2, 3)] {
            let mut bytes = valid.clone();
            bytes[index] ^= mask;
            invalid.push(bytes);
        }
        let mut w1 = valid.clone();
        w1[2] |= 0x80;
        invalid.push(w1);
        let mut vvvv = valid.clone();
        vvvv[2] &= !0x08;
        invalid.push(vvvv);
        let mut v_prime = valid.clone();
        v_prime[3] &= !0x08;
        invalid.push(v_prime);
        let mut broadcast = valid.clone();
        broadcast[3] |= 0x10;
        invalid.push(broadcast);
        let mut mask = valid.clone();
        mask[3] |= 1;
        invalid.push(mask);
        let mut zeroing = valid.clone();
        zeroing[3] |= 0x80;
        invalid.push(zeroing);
        let mut reserved_ll = valid.clone();
        reserved_ll[3] |= 0x60;
        invalid.push(reserved_ll);
        let mut opcode = valid.clone();
        opcode[4] ^= 1;
        invalid.push(opcode);
        let mut register = valid.clone();
        register[5] |= 0xC0;
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
        invalid.push(vec![0xC4, 0xE2, 0x7D, 0x2A, 0x08]);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_movntdqa_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
