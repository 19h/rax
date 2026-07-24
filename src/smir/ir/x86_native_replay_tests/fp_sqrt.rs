//! Exact source-byte replay classification for EVEX floating-point square root.

use super::*;

fn legacy_sqrt_encoding(mandatory_prefix: u8, rex: Option<u8>, modrm: u8) -> Vec<u8> {
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
    bytes.extend([0x0F, 0x51, modrm]);
    bytes
}

fn c4_sqrt_encoding(
    extension_bits: u8,
    w: bool,
    encoded_vvvv: u8,
    l: bool,
    pp: u8,
    modrm: u8,
) -> [u8; 5] {
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(encoded_vvvv < 16 && pp < 4);
    [
        0xC4,
        extension_bits | 1,
        (if w { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (if l { 0x04 } else { 0 }) | pp,
        0x51,
        modrm,
    ]
}

fn c5_sqrt_encoding(encoded_r: bool, encoded_vvvv: u8, l: bool, pp: u8, modrm: u8) -> [u8; 4] {
    assert!(encoded_vvvv < 16 && pp < 4);
    [
        0xC5,
        (if encoded_r { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (if l { 0x04 } else { 0 }) | pp,
        0x51,
        modrm,
    ]
}

#[test]
fn non_evex_classifier_exhaustively_accepts_45_824_safe_register_encodings() {
    let mut accepted = 0usize;
    let mut tested = 0usize;

    for mandatory_prefix in 0u8..4 {
        for rex in std::iter::once(None).chain((0x40u8..=0x4F).map(Some)) {
            for reg_rm in 0u8..=0x3F {
                let bytes = legacy_sqrt_encoding(mandatory_prefix, rex, 0xC0 | reg_rm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_vex_register_fp_sqrt_needs_avx(),
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
                            let bytes = c4_sqrt_encoding(
                                extension_bits,
                                w,
                                encoded_vvvv,
                                l,
                                pp,
                                0xC0 | reg_rm,
                            );
                            let expected = if pp <= 1 {
                                (encoded_vvvv == 0x0F).then_some(true)
                            } else {
                                (!l).then_some(true)
                            };
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .legacy_vex_register_fp_sqrt_needs_avx(),
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
                        let bytes = c5_sqrt_encoding(encoded_r, encoded_vvvv, l, pp, 0xC0 | reg_rm);
                        let expected = if pp <= 1 {
                            (encoded_vvvv == 0x0F).then_some(true)
                        } else {
                            (!l).then_some(true)
                        };
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .legacy_vex_register_fp_sqrt_needs_avx(),
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

    assert_eq!(accepted, 45_824);
    assert_eq!(tested, 151_808);

    // Independently assembled by LLVM 21.1.8.
    for (bytes, needs_avx) in [
        (&[0x0F, 0x51, 0xCB][..], false),             // sqrtps xmm1,xmm3
        (&[0xF2, 0x45, 0x0F, 0x51, 0xCB][..], false), // sqrtsd xmm9,xmm11
        (&[0xC4, 0x41, 0x7C, 0x51, 0xCB][..], true),  // vsqrtps ymm9,ymm11
        (&[0xC4, 0x41, 0x2A, 0x51, 0xCB][..], true),  // vsqrtss xmm9,xmm10,xmm11
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_fp_sqrt_needs_avx(),
            Some(needs_avx),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn non_evex_classifier_rejects_structural_and_unpredictable_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0xF0, 0x0F, 0x51, 0xC1],       // LOCK prefix
        &[0x66, 0xF3, 0x0F, 0x51, 0xC1], // multiple mandatory prefixes
        &[0x40, 0x66, 0x0F, 0x51, 0xC1], // REX is not last
        &[0x0F, 0x51, 0x01],             // legacy memory source
        &[0x0F, 0x50, 0xC1],             // wrong opcode
        &[0x0F, 0x51, 0xC1, 0],          // trailing byte
        &[0xC4, 0xE2, 0x78, 0x51, 0xC1], // VEX map 0F38
        &[0xC4, 0xE1, 0x70, 0x51, 0xC1], // packed reserved vvvv
        &[0xC4, 0xE1, 0x7A, 0x51, 0x01], // VEX memory source
        &[0xC4, 0xE1, 0x7E, 0x51, 0xC1], // scalar VEX.L=1
        &[0xC5, 0x74, 0x51, 0xC1],       // packed reserved vvvv
        &[0xC5, 0xFE, 0x51, 0xC1],       // scalar VEX.L=1
        &[0xC5, 0xF8, 0x50, 0xC1],       // wrong opcode
        &[0xC5, 0xF8, 0x51, 0xC1, 0],    // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_vex_register_fp_sqrt_needs_avx(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn non_evex_dedicated_and_aggregate_spans_require_exact_provenance() {
    for bytes in [
        &[0xF2, 0x45, 0x0F, 0x51, 0xCB][..],
        &[0xC4, 0x41, 0x2A, 0x51, 0xCB][..],
    ] {
        let pc = 0x5151;
        let mut block = SmirBlock::new(BlockId(32), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_legacy_vex_fp_sqrt_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
        }
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + bytes.len() as u64, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SqrtKind {
    PackedF16,
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl SqrtKind {
    const ALL: [Self; 5] = [
        Self::PackedF16,
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn fields(self) -> (u8, u8, bool, bool, bool) {
        match self {
            Self::PackedF16 => (5, 0, false, false, true),
            Self::PackedF32 => (1, 0, false, false, false),
            Self::PackedF64 => (1, 1, true, false, false),
            Self::ScalarF32 => (1, 2, false, true, false),
            Self::ScalarF64 => (1, 3, true, true, false),
        }
    }

    fn controls(self) -> Vec<(u8, bool)> {
        let scalar = self.fields().3;
        if scalar {
            (0..=2)
                .flat_map(|ll| [(ll, false), (ll, true)])
                .chain([(3, true)])
                .collect()
        } else {
            (0..=2)
                .map(|ll| (ll, false))
                .chain((0..=3).map(|ll| (ll, true)))
                .collect()
        }
    }
}

fn encoding(
    kind: SqrtKind,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (map, pp, w, scalar, _) = kind.fields();
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 32 && mask < 8);
    assert!(scalar || merge == 0);
    assert!(scalar || embedded_control || ll < 3);
    assert!(!zeroing || mask != 0);

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
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    let encoded_merge = if scalar { merge } else { 0 };
    [
        0x62,
        p0,
        (((!encoded_merge) & 0x0F) << 3) | 0x04 | pp | if w { 0x80 } else { 0 },
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if encoded_merge < 16 { 0x08 } else { 0 }
            | mask,
        0x51,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn requirements(kind: SqrtKind, ll: u8, embedded_control: bool) -> (bool, bool) {
    let (_, _, _, scalar, fp16) = kind.fields();
    (!scalar && !embedded_control && ll != 2, fp16)
}

#[test]
fn classifier_covers_9100_legal_control_mask_and_extension_encodings() {
    let registers = [0u8, 8, 16, 24, 31];
    let masks = [(0u8, false), (1, false), (1, true), (7, true)];
    let mut classified = 0usize;

    for kind in SqrtKind::ALL {
        let scalar = kind.fields().3;
        for (ll, embedded_control) in kind.controls() {
            for destination in registers {
                for source in registers {
                    for merge in if scalar { registers.as_slice() } else { &[0] } {
                        for (mask, zeroing) in masks {
                            let bytes = encoding(
                                kind,
                                ll,
                                embedded_control,
                                destination,
                                *merge,
                                source,
                                mask,
                                zeroing,
                            );
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_register_fp_sqrt_requirements(),
                                Some(requirements(kind, ll, embedded_control)),
                                "{kind:?} {bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }

    assert_eq!(classified, 9_100);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let packed = encoding(SqrtKind::PackedF32, 2, false, 17, 0, 24, 1, false);
    let scalar = encoding(SqrtKind::ScalarF64, 3, true, 17, 18, 24, 1, false);
    let invalid: &[&[u8]] = &[
        &[0x61, 0x81, 0x7C, 0x49, 0x51, 0xC8],    // not EVEX
        &[0x62, 0x82, 0x7C, 0x49, 0x51, 0xC8],    // wrong map
        &[0x62, 0x89, 0x7C, 0x49, 0x51, 0xC8],    // reserved P0 bit 3
        &[0x62, 0x81, 0x78, 0x49, 0x51, 0xC8],    // missing P1 fixed-one bit
        &[0x62, 0x81, 0x7D, 0x49, 0x51, 0xC8],    // wrong pp for W0
        &[0x62, 0x81, 0xFC, 0x49, 0x51, 0xC8],    // wrong W for packed F32
        &[0x62, 0x85, 0x7D, 0x49, 0x51, 0xC8],    // wrong pp for packed F16
        &[0x62, 0x85, 0xFC, 0x49, 0x51, 0xC8],    // wrong W for packed F16
        &[0x62, 0x85, 0x7E, 0x49, 0x51, 0xC8],    // VSQRTSH belongs to scalar FP16 classifier
        &[0x62, 0x81, 0x7C, 0x49, 0x50, 0xC8],    // wrong opcode
        &[0x62, 0x81, 0x7C, 0x49, 0x51, 0x08],    // memory source
        &[0x62, 0x81, 0x7C, 0xC8, 0x51, 0xC8],    // zeroing with k0
        &[0x62, 0x81, 0x7C, 0x69, 0x51, 0xC8],    // packed L'L=3 without EVEX.b
        &[0x62, 0x81, 0x7C, 0x49, 0x51],          // missing ModR/M
        &[0x62, 0x81, 0x7C, 0x49, 0x51, 0xC8, 0], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp_sqrt_requirements(),
            None,
            "{bytes:02X?}"
        );
    }

    for encoded_vvvv in 0u8..=0x0E {
        let mut reserved = packed;
        reserved[2] = (reserved[2] & !0x78) | (encoded_vvvv << 3);
        assert_eq!(
            X86InstructionBytes::new(&reserved)
                .unwrap()
                .evex_register_fp_sqrt_requirements(),
            None,
            "{reserved:02X?}"
        );
    }
    let mut reserved_v_prime = packed;
    reserved_v_prime[3] &= !0x08;
    assert_eq!(
        X86InstructionBytes::new(&reserved_v_prime)
            .unwrap()
            .evex_register_fp_sqrt_requirements(),
        None
    );

    // Scalar vvvv/V' and all three defined LLIG values are true operands.
    // Embedded rounding repurposes L'L and thereby admits all four values.
    for ll in 0..=3 {
        for embedded_control in [false, true] {
            let bytes = encoding(
                SqrtKind::ScalarF64,
                ll,
                embedded_control,
                31,
                31,
                31,
                7,
                true,
            );
            let expected = (embedded_control || ll != 3).then_some((false, false));
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_fp_sqrt_requirements(),
                expected,
                "{bytes:02X?}"
            );
        }
    }
    assert_eq!(
        X86InstructionBytes::new(&scalar)
            .unwrap()
            .evex_register_fp_sqrt_requirements(),
        Some((false, false))
    );
}

#[test]
fn replay_spans_encode_exact_vl_and_fp16_requirements() {
    let pc = 0x5100;
    let mut block = SmirBlock::new(BlockId(31), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (kind, ll, embedded_control) in [
        (SqrtKind::PackedF16, 0, false),
        (SqrtKind::PackedF32, 1, false),
        (SqrtKind::PackedF64, 2, false),
        (SqrtKind::PackedF64, 3, true),
        (SqrtKind::ScalarF32, 2, false),
        (SqrtKind::ScalarF64, 2, true),
    ] {
        let merge = if kind.fields().3 { 18 } else { 0 };
        let bytes = encoding(kind, ll, embedded_control, 17, merge, 24, 1, false);
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(31), pc), instruction)]);
        let expected = requirements(kind, ll, embedded_control);
        for spans in [
            x86_evex_fp_sqrt_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, expected.0, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert_eq!(span.needs_avx512fp16, expected.1, "{bytes:02X?}");
        }
    }
}
