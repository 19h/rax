//! Exact source-byte replay classification for register-destination AVX/AVX2
//! VEX 128-bit chunk extracts.

use super::*;

fn encoding(extension_bits: u8, opcode: u8, modrm: u8, immediate: u8) -> [u8; 6] {
    assert_eq!(extension_bits & !0xE0, 0);
    [0xC4, extension_bits | 3, 0x7D, opcode, modrm, immediate]
}

fn assert_classified(bytes: &[u8], needs_avx2: bool, destination: u8) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    assert_eq!(
        instruction.vex_register_chunk_extract_needs_avx2(),
        Some(needs_avx2),
        "{bytes:02X?}"
    );
    assert_eq!(
        instruction.vex_chunk_extract_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_262144_legal_register_byte_encodings() {
    let mut classified = 0usize;
    for extension_bits in (0u8..8).map(|value| value << 5) {
        for (opcode, needs_avx2) in [(0x19, false), (0x39, true)] {
            for modrm in 0xC0..=0xFF {
                let destination = (modrm & 7) + if extension_bits & 0x20 == 0 { 8 } else { 0 };
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = encoding(extension_bits, opcode, modrm, immediate);
                    assert_classified(&bytes, needs_avx2, destination);
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 262_144);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_shape_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let mut bytes = encoding(0xE0, 0x19, 0xCA, 0xA5);
        bytes[1] = p0;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_chunk_extract_needs_avx2(),
            (p0 & 0x1F == 3).then_some(false),
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE3, p1, 0x19, 0xCA, 0x4E];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_chunk_extract_needs_avx2(),
            (p1 == 0x7D).then_some(false),
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        let bytes = encoding(0xE0, opcode, 0xCA, 0x4E);
        let expected = match opcode {
            0x19 => Some(false),
            0x39 => Some(true),
            _ => None,
        };
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_chunk_extract_needs_avx2(),
            expected,
            "{bytes:02X?}"
        );
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = encoding(0xE0, 0x39, modrm, 0x1B);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_chunk_extract_needs_avx2(),
            (modrm >> 6 == 3).then_some(true),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0xC4, 0xE3, 0x7D, 0x19, 0xCA],
        &[0xC4, 0xE3, 0x7D, 0x19, 0xCA, 0, 0],
        &[0xC5, 0xFD, 0x19, 0xCA, 0],
        &[0x62, 0xF3, 0x7D, 0x28, 0x19, 0xCA, 0],
        &[0xC4, 0xE2, 0x7D, 0x19, 0xCA, 0],
        &[0xC4, 0xE3, 0x75, 0x19, 0xCA, 0],
        &[0xC4, 0xE3, 0x79, 0x19, 0xCA, 0],
        &[0xC4, 0xE3, 0xFD, 0x19, 0xCA, 0],
        &[0xC4, 0xE3, 0x7D, 0x18, 0xCA, 0],
        &[0xC4, 0xE3, 0x7D, 0x19, 0x0A, 0],
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_chunk_extract_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_chunk_extract_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn llvm_samples_and_replay_spans_preserve_exact_bytes_without_evex_features() {
    let pc = 0x1039;
    let mut block = SmirBlock::new(BlockId(39), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    // LLVM 23.0.0 independently assembled these four canonical samples.
    for (bytes, needs_avx2, destination) in [
        (&[0xC4, 0xE3, 0x7D, 0x19, 0xD9, 0x00][..], false, 1),
        (&[0xC4, 0x43, 0x7D, 0x19, 0xD9, 0x01], false, 9),
        (&[0xC4, 0x43, 0x7D, 0x39, 0xCB, 0x00], true, 11),
        (&[0xC4, 0x43, 0x7D, 0x39, 0xD9, 0x01], true, 9),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_chunk_extract_needs_avx2(),
            Some(needs_avx2),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_chunk_extract_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );

        let provenance = HashMap::from([((BlockId(39), pc), instruction)]);
        for spans in [
            x86_vex_chunk_extract_replay_spans(&block, &provenance),
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

    assert!(x86_vex_chunk_extract_replay_spans(&block, &HashMap::new()).is_empty());
}
