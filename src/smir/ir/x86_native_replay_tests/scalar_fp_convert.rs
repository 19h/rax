//! Exact classifier tests for EVEX scalar floating-point precision replay.

use super::*;

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
fn classifier_accepts_exactly_24_000_sampled_legal_register_encodings() {
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
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_register_scalar_fp_convert_requires_fp16(),
                                    Some(conversion.fields().4),
                                    "{conversion:?} {bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 24_000);

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
                for spans in [
                    x86_evex_scalar_fp_convert_replay_spans(&block, &provenance),
                    x86_evex_native_replay_spans(&block, &provenance),
                ] {
                    let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
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
