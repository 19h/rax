//! Exact source-byte replay classification for register-only AVX/AVX2 VEX
//! immediate permutes.

use super::*;

#[derive(Clone, Copy)]
struct Shape {
    opcode: u8,
    ymm: bool,
    w: bool,
    needs_avx2: bool,
}

const SHAPES: [Shape; 6] = [
    Shape {
        opcode: 0x04,
        ymm: false,
        w: false,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x04,
        ymm: true,
        w: false,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x05,
        ymm: false,
        w: false,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x05,
        ymm: true,
        w: false,
        needs_avx2: false,
    },
    Shape {
        opcode: 0x00,
        ymm: true,
        w: true,
        needs_avx2: true,
    },
    Shape {
        opcode: 0x01,
        ymm: true,
        w: true,
        needs_avx2: true,
    },
];

fn encoding(extension_bits: u8, shape: Shape, modrm: u8, immediate: u8) -> [u8; 6] {
    assert_eq!(extension_bits & !0xE0, 0);
    [
        0xC4,
        extension_bits | 3,
        (u8::from(shape.w) << 7) | 0x78 | (u8::from(shape.ymm) << 2) | 1,
        shape.opcode,
        modrm,
        immediate,
    ]
}

fn assert_classified(bytes: &[u8], needs_avx2: bool, destination: u8) {
    let instruction = X86InstructionBytes::new(bytes).unwrap();
    assert_eq!(
        instruction.vex_register_immediate_permute_needs_avx2(),
        Some(needs_avx2),
        "{bytes:02X?}"
    );
    assert_eq!(
        instruction.vex_immediate_permute_destination_index(),
        Some(destination),
        "{bytes:02X?}"
    );
}

#[test]
fn classifier_covers_all_786432_legal_register_byte_encodings() {
    let mut classified = 0usize;
    for extension_bits in (0u8..8).map(|value| value << 5) {
        for shape in SHAPES {
            for modrm in 0xC0..=0xFF {
                let destination =
                    ((modrm >> 3) & 7) + if extension_bits & 0x80 == 0 { 8 } else { 0 };
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = encoding(extension_bits, shape, modrm, immediate);
                    assert_classified(&bytes, shape.needs_avx2, destination);
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 786_432);
}

#[test]
fn classifier_exhausts_prefix_opcode_modrm_and_shape_frontiers() {
    let permil = SHAPES[0];
    for p0 in u8::MIN..=u8::MAX {
        let mut bytes = encoding(0xE0, permil, 0xCA, 0xA5);
        bytes[1] = p0;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_immediate_permute_needs_avx2(),
            (p0 & 0x1F == 3).then_some(false),
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = encoding(0xE0, permil, 0xCA, 0xA5);
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_immediate_permute_needs_avx2(),
            (p1 & 0x78 == 0x78 && p1 & 0x03 == 1 && p1 & 0x80 == 0).then_some(false),
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        for (p1, expected) in [
            (0x79, matches!(opcode, 0x04 | 0x05).then_some(false)),
            (0x7D, matches!(opcode, 0x04 | 0x05).then_some(false)),
            (0xFD, matches!(opcode, 0x00 | 0x01).then_some(true)),
        ] {
            let bytes = [0xC4, 0xE3, p1, opcode, 0xCA, 0x4E];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_register_immediate_permute_needs_avx2(),
                expected,
                "{bytes:02X?}"
            );
        }
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = encoding(0xE0, SHAPES[5], modrm, 0x1B);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_immediate_permute_needs_avx2(),
            (modrm >> 6 == 3).then_some(true),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0xC4, 0xE3, 0x79, 0x04, 0xCA],
        &[0xC4, 0xE3, 0x79, 0x04, 0xCA, 0, 0],
        &[0xC5, 0xF9, 0x04, 0xCA, 0],
        &[0x62, 0xF3, 0x7D, 0x08, 0x04, 0xCA, 0],
        &[0xC4, 0xE2, 0x79, 0x04, 0xCA, 0],
        &[0xC4, 0xE3, 0x71, 0x04, 0xCA, 0],
        &[0xC4, 0xE3, 0x7A, 0x04, 0xCA, 0],
        &[0xC4, 0xE3, 0xF9, 0x04, 0xCA, 0],
        &[0xC4, 0xE3, 0x79, 0x00, 0xCA, 0],
        &[0xC4, 0xE3, 0xF9, 0x00, 0xCA, 0],
        &[0xC4, 0xE3, 0x7D, 0x01, 0xCA, 0],
        &[0xC4, 0xE3, 0x79, 0x04, 0x0A, 0],
        &[0xC4, 0xE3, 0x79, 0x06, 0xCA, 0],
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_immediate_permute_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_immediate_permute_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn llvm_samples_and_replay_spans_preserve_exact_bytes_without_evex_features() {
    let pc = 0x1041;
    let mut block = SmirBlock::new(BlockId(43), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    // LLVM 23.0.0 independently assembled these six canonical samples.
    for (bytes, needs_avx2, destination) in [
        (&[0xC4, 0xE3, 0x79, 0x04, 0xCB, 0x1B][..], false, 1),
        (&[0xC4, 0x43, 0x7D, 0x04, 0xCB, 0xE4], false, 9),
        (&[0xC4, 0x43, 0x79, 0x05, 0xD9, 0x02], false, 11),
        (&[0xC4, 0x43, 0x7D, 0x05, 0xCB, 0x0D], false, 9),
        (&[0xC4, 0x43, 0xFD, 0x00, 0xD9, 0x1B], true, 11),
        (&[0xC4, 0x43, 0xFD, 0x01, 0xCB, 0xE4], true, 9),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_immediate_permute_needs_avx2(),
            Some(needs_avx2),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_immediate_permute_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );

        let provenance = HashMap::from([((BlockId(43), pc), instruction)]);
        for spans in [
            x86_vex_immediate_permute_replay_spans(&block, &provenance),
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

    assert!(x86_vex_immediate_permute_replay_spans(&block, &HashMap::new()).is_empty());
}
