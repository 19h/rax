//! Exact classifier tests for EVEX scalar floating-point precision replay.

use super::*;

fn vex_destination(encoded_r: bool, modrm: u8) -> u8 {
    ((modrm >> 3) & 7) | (u8::from(!encoded_r) << 3)
}

#[test]
fn vex_classifier_covers_all_36864_defined_l0_register_byte_images() {
    let mut classified = 0usize;

    for encoded_r in [false, true] {
        for encoded_vvvv in 0u8..16 {
            for pp in [2u8, 3] {
                let p1 = (u8::from(encoded_r) << 7) | (encoded_vvvv << 3) | pp;
                for modrm in 0xC0u8..=0xFF {
                    let bytes = [0xC5, p1, 0x5A, modrm];
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_scalar_fp_convert_destination_index(),
                        Some(vex_destination(encoded_r, modrm)),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }

    for extension_bits in 0u8..8 {
        let p0 = (extension_bits << 5) | 1;
        for w in [false, true] {
            for encoded_vvvv in 0u8..16 {
                for pp in [2u8, 3] {
                    let p1 = (u8::from(w) << 7) | (encoded_vvvv << 3) | pp;
                    for modrm in 0xC0u8..=0xFF {
                        let bytes = [0xC4, p0, p1, 0x5A, modrm];
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .vex_scalar_fp_convert_destination_index(),
                            Some(vex_destination(p0 & 0x80 != 0, modrm)),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }

    assert_eq!(classified, 36_864);

    // Independently assembled by LLVM 23.0.0.
    for (bytes, expected_destination) in [
        (&[0xC4, 0x41, 0x2B, 0x5A, 0xCB][..], 9),
        (&[0xC5, 0xEA, 0x5A, 0xCB][..], 1),
        (&[0xC5, 0xEB, 0x5A, 0xCB][..], 1),
        (&[0xC4, 0x41, 0x2A, 0x5A, 0xCB][..], 9),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_scalar_fp_convert_destination_index(),
            Some(expected_destination),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vex_classifier_exhausts_prefix_opcode_modrm_and_shape_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let bytes = [0xC4, p0, 0x2A, 0x5A, 0xCB];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_scalar_fp_convert_destination_index(),
            (p0 & 0x1F == 1).then_some(vex_destination(p0 & 0x80 != 0, 0xCB)),
            "{bytes:02X?}"
        );
    }
    for p1 in u8::MIN..=u8::MAX {
        let valid = p1 & 0x04 == 0 && matches!(p1 & 3, 2 | 3);
        let c5 = [0xC5, p1, 0x5A, 0xCB];
        assert_eq!(
            X86InstructionBytes::new(&c5)
                .unwrap()
                .vex_scalar_fp_convert_destination_index(),
            valid.then_some(vex_destination(p1 & 0x80 != 0, 0xCB)),
            "{c5:02X?}"
        );
        let c4 = [0xC4, 0xE1, p1, 0x5A, 0xCB];
        assert_eq!(
            X86InstructionBytes::new(&c4)
                .unwrap()
                .vex_scalar_fp_convert_destination_index(),
            valid.then_some(1),
            "{c4:02X?}"
        );
    }
    for opcode in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE1, 0x2A, opcode, 0xCB];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_scalar_fp_convert_destination_index(),
            (opcode == 0x5A).then_some(1),
            "{bytes:02X?}"
        );
    }
    for modrm in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0x61, 0xAA, 0x5A, modrm];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_scalar_fp_convert_destination_index(),
            (modrm >> 6 == 3).then_some(vex_destination(false, modrm)),
            "{bytes:02X?}"
        );
    }

    for invalid in [
        &[0xC5, 0xEE, 0x5A, 0xCB][..],             // VEX.L=1 is unpredictable
        &[0xC4, 0xE2, 0x2A, 0x5A, 0xCB][..],       // map 0F38
        &[0xC4, 0xE1, 0x29, 0x5A, 0xCB][..],       // wrong mandatory prefix
        &[0xC4, 0xE1, 0x2A, 0x5A, 0x0B][..],       // memory source
        &[0x66, 0xC4, 0xE1, 0x2A, 0x5A, 0xCB][..], // leading prefix
        &[0xC4, 0xE1, 0x2A, 0x5A][..],             // missing ModR/M
        &[0xC4, 0xE1, 0x2A, 0x5A, 0xCB, 0x90][..], // trailing byte
        &[0x62, 0xF1, 0x66, 0x08, 0x5A, 0xCB][..], // EVEX neighbor
    ] {
        assert!(
            X86InstructionBytes::new(invalid)
                .unwrap()
                .vex_scalar_fp_convert_destination_index()
                .is_none(),
            "{invalid:02X?}"
        );
    }
}

#[test]
fn vex_replay_spans_preserve_exact_defined_source_provenance() {
    let pc = 0x5A32_64;
    let mut block = SmirBlock::new(BlockId(48), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        &[0xC4, 0x41, 0x2B, 0x5A, 0xCB][..],
        &[0xC5, 0xEA, 0x5A, 0xCB][..],
        &[0xC4, 0x01, 0xAA, 0x5A, 0xFF][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(48), pc), instruction)]);
        for spans in [
            x86_vex_scalar_fp_convert_replay_spans(&block, &provenance),
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
            x86_evex_scalar_fp_convert_replay_spans(&block, &provenance).is_empty(),
            "{bytes:02X?}"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Conversion {
    F64ToF32,
    F32ToF64,
    F64ToF16,
    F16ToF64,
    F32ToF16,
    F16ToF32,
}

impl Conversion {
    const ALL: [Self; 6] = [
        Self::F64ToF32,
        Self::F32ToF64,
        Self::F64ToF16,
        Self::F16ToF64,
        Self::F32ToF16,
        Self::F16ToF32,
    ];

    fn fields(self) -> (u8, u8, u8, bool, bool) {
        match self {
            Self::F64ToF32 => (1, 0x5A, 3, true, false),
            Self::F32ToF64 => (1, 0x5A, 2, false, false),
            Self::F64ToF16 => (5, 0x5A, 3, true, true),
            Self::F16ToF64 => (5, 0x5A, 2, false, true),
            Self::F32ToF16 => (5, 0x1D, 0, false, true),
            Self::F16ToF32 => (6, 0x13, 0, false, true),
        }
    }

    fn valid_control(self, ll: u8, embedded_control: bool) -> bool {
        ll != 3 || embedded_control
    }
}

#[allow(clippy::too_many_arguments)]
fn encoding(
    conversion: Conversion,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let (map, opcode, pp, w, _) = conversion.fields();
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
    [
        0x62,
        p0,
        (if w { 0x80 } else { 0 }) | ((!merge & 0x0F) << 3) | 0x04 | pp,
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if merge & 0x10 == 0 { 0x08 } else { 0 }
            | mask,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn classifier_accepts_exactly_21_000_sampled_legal_register_encodings() {
    let registers = [0u8, 8, 16, 24, 31];
    let masks = [(0u8, false), (1, false), (1, true), (7, true)];
    let mut classified = 0usize;
    for conversion in Conversion::ALL {
        for ll in 0..=3 {
            for embedded_control in [false, true] {
                for destination in registers {
                    for merge in registers {
                        for source in registers {
                            for (mask, zeroing) in masks {
                                let bytes = encoding(
                                    conversion,
                                    ll,
                                    embedded_control,
                                    destination,
                                    merge,
                                    source,
                                    mask,
                                    zeroing,
                                );
                                let expected = conversion
                                    .valid_control(ll, embedded_control)
                                    .then_some(conversion.fields().4);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_scalar_fp_convert_requires_fp16(),
                                    expected,
                                    "{conversion:?} {bytes:02X?}"
                                );
                                classified += usize::from(expected.is_some());
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 21_000);

    // Independently assembled by LLVM 21.1.8. Collectively these cover all
    // six mnemonics, high destination/merge/source registers, k1/k7,
    // merge/zero masking, dynamic rounding, ER, and SAE.
    for (bytes, needs_fp16) in [
        ([0x62, 0x81, 0xEF, 0x00, 0x5A, 0xCA], false),
        ([0x62, 0x81, 0xEF, 0xB7, 0x5A, 0xCA], false),
        ([0x62, 0x81, 0x66, 0x00, 0x5A, 0xC3], false),
        ([0x62, 0x81, 0x66, 0x11, 0x5A, 0xC3], false),
        ([0x62, 0x05, 0xDF, 0x00, 0x5A, 0xFC], true),
        ([0x62, 0x05, 0xDF, 0xF7, 0x5A, 0xFC], true),
        ([0x62, 0x05, 0x56, 0x00, 0x5A, 0xF5], true),
        ([0x62, 0x05, 0x56, 0x11, 0x5A, 0xF5], true),
        ([0x62, 0x05, 0x4C, 0x00, 0x1D, 0xEE], true),
        ([0x62, 0x05, 0x4C, 0xD7, 0x1D, 0xEE], true),
        ([0x62, 0x06, 0x44, 0x00, 0x13, 0xE7], true),
        ([0x62, 0x06, 0x44, 0x11, 0x13, 0xE7], true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_fp_convert_requires_fp16(),
            Some(needs_fp16),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_reserved_or_nonfamily_frontier() {
    let canonical = encoding(Conversion::F64ToF32, 3, true, 17, 18, 26, 7, true);
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
        (2, canonical[2] & !0x04),          // Missing EVEX fixed-one bit.
        (3, (canonical[3] & !0x07) | 0x80), // Zeroing with k0.
        (4, 0x5B),                          // Neighboring conversion opcode.
        (5, canonical[5] & 0x3F),           // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_fp_convert_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }

    for (map, opcode, pp, w) in [
        (0, 0x5A, 3, true),
        (1, 0x5A, 0, false),
        (1, 0x5A, 3, false),
        (1, 0x1D, 0, false),
        (5, 0x5A, 1, true),
        (5, 0x1D, 0, true),
        (6, 0x13, 1, false),
        (6, 0x13, 0, true),
        (7, 0x13, 0, false),
    ] {
        let bytes = [
            0x62,
            0xF0 | map,
            (if w { 0x80 } else { 0 }) | 0x7C | pp,
            0x08,
            opcode,
            0xC0,
        ];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_scalar_fp_convert_requires_fp16(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_expose_exact_fp16_requirements() {
    let pc = 0x5A1D;
    let mut block = SmirBlock::new(BlockId(90), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for conversion in Conversion::ALL {
        for ll in 0..=3 {
            for embedded_control in [false, true] {
                let bytes = encoding(conversion, ll, embedded_control, 31, 30, 29, 7, true);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let provenance =
                    std::collections::HashMap::from([((BlockId(90), pc), instruction)]);
                let valid = conversion.valid_control(ll, embedded_control);
                for spans in [
                    x86_evex_scalar_fp_convert_replay_spans(&block, &provenance),
                    x86_evex_native_replay_spans(&block, &provenance),
                ] {
                    let Some(span) = spans.get(&0) else {
                        assert!(!valid, "missing legal replay span: {bytes:02X?}");
                        continue;
                    };
                    assert!(valid, "admitted reserved replay encoding: {bytes:02X?}");
                    assert_eq!(span.end, 1, "{bytes:02X?}");
                    assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                    assert!(!span.needs_avx512vl, "{bytes:02X?}");
                    assert!(!span.needs_avx512dq, "{bytes:02X?}");
                    assert_eq!(span.needs_avx512fp16, conversion.fields().4, "{bytes:02X?}");
                }
            }
        }
    }
}
