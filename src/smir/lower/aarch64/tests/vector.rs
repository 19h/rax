//! tests::vector tests

use super::*;
use crate::smir::ir::ops::X86SatFpFormat;
use crate::smir::lower::aarch64::*;

#[test]
fn lowers_vector_move_and_logic_runtime() {
    let a_low = 0x0123_4567_89ab_cdef;
    let a_high = 0xfedc_ba98_7654_3210;
    let b_low = 0x0f0f_f0f0_55aa_aa55;
    let b_high = 0x3333_cccc_9696_6969;
    let code = lower_ops(vec![
        OpKind::VMov {
            dst: v(0),
            src: v(1),
            width: VecWidth::V128,
        },
        OpKind::VMov {
            dst: v(6),
            src: v(1),
            width: VecWidth::V64,
        },
        OpKind::VAnd {
            dst: v(2),
            src1: v(1),
            src2: v(3),
            width: VecWidth::V128,
        },
        OpKind::VOr {
            dst: v(4),
            src1: v(1),
            src2: v(3),
            width: VecWidth::V64,
        },
        OpKind::VXor {
            dst: v(5),
            src1: v(1),
            src2: v(3),
            width: VecWidth::V128,
        },
    ]);

    let (_, simd, _) =
        run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, a_low, a_high), (3, b_low, b_high)]);
    assert_eq!(simd[0], (a_low, a_high));
    assert_eq!(simd[6], (a_low, 0));
    assert_eq!(simd[2], (a_low & b_low, a_high & b_high));
    assert_eq!(simd[4], (a_low | b_low, 0));
    assert_eq!(simd[5], (a_low ^ b_low, a_high ^ b_high));
}
#[test]
fn lowers_vector_insert_lane_runtime() {
    let src = (0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210);
    let inplace = (0x1111_2222_3333_4444, 0x5555_6666_7777_8888);
    let code = lower_ops(vec![
        OpKind::VInsertLane {
            dst: v(0),
            vec: v(1),
            scalar: x(2),
            lane: 3,
            elem: VecElementType::I8,
        },
        OpKind::VInsertLane {
            dst: v(3),
            vec: v(1),
            scalar: x(4),
            lane: 7,
            elem: VecElementType::I16,
        },
        OpKind::VInsertLane {
            dst: v(5),
            vec: v(5),
            scalar: x(6),
            lane: 3,
            elem: VecElementType::I32,
        },
        OpKind::VInsertLane {
            dst: v(7),
            vec: v(1),
            scalar: x(8),
            lane: 1,
            elem: VecElementType::I64,
        },
    ]);

    let (_, simd, sp) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[
            (2, 0xaa),
            (4, 0xbeef),
            (6, 0xfeed_face_cafe_babe),
            (8, 0x8877_6655_4433_2211),
        ],
        &[(1, src.0, src.1), (5, inplace.0, inplace.1)],
    );

    assert_eq!(simd[0], set_simd_lane(src, VecElementType::I8, 3, 0xaa));
    assert_eq!(simd[3], set_simd_lane(src, VecElementType::I16, 7, 0xbeef));
    assert_eq!(
        simd[5],
        set_simd_lane(inplace, VecElementType::I32, 3, 0xcafe_babe)
    );
    assert_eq!(
        simd[7],
        set_simd_lane(src, VecElementType::I64, 1, 0x8877_6655_4433_2211)
    );
    assert_eq!(simd[1], src);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_vector_extract_lane_runtime() {
    let source = (0x1122_3344_5566_7788, 0x8000_0001_7fff_8001);
    let code = lower_ops(vec![
        OpKind::VExtractLane {
            dst: x(0),
            vec: v(1),
            lane: 15,
            elem: VecElementType::I8,
            sign: SignExtend::Zero,
        },
        OpKind::VExtractLane {
            dst: x(2),
            vec: v(1),
            lane: 15,
            elem: VecElementType::I8,
            sign: SignExtend::Sign,
        },
        OpKind::VExtractLane {
            dst: x(3),
            vec: v(1),
            lane: 4,
            elem: VecElementType::I16,
            sign: SignExtend::Sign,
        },
        OpKind::VExtractLane {
            dst: x(4),
            vec: v(1),
            lane: 2,
            elem: VecElementType::I32,
            sign: SignExtend::Zero,
        },
        OpKind::VExtractLane {
            dst: x(5),
            vec: v(1),
            lane: 3,
            elem: VecElementType::I32,
            sign: SignExtend::Sign,
        },
        OpKind::VExtractLane {
            dst: x(6),
            vec: v(1),
            lane: 1,
            elem: VecElementType::I64,
            sign: SignExtend::Sign,
        },
    ]);

    let (regs, simd, sp) =
        run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, source.0, source.1)]);

    let b15 = get_simd_lane(source, VecElementType::I8, 15);
    let h4 = get_simd_lane(source, VecElementType::I16, 4);
    let s2 = get_simd_lane(source, VecElementType::I32, 2);
    let s3 = get_simd_lane(source, VecElementType::I32, 3);
    assert_eq!(regs[0], b15);
    assert_eq!(regs[2], sign_extend_simd_lane(b15, VecElementType::I8));
    assert_eq!(regs[3], sign_extend_simd_lane(h4, VecElementType::I16));
    assert_eq!(regs[4], s2);
    assert_eq!(regs[5], sign_extend_simd_lane(s3, VecElementType::I32));
    assert_eq!(regs[6], source.1);
    assert_eq!(simd[1], source);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_vector_lane_scalar_apx_egpr_operands_runtime() {
    fn splat(scalar: u64, elem: VecElementType, lanes: usize) -> (u64, u64) {
        let mut bytes = [0u8; 16];
        let elem_bytes = elem.bytes() as usize;
        for lane in 0..lanes {
            let offset = lane * elem_bytes;
            bytes[offset..offset + elem_bytes].copy_from_slice(&scalar.to_le_bytes()[..elem_bytes]);
        }
        simd_pair_from_bytes(bytes)
    }

    let insert_src = (0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210);
    let extract_src = (0x7654_3210_89ab_cdef, 0x8000_0001_7fff_8001);
    let code = lower_ops(vec![
        OpKind::VBroadcast {
            dst: v(0),
            scalar: x86(X86Reg::R16),
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VInsertLane {
            dst: v(1),
            vec: v(1),
            scalar: x86(X86Reg::R17),
            lane: 2,
            elem: VecElementType::I32,
        },
        OpKind::VExtractLane {
            dst: x86(X86Reg::R18),
            vec: v(2),
            lane: 3,
            elem: VecElementType::I32,
            sign: SignExtend::Sign,
        },
    ]);
    let words = code_words(&code);
    assert_eq!(words.len(), 4);
    assert_eq!(words[0] & 0x1f, 0);
    assert_eq!((words[0] >> 5) & 0x1f, 16);
    assert_eq!(words[1] & 0x1f, 1);
    assert_eq!((words[1] >> 5) & 0x1f, 17);
    assert_eq!(words[2] & 0x1f, 18);
    assert_eq!((words[2] >> 5) & 0x1f, 2);

    let r16 = 0x8877_6655_4433_2211;
    let r17 = 0xaaaa_bbbb_ffff_eeee;
    let sentinel = 0x1919_1919_1919_1919;
    let (regs, simd, sp) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[(16, r16), (17, r17), (19, sentinel)],
        &[
            (1, insert_src.0, insert_src.1),
            (2, extract_src.0, extract_src.1),
        ],
    );
    let s3 = get_simd_lane(extract_src, VecElementType::I32, 3);
    assert_eq!(simd[0], splat(r16, VecElementType::I32, 4));
    assert_eq!(
        simd[1],
        set_simd_lane(insert_src, VecElementType::I32, 2, r17)
    );
    assert_eq!(simd[2], extract_src);
    assert_eq!(regs[16], r16);
    assert_eq!(regs[17], r17);
    assert_eq!(regs[18], sign_extend_simd_lane(s3, VecElementType::I32));
    assert_eq!(regs[19], sentinel);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_vector_lane_scalar_apx_r31_identity_mapping() {
    for kind in [
        OpKind::VBroadcast {
            dst: v(0),
            scalar: x86(X86Reg::R31),
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VInsertLane {
            dst: v(0),
            vec: v(1),
            scalar: x86(X86Reg::R31),
            lane: 0,
            elem: VecElementType::I32,
        },
        OpKind::VExtractLane {
            dst: x86(X86Reg::R31),
            vec: v(1),
            lane: 0,
            elem: VecElementType::I32,
            sign: SignExtend::Zero,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn rejects_vector_lane_invalid_operands() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::VInsertLane {
            dst: v(0),
            vec: v(1),
            scalar: x(2),
            lane: 16,
            elem: VecElementType::I8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::InvalidOperand { .. }));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::VExtractLane {
            dst: x(0),
            vec: v(1),
            lane: 0,
            elem: VecElementType::F32,
            sign: SignExtend::Zero,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_vector_integer_arithmetic_runtime() {
    fn apply_lanes<F: Fn(u64, u64) -> u64>(
        a_low: u64,
        a_high: u64,
        b_low: u64,
        b_high: u64,
        elem_bytes: usize,
        lanes: usize,
        op: F,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        let mut out = [0u8; 16];
        a[..8].copy_from_slice(&a_low.to_le_bytes());
        a[8..].copy_from_slice(&a_high.to_le_bytes());
        b[..8].copy_from_slice(&b_low.to_le_bytes());
        b[8..].copy_from_slice(&b_high.to_le_bytes());

        let mask = if elem_bytes == 8 {
            u64::MAX
        } else {
            (1u64 << (elem_bytes * 8)) - 1
        };
        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let value = op(
                read_lane(&a, off, elem_bytes),
                read_lane(&b, off, elem_bytes),
            ) & mask;
            out[off..off + elem_bytes].copy_from_slice(&value.to_le_bytes()[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let a_low = 0xfedc_ba98_7654_3210;
    let a_high = 0x0123_4567_89ab_cdef;
    let b_low = 0x1020_3040_5060_7080;
    let b_high = 0x8877_6655_4433_2211;
    let code = lower_ops(vec![
        OpKind::VAdd {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
        },
        OpKind::VSub {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
        },
        OpKind::VMul {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VAdd {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 2,
        },
        OpKind::VSub {
            dst: v(6),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I64,
            lanes: 2,
        },
        OpKind::VMul {
            dst: v(7),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 4,
        },
    ]);

    let (_, simd, _) =
        run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, a_low, a_high), (2, b_low, b_high)]);
    assert_eq!(
        simd[0],
        apply_lanes(a_low, a_high, b_low, b_high, 1, 16, u64::wrapping_add)
    );
    assert_eq!(
        simd[3],
        apply_lanes(a_low, a_high, b_low, b_high, 2, 8, u64::wrapping_sub)
    );
    assert_eq!(
        simd[4],
        apply_lanes(a_low, a_high, b_low, b_high, 4, 4, u64::wrapping_mul)
    );
    assert_eq!(
        simd[5],
        apply_lanes(a_low, a_high, b_low, b_high, 4, 2, u64::wrapping_add)
    );
    assert_eq!(
        simd[6],
        apply_lanes(a_low, a_high, b_low, b_high, 8, 2, u64::wrapping_sub)
    );
    assert_eq!(
        simd[7],
        apply_lanes(a_low, a_high, b_low, b_high, 2, 4, u64::wrapping_mul)
    );
}
#[test]
fn lowers_vector_float_arithmetic_runtime() {
    fn apply_f32<F: Fn(f32, f32) -> f32>(a: [f32; 4], b: [f32; 4], op: F) -> (u64, u64) {
        simd_pair_from_f32([
            op(a[0], b[0]),
            op(a[1], b[1]),
            op(a[2], b[2]),
            op(a[3], b[3]),
        ])
    }

    fn apply_f64<F: Fn(f64, f64) -> f64>(a: [f64; 2], b: [f64; 2], op: F) -> (u64, u64) {
        simd_pair_from_f64([op(a[0], b[0]), op(a[1], b[1])])
    }

    let a32 = [1.5, -2.25, 8.0, -0.5];
    let b32 = [2.25, 3.5, -1.5, 4.0];
    let a32x2 = [0.75, -3.0, 99.0, -99.0];
    let b32x2 = [1.25, 2.5, -7.0, 11.0];
    let a64 = [-7.0, 2.5];
    let b64 = [3.0, -1.25];
    let code = lower_ops(vec![
        OpKind::VAdd {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VSub {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VMul {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VMax {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VMin {
            dst: v(6),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            lanes: 4,
            signed: false,
        },
        OpKind::VAdd {
            dst: v(9),
            src1: v(7),
            src2: v(8),
            elem: VecElementType::F32,
            lanes: 2,
        },
        OpKind::VAdd {
            dst: v(10),
            src1: v(11),
            src2: v(12),
            elem: VecElementType::F64,
            lanes: 2,
        },
        OpKind::VSub {
            dst: v(13),
            src1: v(11),
            src2: v(12),
            elem: VecElementType::F64,
            lanes: 2,
        },
        OpKind::VMul {
            dst: v(14),
            src1: v(11),
            src2: v(12),
            elem: VecElementType::F64,
            lanes: 2,
        },
        OpKind::VMax {
            dst: v(15),
            src1: v(11),
            src2: v(12),
            elem: VecElementType::F64,
            lanes: 2,
        },
        OpKind::VMin {
            dst: v(16),
            src1: v(11),
            src2: v(12),
            elem: VecElementType::F64,
            lanes: 2,
            signed: false,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (1, simd_pair_from_f32(a32).0, simd_pair_from_f32(a32).1),
            (2, simd_pair_from_f32(b32).0, simd_pair_from_f32(b32).1),
            (7, simd_pair_from_f32(a32x2).0, simd_pair_from_f32(a32x2).1),
            (8, simd_pair_from_f32(b32x2).0, simd_pair_from_f32(b32x2).1),
            (11, simd_pair_from_f64(a64).0, simd_pair_from_f64(a64).1),
            (12, simd_pair_from_f64(b64).0, simd_pair_from_f64(b64).1),
        ],
    );

    assert_eq!(simd[0], apply_f32(a32, b32, |a, b| a + b));
    assert_eq!(simd[3], apply_f32(a32, b32, |a, b| a - b));
    assert_eq!(simd[4], apply_f32(a32, b32, |a, b| a * b));
    assert_eq!(simd[5], apply_f32(a32, b32, f32::max));
    assert_eq!(simd[6], apply_f32(a32, b32, f32::min));
    assert_eq!(
        simd[9],
        simd_pair_from_f32([a32x2[0] + b32x2[0], a32x2[1] + b32x2[1], 0.0, 0.0])
    );
    assert_eq!(simd[10], apply_f64(a64, b64, |a, b| a + b));
    assert_eq!(simd[13], apply_f64(a64, b64, |a, b| a - b));
    assert_eq!(simd[14], apply_f64(a64, b64, |a, b| a * b));
    assert_eq!(simd[15], apply_f64(a64, b64, f64::max));
    assert_eq!(simd[16], apply_f64(a64, b64, f64::min));
}
#[test]
fn lowers_vector_minmax_runtime() {
    fn apply_f32<F: Fn(f32, f32) -> f32>(a: [f32; 4], b: [f32; 4], op: F) -> (u64, u64) {
        simd_pair_from_f32([
            op(a[0], b[0]),
            op(a[1], b[1]),
            op(a[2], b[2]),
            op(a[3], b[3]),
        ])
    }

    fn apply_f64<F: Fn(f64, f64) -> f64>(a: [f64; 2], b: [f64; 2], op: F) -> (u64, u64) {
        simd_pair_from_f64([op(a[0], b[0]), op(a[1], b[1])])
    }

    let a32 = [1.5, -2.25, 8.0, -0.5];
    let b32 = [2.25, 3.5, -1.5, 4.0];
    let a64 = [-7.0, 2.5];
    let b64 = [3.0, -1.25];
    let code = lower_ops(vec![
        OpKind::VMinMax {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            width: VecWidth::V128,
            imm: 0,
        },
        OpKind::VMinMax {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::F32,
            width: VecWidth::V128,
            imm: 5,
        },
        OpKind::VMinMax {
            dst: v(4),
            src1: v(5),
            src2: v(6),
            elem: VecElementType::F64,
            width: VecWidth::V128,
            imm: 2,
        },
        OpKind::VMinMax {
            dst: v(7),
            src1: v(5),
            src2: v(6),
            elem: VecElementType::F64,
            width: VecWidth::V128,
            imm: 3,
        },
    ]);

    let (_, simd, sp) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (1, simd_pair_from_f32(a32).0, simd_pair_from_f32(a32).1),
            (2, simd_pair_from_f32(b32).0, simd_pair_from_f32(b32).1),
            (5, simd_pair_from_f64(a64).0, simd_pair_from_f64(a64).1),
            (6, simd_pair_from_f64(b64).0, simd_pair_from_f64(b64).1),
        ],
    );

    assert_eq!(simd[0], apply_f32(a32, b32, f32::min));
    assert_eq!(simd[3], apply_f32(a32, b32, f32::max));
    assert_eq!(simd[4], apply_f64(a64, b64, f64::min));
    assert_eq!(simd[7], apply_f64(a64, b64, f64::max));
    assert_eq!(simd[1], simd_pair_from_f32(a32));
    assert_eq!(simd[2], simd_pair_from_f32(b32));
    assert_eq!(simd[5], simd_pair_from_f64(a64));
    assert_eq!(simd[6], simd_pair_from_f64(b64));
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_vector_fp16_arithmetic_runtime() {
    let a128 = [
        f16_bits(1.5),
        f16_bits(-2.0),
        f16_bits(0.5),
        f16_bits(4.0),
        f16_bits(8.0),
        f16_bits(-0.25),
        f16_bits(16.0),
        f16_bits(-32.0),
    ];
    let b128 = [
        f16_bits(2.0),
        f16_bits(3.0),
        f16_bits(-1.5),
        f16_bits(0.5),
        f16_bits(-4.0),
        f16_bits(-0.75),
        f16_bits(0.5),
        f16_bits(-2.0),
    ];
    let a64 = [
        f16_bits(3.0),
        f16_bits(-4.0),
        f16_bits(0.5),
        f16_bits(-0.25),
        f16_bits(12.0),
        f16_bits(13.0),
        f16_bits(14.0),
        f16_bits(15.0),
    ];
    let b64 = [
        f16_bits(0.5),
        f16_bits(2.0),
        f16_bits(-8.0),
        f16_bits(-0.5),
        f16_bits(16.0),
        f16_bits(17.0),
        f16_bits(18.0),
        f16_bits(19.0),
    ];
    let code = lower_ops(vec![
        OpKind::VFP16Arith {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            mask: None,
            op: Avx10FP16Op::Add,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            lanes: 8,
            zeroing: false,
        },
        OpKind::VFP16Arith {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            mask: None,
            op: Avx10FP16Op::Sub,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            lanes: 8,
            zeroing: false,
        },
        OpKind::VFP16Arith {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            mask: None,
            op: Avx10FP16Op::Mul,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            lanes: 8,
            zeroing: false,
        },
        OpKind::VFP16Arith {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            mask: None,
            op: Avx10FP16Op::Div,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            lanes: 8,
            zeroing: false,
        },
        OpKind::VFP16Arith {
            dst: v(8),
            src1: v(6),
            src2: v(7),
            mask: None,
            op: Avx10FP16Op::Add,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V64,
            lanes: 4,
            zeroing: false,
        },
        OpKind::VFP16Arith {
            dst: v(9),
            src1: v(6),
            src2: v(7),
            mask: None,
            op: Avx10FP16Op::Div,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V64,
            lanes: 4,
            zeroing: false,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (1, simd_pair_from_f16(a128).0, simd_pair_from_f16(a128).1),
            (2, simd_pair_from_f16(b128).0, simd_pair_from_f16(b128).1),
            (6, simd_pair_from_f16(a64).0, simd_pair_from_f16(a64).1),
            (7, simd_pair_from_f16(b64).0, simd_pair_from_f16(b64).1),
        ],
    );

    assert_eq!(simd[0], apply_f16_lanes(a128, b128, 8, Avx10FP16Op::Add));
    assert_eq!(simd[3], apply_f16_lanes(a128, b128, 8, Avx10FP16Op::Sub));
    assert_eq!(simd[4], apply_f16_lanes(a128, b128, 8, Avx10FP16Op::Mul));
    assert_eq!(simd[5], apply_f16_lanes(a128, b128, 8, Avx10FP16Op::Div));
    assert_eq!(simd[8], apply_f16_lanes(a64, b64, 4, Avx10FP16Op::Add));
    assert_eq!(simd[9], apply_f16_lanes(a64, b64, 4, Avx10FP16Op::Div));
}
#[test]
fn lowers_vector_bf16_conversion_encodings() {
    let code = lower_ops(vec![
        OpKind::VCvtFP32ToBF16 {
            dst: v(0),
            src1: v(1),
            src2: None,
            mask: None,
            width: VecWidth::V128,
            zeroing: false,
        },
        OpKind::VCvtFP32ToBF16 {
            dst: v(3),
            src1: v(4),
            src2: Some(v(5)),
            mask: None,
            width: VecWidth::V128,
            zeroing: false,
        },
    ]);
    let words = code_words(&code);

    assert_eq!(words[0], 0x0ea1_6800 | (1 << 5));
    assert_eq!(words[1], 0x0ea1_6800 | (5 << 5) | 3);
    assert_eq!(words[2], 0x4ea1_6800 | (4 << 5) | 3);
    assert_eq!(words[3], 0xd65f_03c0);
}
#[test]
fn lowers_vector_bf16_conversion_runtime() {
    let src1 = [0x3f80_0000, 0xbf80_0000, 0x3f80_8000, 0x3f81_8000];
    let src2 = [0xc020_0000, 0x0080_0000, 0x0000_0001, 0x7fc0_1234];
    let code = lower_ops(vec![
        OpKind::VCvtFP32ToBF16 {
            dst: v(0),
            src1: v(1),
            src2: None,
            mask: None,
            width: VecWidth::V128,
            zeroing: false,
        },
        OpKind::VCvtFP32ToBF16 {
            dst: v(3),
            src1: v(1),
            src2: Some(v(2)),
            mask: None,
            width: VecWidth::V128,
            zeroing: false,
        },
        OpKind::VCvtFP32ToBF16 {
            dst: v(2),
            src1: v(1),
            src2: Some(v(2)),
            mask: None,
            width: VecWidth::V128,
            zeroing: false,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (
                1,
                simd_pair_from_f32_bits(src1).0,
                simd_pair_from_f32_bits(src1).1,
            ),
            (
                2,
                simd_pair_from_f32_bits(src2).0,
                simd_pair_from_f32_bits(src2).1,
            ),
        ],
    );

    assert_eq!(simd[0], bf16_pair_from_f32_bits(src1));
    assert_eq!(simd[3], bf16_pair_from_two_f32_bits(src2, src1));
    assert_eq!(simd[2], bf16_pair_from_two_f32_bits(src2, src1));
}
#[test]
fn lowers_vector_bf16_to_fp32_conversion_encodings() {
    let code = lower_ops(vec![OpKind::VCvtBF16ToFP32 {
        dst: v(0),
        src: v(1),
        width: VecWidth::V128,
    }]);
    let words = code_words(&code);

    assert_eq!(words[0], enc_simd_shift_imm(0, 1, 0, 1, 0b0010, 0, 0b10100));
    assert_eq!(words[1], enc_simd_shift_imm(0, 0, 1, 0, 0b0110, 0, 0b01010));
    assert_eq!(words[2], 0xd65f_03c0);
}
#[test]
fn lowers_vector_bf16_to_fp32_conversion_runtime() {
    let src = [
        0x3f80, 0xbf80, 0x7fc1, 0x0080, 0x4000, 0xc040, 0x0001, 0x7f80,
    ];
    let expected = [0x3f80_0000, 0xbf80_0000, 0x7fc1_0000, 0x0080_0000];
    let code = lower_ops(vec![OpKind::VCvtBF16ToFP32 {
        dst: v(0),
        src: v(1),
        width: VecWidth::V128,
    }]);

    let (_, simd, sp) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[(1, simd_pair_from_bf16(src).0, simd_pair_from_bf16(src).1)],
    );

    assert_eq!(simd[0], simd_pair_from_f32_bits(expected));
    assert_eq!(simd[1], simd_pair_from_bf16(src));
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_x86_saturating_fp_to_int_conversion() {
    let err = try_lower_single_op(OpKind::VCvtFpToIntSat {
        dst: v(0),
        src: v(1),
        mask: None,
        fp_elem: X86SatFpFormat::F32,
        int_elem: VecElementType::I8,
        width: VecWidth::V128,
        signed: true,
        truncate: true,
        round: FpRoundMode::RoundTowardZero,
        zeroing: false,
        suppress_exceptions: false,
    })
    .unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
#[test]
fn lowers_vector_bf16_dot_product_encodings() {
    let code = lower_ops(vec![
        OpKind::VDotProductBF16 {
            dst: v(0),
            acc: v(0),
            src1: v(1),
            src2: v(2),
            mask: None,
            width: VecWidth::V128,
            zeroing: false,
        },
        OpKind::VDotProductBF16 {
            dst: v(3),
            acc: v(4),
            src1: v(5),
            src2: v(6),
            mask: None,
            width: VecWidth::V64,
            zeroing: false,
        },
    ]);
    let words = code_words(&code);

    assert_eq!(words[0], 0x6e40_fc00 | (2 << 16) | (1 << 5));
    assert_eq!(words[1], 0x0ea4_1c83);
    assert_eq!(words[2], 0x2e40_fc00 | (6 << 16) | (5 << 5) | 3);
    assert_eq!(words[3], 0xd65f_03c0);
}
#[test]
fn lowers_vector_bf16_dot_product_runtime() {
    let acc0 = [10.0, -20.0, 0.5, 1000.0];
    let src1 = [
        0x3f80, 0x4000, 0xc040, 0x4080, 0x4100, 0xc100, 0x3f00, 0xbf80,
    ];
    let src2 = [
        0x4040, 0xbf80, 0x4000, 0x3f80, 0xc000, 0xc040, 0x4080, 0x4000,
    ];
    let acc1 = [1.0, -2.0, 0.0, 0.0];
    let src3 = [
        0x3f80, 0xbf80, 0x4000, 0x4040, 0x3f00, 0xc000, 0x4080, 0x4100,
    ];
    let src4 = [
        0x4000, 0x4000, 0xc000, 0x3f80, 0x3f80, 0x3f80, 0x4000, 0x4000,
    ];
    let code = lower_ops(vec![
        OpKind::VDotProductBF16 {
            dst: v(0),
            acc: v(0),
            src1: v(1),
            src2: v(2),
            mask: None,
            width: VecWidth::V128,
            zeroing: false,
        },
        OpKind::VDotProductBF16 {
            dst: v(3),
            acc: v(4),
            src1: v(5),
            src2: v(6),
            mask: None,
            width: VecWidth::V64,
            zeroing: false,
        },
    ]);

    let (_, simd, sp) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (0, simd_pair_from_f32(acc0).0, simd_pair_from_f32(acc0).1),
            (1, simd_pair_from_bf16(src1).0, simd_pair_from_bf16(src1).1),
            (2, simd_pair_from_bf16(src2).0, simd_pair_from_bf16(src2).1),
            (4, simd_pair_from_f32(acc1).0, simd_pair_from_f32(acc1).1),
            (5, simd_pair_from_bf16(src3).0, simd_pair_from_bf16(src3).1),
            (6, simd_pair_from_bf16(src4).0, simd_pair_from_bf16(src4).1),
        ],
    );

    assert_eq!(simd[0], ref_bf16_dot(acc0, src1, src2, 4));
    assert_eq!(simd[3], ref_bf16_dot(acc1, src3, src4, 2));
    assert_eq!(simd[1], simd_pair_from_bf16(src1));
    assert_eq!(simd[2], simd_pair_from_bf16(src2));
    assert_eq!(simd[5], simd_pair_from_bf16(src3));
    assert_eq!(simd[6], simd_pair_from_bf16(src4));
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_vector_i8_dot_product_encodings() {
    let code = lower_ops(vec![
        OpKind::VDotProduct {
            dst: v(0),
            acc: v(0),
            src1: v(1),
            src2: v(2),
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V128,
            src1_unsigned: true,
            saturate: false,
            zeroing: false,
        },
        OpKind::VDotProduct {
            dst: v(3),
            acc: v(3),
            src1: v(4),
            src2: v(5),
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V64,
            src1_unsigned: false,
            saturate: false,
            zeroing: false,
        },
    ]);
    let words = code_words(&code);

    assert_eq!(words[0], 0x4e80_9c00 | (2 << 16) | (1 << 5));
    assert_eq!(words[1], 0x0e80_9400 | (5 << 16) | (4 << 5) | 3);
    assert_eq!(words[2], 0xd65f_03c0);
}
#[test]
fn lowers_vector_i8_dot_product_runtime() {
    let acc0 = [1000, -2000, 0x7fff_ff00u32 as i32, i32::MIN + 16];
    let src1_unsigned = [1, 2, 3, 4, 255, 128, 0, 7, 10, 20, 30, 40, 5, 6, 7, 8];
    let src2_signed = [
        0xff, 2, 0xfe, 4, 1, 0x80, 0x7f, 0, 3, 4, 5, 6, 0x80, 0x80, 2, 3,
    ];
    let acc1 = [17, -33, 44, -55];
    let src1_signed = [
        0xff, 2, 0x80, 0x7f, 9, 0xf0, 0x10, 0x80, 1, 3, 5, 7, 0x7f, 0x80, 0, 4,
    ];
    let src2_signed_2 = [
        2, 0xfe, 3, 0xff, 0x80, 4, 0xfc, 5, 6, 0xfa, 8, 0xf8, 0xff, 2, 3, 4,
    ];
    let acc64 = [123, -456, 0x1111_1111, 0x2222_2222];
    let code = lower_ops(vec![
        OpKind::VDotProduct {
            dst: v(0),
            acc: v(0),
            src1: v(1),
            src2: v(2),
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V128,
            src1_unsigned: true,
            saturate: false,
            zeroing: false,
        },
        OpKind::VDotProduct {
            dst: v(3),
            acc: v(4),
            src1: v(5),
            src2: v(6),
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V128,
            src1_unsigned: false,
            saturate: false,
            zeroing: false,
        },
        OpKind::VDotProduct {
            dst: v(7),
            acc: v(7),
            src1: v(8),
            src2: v(9),
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V64,
            src1_unsigned: true,
            saturate: false,
            zeroing: false,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (0, simd_pair_from_i32(acc0).0, simd_pair_from_i32(acc0).1),
            (
                1,
                simd_pair_from_bytes(src1_unsigned).0,
                simd_pair_from_bytes(src1_unsigned).1,
            ),
            (
                2,
                simd_pair_from_bytes(src2_signed).0,
                simd_pair_from_bytes(src2_signed).1,
            ),
            (4, simd_pair_from_i32(acc1).0, simd_pair_from_i32(acc1).1),
            (
                5,
                simd_pair_from_bytes(src1_signed).0,
                simd_pair_from_bytes(src1_signed).1,
            ),
            (
                6,
                simd_pair_from_bytes(src2_signed_2).0,
                simd_pair_from_bytes(src2_signed_2).1,
            ),
            (7, simd_pair_from_i32(acc64).0, simd_pair_from_i32(acc64).1),
            (
                8,
                simd_pair_from_bytes(src1_unsigned).0,
                simd_pair_from_bytes(src1_unsigned).1,
            ),
            (
                9,
                simd_pair_from_bytes(src2_signed).0,
                simd_pair_from_bytes(src2_signed).1,
            ),
        ],
    );

    assert_eq!(
        simd[0],
        ref_i8_dot(acc0, src1_unsigned, src2_signed, true, 4)
    );
    assert_eq!(
        simd[3],
        ref_i8_dot(acc1, src1_signed, src2_signed_2, false, 4)
    );
    assert_eq!(
        simd[7],
        ref_i8_dot(acc64, src1_unsigned, src2_signed, true, 2)
    );
}
#[test]
fn lowers_vector_i8_dot_product_ext_runtime() {
    let acc0 = [1000, -2000, 0x7fff_ff00u32 as i32, i32::MIN + 16];
    let src1_signed = [
        0xff, 2, 0x80, 0x7f, 9, 0xf0, 0x10, 0x80, 1, 3, 5, 7, 0x7f, 0x80, 0, 4,
    ];
    let src2_signed = [
        2, 0xfe, 3, 0xff, 0x80, 4, 0xfc, 5, 6, 0xfa, 8, 0xf8, 0xff, 2, 3, 4,
    ];
    let acc1 = [17, -33, 44, -55];
    let src1_unsigned = [1, 2, 3, 4, 255, 128, 0, 7, 10, 20, 30, 40, 5, 6, 7, 8];
    let src2_unsigned = [250, 2, 254, 4, 1, 128, 127, 0, 3, 4, 5, 6, 128, 128, 2, 3];
    let acc2 = [123, -456, 0x1111_1111, 0x2222_2222];
    let code = lower_ops(vec![
        OpKind::VDotProductExt {
            dst: v(0),
            acc: v(0),
            src1: v(1),
            src2: v(2),
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V128,
            src1_signed: true,
            src2_signed: true,
            saturate: false,
        },
        OpKind::VDotProductExt {
            dst: v(3),
            acc: v(4),
            src1: v(5),
            src2: v(6),
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V128,
            src1_signed: false,
            src2_signed: false,
            saturate: false,
        },
        OpKind::VDotProductExt {
            dst: v(7),
            acc: v(7),
            src1: v(8),
            src2: v(9),
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V64,
            src1_signed: true,
            src2_signed: false,
            saturate: false,
        },
    ]);

    let (_, simd, sp) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (0, simd_pair_from_i32(acc0).0, simd_pair_from_i32(acc0).1),
            (
                1,
                simd_pair_from_bytes(src1_signed).0,
                simd_pair_from_bytes(src1_signed).1,
            ),
            (
                2,
                simd_pair_from_bytes(src2_signed).0,
                simd_pair_from_bytes(src2_signed).1,
            ),
            (4, simd_pair_from_i32(acc1).0, simd_pair_from_i32(acc1).1),
            (
                5,
                simd_pair_from_bytes(src1_unsigned).0,
                simd_pair_from_bytes(src1_unsigned).1,
            ),
            (
                6,
                simd_pair_from_bytes(src2_unsigned).0,
                simd_pair_from_bytes(src2_unsigned).1,
            ),
            (7, simd_pair_from_i32(acc2).0, simd_pair_from_i32(acc2).1),
            (
                8,
                simd_pair_from_bytes(src1_signed).0,
                simd_pair_from_bytes(src1_signed).1,
            ),
            (
                9,
                simd_pair_from_bytes(src2_unsigned).0,
                simd_pair_from_bytes(src2_unsigned).1,
            ),
        ],
    );

    assert_eq!(
        simd[0],
        ref_i8_dot_ext(acc0, src1_signed, src2_signed, true, true, 4)
    );
    assert_eq!(
        simd[3],
        ref_i8_dot_ext(acc1, src1_unsigned, src2_unsigned, false, false, 4)
    );
    assert_eq!(
        simd[7],
        ref_i8_dot_ext(acc2, src1_signed, src2_unsigned, true, false, 2)
    );
    assert_eq!(simd[1], simd_pair_from_bytes(src1_signed));
    assert_eq!(simd[2], simd_pair_from_bytes(src2_signed));
    assert_eq!(simd[5], simd_pair_from_bytes(src1_unsigned));
    assert_eq!(simd[6], simd_pair_from_bytes(src2_unsigned));
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_vector_float_fma_runtime() {
    fn apply_f32(a: [f32; 4], b: [f32; 4], acc: [f32; 4], negate_product: bool) -> (u64, u64) {
        let lhs = |value: f32| if negate_product { -value } else { value };
        simd_pair_from_f32([
            lhs(a[0]).mul_add(b[0], acc[0]),
            lhs(a[1]).mul_add(b[1], acc[1]),
            lhs(a[2]).mul_add(b[2], acc[2]),
            lhs(a[3]).mul_add(b[3], acc[3]),
        ])
    }

    fn apply_f64(a: [f64; 2], b: [f64; 2], acc: [f64; 2], negate_product: bool) -> (u64, u64) {
        let lhs = |value: f64| if negate_product { -value } else { value };
        simd_pair_from_f64([
            lhs(a[0]).mul_add(b[0], acc[0]),
            lhs(a[1]).mul_add(b[1], acc[1]),
        ])
    }

    let a32 = [1.5, -2.0, 0.25, 4.0];
    let b32 = [2.0, 3.0, -8.0, 0.5];
    let acc32 = [0.25, 10.0, 1.0, -1.0];
    let a32x2 = [3.0, -4.0, 77.0, -99.0];
    let b32x2 = [0.5, 2.0, 1.0, 1.0];
    let acc32x2 = [1.25, -2.0, 5.0, 6.0];
    let a64 = [1.5, -3.0];
    let b64 = [4.0, -2.0];
    let acc64 = [-0.5, 7.0];
    let code = lower_ops(vec![
        OpKind::VFma {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            acc: v(3),
            elem: VecElementType::F32,
            lanes: 4,
            negate_product: false,
            negate_acc: false,
        },
        OpKind::VFma {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            acc: v(13),
            elem: VecElementType::F32,
            lanes: 4,
            negate_product: true,
            negate_acc: false,
        },
        OpKind::VFma {
            dst: v(7),
            src1: v(5),
            src2: v(6),
            acc: v(7),
            elem: VecElementType::F32,
            lanes: 2,
            negate_product: false,
            negate_acc: false,
        },
        OpKind::VFma {
            dst: v(10),
            src1: v(8),
            src2: v(9),
            acc: v(10),
            elem: VecElementType::F64,
            lanes: 2,
            negate_product: false,
            negate_acc: false,
        },
        OpKind::VFma {
            dst: v(12),
            src1: v(8),
            src2: v(9),
            acc: v(14),
            elem: VecElementType::F64,
            lanes: 2,
            negate_product: true,
            negate_acc: false,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (1, simd_pair_from_f32(a32).0, simd_pair_from_f32(a32).1),
            (2, simd_pair_from_f32(b32).0, simd_pair_from_f32(b32).1),
            (3, simd_pair_from_f32(acc32).0, simd_pair_from_f32(acc32).1),
            (5, simd_pair_from_f32(a32x2).0, simd_pair_from_f32(a32x2).1),
            (6, simd_pair_from_f32(b32x2).0, simd_pair_from_f32(b32x2).1),
            (
                7,
                simd_pair_from_f32(acc32x2).0,
                simd_pair_from_f32(acc32x2).1,
            ),
            (8, simd_pair_from_f64(a64).0, simd_pair_from_f64(a64).1),
            (9, simd_pair_from_f64(b64).0, simd_pair_from_f64(b64).1),
            (10, simd_pair_from_f64(acc64).0, simd_pair_from_f64(acc64).1),
            (13, simd_pair_from_f32(acc32).0, simd_pair_from_f32(acc32).1),
            (14, simd_pair_from_f64(acc64).0, simd_pair_from_f64(acc64).1),
        ],
    );

    assert_eq!(simd[3], apply_f32(a32, b32, acc32, false));
    assert_eq!(simd[4], apply_f32(a32, b32, acc32, true));
    assert_eq!(
        simd[7],
        simd_pair_from_f32([
            a32x2[0].mul_add(b32x2[0], acc32x2[0]),
            a32x2[1].mul_add(b32x2[1], acc32x2[1]),
            0.0,
            0.0,
        ])
    );
    assert_eq!(simd[10], apply_f64(a64, b64, acc64, false));
    assert_eq!(simd[12], apply_f64(a64, b64, acc64, true));
}
#[test]
fn lowers_vector_integer_max_runtime() {
    fn apply_max(
        a_low: u64,
        a_high: u64,
        b_low: u64,
        b_high: u64,
        elem_bytes: usize,
        lanes: usize,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        let mut out = [0u8; 16];
        a[..8].copy_from_slice(&a_low.to_le_bytes());
        a[8..].copy_from_slice(&a_high.to_le_bytes());
        b[..8].copy_from_slice(&b_low.to_le_bytes());
        b[8..].copy_from_slice(&b_high.to_le_bytes());

        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let value = read_lane(&a, off, elem_bytes).max(read_lane(&b, off, elem_bytes));
            out[off..off + elem_bytes].copy_from_slice(&value.to_le_bytes()[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let a_low = 0x807f_00ff_7f80_ff00;
    let a_high = 0x0001_ffff_8000_7fff;
    let b_low = 0x7f80_ff00_0080_00ff;
    let b_high = 0xffff_0001_7fff_8000;
    let code = lower_ops(vec![
        OpKind::VMax {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
        },
        OpKind::VMax {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
        },
        OpKind::VMax {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VMax {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 2,
        },
    ]);

    let (_, simd, _) =
        run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, a_low, a_high), (2, b_low, b_high)]);
    assert_eq!(simd[0], apply_max(a_low, a_high, b_low, b_high, 1, 16));
    assert_eq!(simd[3], apply_max(a_low, a_high, b_low, b_high, 2, 8));
    assert_eq!(simd[4], apply_max(a_low, a_high, b_low, b_high, 4, 4));
    assert_eq!(simd[5], apply_max(a_low, a_high, b_low, b_high, 4, 2));
}
#[test]
fn lowers_vector_integer_min_runtime() {
    fn apply_min(
        a_low: u64,
        a_high: u64,
        b_low: u64,
        b_high: u64,
        elem_bytes: usize,
        lanes: usize,
        signed: bool,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        fn sign_extend(value: u64, bits: usize) -> i64 {
            let shift = 64 - bits;
            ((value << shift) as i64) >> shift
        }

        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        let mut out = [0u8; 16];
        a[..8].copy_from_slice(&a_low.to_le_bytes());
        a[8..].copy_from_slice(&a_high.to_le_bytes());
        b[..8].copy_from_slice(&b_low.to_le_bytes());
        b[8..].copy_from_slice(&b_high.to_le_bytes());

        let elem_bits = elem_bytes * 8;
        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let av = read_lane(&a, off, elem_bytes);
            let bv = read_lane(&b, off, elem_bytes);
            let value = if signed {
                if sign_extend(av, elem_bits) <= sign_extend(bv, elem_bits) {
                    av
                } else {
                    bv
                }
            } else {
                av.min(bv)
            };
            out[off..off + elem_bytes].copy_from_slice(&value.to_le_bytes()[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let a_low = 0x807f_00ff_7f80_ff00;
    let a_high = 0x0001_ffff_8000_7fff;
    let b_low = 0x7f80_ff00_0080_00ff;
    let b_high = 0xffff_0001_7fff_8000;
    let code = lower_ops(vec![
        OpKind::VMin {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
            signed: false,
        },
        OpKind::VMin {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
            signed: false,
        },
        OpKind::VMin {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
            signed: true,
        },
        OpKind::VMin {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 4,
            signed: true,
        },
    ]);

    let (_, simd, _) =
        run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, a_low, a_high), (2, b_low, b_high)]);
    assert_eq!(
        simd[0],
        apply_min(a_low, a_high, b_low, b_high, 1, 16, false)
    );
    assert_eq!(
        simd[3],
        apply_min(a_low, a_high, b_low, b_high, 2, 8, false)
    );
    assert_eq!(
        simd[4],
        apply_min(a_low, a_high, b_low, b_high, 1, 16, true)
    );
    assert_eq!(simd[5], apply_min(a_low, a_high, b_low, b_high, 4, 4, true));
}
#[test]
fn lowers_vlane_integer_runtime() {
    fn apply_vlane(
        a_low: u64,
        a_high: u64,
        b_low: u64,
        b_high: u64,
        elem_bytes: usize,
        lanes: usize,
        op: VLaneOp,
        signed: bool,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        let mut out = [0u8; 16];
        a[..8].copy_from_slice(&a_low.to_le_bytes());
        a[8..].copy_from_slice(&a_high.to_le_bytes());
        b[..8].copy_from_slice(&b_low.to_le_bytes());
        b[8..].copy_from_slice(&b_high.to_le_bytes());

        let elem_bits = elem_bytes * 8;
        let mask = if elem_bits == 64 {
            u64::MAX
        } else {
            (1u64 << elem_bits) - 1
        };
        let sx = |value: u64| -> i128 {
            if elem_bits == 64 {
                value as i64 as i128
            } else {
                let shift = 64 - elem_bits;
                ((value << shift) as i64 >> shift) as i128
            }
        };
        let smin = if elem_bits == 64 {
            i64::MIN as i128
        } else {
            -(1i128 << (elem_bits - 1))
        };
        let smax = if elem_bits == 64 {
            i64::MAX as i128
        } else {
            (1i128 << (elem_bits - 1)) - 1
        };

        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let av = read_lane(&a, off, elem_bytes);
            let bv = read_lane(&b, off, elem_bytes);
            let result = match op {
                VLaneOp::Add => av.wrapping_add(bv),
                VLaneOp::Sub => av.wrapping_sub(bv),
                VLaneOp::Mul => av.wrapping_mul(bv),
                VLaneOp::Min if signed => sx(av).min(sx(bv)) as u64,
                VLaneOp::Min => (av & mask).min(bv & mask),
                VLaneOp::Max if signed => sx(av).max(sx(bv)) as u64,
                VLaneOp::Max => (av & mask).max(bv & mask),
                VLaneOp::And => av & bv,
                VLaneOp::Or => av | bv,
                VLaneOp::Xor => av ^ bv,
                VLaneOp::AndNot => av & !bv,
                VLaneOp::OrNot => av | !bv,
                VLaneOp::Not => !av,
                VLaneOp::AddSat if signed => (sx(av) + sx(bv)).clamp(smin, smax) as u64,
                VLaneOp::AddSat => {
                    ((av & mask) as u128 + (bv & mask) as u128).min(mask as u128) as u64
                }
                VLaneOp::SubSat if signed => (sx(av) - sx(bv)).clamp(smin, smax) as u64,
                VLaneOp::SubSat => (av & mask).saturating_sub(bv & mask),
                VLaneOp::Avg if signed => ((sx(av) + sx(bv)) >> 1) as u64,
                VLaneOp::Avg => (((av & mask) as u128 + (bv & mask) as u128) >> 1) as u64,
                VLaneOp::AvgRnd if signed => ((sx(av) + sx(bv) + 1) >> 1) as u64,
                VLaneOp::AvgRnd => (((av & mask) as u128 + (bv & mask) as u128 + 1) >> 1) as u64,
                VLaneOp::Sign if bv & mask == 0 => 0,
                VLaneOp::Sign if sx(bv) < 0 => 0u64.wrapping_sub(av),
                VLaneOp::Sign => av,
                VLaneOp::AbsDiff if signed => (sx(av) - sx(bv)).unsigned_abs() as u64,
                VLaneOp::AbsDiff => {
                    let (x, y) = (av & mask, bv & mask);
                    if x >= y { x - y } else { y - x }
                }
            } & mask;
            out[off..off + elem_bytes].copy_from_slice(&result.to_le_bytes()[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let a_low = 0x807f_00ff_7f80_ff00;
    let a_high = 0x7fff_8000_ffff_0001;
    let b_low = 0x7f80_ff00_0080_00ff;
    let b_high = 0x8000_7fff_0001_ffff;
    let code = lower_ops(vec![
        OpKind::VLane {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
            op: VLaneOp::Max,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
            op: VLaneOp::Min,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
            op: VLaneOp::AddSat,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
            op: VLaneOp::SubSat,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(6),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
            op: VLaneOp::Avg,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(7),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 2,
            op: VLaneOp::AvgRnd,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(8),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 4,
            op: VLaneOp::AbsDiff,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(9),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I64,
            lanes: 1,
            op: VLaneOp::AndNot,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(10),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 4,
            op: VLaneOp::OrNot,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(11),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 4,
            op: VLaneOp::Mul,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(12),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I64,
            lanes: 2,
            op: VLaneOp::Not,
            signed: false,
            set_ovf: false,
        },
    ]);

    let (_, simd, _) =
        run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, a_low, a_high), (2, b_low, b_high)]);
    assert_eq!(
        simd[0],
        apply_vlane(a_low, a_high, b_low, b_high, 1, 16, VLaneOp::Max, true)
    );
    assert_eq!(
        simd[3],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 8, VLaneOp::Min, false)
    );
    assert_eq!(
        simd[4],
        apply_vlane(a_low, a_high, b_low, b_high, 1, 16, VLaneOp::AddSat, true)
    );
    assert_eq!(
        simd[5],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 8, VLaneOp::SubSat, false)
    );
    assert_eq!(
        simd[6],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 8, VLaneOp::Avg, true)
    );
    assert_eq!(
        simd[7],
        apply_vlane(a_low, a_high, b_low, b_high, 4, 2, VLaneOp::AvgRnd, false)
    );
    assert_eq!(
        simd[8],
        apply_vlane(a_low, a_high, b_low, b_high, 4, 4, VLaneOp::AbsDiff, true)
    );
    assert_eq!(
        simd[9],
        apply_vlane(a_low, a_high, b_low, b_high, 8, 1, VLaneOp::AndNot, false)
    );
    assert_eq!(
        simd[10],
        apply_vlane(a_low, a_high, b_low, b_high, 4, 4, VLaneOp::OrNot, false)
    );
    assert_eq!(
        simd[11],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 4, VLaneOp::Mul, true)
    );
    assert_eq!(
        simd[12],
        apply_vlane(a_low, a_high, b_low, b_high, 8, 2, VLaneOp::Not, false)
    );
}
#[test]
fn lowers_vlane_unary_clb_encodings() {
    let words = code_words(&lower_single_op(OpKind::VLaneUnary {
        dst: v(0),
        src: v(1),
        elem: VecElementType::I16,
        lanes: 8,
        op: 7,
        signed: false,
    }));

    let mut expected = vec![
        enc_simd_two_reg_misc(0, 1, 1, 0, 1, 0b00100),
        enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31),
    ];
    for lane in 0..8 {
        let imm5 = (lane << 2) | 2;
        expected.push(enc_simd_umov(16, 0, imm5, false));
        expected.push(enc_addsub_imm_regs(0, 0, 0, 0, 1, 16, 16));
        expected.push(enc_simd_ins_general(0, 16, imm5));
    }
    expected.push(enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31));
    expected.push(0xd65f_03c0);

    assert_eq!(words, expected);
}
#[test]
fn lowers_vlane_unary_integer_runtime() {
    fn apply_vlane_unary(
        src_low: u64,
        src_high: u64,
        elem_bytes: usize,
        lanes: usize,
        op: u8,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        let mut src = [0u8; 16];
        let mut out = [0u8; 16];
        src[..8].copy_from_slice(&src_low.to_le_bytes());
        src[8..].copy_from_slice(&src_high.to_le_bytes());

        let elem_bits = elem_bytes * 8;
        let mask = if elem_bits == 64 {
            u64::MAX
        } else {
            (1u64 << elem_bits) - 1
        };
        let sx = |value: u64| -> i128 {
            if elem_bits == 64 {
                value as i64 as i128
            } else {
                let shift = 64 - elem_bits;
                ((value << shift) as i64 >> shift) as i128
            }
        };
        let smax = if elem_bits == 64 {
            i64::MAX as i128
        } else {
            (1i128 << (elem_bits - 1)) - 1
        };

        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let av = read_lane(&src, off, elem_bytes);
            let result = match op {
                0 => !av,
                1 => sx(av).wrapping_abs() as u64,
                2 => sx(av).abs().min(smax) as u64,
                3 => ((av & mask) << (64 - elem_bits)).leading_zeros() as u64,
                4 => (av & mask).count_ones() as u64,
                5 => {
                    let v = (av & mask) << (64 - elem_bits);
                    let nv = (!av & mask) << (64 - elem_bits);
                    let n = v
                        .leading_zeros()
                        .min(elem_bits as u32)
                        .max(nv.leading_zeros().min(elem_bits as u32));
                    (n - 1) as u64
                }
                6 => sx(av).wrapping_neg() as u64,
                7 => {
                    let lj = (av & mask) << (64 - elem_bits);
                    let zeros = lj.leading_zeros().min(elem_bits as u32);
                    let ones = lj.leading_ones().min(elem_bits as u32);
                    zeros.max(ones) as u64
                }
                _ => av,
            } & mask;
            out[off..off + elem_bytes].copy_from_slice(&result.to_le_bytes()[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let src_low = 0x8001_7fff_00f0_8000;
    let src_high = 0xffff_0001_8000_7f00;
    let code = lower_ops(vec![
        OpKind::VLaneUnary {
            dst: v(0),
            src: v(1),
            elem: VecElementType::I32,
            lanes: 4,
            op: 0,
            signed: false,
        },
        OpKind::VLaneUnary {
            dst: v(2),
            src: v(1),
            elem: VecElementType::I8,
            lanes: 16,
            op: 1,
            signed: true,
        },
        OpKind::VLaneUnary {
            dst: v(3),
            src: v(1),
            elem: VecElementType::I16,
            lanes: 8,
            op: 2,
            signed: true,
        },
        OpKind::VLaneUnary {
            dst: v(4),
            src: v(1),
            elem: VecElementType::I32,
            lanes: 4,
            op: 3,
            signed: false,
        },
        OpKind::VLaneUnary {
            dst: v(5),
            src: v(1),
            elem: VecElementType::I8,
            lanes: 16,
            op: 4,
            signed: false,
        },
        OpKind::VLaneUnary {
            dst: v(6),
            src: v(1),
            elem: VecElementType::I64,
            lanes: 2,
            op: 6,
            signed: true,
        },
        OpKind::VLaneUnary {
            dst: v(7),
            src: v(1),
            elem: VecElementType::I16,
            lanes: 8,
            op: 5,
            signed: false,
        },
        OpKind::VLaneUnary {
            dst: v(8),
            src: v(1),
            elem: VecElementType::I32,
            lanes: 2,
            op: 5,
            signed: false,
        },
        OpKind::VLaneUnary {
            dst: v(9),
            src: v(1),
            elem: VecElementType::I16,
            lanes: 8,
            op: 7,
            signed: false,
        },
        OpKind::VLaneUnary {
            dst: v(10),
            src: v(1),
            elem: VecElementType::I32,
            lanes: 2,
            op: 7,
            signed: false,
        },
        OpKind::VLaneUnary {
            dst: v(11),
            src: v(1),
            elem: VecElementType::I8,
            lanes: 16,
            op: 7,
            signed: false,
        },
    ]);

    let (regs, simd, sp) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[(16, 0x1616_1616_1616_1616)],
        &[(1, src_low, src_high)],
    );
    assert_eq!(simd[0], apply_vlane_unary(src_low, src_high, 4, 4, 0));
    assert_eq!(simd[2], apply_vlane_unary(src_low, src_high, 1, 16, 1));
    assert_eq!(simd[3], apply_vlane_unary(src_low, src_high, 2, 8, 2));
    assert_eq!(simd[4], apply_vlane_unary(src_low, src_high, 4, 4, 3));
    assert_eq!(simd[5], apply_vlane_unary(src_low, src_high, 1, 16, 4));
    assert_eq!(simd[6], apply_vlane_unary(src_low, src_high, 8, 2, 6));
    assert_eq!(simd[7], apply_vlane_unary(src_low, src_high, 2, 8, 5));
    assert_eq!(simd[8], apply_vlane_unary(src_low, src_high, 4, 2, 5));
    assert_eq!(simd[9], apply_vlane_unary(src_low, src_high, 2, 8, 7));
    assert_eq!(simd[10], apply_vlane_unary(src_low, src_high, 4, 2, 7));
    assert_eq!(simd[11], apply_vlane_unary(src_low, src_high, 1, 16, 7));
    assert_eq!(regs[16], 0x1616_1616_1616_1616);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_vector_popcnt_byte_runtime() {
    fn byte_popcnt(pair: (u64, u64), width: VecWidth) -> (u64, u64) {
        let mut bytes = simd_pair_bytes(pair);
        let count = width.bytes() as usize;
        for byte in bytes[..count].iter_mut() {
            *byte = byte.count_ones() as u8;
        }
        for byte in bytes[count..].iter_mut() {
            *byte = 0;
        }
        simd_pair_from_bytes(bytes)
    }

    let src128 = (0xfedc_ba98_7654_3210, 0x0123_4567_89ab_cdef);
    let src64 = (0x8081_7f7e_0001_fffe, 0xeeee_dddd_cccc_bbbb);
    let code = lower_ops(vec![
        OpKind::VPopcnt {
            dst: v(0),
            src: v(1),
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V128,
            zeroing: false,
        },
        OpKind::VPopcnt {
            dst: v(2),
            src: v(3),
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V64,
            zeroing: false,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[(1, src128.0, src128.1), (3, src64.0, src64.1)],
    );
    assert_eq!(simd[0], byte_popcnt(src128, VecWidth::V128));
    assert_eq!(simd[2], byte_popcnt(src64, VecWidth::V64));
}
#[test]
fn lowers_vector_broadcast_runtime() {
    fn splat(scalar: u64, elem_bytes: usize, lanes: usize) -> (u64, u64) {
        let mut out = [0u8; 16];
        let bytes = scalar.to_le_bytes();
        for lane in 0..lanes {
            let off = lane * elem_bytes;
            out[off..off + elem_bytes].copy_from_slice(&bytes[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let scalar = 0x8877_6655_4433_2211;
    let code = lower_ops(vec![
        OpKind::VBroadcast {
            dst: v(0),
            scalar: x(1),
            elem: VecElementType::I8,
            lanes: 16,
        },
        OpKind::VBroadcast {
            dst: v(2),
            scalar: x(1),
            elem: VecElementType::I16,
            lanes: 8,
        },
        OpKind::VBroadcast {
            dst: v(3),
            scalar: x(1),
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VBroadcast {
            dst: v(4),
            scalar: x(1),
            elem: VecElementType::I64,
            lanes: 2,
        },
        OpKind::VBroadcast {
            dst: v(5),
            scalar: x(1),
            elem: VecElementType::I32,
            lanes: 2,
        },
        OpKind::VBroadcast {
            dst: v(6),
            scalar: VReg::Imm(0),
            elem: VecElementType::I16,
            lanes: 4,
        },
        OpKind::VBroadcast {
            dst: v(7),
            scalar: x(1),
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VBroadcast {
            dst: v(8),
            scalar: x(1),
            elem: VecElementType::F32,
            lanes: 2,
        },
        OpKind::VBroadcast {
            dst: v(9),
            scalar: x(1),
            elem: VecElementType::F64,
            lanes: 2,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(&code, &[(1, scalar)], &[]);
    assert_eq!(simd[0], splat(scalar, 1, 16));
    assert_eq!(simd[2], splat(scalar, 2, 8));
    assert_eq!(simd[3], splat(scalar, 4, 4));
    assert_eq!(simd[4], splat(scalar, 8, 2));
    assert_eq!(simd[5], splat(scalar, 4, 2));
    assert_eq!(simd[6], (0, 0));
    assert_eq!(simd[7], splat(scalar, 4, 4));
    assert_eq!(simd[8], splat(scalar, 4, 2));
    assert_eq!(simd[9], splat(scalar, 8, 2));
}
#[test]
fn lowers_vector_shift_immediate_runtime() {
    fn apply_shift(
        src_low: u64,
        src_high: u64,
        elem_bytes: usize,
        lanes: usize,
        amount: i64,
        shift: ShiftOp,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        let mut src = [0u8; 16];
        let mut out = [0u8; 16];
        src[..8].copy_from_slice(&src_low.to_le_bytes());
        src[8..].copy_from_slice(&src_high.to_le_bytes());

        let elem_bits = elem_bytes * 8;
        let mask = if elem_bits == 64 {
            u64::MAX
        } else {
            (1u64 << elem_bits) - 1
        };
        let amount = (amount as u32) % elem_bits as u32;
        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let value = read_lane(&src, off, elem_bytes);
            let shifted = match shift {
                ShiftOp::Lsl => (value << amount) & mask,
                ShiftOp::Lsr => (value >> amount) & mask,
                ShiftOp::Asr => {
                    let signed = if elem_bits == 64 {
                        value as i64
                    } else {
                        let sh = 64 - elem_bits;
                        ((value << sh) as i64) >> sh
                    };
                    ((signed >> amount) as u64) & mask
                }
                _ => value & mask,
            };
            out[off..off + elem_bytes].copy_from_slice(&shifted.to_le_bytes()[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let src_low = 0xf080_7f01_8000_00ff;
    let src_high = 0x8000_0001_7fff_ff00;
    let code = lower_ops(vec![
        OpKind::VShift {
            dst: v(0),
            src: v(1),
            amount: SrcOperand::Imm(3),
            shift: ShiftOp::Lsl,
            elem: VecElementType::I8,
            lanes: 16,
        },
        OpKind::VShift {
            dst: v(2),
            src: v(1),
            amount: SrcOperand::Imm(5),
            shift: ShiftOp::Lsr,
            elem: VecElementType::I16,
            lanes: 8,
        },
        OpKind::VShift {
            dst: v(3),
            src: v(1),
            amount: SrcOperand::Imm(4),
            shift: ShiftOp::Asr,
            elem: VecElementType::I16,
            lanes: 8,
        },
        OpKind::VShift {
            dst: v(4),
            src: v(1),
            amount: SrcOperand::Imm(35),
            shift: ShiftOp::Lsl,
            elem: VecElementType::I32,
            lanes: 2,
        },
        OpKind::VShift {
            dst: v(5),
            src: v(1),
            amount: SrcOperand::Imm(68),
            shift: ShiftOp::Lsr,
            elem: VecElementType::I64,
            lanes: 2,
        },
        OpKind::VShift {
            dst: v(6),
            src: v(1),
            amount: SrcOperand::Imm(-1),
            shift: ShiftOp::Asr,
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VShift {
            dst: v(7),
            src: v(1),
            amount: SrcOperand::Imm(32),
            shift: ShiftOp::Lsr,
            elem: VecElementType::I32,
            lanes: 4,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, src_low, src_high)]);
    assert_eq!(
        simd[0],
        apply_shift(src_low, src_high, 1, 16, 3, ShiftOp::Lsl)
    );
    assert_eq!(
        simd[2],
        apply_shift(src_low, src_high, 2, 8, 5, ShiftOp::Lsr)
    );
    assert_eq!(
        simd[3],
        apply_shift(src_low, src_high, 2, 8, 4, ShiftOp::Asr)
    );
    assert_eq!(
        simd[4],
        apply_shift(src_low, src_high, 4, 2, 35, ShiftOp::Lsl)
    );
    assert_eq!(
        simd[5],
        apply_shift(src_low, src_high, 8, 2, 68, ShiftOp::Lsr)
    );
    assert_eq!(
        simd[6],
        apply_shift(src_low, src_high, 4, 4, -1, ShiftOp::Asr)
    );
    assert_eq!(
        simd[7],
        apply_shift(src_low, src_high, 4, 4, 32, ShiftOp::Lsr)
    );
}
#[test]
fn lowers_vector_shift_acc_runtime() {
    fn apply_shift_acc(
        dst_low: u64,
        dst_high: u64,
        src_low: u64,
        src_high: u64,
        elem_bytes: usize,
        lanes: usize,
        amount: i64,
        shift: ShiftOp,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        let mut dst = [0u8; 16];
        let mut src = [0u8; 16];
        dst[..8].copy_from_slice(&dst_low.to_le_bytes());
        dst[8..].copy_from_slice(&dst_high.to_le_bytes());
        src[..8].copy_from_slice(&src_low.to_le_bytes());
        src[8..].copy_from_slice(&src_high.to_le_bytes());

        let elem_bits = elem_bytes * 8;
        let mask = if elem_bits == 64 {
            u64::MAX
        } else {
            (1u64 << elem_bits) - 1
        };
        let amount = (amount as u32) % elem_bits as u32;
        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let value = read_lane(&src, off, elem_bytes);
            let shifted = match shift {
                ShiftOp::Lsr => (value >> amount) & mask,
                ShiftOp::Asr => {
                    let signed = if elem_bits == 64 {
                        value as i64
                    } else {
                        let sh = 64 - elem_bits;
                        ((value << sh) as i64) >> sh
                    };
                    ((signed >> amount) as u64) & mask
                }
                _ => value & mask,
            };
            let prev = read_lane(&dst, off, elem_bytes);
            let result = prev.wrapping_add(shifted) & mask;
            dst[off..off + elem_bytes].copy_from_slice(&result.to_le_bytes()[..elem_bytes]);
        }
        for byte in dst[lanes * elem_bytes..].iter_mut() {
            *byte = 0;
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&dst[..8]);
        high.copy_from_slice(&dst[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let dst_low = 0x0102_0304_0506_0708;
    let dst_high = 0x8081_7f7e_0001_fffe;
    let src_low = 0xf080_7f01_8000_00ff;
    let src_high = 0x8000_0001_7fff_ff00;
    let code = lower_ops(vec![
        OpKind::VShiftAcc {
            dst: v(0),
            src: v(1),
            amount: SrcOperand::Imm(3),
            shift: ShiftOp::Lsr,
            elem: VecElementType::I8,
            lanes: 16,
        },
        OpKind::VShiftAcc {
            dst: v(2),
            src: v(1),
            amount: SrcOperand::Imm(4),
            shift: ShiftOp::Asr,
            elem: VecElementType::I16,
            lanes: 8,
        },
        OpKind::VShiftAcc {
            dst: v(3),
            src: v(1),
            amount: SrcOperand::Imm(32),
            shift: ShiftOp::Lsr,
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VShiftAcc {
            dst: v(4),
            src: v(1),
            amount: SrcOperand::Imm(-1),
            shift: ShiftOp::Asr,
            elem: VecElementType::I64,
            lanes: 2,
        },
        OpKind::VShiftAcc {
            dst: v(5),
            src: v(1),
            amount: SrcOperand::Imm(4),
            shift: ShiftOp::Lsr,
            elem: VecElementType::I32,
            lanes: 2,
        },
        OpKind::VShiftAcc {
            dst: v(6),
            src: v(1),
            amount: SrcOperand::Imm(3),
            shift: ShiftOp::Asr,
            elem: VecElementType::I16,
            lanes: 4,
        },
    ]);

    let (_, simd, _) = run_aarch64_code_with_regs_and_simd(
        &code,
        &[],
        &[
            (0, dst_low, dst_high),
            (1, src_low, src_high),
            (2, dst_low, dst_high),
            (3, dst_low, dst_high),
            (4, dst_low, dst_high),
            (5, dst_low, dst_high),
            (6, dst_low, dst_high),
        ],
    );
    assert_eq!(
        simd[0],
        apply_shift_acc(dst_low, dst_high, src_low, src_high, 1, 16, 3, ShiftOp::Lsr)
    );
    assert_eq!(
        simd[2],
        apply_shift_acc(dst_low, dst_high, src_low, src_high, 2, 8, 4, ShiftOp::Asr)
    );
    assert_eq!(
        simd[3],
        apply_shift_acc(dst_low, dst_high, src_low, src_high, 4, 4, 32, ShiftOp::Lsr)
    );
    assert_eq!(
        simd[4],
        apply_shift_acc(dst_low, dst_high, src_low, src_high, 8, 2, -1, ShiftOp::Asr)
    );
    assert_eq!(
        simd[5],
        apply_shift_acc(dst_low, dst_high, src_low, src_high, 4, 2, 4, ShiftOp::Lsr)
    );
    assert_eq!(
        simd[6],
        apply_shift_acc(dst_low, dst_high, src_low, src_high, 2, 4, 3, ShiftOp::Asr)
    );
}
#[test]
fn lowers_vector_load_store_runtime() {
    fn le_u64(bytes: &[u8], offset: usize) -> u64 {
        let mut word = [0u8; 8];
        word.copy_from_slice(&bytes[offset..offset + 8]);
        u64::from_le_bytes(word)
    }

    let mem_addr = 0x400;
    let mut mem = [0u8; 128];
    for (idx, byte) in mem.iter_mut().enumerate() {
        *byte = (idx as u8).wrapping_mul(3).wrapping_add(1);
    }
    let v7_low: u64 = 0x1122_3344_5566_7788;
    let v7_high: u64 = 0x99aa_bbcc_ddee_ff00;
    let mut v7_bytes = [0u8; 16];
    v7_bytes[..8].copy_from_slice(&v7_low.to_le_bytes());
    v7_bytes[8..].copy_from_slice(&v7_high.to_le_bytes());

    let code = lower_ops(vec![
        OpKind::VLoad {
            dst: v(0),
            addr: Address::Direct(x(1)),
            width: VecWidth::V128,
        },
        OpKind::VLoad {
            dst: v(2),
            addr: Address::base_off(x(1), 16),
            width: VecWidth::V64,
        },
        OpKind::VLoad {
            dst: v(4),
            addr: Address::base_off(x(2), -16),
            width: VecWidth::V128,
        },
        OpKind::VStore {
            src: v(0),
            addr: Address::base_off(x(1), 48),
            width: VecWidth::V128,
        },
        OpKind::VStore {
            src: v(2),
            addr: Address::base_off(x(1), 64),
            width: VecWidth::V64,
        },
        OpKind::VStore {
            src: v(7),
            addr: Address::sib(Some(x(1)), x(3), 8, 80),
            width: VecWidth::V128,
        },
    ]);

    let (_, simd, out_mem) = run_aarch64_code_with_regs_simd_and_memory(
        &code,
        &[(1, mem_addr), (2, mem_addr + 32), (3, 2)],
        &[(7, v7_low, v7_high)],
        &[(mem_addr, &mem)],
        mem_addr,
        mem.len(),
    );

    assert_eq!(simd[0], (le_u64(&mem, 0), le_u64(&mem, 8)));
    assert_eq!(simd[2], (le_u64(&mem, 16), 0));
    assert_eq!(simd[4], (le_u64(&mem, 16), le_u64(&mem, 24)));
    assert_eq!(&out_mem[48..64], &mem[0..16]);
    assert_eq!(&out_mem[64..72], &mem[16..24]);
    assert_eq!(&out_mem[72..80], &mem[72..80]);
    assert_eq!(&out_mem[96..112], &v7_bytes);
}
#[test]
fn lowers_vector_memory_apx_egpr_address_operands_runtime() {
    fn le_u64(bytes: &[u8], offset: usize) -> u64 {
        let mut word = [0u8; 8];
        word.copy_from_slice(&bytes[offset..offset + 8]);
        u64::from_le_bytes(word)
    }

    let mem_addr = 0x500;
    let mut mem = [0u8; 160];
    for (idx, byte) in mem.iter_mut().enumerate() {
        *byte = (idx as u8).wrapping_mul(5).wrapping_add(7);
    }
    let v5_low: u64 = 0x0102_0304_0506_0708;
    let v5_high: u64 = 0x1112_1314_1516_1718;
    let mut v5_bytes = [0u8; 16];
    v5_bytes[..8].copy_from_slice(&v5_low.to_le_bytes());
    v5_bytes[8..].copy_from_slice(&v5_high.to_le_bytes());

    let code = lower_ops(vec![
        OpKind::VLoad {
            dst: v(1),
            addr: Address::Direct(x86(X86Reg::R16)),
            width: VecWidth::V128,
        },
        OpKind::VLoad {
            dst: v(3),
            addr: Address::base_off(x86(X86Reg::R17), -16),
            width: VecWidth::V64,
        },
        OpKind::VStore {
            src: v(3),
            addr: Address::base_off(x86(X86Reg::R16), 64),
            width: VecWidth::V64,
        },
        OpKind::VStore {
            src: v(5),
            addr: Address::sib(Some(x86(X86Reg::R16)), x86(X86Reg::R18), 8, 80),
            width: VecWidth::V128,
        },
    ]);

    let (_, simd, out_mem) = run_aarch64_code_with_regs_simd_and_memory(
        &code,
        &[(16, mem_addr), (17, mem_addr + 48), (18, 2)],
        &[(5, v5_low, v5_high)],
        &[(mem_addr, &mem)],
        mem_addr,
        mem.len(),
    );

    assert_eq!(simd[1], (le_u64(&mem, 0), le_u64(&mem, 8)));
    assert_eq!(simd[3], (le_u64(&mem, 32), 0));
    assert_eq!(&out_mem[64..72], &mem[32..40]);
    assert_eq!(&out_mem[72..80], &mem[72..80]);
    assert_eq!(&out_mem[96..112], &v5_bytes);
}
#[test]
fn rejects_vector_memory_apx_r31_address_mapping() {
    let err = try_lower_single_op(OpKind::VLoad {
        dst: v(0),
        addr: Address::Direct(x86(X86Reg::R31)),
        width: VecWidth::V128,
    })
    .unwrap_err();
    assert!(matches!(err, LowerError::InvalidRegister(_)));
}
#[test]
fn rejects_vector_unsupported_widths() {
    fn assert_unsupported(kind: OpKind) {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(0, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        let err = lowerer.lower_function(&func).unwrap_err();
        assert!(matches!(err, LowerError::UnsupportedOp { .. }));
    }

    assert_unsupported(OpKind::VLoad {
        dst: v(0),
        addr: Address::Direct(x(1)),
        width: VecWidth::V256,
    });

    assert_unsupported(OpKind::VXor {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        width: VecWidth::V512,
    });

    assert_unsupported(OpKind::VAdd {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I32,
        lanes: 8,
    });

    assert_unsupported(OpKind::VAdd {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::F64,
        lanes: 1,
    });

    assert_unsupported(OpKind::VSub {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I64,
        lanes: 1,
    });

    assert_unsupported(OpKind::VMul {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I64,
        lanes: 2,
    });

    assert_unsupported(OpKind::VMax {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I64,
        lanes: 2,
    });

    assert_unsupported(OpKind::VMax {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::F64,
        lanes: 1,
    });

    assert_unsupported(OpKind::VMax {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I32,
        lanes: 8,
    });

    assert_unsupported(OpKind::VMin {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I64,
        lanes: 2,
        signed: true,
    });

    assert_unsupported(OpKind::VMin {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::F64,
        lanes: 1,
        signed: false,
    });

    assert_unsupported(OpKind::VMin {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I32,
        lanes: 8,
        signed: false,
    });

    assert_unsupported(OpKind::VFma {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        acc: v(3),
        elem: VecElementType::F64,
        lanes: 1,
        negate_product: false,
        negate_acc: false,
    });

    assert_unsupported(OpKind::VFma {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        acc: v(3),
        elem: VecElementType::F32,
        lanes: 4,
        negate_product: false,
        negate_acc: true,
    });

    assert_unsupported(OpKind::VDotProduct {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V256,
        src1_unsigned: true,
        saturate: false,
        zeroing: false,
    });

    assert_unsupported(OpKind::VDotProduct {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        src_elem: VecElementType::I16,
        acc_elem: VecElementType::I32,
        width: VecWidth::V128,
        src1_unsigned: false,
        saturate: false,
        zeroing: false,
    });

    assert_unsupported(OpKind::VDotProduct {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I16,
        width: VecWidth::V128,
        src1_unsigned: true,
        saturate: false,
        zeroing: false,
    });

    assert_unsupported(OpKind::VDotProduct {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V128,
        src1_unsigned: true,
        saturate: true,
        zeroing: false,
    });

    assert_unsupported(OpKind::VDotProduct {
        dst: v(1),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V128,
        src1_unsigned: true,
        saturate: false,
        zeroing: false,
    });

    assert_unsupported(OpKind::VShuffleBitQM {
        dst: x86(X86Reg::K(1)),
        src: v(1),
        indices: v(2),
        mask: None,
        width: VecWidth::V128,
    });

    assert_unsupported(OpKind::VDotProductExt {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V256,
        src1_signed: true,
        src2_signed: true,
        saturate: false,
    });

    assert_unsupported(OpKind::VDotProductExt {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        src_elem: VecElementType::I16,
        acc_elem: VecElementType::I32,
        width: VecWidth::V128,
        src1_signed: true,
        src2_signed: false,
        saturate: false,
    });

    assert_unsupported(OpKind::VDotProductExt {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V128,
        src1_signed: true,
        src2_signed: true,
        saturate: true,
    });

    assert_unsupported(OpKind::VDotProductExt {
        dst: v(1),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V128,
        src1_signed: false,
        src2_signed: false,
        saturate: false,
    });

    assert_unsupported(OpKind::VDotProductBF16 {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        width: VecWidth::V256,
        zeroing: false,
    });

    assert_unsupported(OpKind::VDotProductBF16 {
        dst: v(1),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        width: VecWidth::V128,
        zeroing: false,
    });

    assert_unsupported(OpKind::VFma {
        dst: v(1),
        src1: v(1),
        src2: v(2),
        acc: v(3),
        elem: VecElementType::F32,
        lanes: 4,
        negate_product: false,
        negate_acc: false,
    });

    assert_unsupported(OpKind::VFP16Arith {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        op: Avx10FP16Op::Add,
        round: FpRoundMode::Dynamic,
        width: VecWidth::V256,
        lanes: 16,
        zeroing: false,
    });

    assert_unsupported(OpKind::VFP16Arith {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        op: Avx10FP16Op::Min,
        round: FpRoundMode::Dynamic,
        width: VecWidth::V128,
        lanes: 8,
        zeroing: false,
    });

    assert_unsupported(OpKind::VFP16Arith {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        op: Avx10FP16Op::Add,
        round: FpRoundMode::Dynamic,
        width: VecWidth::V128,
        lanes: 1,
        zeroing: false,
    });

    assert_unsupported(OpKind::VCvtFP32ToBF16 {
        dst: v(0),
        src1: v(1),
        src2: None,
        mask: None,
        width: VecWidth::V64,
        zeroing: false,
    });

    assert_unsupported(OpKind::VCvtFP32ToBF16 {
        dst: v(0),
        src1: v(1),
        src2: None,
        mask: None,
        width: VecWidth::V256,
        zeroing: false,
    });

    assert_unsupported(OpKind::VCvtFP32ToBF16 {
        dst: v(1),
        src1: v(1),
        src2: Some(v(2)),
        mask: None,
        width: VecWidth::V128,
        zeroing: false,
    });

    assert_unsupported(OpKind::VCvtBF16ToFP32 {
        dst: v(0),
        src: v(1),
        width: VecWidth::V64,
    });

    assert_unsupported(OpKind::VCvtBF16ToFP32 {
        dst: v(0),
        src: v(1),
        width: VecWidth::V256,
    });

    assert_unsupported(OpKind::VCvtFpToIntSat {
        dst: v(0),
        src: v(1),
        mask: None,
        fp_elem: X86SatFpFormat::F32,
        int_elem: VecElementType::I8,
        width: VecWidth::V256,
        signed: true,
        truncate: true,
        round: FpRoundMode::RoundTowardZero,
        zeroing: false,
        suppress_exceptions: false,
    });

    assert_unsupported(OpKind::VCvtFpToIntSat {
        dst: v(0),
        src: v(1),
        mask: None,
        fp_elem: X86SatFpFormat::F64,
        int_elem: VecElementType::I64,
        width: VecWidth::V64,
        signed: false,
        truncate: true,
        round: FpRoundMode::RoundTowardZero,
        zeroing: false,
        suppress_exceptions: false,
    });

    assert_unsupported(OpKind::VCvtFpToIntSat {
        dst: v(0),
        src: v(1),
        mask: None,
        fp_elem: X86SatFpFormat::F64,
        int_elem: VecElementType::I8,
        width: VecWidth::V128,
        signed: true,
        truncate: true,
        round: FpRoundMode::RoundTowardZero,
        zeroing: false,
        suppress_exceptions: false,
    });

    assert_unsupported(OpKind::VPopcnt {
        dst: v(0),
        src: v(1),
        mask: None,
        elem: VecElementType::I16,
        width: VecWidth::V128,
        zeroing: false,
    });

    assert_unsupported(OpKind::VPopcnt {
        dst: v(0),
        src: v(1),
        mask: None,
        elem: VecElementType::I8,
        width: VecWidth::V256,
        zeroing: false,
    });

    assert_unsupported(OpKind::VMultiplyAdd52 {
        dst: v(0),
        acc: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        width: VecWidth::V256,
        high: false,
        zeroing: false,
    });

    assert_unsupported(OpKind::VMpsadbw {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        mask: None,
        width: VecWidth::V256,
        imm: 0,
        zeroing: false,
    });
    assert_unsupported(OpKind::VMpsadbw {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
        width: VecWidth::V128,
        imm: 0,
        zeroing: false,
    });
    assert_unsupported(OpKind::VMpsadbw {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
        width: VecWidth::V128,
        imm: 0,
        zeroing: true,
    });

    assert_unsupported(OpKind::VMinMax {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::F32,
        width: VecWidth::V256,
        imm: 0,
    });

    assert_unsupported(OpKind::VMinMax {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I32,
        width: VecWidth::V128,
        imm: 0,
    });

    assert_unsupported(OpKind::VPermute {
        dst: v(0),
        src1: v(1),
        src2: None,
        indices: v(2),
        elem: VecElementType::I16,
        width: VecWidth::V128,
        overwrite_table: false,
    });

    assert_unsupported(OpKind::VPermute {
        dst: v(0),
        src1: v(1),
        src2: None,
        indices: v(2),
        elem: VecElementType::I8,
        width: VecWidth::V256,
        overwrite_table: false,
    });

    assert_unsupported(OpKind::VPermute {
        dst: v(0),
        src1: v(1),
        src2: None,
        indices: v(2),
        elem: VecElementType::I8,
        width: VecWidth::V128,
        overwrite_table: true,
    });

    assert_unsupported(OpKind::VLane {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I64,
        lanes: 2,
        op: VLaneOp::Min,
        signed: true,
        set_ovf: false,
    });

    assert_unsupported(OpKind::VLane {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::F32,
        lanes: 4,
        op: VLaneOp::Avg,
        signed: false,
        set_ovf: false,
    });

    assert_unsupported(OpKind::VLane {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I32,
        lanes: 8,
        op: VLaneOp::And,
        signed: false,
        set_ovf: false,
    });

    assert_unsupported(OpKind::VLane {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I32,
        lanes: 4,
        op: VLaneOp::AddSat,
        signed: false,
        set_ovf: true,
    });

    assert_unsupported(OpKind::VNavg {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I64,
        lanes: 2,
        signed: true,
    });

    assert_unsupported(OpKind::VNavg {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::F32,
        lanes: 4,
        signed: true,
    });

    assert_unsupported(OpKind::VNavg {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I32,
        lanes: 8,
        signed: false,
    });

    assert_unsupported(OpKind::VLaneUnary {
        dst: v(0),
        src: v(1),
        elem: VecElementType::I64,
        lanes: 2,
        op: 3,
        signed: false,
    });

    assert_unsupported(OpKind::VLaneUnary {
        dst: v(0),
        src: v(1),
        elem: VecElementType::I16,
        lanes: 8,
        op: 4,
        signed: false,
    });

    assert_unsupported(OpKind::VLaneUnary {
        dst: v(0),
        src: v(1),
        elem: VecElementType::I64,
        lanes: 2,
        op: 5,
        signed: false,
    });

    assert_unsupported(OpKind::VLaneUnary {
        dst: v(0),
        src: v(1),
        elem: VecElementType::I64,
        lanes: 2,
        op: 7,
        signed: false,
    });

    assert_unsupported(OpKind::VLaneUnary {
        dst: v(0),
        src: v(1),
        elem: VecElementType::I32,
        lanes: 8,
        op: 0,
        signed: false,
    });

    assert_unsupported(OpKind::VLaneUnary {
        dst: v(0),
        src: v(1),
        elem: VecElementType::F32,
        lanes: 4,
        op: 1,
        signed: true,
    });

    assert_unsupported(OpKind::VBroadcast {
        dst: v(0),
        scalar: x(1),
        elem: VecElementType::I32,
        lanes: 32,
    });

    assert_unsupported(OpKind::VBroadcast {
        dst: v(0),
        scalar: x(1),
        elem: VecElementType::F64,
        lanes: 1,
    });

    assert_unsupported(OpKind::VShift {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Reg(x(2)),
        shift: ShiftOp::Lsl,
        elem: VecElementType::I32,
        lanes: 4,
    });

    assert_unsupported(OpKind::VShift {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Imm(1),
        shift: ShiftOp::Lsr,
        elem: VecElementType::F32,
        lanes: 4,
    });

    assert_unsupported(OpKind::VShift {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Imm(1),
        shift: ShiftOp::Asr,
        elem: VecElementType::I32,
        lanes: 8,
    });

    assert_unsupported(OpKind::VShift {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Imm(1),
        shift: ShiftOp::Ror,
        elem: VecElementType::I32,
        lanes: 4,
    });

    assert_unsupported(OpKind::VShiftAcc {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Imm(1),
        shift: ShiftOp::Lsl,
        elem: VecElementType::I16,
        lanes: 8,
    });

    assert_unsupported(OpKind::VShiftAcc {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Reg(x(1)),
        shift: ShiftOp::Lsr,
        elem: VecElementType::I16,
        lanes: 8,
    });

    assert_unsupported(OpKind::VShiftAcc {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Imm(1),
        shift: ShiftOp::Lsr,
        elem: VecElementType::I64,
        lanes: 1,
    });

    assert_unsupported(OpKind::VShiftAcc {
        dst: v(0),
        src: v(1),
        amount: SrcOperand::Imm(1),
        shift: ShiftOp::Asr,
        elem: VecElementType::F32,
        lanes: 4,
    });
}
#[test]
fn fp_trampoline_detection_still_includes_vector_regs() {
    let func = func_with_ops(vec![OpKind::Mov {
        dst: v(0),
        src: SrcOperand::Reg(v(1)),
        width: OpWidth::W128,
    }]);
    assert!(uses_aarch64_fp_trampoline(&func));
}
