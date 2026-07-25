//! Exact source-byte replay classification for register-only legacy SSE and
//! AVX VEX `MOVHLPS`/`MOVLHPS`.

use super::*;

fn legacy_encoding(rex: Option<u8>, opcode: u8, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    bytes.extend(rex);
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
fn classifier_covers_all_39_040_safe_canonical_register_encodings() {
    let mut classified = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for opcode in [0x12, 0x16] {
            for modrm in 0xC0..=0xFF {
                let bytes = legacy_encoding(rex, opcode, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_high_low_move_needs_avx(),
                    Some(false),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }

    for p1 in u8::MIN..=u8::MAX {
        if p1 & 0x07 != 0 {
            continue;
        }
        for opcode in [0x12, 0x16] {
            for modrm in 0xC0..=0xFF {
                let bytes = vex_c5_encoding(p1, opcode, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_high_low_move_needs_avx(),
                    Some(true),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }

    for p0 in u8::MIN..=u8::MAX {
        if p0 & 0x1F != 1 {
            continue;
        }
        for p1 in u8::MIN..=u8::MAX {
            if p1 & 0x07 != 0 {
                continue;
            }
            for opcode in [0x12, 0x16] {
                for modrm in 0xC0..=0xFF {
                    let bytes = vex_c4_encoding(p0, p1, opcode, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_high_low_move_needs_avx(),
                        Some(true),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 39_040);
}

#[test]
fn classifier_exhausts_maps_prefix_fields_opcodes_and_modrm_modes() {
    for p1 in u8::MIN..=u8::MAX {
        let bytes = vex_c5_encoding(p1, 0x12, 0xCB);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_high_low_move_needs_avx(),
            (p1 & 0x07 == 0).then_some(true),
            "{bytes:02X?}"
        );
    }

    for p0 in u8::MIN..=u8::MAX {
        for p1 in [0x00, 0x80] {
            let bytes = vex_c4_encoding(p0, p1, 0x16, 0xF4);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_high_low_move_needs_avx(),
                (p0 & 0x1F == 1).then_some(true),
                "{bytes:02X?}"
            );
        }
    }

    for p1 in u8::MIN..=u8::MAX {
        let bytes = vex_c4_encoding(0x41, p1, 0x12, 0xC1);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_high_low_move_needs_avx(),
            (p1 & 0x07 == 0).then_some(true),
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        let expected = matches!(opcode, 0x12 | 0x16);
        for bytes in [
            legacy_encoding(Some(0x4F), opcode, 0xFF),
            vex_c5_encoding(0x68, opcode, 0xE1).to_vec(),
            vex_c4_encoding(0x41, 0xE8, opcode, 0xD2).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_high_low_move_needs_avx()
                    .is_some(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for modrm in u8::MIN..=u8::MAX {
        let expected_legacy = (modrm >> 6 == 3).then_some(false);
        let expected_vex = (modrm >> 6 == 3).then_some(true);
        for bytes in [
            legacy_encoding(None, 0x12, modrm),
            legacy_encoding(Some(0x48), 0x16, modrm),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_high_low_move_needs_avx(),
                expected_legacy,
                "{bytes:02X?}"
            );
        }
        for bytes in [
            vex_c5_encoding(0xE8, 0x12, modrm).to_vec(),
            vex_c4_encoding(0x21, 0x60, 0x16, modrm).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_high_low_move_needs_avx(),
                expected_vex,
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn classifier_rejects_noncanonical_prefixes_lengths_and_evex() {
    let invalid: &[&[u8]] = &[
        &[0x66, 0x0F, 0x12, 0xC1],
        &[0xF2, 0x0F, 0x16, 0xC1],
        &[0xF3, 0x0F, 0x12, 0xC1],
        &[0x67, 0x0F, 0x16, 0xC1],
        &[0x48, 0x66, 0x0F, 0x12, 0xC1],
        &[0x0F, 0x12],
        &[0x0F, 0x12, 0xC1, 0],
        &[0xC5, 0xE8, 0x12],
        &[0xC5, 0xE8, 0x12, 0xC1, 0],
        &[0xC4, 0xE1, 0x68, 0x16],
        &[0xC4, 0xE1, 0x68, 0x16, 0xC1, 0],
        &[0x62, 0xF1, 0x6C, 0x08, 0x12, 0xCB],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_high_low_move_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_preserve_exact_bytes_and_keep_evex_disjoint() {
    let pc = 0x1014;
    let mut block = SmirBlock::new(BlockId(36), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_avx) in [
        (legacy_encoding(Some(0x4D), 0x12, 0xCB), false),
        (vex_c5_encoding(0x68, 0x16, 0xCB).to_vec(), true),
        (vex_c4_encoding(0x41, 0xE8, 0x12, 0xCB).to_vec(), true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.legacy_vex_register_high_low_move_needs_avx(),
            Some(needs_avx),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.evex_register_high_low_move_needs_vl(),
            None,
            "{bytes:02X?}"
        );
        let provenance = HashMap::from([((BlockId(36), pc), instruction)]);
        for spans in [
            x86_legacy_vex_high_low_move_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
            assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
        }
        assert!(
            x86_evex_high_low_move_replay_spans(&block, &provenance).is_empty(),
            "{bytes:02X?}"
        );
    }
}
