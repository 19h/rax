//! EVEX register-only packed-move replay classification.

use super::*;

type PackedMoveShape = (u8, u8, bool, u8);

fn packed_move_shapes() -> Vec<PackedMoveShape> {
    let mut shapes = Vec::new();

    // VMOVUPS/UPD and VMOVAPS/APD, load and store opcode directions.
    for opcode in [0x10, 0x11, 0x28, 0x29] {
        for (pp, w) in [(0, false), (1, true)] {
            for ll in 0..=2 {
                shapes.push((opcode, pp, w, ll));
            }
        }
    }
    // VMOVDQA32/64 and VMOVDQU8/16/32/64, both directions.
    for opcode in [0x6F, 0x7F] {
        for pp in 1..=3 {
            for w in [false, true] {
                for ll in 0..=2 {
                    shapes.push((opcode, pp, w, ll));
                }
            }
        }
    }
    shapes
}

fn generated_packed_move_encoding(shape: PackedMoveShape, rm: u8) -> [u8; 6] {
    let (opcode, pp, w, ll) = shape;
    let mut p0 = 0xF1;
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x09,
        opcode,
        0xC8 | (rm & 0x07),
    ]
}

#[test]
fn packed_move_replay_classifier_covers_240_generated_register_forms() {
    let shapes = packed_move_shapes();
    assert_eq!(shapes.len(), 60);

    let mut register_forms = 0usize;
    for shape in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = generated_packed_move_encoding(shape, rm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_packed_move_needs_vl(),
                Some(shape.3 != 2),
                "{bytes:02X?}"
            );
            register_forms += 1;
        }

        let mut memory = generated_packed_move_encoding(shape, 0);
        memory[5] = 0x08;
        assert_eq!(
            X86InstructionBytes::new(&memory)
                .unwrap()
                .evex_register_packed_move_needs_vl(),
            None,
            "{memory:02X?}"
        );
    }
    assert_eq!(register_forms, 240);

    // Independent LLVM encodings cover all ten mnemonics with high vector
    // registers. Store-direction bytes are independently disassembled forms.
    for bytes in [
        &[0x62, 0xA1, 0x7C, 0xC9, 0x10, 0xCA][..],
        &[0x62, 0xA1, 0xFD, 0xC9, 0x10, 0xCA],
        &[0x62, 0xA1, 0x7C, 0xC9, 0x28, 0xCA],
        &[0x62, 0xA1, 0xFD, 0xC9, 0x28, 0xCA],
        &[0x62, 0xA1, 0x7D, 0xC9, 0x6F, 0xCA],
        &[0x62, 0xA1, 0xFD, 0xC9, 0x6F, 0xCA],
        &[0x62, 0xA1, 0x7F, 0xC9, 0x6F, 0xCA],
        &[0x62, 0xA1, 0xFF, 0xC9, 0x6F, 0xCA],
        &[0x62, 0xA1, 0x7E, 0xC9, 0x6F, 0xCA],
        &[0x62, 0xA1, 0xFE, 0xC9, 0x6F, 0xCA],
        &[0x62, 0xA1, 0x7C, 0xC9, 0x11, 0xCA],
        &[0x62, 0xA1, 0xFD, 0xC9, 0x29, 0xCA],
        &[0x62, 0xA1, 0x7D, 0xC9, 0x7F, 0xCA],
        &[0x62, 0xA1, 0xFF, 0xC9, 0x7F, 0xCA],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_move_needs_vl(),
            Some(false),
            "{bytes:02X?}"
        );
    }

    let unmasked = [0x62, 0xF1, 0x7C, 0x48, 0x10, 0xC8];
    assert_eq!(
        X86InstructionBytes::new(&unmasked)
            .unwrap()
            .evex_register_packed_move_needs_vl(),
        Some(false),
        "{unmasked:02X?}"
    );
}

#[test]
fn packed_move_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0x7C, 0x09, 0x10, 0xC8],       // not EVEX
        &[0x62, 0xF2, 0x7C, 0x09, 0x10, 0xC8],       // map 2
        &[0x62, 0xF1, 0x78, 0x09, 0x10, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF1, 0x6C, 0x09, 0x10, 0xC8],       // nonreserved vvvv
        &[0x62, 0xF1, 0x7C, 0x01, 0x10, 0xC8],       // nonreserved V'
        &[0x62, 0xF1, 0xFC, 0x09, 0x10, 0xC8],       // VMOVUPS with W1
        &[0x62, 0xF1, 0x7D, 0x09, 0x10, 0xC8],       // VMOVUPD with W0
        &[0x62, 0xF1, 0x7C, 0x09, 0x6F, 0xC8],       // integer move without prefix
        &[0x62, 0xF1, 0x7C, 0x19, 0x10, 0xC8],       // EVEX.b
        &[0x62, 0xF1, 0x7C, 0x69, 0x10, 0xC8],       // reserved L'L=3
        &[0x62, 0xF1, 0x7C, 0x88, 0x10, 0xC8],       // {z} with k0
        &[0x62, 0xF1, 0x7C, 0x09, 0x10, 0x08],       // memory operand
        &[0x62, 0xF1, 0x7C, 0x09, 0x12, 0xC8],       // unrelated opcode
        &[0x62, 0xF1, 0x7C, 0x09, 0x10],             // missing ModR/M
        &[0x62, 0xF1, 0x7C, 0x09, 0x10, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_move_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn packed_move_replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x3000;
    let mut block = SmirBlock::new(BlockId(11), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF1, 0x7C, 0x09, 0x10, 0xC8][..], true),
        (&[0x62, 0xF1, 0xFF, 0x29, 0x7F, 0xC8], true),
        (&[0x62, 0xF1, 0xFD, 0x49, 0x28, 0xC8], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(11), pc), instruction)]);
        for spans in [
            x86_evex_packed_move_replay_spans(&block, &provenance),
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
