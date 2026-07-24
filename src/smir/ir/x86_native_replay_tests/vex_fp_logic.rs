//! Exact classifier and span tests for AVX VEX floating logical replay.

use super::*;

fn c4_encoding(
    opcode: u8,
    extension_bits: u8,
    w: bool,
    encoded_vvvv: u8,
    l: bool,
    pp: u8,
    modrm: u8,
) -> [u8; 5] {
    assert!(matches!(opcode, 0x54..=0x57));
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(encoded_vvvv < 16);
    assert!(pp < 4);
    [
        0xC4,
        extension_bits | 1,
        (if w { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (if l { 0x04 } else { 0 }) | pp,
        opcode,
        modrm,
    ]
}

fn c5_encoding(
    opcode: u8,
    encoded_r: bool,
    encoded_vvvv: u8,
    l: bool,
    pp: u8,
    modrm: u8,
) -> [u8; 4] {
    assert!(matches!(opcode, 0x54..=0x57));
    assert!(encoded_vvvv < 16);
    assert!(pp < 4);
    [
        0xC5,
        (if encoded_r { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (if l { 0x04 } else { 0 }) | pp,
        opcode,
        modrm,
    ]
}

#[test]
fn classifier_accepts_all_294_912_register_encodings() {
    let mut classified = 0usize;
    for opcode in 0x54..=0x57 {
        for pp in 0u8..=1 {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for w in [false, true] {
                    for encoded_vvvv in 0u8..16 {
                        for l in [false, true] {
                            for reg_rm in 0u8..=0x3F {
                                let bytes = c4_encoding(
                                    opcode,
                                    extension_bits,
                                    w,
                                    encoded_vvvv,
                                    l,
                                    pp,
                                    0xC0 | reg_rm,
                                );
                                assert!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .is_vex_register_fp_logic(),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }

            for encoded_r in [false, true] {
                for encoded_vvvv in 0u8..16 {
                    for l in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            let bytes =
                                c5_encoding(opcode, encoded_r, encoded_vvvv, l, pp, 0xC0 | reg_rm);
                            assert!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .is_vex_register_fp_logic(),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 294_912);

    // Independently assembled by LLVM 21.1.8.
    for bytes in [
        &[0xC5, 0xE8, 0x54, 0xCB][..],       // vandps xmm1,xmm2,xmm3
        &[0xC4, 0x41, 0x2D, 0x55, 0xCB][..], // vandnpd ymm9,ymm10,ymm11
        &[0xC4, 0x41, 0x09, 0x56, 0xEF][..], // vorpd xmm13,xmm14,xmm15
        &[0xC5, 0xD4, 0x57, 0xE6][..],       // vxorps ymm4,ymm5,ymm6
    ] {
        assert!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .is_vex_register_fp_logic(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_structural_frontier() {
    let canonical = c4_encoding(0x54, 0xE0, true, 0x0D, true, 0, 0xCA);
    let mut invalid = vec![
        canonical[..4].to_vec(),
        canonical.iter().copied().chain([0]).collect(),
        [0xC5, canonical[2], canonical[3], canonical[4], 0].to_vec(),
    ];
    for (index, value) in [
        (0, 0x62),                       // EVEX, not VEX
        (1, (canonical[1] & !0x1F) | 2), // map 0F38
        (1, (canonical[1] & !0x1F) | 3), // map 0F3A
        (1, canonical[1] & !0x1F),       // reserved map zero
        (2, (canonical[2] & !0x03) | 2), // F3 instead of no prefix/66
        (2, (canonical[2] & !0x03) | 3), // F2 instead of no prefix/66
        (3, 0x53),                       // below range
        (3, 0x58),                       // above range
        (4, canonical[4] & 0x3F),        // memory source
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for bytes in invalid {
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fp_logic(),
            "{bytes:02X?}"
        );
    }

    let c5 = c5_encoding(0x57, true, 3, false, 0, 0xC1);
    for (index, value) in [
        (1, (c5[1] & !0x03) | 2),
        (1, (c5[1] & !0x03) | 3),
        (2, 0x53),
        (2, 0x58),
        (3, c5[3] & 0x3F),
    ] {
        let mut bytes = c5;
        bytes[index] = value;
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fp_logic(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
    let pc = 0x5457;
    let instruction =
        X86InstructionBytes::new(&c5_encoding(0x57, false, 3, true, 1, 0xFF)).unwrap();
    let mut block = SmirBlock::new(BlockId(5), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
    let provenance = std::collections::HashMap::from([((block.id, pc), instruction)]);

    for spans in [
        x86_vex_fp_logic_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("exact VEX floating logic replay span");
        assert_eq!(span.end, 2);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(!span.needs_avx512fp16);
    }
    assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

    block.push_op(SmirOp::new(OpId(2), pc + 4, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}
