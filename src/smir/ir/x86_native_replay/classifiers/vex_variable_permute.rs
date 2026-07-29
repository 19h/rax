//! Register-only AVX/AVX2 VEX variable permutes.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth};

/// One complete variable-permute memory encoding rewritten to consume the
/// helper-loaded r/m source from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexVariablePermuteMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) scratch: u8,
    pub(crate) opcode: u8,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx2: bool,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86VexVariablePermuteMemoryEncoding {
    pub(crate) fn is_permil(self) -> bool {
        matches!(self.opcode, 0x0C | 0x0D)
    }
}

impl X86InstructionBytes {
    /// Validate one exact five-byte register-only VEX variable permute and
    /// report whether the selected form requires AVX2 rather than AVX.
    ///
    /// Intel SDM Volume 2 assigns the variable-control `VPERMILPS` and
    /// `VPERMILPD` forms to map 0F38, mandatory 66H, VEX.W=0, and opcodes
    /// 0CH/0DH. Both 128-bit and 256-bit forms require AVX. `VPERMPS` and
    /// `VPERMD` use the same map/prefix/W fields with opcodes 16H/36H, require
    /// VEX.L=1, and require AVX2. Memory forms remain excluded so replay cannot
    /// bypass guest translation or precise fault handling.
    pub fn vex_register_variable_permute_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, opcode, modrm] = bytes else {
            return None;
        };
        if p0 & 0x1F != 2 || p1 & 0x83 != 1 || modrm >> 6 != 3 {
            return None;
        }

        match opcode {
            0x0C | 0x0D => Some(false),
            0x16 | 0x36 if p1 & 0x04 != 0 => Some(true),
            _ => None,
        }
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX variable permute. The ModR/M.reg field is extended by inverted
    /// VEX.R.
    pub(crate) fn vex_variable_permute_destination_index(&self) -> Option<u8> {
        self.vex_register_variable_permute_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }

    /// Validate one complete VEX variable permute whose r/m source is memory
    /// and rewrite only that source to a borrowed low vector register.
    ///
    /// `VPERMILPS` and `VPERMILPD` use a 16- or 32-byte memory control vector
    /// and require AVX. `VPERMPS` and `VPERMD` use a 32-byte memory table,
    /// require VEX.L=1, and require AVX2. Every form uses map 0F38, mandatory
    /// 66H, and VEX.W=0. Segment and address-size prefixes are consumed by
    /// guest effective-address evaluation and omitted from the register
    /// rewrite.
    pub(crate) fn vex_variable_permute_memory_encoding(
        &self,
    ) -> Option<X86VexVariablePermuteMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 2 || fields.pp != 1 || fields.w {
            return None;
        }
        let (elem, needs_avx2) = match fields.opcode {
            0x0C => (VecElementType::F32, false),
            0x0D => (VecElementType::F64, false),
            0x16 if fields.width_256 => (VecElementType::F32, true),
            0x36 if fields.width_256 => (VecElementType::I32, true),
            _ => return None,
        };
        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination && *candidate != fields.source1)
            .expect("two operands cannot consume every low vector register");

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
            // Preserve VEX.R and the map, canonicalize the ignored X bit, and
            // encode the borrowed scratch through inverted VEX.B.
            (p0 & 0x9F) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            p1,
            fields.opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes)?;
        if register_instruction.vex_register_variable_permute_needs_avx2() != Some(needs_avx2)
            || register_instruction.vex_variable_permute_destination_index()
                != Some(fields.destination)
        {
            return None;
        }

        Some(X86VexVariablePermuteMemoryEncoding {
            width,
            elem,
            destination: fields.destination,
            source1: fields.source1,
            scratch,
            opcode: fields.opcode,
            memory_size: width.bytes(),
            needs_avx2,
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::{OpKind, SmirOp};
    use crate::smir::ir::types::{BlockId, OpId};
    use crate::smir::ir::{
        SmirBlock, x86_evex_native_replay_spans, x86_native_replay_spans,
        x86_vex_variable_blend_replay_spans, x86_vex_variable_permute_replay_spans,
    };
    use std::collections::HashMap;

    const OPCODES: [u8; 4] = [0x0C, 0x0D, 0x16, 0x36];

    fn encoding(
        extension_bits: u8,
        w: bool,
        encoded_vvvv: u8,
        l: bool,
        opcode: u8,
        modrm: u8,
    ) -> [u8; 5] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        [
            0xC4,
            extension_bits | 2,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1,
            opcode,
            modrm,
        ]
    }

    fn expected_requirement(opcode: u8, w: bool, l: bool) -> Option<bool> {
        if w {
            return None;
        }
        match opcode {
            0x0C | 0x0D => Some(false),
            0x16 | 0x36 if l => Some(true),
            _ => None,
        }
    }

    #[test]
    fn classifier_exhaustively_covers_131_072_prefix_opcode_and_register_combinations() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for opcode in OPCODES {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for w in [false, true] {
                    for encoded_vvvv in 0u8..16 {
                        for l in [false, true] {
                            for reg_rm in 0u8..=0x3F {
                                let bytes = encoding(
                                    extension_bits,
                                    w,
                                    encoded_vvvv,
                                    l,
                                    opcode,
                                    0xC0 | reg_rm,
                                );
                                let expected = expected_requirement(opcode, w, l);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_register_variable_permute_needs_avx2(),
                                    expected,
                                    "{bytes:02X?}"
                                );
                                accepted += usize::from(expected.is_some());
                                tested += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 49_152);
        assert_eq!(tested, 131_072);

        // Independently assembled by LLVM 23.
        for (bytes, needs_avx2, destination) in [
            ([0xC4, 0xE2, 0x69, 0x0C, 0xCB], false, 1),
            ([0xC4, 0x42, 0x1D, 0x0D, 0xDD], false, 11),
            ([0xC4, 0xE2, 0x6D, 0x16, 0xCB], true, 1),
            ([0xC4, 0x42, 0x15, 0x36, 0xE6], true, 12),
        ] {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_variable_permute_needs_avx2(),
                Some(needs_avx2),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_variable_permute_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_rejects_every_structural_and_reserved_frontier() {
        let canonical = encoding(0xE0, false, 0x0D, true, 0x36, 0xCA);
        let mut invalid = vec![
            canonical[..4].to_vec(),
            canonical.iter().copied().chain([0]).collect(),
            [0xC5, canonical[1], canonical[2], canonical[3], canonical[4]].to_vec(),
            [0x62, canonical[1], canonical[2], canonical[3], canonical[4]].to_vec(),
        ];
        for (index, value) in [
            (1, (canonical[1] & !0x1F) | 1),
            (1, (canonical[1] & !0x1F) | 3),
            (1, (canonical[1] & !0x1F) | 4),
            (1, canonical[1] & !0x1F),
            (2, canonical[2] & !0x03),
            (2, (canonical[2] & !0x03) | 2),
            (2, (canonical[2] & !0x03) | 3),
            (3, 0x0B),
            (3, 0x0E),
            (3, 0x15),
            (3, 0x17),
            (3, 0x35),
            (3, 0x37),
            (4, canonical[4] & 0x3F),
            (4, (canonical[4] & 0x3F) | 0x40),
            (4, (canonical[4] & 0x3F) | 0x80),
        ] {
            let mut bytes = canonical;
            bytes[index] = value;
            invalid.push(bytes.to_vec());
        }
        let mut reserved_w = canonical;
        reserved_w[2] |= 0x80;
        invalid.push(reserved_w.to_vec());
        for opcode in [0x16, 0x36] {
            invalid.push(encoding(0xE0, false, 0x0D, false, opcode, 0xCA).to_vec());
        }

        for bytes in invalid {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_variable_permute_needs_avx2(),
                None,
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_variable_permute_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
        let pc = 0xA11C;
        let instruction =
            X86InstructionBytes::new(&encoding(0x40, false, 3, true, 0x16, 0xFF)).unwrap();
        let mut block = SmirBlock::new(BlockId(47), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_vex_variable_permute_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).expect("exact VEX variable-permute span");
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
            assert!(!span.preserve_mxcsr_de);
        }
        assert!(x86_vex_variable_blend_replay_spans(&block, &provenance).is_empty());
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + 5, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }

    fn memory_encoding(
        opcode: u8,
        width_256: bool,
        destination: u8,
        source1: u8,
        base: u8,
        clear_ignored_x: bool,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        let mut bytes = vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | (if clear_ignored_x { 0 } else { 0x40 })
                | (if base < 8 { 0x20 } else { 0 })
                | 2,
            (((!source1) & 0x0F) << 3) | (u8::from(width_256) << 2) | 1,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
        ];
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(0x20);
        bytes
    }

    #[test]
    fn memory_classifier_exhaustively_covers_65_536_encoding_field_cells() {
        let mut classified = 0usize;
        let mut tested = 0usize;
        for opcode in OPCODES {
            for width_256 in [false, true] {
                for destination in 0..16u8 {
                    for source1 in 0..16u8 {
                        for base in 0..16u8 {
                            for clear_ignored_x in [false, true] {
                                let bytes = memory_encoding(
                                    opcode,
                                    width_256,
                                    destination,
                                    source1,
                                    base,
                                    clear_ignored_x,
                                );
                                let actual = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_variable_permute_memory_encoding();
                                let valid = matches!(opcode, 0x0C | 0x0D) || width_256;
                                if !valid {
                                    assert_eq!(actual, None, "{bytes:02X?}");
                                    tested += 1;
                                    continue;
                                }
                                let encoding = actual.unwrap_or_else(|| panic!("{bytes:02X?}"));
                                let width = if width_256 {
                                    VecWidth::V256
                                } else {
                                    VecWidth::V128
                                };
                                let elem = match opcode {
                                    0x0C | 0x16 => VecElementType::F32,
                                    0x0D => VecElementType::F64,
                                    0x36 => VecElementType::I32,
                                    _ => unreachable!(),
                                };
                                let scratch = (0..16u8)
                                    .find(|candidate| {
                                        *candidate != destination && *candidate != source1
                                    })
                                    .unwrap();
                                assert_eq!(encoding.width, width, "{bytes:02X?}");
                                assert_eq!(encoding.elem, elem, "{bytes:02X?}");
                                assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                                assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                                assert_eq!(encoding.memory_size, width.bytes(), "{bytes:02X?}");
                                assert_eq!(
                                    encoding.needs_avx2,
                                    matches!(opcode, 0x16 | 0x36),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    encoding.is_permil(),
                                    matches!(opcode, 0x0C | 0x0D),
                                    "{bytes:02X?}"
                                );
                                let rewritten = encoding.register_instruction.as_slice();
                                assert_eq!(rewritten[0], 0xC4, "{bytes:02X?}");
                                assert_eq!(rewritten[1] & 0x40, 0x40, "{bytes:02X?}");
                                assert_eq!(rewritten[1] & 0x20 == 0, scratch >= 8, "{bytes:02X?}");
                                assert_eq!(rewritten[2], bytes[2], "{bytes:02X?}");
                                assert_eq!(rewritten[3], opcode, "{bytes:02X?}");
                                assert_eq!(rewritten[4] >> 6, 3, "{bytes:02X?}");
                                assert_eq!(rewritten[4] & 7, scratch & 7, "{bytes:02X?}");
                                classified += 1;
                                tested += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 49_152);
        assert_eq!(tested, 65_536);
    }

    #[test]
    fn memory_classifier_matches_llvm_23_and_prefixed_address_shapes() {
        for (bytes, expected) in [
            (
                &[0xC4, 0xE2, 0x69, 0x0C, 0x4F, 0x20][..],
                (VecWidth::V128, VecElementType::F32, 1, 2, 0, false),
            ),
            (
                &[0xC4, 0x42, 0x2D, 0x0C, 0x4B, 0x20][..],
                (VecWidth::V256, VecElementType::F32, 9, 10, 0, false),
            ),
            (
                &[0xC4, 0x42, 0x01, 0x0D, 0x7E, 0x20][..],
                (VecWidth::V128, VecElementType::F64, 15, 15, 0, false),
            ),
            (
                &[0xC4, 0xE2, 0x6D, 0x0D, 0x4F, 0x20][..],
                (VecWidth::V256, VecElementType::F64, 1, 2, 0, false),
            ),
            (
                &[0xC4, 0x42, 0x2D, 0x16, 0x4B, 0x20][..],
                (VecWidth::V256, VecElementType::F32, 9, 10, 0, true),
            ),
            (
                &[0xC4, 0x42, 0x05, 0x36, 0x7E, 0x20][..],
                (VecWidth::V256, VecElementType::I32, 15, 15, 0, true),
            ),
            (
                &[
                    0x64, 0x67, 0xC4, 0x02, 0x2D, 0x16, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
                ][..],
                (VecWidth::V256, VecElementType::F32, 14, 10, 0, true),
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_variable_permute_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(
                (
                    encoding.width,
                    encoding.elem,
                    encoding.destination,
                    encoding.source1,
                    encoding.scratch,
                    encoding.needs_avx2,
                ),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn memory_classifier_fails_closed_at_every_structural_frontier() {
        let valid = memory_encoding(0x36, true, 9, 10, 11, false).to_vec();
        let mut invalid = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 3;
        invalid.push(wrong_map);
        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        invalid.push(wrong_prefix);
        let mut w1 = valid.clone();
        w1[2] |= 0x80;
        invalid.push(w1);
        let mut l0 = valid.clone();
        l0[2] &= !0x04;
        invalid.push(l0);
        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x37;
        invalid.push(wrong_opcode);
        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.remove(5);
        invalid.push(register);
        invalid.push(valid[..valid.len() - 1].to_vec());
        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);
        let mut forbidden_prefix = valid;
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_variable_permute_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
