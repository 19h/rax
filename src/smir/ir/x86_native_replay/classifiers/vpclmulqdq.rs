//! VEX/EVEX VPCLMULQDQ replay classification.

use super::super::X86VpclmulqdqMemoryEncoding;
use super::X86InstructionBytes;
use crate::smir::ir::types::VecWidth;

fn memory_operand_end(bytes: &[u8], modrm_index: usize) -> Option<usize> {
    let modrm = *bytes.get(modrm_index)?;
    let mode = modrm >> 6;
    let rm = modrm & 7;
    if mode == 3 {
        return None;
    }

    let mut end = modrm_index + 1;
    let sib_base = if rm == 4 {
        let sib = *bytes.get(end)?;
        end += 1;
        Some(sib & 7)
    } else {
        None
    };
    let displacement = match mode {
        0 if rm == 5 || sib_base == Some(5) => 4,
        0 => 0,
        1 => 1,
        2 => 4,
        _ => unreachable!("register mode rejected"),
    };
    end.checked_add(displacement)
        .filter(|operand_end| *operand_end <= bytes.len())
}

fn vector_legacy_prefix_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
        .count()
}

impl X86InstructionBytes {
    /// Validate one register-only VEX VPCLMULQDQ instruction and return
    /// whether it has a 256-bit YMM destination.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. VEX.128 additionally
    /// requires PCLMULQDQ, whereas VEX.256 requires VPCLMULQDQ. W and VEX.X
    /// are ignored. Memory sources and every malformed map, mandatory prefix,
    /// opcode, length, or trailing-byte form fail closed.
    pub fn vex_register_vpclmulqdq_uses_ymm(&self) -> Option<bool> {
        let [0xC4, p0, p1, 0x44, modrm, _imm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x1F != 3 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }
        Some(p1 & 0x04 != 0)
    }

    /// Architectural XMM/YMM destination selected by an exact register-only
    /// VEX VPCLMULQDQ instruction.
    pub(crate) fn vex_vpclmulqdq_destination_index(&self) -> Option<u8> {
        self.vex_register_vpclmulqdq_uses_ymm()?;
        let [0xC4, p0, _p1, 0x44, modrm, _imm] = self.as_slice() else {
            unreachable!()
        };
        Some((u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7))
    }

    /// Validate one register-only EVEX VPCLMULQDQ instruction and return
    /// whether its vector length requires AVX-512VL in addition to AVX-512F
    /// and VPCLMULQDQ.
    ///
    /// W is ignored architecturally and therefore both values are admitted.
    /// Memory sources, masking, zeroing, EVEX.b, reserved vector lengths,
    /// incorrect pp/map/opcode combinations, and incomplete or trailing bytes
    /// fail closed.
    pub fn evex_register_vpclmulqdq_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 3 || p1 & 0x04 == 0 || p1 & 0x03 != 1 || opcode != 0x44 || modrm >> 6 != 3 {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        if embedded_control || zeroing || mask != 0 || ll == 3 {
            return None;
        }
        Some(ll != 2)
    }

    /// Validate one unmasked VEX/EVEX VPCLMULQDQ memory encoding and rewrite
    /// it to an exact register-source instruction using a low scratch register
    /// distinct from both architectural register operands.
    pub(crate) fn vpclmulqdq_memory_encoding(&self) -> Option<X86VpclmulqdqMemoryEncoding> {
        let bytes = self.as_slice();
        let start = vector_legacy_prefix_len(bytes);
        let (width, destination, source1, modrm_index, mut register_bytes, register_len) =
            match bytes.get(start).copied()? {
                0xC4 => {
                    let p0 = *bytes.get(start + 1)?;
                    let p1 = *bytes.get(start + 2)?;
                    if p0 & 0x1F != 3 || p1 & 0x03 != 1 || bytes.get(start + 3) != Some(&0x44) {
                        return None;
                    }
                    let modrm_index = start + 4;
                    let modrm = *bytes.get(modrm_index)?;
                    if modrm >> 6 == 3 {
                        return None;
                    }
                    let width = if p1 & 0x04 == 0 {
                        VecWidth::V128
                    } else {
                        VecWidth::V256
                    };
                    let destination = (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7);
                    let source1 = (!p1 >> 3) & 0x0F;
                    (
                        width,
                        destination,
                        source1,
                        modrm_index,
                        [0xC4, p0, p1, 0x44, 0, 0, 0],
                        6,
                    )
                }
                0x62 => {
                    let p0 = *bytes.get(start + 1)?;
                    let p1 = *bytes.get(start + 2)?;
                    let p2 = *bytes.get(start + 3)?;
                    if p0 & 0x07 != 3
                        || p1 & 0x07 != 0x05
                        || bytes.get(start + 4) != Some(&0x44)
                        || p2 & 0x10 != 0
                        || p2 & 0x80 != 0
                        || p2 & 0x07 != 0
                        || p2 & 0x60 == 0x60
                    {
                        return None;
                    }
                    let modrm_index = start + 5;
                    let modrm = *bytes.get(modrm_index)?;
                    if modrm >> 6 == 3 {
                        return None;
                    }
                    let width = match (p2 >> 5) & 3 {
                        0 => VecWidth::V128,
                        1 => VecWidth::V256,
                        2 => VecWidth::V512,
                        _ => unreachable!("reserved vector length rejected"),
                    };
                    let destination = (u8::from(p0 & 0x80 == 0) << 3)
                        | (u8::from(p0 & 0x10 == 0) << 4)
                        | ((modrm >> 3) & 7);
                    let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
                    (
                        width,
                        destination,
                        source1,
                        modrm_index,
                        [0x62, p0, p1, p2, 0x44, 0, 0],
                        7,
                    )
                }
                _ => return None,
            };

        let operand_end = memory_operand_end(bytes, modrm_index)?;
        if operand_end.checked_add(1) != Some(bytes.len()) {
            return None;
        }
        let immediate = bytes[operand_end];
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination && *candidate != source1)
            .expect("two operands cannot consume every low vector register");
        let memory_modrm = bytes[modrm_index];

        match register_bytes[0] {
            0xC4 => {
                register_bytes[1] =
                    (register_bytes[1] & 0x9F) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 };
                register_bytes[4] = 0xC0 | (memory_modrm & 0x38) | (scratch & 7);
                register_bytes[5] = immediate;
            }
            0x62 => {
                register_bytes[1] =
                    (register_bytes[1] & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 };
                register_bytes[2] |= 0x04;
                register_bytes[5] = 0xC0 | (memory_modrm & 0x38) | (scratch & 7);
                register_bytes[6] = immediate;
            }
            _ => unreachable!("validated vector encoding"),
        }
        let register_instruction =
            X86InstructionBytes::new(&register_bytes[..register_len]).unwrap();
        let vex = register_bytes[0] == 0xC4;
        Some(X86VpclmulqdqMemoryEncoding {
            width,
            destination,
            source1,
            scratch,
            immediate,
            register_instruction,
            needs_pclmulqdq: vex && width == VecWidth::V128,
            needs_vpclmulqdq: !vex || width == VecWidth::V256,
            needs_avx512vl: !vex && width != VecWidth::V512,
            supports_avx_ymm16: vex,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::{OpKind, SmirOp};
    use crate::smir::ir::types::{BlockId, OpId};
    use crate::smir::ir::{
        SmirBlock, x86_evex_vpclmulqdq_replay_spans, x86_native_replay_spans,
        x86_vex_vpclmulqdq_replay_spans,
    };
    use std::collections::HashMap;

    fn encoding(
        extension_bits: u8,
        w: bool,
        encoded_vvvv: u8,
        ymm: bool,
        modrm: u8,
        immediate: u8,
    ) -> [u8; 6] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        [
            0xC4,
            extension_bits | 3,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(ymm) << 2) | 1,
            0x44,
            modrm,
            immediate,
        ]
    }

    #[test]
    fn vex_classifier_exhaustively_covers_131_072_extension_w_vvvv_l_and_modrm_shapes() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for extension_bits in (0u8..8).map(|value| value << 5) {
            for w in [false, true] {
                for encoded_vvvv in 0u8..16 {
                    for ymm in [false, true] {
                        for modrm in u8::MIN..=u8::MAX {
                            let bytes = encoding(extension_bits, w, encoded_vvvv, ymm, modrm, 0xA5);
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let expected = (modrm >> 6 == 3).then_some(ymm);
                            assert_eq!(
                                instruction.vex_register_vpclmulqdq_uses_ymm(),
                                expected,
                                "{bytes:02X?}"
                            );
                            let destination_extension = u8::from(extension_bits & 0x80 == 0) << 3;
                            assert_eq!(
                                instruction.vex_vpclmulqdq_destination_index(),
                                expected.map(|_| destination_extension | ((modrm >> 3) & 7)),
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected.is_some());
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 32_768);
        assert_eq!(tested, 131_072);
    }

    #[test]
    fn vex_classifier_exhaustively_rejects_wrong_map_pp_opcode_and_length() {
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
                                let expected =
                                    map == 3 && pp == 1 && opcode == 0x44 && has_immediate;
                                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                                assert_eq!(
                                    instruction.vex_register_vpclmulqdq_uses_ymm(),
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
        assert_eq!(accepted, 4);
        assert_eq!(tested, 262_144);
    }

    #[test]
    fn vex_classifier_accepts_all_immediates_wig_and_ignored_x() {
        for immediate in u8::MIN..=u8::MAX {
            for extension_bits in [0xE0, 0xA0] {
                for w in [false, true] {
                    for ymm in [false, true] {
                        let bytes = encoding(extension_bits, w, 0x0D, ymm, 0xCA, immediate);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_register_vpclmulqdq_uses_ymm(),
                            Some(ymm),
                            "{bytes:02X?}"
                        );
                    }
                }
            }
        }

        // Independently assembled by LLVM 23. The W1 aliases are the
        // corresponding LLVM encodings with architecturally ignored W toggled.
        for (bytes, ymm, destination) in [
            (&[0xC4, 0x43, 0x29, 0x44, 0xCB, 0x11][..], false, 9),
            (&[0xC4, 0x43, 0xA9, 0x44, 0xCB, 0x11][..], false, 9),
            (&[0xC4, 0x43, 0x0D, 0x44, 0xEF, 0x11][..], true, 13),
            (&[0xC4, 0x43, 0x8D, 0x44, 0xEF, 0x11][..], true, 13),
        ] {
            let instruction = X86InstructionBytes::new(bytes).unwrap();
            assert_eq!(
                instruction.vex_register_vpclmulqdq_uses_ymm(),
                Some(ymm),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_vpclmulqdq_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn vex_classifier_rejects_memory_non_vex_and_trailing_forms() {
        let register = encoding(0xE0, true, 0x0D, true, 0xCA, 0xA5);
        let mut memory = register;
        memory[4] &= 0x3F;
        for bytes in [
            register[..5].to_vec(),
            register.iter().copied().chain([0]).collect(),
            [
                0x62,
                register[1],
                register[2],
                register[3],
                register[4],
                register[5],
            ]
            .to_vec(),
            [0xC5, register[2], register[3], register[4], register[5]].to_vec(),
            memory.to_vec(),
        ] {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_vpclmulqdq_uses_ymm(),
                None,
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_vpclmulqdq_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn vex_dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
        let pc = 0xC1A0;
        let instruction =
            X86InstructionBytes::new(&encoding(0x40, true, 3, true, 0xFF, 0x82)).unwrap();
        let mut block = SmirBlock::new(BlockId(51), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_vex_vpclmulqdq_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).expect("exact VEX VPCLMULQDQ span");
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
            assert!(!span.preserve_mxcsr_de);
        }
        assert!(x86_evex_vpclmulqdq_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }

    fn assert_memory_encoding(
        bytes: &[u8],
        width: VecWidth,
        destination: u8,
        source1: u8,
        scratch: u8,
        register_bytes: &[u8],
    ) -> X86VpclmulqdqMemoryEncoding {
        let encoding = X86InstructionBytes::new(bytes)
            .unwrap()
            .vpclmulqdq_memory_encoding()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(encoding.width, width, "{bytes:02X?}");
        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
        assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
        assert_eq!(encoding.immediate, *bytes.last().unwrap(), "{bytes:02X?}");
        assert_eq!(
            encoding.register_instruction.as_slice(),
            register_bytes,
            "{bytes:02X?}"
        );
        encoding
    }

    #[test]
    fn memory_classifier_rewrites_independently_assembled_vex_evex_and_apx_forms() {
        let vex128 = assert_memory_encoding(
            &[0xC4, 0xE3, 0x71, 0x44, 0x43, 0x20, 0xA5],
            VecWidth::V128,
            0,
            1,
            2,
            &[0xC4, 0xE3, 0x71, 0x44, 0xC2, 0xA5],
        );
        assert!(vex128.needs_pclmulqdq);
        assert!(!vex128.needs_vpclmulqdq);
        assert!(!vex128.needs_avx512vl);
        assert!(vex128.supports_avx_ymm16);

        let vex256 = assert_memory_encoding(
            &[0xC4, 0x43, 0x25, 0x44, 0x4B, 0x20, 0x11],
            VecWidth::V256,
            9,
            11,
            0,
            &[0xC4, 0x63, 0x25, 0x44, 0xC8, 0x11],
        );
        assert!(!vex256.needs_pclmulqdq);
        assert!(vex256.needs_vpclmulqdq);
        assert!(!vex256.needs_avx512vl);
        assert!(vex256.supports_avx_ymm16);

        let evex128 = assert_memory_encoding(
            &[0x62, 0xF3, 0x75, 0x08, 0x44, 0x43, 0x02, 0xEF],
            VecWidth::V128,
            0,
            1,
            2,
            &[0x62, 0xF3, 0x75, 0x08, 0x44, 0xC2, 0xEF],
        );
        assert!(!evex128.needs_pclmulqdq);
        assert!(evex128.needs_vpclmulqdq);
        assert!(evex128.needs_avx512vl);
        assert!(!evex128.supports_avx_ymm16);

        let evex512 = assert_memory_encoding(
            &[0x62, 0x43, 0x0D, 0x40, 0x44, 0x7B, 0x01, 0x01],
            VecWidth::V512,
            31,
            30,
            0,
            &[0x62, 0x63, 0x0D, 0x40, 0x44, 0xF8, 0x01],
        );
        assert!(evex512.needs_vpclmulqdq);
        assert!(!evex512.needs_avx512vl);
        assert!(!evex512.supports_avx_ymm16);

        assert_memory_encoding(
            &[0x62, 0xFB, 0x75, 0x08, 0x44, 0x00, 0x00],
            VecWidth::V128,
            0,
            1,
            2,
            &[0x62, 0xF3, 0x75, 0x08, 0x44, 0xC2, 0x00],
        );
        assert_memory_encoding(
            &[0x64, 0x67, 0xC4, 0xE3, 0xF1, 0x44, 0x44, 0x73, 0x20, 0xAA],
            VecWidth::V128,
            0,
            1,
            2,
            &[0xC4, 0xE3, 0xF1, 0x44, 0xC2, 0xAA],
        );
    }

    fn memory_form(evex: bool, modrm: u8, sib_base5: bool) -> Vec<u8> {
        assert!(modrm >> 6 != 3);
        let mut bytes = if evex {
            vec![0x62, 0xF3, 0x75, 0x08, 0x44]
        } else {
            vec![0xC4, 0xE3, 0x71, 0x44]
        };
        bytes.push(modrm);
        let mode = modrm >> 6;
        let rm = modrm & 7;
        if rm == 4 {
            bytes.push(if sib_base5 { 0x25 } else { 0x20 });
        }
        let displacement = match mode {
            0 if rm == 5 || (rm == 4 && sib_base5) => 4,
            0 => 0,
            1 => 1,
            2 => 4,
            _ => unreachable!(),
        };
        bytes.extend(std::iter::repeat_n(0xA5, displacement));
        bytes.push(0xEF);
        bytes
    }

    #[test]
    fn memory_classifier_exhaustively_accepts_all_modrm_sib_and_displacement_lengths() {
        let mut classified = 0usize;
        for evex in [false, true] {
            for modrm in 0u8..0xC0 {
                let sib_classes: &[bool] = if modrm & 7 == 4 {
                    &[false, true]
                } else {
                    &[false]
                };
                for &sib_base5 in sib_classes {
                    let bytes = memory_form(evex, modrm, sib_base5);
                    let encoding = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vpclmulqdq_memory_encoding()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(encoding.width, VecWidth::V128, "{bytes:02X?}");
                    assert_eq!(
                        encoding.register_instruction.as_slice().last(),
                        Some(&0xEF),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
        assert_eq!(classified, 432);
    }

    #[test]
    fn memory_classifier_exhaustively_decodes_all_vector_operands_widths_and_wig_values() {
        let mut classified = 0usize;
        for width in [VecWidth::V128, VecWidth::V256] {
            for w in [false, true] {
                for destination in 0u8..16 {
                    for source1 in 0u8..16 {
                        let mut p0 = 0xE3;
                        if destination & 8 != 0 {
                            p0 &= !0x80;
                        }
                        let p1 = (u8::from(w) << 7)
                            | (((!source1) & 0x0F) << 3)
                            | (u8::from(width == VecWidth::V256) << 2)
                            | 1;
                        let bytes = [0xC4, p0, p1, 0x44, (destination & 7) << 3, 0xA5];
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vpclmulqdq_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_ne!(encoding.scratch, destination, "{bytes:02X?}");
                        assert_ne!(encoding.scratch, source1, "{bytes:02X?}");
                        classified += 1;
                    }
                }
            }
        }

        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            for w in [false, true] {
                for destination in 0u8..32 {
                    for source1 in 0u8..32 {
                        let mut p0 = 0xF3;
                        if destination & 8 != 0 {
                            p0 &= !0x80;
                        }
                        if destination & 16 != 0 {
                            p0 &= !0x10;
                        }
                        let p1 = (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | 0x05;
                        let ll = match width {
                            VecWidth::V128 => 0,
                            VecWidth::V256 => 1,
                            VecWidth::V512 => 2,
                            _ => unreachable!(),
                        };
                        let p2 = (ll << 5) | if source1 & 16 == 0 { 0x08 } else { 0 };
                        let bytes = [0x62, p0, p1, p2, 0x44, (destination & 7) << 3, 0x5A];
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vpclmulqdq_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(encoding.width, width, "{bytes:02X?}");
                        assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_ne!(encoding.scratch, destination, "{bytes:02X?}");
                        assert_ne!(encoding.scratch, source1, "{bytes:02X?}");
                        classified += 1;
                    }
                }
            }
        }
        assert_eq!(classified, 2 * 2 * 16 * 16 + 3 * 2 * 32 * 32);
    }

    #[test]
    fn memory_classifier_fails_closed_at_every_byte_structure_boundary() {
        let vex = [0xC4, 0xE3, 0x71, 0x44, 0x43, 0x20, 0xA5];
        let evex = [0x62, 0xF3, 0x75, 0x08, 0x44, 0x43, 0x02, 0xEF];
        let mut invalid = Vec::new();

        for index in [1usize, 2, 3] {
            let mut bytes = vex;
            bytes[index] ^= 1;
            invalid.push(bytes.to_vec());
        }
        let mut bytes = vex;
        bytes[4] |= 0xC0;
        invalid.push(bytes.to_vec());
        invalid.push(vex[..6].to_vec());
        let mut bytes = vex.to_vec();
        bytes.push(0);
        invalid.push(bytes);
        invalid.push(vec![0xC4, 0xE3, 0x71, 0x44, 0x04]);
        invalid.push(vec![0xC4, 0xE3, 0x71, 0x44, 0x05, 0, 0, 0]);
        for prefix in [0x40, 0x66, 0xF0, 0xF2, 0xF3] {
            let mut bytes = vex.to_vec();
            bytes.insert(0, prefix);
            invalid.push(bytes);
        }

        for (index, mask) in [(1usize, 1u8), (2, 1), (2, 4), (3, 0x10), (3, 0x80), (3, 1)] {
            let mut bytes = evex;
            bytes[index] ^= mask;
            invalid.push(bytes.to_vec());
        }
        let mut bytes = evex;
        bytes[3] = (bytes[3] & !0x60) | 0x60;
        invalid.push(bytes.to_vec());
        let mut bytes = evex;
        bytes[5] |= 0xC0;
        invalid.push(bytes.to_vec());

        for bytes in invalid {
            assert!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vpclmulqdq_memory_encoding()
                    .is_none(),
                "{bytes:02X?}"
            );
        }
    }
}
