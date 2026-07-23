//! Exact source-byte replay classification for EVEX VALIGND/Q.

use super::*;

type VectorAlignShape = (bool, u8);

fn encoding(
    shape: VectorAlignShape,
    destination: u8,
    source1: u8,
    source2: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> [u8; 7] {
    let (w, ll) = shape;
    assert!(destination < 32 && source1 < 32 && source2 < 32 && ll < 3 && mask < 8);
    assert!(!zeroing || mask != 0);
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
        0x03,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
        immediate,
    ]
}

#[test]
fn classifier_covers_27648_legal_register_encodings() {
    let mut classified = 0usize;
    for w in [false, true] {
        for ll in 0u8..=2 {
            for destination in [1u8, 9, 17, 25] {
                for source1 in 0u8..32 {
                    for source2 in [2u8, 10, 18, 26] {
                        for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                            for immediate in [0u8, 1, 0xFF] {
                                let bytes = encoding(
                                    (w, ll),
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
                                        .evex_register_vector_align_needs_vl(),
                                    Some(ll != 2),
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
    assert_eq!(classified, 27_648);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF3, 0x6D, 0x08, 0x03, 0xCB, 0x01], // not EVEX
        &[0x62, 0xF2, 0x6D, 0x08, 0x03, 0xCB, 0x01], // wrong map
        &[0x62, 0xF3, 0x69, 0x08, 0x03, 0xCB, 0x01], // missing fixed-one bit
        &[0x62, 0xF3, 0x6C, 0x08, 0x03, 0xCB, 0x01], // wrong mandatory prefix
        &[0x62, 0xF3, 0x6D, 0x18, 0x03, 0xCB, 0x01], // EVEX.b on register
        &[0x62, 0xF3, 0x6D, 0x68, 0x03, 0xCB, 0x01], // reserved L'L=3
        &[0x62, 0xF3, 0x6D, 0x88, 0x03, 0xCB, 0x01], // {z} with k0
        &[0x62, 0xF3, 0x6D, 0x08, 0x03, 0x0B, 0x01], // memory source
        &[0x62, 0xF3, 0x6D, 0x08, 0x04, 0xCB, 0x01], // unrelated opcode
        &[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB],       // missing imm8
        &[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB, 0x01, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_vector_align_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x3C00;
    let mut block = SmirBlock::new(BlockId(17), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB, 0xFF][..], true),
        (&[0x62, 0x03, 0x8D, 0x21, 0x03, 0xFD, 0x07], true),
        (&[0x62, 0xA3, 0xD5, 0xC2, 0x03, 0xE6, 0x0F], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(17), pc), instruction)]);
        for spans in [
            x86_evex_vector_align_replay_spans(&block, &provenance),
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
