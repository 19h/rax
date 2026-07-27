//! Exact source-byte replay classification for register-destination AVX VEX
//! scalar lane extracts.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WMode {
    Ignored,
    W0,
    W1,
}

impl WMode {
    fn values(self) -> &'static [bool] {
        match self {
            Self::Ignored => &[false, true],
            Self::W0 => &[false],
            Self::W1 => &[true],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GprField {
    Reg,
    Rm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtractKind {
    Vextractps,
    Vpextrb,
    Vpextrd,
    Vpextrq,
    VpextrwMap1,
    VpextrwMap3,
}

impl ExtractKind {
    const ALL: [Self; 6] = [
        Self::Vextractps,
        Self::Vpextrb,
        Self::Vpextrd,
        Self::Vpextrq,
        Self::VpextrwMap1,
        Self::VpextrwMap3,
    ];

    fn fields(self) -> (u8, u8, WMode, GprField) {
        match self {
            Self::Vextractps => (3, 0x17, WMode::Ignored, GprField::Rm),
            Self::Vpextrb => (3, 0x14, WMode::Ignored, GprField::Rm),
            Self::Vpextrd => (3, 0x16, WMode::W0, GprField::Rm),
            Self::Vpextrq => (3, 0x16, WMode::W1, GprField::Rm),
            Self::VpextrwMap1 => (1, 0xC5, WMode::Ignored, GprField::Reg),
            Self::VpextrwMap3 => (3, 0x15, WMode::Ignored, GprField::Rm),
        }
    }
}

fn c4_encoding(
    kind: ExtractKind,
    extension_bits: u8,
    w: bool,
    modrm: u8,
    immediate: u8,
) -> [u8; 6] {
    let (map, opcode, w_mode, _) = kind.fields();
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(w_mode.values().contains(&w));
    [
        0xC4,
        extension_bits | map,
        0x79 | (u8::from(w) << 7),
        opcode,
        modrm,
        immediate,
    ]
}

fn c5_encoding(r_extension: bool, modrm: u8, immediate: u8) -> [u8; 5] {
    [
        0xC5,
        0x79 | if r_extension { 0 } else { 0x80 },
        0xC5,
        modrm,
        immediate,
    ]
}

fn c4_destination(kind: ExtractKind, extension_bits: u8, modrm: u8) -> u8 {
    match kind.fields().3 {
        GprField::Reg => (u8::from(extension_bits & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
        GprField::Rm => (u8::from(extension_bits & 0x20 == 0) << 3) | (modrm & 7),
    }
}

fn assert_classified(bytes: &[u8], destination: u8) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    assert!(instruction.is_vex_register_scalar_extract(), "{bytes:02X?}");
    assert_eq!(
        instruction.vex_scalar_extract_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_1_343_488_legal_register_byte_encodings() {
    let mut classified = 0usize;
    for kind in ExtractKind::ALL {
        let (_, _, w_mode, _) = kind.fields();
        for &w in w_mode.values() {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for modrm in 0xC0..=0xFF {
                    let destination = c4_destination(kind, extension_bits, modrm);
                    for immediate in u8::MIN..=u8::MAX {
                        let bytes = c4_encoding(kind, extension_bits, w, modrm, immediate);
                        assert_classified(&bytes, destination);
                        classified += 1;
                    }
                }
            }
        }
    }

    for r_extension in [false, true] {
        for modrm in 0xC0..=0xFF {
            let destination = (u8::from(r_extension) << 3) | ((modrm >> 3) & 7);
            for immediate in u8::MIN..=u8::MAX {
                let bytes = c5_encoding(r_extension, modrm, immediate);
                assert_classified(&bytes, destination);
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 1_343_488);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_shape_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let mut bytes = c4_encoding(ExtractKind::Vextractps, 0xE0, false, 0xC1, 0xA5);
        bytes[1] = p0;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_extract(),
            p0 & 0x1F == 3,
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = c4_encoding(ExtractKind::Vextractps, 0xE0, false, 0xC1, 0xA5);
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_extract(),
            p1 & 0x7F == 0x79,
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        let mut bytes = c4_encoding(ExtractKind::Vextractps, 0xE0, false, 0xC1, 0xA5);
        bytes[3] = opcode;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_extract(),
            matches!(opcode, 0x14..=0x17),
            "{bytes:02X?}"
        );
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = c4_encoding(ExtractKind::Vextractps, 0xE0, true, modrm, 0xA5);
        let destination = modrm & 7;
        let expected = modrm >> 6 == 3;
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.is_vex_register_scalar_extract(),
            expected,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_scalar_extract_destination_index(),
            expected.then_some(destination),
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let bytes = [0xC5, p1, 0xC5, 0xC1, 0xA5];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_scalar_extract(),
            p1 & 0x7F == 0x79,
            "{bytes:02X?}"
        );
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = c5_encoding(false, modrm, 0xA5);
        let destination = (modrm >> 3) & 7;
        let expected = modrm >> 6 == 3;
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.is_vex_register_scalar_extract(),
            expected,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_scalar_extract_destination_index(),
            expected.then_some(destination),
            "{bytes:02X?}"
        );
    }

    let base = c4_encoding(ExtractKind::VpextrwMap1, 0xE0, true, 0xC1, 0xA5);
    let invalid: &[&[u8]] = &[
        &base[..5],
        &[base[0], base[1], base[2], base[3], base[4], base[5], 0],
        &[0x62, 0xF1, 0x7D, 0x08, 0xC5, 0xC1, 0xA5],
        &[0xC4, 0xE2, 0x79, 0xC5, 0xC1, 0xA5],
        &[0xC4, 0xE1, 0x75, 0xC5, 0xC1, 0xA5],
        &[0xC4, 0xE1, 0x7D, 0xC5, 0xC1, 0xA5],
        &[0xC4, 0xE1, 0x7A, 0xC5, 0xC1, 0xA5],
        &[0xC4, 0xE1, 0x79, 0xC4, 0xC1, 0xA5],
        &[0xC4, 0xE1, 0x79, 0xC5, 0x01, 0xA5],
        &[0xC5, 0x75, 0xC5, 0xC1, 0xA5],
        &[0xC5, 0x7D, 0xC5, 0xC1, 0xA5],
        &[0xC5, 0x7A, 0xC5, 0xC1, 0xA5],
        &[0xC5, 0x79, 0xC4, 0xC1, 0xA5],
        &[0xC5, 0x79, 0xC5, 0x01, 0xA5],
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert!(
            !instruction.is_vex_register_scalar_extract(),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_scalar_extract_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_accepts_every_stack_field_and_rewrites_only_the_destination() {
    for destination_low in [4, 5] {
        for kind in ExtractKind::ALL {
            let (_, _, w_mode, gpr_field) = kind.fields();
            for &w in w_mode.values() {
                for ignored_x in [false, true] {
                    let low_extensions = if ignored_x { 0xA0 } else { 0xE0 };
                    let high_extensions = match gpr_field {
                        GprField::Reg => {
                            if ignored_x {
                                0x20
                            } else {
                                0x60
                            }
                        }
                        GprField::Rm => {
                            if ignored_x {
                                0x80
                            } else {
                                0xC0
                            }
                        }
                    };
                    let modrm = match gpr_field {
                        GprField::Reg => 0xC0 | (destination_low << 3) | 1,
                        GprField::Rm => 0xC0 | (1 << 3) | destination_low,
                    };
                    for (bytes, original_destination) in [
                        (
                            c4_encoding(kind, low_extensions, w, modrm, 0xA5),
                            destination_low,
                        ),
                        (
                            c4_encoding(kind, high_extensions, w, modrm, 0xA5),
                            destination_low + 8,
                        ),
                    ] {
                        assert_classified(&bytes, original_destination);
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        for rewritten_destination in [0, 3, 4, 5, 8, 15] {
                            let rewritten = instruction
                                .vex_scalar_extract_with_destination(rewritten_destination)
                                .unwrap();
                            assert_eq!(
                                rewritten.vex_scalar_extract_destination_index(),
                                Some(rewritten_destination),
                                "{kind:?} {bytes:02X?}"
                            );
                            assert_eq!(
                                rewritten.vex_scalar_extract_with_destination(original_destination),
                                Some(instruction),
                                "{kind:?} {bytes:02X?}"
                            );
                        }
                        assert_eq!(
                            instruction.vex_scalar_extract_with_destination(16),
                            None,
                            "{kind:?} {bytes:02X?}"
                        );
                    }
                }
            }
        }

        let modrm = 0xC0 | (destination_low << 3) | 1;
        for (bytes, original_destination) in [
            (c5_encoding(false, modrm, 0xA5), destination_low),
            (c5_encoding(true, modrm, 0xA5), destination_low + 8),
        ] {
            assert_classified(&bytes, original_destination);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            for rewritten_destination in [0, 3, 4, 5, 8, 15] {
                let rewritten = instruction
                    .vex_scalar_extract_with_destination(rewritten_destination)
                    .unwrap();
                assert_eq!(
                    rewritten.vex_scalar_extract_destination_index(),
                    Some(rewritten_destination),
                    "{bytes:02X?}"
                );
                assert_eq!(
                    rewritten.vex_scalar_extract_with_destination(original_destination),
                    Some(instruction),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn llvm_samples_raw_map3_word_and_replay_spans_preserve_exact_bytes() {
    let pc = 0x1041;
    let mut block = SmirBlock::new(BlockId(41), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    // LLVM 23.0.0 independently assembled the first six canonical samples.
    // The final sample selects the architecturally equivalent map-0F3A word
    // form, which LLVM canonicalizes to map-0F for a register destination.
    for (bytes, destination) in [
        (&[0xC4, 0x43, 0x79, 0x17, 0xFE, 0x03][..], 14),
        (&[0xC4, 0x43, 0x79, 0x14, 0xFE, 0x0F], 14),
        (&[0xC4, 0x43, 0x79, 0x16, 0xFE, 0x03], 14),
        (&[0xC4, 0x43, 0xF9, 0x16, 0xFE, 0x01], 14),
        (&[0xC5, 0x79, 0xC5, 0xF7, 0x07], 14),
        (&[0xC4, 0x41, 0x79, 0xC5, 0xF7, 0x07], 14),
        (&[0xC4, 0x43, 0x79, 0x15, 0xFE, 0x07], 14),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_classified(bytes, destination);
        let provenance = HashMap::from([((BlockId(41), pc), instruction)]);
        for spans in [
            x86_vex_scalar_extract_replay_spans(&block, &provenance),
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

    assert!(x86_vex_scalar_extract_replay_spans(&block, &HashMap::new()).is_empty());
}
