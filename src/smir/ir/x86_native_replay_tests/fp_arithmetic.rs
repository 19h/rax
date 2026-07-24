//! Exact source-byte replay classification for binary floating-point arithmetic.

use super::*;

const OPCODES: [u8; 6] = [0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

fn legacy_encoding(mandatory_prefix: u8, rex: Option<u8>, opcode: u8, modrm: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    match mandatory_prefix {
        0 => {}
        1 => bytes.push(0x66),
        2 => bytes.push(0xF3),
        3 => bytes.push(0xF2),
        _ => panic!("invalid mandatory prefix"),
    }
    if let Some(rex) = rex {
        assert!(matches!(rex, 0x40..=0x4F));
        bytes.push(rex);
    }
    bytes.extend([0x0F, opcode, modrm]);
    bytes
}

fn c4_encoding(
    extension_bits: u8,
    w: bool,
    encoded_vvvv: u8,
    l: bool,
    pp: u8,
    opcode: u8,
    modrm: u8,
) -> [u8; 5] {
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(encoded_vvvv < 16 && pp < 4);
    [
        0xC4,
        extension_bits | 1,
        (if w { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (if l { 0x04 } else { 0 }) | pp,
        opcode,
        modrm,
    ]
}

fn c5_encoding(
    encoded_r: bool,
    encoded_vvvv: u8,
    l: bool,
    pp: u8,
    opcode: u8,
    modrm: u8,
) -> [u8; 4] {
    assert!(encoded_vvvv < 16 && pp < 4);
    [
        0xC5,
        (if encoded_r { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (if l { 0x04 } else { 0 }) | pp,
        opcode,
        modrm,
    ]
}

#[test]
fn non_evex_classifier_exhaustively_accepts_689_664_safe_register_encodings() {
    let mut accepted = 0usize;
    let mut tested = 0usize;

    for opcode in OPCODES {
        for mandatory_prefix in 0u8..4 {
            for rex in std::iter::once(None).chain((0x40u8..=0x4F).map(Some)) {
                for reg_rm in 0u8..=0x3F {
                    let bytes = legacy_encoding(mandatory_prefix, rex, opcode, 0xC0 | reg_rm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_arithmetic_needs_avx(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    accepted += 1;
                    tested += 1;
                }
            }
        }

        for pp in 0u8..4 {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for w in [false, true] {
                    for encoded_vvvv in 0u8..16 {
                        for l in [false, true] {
                            for reg_rm in 0u8..=0x3F {
                                let bytes = c4_encoding(
                                    extension_bits,
                                    w,
                                    encoded_vvvv,
                                    l,
                                    pp,
                                    opcode,
                                    0xC0 | reg_rm,
                                );
                                let expected = (!(pp >= 2 && l)).then_some(true);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .legacy_vex_register_fp_arithmetic_needs_avx(),
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

            for encoded_r in [false, true] {
                for encoded_vvvv in 0u8..16 {
                    for l in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            let bytes =
                                c5_encoding(encoded_r, encoded_vvvv, l, pp, opcode, 0xC0 | reg_rm);
                            let expected = (!(pp >= 2 && l)).then_some(true);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .legacy_vex_register_fp_arithmetic_needs_avx(),
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

    assert_eq!(accepted, 689_664);
    assert_eq!(tested, 910_848);

    // Independently assembled by LLVM 21.1.8.
    for (bytes, needs_avx) in [
        (&[0x0F, 0x58, 0xCB][..], false),
        (&[0xF2, 0x45, 0x0F, 0x5F, 0xCB][..], false),
        (&[0xC4, 0x41, 0x2C, 0x58, 0xCB][..], true),
        (&[0xC4, 0x41, 0x29, 0x59, 0xCB][..], true),
        (&[0xC4, 0x41, 0x2A, 0x5C, 0xCB][..], true),
        (&[0xC4, 0x41, 0x2B, 0x5E, 0xCB][..], true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_fp_arithmetic_needs_avx(),
            Some(needs_avx),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn non_evex_classifier_rejects_memory_multiple_prefix_and_unpredictable_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0xF0, 0x0F, 0x58, 0xC1],
        &[0x66, 0xF3, 0x0F, 0x58, 0xC1],
        &[0x40, 0x66, 0x0F, 0x58, 0xC1],
        &[0x0F, 0x58, 0x01],
        &[0x0F, 0x51, 0xC1],
        &[0x0F, 0x58, 0xC1, 0],
        &[0xC4, 0xE2, 0x78, 0x58, 0xC1],
        &[0xC4, 0xE1, 0x7A, 0x58, 0x01],
        &[0xC4, 0xE1, 0x7E, 0x58, 0xC1],
        &[0xC5, 0xFE, 0x58, 0xC1],
        &[0xC5, 0xF8, 0x51, 0xC1],
        &[0xC5, 0xF8, 0x58, 0xC1, 0],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_fp_arithmetic_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn evex_classifier_exhaustively_covers_controls_masks_widths_and_registers() {
    for opcode in OPCODES {
        for pp in 0u8..4 {
            let w = matches!(pp, 1 | 3);
            let p1 = (if w { 0x80 } else { 0 }) | 0x64 | pp;
            let scalar = pp >= 2;
            for ll in 0u8..4 {
                for embedded_control in [false, true] {
                    for zeroing in [false, true] {
                        for mask in 0u8..8 {
                            let p2 = (if zeroing { 0x80 } else { 0 })
                                | (ll << 5)
                                | (if embedded_control { 0x10 } else { 0 })
                                | 0x08
                                | mask;
                            for reg_rm in 0u8..=0x3F {
                                let bytes = [0x62, 0xF1, p1, p2, opcode, 0xC0 | reg_rm];
                                let expected =
                                    if zeroing && mask == 0 || (!embedded_control && ll == 3) {
                                        None
                                    } else if scalar || embedded_control {
                                        Some(false)
                                    } else if ll <= 1 {
                                        Some(true)
                                    } else {
                                        Some(false)
                                    };
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_fp_arithmetic_needs_vl(),
                                    expected,
                                    "{bytes:02X?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    for p0 in 0u8..=u8::MAX {
        let bytes = [0x62, p0, 0x6C, 0x08, 0x58, 0xC1];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp_arithmetic_needs_vl(),
            (p0 & 0x0F == 1).then_some(true),
            "{bytes:02X?}"
        );
    }
    for p1 in 0u8..=u8::MAX {
        let bytes = [0x62, 0xF1, p1, 0x08, 0x58, 0xC1];
        let pp = p1 & 3;
        let valid = p1 & 4 != 0 && (p1 & 0x80 != 0) == matches!(pp, 1 | 3);
        let expected = valid.then_some(pp < 2);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp_arithmetic_needs_vl(),
            expected,
            "{bytes:02X?}"
        );
    }
    for p2 in 0u8..=u8::MAX {
        let bytes = [0x62, 0xF1, 0x6C, p2, 0x58, 0xC1];
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 3;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 7;
        let expected = if zeroing && mask == 0 || (!embedded_control && ll == 3) {
            None
        } else if embedded_control || ll == 2 {
            Some(false)
        } else {
            Some(true)
        };
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp_arithmetic_needs_vl(),
            expected,
            "{bytes:02X?}"
        );
    }
    for modrm in 0u8..=u8::MAX {
        let bytes = [0x62, 0xF1, 0x6C, 0x08, 0x58, modrm];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp_arithmetic_needs_vl(),
            (modrm >> 6 == 3).then_some(true),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x62, 0xF1, 0x6C, 0x08, 0x58],
        &[0x61, 0xF1, 0x6C, 0x08, 0x58, 0xC1],
        &[0x62, 0xF5, 0x6C, 0x08, 0x58, 0xC1],
        &[0x62, 0xF1, 0x68, 0x08, 0x58, 0xC1],
        &[0x62, 0xF1, 0xEC, 0x08, 0x58, 0xC1],
        &[0x62, 0xF1, 0x6D, 0x08, 0x58, 0xC1],
        &[0x62, 0xF1, 0x6C, 0x08, 0x58, 0x01],
        &[0x62, 0xF1, 0x6C, 0x08, 0x51, 0xC1],
        &[0x62, 0xF1, 0x6C, 0x08, 0x58, 0xC1, 0],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp_arithmetic_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_provenance() {
    for (bytes, evex) in [
        (&[0xF2, 0x45, 0x0F, 0x5F, 0xCB][..], false),
        (&[0xC4, 0x41, 0x2C, 0x58, 0xCB][..], false),
        (&[0x62, 0xF1, 0x6C, 0x18, 0x58, 0xCB][..], true),
    ] {
        let pc = 0x5858;
        let mut block = SmirBlock::new(BlockId(33), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        let dedicated = if evex {
            x86_evex_fp_replay_spans(&block, &provenance)
        } else {
            x86_legacy_vex_fp_arithmetic_replay_spans(&block, &provenance)
        };
        for spans in [dedicated, x86_native_replay_spans(&block, &provenance)] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
        }
        assert_eq!(
            x86_evex_native_replay_spans(&block, &provenance).is_empty(),
            !evex
        );

        block.push_op(SmirOp::new(OpId(2), pc + bytes.len() as u64, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }
}
