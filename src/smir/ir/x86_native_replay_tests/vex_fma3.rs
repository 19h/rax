//! Exact classifier and span tests for AVX VEX FMA3 replay.

use super::*;

fn encoding(
    opcode: u8,
    extension_bits: u8,
    w: bool,
    encoded_vvvv: u8,
    l: bool,
    modrm: u8,
) -> [u8; 5] {
    assert!(matches!(opcode, 0x96..=0x9F | 0xA6..=0xAF | 0xB6..=0xBF));
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(encoded_vvvv < 16);
    [
        0xC4,
        extension_bits | 2,
        (if w { 0x80 } else { 0 }) | (encoded_vvvv << 3) | (if l { 0x04 } else { 0 }) | 1,
        opcode,
        modrm,
    ]
}

#[test]
fn classifier_accepts_all_983_040_canonical_register_encodings() {
    let mut classified = 0usize;
    for opcode in (0x96..=0x9F).chain(0xA6..=0xAF).chain(0xB6..=0xBF) {
        for extension_bits in (0u8..8).map(|value| value << 5) {
            for w in [false, true] {
                for encoded_vvvv in 0u8..16 {
                    for l in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            let bytes =
                                encoding(opcode, extension_bits, w, encoded_vvvv, l, 0xC0 | reg_rm);
                            assert!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .is_vex_register_fma3(),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 983_040);

    // Independently assembled by LLVM 21.1.8.
    for bytes in [
        [0xC4, 0xE2, 0x69, 0x98, 0xCB], // vfmadd132ps xmm1,xmm2,xmm3
        [0xC4, 0xE2, 0xED, 0xA7, 0xCB], // vfmsubadd213pd ymm1,ymm2,ymm3
        [0xC4, 0x42, 0xA9, 0xBF, 0xD1], // vfnmsub231sd xmm10,xmm10,xmm9
    ] {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fma3(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_every_structural_frontier() {
    let canonical = encoding(0x98, 0xE0, true, 0x0D, true, 0xCA);
    let mut invalid = vec![
        canonical[..4].to_vec(),
        canonical.iter().copied().chain([0]).collect(),
        [0xC5, canonical[1], canonical[2], canonical[3], canonical[4]].to_vec(),
    ];
    for (index, value) in [
        (1, (canonical[1] & !0x1F) | 1), // map 0F
        (1, (canonical[1] & !0x1F) | 3), // map 0F3A
        (1, canonical[1] & !0x1F),       // reserved map zero
        (2, canonical[2] & !0x03),       // no mandatory prefix
        (2, (canonical[2] & !0x03) | 2), // F3 instead of 66
        (2, (canonical[2] & !0x03) | 3), // F2 instead of 66
        (3, 0x95),                       // below first range
        (3, 0xA5),                       // gap before second range
        (3, 0xB5),                       // gap before third range
        (3, 0xC0),                       // above final range
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
                .is_vex_register_fma3(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
    let pc = 0x98A8;
    let instruction =
        X86InstructionBytes::new(&encoding(0xBE, 0x40, false, 3, true, 0xFF)).unwrap();
    let mut block = SmirBlock::new(BlockId(5), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
    let provenance = std::collections::HashMap::from([((block.id, pc), instruction)]);

    for spans in [
        x86_vex_fma3_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("exact VEX FMA3 replay span");
        assert_eq!(span.end, 2);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(!span.needs_avx512fp16);
    }
    assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

    block.push_op(SmirOp::new(OpId(2), pc + 5, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}
