//! Exact source-byte replay classification for register-only legacy SSE3 and
//! AVX VEX packed floating-point `HADD`/`HSUB`/`ADDSUB`.

use super::*;

const OPCODES: [u8; 3] = [0x7C, 0x7D, 0xD0];

fn legacy_encoding(prefix: u8, rex: Option<u8>, opcode: u8, modrm: u8) -> Vec<u8> {
    assert!(matches!(prefix, 0x66 | 0xF2));
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![prefix];
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
fn classifier_covers_all_227_712_safe_canonical_register_encodings() {
    let mut classified = 0usize;

    for prefix in [0x66, 0xF2] {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for opcode in OPCODES {
                for modrm in 0xC0..=0xFF {
                    let bytes = legacy_encoding(prefix, rex, opcode, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
                        Some(false),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }

    for p1 in u8::MIN..=u8::MAX {
        if !matches!(p1 & 0x03, 1 | 3) {
            continue;
        }
        for opcode in OPCODES {
            for modrm in 0xC0..=0xFF {
                let bytes = vex_c5_encoding(p1, opcode, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
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
            if !matches!(p1 & 0x03, 1 | 3) {
                continue;
            }
            for opcode in OPCODES {
                for modrm in 0xC0..=0xFF {
                    let bytes = vex_c4_encoding(p0, p1, opcode, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
                        Some(true),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }

    assert_eq!(classified, 227_712);
}

#[test]
fn classifier_exhausts_prefix_maps_w_l_vvvv_opcodes_and_modrm_modes() {
    for prefix in u8::MIN..=u8::MAX {
        let bytes = [prefix, 0x0F, 0x7C, 0xCB];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
            match prefix {
                0x66 | 0xF2 => Some(false),
                // This four-byte sequence is independently a valid C5 VEX
                // VHADDPS encoding, not a legacy-prefix shape.
                0xC5 => Some(true),
                _ => None,
            },
            "{bytes:02X?}"
        );
    }

    for p0 in u8::MIN..=u8::MAX {
        let bytes = vex_c4_encoding(p0, 0xED, 0x7D, 0xF4);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
            (p0 & 0x1F == 1).then_some(true),
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let expected = matches!(p1 & 0x03, 1 | 3).then_some(true);
        for bytes in [
            vex_c5_encoding(p1, 0xD0, 0xC1).to_vec(),
            vex_c4_encoding(0x21, p1, 0x7C, 0xFE).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for opcode in u8::MIN..=u8::MAX {
        let valid = OPCODES.contains(&opcode);
        for bytes in [
            legacy_encoding(0x66, Some(0x4F), opcode, 0xFF),
            legacy_encoding(0xF2, None, opcode, 0xC1),
            vex_c5_encoding(0xEF, opcode, 0xD2).to_vec(),
            vex_c4_encoding(0x41, 0x6D, opcode, 0xE3).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_horizontal_addsub_needs_avx()
                    .is_some(),
                valid,
                "{bytes:02X?}"
            );
        }
    }

    for modrm in u8::MIN..=u8::MAX {
        let expected_legacy = (modrm >> 6 == 3).then_some(false);
        let expected_vex = (modrm >> 6 == 3).then_some(true);
        for bytes in [
            legacy_encoding(0x66, None, 0x7C, modrm),
            legacy_encoding(0xF2, Some(0x4A), 0xD0, modrm),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
                expected_legacy,
                "{bytes:02X?}"
            );
        }
        for bytes in [
            vex_c5_encoding(0xEF, 0x7D, modrm).to_vec(),
            vex_c4_encoding(0x21, 0x6D, 0xD0, modrm).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
                expected_vex,
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn classifier_rejects_noncanonical_prefixes_lengths_evex_and_memory() {
    let invalid: &[&[u8]] = &[
        &[0x0F, 0x7C, 0xC1],
        &[0xF3, 0x0F, 0x7D, 0xC1],
        &[0x66, 0xF2, 0x0F, 0xD0, 0xC1],
        &[0x48, 0x66, 0x0F, 0x7C, 0xC1],
        &[0x66, 0x0F, 0x7C],
        &[0xF2, 0x0F, 0x7D, 0xC1, 0],
        &[0xC5, 0xEF, 0xD0],
        &[0xC5, 0xEF, 0xD0, 0xC1, 0],
        &[0xC4, 0xE1, 0x6D, 0x7C],
        &[0xC4, 0xE1, 0x6D, 0x7C, 0xC1, 0],
        &[0xC4, 0xE2, 0x6D, 0x7C, 0xC1],
        &[0x66, 0x0F, 0x7C, 0x01],
        &[0xC5, 0xEF, 0x7D, 0x41],
        &[0x62, 0xF1, 0x6D, 0x08, 0x7C, 0xCB],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_fp_horizontal_addsub_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_preserve_exact_bytes_and_remain_disjoint() {
    let pc = 0x7C7D;
    let mut block = SmirBlock::new(BlockId(38), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_avx) in [
        (legacy_encoding(0x66, Some(0x4D), 0x7C, 0xCB), false),
        (vex_c5_encoding(0x6F, 0x7D, 0xCB).to_vec(), true),
        (vex_c4_encoding(0x41, 0xED, 0xD0, 0xCB).to_vec(), true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.legacy_vex_register_fp_horizontal_addsub_needs_avx(),
            Some(needs_avx),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.legacy_vex_register_fp_arithmetic_needs_avx(),
            None,
            "{bytes:02X?}"
        );
        let provenance = HashMap::from([((BlockId(38), pc), instruction)]);
        for spans in [
            x86_legacy_vex_fp_horizontal_addsub_replay_spans(&block, &provenance),
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
            x86_legacy_vex_fp_arithmetic_replay_spans(&block, &provenance).is_empty(),
            "{bytes:02X?}"
        );
    }
}
