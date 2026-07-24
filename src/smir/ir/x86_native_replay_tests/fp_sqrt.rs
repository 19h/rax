//! Exact source-byte replay classification for EVEX floating-point square root.

use super::*;

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
            (0..=3).flat_map(|ll| [(ll, false), (ll, true)]).collect()
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
fn classifier_covers_10100_legal_control_mask_and_extension_encodings() {
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

    assert_eq!(classified, 10_100);
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

    // Scalar vvvv/V' and every LLIG value are true operands/control bits.
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
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_fp_sqrt_requirements(),
                Some((false, false)),
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
        (SqrtKind::ScalarF32, 3, false),
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
