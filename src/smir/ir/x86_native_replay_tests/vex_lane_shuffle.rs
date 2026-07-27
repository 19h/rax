//! Exact source-byte replay classification for register-only VEX one-source
//! lane shuffles.

use super::*;

#[derive(Clone, Copy)]
struct Family {
    opcode: u8,
    pp: u8,
    immediate: bool,
}

const FAMILIES: [Family; 6] = [
    Family {
        opcode: 0x12,
        pp: 2,
        immediate: false,
    },
    Family {
        opcode: 0x16,
        pp: 2,
        immediate: false,
    },
    Family {
        opcode: 0x12,
        pp: 3,
        immediate: false,
    },
    Family {
        opcode: 0x70,
        pp: 1,
        immediate: true,
    },
    Family {
        opcode: 0x70,
        pp: 2,
        immediate: true,
    },
    Family {
        opcode: 0x70,
        pp: 3,
        immediate: true,
    },
];

fn assert_classified(bytes: &[u8], needs_avx2: bool, destination: u8) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    assert_eq!(
        instruction.vex_register_lane_shuffle_needs_avx2(),
        Some(needs_avx2),
        "{bytes:02X?}"
    );
    assert_eq!(
        instruction.vex_lane_shuffle_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_1776384_legal_register_byte_encodings() {
    let mut classified = 0usize;

    for family in FAMILIES {
        for encoded_r in [false, true] {
            for l in [false, true] {
                let p1 = (u8::from(encoded_r) << 7) | 0x78 | (u8::from(l) << 2) | family.pp;
                for modrm in 0xC0..=0xFF {
                    let destination = ((modrm >> 3) & 7) + if encoded_r { 0 } else { 8 };
                    if family.immediate {
                        for immediate in u8::MIN..=u8::MAX {
                            let bytes = [0xC5, p1, family.opcode, modrm, immediate];
                            assert_classified(&bytes, l, destination);
                            classified += 1;
                        }
                    } else {
                        let bytes = [0xC5, p1, family.opcode, modrm];
                        assert_classified(&bytes, false, destination);
                        classified += 1;
                    }
                }
            }
        }
    }

    for extension_bits in 0u8..=7 {
        let p0 = (extension_bits << 5) | 1;
        for family in FAMILIES {
            for w in [false, true] {
                for l in [false, true] {
                    let p1 = (u8::from(w) << 7) | 0x78 | (u8::from(l) << 2) | family.pp;
                    for modrm in 0xC0..=0xFF {
                        let destination = ((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 };
                        if family.immediate {
                            for immediate in u8::MIN..=u8::MAX {
                                let bytes = [0xC4, p0, p1, family.opcode, modrm, immediate];
                                assert_classified(&bytes, l, destination);
                                classified += 1;
                            }
                        } else {
                            let bytes = [0xC4, p0, p1, family.opcode, modrm];
                            assert_classified(&bytes, false, destination);
                            classified += 1;
                        }
                    }
                }
            }
        }
    }

    assert_eq!(classified, 1_776_384);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_shape_frontiers() {
    for p1 in u8::MIN..=u8::MAX {
        let duplicate = [0xC5, p1, 0x12, 0xCA];
        let duplicate_expected = (p1 & 0x78 == 0x78 && matches!(p1 & 0x03, 2 | 3)).then_some(false);
        assert_eq!(
            X86InstructionBytes::new(&duplicate)
                .unwrap()
                .vex_register_lane_shuffle_needs_avx2(),
            duplicate_expected,
            "{duplicate:02X?}"
        );

        let packed = [0xC5, p1, 0x70, 0xCA, 0xA5];
        let packed_expected =
            (p1 & 0x78 == 0x78 && matches!(p1 & 0x03, 1 | 2 | 3)).then_some(p1 & 0x04 != 0);
        assert_eq!(
            X86InstructionBytes::new(&packed)
                .unwrap()
                .vex_register_lane_shuffle_needs_avx2(),
            packed_expected,
            "{packed:02X?}"
        );
    }

    for p0 in u8::MIN..=u8::MAX {
        for bytes in [
            vec![0xC4, p0, 0xFA, 0x16, 0xCA],
            vec![0xC4, p0, 0xFD, 0x70, 0xCA, 0x1B],
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_lane_shuffle_needs_avx2()
                    .is_some(),
                p0 & 0x1F == 1,
                "{bytes:02X?}"
            );
        }
    }

    for opcode in u8::MIN..=u8::MAX {
        let duplicate = [0xC5, 0xFA, opcode, 0xCA];
        assert_eq!(
            X86InstructionBytes::new(&duplicate)
                .unwrap()
                .vex_register_lane_shuffle_needs_avx2(),
            matches!(opcode, 0x12 | 0x16).then_some(false),
            "{duplicate:02X?}"
        );

        let packed = [0xC5, 0xF9, opcode, 0xCA, 0x4E];
        assert_eq!(
            X86InstructionBytes::new(&packed)
                .unwrap()
                .vex_register_lane_shuffle_needs_avx2(),
            (opcode == 0x70).then_some(false),
            "{packed:02X?}"
        );
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0x01, 0xFE, 0x70, modrm, 0xE4];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_lane_shuffle_needs_avx2(),
            (modrm >> 6 == 3).then_some(true),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0xF3, 0x0F, 0x12, 0xCA],
        &[0xC5, 0xFA, 0x12],
        &[0xC5, 0xFA, 0x12, 0xCA, 0],
        &[0xC5, 0xEA, 0x12, 0xCA],
        &[0xC5, 0xF9, 0x70, 0xCA],
        &[0xC5, 0xF9, 0x70, 0xCA, 0, 0],
        &[0xC5, 0xF8, 0x70, 0xCA, 0],
        &[0xC4, 0xE2, 0xFA, 0x12, 0xCA],
        &[0xC4, 0xE1, 0xFA, 0x16],
        &[0xC4, 0xE1, 0xFA, 0x16, 0xCA, 0],
        &[0xC4, 0xE1, 0xFD, 0x70, 0xCA],
        &[0xC4, 0xE1, 0xFD, 0x70, 0xCA, 0, 0],
        &[0x62, 0xF1, 0x7D, 0x08, 0x70, 0xCA, 0],
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_lane_shuffle_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_lane_shuffle_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn llvm_samples_and_replay_spans_preserve_exact_bytes_without_evex_features() {
    let pc = 0x1041;
    let mut block = SmirBlock::new(BlockId(39), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    // LLVM 23.0.0 independently assembled these six canonical samples.
    for (bytes, needs_avx2, destination) in [
        (&[0xC5, 0xFA, 0x12, 0xCB][..], false, 1),
        (&[0xC4, 0x41, 0x7E, 0x16, 0xCB], false, 9),
        (&[0xC4, 0x41, 0x7B, 0x12, 0xD9], false, 11),
        (&[0xC5, 0xF9, 0x70, 0xCB, 0x1B], false, 1),
        (&[0xC4, 0x41, 0x7E, 0x70, 0xCB, 0x4E], true, 9),
        (&[0xC4, 0x41, 0x7B, 0x70, 0xD9, 0xE4], false, 11),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_lane_shuffle_needs_avx2(),
            Some(needs_avx2),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_lane_shuffle_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );

        let provenance = HashMap::from([((BlockId(39), pc), instruction)]);
        for spans in [
            x86_vex_lane_shuffle_replay_spans(&block, &provenance),
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

    assert!(x86_vex_lane_shuffle_replay_spans(&block, &HashMap::new()).is_empty());
}
