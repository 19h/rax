//! Exact source-byte replay classification for legacy SSE and AVX VEX
//! floating-point compares.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompareKind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl CompareKind {
    const ALL: [Self; 4] = [
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn pp(self) -> u8 {
        match self {
            Self::PackedF32 => 0,
            Self::PackedF64 => 1,
            Self::ScalarF32 => 2,
            Self::ScalarF64 => 3,
        }
    }

    fn legacy_prefix(self) -> Option<u8> {
        match self {
            Self::PackedF32 => None,
            Self::PackedF64 => Some(0x66),
            Self::ScalarF32 => Some(0xF3),
            Self::ScalarF64 => Some(0xF2),
        }
    }
}

fn legacy_encoding(kind: CompareKind, rex: Option<u8>, modrm: u8, predicate: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = Vec::new();
    if let Some(prefix) = kind.legacy_prefix() {
        bytes.push(prefix);
    }
    if let Some(rex) = rex {
        bytes.push(rex);
    }
    bytes.extend([0x0F, 0xC2, modrm, predicate]);
    bytes
}

fn vex_c5_encoding(p1: u8, modrm: u8, predicate: u8) -> [u8; 5] {
    [0xC5, p1, 0xC2, modrm, predicate]
}

fn vex_c4_encoding(p0: u8, p1: u8, modrm: u8, predicate: u8) -> [u8; 6] {
    [0xC4, p0, p1, 0xC2, modrm, predicate]
}

#[test]
fn classifier_covers_all_3_573_760_safe_canonical_legacy_and_vex_register_encodings() {
    let mut classified = 0usize;

    for kind in CompareKind::ALL {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                for predicate in 0..=7 {
                    let bytes = legacy_encoding(kind, rex, modrm, predicate);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_compare_needs_avx(),
                        Some(false),
                        "{kind:?} {bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }

    for p1 in 0u8..=u8::MAX {
        let expected = (!matches!(p1 & 0x03, 2 | 3) || p1 & 0x04 == 0).then_some(true);
        for modrm in 0xC0..=0xFF {
            for predicate in 0..=31 {
                let bytes = vex_c5_encoding(p1, modrm, predicate);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_fp_compare_needs_avx(),
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
            let expected = (!matches!(p1 & 0x03, 2 | 3) || p1 & 0x04 == 0).then_some(true);
            for modrm in 0xC0..=0xFF {
                for predicate in 0..=31 {
                    let bytes = vex_c4_encoding(p0, p1, modrm, predicate);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .legacy_vex_register_fp_compare_needs_avx(),
                        expected,
                        "{bytes:02X?}"
                    );
                    classified += usize::from(expected.is_some());
                }
            }
        }
    }

    assert_eq!(classified, 3_573_760);
}

#[test]
fn classifier_rejects_memory_reserved_immediate_wrong_map_and_noncanonical_bytes() {
    for kind in CompareKind::ALL {
        for rex in [None, Some(0x40), Some(0x4F)] {
            for modrm in [0x00, 0x45, 0x84, 0xBF] {
                let bytes = legacy_encoding(kind, rex, modrm, 7);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_fp_compare_needs_avx(),
                    None,
                    "{bytes:02X?}"
                );
            }
        }
        for predicate in 8..=u8::MAX {
            let bytes = legacy_encoding(kind, Some(0x4F), 0xFF, predicate);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_compare_needs_avx(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    for bytes in [
        vex_c5_encoding(0x00, 0x00, 31).to_vec(),
        vex_c5_encoding(0xFF, 0xBF, 31).to_vec(),
        vex_c4_encoding(0x01, 0x00, 0x00, 31).to_vec(),
        vex_c4_encoding(0xE1, 0xFF, 0xBF, 31).to_vec(),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_fp_compare_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }

    for predicate in 32..=u8::MAX {
        for bytes in [
            vex_c5_encoding(0xFF, 0xFF, predicate).to_vec(),
            vex_c4_encoding(0xE1, 0xFF, 0xFF, predicate).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .legacy_vex_register_fp_compare_needs_avx(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    for map in 0u8..=31 {
        if map == 1 {
            continue;
        }
        let bytes = vex_c4_encoding(0xE0 | map, 0x7C, 0xC0, 0);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_vex_register_fp_compare_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0xC3, 0xC0, 0],
        &[0x67, 0x0F, 0xC2, 0xC0, 0],
        &[0x66, 0xF2, 0x0F, 0xC2, 0xC0, 0],
        &[0x48, 0x66, 0x0F, 0xC2, 0xC0, 0],
        &[0xC5, 0x7C, 0xC3, 0xC0, 0],
        &[0xC4, 0xE1, 0x7C, 0xC3, 0xC0, 0],
        &[0x62, 0xF1, 0x7C, 0x08, 0xC2, 0xC0, 0],
        &[0xC5, 0x7C, 0xC2, 0xC0],
        &[0xC5, 0x7C, 0xC2, 0xC0, 0, 0],
        &[0xC4, 0xE1, 0x7C, 0xC2, 0xC0],
        &[0xC4, 0xE1, 0x7C, 0xC2, 0xC0, 0, 0],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_fp_compare_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_preserve_exact_bytes_and_keep_non_evex_families_disjoint() {
    let pc = 0xC240;
    let mut block = SmirBlock::new(BlockId(34), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_avx) in [
        (
            legacy_encoding(CompareKind::ScalarF64, Some(0x4D), 0xCB, 7),
            false,
        ),
        (vex_c5_encoding(0x6A, 0xCB, 31).to_vec(), true),
        (vex_c4_encoding(0x41, 0xEA, 0xCB, 31).to_vec(), true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.legacy_vex_register_fp_compare_needs_avx(),
            Some(needs_avx),
            "{bytes:02X?}"
        );
        let provenance = HashMap::from([((BlockId(34), pc), instruction)]);
        for spans in [
            x86_legacy_vex_fp_compare_replay_spans(&block, &provenance),
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
