//! Exact source-byte replay classification for legacy SSE, AVX VEX, and EVEX
//! floating-point shuffle/interleave instructions.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Element {
    F32,
    F64,
}

impl Element {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn pp(self) -> u8 {
        match self {
            Self::F32 => 0,
            Self::F64 => 1,
        }
    }
}

fn legacy_encoding(
    element: Element,
    rex: Option<u8>,
    opcode: u8,
    modrm: u8,
    imm: Option<u8>,
) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    assert_eq!(imm.is_some(), opcode == 0xC6);
    let mut bytes = Vec::new();
    if element == Element::F64 {
        bytes.push(0x66);
    }
    if let Some(rex) = rex {
        bytes.push(rex);
    }
    bytes.extend([0x0F, opcode, modrm]);
    if let Some(imm) = imm {
        bytes.push(imm);
    }
    bytes
}

fn vex_c5_encoding(p1: u8, opcode: u8, modrm: u8, imm: Option<u8>) -> Vec<u8> {
    assert_eq!(imm.is_some(), opcode == 0xC6);
    let mut bytes = vec![0xC5, p1, opcode, modrm];
    bytes.extend(imm);
    bytes
}

fn vex_c4_encoding(p0: u8, p1: u8, opcode: u8, modrm: u8, imm: Option<u8>) -> Vec<u8> {
    assert_eq!(imm.is_some(), opcode == 0xC6);
    let mut bytes = vec![0xC4, p0, p1, opcode, modrm];
    bytes.extend(imm);
    bytes
}

#[test]
fn legacy_classifier_covers_all_561_408_safe_canonical_register_encodings() {
    let mut classified = 0usize;
    for element in Element::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                for opcode in [0x14, 0x15] {
                    let bytes = legacy_encoding(element, rex, opcode, modrm, None);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_shuffle_needs_avx(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
                for imm in u8::MIN..=u8::MAX {
                    let bytes = legacy_encoding(element, rex, 0xC6, modrm, Some(imm));
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_shuffle_needs_avx(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 561_408);
}

#[test]
fn vex_classifier_exhausts_header_map_modrm_opcode_and_immediate_axes() {
    for p1 in u8::MIN..=u8::MAX {
        let expected = matches!(p1 & 0x03, 0 | 1).then_some(true);
        for (opcode, imm) in [(0x14, None), (0x15, None), (0xC6, Some(0xA5))] {
            let bytes = vex_c5_encoding(p1, opcode, 0xC3, imm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_shuffle_needs_avx(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for p0 in u8::MIN..=u8::MAX {
        for p1 in u8::MIN..=u8::MAX {
            let expected = (p0 & 0x1F == 1 && matches!(p1 & 0x03, 0 | 1)).then_some(true);
            for (opcode, imm) in [(0x14, None), (0x15, None), (0xC6, Some(0x5A))] {
                let bytes = vex_c4_encoding(p0, p1, opcode, 0xFC, imm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_fp_shuffle_needs_avx(),
                    expected,
                    "{bytes:02X?}"
                );
            }
        }
    }

    for element in Element::ALL {
        for opcode in [0x14, 0x15, 0xC6] {
            let imm = (opcode == 0xC6).then_some(0xE4);
            for modrm in u8::MIN..=u8::MAX {
                let expected = (modrm >> 6 == 3).then_some(true);
                for bytes in [
                    vex_c5_encoding(0xF8 | element.pp(), opcode, modrm, imm),
                    vex_c4_encoding(0x41, 0x78 | element.pp(), opcode, modrm, imm),
                ] {
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_shuffle_needs_avx(),
                        expected,
                        "{bytes:02X?}"
                    );
                }
            }
        }
        for imm in u8::MIN..=u8::MAX {
            for bytes in [
                vex_c5_encoding(0xFC | element.pp(), 0xC6, 0xC8, Some(imm)),
                vex_c4_encoding(0x21, 0xFC | element.pp(), 0xC6, 0xFF, Some(imm)),
            ] {
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_fp_shuffle_needs_avx(),
                    Some(true),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn legacy_vex_classifier_rejects_wrong_opcodes_prefixes_lengths_and_memory() {
    for opcode in u8::MIN..=u8::MAX {
        if matches!(opcode, 0x14 | 0x15 | 0xC6) {
            continue;
        }
        for bytes in [
            legacy_encoding(Element::F32, Some(0x4F), opcode, 0xFF, None),
            vex_c5_encoding(0xF8, opcode, 0xFF, None),
            vex_c4_encoding(0x01, 0xF9, opcode, 0xFF, None),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_shuffle_needs_avx(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    for element in Element::ALL {
        for opcode in [0x14, 0x15, 0xC6] {
            let imm = (opcode == 0xC6).then_some(0x1B);
            for modrm in [0x00, 0x45, 0x84, 0xBF] {
                for bytes in [
                    legacy_encoding(element, Some(0x48), opcode, modrm, imm),
                    vex_c5_encoding(0xF8 | element.pp(), opcode, modrm, imm),
                    vex_c4_encoding(0xE1, 0xF8 | element.pp(), opcode, modrm, imm),
                ] {
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_shuffle_needs_avx(),
                        None,
                        "{bytes:02X?}"
                    );
                }
            }
        }
    }

    let invalid: &[&[u8]] = &[
        &[0xF2, 0x0F, 0x14, 0xC0],
        &[0xF3, 0x0F, 0x15, 0xC0],
        &[0x48, 0x66, 0x0F, 0x14, 0xC0],
        &[0x67, 0x0F, 0x14, 0xC0],
        &[0x0F, 0xC6, 0xC0],
        &[0x0F, 0x14, 0xC0, 0],
        &[0xC5, 0xF8, 0xC6, 0xC0],
        &[0xC5, 0xF8, 0x14, 0xC0, 0],
        &[0xC4, 0xE1, 0xF8, 0xC6, 0xC0],
        &[0xC4, 0xE1, 0xF8, 0x14, 0xC0, 0],
        &[0x62, 0xF1, 0x7C, 0x08, 0x14, 0xC0],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_fp_shuffle_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_preserve_exact_bytes_and_keep_legacy_vex_evex_disjoint() {
    let pc = 0x1013;
    let mut block = SmirBlock::new(BlockId(36), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_avx) in [
        (
            legacy_encoding(Element::F64, Some(0x4D), 0xC6, 0xCB, Some(0xA5)),
            false,
        ),
        (vex_c5_encoding(0x2C, 0x14, 0xCB, None), true),
        (vex_c4_encoding(0x41, 0xAD, 0xC6, 0xCB, Some(0x1B)), true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.legacy_vex_register_fp_shuffle_needs_avx(),
            Some(needs_avx),
            "{bytes:02X?}"
        );
        let provenance = HashMap::from([((BlockId(36), pc), instruction)]);
        for spans in [
            x86_legacy_vex_fp_shuffle_replay_spans(&block, &provenance),
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
        assert!(x86_evex_fp_shuffle_replay_spans(&block, &provenance).is_empty());
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());
    }

    assert!(x86_native_replay_spans(&block, &HashMap::new()).is_empty());
}

#[test]
fn evex_classifier_is_exact_and_fail_closed() {
    let valid = [
        // vunpcklpd zmm17{k1}{z}, zmm18, zmm19
        (&[0x62, 0xA1, 0xED, 0xC1, 0x14, 0xCB][..], Some(false)),
        (&[0x62, 0xF1, 0x6C, 0x29, 0x15, 0xC8][..], Some(true)),
        (&[0x62, 0xF1, 0xED, 0x09, 0xC6, 0xC8, 0xE4][..], Some(true)),
    ];
    for (bytes, expected) in valid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp_shuffle_needs_vl(),
            expected,
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x62, 0xF2, 0x7C, 0x09, 0x14, 0xC8],       // wrong map
        &[0x62, 0xF1, 0x78, 0x09, 0x14, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF1, 0x7E, 0x09, 0x14, 0xC8],       // invalid mandatory prefix
        &[0x62, 0xF1, 0xFC, 0x09, 0x14, 0xC8],       // VUNPCKLPS with W1
        &[0x62, 0xF1, 0x7D, 0x09, 0x14, 0xC8],       // VUNPCKLPD with W0
        &[0x62, 0xF1, 0x7C, 0x09, 0x14, 0x08],       // memory source
        &[0x62, 0xF1, 0x7C, 0x19, 0x14, 0xC8],       // EVEX.b
        &[0x62, 0xF1, 0x7C, 0x88, 0x14, 0xC8],       // {z} with k0
        &[0x62, 0xF1, 0x7C, 0x69, 0x14, 0xC8],       // L'L=3
        &[0x62, 0xF1, 0x7C, 0x09, 0xC6, 0xC8],       // missing shuffle imm8
        &[0x62, 0xF1, 0x7C, 0x09, 0x14, 0xC8, 0x00], // spurious unpack imm8
        &[0x62, 0xF1, 0x7C, 0x09, 0x16, 0xC8],       // unrelated opcode
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp_shuffle_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}
