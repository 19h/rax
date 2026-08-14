//! Exact source-byte replay classification for guest-stack-destination VEX
//! vector sign-mask extracts.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaskKind {
    Vmovmskps,
    Vmovmskpd,
    Vpmovmskb,
}

impl MaskKind {
    const ALL: [Self; 3] = [Self::Vmovmskps, Self::Vmovmskpd, Self::Vpmovmskb];

    fn fields(self) -> (u8, u8) {
        match self {
            Self::Vmovmskps => (0, 0x50),
            Self::Vmovmskpd => (1, 0x50),
            Self::Vpmovmskb => (1, 0xD7),
        }
    }
}

fn c4_encoding(
    kind: MaskKind,
    w: bool,
    wide: bool,
    ignored_x: bool,
    destination: u8,
    source: u8,
) -> [u8; 5] {
    assert!(matches!(destination, 4 | 5));
    assert!(source < 16);
    let (pp, opcode) = kind.fields();
    let mut p0 = 0xE1;
    if ignored_x {
        p0 &= !0x40;
    }
    if source >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(w) << 7) | 0x78 | (u8::from(wide) << 2) | pp,
        opcode,
        0xC0 | (destination << 3) | (source & 7),
    ]
}

fn c5_encoding(kind: MaskKind, wide: bool, destination: u8, source: u8) -> [u8; 4] {
    assert!(matches!(destination, 4 | 5));
    assert!(source < 8);
    let (pp, opcode) = kind.fields();
    [
        0xC5,
        0xF8 | (u8::from(wide) << 2) | pp,
        opcode,
        0xC0 | (destination << 3) | source,
    ]
}

fn encoded_destination(bytes: &[u8]) -> u8 {
    match bytes {
        [0xC5, p1, _opcode, modrm] => (u8::from(p1 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
        [0xC4, p0, _p1, _opcode, modrm] => (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
        _ => panic!("not a VEX MOVMSK byte shape: {bytes:02X?}"),
    }
}

fn assert_classified(bytes: &[u8], destination: u8, needs_avx2: bool) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    assert_eq!(
        instruction.vex_mov_mask_stack_destination_needs_avx2(),
        Some(needs_avx2),
        "{bytes:02X?}"
    );
    assert_eq!(
        instruction.vex_mov_mask_stack_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_864_legal_guest_stack_destination_encodings() {
    let mut classified = 0usize;
    for kind in MaskKind::ALL {
        for w in [false, true] {
            for wide in [false, true] {
                for ignored_x in [false, true] {
                    for destination in [4, 5] {
                        for source in 0..16 {
                            let bytes = c4_encoding(kind, w, wide, ignored_x, destination, source);
                            assert_classified(
                                &bytes,
                                destination,
                                kind == MaskKind::Vpmovmskb && wide,
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
        for wide in [false, true] {
            for destination in [4, 5] {
                for source in 0..8 {
                    let bytes = c5_encoding(kind, wide, destination, source);
                    assert_classified(&bytes, destination, kind == MaskKind::Vpmovmskb && wide);
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 864);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_exact_shape_frontiers() {
    let base = c4_encoding(MaskKind::Vmovmskps, false, false, false, 4, 1);
    for p0 in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[1] = p0;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2()
                .is_some(),
            p0 & 0x9F == 0x81,
            "{bytes:02X?}"
        );
    }
    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2()
                .is_some(),
            p1 & 0x78 == 0x78 && p1 & 0x03 <= 1,
            "{bytes:02X?}"
        );
    }
    for opcode in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[3] = opcode;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2()
                .is_some(),
            opcode == 0x50,
            "{bytes:02X?}"
        );
    }
    for modrm in u8::MIN..=u8::MAX {
        let mut bytes = base;
        bytes[4] = modrm;
        let expected = modrm >> 6 == 3 && matches!((modrm >> 3) & 7, 4 | 5);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2()
                .is_some(),
            expected,
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let bytes = [0xC5, p1, 0x50, 0xE1];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_mov_mask_stack_destination_needs_avx2()
                .is_some(),
            p1 & 0x80 != 0 && p1 & 0x78 == 0x78 && p1 & 0x03 <= 1,
            "{bytes:02X?}"
        );
    }

    for bytes in [
        &base[..4],
        &[base[0], base[1], base[2], base[3], base[4], 0][..],
        &[0x62, 0xF1, 0x7C, 0x08, 0x50, 0xE1][..],
        &[0xC4, 0xE2, 0x78, 0x50, 0xE1][..],
        &[0xC4, 0xE1, 0x70, 0x50, 0xE1][..],
        &[0xC4, 0xE1, 0x7A, 0x50, 0xE1][..],
        &[0xC4, 0xE1, 0x78, 0xD7, 0xE1][..],
        &[0xC4, 0xE1, 0x79, 0x51, 0xE1][..],
        &[0xC4, 0xE1, 0x78, 0x50, 0xC1][..],
        &[0xC4, 0x61, 0x78, 0x50, 0xE1][..],
        &[0xC5, 0xF0, 0x50, 0xE1][..],
        &[0xC5, 0xFA, 0x50, 0xE1][..],
        &[0xC5, 0xF8, 0x50, 0xC1][..],
        &[0xC5, 0x78, 0x50, 0xE1][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_mov_mask_stack_destination_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_mov_mask_stack_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn destination_rewrite_changes_only_vex_r_and_modrm_reg() {
    for kind in MaskKind::ALL {
        for wide in [false, true] {
            for destination in [4, 5] {
                for bytes in [
                    c4_encoding(kind, true, wide, true, destination, 15).to_vec(),
                    c5_encoding(kind, wide, destination, 7).to_vec(),
                ] {
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    for rewritten_destination in [0, 3, 4, 5, 8, 15] {
                        let rewritten = instruction
                            .vex_mov_mask_stack_destination_with_destination(rewritten_destination)
                            .unwrap();
                        assert_eq!(
                            encoded_destination(rewritten.as_slice()),
                            rewritten_destination,
                            "{kind:?} {bytes:02X?}"
                        );
                        let mut expected = bytes.clone();
                        let (prefix_index, modrm_index) =
                            if expected[0] == 0xC5 { (1, 3) } else { (1, 4) };
                        if rewritten_destination < 8 {
                            expected[prefix_index] |= 0x80;
                        } else {
                            expected[prefix_index] &= !0x80;
                        }
                        expected[modrm_index] =
                            (expected[modrm_index] & !0x38) | ((rewritten_destination & 7) << 3);
                        assert_eq!(rewritten.as_slice(), expected, "{kind:?} {bytes:02X?}");
                    }
                    assert_eq!(
                        instruction.vex_mov_mask_stack_destination_with_destination(16),
                        None,
                        "{kind:?} {bytes:02X?}"
                    );
                }
            }
        }
    }
}

#[test]
fn llvm_samples_and_replay_spans_preserve_exact_stack_destination_bytes() {
    let pc = 0x10D7;

    // LLVM 23.0.0 independently assembled these compact and extended samples.
    for (bytes, destination, needs_avx2) in [
        (&[0xC4, 0xC1, 0x7C, 0x50, 0xE1][..], 4, false),
        (&[0xC4, 0xC1, 0x79, 0x50, 0xEA][..], 5, false),
        (&[0xC4, 0xC1, 0x79, 0xD7, 0xE3][..], 4, false),
        (&[0xC4, 0xC1, 0x7D, 0xD7, 0xEC][..], 5, true),
        (&[0xC5, 0xF8, 0x50, 0xE1][..], 4, false),
        (&[0xC5, 0xFD, 0x50, 0xEA][..], 5, false),
        (&[0xC5, 0xF9, 0xD7, 0xE3][..], 4, false),
        (&[0xC5, 0xFD, 0xD7, 0xEC][..], 5, true),
    ] {
        use crate::smir::lift::x86_64::X86_64Lifter;
        use crate::smir::lift::{LiftContext, SmirLifter};

        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_classified(bytes, destination, needs_avx2);
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(pc, bytes, &mut context).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        let mut block = SmirBlock::new(BlockId(57), pc);
        block.ops = result.ops;
        let provenance = HashMap::from([((BlockId(57), pc), instruction)]);
        for spans in [
            x86_vex_mov_mask_stack_destination_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            assert_eq!(
                spans.get(&0),
                Some(&X86NativeReplaySpan {
                    end: 1,
                    instruction,
                    needs_avx512vl: false,
                    needs_avx512dq: false,
                    needs_avx512fp16: false,
                    preserve_mxcsr_de: false,
                }),
                "{bytes:02X?}"
            );
        }
        assert!(
            x86_vex_mov_mask_stack_destination_replay_spans(&block, &HashMap::new()).is_empty()
        );
    }
}
