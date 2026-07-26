//! Exact classifiers for VEX/EVEX packed sign/zero-extension replay.

use super::*;

fn vex_encoding(p0: u8, p1: u8, opcode: u8, modrm: u8) -> [u8; 5] {
    [0xC4, p0, p1, opcode, modrm]
}

#[test]
fn vex_classifier_covers_all_24576_legal_register_encodings_and_destinations() {
    let mut classified = 0usize;
    for p0 in u8::MIN..=u8::MAX {
        if p0 & 0x1F != 2 {
            continue;
        }
        for p1 in u8::MIN..=u8::MAX {
            if p1 & 0x78 != 0x78 || p1 & 0x03 != 1 {
                continue;
            }
            for opcode in (0x20..=0x25).chain(0x30..=0x35) {
                for modrm in 0xC0..=0xFF {
                    let bytes = vex_encoding(p0, p1, opcode, modrm);
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    assert_eq!(
                        instruction.vex_register_packed_extend_needs_avx2(),
                        Some(p1 & 0x04 != 0),
                        "{bytes:02X?}"
                    );
                    assert_eq!(
                        instruction.vex_packed_extend_destination_index(),
                        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 }),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 24_576);
}

#[test]
fn vex_classifier_exhausts_prefix_opcode_and_modrm_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(p0, 0x79, 0x20, 0xCA);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            (p0 & 0x1F == 2).then_some(false),
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(0xE2, p1, 0x20, 0xCA);
        let expected = (p1 & 0x78 == 0x78 && p1 & 0x03 == 1).then_some(p1 & 0x04 != 0);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            expected,
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(0xE2, 0x79, opcode, 0xCA);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            matches!(opcode, 0x20..=0x25 | 0x30..=0x35).then_some(false),
            "{bytes:02X?}"
        );
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(0xE2, 0xFD, 0x35, modrm);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            (modrm >> 6 == 3).then_some(true),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vex_classifier_accepts_llvm_samples_wig_and_ignored_x_and_rejects_neighbors() {
    // LLVM 23.0.0 independently assembled the first four samples. It also
    // independently decoded the W=1 and X'=0 mutations to the same canonical
    // mnemonics and operands as their W=0/X'=1 counterparts.
    for (bytes, needs_avx2, destination) in [
        (&[0xC4, 0xE2, 0x79, 0x20, 0xCA][..], false, 1),
        (&[0xC4, 0x42, 0x7D, 0x20, 0xCA], true, 9),
        (&[0xC4, 0xE2, 0x79, 0x35, 0xCA], false, 1),
        (&[0xC4, 0x42, 0x7D, 0x35, 0xCA], true, 9),
        (&[0xC4, 0xE2, 0xF9, 0x20, 0xCA], false, 1),
        (&[0xC4, 0xA2, 0x79, 0x20, 0xCA], false, 1),
        (&[0xC4, 0x42, 0xFD, 0x35, 0xCA], true, 9),
        (&[0xC4, 0x02, 0x7D, 0x35, 0xCA], true, 9),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_packed_extend_needs_avx2(),
            Some(needs_avx2),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_packed_extend_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0xC5, 0xF9, 0x20, 0xCA],       // two-byte VEX cannot select map 0F38
        &[0xC4, 0xE1, 0x79, 0x20, 0xCA], // map 0F
        &[0xC4, 0xE3, 0x79, 0x20, 0xCA], // map 0F3A
        &[0xC4, 0xE2, 0x78, 0x20, 0xCA], // missing mandatory 66
        &[0xC4, 0xE2, 0x69, 0x20, 0xCA], // nonreserved VEX.vvvv
        &[0xC4, 0xE2, 0x79, 0x1F, 0xCA], // unrelated opcode
        &[0xC4, 0xE2, 0x79, 0x26, 0xCA], // unrelated opcode
        &[0xC4, 0xE2, 0x79, 0x20, 0x0A], // memory source
        &[0xC4, 0xE2, 0x79, 0x20],       // missing ModR/M
        &[0xC4, 0xE2, 0x79, 0x20, 0xCA, 0x00], // trailing byte
        &[0x62, 0xF2, 0x7D, 0x08, 0x20, 0xCA], // EVEX, not VEX
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_packed_extend_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_packed_extend_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vex_packed_extend_replay_spans_require_no_avx512_features() {
    let pc = 0x30F0;
    let mut block = SmirBlock::new(BlockId(12), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        &[0xC4, 0xE2, 0x79, 0x20, 0xCA][..],
        &[0xC4, 0x42, 0xFD, 0x35, 0xCA],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(12), pc), instruction)]);
        for spans in [
            x86_vex_packed_extend_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}

type PackedExtendShape = (u8, bool, u8);

fn packed_extend_shapes() -> Vec<PackedExtendShape> {
    let mut shapes = Vec::new();
    for opcode in (0x20..=0x25).chain(0x30..=0x35) {
        let widths: &[bool] = if matches!(opcode, 0x25 | 0x35) {
            &[false]
        } else {
            &[false, true]
        };
        for &w in widths {
            for ll in 0..=2 {
                shapes.push((opcode, w, ll));
            }
        }
    }
    shapes
}

fn generated_packed_extend_encoding(shape: PackedExtendShape, rm: u8) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    let mut p0 = 0xF2;
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x09,
        opcode,
        0xC8 | (rm & 0x07),
    ]
}

#[test]
fn evex_classifier_covers_264_register_encodings() {
    let shapes = packed_extend_shapes();
    assert_eq!(shapes.len(), 66);

    let mut register_encodings = 0usize;
    for shape in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = generated_packed_extend_encoding(shape, rm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_packed_extend_needs_vl(),
                Some(shape.2 != 2),
                "{bytes:02X?}"
            );
            register_encodings += 1;
        }

        let mut memory = generated_packed_extend_encoding(shape, 0);
        memory[5] = 0x08;
        assert_eq!(
            X86InstructionBytes::new(&memory)
                .unwrap()
                .evex_register_packed_extend_needs_vl(),
            None,
            "{memory:02X?}"
        );
    }
    assert_eq!(register_encodings, 264);

    // Independent LLVM encodings cover all twelve mnemonics and every EVEX
    // destination/source register-extension channel.
    for bytes in [
        &[0x62, 0x02, 0x7D, 0xC9, 0x20, 0xCA][..],
        &[0x62, 0x02, 0x7D, 0xC9, 0x21, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x22, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x23, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x24, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x25, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x30, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x31, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x32, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x33, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x34, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x35, 0xCA],
        // Intel WIG forms with W1, independently decoded by LLVM.
        &[0x62, 0xF2, 0xFD, 0x49, 0x20, 0xC8],
        &[0x62, 0xF2, 0xFD, 0x49, 0x34, 0xC8],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_extend_needs_vl(),
            Some(false),
            "{bytes:02X?}"
        );
    }

    let unmasked = [0x62, 0xF2, 0x7D, 0x48, 0x20, 0xC8];
    assert_eq!(
        X86InstructionBytes::new(&unmasked)
            .unwrap()
            .evex_register_packed_extend_needs_vl(),
        Some(false)
    );
}

#[test]
fn evex_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0x7D, 0x09, 0x20, 0xC8],       // not EVEX
        &[0x62, 0xF1, 0x7D, 0x09, 0x20, 0xC8],       // map 1
        &[0x62, 0xF2, 0x79, 0x09, 0x20, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF2, 0x7C, 0x09, 0x20, 0xC8],       // missing mandatory 66
        &[0x62, 0xF2, 0x6D, 0x09, 0x20, 0xC8],       // nonreserved vvvv
        &[0x62, 0xF2, 0x7D, 0x01, 0x20, 0xC8],       // nonreserved V'
        &[0x62, 0xF2, 0xFD, 0x09, 0x25, 0xC8],       // VPMOVSXDQ with W1
        &[0x62, 0xF2, 0xFD, 0x09, 0x35, 0xC8],       // VPMOVZXDQ with W1
        &[0x62, 0xF2, 0x7D, 0x19, 0x20, 0xC8],       // EVEX.b
        &[0x62, 0xF2, 0x7D, 0x69, 0x20, 0xC8],       // reserved L'L=3
        &[0x62, 0xF2, 0x7D, 0x88, 0x20, 0xC8],       // {z} with k0
        &[0x62, 0xF2, 0x7D, 0x09, 0x20, 0x08],       // memory operand
        &[0x62, 0xF2, 0x7D, 0x09, 0x26, 0xC8],       // unrelated opcode
        &[0x62, 0xF2, 0x7D, 0x09, 0x20],             // missing ModR/M
        &[0x62, 0xF2, 0x7D, 0x09, 0x20, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_extend_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn evex_replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x3100;
    let mut block = SmirBlock::new(BlockId(12), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF2, 0x7D, 0x09, 0x20, 0xC8][..], true),
        (&[0x62, 0xF2, 0xFD, 0x29, 0x34, 0xC8], true),
        (&[0x62, 0xF2, 0x7D, 0x49, 0x35, 0xC8], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(12), pc), instruction)]);
        for spans in [
            x86_evex_packed_extend_replay_spans(&block, &provenance),
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
