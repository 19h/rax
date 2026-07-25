//! Register-only EVEX VPCLMULQDQ replay classification.

use super::X86InstructionBytes;

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
}
