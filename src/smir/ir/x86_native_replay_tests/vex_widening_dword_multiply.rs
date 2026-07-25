//! Exact source-byte replay classification for register-only AVX VEX
//! `VPMULUDQ` and `VPMULDQ`.

use super::*;

fn vex_c5_encoding(p1: u8, opcode: u8, modrm: u8) -> [u8; 4] {
    [0xC5, p1, opcode, modrm]
}

fn vex_c4_encoding(p0: u8, p1: u8, opcode: u8, modrm: u8) -> [u8; 5] {
    [0xC4, p0, p1, opcode, modrm]
}

#[test]
fn classifier_covers_all_69_632_safe_canonical_register_encodings() {
    let mut classified = 0usize;

    // C5 implies map 0F and therefore encodes only VPMULUDQ.
    for p1 in u8::MIN..=u8::MAX {
        if p1 & 0x03 != 1 {
            continue;
        }
        for modrm in 0xC0..=0xFF {
            let bytes = vex_c5_encoding(p1, 0xF4, modrm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_widening_dword_multiply_needs_avx2(),
                Some(p1 & 0x04 != 0),
                "{bytes:02X?}"
            );
            classified += 1;
        }
    }

    // C4 supplies map 0F for VPMULUDQ and map 0F38 for VPMULDQ.
    for p0 in u8::MIN..=u8::MAX {
        let opcode = match p0 & 0x1F {
            1 => 0xF4,
            2 => 0x28,
            _ => continue,
        };
        for p1 in u8::MIN..=u8::MAX {
            if p1 & 0x03 != 1 {
                continue;
            }
            for modrm in 0xC0..=0xFF {
                let bytes = vex_c4_encoding(p0, p1, opcode, modrm);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_register_widening_dword_multiply_needs_avx2(),
                    Some(p1 & 0x04 != 0),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }

    assert_eq!(classified, 69_632);
}

#[test]
fn classifier_exhausts_maps_prefix_fields_opcodes_and_modrm_modes() {
    for p1 in u8::MIN..=u8::MAX {
        let bytes = vex_c5_encoding(p1, 0xF4, 0xCB);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_widening_dword_multiply_needs_avx2(),
            (p1 & 0x03 == 1).then_some(p1 & 0x04 != 0),
            "{bytes:02X?}"
        );
    }

    for p0 in u8::MIN..=u8::MAX {
        for (opcode, expected_map) in [(0xF4, 1), (0x28, 2)] {
            let bytes = vex_c4_encoding(p0, 0xE5, opcode, 0xF4);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_widening_dword_multiply_needs_avx2(),
                (p0 & 0x1F == expected_map).then_some(true),
                "{bytes:02X?}"
            );
        }
    }

    for p1 in u8::MIN..=u8::MAX {
        for bytes in [
            vex_c4_encoding(0xA1, p1, 0xF4, 0xC1),
            vex_c4_encoding(0x42, p1, 0x28, 0xFE),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_widening_dword_multiply_needs_avx2(),
                (p1 & 0x03 == 1).then_some(p1 & 0x04 != 0),
                "{bytes:02X?}"
            );
        }
    }

    for opcode in u8::MIN..=u8::MAX {
        for (bytes, expected) in [
            (vex_c5_encoding(0x69, opcode, 0xD2).to_vec(), opcode == 0xF4),
            (
                vex_c4_encoding(0xE1, 0xE1, opcode, 0xE3).to_vec(),
                opcode == 0xF4,
            ),
            (
                vex_c4_encoding(0xE2, 0x65, opcode, 0xF4).to_vec(),
                opcode == 0x28,
            ),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_widening_dword_multiply_needs_avx2()
                    .is_some(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for modrm in u8::MIN..=u8::MAX {
        let expected_128 = (modrm >> 6 == 3).then_some(false);
        let expected_256 = (modrm >> 6 == 3).then_some(true);
        for bytes in [
            vex_c5_encoding(0xE1, 0xF4, modrm).to_vec(),
            vex_c4_encoding(0x21, 0x61, 0xF4, modrm).to_vec(),
            vex_c4_encoding(0x42, 0xE1, 0x28, modrm).to_vec(),
        ] {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_widening_dword_multiply_needs_avx2(),
                expected_128,
                "{bytes:02X?}"
            );
        }
        let bytes = vex_c4_encoding(0xE2, 0x65, 0x28, modrm);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_widening_dword_multiply_needs_avx2(),
            expected_256,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_legacy_evex_noncanonical_lengths_and_memory() {
    let invalid: &[&[u8]] = &[
        &[0x66, 0x0F, 0xF4, 0xC1],
        &[0x66, 0x0F, 0x38, 0x28, 0xC1],
        &[0xC5, 0xE1, 0xF4],
        &[0xC5, 0xE1, 0xF4, 0xC1, 0],
        &[0xC4, 0xE2, 0x65, 0x28],
        &[0xC4, 0xE2, 0x65, 0x28, 0xC1, 0],
        &[0xC4, 0xE1, 0x60, 0xF4, 0xC1], // no mandatory prefix
        &[0xC4, 0xE2, 0x62, 0x28, 0xC1], // F3, not 66
        &[0xC4, 0xE3, 0x61, 0x28, 0xC1], // map 0F3A, not 0F38
        &[0xC5, 0xE1, 0xF4, 0x01],       // memory source
        &[0xC4, 0xE2, 0x65, 0x28, 0x41], // memory source
        &[0x62, 0xF1, 0x6D, 0x08, 0xF4, 0xCB],
        &[0x62, 0xF2, 0x6D, 0x08, 0x28, 0xCB],
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_register_widening_dword_multiply_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_preserve_exact_bytes_and_vector_length_requirement() {
    let pc = 0x1018;
    let mut block = SmirBlock::new(BlockId(37), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_avx2) in [
        (vex_c5_encoding(0x69, 0xF4, 0xCB).to_vec(), false),
        (vex_c4_encoding(0x41, 0xE1, 0xF4, 0xCB).to_vec(), false),
        (vex_c4_encoding(0x22, 0xED, 0x28, 0xF4).to_vec(), true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.vex_register_widening_dword_multiply_needs_avx2(),
            Some(needs_avx2),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.evex_register_integer_multiply_requirements(),
            None,
            "{bytes:02X?}"
        );
        let provenance = HashMap::from([((BlockId(37), pc), instruction)]);
        for spans in [
            x86_vex_widening_dword_multiply_replay_spans(&block, &provenance),
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
            x86_evex_integer_multiply_replay_spans(&block, &provenance).is_empty(),
            "{bytes:02X?}"
        );
    }
}
