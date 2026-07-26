//! Exact source-byte replay classification for EVEX FP32/FP64 flag compares.

use super::*;

#[derive(Clone, Copy, Debug)]
enum Format {
    F32,
    F64,
}

impl Format {
    const ALL: [Self; 2] = [Self::F32, Self::F64];

    fn p1(self) -> u8 {
        match self {
            Self::F32 => 0x7C,
            Self::F64 => 0xFD,
        }
    }
}

fn encoding(
    format: Format,
    opcode: u8,
    src1: u8,
    src2: u8,
    ll: u8,
    suppress_exceptions: bool,
) -> [u8; 6] {
    assert!(matches!(opcode, 0x2E | 0x2F));
    assert!(src1 < 32 && src2 < 32 && ll < 4);
    let mut p0 = 0xF1;
    if src1 & 0x08 != 0 {
        p0 &= !0x80;
    }
    if src1 & 0x10 != 0 {
        p0 &= !0x10;
    }
    if src2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if src2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        format.p1(),
        (ll << 5) | if suppress_exceptions { 0x10 } else { 0 } | 0x08,
        opcode,
        0xC0 | ((src1 & 7) << 3) | (src2 & 7),
    ]
}

#[test]
fn classifier_covers_all_32768_legal_format_extension_llig_and_sae_encodings() {
    let mut classified = 0usize;
    for format in Format::ALL {
        for opcode in [0x2E, 0x2F] {
            for src1 in 0..32 {
                for src2 in 0..32 {
                    for ll in 0..4 {
                        for suppress_exceptions in [false, true] {
                            let bytes =
                                encoding(format, opcode, src1, src2, ll, suppress_exceptions);
                            let expected = Some((false, false));
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_register_fp32_fp64_flag_compare_requirements(),
                                expected,
                                "{format:?} {bytes:02X?}"
                            );
                            classified += usize::from(expected.is_some());
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 32_768);
}

#[test]
fn classifier_accepts_independently_assembled_samples_and_rejects_frontiers() {
    for bytes in [
        [0x62, 0xF1, 0x7C, 0x08, 0x2E, 0xD3],
        [0x62, 0xF1, 0x7C, 0x18, 0x2F, 0xD3],
        [0x62, 0xF1, 0xFD, 0x08, 0x2E, 0xD3],
        [0x62, 0xF1, 0xFD, 0x18, 0x2F, 0xD3],
        [0x62, 0x01, 0x7C, 0x18, 0x2E, 0xF7],
        [0x62, 0x01, 0xFD, 0x18, 0x2F, 0xF7],
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp32_fp64_flag_compare_requirements(),
            Some((false, false)),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0x7C, 0x08, 0x2F, 0xD3],       // not EVEX
        &[0x62, 0xF2, 0x7C, 0x08, 0x2F, 0xD3],       // MAP2, not MAP1
        &[0x62, 0xF9, 0x7C, 0x08, 0x2F, 0xD3],       // reserved P0 bit 3
        &[0x62, 0xF1, 0xFC, 0x08, 0x2F, 0xD3],       // F32 with W1
        &[0x62, 0xF1, 0x7D, 0x08, 0x2F, 0xD3],       // F64 with W0
        &[0x62, 0xF1, 0x74, 0x08, 0x2F, 0xD3],       // reserved vvvv
        &[0x62, 0xF1, 0x7C, 0x00, 0x2F, 0xD3],       // reserved V'
        &[0x62, 0xF1, 0x7C, 0x09, 0x2F, 0xD3],       // reserved opmask
        &[0x62, 0xF1, 0x7C, 0x88, 0x2F, 0xD3],       // reserved zeroing
        &[0x62, 0xF1, 0x7C, 0x08, 0x2D, 0xD3],       // unrelated opcode
        &[0x62, 0xF1, 0x7C, 0x08, 0x2F, 0x13],       // memory source
        &[0x62, 0xF1, 0x7C, 0x08, 0x2F],             // missing ModR/M
        &[0x62, 0xF1, 0x7C, 0x08, 0x2F, 0xD3, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp32_fp64_flag_compare_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_neither_vl_dq_nor_fp16() {
    let pc = 0x2F00;
    let mut block = SmirBlock::new(BlockId(48), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for format in Format::ALL {
        for opcode in [0x2E, 0x2F] {
            for suppress_exceptions in [false, true] {
                let bytes = encoding(format, opcode, 30, 31, 2, suppress_exceptions);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let provenance = HashMap::from([((BlockId(48), pc), instruction)]);
                for spans in [
                    x86_evex_fp_compare_replay_spans(&block, &provenance),
                    x86_evex_native_replay_spans(&block, &provenance),
                ] {
                    let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(span.end, 1, "{bytes:02X?}");
                    assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                    assert!(!span.needs_avx512vl, "{bytes:02X?}");
                    assert!(!span.needs_avx512dq, "{bytes:02X?}");
                    assert!(!span.needs_avx512fp16, "{bytes:02X?}");
                }
            }
        }
    }
}
