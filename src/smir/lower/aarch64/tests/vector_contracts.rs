//! Native vector width and operation rejection contracts.

use crate::smir::ir::ops::{OpKind, X86SatFpFormat};
use crate::smir::ir::types::{
    Address, ArchReg, Avx10FP16Op, FpRoundMode, FunctionId, ShiftOp, SrcOperand, VLaneOp, VReg,
    VecElementType, VecWidth, X86Reg,
};
use crate::smir::ir::{FunctionBuilder, Terminator};
use crate::smir::lower::aarch64::Aarch64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};

use super::super::{v, x, x86};

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
        lanes: 3,
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
