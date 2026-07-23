//! Exact classifier tests for x86 native source-byte replay.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::OpId;

const SCALAR_FP16_ARITHMETIC_OPCODES: [u8; 7] = [0x51, 0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

#[test]
fn scalar_fp16_arithmetic_replay_classifier_is_exact_and_fail_closed() {
    for opcode in SCALAR_FP16_ARITHMETIC_OPCODES {
        // LLIG admits every L'L value. With EVEX.b clear the host observes
        // MXCSR.RC; with EVEX.b set L'L selects embedded rounding or is ignored
        // by the instruction's SAE form.
        for p2 in [0x09, 0x29, 0x49, 0x69, 0x19, 0x39, 0x59, 0x79] {
            let bytes = [0x62, 0xF5, 0x7E, p2, opcode, 0xC8];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_scalar_fp16_arithmetic_needs_vl(),
                Some(false),
                "{bytes:02X?}"
            );
        }

        // xmm17{k1}{z}, xmm18, xmm19 exercises every EVEX register-extension
        // channel while remaining a register-only scalar instruction.
        let high_registers = [0x62, 0xA5, 0x6E, 0x81, opcode, 0xCB];
        assert_eq!(
            X86InstructionBytes::new(&high_registers)
                .unwrap()
                .evex_register_scalar_fp16_arithmetic_needs_vl(),
            Some(false),
            "{high_registers:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x61, 0xF5, 0x7E, 0x09, 0x58, 0xC8],       // not EVEX
        &[0x62, 0xF6, 0x7E, 0x09, 0x58, 0xC8],       // MAP6, not MAP5
        &[0x62, 0xF5, 0x7A, 0x09, 0x58, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF5, 0x7D, 0x09, 0x58, 0xC8],       // 66, not F3
        &[0x62, 0xF5, 0xFE, 0x09, 0x58, 0xC8],       // W1, not W0
        &[0x62, 0xF5, 0x7E, 0x09, 0x58, 0x08],       // memory source
        &[0x62, 0xF5, 0x7E, 0x88, 0x58, 0xC8],       // {z} with k0
        &[0x62, 0xF5, 0x7E, 0x09, 0x50, 0xC8],       // unrelated opcode
        &[0x62, 0xF5, 0x7E, 0x09, 0x58],             // missing ModR/M
        &[0x62, 0xF5, 0x7E, 0x09, 0x58, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_scalar_fp16_arithmetic_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn scalar_fp16_arithmetic_replay_spans_require_fp16_without_vl_or_dq() {
    let pc = 0x1000;
    let mut block = SmirBlock::new(BlockId(7), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for opcode in SCALAR_FP16_ARITHMETIC_OPCODES {
        let bytes = [0x62, 0xA5, 0x6E, 0xD1, opcode, 0xCB];
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(7), pc), instruction)]);

        for spans in [
            x86_evex_scalar_fp16_arithmetic_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}

fn avx512f_permute_shapes() -> Vec<(u8, u8, bool, u8, bool)> {
    let mut shapes = Vec::new();

    // Variable-control VPERMPS/PD and VPERMD/Q exclude EVEX.128.
    for opcode in [0x16, 0x36] {
        for w in [false, true] {
            for ll in [1, 2] {
                shapes.push((2, opcode, w, ll, false));
            }
        }
    }
    // VPERMI2D/Q/PS/PD and VPERMT2D/Q/PS/PD.
    for opcode in [0x76, 0x77, 0x7E, 0x7F] {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((2, opcode, w, ll, false));
            }
        }
    }
    // Variable and immediate VPERMILPS/PD.
    for (map, opcode, w, immediate) in [
        (2, 0x0C, false, false),
        (2, 0x0D, true, false),
        (3, 0x04, false, true),
        (3, 0x05, true, true),
    ] {
        for ll in 0..=2 {
            shapes.push((map, opcode, w, ll, immediate));
        }
    }
    // Immediate-control VPERMQ/PD exclude EVEX.128.
    for opcode in [0x00, 0x01] {
        for ll in [1, 2] {
            shapes.push((3, opcode, true, ll, true));
        }
    }
    shapes
}

fn generated_avx512f_permute_encoding(
    map: u8,
    opcode: u8,
    w: bool,
    ll: u8,
    immediate: bool,
    rm: u8,
) -> Vec<u8> {
    let mut p0 = 0xF0 | map;
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    let p1 = 0x7D | if w { 0x80 } else { 0 };
    let p2 = (ll << 5) | 0x09;
    let mut bytes = vec![0x62, p0, p1, p2, opcode, 0xC8 | (rm & 0x07)];
    if immediate {
        bytes.push(3);
    }
    bytes
}

#[test]
fn avx512f_permute_replay_classifier_covers_192_generated_register_forms() {
    let shapes = avx512f_permute_shapes();
    assert_eq!(shapes.len(), 48);

    let mut register_forms = 0usize;
    for (map, opcode, w, ll, immediate) in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = generated_avx512f_permute_encoding(map, opcode, w, ll, immediate, rm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_avx512f_permute_needs_vl(),
                Some(ll != 2),
                "{bytes:02X?}"
            );
            register_forms += 1;
        }

        let mut memory = generated_avx512f_permute_encoding(map, opcode, w, ll, immediate, 0);
        memory[5] = 0x08;
        assert_eq!(
            X86InstructionBytes::new(&memory)
                .unwrap()
                .evex_register_avx512f_permute_needs_vl(),
            None,
            "{memory:02X?}"
        );
    }
    assert_eq!(register_forms, 192);

    // Independent LLVM encodings exercise destination, vvvv/V', and r/m
    // extension channels for variable and reserved-vvvv immediate forms.
    for bytes in [
        &[0x62, 0xA2, 0x6D, 0xC1, 0x36, 0xCB][..],
        &[0x62, 0xA2, 0xED, 0xC1, 0x76, 0xCB],
        &[0x62, 0xA2, 0xED, 0xC1, 0x7F, 0xCB],
        &[0x62, 0xA2, 0x6D, 0xC1, 0x0C, 0xCB],
        &[0x62, 0xA3, 0xFD, 0xC9, 0x05, 0xCB, 0x03],
        &[0x62, 0xA3, 0xFD, 0xC9, 0x00, 0xCB, 0x03],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_avx512f_permute_needs_vl(),
            Some(false),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn avx512f_permute_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0x6D, 0x49, 0x36, 0xC8],       // not EVEX
        &[0x62, 0xF1, 0x6D, 0x49, 0x36, 0xC8],       // map 1
        &[0x62, 0xF2, 0x69, 0x49, 0x36, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF2, 0x6C, 0x49, 0x36, 0xC8],       // no mandatory 66
        &[0x62, 0xF2, 0x6D, 0x09, 0x36, 0xC8],       // reserved VPERMD.128
        &[0x62, 0xF2, 0x6D, 0x69, 0x76, 0xC8],       // reserved L'L=3
        &[0x62, 0xF2, 0x6D, 0x59, 0x76, 0xC8],       // EVEX.b with register
        &[0x62, 0xF2, 0x6D, 0xC8, 0x76, 0xC8],       // {z} with k0
        &[0x62, 0xF2, 0x6D, 0x49, 0x76, 0x08],       // memory source
        &[0x62, 0xF2, 0xED, 0x49, 0x0C, 0xC8],       // VPERMILPS with W1
        &[0x62, 0xF2, 0x7D, 0x49, 0x0D, 0xC8],       // VPERMILPD with W0
        &[0x62, 0xF3, 0x7D, 0x49, 0x00, 0xC8, 0x03], // VPERMQ with W0
        &[0x62, 0xF3, 0xFD, 0x09, 0x00, 0xC8, 0x03], // reserved VPERMQ.128
        &[0x62, 0xF3, 0xED, 0x49, 0x00, 0xC8, 0x03], // nonreserved vvvv
        &[0x62, 0xF3, 0xFD, 0x41, 0x00, 0xC8, 0x03], // nonreserved V'
        &[0x62, 0xF3, 0xFD, 0x49, 0x04, 0xC8, 0x03], // VPERMILPS with W1
        &[0x62, 0xF3, 0x7D, 0x49, 0x05, 0xC8, 0x03], // VPERMILPD with W0
        &[0x62, 0xF2, 0x6D, 0x49, 0x10, 0xC8],       // unrelated opcode
        &[0x62, 0xF2, 0x6D, 0x49, 0x36],             // missing ModR/M
        &[0x62, 0xF2, 0x6D, 0x49, 0x36, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_avx512f_permute_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn avx512f_permute_replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x2000;
    let mut block = SmirBlock::new(BlockId(9), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF2, 0x7D, 0x09, 0x76, 0xC8][..], true),
        (&[0x62, 0xF2, 0xFD, 0x29, 0x0D, 0xC8], true),
        (&[0x62, 0xF2, 0x7D, 0x49, 0x36, 0xC8], false),
        (&[0x62, 0xF3, 0xFD, 0x49, 0x00, 0xC8, 0x03], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(9), pc), instruction)]);
        for spans in [
            x86_evex_avx512f_permute_replay_spans(&block, &provenance),
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
fn packed_extend_replay_classifier_covers_264_register_encodings() {
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
fn packed_extend_replay_classifier_rejects_every_reserved_frontier() {
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
fn packed_extend_replay_spans_require_only_vl_for_sub_512_widths() {
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
