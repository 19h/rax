//! Exact source-byte replay classification for EVEX vector-chunk insertion.

use super::*;

type InsertShape = (u8, bool, u8);

fn shapes() -> Vec<InsertShape> {
    let mut shapes = Vec::new();
    for opcode in [0x18, 0x38] {
        for w in [false, true] {
            for ll in [1, 2] {
                shapes.push((opcode, w, ll));
            }
        }
    }
    for opcode in [0x1A, 0x3A] {
        for w in [false, true] {
            shapes.push((opcode, w, 2));
        }
    }
    shapes
}

fn requirements(shape: InsertShape) -> (bool, bool) {
    let (opcode, w, ll) = shape;
    (ll != 2, w != matches!(opcode, 0x1A | 0x3A))
}

fn encoding(
    shape: InsertShape,
    destination: u8,
    source1: u8,
    source2: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> [u8; 7] {
    let (opcode, w, ll) = shape;
    assert!(matches!(opcode, 0x18 | 0x1A | 0x38 | 0x3A));
    assert!(destination < 32 && source1 < 32 && source2 < 32 && ll < 3);
    assert!(mask < 8 && (!zeroing || mask != 0));
    let mut p0 = 0xF3;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!source1) & 0x0F) << 3) | 0x05 | if w { 0x80 } else { 0 },
        (ll << 5) | if source1 < 16 { 0x08 } else { 0 } | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
        immediate,
    ]
}

#[test]
fn classifier_covers_55296_legal_register_encodings() {
    let mut classified = 0usize;
    for shape in shapes() {
        for destination in [1u8, 9, 17, 25] {
            for source1 in 0u8..32 {
                for source2 in [2u8, 10, 18, 26] {
                    for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                        for immediate in [0u8, 0x80, 0xFF] {
                            let bytes = encoding(
                                shape,
                                destination,
                                source1,
                                source2,
                                mask,
                                zeroing,
                                immediate,
                            );
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_register_chunk_insert_requirements(),
                                Some(requirements(shape)),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 55_296);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF3, 0x6D, 0x28, 0x18, 0xCB, 0x00], // not EVEX
        &[0x62, 0xF2, 0x6D, 0x28, 0x18, 0xCB, 0x00], // wrong map
        &[0x62, 0xF3, 0x69, 0x28, 0x18, 0xCB, 0x00], // missing fixed-one bit
        &[0x62, 0xF3, 0x6C, 0x28, 0x18, 0xCB, 0x00], // wrong mandatory prefix
        &[0x62, 0xF3, 0x6D, 0x08, 0x18, 0xCB, 0x00], // 128-bit vector
        &[0x62, 0xF3, 0x6D, 0x68, 0x18, 0xCB, 0x00], // reserved L'L=3
        &[0x62, 0xF3, 0x6D, 0x28, 0x1A, 0xCB, 0x00], // 256-bit half chunk
        &[0x62, 0xF3, 0x6D, 0x38, 0x18, 0xCB, 0x00], // reserved EVEX.b
        &[0x62, 0xF3, 0x6D, 0xA8, 0x18, 0xCB, 0x00], // {z} with k0
        &[0x62, 0xF3, 0x6D, 0x28, 0x18, 0x0B, 0x00], // memory source
        &[0x62, 0xF3, 0x6D, 0x28, 0x19, 0xCB, 0x00], // extract opcode
        &[0x62, 0xF3, 0x6D, 0x28, 0x18, 0xCB],       // missing imm8
        &[0x62, 0xF3, 0x6D, 0x28, 0x18, 0xCB, 0, 0], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_chunk_insert_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_encode_vl_and_nonobvious_dq_requirements() {
    let pc = 0x4000;
    let mut block = SmirBlock::new(BlockId(21), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        encoding((0x18, false, 1), 17, 18, 19, 1, false, 0x01),
        encoding((0x38, true, 2), 25, 26, 27, 2, true, 0x03),
        encoding((0x1A, false, 2), 1, 2, 3, 0, false, 0xFF),
        encoding((0x3A, true, 2), 9, 10, 11, 1, false, 0xFE),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(21), pc), instruction)]);
        let expected = instruction
            .evex_register_chunk_insert_requirements()
            .unwrap();
        for spans in [
            x86_evex_chunk_insert_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, expected.0, "{bytes:02X?}");
            assert_eq!(span.needs_avx512dq, expected.1, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}
