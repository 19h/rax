//! Exact source-byte replay classification for register-only VEX ROUND.

use super::*;

fn destination(p0: u8, modrm: u8) -> u8 {
    ((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3)
}

fn assert_classified(bytes: &[u8], expected_destination: u8) {
    assert_eq!(
        X86InstructionBytes::new(bytes)
            .unwrap()
            .vex_round_destination_index(),
        Some(expected_destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_17_825_792_defined_register_byte_images() {
    let mut classified = 0usize;

    for extension_bits in 0u8..8 {
        let p0 = (extension_bits << 5) | 3;
        for w in [false, true] {
            for l in [false, true] {
                let packed_p1 = (u8::from(w) << 7) | 0x78 | (u8::from(l) << 2) | 1;
                for opcode in [0x08u8, 0x09] {
                    for modrm in 0xC0u8..=0xFF {
                        for immediate in u8::MIN..=u8::MAX {
                            let bytes = [0xC4, p0, packed_p1, opcode, modrm, immediate];
                            assert_classified(&bytes, destination(p0, modrm));
                            classified += 1;
                        }
                    }
                }

                for encoded_vvvv in 0u8..16 {
                    let scalar_p1 =
                        (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1;
                    for opcode in [0x0Au8, 0x0B] {
                        for modrm in 0xC0u8..=0xFF {
                            for immediate in u8::MIN..=u8::MAX {
                                let bytes = [0xC4, p0, scalar_p1, opcode, modrm, immediate];
                                assert_classified(&bytes, destination(p0, modrm));
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(classified, 17_825_792);

    // Independently assembled by LLVM 23.0.0.
    for (bytes, expected_destination) in [
        (&[0xC4, 0x43, 0x7D, 0x08, 0xCA, 0xFF][..], 9),
        (&[0xC4, 0x43, 0x29, 0x0A, 0xCB, 0xFF][..], 9),
        (&[0xC4, 0xE3, 0x79, 0x09, 0xCA, 0x08][..], 1),
        (&[0xC4, 0x43, 0x11, 0x0B, 0xE6, 0x04][..], 12),
    ] {
        assert_classified(bytes, expected_destination);
    }
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_shape_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let bytes = [0xC4, p0, 0x79, 0x09, 0xCA, 0xA5];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_round_destination_index(),
            (p0 & 0x1F == 3).then_some(destination(p0, 0xCA)),
            "{bytes:02X?}"
        );
    }
    for p1 in u8::MIN..=u8::MAX {
        let packed = [0xC4, 0xE3, p1, 0x08, 0xCA, 0xA5];
        assert_eq!(
            X86InstructionBytes::new(&packed)
                .unwrap()
                .vex_round_destination_index(),
            (p1 & 0x03 == 1 && p1 & 0x78 == 0x78).then_some(1),
            "{packed:02X?}"
        );
        let scalar = [0xC4, 0xE3, p1, 0x0A, 0xCA, 0xA5];
        assert_eq!(
            X86InstructionBytes::new(&scalar)
                .unwrap()
                .vex_round_destination_index(),
            (p1 & 0x03 == 1).then_some(1),
            "{scalar:02X?}"
        );
    }
    for opcode in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE3, 0x79, opcode, 0xCA, 0xA5];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_round_destination_index(),
            matches!(opcode, 0x08..=0x0B).then_some(1),
            "{bytes:02X?}"
        );
    }
    for modrm in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0x63, 0xF9, 0x09, modrm, 0xA5];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_round_destination_index(),
            (modrm >> 6 == 3).then_some(destination(0x63, modrm)),
            "{bytes:02X?}"
        );
    }

    for invalid in [
        &[0xC5, 0xF9, 0x09, 0xCA, 0x00][..],       // map 0F3A needs C4
        &[0xC4, 0xE2, 0x79, 0x09, 0xCA, 0x00][..], // map 0F38
        &[0xC4, 0xE3, 0x71, 0x09, 0xCA, 0x00][..], // packed vvvv reserved
        &[0xC4, 0xE3, 0x78, 0x09, 0xCA, 0x00][..], // wrong mandatory prefix
        &[0xC4, 0xE3, 0x79, 0x09, 0x0A, 0x00][..], // memory source
        &[0x66, 0xC4, 0xE3, 0x79, 0x09, 0xCA, 0x00][..], // leading prefix
        &[0xC4, 0xE3, 0x79, 0x09, 0xCA][..],       // missing immediate
        &[0xC4, 0xE3, 0x79, 0x09, 0xCA, 0x00, 0x90][..], // trailing byte
        &[0x62, 0xF3, 0xFD, 0x08, 0x09, 0xCA, 0x00][..], // EVEX neighbor
    ] {
        assert!(
            X86InstructionBytes::new(invalid)
                .unwrap()
                .vex_round_destination_index()
                .is_none(),
            "{invalid:02X?}"
        );
    }
}

#[test]
fn replay_spans_preserve_exact_defined_source_provenance() {
    let pc = 0x0B0A_0908;
    let mut block = SmirBlock::new(BlockId(47), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        &[0xC4, 0x43, 0x7D, 0x08, 0xCA, 0xFF][..],
        &[0xC4, 0x43, 0x29, 0x0A, 0xCB, 0xFF][..],
        &[0xC4, 0xE3, 0x79, 0x09, 0xCA, 0x08][..],
        &[0xC4, 0x43, 0x15, 0x0B, 0xE6, 0x04][..],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(47), pc), instruction)]);
        for spans in [
            x86_vex_round_replay_spans(&block, &provenance),
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
