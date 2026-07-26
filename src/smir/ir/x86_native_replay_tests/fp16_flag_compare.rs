//! Exact source-byte replay classification for FP16 flag compares.

use super::*;

fn encoding(opcode: u8, src1: u8, src2: u8, ll: u8, suppress_exceptions: bool) -> [u8; 6] {
    assert!(matches!(opcode, 0x2E | 0x2F));
    assert!(src1 < 32 && src2 < 32 && ll < 4);
    let mut p0 = 0xF5;
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
        0x7C,
        (ll << 5) | if suppress_exceptions { 0x10 } else { 0 } | 0x08,
        opcode,
        0xC0 | ((src1 & 7) << 3) | (src2 & 7),
    ]
}

#[test]
fn classifier_covers_all_14336_legal_register_extension_llig_and_sae_encodings() {
    let mut classified = 0usize;
    for opcode in [0x2E, 0x2F] {
        for src1 in 0..32 {
            for src2 in 0..32 {
                for ll in 0..4 {
                    for suppress_exceptions in [false, true] {
                        let bytes = encoding(opcode, src1, src2, ll, suppress_exceptions);
                        let expected = (suppress_exceptions || ll != 3).then_some((false, true));
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_fp16_flag_compare_requirements(),
                            expected,
                            "{bytes:02X?}"
                        );
                        classified += usize::from(expected.is_some());
                    }
                }
            }
        }
    }
    assert_eq!(classified, 14_336);

    // Independently assembled by LLVM 21.1.8 with +avx512fp16.
    for bytes in [
        [0x62, 0xF5, 0x7C, 0x08, 0x2F, 0xD3],
        [0x62, 0xF5, 0x7C, 0x08, 0x2E, 0xD3],
        [0x62, 0xF5, 0x7C, 0x18, 0x2F, 0xD3],
        [0x62, 0xA5, 0x7C, 0x08, 0x2F, 0xD3],
        [0x62, 0x05, 0x7C, 0x18, 0x2E, 0xF7],
    ] {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_register_fp16_flag_compare_requirements(),
            Some((false, true)),
            "{bytes:02X?}"
        );
    }

    // Intel XED 2026.07.15 independently accepts L'L=11b when EVEX.b selects
    // SAE, while rejecting the same L'L value without SAE.
    let sae_ll3 = [0x62, 0xF5, 0x7C, 0x78, 0x2F, 0xD3];
    assert_eq!(
        X86InstructionBytes::new(&sae_ll3)
            .unwrap()
            .evex_register_fp16_flag_compare_requirements(),
        Some((false, true))
    );
}

#[test]
fn classifier_rejects_every_reserved_or_unsafe_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF5, 0x7C, 0x08, 0x2F, 0xD3],       // not EVEX
        &[0x62, 0xF4, 0x7C, 0x08, 0x2F, 0xD3],       // MAP4, not MAP5
        &[0x62, 0xFD, 0x7C, 0x08, 0x2F, 0xD3],       // reserved P0 bit 3
        &[0x62, 0xF5, 0xFC, 0x08, 0x2F, 0xD3],       // W1
        &[0x62, 0xF5, 0x74, 0x08, 0x2F, 0xD3],       // reserved vvvv
        &[0x62, 0xF5, 0x7D, 0x08, 0x2F, 0xD3],       // mandatory prefix
        &[0x62, 0xF5, 0x7C, 0x00, 0x2F, 0xD3],       // reserved V'
        &[0x62, 0xF5, 0x7C, 0x09, 0x2F, 0xD3],       // reserved opmask
        &[0x62, 0xF5, 0x7C, 0x88, 0x2F, 0xD3],       // reserved zeroing
        &[0x62, 0xF5, 0x7C, 0x68, 0x2F, 0xD3],       // reserved no-SAE L'L=11b
        &[0x62, 0xF5, 0x7C, 0x08, 0x2D, 0xD3],       // unrelated opcode
        &[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0x13],       // memory source
        &[0x62, 0xF5, 0x7C, 0x08, 0x2F],             // missing ModR/M
        &[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0xD3, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp16_flag_compare_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_fp16_without_vl_or_dq() {
    let pc = 0x2F00;
    let mut block = SmirBlock::new(BlockId(47), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for opcode in [0x2E, 0x2F] {
        for suppress_exceptions in [false, true] {
            let bytes = encoding(opcode, 30, 31, 2, suppress_exceptions);
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            let provenance = HashMap::from([((BlockId(47), pc), instruction)]);
            for spans in [
                x86_evex_fp_compare_replay_spans(&block, &provenance),
                x86_evex_native_replay_spans(&block, &provenance),
            ] {
                let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                assert_eq!(span.end, 1, "{bytes:02X?}");
                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                assert!(!span.needs_avx512vl, "{bytes:02X?}");
                assert!(!span.needs_avx512dq, "{bytes:02X?}");
                assert!(span.needs_avx512fp16, "{bytes:02X?}");
            }
        }
    }
}
