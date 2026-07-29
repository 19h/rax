//! AVX/AVX2 VEX `VPALIGNR` replay classifiers.

use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

impl X86InstructionBytes {
    /// Validate one exact six-byte register-only VEX VPALIGNR instruction and
    /// report whether its vector length requires AVX2 rather than AVX.
    ///
    /// Intel SDM Volume 2 assigns VPALIGNR to map 0F3A, mandatory 66H, WIG,
    /// opcode 0FH. VEX.128 requires AVX and VEX.256 requires AVX2. Every imm8
    /// value is architectural. Memory forms remain excluded so replay cannot
    /// bypass guest translation or precise fault handling.
    pub fn vex_register_alignr_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, 0x0F, modrm, _imm] = bytes else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }
        Some(p1 & 0x04 != 0)
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX VPALIGNR. The ModR/M.reg field is extended by inverted VEX.R.
    pub(crate) fn vex_alignr_destination_index(&self) -> Option<u8> {
        self.vex_register_alignr_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }

    /// Validate one complete VEX `VPALIGNR` instruction whose second source
    /// is memory and return `(destination, first source, width, immediate, W)`.
    ///
    /// Intel SDM Volume 2 assigns `VPALIGNR` to map 0F3A, mandatory prefix
    /// 66H, opcode 0FH, with 128- and 256-bit vector lengths. VEX.W is ignored
    /// and retained so native replay can preserve either accepted encoding.
    /// The shared parser validates the complete ModR/M/SIB/displacement plus
    /// imm8 shape and permits only segment/address-size legacy prefixes.
    /// Runtime and auxiliary space are O(1) because x86 instructions are at
    /// most 15 bytes.
    pub(crate) fn vex_memory_alignr_fields(&self) -> Option<(u8, u8, VecWidth, u8, bool)> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || fields.opcode != 0x0F {
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
            immediate,
            fields.w,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::{OpKind, SmirOp};
    use crate::smir::ir::types::{BlockId, OpId};
    use crate::smir::ir::{
        SmirBlock, x86_evex_native_replay_spans, x86_native_replay_spans,
        x86_vex_alignr_replay_spans, x86_vex_cross_lane_128_replay_spans,
    };
    use std::collections::HashMap;

    fn encoding(
        extension_bits: u8,
        w: bool,
        encoded_vvvv: u8,
        l: bool,
        modrm: u8,
        imm: u8,
    ) -> [u8; 6] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        [
            0xC4,
            extension_bits | 3,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1,
            0x0F,
            modrm,
            imm,
        ]
    }

    fn memory_encoding(
        destination: u8,
        source1: u8,
        base: u8,
        width: VecWidth,
        immediate: u8,
        w: bool,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16 && base < 16);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7)
                | (((!source1) & 0x0F) << 3)
                | (u8::from(width == VecWidth::V256) << 2)
                | 1,
            0x0F,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    #[test]
    fn memory_classifier_covers_all_register_width_w_and_immediate_cells() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for width in [VecWidth::V128, VecWidth::V256] {
                    for w in [false, true] {
                        for immediate in u8::MIN..=u8::MAX {
                            let base = if immediate & 1 == 0 { 3 } else { 11 };
                            let bytes =
                                memory_encoding(destination, source1, base, width, immediate, w);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_alignr_fields(),
                                Some((destination, source1, width, immediate, w)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2 * 2 * 256);
    }

    #[test]
    fn memory_classifier_accepts_complete_prefixed_sib_displacement_shape() {
        // FS addr32: VPALIGNR ymm14,ymm9,[r14d+r15d*2+0x44332211],0xA5.
        let bytes = [
            0x64, 0x67, 0xC4, 0x03, 0xB5, 0x0F, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xA5,
        ];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_alignr_fields(),
            Some((14, 9, VecWidth::V256, 0xA5, true))
        );
    }

    #[test]
    fn memory_classifier_rejects_every_structural_frontier() {
        let valid = memory_encoding(3, 9, 11, VecWidth::V128, 0xA5, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x0E;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.remove(5);
        cases.push(register_source);

        let mut missing_immediate = valid.clone();
        missing_immediate.pop();
        cases.push(missing_immediate);

        let mut truncated_displacement = valid.clone();
        truncated_displacement.remove(5);
        cases.push(truncated_displacement);

        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut forbidden_prefix = valid;
        forbidden_prefix.insert(0, 0x66);
        cases.push(forbidden_prefix);

        for bytes in cases {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_memory_alignr_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_exhaustively_covers_32_768_prefix_and_register_combinations() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for extension_bits in (0u8..8).map(|value| value << 5) {
            for w in [false, true] {
                for encoded_vvvv in 0u8..16 {
                    for l in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            let bytes =
                                encoding(extension_bits, w, encoded_vvvv, l, 0xC0 | reg_rm, 0xA5);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            assert_eq!(
                                instruction.vex_register_alignr_needs_avx2(),
                                Some(l),
                                "{bytes:02X?}"
                            );
                            let destination_extension = u8::from(extension_bits & 0x80 == 0) << 3;
                            assert_eq!(
                                instruction.vex_alignr_destination_index(),
                                Some(destination_extension | ((reg_rm >> 3) & 7)),
                                "{bytes:02X?}"
                            );
                            accepted += 1;
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 32_768);
        assert_eq!(tested, 32_768);

        // Independently assembled by LLVM 23.
        for (bytes, needs_avx2, destination) in [
            ([0xC4, 0xE3, 0x69, 0x0F, 0xCB, 0xA5], false, 1),
            ([0xC4, 0x43, 0x2D, 0x0F, 0xCB, 0x5A], true, 9),
            ([0xC4, 0x43, 0x0D, 0x0F, 0xEF, 0x1F], true, 13),
        ] {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_alignr_needs_avx2(),
                Some(needs_avx2),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_alignr_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_accepts_all_256_immediates_and_ignores_vex_w_and_x() {
        for imm in u8::MIN..=u8::MAX {
            for extension_bits in [0xE0, 0xA0] {
                for w in [false, true] {
                    for l in [false, true] {
                        let bytes = encoding(extension_bits, w, 0x0D, l, 0xCA, imm);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_register_alignr_needs_avx2(),
                            Some(l),
                            "{bytes:02X?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn classifier_rejects_every_structural_frontier() {
        let canonical = encoding(0xE0, false, 0x0D, true, 0xCA, 0xA5);
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
            (1, canonical[1] & !0x1F),
            (1, (canonical[1] & !0x1F) | 1),
            (1, (canonical[1] & !0x1F) | 2),
            (1, (canonical[1] & !0x1F) | 4),
            (1, (canonical[1] & !0x1F) | 0x1F),
            (2, canonical[2] & !0x03),
            (2, (canonical[2] & !0x03) | 2),
            (2, (canonical[2] & !0x03) | 3),
            (3, 0x0E),
            (3, 0x10),
            (3, 0x40),
            (4, canonical[4] & 0x3F),
            (4, (canonical[4] & 0x3F) | 0x40),
            (4, (canonical[4] & 0x3F) | 0x80),
        ] {
            let mut bytes = canonical;
            bytes[index] = value;
            invalid.push(bytes.to_vec());
        }

        for bytes in invalid {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_alignr_needs_avx2(),
                None,
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_alignr_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
        let pc = 0xA119;
        let instruction =
            X86InstructionBytes::new(&encoding(0x40, true, 3, true, 0xFF, 0x82)).unwrap();
        let mut block = SmirBlock::new(BlockId(49), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_vex_alignr_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).expect("exact VEX VPALIGNR span");
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
            assert!(!span.preserve_mxcsr_de);
        }
        assert!(x86_vex_cross_lane_128_replay_spans(&block, &provenance).is_empty());
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }
}
