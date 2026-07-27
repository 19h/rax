//! Exact source-byte replay classification for register-only AVX2 VEX scalar
//! broadcasts.

use super::*;

#[derive(Clone, Copy)]
struct Shape {
    opcode: u8,
    ymm: bool,
    element_bits: u8,
}

const SHAPES: [Shape; 11] = [
    Shape {
        opcode: 0x18,
        ymm: false,
        element_bits: 32,
    },
    Shape {
        opcode: 0x18,
        ymm: true,
        element_bits: 32,
    },
    Shape {
        opcode: 0x19,
        ymm: true,
        element_bits: 64,
    },
    Shape {
        opcode: 0x58,
        ymm: false,
        element_bits: 32,
    },
    Shape {
        opcode: 0x58,
        ymm: true,
        element_bits: 32,
    },
    Shape {
        opcode: 0x59,
        ymm: false,
        element_bits: 64,
    },
    Shape {
        opcode: 0x59,
        ymm: true,
        element_bits: 64,
    },
    Shape {
        opcode: 0x78,
        ymm: false,
        element_bits: 8,
    },
    Shape {
        opcode: 0x78,
        ymm: true,
        element_bits: 8,
    },
    Shape {
        opcode: 0x79,
        ymm: false,
        element_bits: 16,
    },
    Shape {
        opcode: 0x79,
        ymm: true,
        element_bits: 16,
    },
];

fn assert_classified(bytes: &[u8], element_bits: u8, destination: u8) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    assert_eq!(
        instruction.vex_register_broadcast_element_bits(),
        Some(element_bits),
        "{bytes:02X?}"
    );
    assert_eq!(
        instruction.vex_register_broadcast_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_5632_legal_register_byte_encodings() {
    let mut classified = 0usize;
    for extension_bits in 0u8..=7 {
        let p0 = (extension_bits << 5) | 2;
        for shape in SHAPES {
            let p1 = 0x79 | (u8::from(shape.ymm) << 2);
            for modrm in 0xC0..=0xFF {
                let destination = ((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 };
                let bytes = [0xC4, p0, p1, shape.opcode, modrm];
                assert_classified(&bytes, shape.element_bits, destination);
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 5_632);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_shape_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let bytes = [0xC4, p0, 0x79, 0x18, 0xCA];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_broadcast_element_bits(),
            (p0 & 0x1F == 2).then_some(32),
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE2, p1, 0x18, 0xCA];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_broadcast_element_bits(),
            (p1 & 0xF8 == 0x78 && p1 & 0x03 == 1).then_some(32),
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        for ymm in [false, true] {
            let bytes = [0xC4, 0xE2, 0x79 | (u8::from(ymm) << 2), opcode, 0xCA];
            let expected = match (opcode, ymm) {
                (0x18 | 0x58, _) => Some(32),
                (0x19, true) | (0x59, _) => Some(64),
                (0x78, _) => Some(8),
                (0x79, _) => Some(16),
                _ => None,
            };
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_broadcast_element_bits(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = [0xC4, 0xE2, 0x7D, 0x59, modrm];
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_broadcast_element_bits(),
            (modrm >> 6 == 3).then_some(64),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0xC4, 0xE2, 0x79, 0x18],
        &[0xC4, 0xE2, 0x79, 0x18, 0xCA, 0],
        &[0xC5, 0xF9, 0x18, 0xCA],
        &[0x62, 0xF2, 0x7D, 0x08, 0x18, 0xCA],
        &[0xC4, 0xE1, 0x79, 0x18, 0xCA],
        &[0xC4, 0xE2, 0xF9, 0x18, 0xCA],
        &[0xC4, 0xE2, 0x71, 0x18, 0xCA],
        &[0xC4, 0xE2, 0x7A, 0x18, 0xCA],
        &[0xC4, 0xE2, 0x79, 0x19, 0xCA],
        &[0xC4, 0xE2, 0x79, 0x18, 0x0A],
        &[0xC4, 0xE2, 0x79, 0x1A, 0xCA],
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_broadcast_element_bits(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_register_broadcast_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn llvm_samples_and_replay_spans_preserve_exact_bytes_without_evex_features() {
    let pc = 0x1041;
    let mut block = SmirBlock::new(BlockId(41), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    // LLVM 23.0.0 independently assembled these six canonical samples.
    for (bytes, element_bits, destination) in [
        (&[0xC4, 0xE2, 0x79, 0x18, 0xCB][..], 32, 1),
        (&[0xC4, 0x42, 0x7D, 0x19, 0xCB], 64, 9),
        (&[0xC4, 0x42, 0x79, 0x78, 0xD9], 8, 11),
        (&[0xC4, 0x42, 0x7D, 0x79, 0xCB], 16, 9),
        (&[0xC4, 0x42, 0x79, 0x58, 0xD9], 32, 11),
        (&[0xC4, 0x42, 0x7D, 0x59, 0xCB], 64, 9),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_broadcast_element_bits(),
            Some(element_bits),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_register_broadcast_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );

        let provenance = HashMap::from([((BlockId(41), pc), instruction)]);
        for spans in [
            x86_vex_register_broadcast_replay_spans(&block, &provenance),
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

    assert!(x86_vex_register_broadcast_replay_spans(&block, &HashMap::new()).is_empty());
}
