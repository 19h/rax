//! VEX and EVEX GFNI replay classification.

use super::super::{X86VexGfniMemoryEncoding, X86VexGfniMemoryKind};
use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

impl X86InstructionBytes {
    /// Validate one register-only VEX GFNI vector instruction and return
    /// whether it has a 256-bit YMM destination.
    ///
    /// The admitted set is exactly VGF2P8MULB, VGF2P8AFFINEQB, and
    /// VGF2P8AFFINEINVQB. All forms require AVX and GFNI. VEX.X is ignored for
    /// register sources. Memory operands and every malformed map, mandatory
    /// prefix, W, opcode, length, or trailing-byte form fail closed.
    pub fn vex_register_gfni_uses_ymm(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 5 | 6) || bytes[0] != 0xC4 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];
        if p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x1F;
        let w = p1 & 0x80 != 0;
        match (map, opcode, w, bytes.len()) {
            (2, 0xCF, false, 5) => {}
            (3, 0xCE | 0xCF, true, 6) => {}
            _ => return None,
        }
        Some(p1 & 0x04 != 0)
    }

    /// Architectural XMM/YMM destination selected by an exact register-only
    /// VEX GFNI instruction.
    pub(crate) fn vex_gfni_destination_index(&self) -> Option<u8> {
        self.vex_register_gfni_uses_ymm()?;
        let [0xC4, p0, _p1, _opcode, modrm, ..] = self.as_slice() else {
            unreachable!()
        };
        Some((u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7))
    }

    /// Architectural first source selected by an exact register-only VEX GFNI
    /// instruction.
    pub(crate) fn vex_gfni_source1_index(&self) -> Option<u8> {
        self.vex_register_gfni_uses_ymm()?;
        let [0xC4, _p0, p1, _opcode, _modrm, ..] = self.as_slice() else {
            unreachable!()
        };
        Some((!p1 >> 3) & 0x0F)
    }

    /// Architectural second source selected by an exact register-only VEX
    /// GFNI instruction.
    pub(crate) fn vex_gfni_source2_index(&self) -> Option<u8> {
        self.vex_register_gfni_uses_ymm()?;
        let [0xC4, p0, _p1, _opcode, modrm, ..] = self.as_slice() else {
            unreachable!()
        };
        Some((u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7))
    }

    /// Validate one VEX GFNI memory encoding and rewrite it to an exact
    /// register-source instruction using a low scratch register distinct from
    /// both architectural register operands.
    pub(crate) fn vex_gfni_memory_encoding(&self) -> Option<X86VexGfniMemoryEncoding> {
        let (fields, kind, immediate, memory_instruction) =
            if let Some(fields) = self.vex_memory_fields() {
                if fields.map != 2 || fields.pp != 1 || fields.opcode != 0xCF || fields.w {
                    return None;
                }
                (fields, X86VexGfniMemoryKind::Multiply, None, *self)
            } else {
                let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
                let kind = match (fields.map, fields.pp, fields.opcode, fields.w) {
                    (3, 1, 0xCE, true) => X86VexGfniMemoryKind::Affine,
                    (3, 1, 0xCF, true) => X86VexGfniMemoryKind::AffineInverse,
                    _ => return None,
                };
                let bytes_without_immediate =
                    X86InstructionBytes::new(&self.as_slice()[..self.as_slice().len() - 1])?;
                (fields, kind, Some(immediate), bytes_without_immediate)
            };

        let width = if fields.width_256 {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination && *candidate != fields.source1)
            .expect("two operands cannot consume every low vector register");
        let rewritten = memory_instruction.vex_memory_with_register_source(scratch)?;
        let register_instruction = if let Some(immediate) = immediate {
            let mut bytes = [0u8; 15];
            let rewritten_bytes = rewritten.as_slice();
            let len = rewritten_bytes.len().checked_add(1)?;
            if len > bytes.len() {
                return None;
            }
            bytes[..rewritten_bytes.len()].copy_from_slice(rewritten_bytes);
            bytes[rewritten_bytes.len()] = immediate;
            X86InstructionBytes::new(&bytes[..len])?
        } else {
            rewritten
        };

        if register_instruction.vex_register_gfni_uses_ymm() != Some(width == VecWidth::V256)
            || register_instruction.vex_gfni_destination_index() != Some(fields.destination)
            || register_instruction.vex_gfni_source1_index() != Some(fields.source1)
            || register_instruction.vex_gfni_source2_index() != Some(scratch)
        {
            return None;
        }

        Some(X86VexGfniMemoryEncoding {
            kind,
            width,
            destination: fields.destination,
            source1: fields.source1,
            scratch,
            immediate,
            register_instruction,
        })
    }

    /// Validate one register-only EVEX GFNI vector instruction and return
    /// whether its vector length requires AVX-512VL in addition to AVX-512F
    /// and GFNI.
    ///
    /// The admitted set is exactly VGF2P8MULB, VGF2P8AFFINEQB, and
    /// VGF2P8AFFINEINVQB. Memory sources, EVEX.b, reserved vector lengths,
    /// malformed masks, incorrect W/pp/map/opcode combinations, and incomplete
    /// or trailing instruction bytes fail closed.
    pub fn evex_register_gfni_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0F;
        let w = p1 & 0x80 != 0;
        match (map, opcode, w, bytes.len()) {
            (2, 0xCF, false, 6) => {}
            (3, 0xCE | 0xCF, true, 7) => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || ll == 3 || (zeroing && mask == 0) {
            return None;
        }
        Some(ll != 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::{OpKind, SmirOp};
    use crate::smir::ir::types::{BlockId, OpId};
    use crate::smir::ir::{
        SmirBlock, x86_evex_gfni_replay_spans, x86_native_replay_spans, x86_vex_gfni_replay_spans,
    };
    use std::collections::HashMap;

    const VEX_SHAPES: [(u8, u8, bool, bool); 3] = [
        (2, 0xCF, false, false),
        (3, 0xCE, true, true),
        (3, 0xCF, true, true),
    ];

    fn encoding(
        shape: (u8, u8, bool, bool),
        extension_bits: u8,
        encoded_vvvv: u8,
        ymm: bool,
        modrm: u8,
        immediate: u8,
    ) -> Vec<u8> {
        let (map, opcode, w, has_immediate) = shape;
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        let mut bytes = vec![
            0xC4,
            extension_bits | map,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(ymm) << 2) | 1,
            opcode,
            modrm,
        ];
        if has_immediate {
            bytes.push(immediate);
        }
        bytes
    }

    #[test]
    fn vex_classifier_exhaustively_covers_196_608_extension_vvvv_l_and_modrm_shapes() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for shape in VEX_SHAPES {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        for modrm in u8::MIN..=u8::MAX {
                            let bytes =
                                encoding(shape, extension_bits, encoded_vvvv, ymm, modrm, 0xA5);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let expected = (modrm >> 6 == 3).then_some(ymm);
                            assert_eq!(
                                instruction.vex_register_gfni_uses_ymm(),
                                expected,
                                "{bytes:02X?}"
                            );
                            let destination_extension = u8::from(extension_bits & 0x80 == 0) << 3;
                            assert_eq!(
                                instruction.vex_gfni_destination_index(),
                                expected.map(|_| destination_extension | ((modrm >> 3) & 7)),
                                "{bytes:02X?}"
                            );
                            assert_eq!(
                                instruction.vex_gfni_source1_index(),
                                expected.map(|_| (!encoded_vvvv) & 0x0F),
                                "{bytes:02X?}"
                            );
                            let source2_extension = u8::from(extension_bits & 0x20 == 0) << 3;
                            assert_eq!(
                                instruction.vex_gfni_source2_index(),
                                expected.map(|_| source2_extension | (modrm & 7)),
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected.is_some());
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 49_152);
        assert_eq!(tested, 196_608);
    }

    #[test]
    fn vex_classifier_exhaustively_rejects_wrong_map_pp_opcode_w_and_length() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for map in 0u8..32 {
            for pp in 0u8..4 {
                for opcode in u8::MIN..=u8::MAX {
                    for w in [false, true] {
                        for ymm in [false, true] {
                            for has_immediate in [false, true] {
                                let mut bytes = vec![
                                    0xC4,
                                    0xE0 | map,
                                    (u8::from(w) << 7) | (0x0D << 3) | (u8::from(ymm) << 2) | pp,
                                    opcode,
                                    0xCA,
                                ];
                                if has_immediate {
                                    bytes.push(0xA5);
                                }
                                let multiply = map == 2 && opcode == 0xCF && !w && !has_immediate;
                                let affine =
                                    map == 3 && matches!(opcode, 0xCE | 0xCF) && w && has_immediate;
                                let expected = pp == 1 && (multiply || affine);
                                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    instruction.vex_register_gfni_uses_ymm(),
                                    expected.then_some(ymm),
                                    "{bytes:02X?}"
                                );
                                accepted += usize::from(expected);
                                tested += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 6);
        assert_eq!(tested, 262_144);
    }

    #[test]
    fn vex_classifier_accepts_all_affine_immediates_and_ignored_x() {
        for shape in [VEX_SHAPES[1], VEX_SHAPES[2]] {
            for immediate in u8::MIN..=u8::MAX {
                for extension_bits in [0xE0, 0xA0] {
                    for ymm in [false, true] {
                        let bytes = encoding(shape, extension_bits, 0x0D, ymm, 0xCA, immediate);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_register_gfni_uses_ymm(),
                            Some(ymm),
                            "{bytes:02X?}"
                        );
                    }
                }
            }
        }
        for extension_bits in [0xE0, 0xA0] {
            for ymm in [false, true] {
                let bytes = encoding(VEX_SHAPES[0], extension_bits, 0x0D, ymm, 0xCA, 0xA5);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_register_gfni_uses_ymm(),
                    Some(ymm),
                    "{bytes:02X?}"
                );
            }
        }

        // Independently assembled by LLVM 23.
        for (bytes, ymm, destination) in [
            (&[0xC4, 0x42, 0x29, 0xCF, 0xCB][..], false, 9),
            (&[0xC4, 0x42, 0x0D, 0xCF, 0xEF][..], true, 13),
            (&[0xC4, 0x43, 0xA9, 0xCE, 0xCB, 0x63][..], false, 9),
            (&[0xC4, 0x43, 0x8D, 0xCF, 0xEF, 0xA5][..], true, 13),
        ] {
            let instruction = X86InstructionBytes::new(bytes).unwrap();
            assert_eq!(
                instruction.vex_register_gfni_uses_ymm(),
                Some(ymm),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_gfni_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
            assert!(
                instruction.vex_gfni_source1_index().is_some(),
                "{bytes:02X?}"
            );
            assert!(
                instruction.vex_gfni_source2_index().is_some(),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn vex_classifier_rejects_memory_non_vex_incomplete_and_trailing_forms() {
        let multiply = encoding(VEX_SHAPES[0], 0xE0, 0x0D, true, 0xCA, 0xA5);
        let affine = encoding(VEX_SHAPES[1], 0xE0, 0x0D, true, 0xCA, 0xA5);
        let mut memory = affine.clone();
        memory[4] &= 0x3F;
        for bytes in [
            affine[..5].to_vec(),
            affine.iter().copied().chain([0]).collect(),
            multiply.iter().copied().chain([0]).collect(),
            [0x62, affine[1], affine[2], affine[3], affine[4], affine[5]].to_vec(),
            [0xC5, affine[2], affine[3], affine[4], affine[5]].to_vec(),
            memory,
        ] {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_gfni_uses_ymm(),
                None,
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_gfni_destination_index(),
                None,
                "{bytes:02X?}"
            );
            assert_eq!(instruction.vex_gfni_source1_index(), None, "{bytes:02X?}");
            assert_eq!(instruction.vex_gfni_source2_index(), None, "{bytes:02X?}");
        }
    }

    fn memory_encoding(
        kind: X86VexGfniMemoryKind,
        destination: u8,
        source1: u8,
        ymm: bool,
        modrm_mode_rm: u8,
        operand_tail: &[u8],
        immediate: u8,
    ) -> Vec<u8> {
        assert!(destination < 16 && source1 < 16);
        assert!(modrm_mode_rm & 0x38 == 0);
        let (map, w, opcode, has_immediate) = match kind {
            X86VexGfniMemoryKind::Multiply => (2, false, 0xCF, false),
            X86VexGfniMemoryKind::Affine => (3, true, 0xCE, true),
            X86VexGfniMemoryKind::AffineInverse => (3, true, 0xCF, true),
        };
        let mut p0 = 0xE0 | map;
        if destination >= 8 {
            p0 &= !0x80;
        }
        let mut bytes = vec![
            0xC4,
            p0,
            (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (u8::from(ymm) << 2) | 1,
            opcode,
            modrm_mode_rm | ((destination & 7) << 3),
        ];
        bytes.extend_from_slice(operand_tail);
        if has_immediate {
            bytes.push(immediate);
        }
        bytes
    }

    #[test]
    fn memory_classifier_exhaustively_covers_all_1_536_kind_width_register_and_immediate_cells() {
        let kinds = [
            X86VexGfniMemoryKind::Multiply,
            X86VexGfniMemoryKind::Affine,
            X86VexGfniMemoryKind::AffineInverse,
        ];
        let mut classified = 0usize;
        for kind in kinds {
            for ymm in [false, true] {
                for destination in 0u8..16 {
                    for source1 in 0u8..16 {
                        let immediate = destination
                            .wrapping_mul(17)
                            .wrapping_add(source1.wrapping_mul(29));
                        let bytes =
                            memory_encoding(kind, destination, source1, ymm, 0x03, &[], immediate);
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_gfni_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(encoding.kind, kind, "{bytes:02X?}");
                        assert_eq!(
                            encoding.width,
                            if ymm { VecWidth::V256 } else { VecWidth::V128 },
                            "{bytes:02X?}"
                        );
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_ne!(encoding.scratch, destination, "{bytes:02X?}");
                        assert_ne!(encoding.scratch, source1, "{bytes:02X?}");
                        assert_eq!(
                            encoding.immediate,
                            (kind != X86VexGfniMemoryKind::Multiply).then_some(immediate),
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            encoding.register_instruction.vex_register_gfni_uses_ymm(),
                            Some(ymm),
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            encoding.register_instruction.vex_gfni_destination_index(),
                            Some(destination),
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            encoding.register_instruction.vex_gfni_source1_index(),
                            Some(source1),
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            encoding.register_instruction.vex_gfni_source2_index(),
                            Some(encoding.scratch),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
        assert_eq!(classified, 3 * 2 * 16 * 16);
    }

    #[test]
    fn memory_classifier_accepts_all_modrm_sib_displacements_prefixes_and_affine_immediates() {
        let operands: [(u8, &[u8]); 8] = [
            (0x00, &[]),
            (0x04, &[0x20]),
            (0x04, &[0x25, 0x11, 0x22, 0x33, 0x44]),
            (0x05, &[0x11, 0x22, 0x33, 0x44]),
            (0x40, &[0x80]),
            (0x44, &[0xA5, 0x80]),
            (0x80, &[0x11, 0x22, 0x33, 0x44]),
            (0x84, &[0xE3, 0x11, 0x22, 0x33, 0x44]),
        ];
        let mut classified = 0usize;
        for kind in [
            X86VexGfniMemoryKind::Multiply,
            X86VexGfniMemoryKind::Affine,
            X86VexGfniMemoryKind::AffineInverse,
        ] {
            for (mode_rm, tail) in operands {
                for immediate in u8::MIN..=u8::MAX {
                    let mut bytes = memory_encoding(kind, 13, 14, true, mode_rm, tail, immediate);
                    bytes.splice(0..0, [0x64, 0x67]);
                    let encoding = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_gfni_memory_encoding()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(encoding.kind, kind, "{bytes:02X?}");
                    assert_eq!(encoding.destination, 13, "{bytes:02X?}");
                    assert_eq!(encoding.source1, 14, "{bytes:02X?}");
                    assert_eq!(encoding.scratch, 0, "{bytes:02X?}");
                    assert_eq!(
                        encoding.immediate,
                        (kind != X86VexGfniMemoryKind::Multiply).then_some(immediate),
                        "{bytes:02X?}"
                    );
                    let expected = match kind {
                        X86VexGfniMemoryKind::Multiply => {
                            vec![0xC4, 0x62, 0x0D, 0xCF, 0xE8]
                        }
                        X86VexGfniMemoryKind::Affine => {
                            vec![0xC4, 0x63, 0x8D, 0xCE, 0xE8, immediate]
                        }
                        X86VexGfniMemoryKind::AffineInverse => {
                            vec![0xC4, 0x63, 0x8D, 0xCF, 0xE8, immediate]
                        }
                    };
                    assert_eq!(
                        encoding.register_instruction.as_slice(),
                        expected,
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
        assert_eq!(classified, 3 * operands.len() * 256);
    }

    #[test]
    fn memory_classifier_fails_closed_for_register_wrong_fields_and_malformed_lengths() {
        let valid = memory_encoding(
            X86VexGfniMemoryKind::AffineInverse,
            9,
            10,
            true,
            0x43,
            &[0x20],
            0xA5,
        );
        let mut candidates = Vec::new();
        for (index, mask) in [(1, 0x1F), (2, 0x03), (2, 0x80), (3, 0xFF)] {
            let mut bytes = valid.clone();
            bytes[index] ^= mask;
            candidates.push(bytes);
        }
        let mut register = valid.clone();
        register[4] |= 0xC0;
        candidates.push(register);
        candidates.push(valid[..valid.len() - 1].to_vec());
        candidates.push(valid.iter().copied().chain([0]).collect());
        candidates.push(
            [0x62]
                .into_iter()
                .chain(valid[1..].iter().copied())
                .collect(),
        );
        candidates.push(
            [0xC5]
                .into_iter()
                .chain(valid[2..].iter().copied())
                .collect(),
        );

        for bytes in candidates {
            let Some(instruction) = X86InstructionBytes::new(&bytes) else {
                continue;
            };
            assert_eq!(instruction.vex_gfni_memory_encoding(), None, "{bytes:02X?}");
        }
    }

    #[test]
    fn vex_dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
        let pc = 0x6F10;
        let bytes = encoding(VEX_SHAPES[2], 0x40, 3, true, 0xFF, 0x82);
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let mut block = SmirBlock::new(BlockId(52), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_vex_gfni_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).expect("exact VEX GFNI span");
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
            assert!(!span.preserve_mxcsr_de);
        }
        assert!(x86_evex_gfni_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + bytes.len() as u64, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }
}
