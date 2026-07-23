//! Exact source-byte replay classification for EVEX GPR-source broadcasts.

use super::*;

type GprBroadcastShape = (u8, bool, u8);

fn shapes() -> Vec<GprBroadcastShape> {
    let mut shapes = Vec::new();
    for (opcode, w) in [(0x7A, false), (0x7B, false), (0x7C, false), (0x7C, true)] {
        for ll in 0..=2 {
            shapes.push((opcode, w, ll));
        }
    }
    shapes
}

fn encoding(
    shape: GprBroadcastShape,
    destination: u8,
    source: u8,
    ignored_x: bool,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    assert!(destination < 32 && source < 16 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

#[test]
fn classifier_covers_4032_safe_register_encodings() {
    let shapes = shapes();
    assert_eq!(shapes.len(), 12);
    let safe_sources = [0u8, 1, 2, 3, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let mut classified = 0usize;

    for shape in shapes {
        for destination in [1u8, 9, 17, 25] {
            for source in safe_sources {
                for ignored_x in [false, true] {
                    for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                        let bytes = encoding(shape, destination, source, ignored_x, mask, zeroing);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_gpr_broadcast_needs_vl(),
                            Some(shape.2 != 2),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 4_032);

    // R12/R13 share low ModR/M codes with RSP/RBP but are safe because EVEX.B
    // selects the high GPR bank. EVEX.X remains ignored in both encodings.
    for source in [12u8, 13] {
        for ignored_x in [false, true] {
            let bytes = encoding((0x7C, true, 2), 25, source, ignored_x, 0, false);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_gpr_broadcast_needs_vl(),
                Some(false),
                "{bytes:02X?}"
            );
        }
    }
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0x7D, 0x08, 0x7A, 0xC8],       // not EVEX
        &[0x62, 0xF1, 0x7D, 0x08, 0x7A, 0xC8],       // wrong map
        &[0x62, 0xF2, 0x79, 0x08, 0x7A, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF2, 0x7C, 0x08, 0x7A, 0xC8],       // wrong mandatory prefix
        &[0x62, 0xF2, 0x75, 0x08, 0x7A, 0xC8],       // EVEX.vvvv != 1111b
        &[0x62, 0xF2, 0x7D, 0x00, 0x7A, 0xC8],       // EVEX.V' is reserved
        &[0x62, 0xF2, 0x7D, 0x18, 0x7A, 0xC8],       // EVEX.b is reserved
        &[0x62, 0xF2, 0x7D, 0x68, 0x7A, 0xC8],       // L'L=3 is reserved
        &[0x62, 0xF2, 0x7D, 0x88, 0x7A, 0xC8],       // {z} with k0
        &[0x62, 0xF2, 0x7D, 0x08, 0x7A, 0x08],       // memory source
        &[0x62, 0xF2, 0xFD, 0x08, 0x7A, 0xC8],       // VPBROADCASTB requires W0
        &[0x62, 0xF2, 0xFD, 0x08, 0x7B, 0xC8],       // VPBROADCASTW requires W0
        &[0x62, 0xF2, 0x7D, 0x08, 0x79, 0xC8],       // unrelated opcode
        &[0x62, 0xF2, 0x7D, 0x08, 0x7A],             // missing ModR/M
        &[0x62, 0xF2, 0x7D, 0x08, 0x7A, 0xC8, 0x00], // trailing byte
        &[0x62, 0xF2, 0x7D, 0x08, 0x7A, 0xCC],       // guest RSP is host stack
        &[0x62, 0xF2, 0x7D, 0x08, 0x7A, 0xCD],       // guest RBP is host frame
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_gpr_broadcast_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x3B00;
    let mut block = SmirBlock::new(BlockId(16), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF2, 0x7D, 0x08, 0x7A, 0xC8][..], true),
        (&[0x62, 0x92, 0x7D, 0x29, 0x7B, 0xCC], true),
        (&[0x62, 0x12, 0xFD, 0xCA, 0x7C, 0xCD], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(16), pc), instruction)]);
        for spans in [
            x86_evex_gpr_broadcast_replay_spans(&block, &provenance),
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
