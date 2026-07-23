//! Exact source-byte replay classification for EVEX 128-bit-chunk shuffles.

use super::*;

type ChunkShuffleShape = (u8, bool, u8);

fn encoding(
    shape: ChunkShuffleShape,
    destination: u8,
    source1: u8,
    source2: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> [u8; 7] {
    let (opcode, w, ll) = shape;
    assert!(matches!(opcode, 0x23 | 0x43));
    assert!(destination < 32 && source1 < 32 && source2 < 32 && matches!(ll, 1 | 2));
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
fn classifier_covers_36864_legal_register_encodings() {
    let mut classified = 0usize;
    for opcode in [0x23u8, 0x43] {
        for w in [false, true] {
            for ll in [1u8, 2] {
                for destination in [1u8, 9, 17, 25] {
                    for source1 in 0u8..32 {
                        for source2 in [2u8, 10, 18, 26] {
                            for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                                for immediate in [0u8, 0x4E, 0xFF] {
                                    let bytes = encoding(
                                        (opcode, w, ll),
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
                                            .evex_register_chunk_shuffle_needs_vl(),
                                        Some(ll == 1),
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
    }
    assert_eq!(classified, 36_864);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF3, 0x6D, 0x28, 0x23, 0xCB, 0x4E], // not EVEX
        &[0x62, 0xF2, 0x6D, 0x28, 0x23, 0xCB, 0x4E], // wrong map
        &[0x62, 0xF3, 0x69, 0x28, 0x23, 0xCB, 0x4E], // missing fixed-one bit
        &[0x62, 0xF3, 0x6C, 0x28, 0x23, 0xCB, 0x4E], // wrong mandatory prefix
        &[0x62, 0xF3, 0x6D, 0x08, 0x23, 0xCB, 0x4E], // nonexistent 128-bit form
        &[0x62, 0xF3, 0x6D, 0x68, 0x23, 0xCB, 0x4E], // reserved L'L=3
        &[0x62, 0xF3, 0x6D, 0x38, 0x23, 0xCB, 0x4E], // EVEX.b on register
        &[0x62, 0xF3, 0x6D, 0xA8, 0x23, 0xCB, 0x4E], // {z} with k0
        &[0x62, 0xF3, 0x6D, 0x28, 0x23, 0x0B, 0x4E], // memory source
        &[0x62, 0xF3, 0x6D, 0x28, 0x24, 0xCB, 0x4E], // unrelated opcode
        &[0x62, 0xF3, 0x6D, 0x28, 0x23, 0xCB],       // missing imm8
        &[0x62, 0xF3, 0x6D, 0x28, 0x23, 0xCB, 0x4E, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_chunk_shuffle_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_only_vl_for_256_bit_forms() {
    let pc = 0x3D00;
    let mut block = SmirBlock::new(BlockId(18), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF3, 0x6D, 0x28, 0x23, 0xCB, 0x4E][..], true),
        (&[0x62, 0x03, 0xAD, 0xC2, 0x43, 0xCB, 0xB1], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(18), pc), instruction)]);
        for spans in [
            x86_evex_chunk_shuffle_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, needs_vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}
