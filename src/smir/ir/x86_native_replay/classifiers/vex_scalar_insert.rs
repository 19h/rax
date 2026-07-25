//! Register-only AVX VEX scalar lane inserts.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only VEX `VPINSRB`, `VPINSRW`, `VPINSRD`,
    /// `VPINSRQ`, or `VINSERTPS`.
    ///
    /// Every admitted form is fixed at VEX.128, mandatory 66H, and requires
    /// AVX. `VPINSRB`, `VPINSRW`, and `VINSERTPS` treat W as ignored;
    /// opcode 22H selects `VPINSRD` with W0 and `VPINSRQ` with W1. Compact C5
    /// and extended C4 encodings are both accepted for map-0F `VPINSRW`.
    /// Guest RSP/RBP scalar sources are excluded because the native trampoline
    /// owns host RSP/RBP. Memory forms remain at the precise interpreter
    /// boundary.
    pub fn is_vex_register_scalar_insert(&self) -> bool {
        let bytes = self.as_slice();
        match bytes {
            [0xC5, p1, 0xC4, modrm, _imm] => {
                p1 & 0x07 == 1 && modrm >> 6 == 3 && !matches!(modrm & 7, 4 | 5)
            }
            [0xC4, p0, p1, opcode, modrm, _imm] => {
                if p1 & 0x07 != 1 || modrm >> 6 != 3 {
                    return false;
                }
                let gpr_source = match (p0 & 0x1F, opcode) {
                    (1, 0xC4) | (3, 0x20 | 0x22) => true,
                    (3, 0x21) => false,
                    _ => return false,
                };
                !gpr_source || p0 & 0x20 == 0 || !matches!(modrm & 7, 4 | 5)
            }
            _ => false,
        }
    }

    /// Architectural XMM destination selected by an exact register-only VEX
    /// scalar insert. ModR/M.reg is extended by inverted VEX.R in either C5
    /// or C4 form.
    pub(crate) fn vex_scalar_insert_destination_index(&self) -> Option<u8> {
        if !self.is_vex_register_scalar_insert() {
            return None;
        }
        let (extension, modrm) = match self.as_slice() {
            [0xC5, p1, 0xC4, modrm, _imm] => (u8::from(p1 & 0x80 == 0) << 3, *modrm),
            [0xC4, p0, _p1, _opcode, modrm, _imm] => (u8::from(p0 & 0x80 == 0) << 3, *modrm),
            _ => unreachable!(),
        };
        Some(extension | ((modrm >> 3) & 7))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::{OpKind, SmirOp};
    use crate::smir::ir::types::{BlockId, OpId};
    use crate::smir::ir::{
        SmirBlock, x86_evex_native_replay_spans, x86_native_replay_spans,
        x86_vex_alignr_replay_spans, x86_vex_scalar_insert_replay_spans,
    };
    use std::collections::HashMap;

    const C4_FAMILIES: [(u8, u8); 4] = [(1, 0xC4), (3, 0x20), (3, 0x21), (3, 0x22)];

    fn c4_encoding(
        map: u8,
        opcode: u8,
        extension_bits: u8,
        w: bool,
        encoded_vvvv: u8,
        l: bool,
        pp: u8,
        modrm: u8,
        imm: u8,
    ) -> [u8; 6] {
        assert!(map < 32);
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        assert!(pp < 4);
        [
            0xC4,
            extension_bits | map,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | pp,
            opcode,
            modrm,
            imm,
        ]
    }

    fn c4_has_gpr_source(map: u8, opcode: u8) -> bool {
        !matches!((map, opcode), (3, 0x21))
    }

    #[test]
    fn classifier_exhaustively_covers_262_144_c4_prefix_opcode_and_modrm_shapes() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for (map, opcode) in C4_FAMILIES {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for w in [false, true] {
                    for encoded_vvvv in 0u8..16 {
                        for modrm in u8::MIN..=u8::MAX {
                            let bytes = c4_encoding(
                                map,
                                opcode,
                                extension_bits,
                                w,
                                encoded_vvvv,
                                false,
                                1,
                                modrm,
                                0xA5,
                            );
                            let instruction = X86InstructionBytes::new(&bytes).unwrap();
                            let low_gpr_sp_bp = c4_has_gpr_source(map, opcode)
                                && extension_bits & 0x20 != 0
                                && matches!(modrm & 7, 4 | 5);
                            let expected = modrm >> 6 == 3 && !low_gpr_sp_bp;
                            assert_eq!(
                                instruction.is_vex_register_scalar_insert(),
                                expected,
                                "{bytes:02X?}"
                            );
                            let destination_extension = u8::from(extension_bits & 0x80 == 0) << 3;
                            assert_eq!(
                                instruction.vex_scalar_insert_destination_index(),
                                expected.then_some(destination_extension | ((modrm >> 3) & 7)),
                                "{bytes:02X?}"
                            );
                            accepted += usize::from(expected);
                            tested += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 59_392);
        assert_eq!(tested, 262_144);
    }

    #[test]
    fn classifier_exhaustively_covers_all_65_536_c5_prefix_and_modrm_shapes() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for p1 in u8::MIN..=u8::MAX {
            for modrm in u8::MIN..=u8::MAX {
                let bytes = [0xC5, p1, 0xC4, modrm, 0xA5];
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let expected = p1 & 0x07 == 1 && modrm >> 6 == 3 && !matches!(modrm & 7, 4 | 5);
                assert_eq!(
                    instruction.is_vex_register_scalar_insert(),
                    expected,
                    "{bytes:02X?}"
                );
                assert_eq!(
                    instruction.vex_scalar_insert_destination_index(),
                    expected.then_some((u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7)),
                    "{bytes:02X?}"
                );
                accepted += usize::from(expected);
                tested += 1;
            }
        }
        assert_eq!(accepted, 1_536);
        assert_eq!(tested, 65_536);
    }

    #[test]
    fn classifier_accepts_all_immediates_wig_values_and_ignored_x_values() {
        for imm in u8::MIN..=u8::MAX {
            for (map, opcode) in C4_FAMILIES {
                for extension_bits in [0xE0, 0xA0] {
                    for w in [false, true] {
                        let bytes =
                            c4_encoding(map, opcode, extension_bits, w, 0x0D, false, 1, 0xCA, imm);
                        assert!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .is_vex_register_scalar_insert(),
                            "{bytes:02X?}"
                        );
                    }
                }
            }
            let compact = [0xC5, 0xC9, 0xC4, 0xEF, imm];
            assert!(
                X86InstructionBytes::new(&compact)
                    .unwrap()
                    .is_vex_register_scalar_insert(),
                "{compact:02X?}"
            );
        }

        // Independently assembled by LLVM 23, except the WIG W1 VINSERTPS
        // encoding, which is the preceding LLVM encoding with VEX.W toggled.
        for (bytes, destination) in [
            (&[0xC4, 0x43, 0x29, 0x20, 0xCB, 0x0F][..], 9),
            (&[0xC5, 0xC9, 0xC4, 0xEF, 0x07][..], 5),
            (&[0xC4, 0x41, 0x09, 0xC4, 0xEF, 0x07][..], 13),
            (&[0xC4, 0x43, 0x29, 0x22, 0xCB, 0x03][..], 9),
            (&[0xC4, 0x43, 0x89, 0x22, 0xEF, 0x01][..], 13),
            (&[0xC4, 0x43, 0x29, 0x21, 0xCB, 0xA5][..], 9),
            (&[0xC4, 0x43, 0xA9, 0x21, 0xCB, 0xA5][..], 9),
        ] {
            let instruction = X86InstructionBytes::new(bytes).unwrap();
            assert!(instruction.is_vex_register_scalar_insert(), "{bytes:02X?}");
            assert_eq!(
                instruction.vex_scalar_insert_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_rejects_every_structural_reserved_and_host_owned_frontier() {
        let canonical = c4_encoding(3, 0x21, 0xE0, false, 0x0D, false, 1, 0xCA, 0xA5);
        let mut invalid = vec![
            canonical[..5].to_vec(),
            canonical.iter().copied().chain([0]).collect(),
            [
                0x62,
                canonical[1],
                canonical[2],
                canonical[3],
                canonical[4],
                canonical[5],
            ]
            .to_vec(),
            [0xC5, canonical[2], canonical[3], canonical[4], canonical[5]].to_vec(),
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
            (2, canonical[2] | 0x04),
            (3, 0x1F),
            (3, 0x23),
            (3, 0xC4),
            (4, canonical[4] & 0x3F),
            (4, (canonical[4] & 0x3F) | 0x40),
            (4, (canonical[4] & 0x3F) | 0x80),
        ] {
            let mut bytes = canonical;
            bytes[index] = value;
            invalid.push(bytes.to_vec());
        }
        let mut wrong_map_opcode = canonical;
        wrong_map_opcode[1] = (wrong_map_opcode[1] & !0x1F) | 1;
        invalid.push(wrong_map_opcode.to_vec());

        for rm in [4, 5] {
            invalid.push(c4_encoding(3, 0x20, 0xE0, false, 0x0D, false, 1, 0xC0 | rm, 0).to_vec());
            invalid.push([0xC5, 0xC9, 0xC4, 0xC0 | rm, 0].to_vec());
        }

        for bytes in invalid {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert!(!instruction.is_vex_register_scalar_insert(), "{bytes:02X?}");
            assert_eq!(
                instruction.vex_scalar_insert_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }

        // R12/R13 use the same low ModR/M numbers as RSP/RBP but are safe
        // identity-mapped guest GPRs when inverted VEX.B selects the high bank.
        for rm in [4, 5] {
            let bytes = c4_encoding(3, 0x20, 0xC0, true, 0x0D, false, 1, 0xC0 | rm, 0);
            assert!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .is_vex_register_scalar_insert(),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
        let pc = 0xA11D;
        let instruction =
            X86InstructionBytes::new(&c4_encoding(3, 0x21, 0x40, true, 3, false, 1, 0xFF, 0x82))
                .unwrap();
        let mut block = SmirBlock::new(BlockId(50), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_vex_scalar_insert_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).expect("exact VEX scalar-insert span");
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
            assert!(!span.preserve_mxcsr_de);
        }
        assert!(x86_vex_alignr_replay_spans(&block, &provenance).is_empty());
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }
}
