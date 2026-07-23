//! Exact source-byte replay classification for AVX-512BW immediate transforms.

use super::*;

type BwImmediateShape = (u8, u8, bool);

fn shapes() -> Vec<BwImmediateShape> {
    let mut shapes = Vec::new();
    for ll in 0..=2 {
        for w in [false, true] {
            shapes.push((0x0F, ll, w));
        }
        shapes.push((0x42, ll, false));
    }
    shapes
}

fn encoding(
    shape: BwImmediateShape,
    destination: u8,
    source1: u8,
    source2: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> [u8; 7] {
    let (opcode, ll, w) = shape;
    assert!(opcode == 0x0F || (opcode == 0x42 && !w));
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
fn classifier_covers_41472_legal_register_encodings() {
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
                                    .evex_register_bw_immediate_needs_vl(),
                                Some(shape.1 != 2),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 41_472);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF3, 0x6D, 0x08, 0x0F, 0xCB, 0x00], // not EVEX
        &[0x62, 0xF2, 0x6D, 0x08, 0x0F, 0xCB, 0x00], // wrong map
        &[0x62, 0xF3, 0x69, 0x08, 0x0F, 0xCB, 0x00], // missing fixed-one bit
        &[0x62, 0xF3, 0x6C, 0x08, 0x0F, 0xCB, 0x00], // wrong mandatory prefix
        &[0x62, 0xF3, 0x6D, 0x68, 0x0F, 0xCB, 0x00], // reserved L'L=3
        &[0x62, 0xF3, 0x6D, 0x18, 0x0F, 0xCB, 0x00], // reserved EVEX.b
        &[0x62, 0xF3, 0x6D, 0x88, 0x0F, 0xCB, 0x00], // {z} with k0
        &[0x62, 0xF3, 0x6D, 0x08, 0x0F, 0x0B, 0x00], // memory source
        &[0x62, 0xF3, 0x6D, 0x08, 0x41, 0xCB, 0x00], // unrelated opcode
        &[0x62, 0xF3, 0xED, 0x08, 0x42, 0xCB, 0x00], // VDBPSADBW with W1
        &[0x62, 0xF3, 0x6D, 0x08, 0x0F, 0xCB],       // missing imm8
        &[0x62, 0xF3, 0x6D, 0x08, 0x0F, 0xCB, 0, 0], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_bw_immediate_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_only_vl_for_128_and_256_bit_forms() {
    let pc = 0x3F00;
    let mut block = SmirBlock::new(BlockId(20), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        encoding((0x0F, 0, false), 17, 18, 19, 1, false, 0x1F),
        encoding((0x0F, 1, true), 25, 26, 27, 2, true, 0x20),
        encoding((0x42, 2, false), 1, 2, 3, 0, false, 0xB1),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(20), pc), instruction)]);
        for spans in [
            x86_evex_bw_immediate_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, bytes[3] & 0x60 != 0x40, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}
