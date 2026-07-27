//! Exact source-byte replay classification for register-only VEX
//! `VMOVUPS`/`VMOVUPD`.

use super::*;

fn vex_c5_encoding(p1: u8, opcode: u8, modrm: u8) -> [u8; 4] {
    [0xC5, p1, opcode, modrm]
}

fn vex_c4_encoding(p0: u8, p1: u8, opcode: u8, modrm: u8) -> [u8; 5] {
    [0xC4, p0, p1, opcode, modrm]
}

#[test]
fn classifier_covers_all_9216_safe_register_encodings() {
    let mut classified = 0usize;

    for p1 in u8::MIN..=u8::MAX {
        let expected = (p1 & 0x78 == 0x78 && matches!(p1 & 0x03, 0 | 1));
        for opcode in [0x10, 0x11] {
            for modrm in 0xC0..=0xFF {
                let bytes = vex_c5_encoding(p1, opcode, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .is_vex_register_unaligned_packed_fp_move(),
                    expected,
                    "{bytes:02X?}"
                );
                let expected_destination = expected.then_some(if opcode == 0x10 {
                    ((modrm >> 3) & 7) + if p1 & 0x80 == 0 { 8 } else { 0 }
                } else {
                    modrm & 7
                });
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_unaligned_packed_fp_move_destination_index(),
                    expected_destination,
                    "{bytes:02X?}"
                );
                classified += usize::from(expected);
            }
        }
    }

    for extension_bits in 0u8..=7 {
        let p0 = (extension_bits << 5) | 1;
        for p1 in u8::MIN..=u8::MAX {
            let expected = (p1 & 0x78 == 0x78 && matches!(p1 & 0x03, 0 | 1));
            for opcode in [0x10, 0x11] {
                for modrm in 0xC0..=0xFF {
                    let bytes = vex_c4_encoding(p0, p1, opcode, modrm);
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .is_vex_register_unaligned_packed_fp_move(),
                        expected,
                        "{bytes:02X?}"
                    );
                    let expected_destination = expected.then_some(if opcode == 0x10 {
                        ((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 }
                    } else {
                        (modrm & 7) + if p0 & 0x20 == 0 { 8 } else { 0 }
                    });
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_unaligned_packed_fp_move_destination_index(),
                        expected_destination,
                        "{bytes:02X?}"
                    );
                    classified += usize::from(expected);
                }
            }
        }
    }

    assert_eq!(classified, 9_216);
}

#[test]
fn classifier_rejects_memory_reserved_and_noncanonical_frontiers() {
    for opcode in [0x10, 0x11] {
        for modrm in [0x00, 0x45, 0x84, 0xBF] {
            for bytes in [
                vex_c5_encoding(0xF8, opcode, modrm).to_vec(),
                vex_c4_encoding(0x01, 0xF9, opcode, modrm).to_vec(),
            ] {
                assert!(
                    !X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .is_vex_register_unaligned_packed_fp_move(),
                    "{bytes:02X?}"
                );
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_unaligned_packed_fp_move_destination_index(),
                    None,
                    "{bytes:02X?}"
                );
            }
        }
    }

    for opcode in u8::MIN..=u8::MAX {
        if matches!(opcode, 0x10 | 0x11) {
            continue;
        }
        for bytes in [
            vex_c5_encoding(0xF8, opcode, 0xFF).to_vec(),
            vex_c4_encoding(0x01, 0xF9, opcode, 0xFF).to_vec(),
        ] {
            assert!(
                !X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .is_vex_register_unaligned_packed_fp_move(),
                "{bytes:02X?}"
            );
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_unaligned_packed_fp_move_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    for map in 0u8..=31 {
        if map == 1 {
            continue;
        }
        let bytes = vex_c4_encoding(0xE0 | map, 0xF9, 0x10, 0xC0);
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_unaligned_packed_fp_move(),
            "{bytes:02X?}"
        );
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_unaligned_packed_fp_move_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x10, 0xC0],
        &[0x66, 0x0F, 0x10, 0xC0],
        &[0xC5, 0x78, 0x10],
        &[0xC5, 0x78, 0x10, 0xC0, 0],
        &[0xC5, 0x70, 0x10, 0xC0],
        &[0xC5, 0xFA, 0x10, 0xC0],
        &[0xC4, 0xE1, 0xF9, 0x10],
        &[0xC4, 0xE1, 0xF9, 0x10, 0xC0, 0],
        &[0xC4, 0xE1, 0xE9, 0x10, 0xC0],
        &[0xC4, 0xE1, 0xFA, 0x10, 0xC0],
        &[0x62, 0xF1, 0x7C, 0x08, 0x10, 0xC0],
    ];
    for bytes in invalid {
        assert!(
            !X86InstructionBytes::new(bytes)
                .unwrap()
                .is_vex_register_unaligned_packed_fp_move(),
            "{bytes:02X?}"
        );
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_unaligned_packed_fp_move_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_preserve_exact_bytes_and_remain_non_evex() {
    let pc = 0x1011;
    let mut block = SmirBlock::new(BlockId(36), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        vex_c5_encoding(0x7C, 0x10, 0xCB).to_vec(),
        vex_c4_encoding(0x41, 0xFD, 0x11, 0xCB).to_vec(),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert!(
            instruction.is_vex_register_unaligned_packed_fp_move(),
            "{bytes:02X?}"
        );
        let provenance = HashMap::from([((BlockId(36), pc), instruction)]);
        for spans in [
            x86_vex_unaligned_packed_fp_move_replay_spans(&block, &provenance),
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

    assert!(x86_vex_unaligned_packed_fp_move_replay_spans(&block, &HashMap::new()).is_empty());
}
