//! Exact classifier tests for x86 native source-byte replay.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{Address, OpId, VReg, VecWidth};

#[path = "x86_native_replay_tests/bw_immediate.rs"]
mod bw_immediate;

#[path = "x86_native_replay_tests/bw_shuffle_madd.rs"]
mod bw_shuffle_madd;

#[path = "x86_native_replay_tests/chunk_extract.rs"]
mod chunk_extract;

#[path = "x86_native_replay_tests/chunk_insert.rs"]
mod chunk_insert;

#[path = "x86_native_replay_tests/chunk_shuffle.rs"]
mod chunk_shuffle;

#[path = "x86_native_replay_tests/fp_class.rs"]
mod fp_class;

#[path = "x86_native_replay_tests/fp_arithmetic.rs"]
mod fp_arithmetic;

#[path = "x86_native_replay_tests/fp_compare.rs"]
mod fp_compare;

#[path = "x86_native_replay_tests/fp_estimate.rs"]
mod fp_estimate;

#[path = "x86_native_replay_tests/fp_round.rs"]
mod fp_round;

#[path = "x86_native_replay_tests/fp_shuffle.rs"]
mod fp_shuffle;

#[path = "x86_native_replay_tests/fp16_flag_compare.rs"]
mod fp16_flag_compare;

#[path = "x86_native_replay_tests/fp32_fp64_flag_compare.rs"]
mod fp32_fp64_flag_compare;

#[path = "x86_native_replay_tests/fp32_fp64_convert.rs"]
mod fp32_fp64_convert;

#[path = "x86_native_replay_tests/fp16_narrow.rs"]
mod fp16_narrow;

#[path = "x86_native_replay_tests/fp16_widen.rs"]
mod fp16_widen;

#[path = "x86_native_replay_tests/fp_sqrt.rs"]
mod fp_sqrt;

#[path = "x86_native_replay_tests/gfni.rs"]
mod gfni;

#[path = "x86_native_replay_tests/gpr_broadcast.rs"]
mod gpr_broadcast;

#[path = "x86_native_replay_tests/high_low_move.rs"]
mod high_low_move;

#[path = "x86_native_replay_tests/legacy_vex_fp_compare.rs"]
mod legacy_vex_fp_compare;

#[path = "x86_native_replay_tests/legacy_vex_fp_horizontal_addsub.rs"]
mod legacy_vex_fp_horizontal_addsub;

#[path = "x86_native_replay_tests/legacy_vex_high_low_move.rs"]
mod legacy_vex_high_low_move;

#[path = "x86_native_replay_tests/legacy_vex_scalar_move.rs"]
mod legacy_vex_scalar_move;

#[path = "x86_native_replay_tests/legacy_aes.rs"]
mod legacy_aes;

#[path = "x86_native_replay_tests/legacy_packed_fp_convert.rs"]
mod legacy_packed_fp_convert;

#[path = "x86_native_replay_tests/legacy_dot_product.rs"]
mod legacy_dot_product;

#[path = "x86_native_replay_tests/legacy_insertps.rs"]
mod legacy_insertps;

#[path = "x86_native_replay_tests/legacy_pclmulqdq.rs"]
mod legacy_pclmulqdq;

#[path = "x86_native_replay_tests/legacy_ptest.rs"]
mod legacy_ptest;

#[path = "x86_native_replay_tests/legacy_fp_round.rs"]
mod legacy_fp_round;

#[path = "x86_native_replay_tests/legacy_scalar_fp_convert.rs"]
mod legacy_scalar_fp_convert;

#[path = "x86_native_replay_tests/legacy_widening_dword_multiply.rs"]
mod legacy_widening_dword_multiply;

#[path = "x86_native_replay_tests/legacy_sha.rs"]
mod legacy_sha;

#[path = "x86_native_replay_tests/packed_extend.rs"]
mod packed_extend;

#[path = "x86_native_replay_tests/evex_packed_move.rs"]
mod evex_packed_move;

#[path = "x86_native_replay_tests/vex_memory_broadcast.rs"]
mod vex_memory_broadcast;

#[path = "x86_native_replay_tests/scalar_fp_convert.rs"]
mod scalar_fp_convert;

#[path = "x86_native_replay_tests/scalar_fp_to_int.rs"]
mod scalar_fp_to_int;

#[path = "x86_native_replay_tests/scalar_int_to_fp.rs"]
mod scalar_int_to_fp;

#[path = "x86_native_replay_tests/scalar_integer_move.rs"]
mod scalar_integer_move;

#[path = "x86_native_replay_tests/scalar_lane_transfer.rs"]
mod scalar_lane_transfer;

#[path = "x86_native_replay_tests/scalar_move.rs"]
mod scalar_move;

#[path = "x86_native_replay_tests/vex_packed_string.rs"]
mod vex_packed_string;

#[path = "x86_native_replay_tests/vex_aligned_packed_fp_move.rs"]
mod vex_aligned_packed_fp_move;

#[path = "x86_native_replay_tests/vex_unaligned_packed_fp_move.rs"]
mod vex_unaligned_packed_fp_move;

#[path = "x86_native_replay_tests/vex_packed_integer_move.rs"]
mod vex_packed_integer_move;

#[path = "x86_native_replay_tests/vex_register_broadcast.rs"]
mod vex_register_broadcast;

#[path = "x86_native_replay_tests/vex_lane_shuffle.rs"]
mod vex_lane_shuffle;

#[path = "x86_native_replay_tests/vex_widening_dword_multiply.rs"]
mod vex_widening_dword_multiply;

#[path = "x86_native_replay_tests/vex_zero.rs"]
mod vex_zero;

#[path = "x86_native_replay_tests/vex_fma3.rs"]
mod vex_fma3;

#[path = "x86_native_replay_tests/vex_fma4.rs"]
mod vex_fma4;

#[path = "x86_native_replay_tests/vex_immediate_blend.rs"]
mod vex_immediate_blend;

#[path = "x86_native_replay_tests/vex_immediate_permute.rs"]
mod vex_immediate_permute;

#[path = "x86_native_replay_tests/vex_chunk_extract.rs"]
mod vex_chunk_extract;

#[path = "x86_native_replay_tests/vex_scalar_extract.rs"]
mod vex_scalar_extract;

#[path = "x86_native_replay_tests/vex_scalar_vmovq.rs"]
mod vex_scalar_vmovq;

#[path = "x86_native_replay_tests/vex_scalar_l1.rs"]
mod vex_scalar_l1;

#[path = "x86_native_replay_tests/vex_mov_mask.rs"]
mod vex_mov_mask;

#[path = "x86_native_replay_tests/vex_ptest.rs"]
mod vex_ptest;

#[path = "x86_native_replay_tests/vex_variable_blend.rs"]
mod vex_variable_blend;

#[path = "x86_native_replay_tests/vex_vpermil2.rs"]
mod vex_vpermil2;

#[path = "x86_native_replay_tests/vex_fp_logic.rs"]
mod vex_fp_logic;

#[path = "x86_native_replay_tests/vp2intersect.rs"]
mod vp2intersect;

#[path = "x86_native_replay_tests/vpclmulqdq.rs"]
mod vpclmulqdq;

#[path = "x86_native_replay_tests/vector_align.rs"]
mod vector_align;

const SCALAR_FP16_ARITHMETIC_OPCODES: [u8; 7] = [0x51, 0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

#[test]
fn scalar_fp16_arithmetic_replay_classifier_is_exact_and_fail_closed() {
    for opcode in SCALAR_FP16_ARITHMETIC_OPCODES {
        // LLIG admits the three defined vector-length encodings. Register-
        // source EVEX.b supplies ER or SAE and makes all four L'L bit images
        // defined, including the SAE-only minimum/maximum forms.
        for p2 in [0x09, 0x29, 0x49, 0x69, 0x19, 0x39, 0x59, 0x79] {
            let bytes = [0x62, 0xF5, 0x7E, p2, opcode, 0xC8];
            let ll = (p2 >> 5) & 3;
            let embedded_control = p2 & 0x10 != 0;
            let expected = (ll != 3 || embedded_control).then_some(false);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_scalar_fp16_arithmetic_needs_vl(),
                expected,
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

type FixedPackedCompareShape = (u8, u8, bool, u8);

fn fixed_packed_compare_shapes() -> Vec<FixedPackedCompareShape> {
    let mut shapes = Vec::new();
    for opcode in [0x64, 0x65, 0x74, 0x75] {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((1, opcode, w, ll));
            }
        }
    }
    for (map, opcode, w) in [
        (1, 0x66, false),
        (1, 0x76, false),
        (2, 0x29, true),
        (2, 0x37, true),
    ] {
        for ll in 0..=2 {
            shapes.push((map, opcode, w, ll));
        }
    }
    shapes
}

fn generated_fixed_packed_compare_encoding(shape: FixedPackedCompareShape, rm: u8) -> [u8; 6] {
    let (map, opcode, w, ll) = shape;
    let mut p0 = 0xF0 | map;
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x75 | if w { 0x80 } else { 0 },
        (ll << 5) | 0x09,
        opcode,
        0xC8 | (rm & 0x07),
    ]
}

#[test]
fn fixed_packed_compare_replay_classifier_covers_144_register_encodings() {
    let shapes = fixed_packed_compare_shapes();
    assert_eq!(shapes.len(), 36);

    let mut register_encodings = 0usize;
    for shape in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = generated_fixed_packed_compare_encoding(shape, rm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_packed_compare_needs_vl(),
                Some(shape.3 != 2),
                "{bytes:02X?}"
            );
            register_encodings += 1;
        }

        let mut memory = generated_fixed_packed_compare_encoding(shape, 0);
        memory[5] = 0x08;
        assert_eq!(
            X86InstructionBytes::new(&memory)
                .unwrap()
                .evex_register_packed_compare_needs_vl(),
            None,
            "{memory:02X?}"
        );
    }
    assert_eq!(register_encodings, 144);

    // Independently assembled LLVM encodings cover all eight mnemonics and
    // every high vector-source extension channel.
    for bytes in [
        &[0x62, 0x91, 0x35, 0x41, 0x74, 0xEA][..],
        &[0x62, 0x91, 0x35, 0x41, 0x75, 0xEA],
        &[0x62, 0x91, 0x35, 0x41, 0x76, 0xEA],
        &[0x62, 0x92, 0xB5, 0x41, 0x29, 0xEA],
        &[0x62, 0x91, 0x35, 0x41, 0x64, 0xEA],
        &[0x62, 0x91, 0x35, 0x41, 0x65, 0xEA],
        &[0x62, 0x91, 0x35, 0x41, 0x66, 0xEA],
        &[0x62, 0x92, 0xB5, 0x41, 0x37, 0xEA],
        // Intel WIG forms with W1, independently decoded by LLVM.
        &[0x62, 0x91, 0xB5, 0x41, 0x74, 0xEA],
        &[0x62, 0x91, 0xB5, 0x41, 0x75, 0xEA],
        &[0x62, 0x91, 0xB5, 0x41, 0x64, 0xEA],
        &[0x62, 0x91, 0xB5, 0x41, 0x65, 0xEA],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_compare_needs_vl(),
            Some(false),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn fixed_packed_compare_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0x75, 0x09, 0x74, 0xC8],       // not EVEX
        &[0x62, 0xF3, 0x75, 0x09, 0x74, 0xC8],       // map 3
        &[0x62, 0x71, 0x75, 0x09, 0x74, 0xC8],       // extended k destination via R
        &[0x62, 0xE1, 0x75, 0x09, 0x74, 0xC8],       // extended k destination via R'
        &[0x62, 0xF1, 0x71, 0x09, 0x74, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF1, 0x74, 0x09, 0x74, 0xC8],       // missing mandatory 66
        &[0x62, 0xF1, 0xF5, 0x09, 0x76, 0xC8],       // VPCMPEQD with W1
        &[0x62, 0xF2, 0x75, 0x09, 0x29, 0xC8],       // VPCMPEQQ with W0
        &[0x62, 0xF1, 0x75, 0x89, 0x74, 0xC8],       // EVEX.z
        &[0x62, 0xF1, 0x75, 0x19, 0x74, 0xC8],       // EVEX.b
        &[0x62, 0xF1, 0x75, 0x69, 0x74, 0xC8],       // reserved L'L=3
        &[0x62, 0xF1, 0x75, 0x09, 0x74, 0x08],       // memory source
        &[0x62, 0xF1, 0x75, 0x09, 0x73, 0xC8],       // unrelated opcode
        &[0x62, 0xF1, 0x75, 0x09, 0x74],             // missing ModR/M
        &[0x62, 0xF1, 0x75, 0x09, 0x74, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_compare_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn fixed_packed_compare_replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x2F00;
    let mut block = SmirBlock::new(BlockId(11), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF1, 0x75, 0x09, 0x74, 0xC8][..], true),
        (&[0x62, 0xF1, 0xF5, 0x29, 0x65, 0xC8], true),
        (&[0x62, 0xF2, 0xF5, 0x49, 0x37, 0xC8], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(11), pc), instruction)]);
        for spans in [
            x86_evex_packed_compare_replay_spans(&block, &provenance),
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

type MaskBlendShape = (u8, bool, u8);

fn mask_blend_shapes() -> Vec<MaskBlendShape> {
    let mut shapes = Vec::new();
    for opcode in 0x64..=0x66 {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((opcode, w, ll));
            }
        }
    }
    shapes
}

fn generated_mask_blend_encoding(shape: MaskBlendShape, dst: u8, src1: u8, src2: u8) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    let mut p0 = 0xF2;
    if dst & 0x08 != 0 {
        p0 &= !0x10;
    }
    if dst & 0x10 != 0 {
        p0 &= !0x80;
    }
    if src2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if src2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!src1) & 0x0F) << 3) | 0x05 | if w { 0x80 } else { 0 },
        (ll << 5) | if src1 < 16 { 0x08 } else { 0 } | 0x81,
        opcode,
        0xC0 | ((dst & 0x07) << 3) | (src2 & 0x07),
    ]
}

#[test]
fn mask_blend_replay_classifier_covers_72_register_encodings() {
    let shapes = mask_blend_shapes();
    assert_eq!(shapes.len(), 18);

    let mut register_encodings = 0usize;
    for shape in shapes {
        for bucket in [0, 8, 16, 24] {
            let bytes = generated_mask_blend_encoding(shape, 1 + bucket, 2 + bucket, bucket);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_mask_blend_needs_vl(),
                Some(shape.2 != 2),
                "{bytes:02X?}"
            );
            register_encodings += 1;
        }

        let mut memory = generated_mask_blend_encoding(shape, 1, 2, 0);
        memory[5] &= 0x3F;
        assert_eq!(
            X86InstructionBytes::new(&memory)
                .unwrap()
                .evex_register_mask_blend_needs_vl(),
            None,
            "{memory:02X?}"
        );
    }
    assert_eq!(register_encodings, 72);

    for bytes in [
        &[0x62, 0xF2, 0x75, 0x08, 0x64, 0xC8][..], // k0: no control mask
        &[0x62, 0xF2, 0x75, 0x09, 0x64, 0xC8],     // merging k1 selector
        &[0x62, 0xF2, 0x75, 0x89, 0x64, 0xC8],     // zeroing k1 selector
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_mask_blend_needs_vl(),
            Some(true),
            "{bytes:02X?}"
        );
    }

    // Independently assembled LLVM encodings cover all six mnemonics and
    // every vector-register extension channel.
    for bytes in [
        &[0x62, 0x02, 0x2D, 0xC3, 0x65, 0xCB][..],
        &[0x62, 0x02, 0xAD, 0xC3, 0x65, 0xCB],
        &[0x62, 0x02, 0x2D, 0xC3, 0x66, 0xCB],
        &[0x62, 0x02, 0xAD, 0xC3, 0x66, 0xCB],
        &[0x62, 0x02, 0x2D, 0xC3, 0x64, 0xCB],
        &[0x62, 0x02, 0xAD, 0xC3, 0x64, 0xCB],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_mask_blend_needs_vl(),
            Some(false),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn mask_blend_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0x75, 0x09, 0x64, 0xC8],       // not EVEX
        &[0x62, 0xF1, 0x75, 0x09, 0x64, 0xC8],       // wrong map
        &[0x62, 0xF2, 0x71, 0x09, 0x64, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF2, 0x74, 0x09, 0x64, 0xC8],       // missing mandatory 66
        &[0x62, 0xF2, 0x75, 0x09, 0x64, 0x08],       // memory source
        &[0x62, 0xF2, 0x75, 0x19, 0x64, 0xC8],       // EVEX.b
        &[0x62, 0xF2, 0x75, 0x88, 0x64, 0xC8],       // {z} with k0
        &[0x62, 0xF2, 0x75, 0x69, 0x64, 0xC8],       // reserved L'L=3
        &[0x62, 0xF2, 0x75, 0x09, 0x63, 0xC8],       // unrelated opcode
        &[0x62, 0xF2, 0x75, 0x09, 0x64],             // missing ModR/M
        &[0x62, 0xF2, 0x75, 0x09, 0x64, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_mask_blend_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn mask_blend_replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x3000;
    let mut block = SmirBlock::new(BlockId(12), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF2, 0x75, 0x09, 0x64, 0xC8][..], true),
        (&[0x62, 0xF2, 0xF5, 0xA9, 0x65, 0xC8], true),
        (&[0x62, 0x02, 0x2D, 0xC3, 0x66, 0xCB], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(12), pc), instruction)]);
        for spans in [
            x86_evex_mask_blend_replay_spans(&block, &provenance),
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

type VectorToMaskShape = (u8, bool, u8);

fn vector_to_mask_shapes() -> Vec<VectorToMaskShape> {
    let mut shapes = Vec::new();
    for opcode in [0x29, 0x39] {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((opcode, w, ll));
            }
        }
    }
    shapes
}

fn generated_vector_to_mask_encoding(
    shape: VectorToMaskShape,
    destination: u8,
    source: u8,
) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    assert!(destination < 8 && source < 32);
    let mut p0 = 0xF2;
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7E | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08,
        opcode,
        0xC0 | (destination << 3) | (source & 0x07),
    ]
}

#[test]
fn vector_to_mask_replay_classifier_covers_48_register_encodings() {
    let shapes = vector_to_mask_shapes();
    assert_eq!(shapes.len(), 12);

    let mut encodings = 0usize;
    for shape in shapes {
        for (destination, source) in [(1, 0), (2, 8), (3, 16), (4, 24)] {
            let bytes = generated_vector_to_mask_encoding(shape, destination, source);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_vector_to_mask_requirements(),
                Some((shape.2 != 2, shape.0 == 0x39)),
                "{bytes:02X?}"
            );
            encodings += 1;
        }
    }
    assert_eq!(encodings, 48);

    // Independently assembled LLVM encodings cover all four mnemonics and
    // both high source-register extension channels.
    for (bytes, needs_dq) in [
        (&[0x62, 0xB2, 0x7E, 0x48, 0x29, 0xD8][..], false),
        (&[0x62, 0xB2, 0xFE, 0x48, 0x29, 0xD8], false),
        (&[0x62, 0xB2, 0x7E, 0x48, 0x39, 0xD8], true),
        (&[0x62, 0xB2, 0xFE, 0x48, 0x39, 0xD8], true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_vector_to_mask_requirements(),
            Some((false, needs_dq)),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vector_to_mask_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0x7E, 0x08, 0x29, 0xC8],       // not EVEX
        &[0x62, 0xF1, 0x7E, 0x08, 0x29, 0xC8],       // wrong map
        &[0x62, 0x72, 0x7E, 0x08, 0x29, 0xC8],       // extended K destination via R
        &[0x62, 0xE2, 0x7E, 0x08, 0x29, 0xC8],       // extended K destination via R'
        &[0x62, 0xF2, 0x7A, 0x08, 0x29, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF2, 0x7D, 0x08, 0x29, 0xC8],       // wrong mandatory prefix
        &[0x62, 0xF2, 0x76, 0x08, 0x29, 0xC8],       // EVEX.vvvv != 1111b
        &[0x62, 0xF2, 0x7E, 0x00, 0x29, 0xC8],       // EVEX.V' is reserved
        &[0x62, 0xF2, 0x7E, 0x88, 0x29, 0xC8],       // EVEX.z is reserved
        &[0x62, 0xF2, 0x7E, 0x09, 0x29, 0xC8],       // writemask is forbidden
        &[0x62, 0xF2, 0x7E, 0x18, 0x29, 0xC8],       // EVEX.b is reserved
        &[0x62, 0xF2, 0x7E, 0x68, 0x29, 0xC8],       // L'L=3 is reserved
        &[0x62, 0xF2, 0x7E, 0x08, 0x29, 0x08],       // memory source
        &[0x62, 0xF2, 0x7E, 0x08, 0x38, 0xC8],       // unrelated opcode
        &[0x62, 0xF2, 0x7E, 0x08, 0x29],             // missing ModR/M
        &[0x62, 0xF2, 0x7E, 0x08, 0x29, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_vector_to_mask_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vector_to_mask_replay_spans_track_vl_and_dq_requirements() {
    let pc = 0x3800;
    let mut block = SmirBlock::new(BlockId(13), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl, needs_dq) in [
        (&[0x62, 0xF2, 0x7E, 0x08, 0x29, 0xC8][..], true, false),
        (&[0x62, 0xB2, 0xFE, 0x28, 0x39, 0xD8], true, true),
        (&[0x62, 0xB2, 0x7E, 0x48, 0x39, 0xD8], false, true),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(13), pc), instruction)]);
        for spans in [
            x86_evex_vector_to_mask_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, needs_vl, "{bytes:02X?}");
            assert_eq!(span.needs_avx512dq, needs_dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}

type MaskToVectorShape = (u8, bool, u8);

fn mask_to_vector_shapes() -> Vec<MaskToVectorShape> {
    let mut shapes = Vec::new();
    for opcode in [0x28, 0x38] {
        for w in [false, true] {
            for ll in 0..=2 {
                shapes.push((opcode, w, ll));
            }
        }
    }
    shapes
}

fn generated_mask_to_vector_encoding(
    shape: MaskToVectorShape,
    destination: u8,
    source: u8,
    ignored_x: bool,
    ignored_b: bool,
) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    assert!(destination < 32 && source < 8);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if ignored_b {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        0x7E | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | source,
    ]
}

#[test]
fn mask_to_vector_replay_classifier_covers_192_register_encodings() {
    let shapes = mask_to_vector_shapes();
    assert_eq!(shapes.len(), 12);

    let mut encodings = 0usize;
    for shape in shapes {
        for (destination, source) in [(1, 0), (9, 2), (17, 5), (25, 7)] {
            for ignored_x in [false, true] {
                for ignored_b in [false, true] {
                    let bytes = generated_mask_to_vector_encoding(
                        shape,
                        destination,
                        source,
                        ignored_x,
                        ignored_b,
                    );
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_mask_to_vector_requirements(),
                        Some((shape.2 != 2, shape.0 == 0x38)),
                        "{bytes:02X?}"
                    );
                    encodings += 1;
                }
            }
        }
    }
    assert_eq!(encodings, 192);

    // Independently assembled LLVM 23 encodings cover all four mnemonics and
    // all vector-destination extension channels.
    for (bytes, needs_vl, needs_dq) in [
        (&[0x62, 0xF2, 0x7E, 0x08, 0x28, 0xD1][..], true, false),
        (&[0x62, 0xE2, 0xFE, 0x28, 0x28, 0xE3], true, false),
        (&[0x62, 0xE2, 0x7E, 0x48, 0x38, 0xD2], false, true),
        (&[0x62, 0x62, 0xFE, 0x08, 0x38, 0xFF], true, true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_mask_to_vector_requirements(),
            Some((needs_vl, needs_dq)),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn mask_to_vector_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0x7E, 0x08, 0x28, 0xD1],       // not EVEX
        &[0x62, 0xF1, 0x7E, 0x08, 0x28, 0xD1],       // wrong map
        &[0x62, 0xF2, 0x7A, 0x08, 0x28, 0xD1],       // missing fixed-one bit
        &[0x62, 0xF2, 0x7D, 0x08, 0x28, 0xD1],       // wrong mandatory prefix
        &[0x62, 0xF2, 0x76, 0x08, 0x28, 0xD1],       // EVEX.vvvv != 1111b
        &[0x62, 0xF2, 0x7E, 0x00, 0x28, 0xD1],       // EVEX.V' is reserved
        &[0x62, 0xF2, 0x7E, 0x88, 0x28, 0xD1],       // EVEX.z is reserved
        &[0x62, 0xF2, 0x7E, 0x09, 0x28, 0xD1],       // writemask is forbidden
        &[0x62, 0xF2, 0x7E, 0x18, 0x28, 0xD1],       // EVEX.b is reserved
        &[0x62, 0xF2, 0x7E, 0x68, 0x28, 0xD1],       // L'L=3 is reserved
        &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0x11],       // memory source
        &[0x62, 0xF2, 0x7E, 0x08, 0x29, 0xD1],       // unrelated opcode
        &[0x62, 0xF2, 0x7E, 0x08, 0x28],             // missing ModR/M
        &[0x62, 0xF2, 0x7E, 0x08, 0x28, 0xD1, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_mask_to_vector_requirements(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn mask_to_vector_replay_spans_track_vl_and_dq_requirements() {
    let pc = 0x3900;
    let mut block = SmirBlock::new(BlockId(14), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl, needs_dq) in [
        (&[0x62, 0xF2, 0x7E, 0x08, 0x28, 0xD1][..], true, false),
        (&[0x62, 0xA2, 0xFE, 0x28, 0x38, 0xEA], true, true),
        (&[0x62, 0x42, 0x7E, 0x48, 0x38, 0xFD], false, true),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(14), pc), instruction)]);
        for spans in [
            x86_evex_mask_to_vector_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, needs_vl, "{bytes:02X?}");
            assert_eq!(span.needs_avx512dq, needs_dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}

type MaskBroadcastShape = (u8, bool, u8);

fn mask_broadcast_shapes() -> Vec<MaskBroadcastShape> {
    let mut shapes = Vec::new();
    for (opcode, w) in [(0x2A, true), (0x3A, false)] {
        for ll in 0..=2 {
            shapes.push((opcode, w, ll));
        }
    }
    shapes
}

fn generated_mask_broadcast_encoding(
    shape: MaskBroadcastShape,
    destination: u8,
    source: u8,
    ignored_x: bool,
    ignored_b: bool,
) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    assert!(destination < 32 && source < 8);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if ignored_b {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        0x7E | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | source,
    ]
}

#[test]
fn mask_broadcast_replay_classifier_covers_96_register_encodings() {
    let shapes = mask_broadcast_shapes();
    assert_eq!(shapes.len(), 6);

    let mut encodings = 0usize;
    for shape in shapes {
        for (destination, source) in [(1, 0), (9, 2), (17, 5), (25, 7)] {
            for ignored_x in [false, true] {
                for ignored_b in [false, true] {
                    let bytes = generated_mask_broadcast_encoding(
                        shape,
                        destination,
                        source,
                        ignored_x,
                        ignored_b,
                    );
                    assert_eq!(
                        X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_register_mask_broadcast_needs_vl(),
                        Some(shape.2 != 2),
                        "{bytes:02X?}"
                    );
                    encodings += 1;
                }
            }
        }
    }
    assert_eq!(encodings, 96);

    // Independently assembled LLVM 23 encodings cover both mnemonics, all
    // vector lengths, and both vector-destination extension channels.
    for (bytes, needs_vl) in [
        (&[0x62, 0xE2, 0xFE, 0x48, 0x2A, 0xCF][..], false),
        (&[0x62, 0x62, 0x7E, 0x28, 0x3A, 0xCB], true),
        (&[0x62, 0x62, 0xFE, 0x08, 0x2A, 0xF8], true),
        (&[0x62, 0x72, 0x7E, 0x48, 0x3A, 0xC7], false),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_mask_broadcast_needs_vl(),
            Some(needs_vl),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn mask_broadcast_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0xFE, 0x08, 0x2A, 0xD1],       // not EVEX
        &[0x62, 0xF1, 0xFE, 0x08, 0x2A, 0xD1],       // wrong map
        &[0x62, 0xF2, 0xFA, 0x08, 0x2A, 0xD1],       // missing fixed-one bit
        &[0x62, 0xF2, 0xFD, 0x08, 0x2A, 0xD1],       // wrong mandatory prefix
        &[0x62, 0xF2, 0xF6, 0x08, 0x2A, 0xD1],       // EVEX.vvvv != 1111b
        &[0x62, 0xF2, 0x7E, 0x08, 0x2A, 0xD1],       // MB2Q requires W1
        &[0x62, 0xF2, 0xFE, 0x08, 0x3A, 0xD1],       // MW2D requires W0
        &[0x62, 0xF2, 0xFE, 0x00, 0x2A, 0xD1],       // EVEX.V' is reserved
        &[0x62, 0xF2, 0xFE, 0x88, 0x2A, 0xD1],       // EVEX.z is reserved
        &[0x62, 0xF2, 0xFE, 0x09, 0x2A, 0xD1],       // writemask is forbidden
        &[0x62, 0xF2, 0xFE, 0x18, 0x2A, 0xD1],       // EVEX.b is reserved
        &[0x62, 0xF2, 0xFE, 0x68, 0x2A, 0xD1],       // L'L=3 is reserved
        &[0x62, 0xF2, 0xFE, 0x08, 0x2A, 0x11],       // memory source
        &[0x62, 0xF2, 0xFE, 0x08, 0x2B, 0xD1],       // unrelated opcode
        &[0x62, 0xF2, 0xFE, 0x08, 0x2A],             // missing ModR/M
        &[0x62, 0xF2, 0xFE, 0x08, 0x2A, 0xD1, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_mask_broadcast_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn mask_broadcast_replay_spans_track_vl_requirements() {
    let pc = 0x3A00;
    let mut block = SmirBlock::new(BlockId(15), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xE2, 0xFE, 0x08, 0x2A, 0xCF][..], true),
        (&[0x62, 0x62, 0x7E, 0x28, 0x3A, 0xCB], true),
        (&[0x62, 0x72, 0x7E, 0x48, 0x3A, 0xC7], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(15), pc), instruction)]);
        for spans in [
            x86_evex_mask_broadcast_replay_spans(&block, &provenance),
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

type LaneShuffleShape = (u8, u8, bool, u8, bool);

fn lane_shuffle_shapes() -> Vec<LaneShuffleShape> {
    let mut shapes = Vec::new();
    for (opcode, pp, w) in [(0x12, 2, false), (0x16, 2, false), (0x12, 3, true)] {
        for ll in 0..=2 {
            shapes.push((opcode, pp, w, ll, false));
        }
    }
    for (pp, widths) in [
        (1, &[false][..]),
        (2, &[false, true][..]),
        (3, &[false, true][..]),
    ] {
        for &w in widths {
            for ll in 0..=2 {
                shapes.push((0x70, pp, w, ll, true));
            }
        }
    }
    shapes
}

fn generated_lane_shuffle_encoding(
    shape: LaneShuffleShape,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> Vec<u8> {
    let (opcode, pp, w, ll, immediate) = shape;
    assert!(destination < 32 && source < 32 && mask < 8 && (!zeroing || mask != 0));
    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x10;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x80;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    let mut bytes = vec![
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ];
    if immediate {
        bytes.push(0x93);
    }
    bytes
}

#[test]
fn lane_shuffle_replay_classifier_covers_96_legal_register_encodings() {
    let shapes = lane_shuffle_shapes();
    assert_eq!(shapes.len(), 24);

    let mut register_encodings = 0usize;
    for shape in shapes {
        for bucket in [0, 8, 16, 24] {
            let bytes = generated_lane_shuffle_encoding(shape, 1 + bucket, bucket, 1, true);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_lane_shuffle_needs_vl(),
                Some(shape.3 != 2),
                "{bytes:02X?}"
            );
            register_encodings += 1;
        }

        let mut memory = generated_lane_shuffle_encoding(shape, 1, 0, 1, false);
        memory[5] &= 0x3F;
        assert_eq!(
            X86InstructionBytes::new(&memory)
                .unwrap()
                .evex_register_lane_shuffle_needs_vl(),
            None,
            "{memory:02X?}"
        );
    }
    assert_eq!(register_encodings, 96);

    // Independent LLVM encodings cover all six mnemonics, every vector
    // extension channel, and both values of W for the architecturally WIG
    // word shuffles.
    for bytes in [
        &[0x62, 0x01, 0xFF, 0xCE, 0x12, 0xE8][..],
        &[0x62, 0x21, 0x7E, 0x4D, 0x12, 0xD9],
        &[0x62, 0x21, 0x7E, 0xAC, 0x16, 0xD0],
        &[0x62, 0x21, 0x7D, 0x4B, 0x70, 0xCA, 0x1B],
        &[0x62, 0x01, 0x7E, 0xAA, 0x70, 0xC7, 0xB1],
        &[0x62, 0x81, 0x7F, 0x09, 0x70, 0xFE, 0x93],
        &[0x62, 0x01, 0xFE, 0xAA, 0x70, 0xC7, 0xB1],
        &[0x62, 0x81, 0xFF, 0x09, 0x70, 0xFE, 0x93],
    ] {
        assert!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_lane_shuffle_needs_vl()
                .is_some(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn lane_shuffle_replay_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF1, 0x7E, 0x09, 0x12, 0xC8],       // not EVEX
        &[0x62, 0xF2, 0x7E, 0x09, 0x12, 0xC8],       // wrong map
        &[0x62, 0xF1, 0x7A, 0x09, 0x12, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF1, 0x6E, 0x09, 0x12, 0xC8],       // nonreserved vvvv
        &[0x62, 0xF1, 0x7E, 0x01, 0x12, 0xC8],       // nonreserved V'
        &[0x62, 0xF1, 0xFE, 0x09, 0x12, 0xC8],       // VMOVSLDUP requires W0
        &[0x62, 0xF1, 0x7F, 0x09, 0x12, 0xC8],       // VMOVDDUP requires W1
        &[0x62, 0xF1, 0xFD, 0x09, 0x70, 0xC8, 0x93], // VPSHUFD requires W0
        &[0x62, 0xF1, 0x7C, 0x09, 0x70, 0xC8, 0x93], // wrong mandatory prefix
        &[0x62, 0xF1, 0x7E, 0x19, 0x12, 0xC8],       // EVEX.b
        &[0x62, 0xF1, 0x7E, 0x69, 0x12, 0xC8],       // reserved L'L=3
        &[0x62, 0xF1, 0x7E, 0x88, 0x12, 0xC8],       // {z} with k0
        &[0x62, 0xF1, 0x7E, 0x09, 0x12, 0x08],       // memory source
        &[0x62, 0xF1, 0x7E, 0x09, 0x13, 0xC8],       // unrelated opcode
        &[0x62, 0xF1, 0x7E, 0x09, 0x12],             // missing ModR/M
        &[0x62, 0xF1, 0x7D, 0x09, 0x70, 0xC8],       // missing imm8
        &[0x62, 0xF1, 0x7E, 0x09, 0x12, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_lane_shuffle_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn lane_shuffle_replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x3900;
    let mut block = SmirBlock::new(BlockId(14), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF1, 0x7E, 0x09, 0x12, 0xC8][..], true),
        (&[0x62, 0x01, 0xFE, 0xAA, 0x70, 0xC7, 0xB1], true),
        (&[0x62, 0x21, 0x7D, 0x4B, 0x70, 0xCA, 0x1B], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(14), pc), instruction)]);
        for spans in [
            x86_evex_lane_shuffle_replay_spans(&block, &provenance),
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
