//! Exact classifier tests for EVEX scalar integer-to-floating-point replay.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestinationFormat {
    F32,
    F64,
    F16,
}

impl DestinationFormat {
    const ALL: [Self; 3] = [Self::F32, Self::F64, Self::F16];

    fn fields(self) -> (u8, u8, bool) {
        match self {
            Self::F32 => (1, 2, false),
            Self::F64 => (1, 3, false),
            Self::F16 => (5, 2, true),
        }
    }
}

fn encoding(
    format: DestinationFormat,
    signed: bool,
    w: bool,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 16);
    let (map, pp, _) = format.fields();
    let mut p0 = 0xF0 | map;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        if w { 0x80 } else { 0 } | ((!merge & 0x0F) << 3) | 0x04 | pp,
        (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if merge & 0x10 == 0 { 0x08 } else { 0 },
        if signed { 0x2A } else { 0x7B },
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn classifier_accepts_exactly_14_400_sampled_legal_register_encodings() {
    let destinations = [0u8, 3, 8, 17, 31];
    let merges = [0u8, 2, 8, 18, 31];
    let sources = [0u8, 3, 8, 12, 13, 15];
    let mut classified = 0usize;

    for format in DestinationFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                for ll in 0..=3 {
                    for embedded_control in [false, true] {
                        for destination in destinations {
                            for merge in merges {
                                for source in sources {
                                    let bytes = encoding(
                                        format,
                                        signed,
                                        w,
                                        ll,
                                        embedded_control,
                                        destination,
                                        merge,
                                        source,
                                    );
                                    assert_eq!(
                                        X86InstructionBytes::new(&bytes)
                                            .unwrap()
                                            .evex_register_scalar_int_to_fp_requires_fp16(),
                                        Some(format.fields().2),
                                        "{format:?} {bytes:02X?}"
                                    );
                                    classified += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 14_400);

    // Independently assembled by LLVM 21.1.8. Collectively these exercise all
    // six mnemonics, W0/W1, destination XMM16-31, merge XMM16-31, and GPR8-15.
    for (bytes, needs_fp16) in [
        ([0x62, 0x41, 0x6E, 0x00, 0x2A, 0xCA], false),
        ([0x62, 0x41, 0xEE, 0x00, 0x2A, 0xCA], false),
        ([0x62, 0x41, 0x67, 0x00, 0x2A, 0xD3], false),
        ([0x62, 0x41, 0xE7, 0x00, 0x2A, 0xD3], false),
        ([0x62, 0x45, 0x5E, 0x00, 0x2A, 0xDC], true),
        ([0x62, 0x45, 0xDE, 0x00, 0x2A, 0xDC], true),
        ([0x62, 0x41, 0x56, 0x00, 0x7B, 0xE5], false),
        ([0x62, 0x41, 0xD6, 0x00, 0x7B, 0xE5], false),
        ([0x62, 0x41, 0x4F, 0x00, 0x7B, 0xEE], false),
        ([0x62, 0x41, 0xCF, 0x00, 0x7B, 0xEE], false),
        ([0x62, 0x45, 0x46, 0x00, 0x7B, 0xF7], true),
        ([0x62, 0x45, 0xC6, 0x00, 0x7B, 0xF7], true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            Some(needs_fp16),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_reserved_or_unsafe_frontier() {
    let canonical = encoding(DestinationFormat::F32, true, false, 0, false, 17, 18, 10);
    let mut invalid = vec![
        [
            0x61,
            canonical[1],
            canonical[2],
            canonical[3],
            canonical[4],
            canonical[5],
        ]
        .to_vec(),
        canonical[..5].to_vec(),
        canonical.iter().copied().chain([0xA5]).collect(),
    ];
    for (index, value) in [
        (1, canonical[1] & !0x40), // Fabricated source GPR bit 4 through EVEX.X.
        (2, canonical[2] & !0x04), // Missing EVEX fixed-one bit.
        (3, canonical[3] | 0x80),  // Zeroing is reserved.
        (3, canonical[3] | 0x01),  // Opmask is reserved.
        (4, 0x2D),                 // Neighboring conversion opcode.
        (5, canonical[5] & 0x3F),  // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for source in [4, 5] {
        invalid
            .push(encoding(DestinationFormat::F64, false, true, 3, true, 31, 30, source).to_vec());
    }

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }

    for (map, pp) in [
        (0, 2),
        (1, 0),
        (1, 1),
        (2, 2),
        (3, 2),
        (5, 0),
        (5, 1),
        (5, 3),
        (6, 2),
        (9, 2),
    ] {
        let bytes = [0x62, 0xF0 | map, 0x6C | pp, 0x08, 0x2A, 0xC8];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }

    for source in [12, 13] {
        let bytes = encoding(DestinationFormat::F16, false, true, 2, true, 31, 30, source);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_int_to_fp_requires_fp16(),
            Some(true),
            "R{source} must remain safe: {bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_expose_exact_fp16_requirements() {
    let pc = 0x2A7B;
    let mut block = SmirBlock::new(BlockId(123), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for format in DestinationFormat::ALL {
        for signed in [false, true] {
            for w in [false, true] {
                for ll in 0..=3 {
                    for embedded_control in [false, true] {
                        let bytes = encoding(format, signed, w, ll, embedded_control, 31, 30, 15);
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        let provenance =
                            std::collections::HashMap::from([((BlockId(123), pc), instruction)]);
                        for spans in [
                            x86_evex_scalar_int_to_fp_replay_spans(&block, &provenance),
                            x86_evex_native_replay_spans(&block, &provenance),
                        ] {
                            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(span.end, 1, "{bytes:02X?}");
                            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                            assert!(!span.needs_avx512vl, "{bytes:02X?}");
                            assert!(!span.needs_avx512dq, "{bytes:02X?}");
                            assert_eq!(span.needs_avx512fp16, format.fields().2, "{bytes:02X?}");
                        }
                    }
                }
            }
        }
    }
}
