//! Register-only AVX/AVX2 VEX 128-bit cross-lane operations.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

/// One complete cross-lane memory encoding rewritten to consume the
/// helper-loaded second source from a nonarchitectural low vector register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexCrossLane128MemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) scratch: u8,
    pub(crate) opcode: u8,
    pub(crate) immediate: u8,
    pub(crate) source_width: VecWidth,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx2: bool,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86VexCrossLane128MemoryEncoding {
    pub(crate) fn is_insert(self) -> bool {
        matches!(self.opcode, 0x18 | 0x38)
    }
}

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX 128-bit cross-lane
    /// instruction and report whether the selected form requires AVX2 rather
    /// than AVX.
    ///
    /// Intel SDM Volume 2 assigns `VPERM2F128`, `VINSERTF128`,
    /// `VINSERTI128`, and `VPERM2I128` to map 0F3A, mandatory 66H,
    /// VEX.W=0, and VEX.L=1 with opcodes 06H, 18H, 38H, and 46H,
    /// respectively. The floating forms require AVX and the integer forms
    /// require AVX2. Memory forms remain excluded so replay cannot bypass
    /// guest translation or precise fault handling.
    pub fn vex_register_cross_lane_128_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, opcode, modrm, _imm] = bytes else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x87 != 0x05 || modrm >> 6 != 3 {
            return None;
        }

        match opcode {
            0x06 | 0x18 => Some(false),
            0x38 | 0x46 => Some(true),
            _ => None,
        }
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX 128-bit cross-lane operation. The ModR/M.reg field is extended by
    /// inverted VEX.R.
    pub(crate) fn vex_cross_lane_128_destination_index(&self) -> Option<u8> {
        self.vex_register_cross_lane_128_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }

    /// Validate one complete VEX cross-lane operation whose second source is
    /// memory and rewrite only that source to a borrowed low vector register.
    ///
    /// `VINSERTF128` and `VINSERTI128` consume 16 bytes; `VPERM2F128` and
    /// `VPERM2I128` consume 32 bytes. All forms require map 0F3A, mandatory
    /// 66H, VEX.W=0, and VEX.L=1. Segment and address-size prefixes are
    /// consumed by guest effective-address evaluation and omitted from the
    /// register rewrite.
    pub(crate) fn vex_cross_lane_128_memory_encoding(
        &self,
    ) -> Option<X86VexCrossLane128MemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || fields.w || !fields.width_256 {
            return None;
        }
        let (source_width, needs_avx2) = match fields.opcode {
            0x06 => (VecWidth::V256, false),
            0x18 => (VecWidth::V128, false),
            0x38 => (VecWidth::V128, true),
            0x46 => (VecWidth::V256, true),
            _ => return None,
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
            immediate,
        ];
        let register_instruction = X86InstructionBytes::new(&register_bytes)?;
        if register_instruction.vex_register_cross_lane_128_needs_avx2() != Some(needs_avx2)
            || register_instruction.vex_cross_lane_128_destination_index()
                != Some(fields.destination)
        {
            return None;
        }

        Some(X86VexCrossLane128MemoryEncoding {
            destination: fields.destination,
            source1: fields.source1,
            scratch,
            opcode: fields.opcode,
            immediate,
            source_width,
            memory_size: source_width.bytes(),
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
        x86_vex_cross_lane_128_replay_spans, x86_vex_variable_permute_replay_spans,
    };
    use std::collections::HashMap;

    const OPCODES: [u8; 4] = [0x06, 0x18, 0x38, 0x46];

    fn encoding(
        extension_bits: u8,
        w: bool,
        encoded_vvvv: u8,
        l: bool,
        opcode: u8,
        modrm: u8,
        imm: u8,
    ) -> [u8; 6] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        [
            0xC4,
            extension_bits | 3,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1,
            opcode,
            modrm,
            imm,
        ]
    }

    fn expected_requirement(opcode: u8, w: bool, l: bool) -> Option<bool> {
        if w || !l {
            return None;
        }
        match opcode {
            0x06 | 0x18 => Some(false),
            0x38 | 0x46 => Some(true),
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
                                    0xA5,
                                );
                                let expected = expected_requirement(opcode, w, l);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_register_cross_lane_128_needs_avx2(),
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
        assert_eq!(accepted, 32_768);
        assert_eq!(tested, 131_072);

        // Independently assembled by LLVM 23.
        for (bytes, needs_avx2, destination) in [
            ([0xC4, 0xE3, 0x6D, 0x06, 0xCB, 0x31], false, 1),
            ([0xC4, 0x43, 0x15, 0x18, 0xE6, 0x01], false, 12),
            ([0xC4, 0x43, 0x3D, 0x38, 0xF9, 0x00], true, 15),
            ([0xC4, 0x43, 0x2D, 0x46, 0xCB, 0x82], true, 9),
        ] {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_cross_lane_128_needs_avx2(),
                Some(needs_avx2),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_cross_lane_128_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_accepts_all_256_immediates_and_ignores_only_vex_x() {
        for opcode in OPCODES {
            for imm in u8::MIN..=u8::MAX {
                for extension_bits in [0xE0, 0xA0] {
                    let bytes = encoding(extension_bits, false, 0x0D, true, opcode, 0xCA, imm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_register_cross_lane_128_needs_avx2(),
                        Some(matches!(opcode, 0x38 | 0x46)),
                        "{bytes:02X?}"
                    );
                }
            }
        }
    }

    #[test]
    fn classifier_rejects_every_structural_and_reserved_frontier() {
        let canonical = encoding(0xE0, false, 0x0D, true, 0x46, 0xCA, 0xA5);
        let mut invalid = vec![
            canonical[..5].to_vec(),
            canonical.iter().copied().chain([0]).collect(),
            [
                0xC5,
                canonical[1],
                canonical[2],
                canonical[3],
                canonical[4],
                canonical[5],
            ]
            .to_vec(),
            [
                0x62,
                canonical[1],
                canonical[2],
                canonical[3],
                canonical[4],
                canonical[5],
            ]
            .to_vec(),
        ];
        for (index, value) in [
            (1, (canonical[1] & !0x1F) | 1),
            (1, (canonical[1] & !0x1F) | 2),
            (1, (canonical[1] & !0x1F) | 4),
            (1, canonical[1] & !0x1F),
            (2, canonical[2] & !0x03),
            (2, (canonical[2] & !0x03) | 2),
            (2, (canonical[2] & !0x03) | 3),
            (3, 0x05),
            (3, 0x07),
            (3, 0x17),
            (3, 0x19),
            (3, 0x37),
            (3, 0x39),
            (3, 0x45),
            (3, 0x47),
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
        let mut reserved_l = canonical;
        reserved_l[2] &= !0x04;
        invalid.push(reserved_l.to_vec());

        for bytes in invalid {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_cross_lane_128_needs_avx2(),
                None,
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_cross_lane_128_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
        let pc = 0xA11C;
        let instruction =
            X86InstructionBytes::new(&encoding(0x40, false, 3, true, 0x46, 0xFF, 0x82)).unwrap();
        let mut block = SmirBlock::new(BlockId(48), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_vex_cross_lane_128_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).expect("exact VEX cross-lane span");
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
            assert!(!span.preserve_mxcsr_de);
        }
        assert!(x86_vex_variable_permute_replay_spans(&block, &provenance).is_empty());
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }

    fn memory_encoding(
        opcode: u8,
        destination: u8,
        source1: u8,
        base: u8,
        immediate: u8,
        clear_ignored_x: bool,
    ) -> [u8; 7] {
        assert!(destination < 16 && source1 < 16 && base < 16);
        [
            0xC4,
            (if destination < 8 { 0x80 } else { 0 })
                | (if clear_ignored_x { 0 } else { 0x40 })
                | (if base < 8 { 0x20 } else { 0 })
                | 3,
            (((!source1) & 0x0F) << 3) | 0x05,
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    #[test]
    fn memory_classifier_exhaustively_covers_262_144_operand_and_immediate_cells() {
        let mut classified = 0usize;
        for opcode in OPCODES {
            for destination in 0..16u8 {
                for source1 in 0..16u8 {
                    for immediate in u8::MIN..=u8::MAX {
                        let base = if immediate & 1 == 0 { 3 } else { 11 };
                        let bytes = memory_encoding(
                            opcode,
                            destination,
                            source1,
                            base,
                            immediate,
                            immediate & 2 != 0,
                        );
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_cross_lane_128_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        let source_width = if matches!(opcode, 0x18 | 0x38) {
                            VecWidth::V128
                        } else {
                            VecWidth::V256
                        };
                        let scratch = (0..16u8)
                            .find(|candidate| *candidate != destination && *candidate != source1)
                            .unwrap();
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                        assert_eq!(encoding.opcode, opcode, "{bytes:02X?}");
                        assert_eq!(encoding.immediate, immediate, "{bytes:02X?}");
                        assert_eq!(encoding.source_width, source_width, "{bytes:02X?}");
                        assert_eq!(encoding.memory_size, source_width.bytes(), "{bytes:02X?}");
                        assert_eq!(
                            encoding.needs_avx2,
                            matches!(opcode, 0x38 | 0x46),
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            encoding.is_insert(),
                            matches!(opcode, 0x18 | 0x38),
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
                        assert_eq!(rewritten[5], immediate, "{bytes:02X?}");
                        classified += 1;
                    }
                }
            }
        }
        assert_eq!(classified, 4 * 16 * 16 * 256);
    }

    #[test]
    fn memory_classifier_matches_llvm_23_examples_and_feature_width_split() {
        for (bytes, expected) in [
            (
                &[0xC4, 0xE3, 0x6D, 0x06, 0x4F, 0x20, 0x31][..],
                (1, 2, 0, 0x06, 0x31, VecWidth::V256, false),
            ),
            (
                &[0xC4, 0x43, 0x2D, 0x18, 0x4B, 0x20, 0xFF][..],
                (9, 10, 0, 0x18, 0xFF, VecWidth::V128, false),
            ),
            (
                &[0xC4, 0x43, 0x05, 0x38, 0x7E, 0x20, 0xA4][..],
                (15, 15, 0, 0x38, 0xA4, VecWidth::V128, true),
            ),
            (
                &[0xC4, 0x43, 0x2D, 0x46, 0x4B, 0x20, 0x82][..],
                (9, 10, 0, 0x46, 0x82, VecWidth::V256, true),
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_cross_lane_128_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(
                (
                    encoding.destination,
                    encoding.source1,
                    encoding.scratch,
                    encoding.opcode,
                    encoding.immediate,
                    encoding.source_width,
                    encoding.needs_avx2,
                ),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn memory_classifier_fails_closed_at_every_structural_frontier() {
        let valid = memory_encoding(0x46, 9, 10, 11, 0x82, false).to_vec();
        let mut invalid = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
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
        wrong_opcode[3] = 0x47;
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
                    .vex_cross_lane_128_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
