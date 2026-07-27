//! Exact classifier tests for register-only EVEX FP16 widening replay.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WidenKind {
    ToF64,
    ToF32,
    ToF32X,
}

impl WidenKind {
    const ALL: [Self; 3] = [Self::ToF64, Self::ToF32, Self::ToF32X];

    fn fields(self) -> (u8, u8, u8, bool) {
        match self {
            Self::ToF64 => (5, 0, 0x5A, true),
            Self::ToF32 => (2, 1, 0x13, false),
            Self::ToF32X => (6, 1, 0x13, true),
        }
    }
}

fn vex_encoding(ymm: bool, destination: u8, source: u8, ignored_x_clear: bool) -> [u8; 5] {
    assert!(destination < 16 && source < 16);
    let mut p0 = 0xE2;
    if destination >= 8 {
        p0 &= !0x80;
    }
    if ignored_x_clear {
        p0 &= !0x40;
    }
    if source >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x79 | (u8::from(ymm) << 2),
        0x13,
        0xC0 | ((destination & 7) << 3) | (source & 7),
    ]
}

#[test]
fn vex_classifier_covers_all_1024_legal_register_byte_images_and_destinations() {
    let mut classified = 0usize;
    for p0 in u8::MIN..=u8::MAX {
        if p0 & 0x1F != 2 {
            continue;
        }
        for p1 in u8::MIN..=u8::MAX {
            if p1 & 0xFB != 0x79 {
                continue;
            }
            for modrm in 0xC0..=0xFF {
                let bytes = [0xC4, p0, p1, 0x13, modrm];
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                assert!(instruction.is_vex_register_fp16_widen(), "{bytes:02X?}");
                assert_eq!(
                    instruction.vex_fp16_widen_destination_index(),
                    Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 }),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 1_024);
}

#[test]
fn vex_classifier_exhausts_prefix_opcode_and_modrm_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let bytes = [0xC4, p0, 0x79, 0x13, 0xCA];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fp16_widen(),
            p0 & 0x1F == 2,
            "{bytes:02X?}"
        );
    }
    for p1 in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE2, p1, 0x13, 0xCA];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fp16_widen(),
            p1 & 0xFB == 0x79,
            "{bytes:02X?}"
        );
    }
    for opcode in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE2, 0x79, opcode, 0xCA];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fp16_widen(),
            opcode == 0x13,
            "{bytes:02X?}"
        );
    }
    for modrm in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE2, 0x7D, 0x13, modrm];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fp16_widen(),
            modrm >> 6 == 3,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vex_classifier_accepts_independent_samples_and_rejects_reserved_neighbors() {
    // The canonical samples were assembled by LLVM 23.0.0. iced-x86 1.21.0
    // independently decodes the last two samples after clearing ignored VEX.X.
    for (bytes, destination) in [
        ([0xC4, 0xE2, 0x79, 0x13, 0xCA], 1),
        ([0xC4, 0x42, 0x79, 0x13, 0xCA], 9),
        ([0xC4, 0xE2, 0x7D, 0x13, 0xCA], 1),
        ([0xC4, 0x42, 0x7D, 0x13, 0xCA], 9),
        ([0xC4, 0xA2, 0x79, 0x13, 0xCA], 1),
        ([0xC4, 0x02, 0x7D, 0x13, 0xCA], 9),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert!(instruction.is_vex_register_fp16_widen(), "{bytes:02X?}");
        assert_eq!(
            instruction.vex_fp16_widen_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0xC4, 0xE2, 0x79, 0x13],
        &[0xC4, 0xE2, 0x79, 0x13, 0xCA, 0],
        &[0xC5, 0x79, 0x13, 0xCA],
        &[0xC4, 0xE1, 0x79, 0x13, 0xCA], // map 0F
        &[0xC4, 0xE3, 0x79, 0x13, 0xCA], // map 0F3A
        &[0xC4, 0xE2, 0xF9, 0x13, 0xCA], // W1
        &[0xC4, 0xE2, 0x71, 0x13, 0xCA], // VEX.vvvv != 1111b
        &[0xC4, 0xE2, 0x78, 0x13, 0xCA], // wrong mandatory prefix
        &[0xC4, 0xE2, 0x79, 0x12, 0xCA], // unrelated opcode
        &[0xC4, 0xE2, 0x79, 0x13, 0x0A], // memory source
        &[0x62, 0xF2, 0x7D, 0x08, 0x13, 0xCA],
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert!(!instruction.is_vex_register_fp16_widen(), "{bytes:02X?}");
        assert_eq!(
            instruction.vex_fp16_widen_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vex_replay_spans_preserve_exact_instruction_provenance() {
    let pc = 0xF16C;
    let mut block = SmirBlock::new(BlockId(60), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        vex_encoding(false, 1, 2, false),
        vex_encoding(true, 9, 10, false),
        vex_encoding(false, 1, 2, true),
        vex_encoding(true, 15, 15, true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = std::collections::HashMap::from([((BlockId(60), pc), instruction)]);
        for spans in [
            x86_vex_fp16_widen_replay_spans(&block, &provenance),
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
            x86_evex_native_replay_spans(&block, &provenance).is_empty(),
            "{bytes:02X?}"
        );
    }
}

fn encoding(
    kind: WidenKind,
    ll: u8,
    suppress_exceptions: bool,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let (map, pp, opcode, _) = kind.fields();
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
        0x7C | pp,
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if suppress_exceptions { 0x10 } else { 0 }
            | 0x08
            | mask,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn classifier_accepts_exactly_1_200_sampled_legal_register_encodings() {
    let registers = [0, 7, 8, 16, 31];
    let masks = [(0, false), (1, false), (2, true), (7, true)];
    let controls = [(0, false), (1, false), (2, false), (0, true)];
    let mut classified = 0usize;

    for kind in WidenKind::ALL {
        let needs_fp16 = kind.fields().3;
        for (ll, suppress_exceptions) in controls {
            let expected = Some((!suppress_exceptions && ll != 2, needs_fp16));
            for destination in registers {
                for source in registers {
                    for (mask, zeroing) in masks {
                        let bytes = encoding(
                            kind,
                            ll,
                            suppress_exceptions,
                            destination,
                            source,
                            mask,
                            zeroing,
                        );
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_fp16_widen_requirements(),
                            expected,
                            "{kind:?} {bytes:02X?}"
                        );
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_fp16_widen_preserves_mxcsr_de(),
                            kind == WidenKind::ToF32X,
                            "{kind:?} {bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 1_200);

    // Independently assembled by LLVM 21.1.8.
    for (bytes, expected) in [
        ([0x62, 0xF5, 0x7C, 0x08, 0x5A, 0xCA], (true, true)),
        ([0x62, 0x55, 0x7C, 0x28, 0x5A, 0xCA], (true, true)),
        ([0x62, 0xA5, 0x7C, 0x48, 0x5A, 0xCA], (false, true)),
        ([0x62, 0x05, 0x7C, 0x18, 0x5A, 0xEE], (false, true)),
        ([0x62, 0xA2, 0x7D, 0x09, 0x13, 0xCA], (true, false)),
        ([0x62, 0x52, 0x7D, 0xAA, 0x13, 0xCA], (true, false)),
        ([0x62, 0xA2, 0x7D, 0x48, 0x13, 0xCA], (false, false)),
        ([0x62, 0x02, 0x7D, 0x18, 0x13, 0xEE], (false, false)),
        ([0x62, 0xF6, 0x7D, 0x08, 0x13, 0xCA], (true, true)),
        ([0x62, 0x56, 0x7D, 0x28, 0x13, 0xCA], (true, true)),
        ([0x62, 0xA6, 0x7D, 0x48, 0x13, 0xCA], (false, true)),
        ([0x62, 0x06, 0x7D, 0x18, 0x13, 0xEE], (false, true)),
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_widen_requirements(),
            Some(expected),
            "{bytes:02X?}"
        );
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_widen_preserves_mxcsr_de(),
            bytes[1] & 0x0F == 6,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_reserved_or_unsafe_frontier() {
    let canonical = encoding(WidenKind::ToF32X, 0, false, 17, 18, 1, false);
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
        canonical.into_iter().chain([0xA5]).collect::<Vec<_>>(),
    ];
    for mutation in [
        (1, canonical[1] | 0x08),  // Reserved EVEX.P0 bit 3.
        (2, canonical[2] & !0x04), // Missing EVEX fixed-one bit.
        (2, canonical[2] | 0x80),  // W1.
        (2, canonical[2] & !0x08), // Reserved vvvv.
        (3, canonical[3] & !0x08), // Reserved V'.
        (3, 0x88),                 // Zeroing with k0.
        (4, 0x12),                 // Unrelated opcode.
        (5, canonical[5] & 0x3F),  // Memory source.
    ] {
        let mut bytes = canonical;
        bytes[mutation.0] = mutation.1;
        invalid.push(bytes.to_vec());
    }

    for kind in WidenKind::ALL {
        for ll in 1..=3 {
            invalid.push(encoding(kind, ll, true, 1, 2, 0, false).to_vec());
        }
        invalid.push(encoding(kind, 3, false, 1, 2, 0, false).to_vec());
    }

    for bytes in invalid {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.evex_register_fp16_widen_requirements(),
            None,
            "{bytes:02X?}"
        );
        assert!(!instruction.evex_register_fp16_widen_preserves_mxcsr_de());
    }

    // Each neighboring map/prefix tuple must remain disjoint.
    for (map, pp, w, opcode) in [
        (1, 1, false, 0x13),
        (2, 0, false, 0x13),
        (2, 1, true, 0x13),
        (5, 1, false, 0x5A),
        (5, 0, true, 0x5A),
        (6, 0, false, 0x13),
        (6, 1, true, 0x13),
    ] {
        let bytes = [
            0x62,
            0xF0 | map,
            0x7C | pp | if w { 0x80 } else { 0 },
            0x08,
            opcode,
            0xCA,
        ];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_widen_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_expose_exact_vl_and_fp16_requirements() {
    let pc = 0x5A13;
    let mut block = SmirBlock::new(BlockId(61), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for kind in WidenKind::ALL {
        for (ll, suppress_exceptions) in [(0, false), (1, false), (2, false), (0, true)] {
            let bytes = encoding(kind, ll, suppress_exceptions, 29, 30, 3, true);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let provenance = std::collections::HashMap::from([((BlockId(61), pc), instruction)]);
            let expected_vl = !suppress_exceptions && ll != 2;
            let expected_fp16 = kind.fields().3;
            for spans in [
                x86_evex_fp16_widen_replay_spans(&block, &provenance),
                x86_evex_native_replay_spans(&block, &provenance),
            ] {
                let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                assert_eq!(span.end, 1, "{bytes:02X?}");
                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                assert_eq!(span.needs_avx512vl, expected_vl, "{bytes:02X?}");
                assert!(!span.needs_avx512dq, "{bytes:02X?}");
                assert_eq!(span.needs_avx512fp16, expected_fp16, "{bytes:02X?}");
                assert_eq!(
                    span.preserve_mxcsr_de,
                    kind == WidenKind::ToF32X,
                    "{bytes:02X?}"
                );
            }
        }
    }
}
