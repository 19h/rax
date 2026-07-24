//! Exact classifier and span tests for AVX VEX packed-string replay.

use super::*;

fn encoding(opcode: u8, w: bool, r: bool, x: bool, b: bool, modrm: u8, imm: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x60..=0x63));
    [
        0xC4,
        (if r { 0 } else { 0x80 }) | (if x { 0 } else { 0x40 }) | (if b { 0 } else { 0x20 }) | 3,
        (if w { 0x80 } else { 0 }) | 0x79,
        opcode,
        modrm,
        imm,
    ]
}

#[test]
fn classifier_accepts_all_1_048_576_canonical_register_encodings() {
    let mut classified = 0usize;
    for opcode in 0x60..=0x63 {
        for w in [false, true] {
            for r in [false, true] {
                for x in [false, true] {
                    for b in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            for imm in u8::MIN..=u8::MAX {
                                let bytes = encoding(opcode, w, r, x, b, 0xC0 | reg_rm, imm);
                                assert!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .is_vex_register_packed_string_compare(),
                                    "{bytes:02X?}"
                                );
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 1_048_576);

    // Independently assembled by LLVM 21.1.8.
    for bytes in [
        [0xC4, 0xE3, 0x79, 0x60, 0xCA, 0x00],
        [0xC4, 0xE3, 0x79, 0x61, 0xCA, 0x00],
        [0xC4, 0xE3, 0x79, 0x62, 0xCA, 0x00],
        [0xC4, 0xE3, 0x79, 0x63, 0xCA, 0x00],
    ] {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_packed_string_compare(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_structural_frontier() {
    let canonical = encoding(0x60, true, true, true, true, 0xCA, 0xA5);
    let mut invalid = vec![
        canonical[..5].to_vec(),
        canonical.iter().copied().chain([0]).collect(),
        [
            0xC5,
            canonical[1],
            canonical[2],
            canonical[3],
            canonical[4],
            canonical[5],
        ]
        .to_vec(),
    ];
    for (index, value) in [
        (1, (canonical[1] & !0x1F) | 1), // map 0F
        (1, (canonical[1] & !0x1F) | 2), // map 0F38
        (2, canonical[2] & !0x01),       // no mandatory 66H
        (2, canonical[2] | 0x04),        // VEX.L = 1
        (2, canonical[2] & !0x08),       // VEX.vvvv != 1111b
        (3, 0x5F),                       // neighboring opcode
        (3, 0x64),                       // neighboring opcode
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
                .is_vex_register_packed_string_compare(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
    let pc = 0x6063;
    let instruction =
        X86InstructionBytes::new(&encoding(0x63, true, true, false, true, 0xFF, 0x7F)).unwrap();
    let mut block = SmirBlock::new(BlockId(4), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
    let provenance = std::collections::HashMap::from([((block.id, pc), instruction)]);

    for spans in [
        x86_vex_packed_string_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("exact VEX replay span");
        assert_eq!(span.end, 2);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(!span.needs_avx512fp16);
    }
    assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

    block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}
