//! Exact classifier tests for register-only EVEX FP32/FP64 conversions.

use super::*;

#[derive(Clone, Copy, Debug)]
enum ConvertKind {
    Widen,
    Narrow,
}

impl ConvertKind {
    const ALL: [Self; 2] = [Self::Widen, Self::Narrow];

    fn p1(self) -> u8 {
        match self {
            Self::Widen => 0x7C,
            Self::Narrow => 0xFD,
        }
    }
}

fn encoding(
    kind: ConvertKind,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    ll: u8,
    embedded_control: bool,
) -> [u8; 6] {
    assert!(destination < 32 && source < 32 && mask < 8 && ll < 4);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        kind.p1(),
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | 0x08
            | mask,
        0x5A,
        0xC0 | ((destination & 7) << 3) | (source & 7),
    ]
}

#[test]
fn classifier_covers_all_215040_legal_register_encodings() {
    let mut classified = 0usize;
    for kind in ConvertKind::ALL {
        for destination in 0..32 {
            for source in 0..32 {
                for mask in 0..8 {
                    for zeroing in [false, true] {
                        if zeroing && mask == 0 {
                            continue;
                        }
                        for (ll, embedded_control) in [
                            (0, false),
                            (1, false),
                            (2, false),
                            (0, true),
                            (1, true),
                            (2, true),
                            (3, true),
                        ] {
                            let bytes = encoding(
                                kind,
                                destination,
                                source,
                                mask,
                                zeroing,
                                ll,
                                embedded_control,
                            );
                            let expected = Some(!embedded_control && matches!(ll, 0 | 1));
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_register_fp32_fp64_convert_needs_vl(),
                                expected,
                                "{kind:?} {bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 215_040);
}

#[test]
fn classifier_accepts_independently_assembled_samples_and_rejects_frontiers() {
    // Independently assembled by LLVM 23.0.0.
    for (bytes, expected_vl) in [
        ([0x62, 0xF1, 0x7C, 0x09, 0x5A, 0xCA], true),
        ([0x62, 0x51, 0x7C, 0xAA, 0x5A, 0xCA], true),
        ([0x62, 0xA1, 0x7C, 0x4B, 0x5A, 0xCA], false),
        ([0x62, 0x01, 0x7C, 0x9F, 0x5A, 0xFE], false),
        ([0x62, 0xF1, 0xFD, 0x09, 0x5A, 0xCA], true),
        ([0x62, 0x51, 0xFD, 0xAA, 0x5A, 0xCA], true),
        ([0x62, 0xA1, 0xFD, 0x4B, 0x5A, 0xCA], false),
        ([0x62, 0x01, 0xFD, 0xFF, 0x5A, 0xFE], false),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp32_fp64_convert_needs_vl(),
            Some(expected_vl),
            "{bytes:02X?}"
        );
    }

    let canonical = encoding(ConvertKind::Widen, 17, 18, 1, false, 0, false);
    let mut invalid = vec![
        vec![
            0x61,
            canonical[1],
            canonical[2],
            canonical[3],
            canonical[4],
            canonical[5],
        ],
        canonical[..5].to_vec(),
        canonical.into_iter().chain([0]).collect::<Vec<_>>(),
        encoding(ConvertKind::Widen, 1, 2, 0, false, 3, false).to_vec(),
        encoding(ConvertKind::Narrow, 1, 2, 0, false, 3, false).to_vec(),
    ];
    for (index, value) in [
        (1, canonical[1] & !0x01), // MAP0, not MAP1.
        (1, canonical[1] | 0x08),  // Reserved EVEX.P0 bit 3.
        (2, canonical[2] & !0x04), // Missing EVEX fixed-one bit.
        (2, canonical[2] & !0x08), // Reserved vvvv.
        (3, canonical[3] & !0x08), // Reserved V'.
        (3, 0x88),                 // Zeroing with k0.
        (4, 0x5B),                 // Unrelated opcode.
        (5, canonical[5] & 0x3F),  // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }

    // Each neighboring mandatory-prefix/W tuple must remain disjoint.
    for p1 in [0x7D, 0xFC, 0x7E, 0xFE] {
        let mut bytes = canonical;
        bytes[2] = p1;
        invalid.push(bytes.to_vec());
    }

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp32_fp64_convert_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_expose_exact_vl_requirements_without_dq_or_fp16() {
    let pc = 0x5A00;
    let mut block = SmirBlock::new(BlockId(62), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for kind in ConvertKind::ALL {
        for (ll, embedded_control) in [
            (0, false),
            (1, false),
            (2, false),
            (0, true),
            (1, true),
            (2, true),
            (3, true),
        ] {
            let bytes = encoding(kind, 31, 30, 7, true, ll, embedded_control);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let provenance = HashMap::from([((BlockId(62), pc), instruction)]);
            let expected_vl = !embedded_control && ll != 2;
            for spans in [
                x86_evex_fp32_fp64_convert_replay_spans(&block, &provenance),
                x86_evex_native_replay_spans(&block, &provenance),
            ] {
                let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                assert_eq!(span.end, 1, "{bytes:02X?}");
                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                assert_eq!(span.needs_avx512vl, expected_vl, "{bytes:02X?}");
                assert!(!span.needs_avx512dq, "{bytes:02X?}");
                assert!(!span.needs_avx512fp16, "{bytes:02X?}");
                assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
            }
        }
    }
}
