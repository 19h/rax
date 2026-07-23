//! Exact source-byte replay classification for EVEX vector-chunk extraction.

use super::*;

type ExtractShape = (u8, bool, u8);

fn shapes() -> Vec<ExtractShape> {
    let mut shapes = Vec::new();
    for opcode in [0x19, 0x39] {
        for w in [false, true] {
            for ll in [1, 2] {
                shapes.push((opcode, w, ll));
            }
        }
    }
    for opcode in [0x1B, 0x3B] {
        for w in [false, true] {
            shapes.push((opcode, w, 2));
        }
    }
    shapes
}

fn requirements(shape: ExtractShape) -> (bool, bool) {
    let (opcode, w, ll) = shape;
    (ll != 2, w != matches!(opcode, 0x1B | 0x3B))
}

fn encoding(
    shape: ExtractShape,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> [u8; 7] {
    let (opcode, w, ll) = shape;
    assert!(matches!(opcode, 0x19 | 0x1B | 0x39 | 0x3B));
    assert!(destination < 32 && source < 32 && ll < 3);
    assert!(mask < 8 && (!zeroing || mask != 0));
    let mut p0 = 0xF3;
    if source & 0x08 != 0 {
        p0 &= !0x80;
    }
    if source & 0x10 != 0 {
        p0 &= !0x10;
    }
    if destination & 0x08 != 0 {
        p0 &= !0x20;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((source & 0x07) << 3) | (destination & 0x07),
        immediate,
    ]
}

#[test]
fn classifier_covers_55296_legal_register_encodings() {
    let destinations = [0u8, 1, 2, 3, 8, 9, 10, 11, 16, 17, 18, 19, 24, 25, 26, 27];
    let mut classified = 0usize;
    for shape in shapes() {
        for destination in destinations {
            for source in 0u8..32 {
                for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                    for immediate in [0u8, 0x80, 0xFF] {
                        let bytes = encoding(shape, destination, source, mask, zeroing, immediate);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_chunk_extract_requirements(),
                            Some(requirements(shape)),
                            "{bytes:02X?}"
                        );
                        classified += 1;
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
        &[0x61, 0xF3, 0x7D, 0x29, 0x19, 0xCA, 0x00], // not EVEX
        &[0x62, 0xF2, 0x7D, 0x29, 0x19, 0xCA, 0x00], // wrong map
        &[0x62, 0xF3, 0x79, 0x29, 0x19, 0xCA, 0x00], // missing fixed-one bit
        &[0x62, 0xF3, 0x7C, 0x29, 0x19, 0xCA, 0x00], // wrong mandatory prefix
        &[0x62, 0xF3, 0x7D, 0x09, 0x19, 0xCA, 0x00], // 128-bit source
        &[0x62, 0xF3, 0x7D, 0x69, 0x19, 0xCA, 0x00], // reserved L'L=3
        &[0x62, 0xF3, 0x7D, 0x29, 0x1B, 0xCA, 0x00], // 256-bit half chunk
        &[0x62, 0xF3, 0x7D, 0x39, 0x19, 0xCA, 0x00], // reserved EVEX.b
        &[0x62, 0xF3, 0x7D, 0xA8, 0x19, 0xCA, 0x00], // {z} with k0
        &[0x62, 0xF3, 0x7D, 0x29, 0x19, 0x0A, 0x00], // memory destination
        &[0x62, 0xF3, 0x7D, 0x29, 0x18, 0xCA, 0x00], // insert opcode
        &[0x62, 0xF3, 0x75, 0x29, 0x19, 0xCA, 0x00], // EVEX.vvvv != 1111b
        &[0x62, 0xF3, 0x7D, 0x21, 0x19, 0xCA, 0x00], // EVEX.V' != 1
        &[0x62, 0xF3, 0x7D, 0x29, 0x19, 0xCA],       // missing imm8
        &[0x62, 0xF3, 0x7D, 0x29, 0x19, 0xCA, 0, 0], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_chunk_extract_requirements(),
            None,
            "{bytes:02X?}"
        );
    }

    let valid = encoding((0x19, false, 1), 1, 2, 1, false, 0xFF);
    for encoded_vvvv in 0u8..=0x0E {
        let mut reserved_vvvv = valid;
        reserved_vvvv[2] = (reserved_vvvv[2] & !0x78) | (encoded_vvvv << 3);
        assert_eq!(
            X86InstructionBytes::new(&reserved_vvvv)
                .unwrap()
                .evex_register_chunk_extract_requirements(),
            None,
            "{reserved_vvvv:02X?}"
        );
    }
}

#[test]
fn replay_spans_encode_vl_and_nonobvious_dq_requirements() {
    let pc = 0x4000;
    let mut block = SmirBlock::new(BlockId(22), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        encoding((0x19, false, 1), 17, 18, 1, false, 0x01),
        encoding((0x39, true, 2), 25, 26, 2, true, 0x03),
        encoding((0x1B, false, 2), 1, 2, 0, false, 0xFF),
        encoding((0x3B, true, 2), 9, 10, 1, false, 0xFE),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(22), pc), instruction)]);
        let expected = instruction
            .evex_register_chunk_extract_requirements()
            .unwrap();
        for spans in [
            x86_evex_chunk_extract_replay_spans(&block, &provenance),
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
