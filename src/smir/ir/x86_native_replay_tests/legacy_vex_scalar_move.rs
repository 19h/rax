//! Exact source-byte replay classification for register-only legacy SSE and
//! AVX VEX scalar floating-point moves.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveKind {
    F32,
    F64,
}

impl MoveKind {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn pp(self) -> u8 {
        match self {
            Self::F32 => 2,
            Self::F64 => 3,
        }
    }

    fn legacy_prefix(self) -> u8 {
        match self {
            Self::F32 => 0xF3,
            Self::F64 => 0xF2,
        }
    }
}

fn legacy_encoding(kind: MoveKind, rex: Option<u8>, opcode: u8, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![kind.legacy_prefix()];
    if let Some(rex) = rex {
        bytes.push(rex);
    }
    bytes.extend([0x0F, opcode, modrm]);
    bytes
}

fn vex_c5_encoding(p1: u8, opcode: u8, modrm: u8) -> [u8; 4] {
    [0xC5, p1, opcode, modrm]
}

fn vex_c4_encoding(p0: u8, p1: u8, opcode: u8, modrm: u8) -> [u8; 5] {
    [0xC4, p0, p1, opcode, modrm]
}

#[test]
fn classifier_covers_all_114_944_safe_canonical_legacy_and_vex_register_encodings() {
    let mut classified = 0usize;

    for kind in MoveKind::ALL {
        for opcode in [0x10, 0x11] {
            for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
                for modrm in 0xC0..=0xFF {
                    let bytes = legacy_encoding(kind, rex, opcode, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_scalar_move_needs_avx(),
                        Some(false),
                        "{kind:?} {bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }

    for p1 in 0u8..=u8::MAX {
        let pp = p1 & 0x03;
        let expected = (matches!(pp, 2 | 3) && !(pp == 2 && p1 & 0x04 != 0)).then_some(true);
        for opcode in [0x10, 0x11] {
            for modrm in 0xC0..=0xFF {
                let bytes = vex_c5_encoding(p1, opcode, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_scalar_move_needs_avx(),
                    expected,
                    "{bytes:02X?}"
                );
                classified += usize::from(expected.is_some());
            }
        }
    }

    for extension_bits in 0u8..=7 {
        let p0 = (extension_bits << 5) | 1;
        for p1 in 0u8..=u8::MAX {
            let pp = p1 & 0x03;
            let expected = (matches!(pp, 2 | 3) && !(pp == 2 && p1 & 0x04 != 0)).then_some(true);
            for opcode in [0x10, 0x11] {
                for modrm in 0xC0..=0xFF {
                    let bytes = vex_c4_encoding(p0, p1, opcode, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_scalar_move_needs_avx(),
                        expected,
                        "{bytes:02X?}"
                    );
                    classified += usize::from(expected.is_some());
                }
            }
        }
    }

    assert_eq!(classified, 114_944);
}

#[test]
fn classifier_rejects_memory_wrong_map_and_noncanonical_bytes() {
    for kind in MoveKind::ALL {
        for opcode in [0x10, 0x11] {
            for rex in [None, Some(0x40), Some(0x4F)] {
                for modrm in [0x00, 0x45, 0x84, 0xBF] {
                    let bytes = legacy_encoding(kind, rex, opcode, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_scalar_move_needs_avx(),
                        None,
                        "{bytes:02X?}"
                    );
                }
            }
        }
    }

    for bytes in [
        vex_c5_encoding(0xFA, 0x10, 0x00).to_vec(),
        vex_c5_encoding(0xFB, 0x11, 0xBF).to_vec(),
        vex_c4_encoding(0xE1, 0xFA, 0x10, 0x00).to_vec(),
        vex_c4_encoding(0x01, 0xFB, 0x11, 0xBF).to_vec(),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_scalar_move_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        if matches!(opcode, 0x10 | 0x11) {
            continue;
        }
        for bytes in [
            legacy_encoding(MoveKind::F32, Some(0x4F), opcode, 0xFF),
            vex_c5_encoding(0xFA, opcode, 0xFF).to_vec(),
            vex_c4_encoding(0x01, 0xFB, opcode, 0xFF).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_scalar_move_needs_avx(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    for map in 0u8..=31 {
        if map == 1 {
            continue;
        }
        let bytes = vex_c4_encoding(0xE0 | map, 0xFA, 0x10, 0xC0);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_scalar_move_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x10, 0xC0],
        &[0x66, 0x0F, 0x10, 0xC0],
        &[0xF2, 0xF3, 0x0F, 0x10, 0xC0],
        &[0x48, 0xF3, 0x0F, 0x10, 0xC0],
        &[0x67, 0xF3, 0x0F, 0x10, 0xC0],
        &[0xC5, 0xFA, 0x10],
        &[0xC5, 0xFA, 0x10, 0xC0, 0],
        &[0xC4, 0xE1, 0xFA, 0x10],
        &[0xC4, 0xE1, 0xFA, 0x10, 0xC0, 0],
        &[0x62, 0xF1, 0x7E, 0x08, 0x10, 0xC0],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_scalar_move_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_keeps_unpredictable_vmovss_l1_out_but_accepts_vmovsd_lig() {
    for opcode in [0x10, 0x11] {
        // P1 bit 7 is inverted R in C5 encodings and W in C4 encodings; both
        // values are legal independently of the vector-length rule.
        for p1_bit7 in [0, 0x80] {
            for encoded_vvvv in [0, 5 << 3, 0x78] {
                for bytes in [
                    vex_c5_encoding(p1_bit7 | encoded_vvvv | 0x06, opcode, 0xC8).to_vec(),
                    vex_c4_encoding(0x41, p1_bit7 | encoded_vvvv | 0x06, opcode, 0xC8).to_vec(),
                ] {
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_scalar_move_needs_avx(),
                        None,
                        "VMOVSS VEX.L=1 must fail closed: {bytes:02X?}"
                    );
                }
                for bytes in [
                    vex_c5_encoding(p1_bit7 | encoded_vvvv | 0x07, opcode, 0xC8).to_vec(),
                    vex_c4_encoding(0x41, p1_bit7 | encoded_vvvv | 0x07, opcode, 0xC8).to_vec(),
                ] {
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_scalar_move_needs_avx(),
                        Some(true),
                        "VMOVSD is VEX.LIG: {bytes:02X?}"
                    );
                }
            }
        }
    }
}

#[test]
fn replay_spans_preserve_exact_bytes_and_keep_non_evex_families_disjoint() {
    let pc = 0x1011;
    let mut block = SmirBlock::new(BlockId(35), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_avx) in [
        (
            legacy_encoding(MoveKind::F64, Some(0x4D), 0x11, 0xCB),
            false,
        ),
        (vex_c5_encoding(0x6A, 0x10, 0xCB).to_vec(), true),
        (vex_c4_encoding(0x41, 0xEF, 0x11, 0xCB).to_vec(), true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.legacy_vex_register_scalar_move_needs_avx(),
            Some(needs_avx),
            "{bytes:02X?}"
        );
        let provenance = HashMap::from([((BlockId(35), pc), instruction)]);
        for spans in [
            x86_legacy_vex_scalar_move_replay_spans(&block, &provenance),
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
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());
    }

    assert!(x86_native_replay_spans(&block, &HashMap::new()).is_empty());
}
