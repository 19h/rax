//! tests::misc tests

use super::*;
use crate::smir::lower::x86_64::*;

#[test]
fn production_lowerer_routes_avx10_ops_and_propagates_shape_errors() {
    let zmm1 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(1)));
    let zmm2 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)));
    let zmm3 = VReg::Arch(ArchReg::X86(X86Reg::Zmm(3)));
    let bytes = lower_single_op(OpKind::VDotProduct {
        dst: zmm1,
        acc: zmm1,
        src1: zmm2,
        src2: zmm3,
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V512,
        src1_unsigned: true,
        saturate: false,
        zeroing: false,
    });
    assert!(
        bytes
            .windows(6)
            .any(|window| window == [0x62, 0xF2, 0x6D, 0x48, 0x50, 0xCB]),
        "production lowering omitted VPDPBUSD: {bytes:02X?}"
    );

    let ternary = lower_single_op(OpKind::X86TernaryLogic {
        dst: zmm1,
        src1: zmm1,
        src2: zmm2,
        src3: zmm3,
        mask: None,
        imm: 0x96,
        width: VecWidth::V512,
        elem: VecElementType::I32,
        zeroing: false,
    });
    assert!(
        ternary
            .windows(7)
            .any(|window| window == [0x62, 0xF3, 0x6D, 0x48, 0x25, 0xCB, 0x96]),
        "production lowering omitted VPTERNLOGD: {ternary:02X?}"
    );

    let error = lower_single_op_err(OpKind::VDotProduct {
        dst: zmm1,
        acc: zmm2,
        src1: zmm2,
        src2: zmm3,
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V512,
        src1_unsigned: true,
        saturate: false,
        zeroing: false,
    });
    assert!(
        matches!(error, LowerError::UnsupportedOperation(message) if message.contains("accumulator aliased with dst"))
    );
}
#[test]
fn lower_fixed_integer_compare_rejects_unhinted_and_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));

    let unhinted = OpKind::VCmp {
        dst: xmm(1),
        src1: xmm(1),
        src2: xmm(2),
        cond: VecCmpCond::Eq,
        elem: VecElementType::I32,
        lanes: 4,
    };
    assert!(matches!(
        lower_single_op_err(unhinted),
        LowerError::UnsupportedOp { .. }
    ));

    for (kind, hint) in [
        (
            OpKind::VCmp {
                dst: xmm(1),
                src1: xmm(2),
                src2: xmm(3),
                cond: VecCmpCond::Eq,
                elem: VecElementType::I8,
                lanes: 16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x74,
            },
        ),
        (
            OpKind::VCmp {
                dst: xmm(1),
                src1: xmm(1),
                src2: xmm(2),
                cond: VecCmpCond::Gt,
                elem: VecElementType::I16,
                lanes: 8,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x64,
            },
        ),
        (
            OpKind::VCmp {
                dst: ymm(1),
                src1: ymm(2),
                src2: ymm(3),
                cond: VecCmpCond::Eq,
                elem: VecElementType::I64,
                lanes: 4,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x29,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            OpKind::VCmp {
                dst: xmm(16),
                src1: xmm(17),
                src2: xmm(18),
                cond: VecCmpCond::Gt,
                elem: VecElementType::I32,
                lanes: 4,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x66,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            OpKind::VCmp {
                dst: ymm(1),
                src1: ymm(2),
                src2: ymm(3),
                cond: VecCmpCond::Eq,
                elem: VecElementType::I16,
                lanes: 16,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x75,
                width: VecWidth::V256,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_integer_interleave_rejects_unhinted_and_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));

    let unhinted = OpKind::VInterleave {
        dst: xmm(1),
        src1: xmm(1),
        src2: xmm(2),
        elem: VecElementType::I8,
        lanes: 16,
        block_lanes: 16,
        high: false,
    };
    assert!(matches!(
        lower_single_op_err(unhinted),
        LowerError::UnsupportedOp { .. }
    ));

    for (kind, hint) in [
        (
            OpKind::VInterleave {
                dst: xmm(1),
                src1: xmm(1),
                src2: xmm(2),
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 8,
                high: false,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x60,
            },
        ),
        (
            OpKind::VInterleave {
                dst: xmm(1),
                src1: xmm(2),
                src2: xmm(3),
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 16,
                high: false,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x60,
            },
        ),
        (
            OpKind::VInterleave {
                dst: xmm(1),
                src1: xmm(1),
                src2: xmm(2),
                elem: VecElementType::I16,
                lanes: 8,
                block_lanes: 8,
                high: true,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x68,
            },
        ),
        (
            OpKind::VInterleave {
                dst: ymm(16),
                src1: ymm(17),
                src2: ymm(18),
                elem: VecElementType::I32,
                lanes: 8,
                block_lanes: 4,
                high: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x62,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            OpKind::VInterleave {
                dst: zmm(16),
                src1: zmm(17),
                src2: zmm(18),
                elem: VecElementType::I32,
                lanes: 16,
                block_lanes: 4,
                high: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x62,
                width: VecWidth::V512,
                w: true,
            },
        ),
        (
            OpKind::VInterleave {
                dst: zmm(16),
                src1: zmm(17),
                src2: zmm(18),
                elem: VecElementType::I64,
                lanes: 8,
                block_lanes: 2,
                high: true,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6D,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_saturating_pack_rejects_unhinted_and_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));
    let pack = |dst, src1, src2, src_elem, to_unsigned, src_lanes, block_lanes| OpKind::VPackSat {
        dst,
        src1,
        src2,
        src_elem,
        to_unsigned,
        src_lanes,
        block_lanes,
    };

    let unhinted = pack(xmm(1), xmm(2), xmm(1), VecElementType::I16, false, 8, 8);
    assert!(matches!(
        lower_single_op_err(unhinted),
        LowerError::UnsupportedOp { .. }
    ));

    for (kind, hint) in [
        (
            pack(xmm(1), xmm(2), xmm(1), VecElementType::I16, false, 8, 4),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x63,
            },
        ),
        (
            pack(xmm(1), xmm(2), xmm(3), VecElementType::I16, false, 8, 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x63,
            },
        ),
        (
            pack(xmm(1), xmm(2), xmm(1), VecElementType::I16, false, 8, 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x67,
            },
        ),
        (
            pack(ymm(1), ymm(3), ymm(2), VecElementType::I32, true, 8, 4),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x2B,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            pack(ymm(16), ymm(18), ymm(17), VecElementType::I16, true, 16, 8),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x67,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            pack(zmm(16), zmm(18), zmm(17), VecElementType::I32, false, 16, 4),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6B,
                width: VecWidth::V512,
                w: true,
            },
        ),
        (
            pack(ymm(16), ymm(18), ymm(17), VecElementType::I16, false, 16, 8),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x63,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_byte_shuffle_rejects_unhinted_and_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));
    let shuffle = |dst, src, control, lanes, block_lanes| OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        block_lanes,
    };

    assert!(matches!(
        lower_single_op_err(shuffle(xmm(1), xmm(1), xmm(2), 16, 16)),
        LowerError::UnsupportedOp { .. }
    ));

    for (kind, hint) in [
        (
            shuffle(xmm(1), xmm(1), xmm(2), 16, 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x00,
            },
        ),
        (
            shuffle(xmm(1), xmm(2), xmm(3), 16, 16),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x00,
            },
        ),
        (
            shuffle(xmm(1), xmm(1), xmm(2), 16, 16),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x00,
            },
        ),
        (
            shuffle(ymm(1), ymm(2), ymm(3), 32, 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            shuffle(ymm(16), ymm(17), ymm(18), 32, 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            shuffle(zmm(16), zmm(17), zmm(18), 64, 16),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x01,
                width: VecWidth::V512,
                w: false,
            },
        ),
        (
            shuffle(ymm(16), ymm(17), ymm(18), 32, 16),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_horizontal_integer_rejects_unhinted_and_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));
    let horizontal =
        |dst, src1, src2, elem, lanes, block_lanes, subtract, saturating| OpKind::VHorizontalBin {
            dst,
            src1,
            src2,
            elem,
            lanes,
            block_lanes,
            subtract,
            saturating,
        };

    assert!(matches!(
        lower_single_op_err(horizontal(
            xmm(1),
            xmm(1),
            xmm(2),
            VecElementType::I16,
            8,
            8,
            false,
            false,
        )),
        LowerError::UnsupportedOp { .. }
    ));

    for (kind, hint) in [
        (
            horizontal(
                xmm(1),
                xmm(1),
                xmm(2),
                VecElementType::I16,
                8,
                4,
                false,
                false,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x01,
            },
        ),
        (
            horizontal(
                xmm(1),
                xmm(2),
                xmm(3),
                VecElementType::I16,
                8,
                8,
                false,
                false,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x01,
            },
        ),
        (
            horizontal(
                xmm(1),
                xmm(1),
                xmm(2),
                VecElementType::I32,
                4,
                4,
                false,
                true,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x02,
            },
        ),
        (
            horizontal(
                xmm(1),
                xmm(1),
                xmm(2),
                VecElementType::I16,
                8,
                8,
                false,
                false,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x01,
            },
        ),
        (
            horizontal(
                ymm(1),
                ymm(2),
                ymm(3),
                VecElementType::I32,
                8,
                4,
                true,
                false,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x06,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            horizontal(
                ymm(16),
                ymm(17),
                ymm(18),
                VecElementType::I16,
                16,
                8,
                true,
                true,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x07,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            horizontal(
                zmm(1),
                zmm(2),
                zmm(3),
                VecElementType::I16,
                32,
                8,
                false,
                false,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x01,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_mulhrs_emits_exact_bytes_and_rejects_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));
    let mulhrs = |dst, src1, src2, lanes| OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem: VecElementType::I16,
        lanes,
        signed1: true,
        signed2: true,
        shift_left: 0,
        round: true,
        sat_bits: 0,
        out_shift: 15,
    };

    for (name, kind, hint, expected) in [
        (
            "PMULHRSW xmm1,xmm2",
            mulhrs(xmm(1), xmm(1), xmm(2), 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x0B,
            },
            &[0x66, 0x0F, 0x38, 0x0B, 0xCA][..],
        ),
        (
            "VEX.W1-hinted VPMULHRSW xmm1,xmm2,xmm3 canonicalized to W0",
            mulhrs(xmm(1), xmm(2), xmm(3), 8),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V128,
                w: true,
            },
            &[0xC4, 0xE2, 0x69, 0x0B, 0xCB][..],
        ),
        (
            "VEX.256 VPMULHRSW ymm1,ymm2,ymm3",
            mulhrs(ymm(1), ymm(2), ymm(3), 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V256,
                w: false,
            },
            &[0xC4, 0xE2, 0x6D, 0x0B, 0xCB][..],
        ),
        (
            "EVEX.W1-hinted VPMULHRSW xmm16,xmm17,xmm18 canonicalized to W0",
            mulhrs(xmm(16), xmm(17), xmm(18), 8),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V128,
                w: true,
            },
            &[0x62, 0xA2, 0x75, 0x00, 0x0B, 0xC2][..],
        ),
        (
            "EVEX.256 VPMULHRSW ymm16,ymm17,ymm18",
            mulhrs(ymm(16), ymm(17), ymm(18), 16),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V256,
                w: false,
            },
            &[0x62, 0xA2, 0x75, 0x20, 0x0B, 0xC2][..],
        ),
        (
            "EVEX.512 VPMULHRSW zmm16,zmm17,zmm18",
            mulhrs(zmm(16), zmm(17), zmm(18), 32),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V512,
                w: false,
            },
            &[0x62, 0xA2, 0x75, 0x40, 0x0B, 0xC2][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, hint);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    assert!(matches!(
        lower_single_op_err(mulhrs(xmm(1), xmm(1), xmm(2), 8)),
        LowerError::UnsupportedOp { .. }
    ));
    for (kind, hint) in [
        (
            mulhrs(xmm(1), xmm(2), xmm(3), 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x0B,
            },
        ),
        (
            mulhrs(ymm(16), ymm(17), ymm(18), 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            mulhrs(ymm(1), ymm(2), ymm(3), 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            mulhrs(zmm(16), zmm(17), zmm(18), 32),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0A,
                width: VecWidth::V512,
                w: false,
            },
        ),
        (
            mulhrs(ymm(16), ymm(17), ymm(18), 16),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x0B,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            OpKind::VMulShiftSat {
                round: false,
                dst: xmm(1),
                src1: xmm(1),
                src2: xmm(2),
                src_elem: VecElementType::I16,
                lanes: 8,
                signed1: true,
                signed2: true,
                shift_left: 0,
                sat_bits: 0,
                out_shift: 15,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x0B,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_mulhw_mulhuw_emit_exact_bytes_and_reject_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));
    let mul_high = |dst, src1, src2, lanes, signed| OpKind::VMulShiftSat {
        dst,
        src1,
        src2,
        src_elem: VecElementType::I16,
        lanes,
        signed1: signed,
        signed2: signed,
        shift_left: 0,
        round: false,
        sat_bits: 0,
        out_shift: 16,
    };

    for (name, kind, hint, expected) in [
        (
            "PMULHW xmm1,xmm2",
            mul_high(xmm(1), xmm(1), xmm(2), 8, true),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE5,
            },
            &[0x66, 0x0F, 0xE5, 0xCA][..],
        ),
        (
            "PMULHUW xmm1,xmm2",
            mul_high(xmm(1), xmm(1), xmm(2), 8, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE4,
            },
            &[0x66, 0x0F, 0xE4, 0xCA][..],
        ),
        (
            "VEX.W1-hinted VPMULHW xmm1,xmm2,xmm3 canonicalized to W0",
            mul_high(xmm(1), xmm(2), xmm(3), 8, true),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE5,
                width: VecWidth::V128,
                w: true,
            },
            &[0xC5, 0xE9, 0xE5, 0xCB][..],
        ),
        (
            "VEX.256 VPMULHUW ymm1,ymm2,ymm3",
            mul_high(ymm(1), ymm(2), ymm(3), 16, false),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE4,
                width: VecWidth::V256,
                w: false,
            },
            &[0xC5, 0xED, 0xE4, 0xCB][..],
        ),
        (
            "EVEX.W1-hinted VPMULHW xmm16,xmm17,xmm18 canonicalized to W0",
            mul_high(xmm(16), xmm(17), xmm(18), 8, true),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE5,
                width: VecWidth::V128,
                w: true,
            },
            &[0x62, 0xA1, 0x75, 0x00, 0xE5, 0xC2][..],
        ),
        (
            "EVEX.512 VPMULHUW zmm16,zmm17,zmm18",
            mul_high(zmm(16), zmm(17), zmm(18), 32, false),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE4,
                width: VecWidth::V512,
                w: false,
            },
            &[0x62, 0xA1, 0x75, 0x40, 0xE4, 0xC2][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, hint);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    assert!(matches!(
        lower_single_op_err(mul_high(xmm(1), xmm(1), xmm(2), 8, true)),
        LowerError::UnsupportedOp { .. }
    ));
    for (kind, hint) in [
        (
            mul_high(xmm(1), xmm(2), xmm(3), 8, true),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE5,
            },
        ),
        (
            mul_high(xmm(1), xmm(1), xmm(2), 8, true),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE4,
            },
        ),
        (
            mul_high(ymm(1), ymm(2), ymm(3), 16, false),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE4,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            mul_high(ymm(16), ymm(17), ymm(18), 16, true),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE5,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            mul_high(zmm(16), zmm(17), zmm(18), 32, false),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE4,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            OpKind::VMulShiftSat {
                dst: xmm(1),
                src1: xmm(1),
                src2: xmm(2),
                src_elem: VecElementType::I16,
                lanes: 8,
                signed1: true,
                signed2: false,
                shift_left: 0,
                round: false,
                sat_bits: 0,
                out_shift: 16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE5,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_movd_q_register_forms_emit_exact_bytes_and_reject_malformed() {
    let gpr = |reg| VReg::Arch(ArchReg::X86(reg));
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let movd_q = |dst, src, width, zero_upper| OpKind::X86MovdQ {
        dst,
        src,
        width,
        zero_upper,
    };

    for (name, kind, hint, expected) in [
        (
            "MOVD xmm1,eax",
            movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W32, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            },
            &[0x66, 0x0F, 0x6E, 0xC8][..],
        ),
        (
            "MOVQ xmm9,r10",
            movd_q(xmm(9), gpr(X86Reg::R10), OpWidth::W64, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            },
            &[0x66, 0x4D, 0x0F, 0x6E, 0xCA][..],
        ),
        (
            "MOVD r8d,xmm9",
            movd_q(gpr(X86Reg::R8), xmm(9), OpWidth::W32, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x7E,
            },
            &[0x66, 0x45, 0x0F, 0x7E, 0xC8][..],
        ),
        (
            "VMOVD xmm1,eax",
            movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W32, true),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: false,
            },
            &[0xC5, 0xF9, 0x6E, 0xC8][..],
        ),
        (
            "VMOVQ xmm9,r10",
            movd_q(xmm(9), gpr(X86Reg::R10), OpWidth::W64, true),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: true,
            },
            &[0xC4, 0x41, 0xF9, 0x6E, 0xCA][..],
        ),
        (
            "VMOVD r8d,xmm9",
            movd_q(gpr(X86Reg::R8), xmm(9), OpWidth::W32, false),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x7E,
                width: VecWidth::V128,
                w: false,
            },
            &[0xC4, 0x41, 0x79, 0x7E, 0xC8][..],
        ),
        (
            "EVEX VMOVQ xmm17,r8",
            movd_q(xmm(17), gpr(X86Reg::R8), OpWidth::W64, true),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: true,
            },
            &[0x62, 0xC1, 0xFD, 0x08, 0x6E, 0xC8][..],
        ),
        (
            "EVEX VMOVD r11d,xmm18",
            movd_q(gpr(X86Reg::R11), xmm(18), OpWidth::W32, false),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x7E,
                width: VecWidth::V128,
                w: false,
            },
            &[0x62, 0xC1, 0x7D, 0x08, 0x7E, 0xD3][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, hint);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let base = movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W32, true);
    assert!(matches!(
        lower_single_op_err(base.clone()),
        LowerError::UnsupportedOp { .. }
    ));

    for (kind, hint) in [
        (
            movd_q(xmm(1), gpr(X86Reg::Rsp), OpWidth::W32, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            },
        ),
        (
            movd_q(xmm(16), gpr(X86Reg::Rax), OpWidth::W32, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            },
        ),
        (
            base.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            },
        ),
        (
            base.clone(),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x7E,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            base.clone(),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            base.clone(),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: true,
            },
        ),
        (
            movd_q(gpr(X86Reg::Rax), xmm(1), OpWidth::W32, true),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x7E,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. }
                | LowerError::InvalidOperand { .. }
                | LowerError::InvalidRegister(_)
        ));
    }
}
#[test]
fn lower_mov_mask_family_emits_exact_bytes_canonicalizes_wig_and_rejects_malformed() {
    let gpr = |reg| VReg::Arch(ArchReg::X86(reg));
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let mov_mask = |dst, src, elem, lanes, dst_width| OpKind::X86MovMask {
        dst,
        src,
        elem,
        lanes,
        dst_width,
    };

    for (name, kind, hint, expected) in [
        (
            "MOVMSKPS eax,xmm2",
            mov_mask(
                gpr(X86Reg::Rax),
                xmm(2),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x50,
            },
            &[0x0F, 0x50, 0xC2][..],
        ),
        (
            "REX.W MOVMSKPD rdx,xmm1",
            mov_mask(
                gpr(X86Reg::Rdx),
                xmm(1),
                VecElementType::F64,
                2,
                OpWidth::W64,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x50,
            },
            &[0x66, 0x48, 0x0F, 0x50, 0xD1][..],
        ),
        (
            "PMOVMSKB r9d,xmm10",
            mov_mask(
                gpr(X86Reg::R9),
                xmm(10),
                VecElementType::I8,
                16,
                OpWidth::W32,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xD7,
            },
            &[0x66, 0x45, 0x0F, 0xD7, 0xCA][..],
        ),
        (
            "VEX.W1-hinted VMOVMSKPS r8d,ymm9 canonicalized to W0",
            mov_mask(
                gpr(X86Reg::R8),
                ymm(9),
                VecElementType::F32,
                8,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V256,
                w: true,
            },
            &[0xC4, 0x41, 0x7C, 0x50, 0xC1][..],
        ),
        (
            "VEX.W1-hinted VMOVMSKPD edx,ymm1 canonicalized to W0",
            mov_mask(
                gpr(X86Reg::Rdx),
                ymm(1),
                VecElementType::F64,
                4,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x50,
                width: VecWidth::V256,
                w: true,
            },
            &[0xC5, 0xFD, 0x50, 0xD1][..],
        ),
        (
            "VEX.W1-hinted VPMOVMSKB eax,xmm1 canonicalized to W0",
            mov_mask(
                gpr(X86Reg::Rax),
                xmm(1),
                VecElementType::I8,
                16,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD7,
                width: VecWidth::V128,
                w: true,
            },
            &[0xC5, 0xF9, 0xD7, 0xC1][..],
        ),
        (
            "VEX.W1-hinted VPMOVMSKB r9d,ymm10 canonicalized to W0",
            mov_mask(
                gpr(X86Reg::R9),
                ymm(10),
                VecElementType::I8,
                32,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD7,
                width: VecWidth::V256,
                w: true,
            },
            &[0xC4, 0x41, 0x7D, 0xD7, 0xCA][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, hint);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let base = mov_mask(
        gpr(X86Reg::Rax),
        xmm(1),
        VecElementType::F32,
        4,
        OpWidth::W32,
    );
    assert!(matches!(
        lower_single_op_err(base.clone()),
        LowerError::UnsupportedOp { .. }
    ));

    for (kind, hint) in [
        (
            mov_mask(
                gpr(X86Reg::Rsp),
                xmm(1),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x50,
            },
        ),
        (
            mov_mask(
                gpr(X86Reg::Rax),
                xmm(16),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x50,
            },
        ),
        (
            mov_mask(
                gpr(X86Reg::Rax),
                ymm(1),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            mov_mask(
                gpr(X86Reg::Rax),
                xmm(1),
                VecElementType::I16,
                8,
                OpWidth::W32,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xD7,
            },
        ),
        (
            base.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x50,
            },
        ),
        (
            mov_mask(
                gpr(X86Reg::Rax),
                xmm(1),
                VecElementType::F32,
                4,
                OpWidth::W64,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            base.clone(),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            base.clone(),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x51,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            base.clone(),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. }
                | LowerError::InvalidOperand { .. }
                | LowerError::InvalidRegister(_)
        ));
    }
}
#[test]
fn lower_maddubs_emits_exact_bytes_and_rejects_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));
    let maddubs = |dst, src1, src2, width| OpKind::VDotProduct {
        dst,
        acc: VReg::Imm(0),
        src1,
        src2,
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I16,
        width,
        src1_unsigned: true,
        saturate: true,
        zeroing: false,
    };

    for (name, kind, hint, expected) in [
        (
            "PMADDUBSW xmm1,xmm2",
            maddubs(xmm(1), xmm(1), xmm(2), VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x04,
            },
            &[0x66, 0x0F, 0x38, 0x04, 0xCA][..],
        ),
        (
            "VEX.W1 VPMADDUBSW xmm1,xmm2,xmm3",
            maddubs(xmm(1), xmm(2), xmm(3), VecWidth::V128),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V128,
                w: true,
            },
            &[0xC4, 0xE2, 0xE9, 0x04, 0xCB][..],
        ),
        (
            "VEX.256 VPMADDUBSW ymm1,ymm2,ymm3",
            maddubs(ymm(1), ymm(2), ymm(3), VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V256,
                w: false,
            },
            &[0xC4, 0xE2, 0x6D, 0x04, 0xCB][..],
        ),
        (
            "EVEX.W1 VPMADDUBSW xmm16,xmm17,xmm18",
            maddubs(xmm(16), xmm(17), xmm(18), VecWidth::V128),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V128,
                w: true,
            },
            &[0x62, 0xA2, 0xF5, 0x00, 0x04, 0xC2][..],
        ),
        (
            "EVEX.256 VPMADDUBSW ymm16,ymm17,ymm18",
            maddubs(ymm(16), ymm(17), ymm(18), VecWidth::V256),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V256,
                w: false,
            },
            &[0x62, 0xA2, 0x75, 0x20, 0x04, 0xC2][..],
        ),
        (
            "EVEX.512 VPMADDUBSW zmm16,zmm17,zmm18",
            maddubs(zmm(16), zmm(17), zmm(18), VecWidth::V512),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V512,
                w: false,
            },
            &[0x62, 0xA2, 0x75, 0x40, 0x04, 0xC2][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, hint);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    assert!(matches!(
        lower_single_op_err(maddubs(xmm(1), xmm(1), xmm(2), VecWidth::V128)),
        LowerError::UnsupportedOp { .. }
    ));
    for (kind, hint) in [
        (
            maddubs(xmm(1), xmm(2), xmm(3), VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x04,
            },
        ),
        (
            maddubs(ymm(16), ymm(17), ymm(18), VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            maddubs(zmm(16), zmm(17), zmm(18), VecWidth::V512),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V512,
                w: false,
            },
        ),
        (
            maddubs(ymm(16), ymm(17), ymm(18), VecWidth::V256),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_maddwd_emits_exact_bytes_and_rejects_malformed_encodings() {
    let xmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)));
    let ymm = |index| VReg::Arch(ArchReg::X86(X86Reg::Ymm(index)));
    let zmm = |index| VReg::Arch(ArchReg::X86(X86Reg::Zmm(index)));
    let maddwd = |dst, src1, src2, width| OpKind::VDotProduct {
        dst,
        acc: VReg::Imm(0),
        src1,
        src2,
        mask: None,
        src_elem: VecElementType::I16,
        acc_elem: VecElementType::I32,
        width,
        src1_unsigned: false,
        saturate: false,
        zeroing: false,
    };

    for (name, kind, hint, expected) in [
        (
            "PMADDWD xmm1,xmm2",
            maddwd(xmm(1), xmm(1), xmm(2), VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF5,
            },
            &[0x66, 0x0F, 0xF5, 0xCA][..],
        ),
        (
            "VEX.W1 VPMADDWD xmm1,xmm2,xmm3",
            maddwd(xmm(1), xmm(2), xmm(3), VecWidth::V128),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V128,
                w: true,
            },
            &[0xC4, 0xE1, 0xE9, 0xF5, 0xCB][..],
        ),
        (
            "VEX.256 VPMADDWD ymm1,ymm2,ymm3",
            maddwd(ymm(1), ymm(2), ymm(3), VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V256,
                w: false,
            },
            &[0xC5, 0xED, 0xF5, 0xCB][..],
        ),
        (
            "EVEX.W1 VPMADDWD xmm16,xmm17,xmm18",
            maddwd(xmm(16), xmm(17), xmm(18), VecWidth::V128),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V128,
                w: true,
            },
            &[0x62, 0xA1, 0xF5, 0x00, 0xF5, 0xC2][..],
        ),
        (
            "EVEX.256 VPMADDWD ymm16,ymm17,ymm18",
            maddwd(ymm(16), ymm(17), ymm(18), VecWidth::V256),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V256,
                w: false,
            },
            &[0x62, 0xA1, 0x75, 0x20, 0xF5, 0xC2][..],
        ),
        (
            "EVEX.512 VPMADDWD zmm16,zmm17,zmm18",
            maddwd(zmm(16), zmm(17), zmm(18), VecWidth::V512),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V512,
                w: false,
            },
            &[0x62, 0xA1, 0x75, 0x40, 0xF5, 0xC2][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, hint);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    assert!(matches!(
        lower_single_op_err(maddwd(xmm(1), xmm(1), xmm(2), VecWidth::V128)),
        LowerError::UnsupportedOp { .. }
    ));
    for (kind, hint) in [
        (
            maddwd(xmm(1), xmm(2), xmm(3), VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF5,
            },
        ),
        (
            maddwd(ymm(16), ymm(17), ymm(18), VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            maddwd(zmm(16), zmm(17), zmm(18), VecWidth::V512),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V512,
                w: false,
            },
        ),
        (
            maddwd(ymm(16), ymm(17), ymm(18), VecWidth::V256),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(matches!(
            lower_single_hinted_op_err(kind, hint),
            LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn test_emit_mov_rr() {
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_mov_rr(PhysReg::Rax, PhysReg::Rcx, OpWidth::W64);
    }
    // MOV RAX, RCX = 48 89 C8
    assert_eq!(buf.data(), &[0x48, 0x89, 0xC8]);
}
#[test]
fn test_emit_mov_ri() {
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_mov_ri(PhysReg::Rax, 42, OpWidth::W64);
    }
    // MOV RAX, 42 (using imm32 sign-extended)
    // 48 C7 C0 2A 00 00 00
    assert_eq!(buf.data(), &[0x48, 0xC7, 0xC0, 0x2A, 0x00, 0x00, 0x00]);
}
#[test]
fn test_emit_add_rr() {
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_add_rr(PhysReg::Rax, PhysReg::Rbx, OpWidth::W64);
    }
    // ADD RAX, RBX = 48 01 D8
    assert_eq!(buf.data(), &[0x48, 0x01, 0xD8]);
}
#[test]
fn emit_imul_imm16_uses_operand_sized_immediate_for_every_address_form() {
    let emitted = |f: &dyn Fn(&mut X86Emitter<'_>)| {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            f(&mut emit);
        }
        buf.data().to_vec()
    };

    assert_eq!(
        emitted(&|emit| { emit.emit_imul_rri(PhysReg::Rax, PhysReg::Rcx, 0x1234, OpWidth::W16) }),
        [0x66, 0x69, 0xC1, 0x34, 0x12]
    );
    assert_eq!(
        emitted(&|emit| {
            emit.emit_imul_rri_force(PhysReg::R8, PhysReg::R9, 0x1234, OpWidth::W16, false)
        }),
        [0x66, 0x45, 0x69, 0xC1, 0x34, 0x12]
    );
    assert_eq!(
        emitted(&|emit| {
            emit.emit_imul_rmi_disp(
                PhysReg::R8,
                PhysReg::Rsp,
                8,
                DispSize::Disp8,
                0x1234,
                OpWidth::W16,
                false,
            )
        }),
        [0x66, 0x44, 0x69, 0x44, 0x24, 0x08, 0x34, 0x12]
    );
    assert_eq!(
        emitted(&|emit| {
            emit.emit_imul_rmi_sib_disp(
                PhysReg::R9,
                Some(PhysReg::Rbx),
                PhysReg::Rsi,
                4,
                0x1234_5678,
                DispSize::Disp32,
                0x1234,
                OpWidth::W16,
                false,
            )
        }),
        [
            0x66, 0x44, 0x69, 0x8C, 0xB3, 0x78, 0x56, 0x34, 0x12, 0x34, 0x12,
        ]
    );
    assert_eq!(
        emitted(&|emit| {
            emit.emit_imul_rmi_abs(PhysReg::R10, 0x1234_5678, 0x1234, OpWidth::W16, false)
        }),
        [
            0x66, 0x44, 0x69, 0x14, 0x25, 0x78, 0x56, 0x34, 0x12, 0x34, 0x12,
        ]
    );

    let mut pcrel = CodeBuffer::new();
    let disp_offset = {
        let mut emit = X86Emitter::new(&mut pcrel);
        emit.emit_imul_rmi_pcrel(PhysReg::R11, 0x1234_5678, 0x1234, OpWidth::W16, false)
    };
    assert_eq!(disp_offset, 4);
    assert_eq!(
        pcrel.data(),
        &[0x66, 0x44, 0x69, 0x1D, 0x78, 0x56, 0x34, 0x12, 0x34, 0x12]
    );

    assert_eq!(
        emitted(&|emit| {
            emit.emit_imul_rri_force(PhysReg::Rax, PhysReg::Rcx, -128, OpWidth::W16, true)
        }),
        [0x66, 0x6B, 0xC1, 0x80],
        "opcode 6B must retain its sign-extended imm8"
    );
    assert_eq!(
        emitted(&|emit| {
            emit.emit_imul_rri_force(PhysReg::Rax, PhysReg::Rcx, 0x1234_5678, OpWidth::W32, false)
        }),
        [0x69, 0xC1, 0x78, 0x56, 0x34, 0x12],
        "32-bit opcode 69 must retain imm32"
    );
    assert_eq!(
        emitted(&|emit| {
            emit.emit_imul_rri_force(PhysReg::Rax, PhysReg::Rcx, 0x1234_5678, OpWidth::W64, false)
        }),
        [0x48, 0x69, 0xC1, 0x78, 0x56, 0x34, 0x12],
        "64-bit opcode 69 must retain sign-extended imm32"
    );
}
#[test]
fn lower_x86_scalar_fp_convert_rejects_interpreter_only_evex_metadata() {
    let xmm0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let k1 = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    for kind in [
        OpKind::X86FpConvert {
            dst: xmm0,
            merge: xmm1,
            src: xmm1,
            mask: Some(k1),
            from: VecElementType::F64,
            to: VecElementType::F32,
            mask_zeroing: true,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            zero_upper: true,
        },
        OpKind::X86FpConvert {
            dst: xmm0,
            merge: xmm0,
            src: xmm1,
            mask: None,
            from: VecElementType::F16,
            to: VecElementType::F32,
            mask_zeroing: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            zero_upper: false,
        },
        OpKind::X86FpConvert {
            dst: xmm0,
            merge: xmm0,
            src: xmm1,
            mask: None,
            from: VecElementType::F64,
            to: VecElementType::F32,
            mask_zeroing: false,
            round: FpRoundMode::RoundUp,
            suppress_exceptions: true,
            zero_upper: false,
        },
    ] {
        assert!(matches!(
            lower_single_op_err(kind),
            LowerError::InvalidOperand { .. } | LowerError::UnsupportedOp { .. }
        ));
    }
}
#[test]
fn lower_x86_get_exponent_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    for (name, kind, hint, expected) in [
        (
            "VGETEXPPS xmm1,xmm3",
            OpKind::X86GetExponent {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                mask: None,
                elem: VecElementType::F32,
                width: VecWidth::V128,
                lanes: 4,
                scalar: false,
                mask_zeroing: false,
                suppress_exceptions: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V128,
                w: false,
            },
            &[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xCB][..],
        ),
        (
            "VGETEXPPD ymm1{k2}{z},ymm3",
            OpKind::X86GetExponent {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                mask: Some(k2),
                elem: VecElementType::F64,
                width: VecWidth::V256,
                lanes: 4,
                scalar: false,
                mask_zeroing: true,
                suppress_exceptions: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V256,
                w: true,
            },
            &[0x62, 0xF2, 0xFD, 0xAA, 0x42, 0xCB][..],
        ),
        (
            "VGETEXPPH zmm17{k2},zmm19,{sae}",
            OpKind::X86GetExponent {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(k2),
                elem: VecElementType::F16,
                width: VecWidth::V512,
                lanes: 32,
                scalar: false,
                mask_zeroing: false,
                suppress_exceptions: true,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map6,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V512,
                w: false,
            },
            &[0x62, 0xA6, 0x7D, 0x1A, 0x42, 0xCB][..],
        ),
        (
            "VGETEXPSS xmm17{k2}{z},xmm18,xmm19,{sae}",
            OpKind::X86GetExponent {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                mask: Some(k2),
                elem: VecElementType::F32,
                width: VecWidth::V128,
                lanes: 1,
                scalar: true,
                mask_zeroing: true,
                suppress_exceptions: true,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x43,
                width: VecWidth::V128,
                w: false,
            },
            &[0x62, 0xA2, 0x6D, 0x92, 0x43, 0xCB][..],
        ),
        (
            "VGETEXPSD xmm1,xmm2,xmm3",
            OpKind::X86GetExponent {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                mask: None,
                elem: VecElementType::F64,
                width: VecWidth::V128,
                lanes: 1,
                scalar: true,
                mask_zeroing: false,
                suppress_exceptions: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x43,
                width: VecWidth::V128,
                w: true,
            },
            &[0x62, 0xF2, 0xED, 0x08, 0x43, 0xCB][..],
        ),
        (
            "VGETEXPSH xmm1,xmm2,xmm3",
            OpKind::X86GetExponent {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                mask: None,
                elem: VecElementType::F16,
                width: VecWidth::V128,
                lanes: 1,
                scalar: true,
                mask_zeroing: false,
                suppress_exceptions: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map6,
                pp: X86SsePrefix::OpSize,
                opcode: 0x43,
                width: VecWidth::V128,
                w: false,
            },
            &[0x62, 0xF6, 0x6D, 0x08, 0x43, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, hint);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    for instruction in [
        &[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xCB][..],
        &[0x62, 0xF2, 0xFD, 0xAA, 0x42, 0xCB][..],
        &[0x62, 0xA6, 0x7D, 0x1A, 0x42, 0xCB][..],
        &[0x62, 0xA2, 0x6D, 0x92, 0x43, 0xCB][..],
        &[0x62, 0xF2, 0xED, 0x08, 0x43, 0xCB][..],
        &[0x62, 0xF6, 0x6D, 0x08, 0x43, 0xCB][..],
    ] {
        let mut block = instruction.to_vec();
        block.push(0xF4);
        let (code, _) = lower_rex2_block(&block);
        assert!(
            code.windows(instruction.len())
                .any(|window| window == instruction),
            "production lift/lower omitted {instruction:02X?} from {code:02X?}"
        );
    }

    let packed_sae_v256 = OpKind::X86GetExponent {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        merge: None,
        src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        mask: Some(k2),
        elem: VecElementType::F32,
        width: VecWidth::V256,
        lanes: 8,
        scalar: false,
        mask_zeroing: false,
        suppress_exceptions: true,
    };
    assert!(matches!(
        lower_single_hinted_op_err(
            packed_sae_v256,
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V256,
                w: false,
            },
        ),
        LowerError::InvalidOperand { .. }
    ));

    let scalar_without_merge = OpKind::X86GetExponent {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        merge: None,
        src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        mask: None,
        elem: VecElementType::F32,
        width: VecWidth::V128,
        lanes: 1,
        scalar: true,
        mask_zeroing: false,
        suppress_exceptions: false,
    };
    assert!(matches!(
        lower_single_hinted_op_err(
            scalar_without_merge,
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x43,
                width: VecWidth::V128,
                w: false,
            },
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_get_mantissa_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions| OpKind::X86GetMantissa {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        imm,
        scalar,
        mask_zeroing,
        suppress_exceptions,
    };
    let hint = |pp, opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F3A,
        pp,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VGETMANTPS xmm1,xmm3,3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                3,
                false,
                false,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x26, VecWidth::V128, false),
            &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x03][..],
        ),
        (
            "VGETMANTPD ymm1{k2}{z},ymm3,7",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V256,
                4,
                7,
                false,
                true,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x26, VecWidth::V256, true),
            &[0x62, 0xF3, 0xFD, 0xAA, 0x26, 0xCB, 0x07][..],
        ),
        (
            "VGETMANTPH zmm17{k2},zmm19,{sae},11",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F16,
                VecWidth::V512,
                32,
                11,
                false,
                false,
                true,
            ),
            hint(X86SsePrefix::None, 0x26, VecWidth::V512, false),
            &[0x62, 0xA3, 0x7C, 0x1A, 0x26, 0xCB, 0x0B][..],
        ),
        (
            "VGETMANTSS xmm17{k2}{z},xmm18,xmm19,{sae},3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F32,
                VecWidth::V128,
                1,
                3,
                true,
                true,
                true,
            ),
            hint(X86SsePrefix::OpSize, 0x27, VecWidth::V128, false),
            &[0x62, 0xA3, 0x6D, 0x92, 0x27, 0xCB, 0x03][..],
        ),
        (
            "VGETMANTSD xmm1,xmm2,xmm3,2",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F64,
                VecWidth::V128,
                1,
                2,
                true,
                false,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x27, VecWidth::V128, true),
            &[0x62, 0xF3, 0xED, 0x08, 0x27, 0xCB, 0x02][..],
        ),
        (
            "VGETMANTSH xmm1,xmm2,xmm3,1",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F16,
                VecWidth::V128,
                1,
                1,
                true,
                false,
                false,
            ),
            hint(X86SsePrefix::None, 0x27, VecWidth::V128, false),
            &[0x62, 0xF3, 0x6C, 0x08, 0x27, 0xCB, 0x01][..],
        ),
        (
            "VGETMANTPS xmm1,xmm3,0xf3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                0xF3,
                false,
                false,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x26, VecWidth::V128, false),
            &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0xF3][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    for instruction in [
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x03][..],
        &[0x62, 0xF3, 0xFD, 0xAA, 0x26, 0xCB, 0x07][..],
        &[0x62, 0xA3, 0x7C, 0x1A, 0x26, 0xCB, 0x0B][..],
        &[0x62, 0xA3, 0x6D, 0x92, 0x27, 0xCB, 0x03][..],
        &[0x62, 0xF3, 0xED, 0x08, 0x27, 0xCB, 0x02][..],
        &[0x62, 0xF3, 0x6C, 0x08, 0x27, 0xCB, 0x01][..],
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0xF3][..],
    ] {
        let mut block = instruction.to_vec();
        block.push(0xF4);
        let (code, _) = lower_rex2_block(&block);
        assert!(
            code.windows(instruction.len())
                .any(|window| window == instruction),
            "production lift/lower omitted {instruction:02X?} from {code:02X?}"
        );
    }

    let packed_sae_v256 = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        Some(k2),
        VecElementType::F32,
        VecWidth::V256,
        8,
        3,
        false,
        false,
        true,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            packed_sae_v256,
            hint(X86SsePrefix::OpSize, 0x26, VecWidth::V256, false),
        ),
        LowerError::InvalidOperand { .. }
    ));

    let scalar_without_merge = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        3,
        true,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            scalar_without_merge,
            hint(X86SsePrefix::OpSize, 0x27, VecWidth::V128, false),
        ),
        LowerError::InvalidOperand { .. }
    ));

    let fp16_wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F16,
        VecWidth::V128,
        8,
        3,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            fp16_wrong_hint,
            hint(X86SsePrefix::OpSize, 0x26, VecWidth::V128, false),
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_round_scale_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions| OpKind::X86RoundScale {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        imm,
        scalar,
        mask_zeroing,
        suppress_exceptions,
    };
    let hint = |pp, opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F3A,
        pp,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VRNDSCALEPS xmm1,xmm3,0x53",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                0x53,
                false,
                false,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x08, VecWidth::V128, false),
            &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x53][..],
        ),
        (
            "VRNDSCALEPD ymm1{k2}{z},ymm3,0xa7",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V256,
                4,
                0xA7,
                false,
                true,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x09, VecWidth::V256, true),
            &[0x62, 0xF3, 0xFD, 0xAA, 0x09, 0xCB, 0xA7][..],
        ),
        (
            "VRNDSCALEPH zmm17{k2}{z},zmm19,{sae},0xb9",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F16,
                VecWidth::V512,
                32,
                0xB9,
                false,
                true,
                true,
            ),
            hint(X86SsePrefix::None, 0x08, VecWidth::V512, false),
            &[0x62, 0xA3, 0x7C, 0x9A, 0x08, 0xCB, 0xB9][..],
        ),
        (
            "VRNDSCALESS xmm17{k2}{z},xmm18,xmm19,{sae},0x4d",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F32,
                VecWidth::V128,
                1,
                0x4D,
                true,
                true,
                true,
            ),
            hint(X86SsePrefix::OpSize, 0x0A, VecWidth::V128, false),
            &[0x62, 0xA3, 0x6D, 0x92, 0x0A, 0xCB, 0x4D][..],
        ),
        (
            "VRNDSCALESD xmm1,xmm2,xmm3,0x21",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F64,
                VecWidth::V128,
                1,
                0x21,
                true,
                false,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x0B, VecWidth::V128, true),
            &[0x62, 0xF3, 0xED, 0x08, 0x0B, 0xCB, 0x21][..],
        ),
        (
            "VRNDSCALESH xmm1,xmm2,xmm3,0x10",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F16,
                VecWidth::V128,
                1,
                0x10,
                true,
                false,
                false,
            ),
            hint(X86SsePrefix::None, 0x0A, VecWidth::V128, false),
            &[0x62, 0xF3, 0x6C, 0x08, 0x0A, 0xCB, 0x10][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    for instruction in [
        &[0x62, 0xF3, 0x7D, 0x08, 0x08, 0xCB, 0x53][..],
        &[0x62, 0xF3, 0xFD, 0xAA, 0x09, 0xCB, 0xA7][..],
        &[0x62, 0xA3, 0x7C, 0x9A, 0x08, 0xCB, 0xB9][..],
        &[0x62, 0xA3, 0x6D, 0x92, 0x0A, 0xCB, 0x4D][..],
        &[0x62, 0xF3, 0xED, 0x08, 0x0B, 0xCB, 0x21][..],
        &[0x62, 0xF3, 0x6C, 0x08, 0x0A, 0xCB, 0x10][..],
    ] {
        let mut block = instruction.to_vec();
        block.push(0xF4);
        let (code, _) = lower_rex2_block(&block);
        assert!(
            code.windows(instruction.len())
                .any(|window| window == instruction),
            "production lift/lower omitted {instruction:02X?} from {code:02X?}"
        );
    }

    let packed_sae_v256 = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        Some(k2),
        VecElementType::F32,
        VecWidth::V256,
        8,
        0x53,
        false,
        false,
        true,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            packed_sae_v256,
            hint(X86SsePrefix::OpSize, 0x08, VecWidth::V256, false),
        ),
        LowerError::InvalidOperand { .. }
    ));

    let scalar_without_merge = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        0x4D,
        true,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            scalar_without_merge,
            hint(X86SsePrefix::OpSize, 0x0A, VecWidth::V128, false),
        ),
        LowerError::InvalidOperand { .. }
    ));

    let fp16_wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F16,
        VecWidth::V128,
        8,
        0x10,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            fp16_wrong_hint,
            hint(X86SsePrefix::OpSize, 0x08, VecWidth::V128, false),
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_reduce_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions| OpKind::X86Reduce {
        dst,
        merge,
        src,
        mask,
        elem,
        width,
        lanes,
        imm,
        scalar,
        mask_zeroing,
        suppress_exceptions,
    };
    let hint = |pp, opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F3A,
        pp,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VREDUCEPS xmm1,xmm3,0x53",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                0x53,
                false,
                false,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x56, VecWidth::V128, false),
            &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x53][..],
        ),
        (
            "VREDUCEPD ymm1{k2}{z},ymm3,0xa7",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V256,
                4,
                0xA7,
                false,
                true,
                false,
            ),
            hint(X86SsePrefix::OpSize, 0x56, VecWidth::V256, true),
            &[0x62, 0xF3, 0xFD, 0xAA, 0x56, 0xCB, 0xA7][..],
        ),
        (
            "VREDUCEPH zmm17{k2}{z},zmm19,{sae},0xb9",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F16,
                VecWidth::V512,
                32,
                0xB9,
                false,
                true,
                true,
            ),
            hint(X86SsePrefix::None, 0x56, VecWidth::V512, false),
            &[0x62, 0xA3, 0x7C, 0x9A, 0x56, 0xCB, 0xB9][..],
        ),
        (
            "VREDUCESS xmm17{k2}{z},xmm18,xmm19,{sae},0x4d",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F32,
                VecWidth::V128,
                1,
                0x4D,
                true,
                true,
                true,
            ),
            hint(X86SsePrefix::OpSize, 0x57, VecWidth::V128, false),
            &[0x62, 0xA3, 0x6D, 0x92, 0x57, 0xCB, 0x4D][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let packed_sae_v256 = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        Some(k2),
        VecElementType::F32,
        VecWidth::V256,
        8,
        0x53,
        false,
        false,
        true,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            packed_sae_v256,
            hint(X86SsePrefix::OpSize, 0x56, VecWidth::V256, false),
        ),
        LowerError::InvalidOperand { .. }
    ));

    let scalar_without_merge = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        0x4D,
        true,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            scalar_without_merge,
            hint(X86SsePrefix::OpSize, 0x57, VecWidth::V128, false),
        ),
        LowerError::InvalidOperand { .. }
    ));

    let fp16_wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F16,
        VecWidth::V128,
        8,
        0x10,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            fp16_wrong_hint,
            hint(X86SsePrefix::OpSize, 0x56, VecWidth::V128, false),
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_range_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions| OpKind::X86Range {
        dst,
        src1,
        src2,
        mask,
        elem,
        width,
        lanes,
        imm,
        scalar,
        mask_zeroing,
        suppress_exceptions,
    };
    let hint = |opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VRANGEPS xmm1,xmm2,xmm3,0x05",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                0x05,
                false,
                false,
                false,
            ),
            hint(0x50, VecWidth::V128, false),
            &[0x62, 0xF3, 0x6D, 0x08, 0x50, 0xCB, 0x05][..],
        ),
        (
            "VRANGEPD ymm1{k2}{z},ymm2,ymm3,0x0d",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V256,
                4,
                0x0D,
                false,
                true,
                false,
            ),
            hint(0x50, VecWidth::V256, true),
            &[0x62, 0xF3, 0xED, 0xAA, 0x50, 0xCB, 0x0D][..],
        ),
        (
            "VRANGEPS zmm17{k2}{z},zmm18,zmm19,{sae},0x0f",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F32,
                VecWidth::V512,
                16,
                0x0F,
                false,
                true,
                true,
            ),
            hint(0x50, VecWidth::V512, false),
            &[0x62, 0xA3, 0x6D, 0x92, 0x50, 0xCB, 0x0F][..],
        ),
        (
            "VRANGESD xmm17{k2}{z},xmm18,xmm19,{sae},0x0d",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V128,
                1,
                0x0D,
                true,
                true,
                true,
            ),
            hint(0x51, VecWidth::V128, true),
            &[0x62, 0xA3, 0xED, 0x92, 0x51, 0xCB, 0x0D][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    for instruction in [
        &[0x62, 0xF3, 0x6D, 0x08, 0x50, 0xCB, 0x05][..],
        &[0x62, 0xF3, 0xED, 0xAA, 0x50, 0xCB, 0x0D][..],
        &[0x62, 0xA3, 0x6D, 0x92, 0x50, 0xCB, 0x0F][..],
        &[0x62, 0xA3, 0xED, 0x92, 0x51, 0xCB, 0x0D][..],
    ] {
        let mut block = instruction.to_vec();
        block.push(0xF4);
        let (code, _) = lower_rex2_block(&block);
        assert!(
            code.windows(instruction.len())
                .any(|window| window == instruction),
            "production lift/lower omitted {instruction:02X?} from {code:02X?}"
        );
    }

    let short_sae = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        Some(k2),
        VecElementType::F32,
        VecWidth::V256,
        8,
        0x05,
        false,
        false,
        true,
    );
    assert!(matches!(
        lower_single_hinted_op_err(short_sae, hint(0x50, VecWidth::V256, false)),
        LowerError::InvalidOperand { .. }
    ));

    let high_imm = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        4,
        0x10,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(high_imm, hint(0x50, VecWidth::V128, false)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        0x05,
        true,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(wrong_hint, hint(0x50, VecWidth::V128, false)),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_fixup_imm_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                imm,
                scalar,
                mask_zeroing,
                suppress_exceptions| OpKind::X86FixupImm {
        dst,
        src1,
        src2,
        mask,
        elem,
        width,
        lanes,
        imm,
        scalar,
        mask_zeroing,
        suppress_exceptions,
    };
    let hint = |opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VFIXUPIMMPS xmm1,xmm2,xmm3,0x00",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                0x00,
                false,
                false,
                false,
            ),
            hint(0x54, VecWidth::V128, false),
            &[0x62, 0xF3, 0x6D, 0x08, 0x54, 0xCB, 0x00][..],
        ),
        (
            "VFIXUPIMMPD ymm1{k2}{z},ymm2,ymm3,0xff",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V256,
                4,
                0xFF,
                false,
                true,
                false,
            ),
            hint(0x54, VecWidth::V256, true),
            &[0x62, 0xF3, 0xED, 0xAA, 0x54, 0xCB, 0xFF][..],
        ),
        (
            "VFIXUPIMMPS zmm17{k2}{z},zmm18,zmm19,{sae},0xa5",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F32,
                VecWidth::V512,
                16,
                0xA5,
                false,
                true,
                true,
            ),
            hint(0x54, VecWidth::V512, false),
            &[0x62, 0xA3, 0x6D, 0x92, 0x54, 0xCB, 0xA5][..],
        ),
        (
            "VFIXUPIMMSD xmm17{k2}{z},xmm18,xmm19,{sae},0xc3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V128,
                1,
                0xC3,
                true,
                true,
                true,
            ),
            hint(0x55, VecWidth::V128, true),
            &[0x62, 0xA3, 0xED, 0x92, 0x55, 0xCB, 0xC3][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let short_sae = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        Some(k2),
        VecElementType::F32,
        VecWidth::V256,
        8,
        0xFF,
        false,
        false,
        true,
    );
    assert!(matches!(
        lower_single_hinted_op_err(short_sae, hint(0x54, VecWidth::V256, false)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        0xFF,
        true,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(wrong_hint, hint(0x54, VecWidth::V128, false)),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_exp2_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make =
        |dst, src, mask, elem, width, lanes, mask_zeroing, suppress_exceptions| OpKind::X86Exp2 {
            dst,
            src,
            mask,
            elem,
            width,
            lanes,
            mask_zeroing,
            suppress_exceptions,
        };
    let hint = |w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0xC8,
        width: VecWidth::V512,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VEXP2PS zmm1,zmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V512,
                16,
                false,
                false,
            ),
            hint(false),
            &[0x62, 0xF2, 0x7D, 0x48, 0xC8, 0xCB][..],
        ),
        (
            "VEXP2PD zmm17{k2}{z},zmm19,{sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V512,
                8,
                true,
                true,
            ),
            hint(true),
            &[0x62, 0xA2, 0xFD, 0x9A, 0xC8, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );

        let mut block = expected.to_vec();
        block.push(0xF4);
        let (production, _) = lower_rex2_block(&block);
        assert!(
            production
                .windows(expected.len())
                .any(|window| window == expected),
            "production lift/lower omitted {expected:02X?} from {production:02X?}"
        );
    }

    let short = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecElementType::F32,
        VecWidth::V256,
        8,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(short, hint(false)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V512,
        16,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            wrong_hint,
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0xC8,
                width: VecWidth::V512,
                w: false,
            }
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_recip14_emits_all_widths_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make =
        |dst, merge, src, mask, elem, width, lanes, scalar, mask_zeroing| OpKind::X86Recip14 {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
        };
    let hint = |opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VRCP14PS xmm1,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                false,
                false,
            ),
            hint(0x4C, VecWidth::V128, false),
            &[0x62, 0xF2, 0x7D, 0x08, 0x4C, 0xCB][..],
        ),
        (
            "VRCP14PD ymm1,ymm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                None,
                VecElementType::F64,
                VecWidth::V256,
                4,
                false,
                false,
            ),
            hint(0x4C, VecWidth::V256, true),
            &[0x62, 0xF2, 0xFD, 0x28, 0x4C, 0xCB][..],
        ),
        (
            "VRCP14PD zmm17{k2}{z},zmm19",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V512,
                8,
                false,
                true,
            ),
            hint(0x4C, VecWidth::V512, true),
            &[0x62, 0xA2, 0xFD, 0xCA, 0x4C, 0xCB][..],
        ),
        (
            "VRCP14SS xmm1,xmm2,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                false,
            ),
            hint(0x4D, VecWidth::V128, false),
            &[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB][..],
        ),
        (
            "VRCP14SD xmm17{k2}{z},xmm18,xmm19",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
                true,
            ),
            hint(0x4D, VecWidth::V128, true),
            &[0x62, 0xA2, 0xED, 0x82, 0x4D, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );

        let mut block = expected.to_vec();
        block.push(0xF4);
        let (production, _) = lower_rex2_block(&block);
        assert!(
            production
                .windows(expected.len())
                .any(|window| window == expected),
            "production lift/lower omitted {expected:02X?} from {production:02X?}"
        );
    }

    let mismatched_width = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecElementType::F32,
        VecWidth::V512,
        16,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(mismatched_width, hint(0x4C, VecWidth::V512, false)),
        LowerError::InvalidOperand { .. }
    ));

    let missing_merge = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        true,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(missing_merge, hint(0x4D, VecWidth::V128, false)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V512,
        16,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(wrong_hint, hint(0x4D, VecWidth::V512, false)),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_rsqrt14_emits_all_widths_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make =
        |dst, merge, src, mask, elem, width, lanes, scalar, mask_zeroing| OpKind::X86Rsqrt14 {
            dst,
            merge,
            src,
            mask,
            elem,
            width,
            lanes,
            scalar,
            mask_zeroing,
        };
    let hint = |opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VRSQRT14PS xmm1,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                false,
                false,
            ),
            hint(0x4E, VecWidth::V128, false),
            &[0x62, 0xF2, 0x7D, 0x08, 0x4E, 0xCB][..],
        ),
        (
            "VRSQRT14PD ymm1,ymm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                None,
                VecElementType::F64,
                VecWidth::V256,
                4,
                false,
                false,
            ),
            hint(0x4E, VecWidth::V256, true),
            &[0x62, 0xF2, 0xFD, 0x28, 0x4E, 0xCB][..],
        ),
        (
            "VRSQRT14PD zmm17{k2}{z},zmm19",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V512,
                8,
                false,
                true,
            ),
            hint(0x4E, VecWidth::V512, true),
            &[0x62, 0xA2, 0xFD, 0xCA, 0x4E, 0xCB][..],
        ),
        (
            "VRSQRT14SS xmm1,xmm2,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                false,
            ),
            hint(0x4F, VecWidth::V128, false),
            &[0x62, 0xF2, 0x6D, 0x08, 0x4F, 0xCB][..],
        ),
        (
            "VRSQRT14SD xmm17{k2}{z},xmm18,xmm19",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
                true,
            ),
            hint(0x4F, VecWidth::V128, true),
            &[0x62, 0xA2, 0xED, 0x82, 0x4F, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );

        let mut block = expected.to_vec();
        block.push(0xF4);
        let (production, _) = lower_rex2_block(&block);
        assert!(
            production
                .windows(expected.len())
                .any(|window| window == expected),
            "production lift/lower omitted {expected:02X?} from {production:02X?}"
        );
    }

    let mismatched_width = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecElementType::F32,
        VecWidth::V512,
        16,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(mismatched_width, hint(0x4E, VecWidth::V512, false)),
        LowerError::InvalidOperand { .. }
    ));

    let missing_merge = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        true,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(missing_merge, hint(0x4F, VecWidth::V128, false)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V512,
        16,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(wrong_hint, hint(0x4F, VecWidth::V512, false)),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_fp16_approx_emits_all_widths_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |rsqrt, dst, merge, src, mask, width, lanes, scalar, mask_zeroing| {
        if rsqrt {
            OpKind::X86RsqrtFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
        } else {
            OpKind::X86RecipFp16 {
                dst,
                merge,
                src,
                mask,
                width,
                lanes,
                scalar,
                mask_zeroing,
            }
        }
    };
    let hint = |opcode, width| X86OpHint::EvexOp {
        map: X86VecMap::Map6,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w: false,
    };
    for (name, kind, encoding, expected) in [
        (
            "VRCPPH xmm1,xmm3",
            make(
                false,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecWidth::V128,
                8,
                false,
                false,
            ),
            hint(0x4C, VecWidth::V128),
            &[0x62, 0xF6, 0x7D, 0x08, 0x4C, 0xCB][..],
        ),
        (
            "VRCPPH ymm1,ymm3",
            make(
                false,
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                None,
                VecWidth::V256,
                16,
                false,
                false,
            ),
            hint(0x4C, VecWidth::V256),
            &[0x62, 0xF6, 0x7D, 0x28, 0x4C, 0xCB][..],
        ),
        (
            "VRSQRTPH zmm17{k2}{z},zmm19",
            make(
                true,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecWidth::V512,
                32,
                false,
                true,
            ),
            hint(0x4E, VecWidth::V512),
            &[0x62, 0xA6, 0x7D, 0xCA, 0x4E, 0xCB][..],
        ),
        (
            "VRCPSH xmm1,xmm2,xmm3",
            make(
                false,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecWidth::V128,
                1,
                true,
                false,
            ),
            hint(0x4D, VecWidth::V128),
            &[0x62, 0xF6, 0x6D, 0x08, 0x4D, 0xCB][..],
        ),
        (
            "VRSQRTSH xmm17{k2}{z},xmm18,xmm19",
            make(
                true,
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecWidth::V128,
                1,
                true,
                true,
            ),
            hint(0x4F, VecWidth::V128),
            &[0x62, 0xA6, 0x6D, 0x82, 0x4F, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let missing_merge = make(
        false,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecWidth::V128,
        1,
        true,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(missing_merge, hint(0x4D, VecWidth::V128)),
        LowerError::InvalidOperand { .. }
    ));

    let mismatched_width = make(
        true,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecWidth::V512,
        32,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(mismatched_width, hint(0x4E, VecWidth::V512)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        false,
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
        None,
        VecWidth::V512,
        32,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            wrong_hint,
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x4C,
                width: VecWidth::V512,
                w: false,
            }
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_recip28_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make =
        |dst, merge, src, mask, elem, width, lanes, scalar, mask_zeroing, suppress_exceptions| {
            OpKind::X86Recip28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            }
        };
    let hint = |opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VRCP28PS zmm1,zmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V512,
                16,
                false,
                false,
                false,
            ),
            hint(0xCA, VecWidth::V512, false),
            &[0x62, 0xF2, 0x7D, 0x48, 0xCA, 0xCB][..],
        ),
        (
            "VRCP28PD zmm17{k2}{z},zmm19,{sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V512,
                8,
                false,
                true,
                true,
            ),
            hint(0xCA, VecWidth::V512, true),
            &[0x62, 0xA2, 0xFD, 0x9A, 0xCA, 0xCB][..],
        ),
        (
            "VRCP28SS xmm1,xmm2,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                false,
                false,
            ),
            hint(0xCB, VecWidth::V128, false),
            &[0x62, 0xF2, 0x6D, 0x08, 0xCB, 0xCB][..],
        ),
        (
            "VRCP28SD xmm17{k2}{z},xmm18,xmm19,{sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
                true,
                true,
            ),
            hint(0xCB, VecWidth::V128, true),
            &[0x62, 0xA2, 0xED, 0x92, 0xCB, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );

        let mut block = expected.to_vec();
        block.push(0xF4);
        let (production, _) = lower_rex2_block(&block);
        assert!(
            production
                .windows(expected.len())
                .any(|window| window == expected),
            "production lift/lower omitted {expected:02X?} from {production:02X?}"
        );
    }

    let short = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecElementType::F32,
        VecWidth::V256,
        8,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(short, hint(0xCA, VecWidth::V256, false)),
        LowerError::InvalidOperand { .. }
    ));

    let missing_merge = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        true,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(missing_merge, hint(0xCB, VecWidth::V128, false)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V512,
        16,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(wrong_hint, hint(0xCB, VecWidth::V512, false)),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_rsqrt28_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make =
        |dst, merge, src, mask, elem, width, lanes, scalar, mask_zeroing, suppress_exceptions| {
            OpKind::X86Rsqrt28 {
                dst,
                merge,
                src,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                suppress_exceptions,
            }
        };
    let hint = |opcode, width, w| X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VRSQRT28PS zmm1,zmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V512,
                16,
                false,
                false,
                false,
            ),
            hint(0xCC, VecWidth::V512, false),
            &[0x62, 0xF2, 0x7D, 0x48, 0xCC, 0xCB][..],
        ),
        (
            "VRSQRT28PD zmm17{k2}{z},zmm19,{sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                None,
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V512,
                8,
                false,
                true,
                true,
            ),
            hint(0xCC, VecWidth::V512, true),
            &[0x62, 0xA2, 0xFD, 0x9A, 0xCC, 0xCB][..],
        ),
        (
            "VRSQRT28SS xmm1,xmm2,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                1,
                true,
                false,
                false,
            ),
            hint(0xCD, VecWidth::V128, false),
            &[0x62, 0xF2, 0x6D, 0x08, 0xCD, 0xCB][..],
        ),
        (
            "VRSQRT28SD xmm17{k2}{z},xmm18,xmm19,{sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
                true,
                true,
            ),
            hint(0xCD, VecWidth::V128, true),
            &[0x62, 0xA2, 0xED, 0x92, 0xCD, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );

        let mut block = expected.to_vec();
        block.push(0xF4);
        let (production, _) = lower_rex2_block(&block);
        assert!(
            production
                .windows(expected.len())
                .any(|window| window == expected),
            "production lift/lower omitted {expected:02X?} from {production:02X?}"
        );
    }

    let short = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecElementType::F32,
        VecWidth::V256,
        8,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(short, hint(0xCC, VecWidth::V256, false)),
        LowerError::InvalidOperand { .. }
    ));

    let missing_merge = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        true,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(missing_merge, hint(0xCD, VecWidth::V128, false)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
        None,
        VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V512,
        16,
        false,
        false,
        false,
    );
    assert!(matches!(
        lower_single_hinted_op_err(wrong_hint, hint(0xCD, VecWidth::V512, false)),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_scale_f_emits_canonical_evex_and_rejects_synthetic_shapes() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                scalar,
                mask_zeroing,
                round,
                suppress_exceptions| OpKind::X86ScaleF {
        dst,
        src1,
        src2,
        mask,
        elem,
        width,
        lanes,
        scalar,
        mask_zeroing,
        round,
        suppress_exceptions,
    };
    let hint = |map, opcode, width, w| X86OpHint::EvexOp {
        map,
        pp: X86SsePrefix::OpSize,
        opcode,
        width,
        w,
    };
    for (name, kind, encoding, expected) in [
        (
            "VSCALEFPS xmm1,xmm2,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F32,
                VecWidth::V128,
                4,
                false,
                false,
                FpRoundMode::Dynamic,
                false,
            ),
            hint(X86VecMap::Map0F38, 0x2C, VecWidth::V128, false),
            &[0x62, 0xF2, 0x6D, 0x08, 0x2C, 0xCB][..],
        ),
        (
            "VSCALEFPD ymm1{k2}{z},ymm2,ymm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                Some(k2),
                VecElementType::F64,
                VecWidth::V256,
                4,
                false,
                true,
                FpRoundMode::Dynamic,
                false,
            ),
            hint(X86VecMap::Map0F38, 0x2C, VecWidth::V256, true),
            &[0x62, 0xF2, 0xED, 0xAA, 0x2C, 0xCB][..],
        ),
        (
            "VSCALEFPH zmm17{k2}{z},zmm18,zmm19,{rn-sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecElementType::F16,
                VecWidth::V512,
                32,
                false,
                true,
                FpRoundMode::RoundNearest,
                true,
            ),
            hint(X86VecMap::Map6, 0x2C, VecWidth::V512, false),
            &[0x62, 0xA6, 0x6D, 0x92, 0x2C, 0xCB][..],
        ),
        (
            "VSCALEFSD xmm1,xmm2,xmm3,{rz-sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecElementType::F64,
                VecWidth::V128,
                1,
                true,
                false,
                FpRoundMode::RoundTowardZero,
                true,
            ),
            hint(X86VecMap::Map0F38, 0x2D, VecWidth::V128, true),
            &[0x62, 0xF2, 0xED, 0x78, 0x2D, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let invalid_short_er = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecElementType::F32,
        VecWidth::V256,
        8,
        false,
        false,
        FpRoundMode::RoundNearest,
        true,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            invalid_short_er,
            hint(X86VecMap::Map0F38, 0x2C, VecWidth::V256, false),
        ),
        LowerError::InvalidOperand { .. }
    ));

    let invalid_dynamic_sae = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecElementType::F32,
        VecWidth::V128,
        1,
        true,
        false,
        FpRoundMode::Dynamic,
        true,
    );
    assert!(matches!(
        lower_single_hinted_op_err(
            invalid_dynamic_sae,
            hint(X86VecMap::Map0F38, 0x2D, VecWidth::V128, false),
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_fp16_complex_emits_canonical_evex_and_rejects_aliases_and_bad_metadata() {
    let k2 = VReg::Arch(ArchReg::X86(X86Reg::K(2)));
    let make = |dst,
                src1,
                src2,
                mask,
                width,
                pairs,
                scalar,
                mask_zeroing,
                accumulate,
                conjugate,
                round| OpKind::X86FP16Complex {
        dst,
        src1,
        src2,
        mask,
        width,
        pairs,
        scalar,
        mask_zeroing,
        accumulate,
        conjugate,
        round,
    };
    let hint = |pp, opcode, width| X86OpHint::EvexOp {
        map: X86VecMap::Map6,
        pp,
        opcode,
        width,
        w: false,
    };
    for (name, kind, encoding, expected) in [
        (
            "VFMULCPH xmm1,xmm2,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecWidth::V128,
                4,
                false,
                false,
                false,
                false,
                FpRoundMode::Dynamic,
            ),
            hint(X86SsePrefix::Rep, 0xD6, VecWidth::V128),
            &[0x62, 0xF6, 0x6E, 0x08, 0xD6, 0xCB][..],
        ),
        (
            "VFCMULCPH ymm1{k2}{z},ymm2,ymm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                Some(k2),
                VecWidth::V256,
                8,
                false,
                true,
                false,
                true,
                FpRoundMode::Dynamic,
            ),
            hint(X86SsePrefix::Repne, 0xD6, VecWidth::V256),
            &[0x62, 0xF6, 0x6F, 0xAA, 0xD6, 0xCB][..],
        ),
        (
            "VFMADDCPH zmm17{k2},zmm18,zmm19,{rn-sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                Some(k2),
                VecWidth::V512,
                16,
                false,
                false,
                true,
                false,
                FpRoundMode::RoundNearest,
            ),
            hint(X86SsePrefix::Rep, 0x56, VecWidth::V512),
            &[0x62, 0xA6, 0x6E, 0x12, 0x56, 0xCB][..],
        ),
        (
            "VFCMADDCSH xmm1,xmm2,xmm3",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                None,
                VecWidth::V128,
                1,
                true,
                false,
                true,
                true,
                FpRoundMode::Dynamic,
            ),
            hint(X86SsePrefix::Repne, 0x57, VecWidth::V128),
            &[0x62, 0xF6, 0x6F, 0x08, 0x57, 0xCB][..],
        ),
        (
            "VFMULCSH xmm17{k2}{z},xmm18,xmm19,{rd-sae}",
            make(
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                Some(k2),
                VecWidth::V128,
                1,
                true,
                true,
                false,
                false,
                FpRoundMode::RoundDown,
            ),
            hint(X86SsePrefix::Rep, 0xD7, VecWidth::V128),
            &[0x62, 0xA6, 0x6E, 0xB2, 0xD7, 0xCB][..],
        ),
    ] {
        let code = lower_single_hinted_op(kind, encoding);
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let alias = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecWidth::V128,
        4,
        false,
        false,
        false,
        false,
        FpRoundMode::Dynamic,
    );
    assert!(matches!(
        lower_single_hinted_op_err(alias, hint(X86SsePrefix::Rep, 0xD6, VecWidth::V128)),
        LowerError::InvalidOperand { .. }
    ));

    let short_er = make(
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
        None,
        VecWidth::V256,
        8,
        false,
        false,
        false,
        false,
        FpRoundMode::RoundNearest,
    );
    assert!(matches!(
        lower_single_hinted_op_err(short_er, hint(X86SsePrefix::Rep, 0xD6, VecWidth::V256)),
        LowerError::InvalidOperand { .. }
    ));

    let wrong_hint = make(
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        None,
        VecWidth::V128,
        1,
        true,
        false,
        true,
        true,
        FpRoundMode::Dynamic,
    );
    assert!(matches!(
        lower_single_hinted_op_err(wrong_hint, hint(X86SsePrefix::Rep, 0x57, VecWidth::V128)),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn test_emit_jmp_rel32() {
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_jmp_rel32(0x12345678);
    }
    // JMP rel32 = E9 78 56 34 12
    assert_eq!(buf.data(), &[0xE9, 0x78, 0x56, 0x34, 0x12]);
}
#[test]
fn test_emit_ret() {
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_ret();
    }
    assert_eq!(buf.data(), &[0xC3]);
}
#[test]
fn test_emit_extended_reg() {
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_mov_rr(PhysReg::R8, PhysReg::R9, OpWidth::W64);
    }
    // MOV R8, R9 = 4D 89 C8
    assert_eq!(buf.data(), &[0x4D, 0x89, 0xC8]);
}
#[test]
fn lower_adc_sbb_alias_second_source_covers_all_integer_widths() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    for (width, adc, sbb) in [
        (
            OpWidth::W8,
            &[0x41, 0x10, 0xC0][..],
            &[
                0x41, 0x50, 0x41, 0x88, 0xC0, 0x44, 0x1A, 0x04, 0x24, 0x48, 0x8D, 0x64, 0x24, 0x08,
            ][..],
        ),
        (
            OpWidth::W16,
            &[0x66, 0x41, 0x11, 0xC0][..],
            &[
                0x41, 0x50, 0x66, 0x41, 0x89, 0xC0, 0x66, 0x44, 0x1B, 0x04, 0x24, 0x48, 0x8D, 0x64,
                0x24, 0x08,
            ][..],
        ),
        (
            OpWidth::W32,
            &[0x41, 0x11, 0xC0][..],
            &[
                0x41, 0x50, 0x41, 0x89, 0xC0, 0x44, 0x1B, 0x04, 0x24, 0x48, 0x8D, 0x64, 0x24, 0x08,
            ][..],
        ),
        (
            OpWidth::W64,
            &[0x49, 0x11, 0xC0][..],
            &[
                0x41, 0x50, 0x49, 0x89, 0xC0, 0x4C, 0x1B, 0x04, 0x24, 0x48, 0x8D, 0x64, 0x24, 0x08,
            ][..],
        ),
    ] {
        let adc_code = lower_single_op(OpKind::Adc {
            dst: r8,
            src1: rax,
            src2: SrcOperand::Reg(r8),
            width,
            flags: FlagUpdate::All,
        });
        assert!(
            adc_code.windows(adc.len()).any(|bytes| bytes == adc),
            "missing alias-safe {width:?} ADC {adc:02X?}: {adc_code:02X?}"
        );

        let sbb_code = lower_single_op(OpKind::Sbb {
            dst: r8,
            src1: rax,
            src2: SrcOperand::Reg(r8),
            width,
            flags: FlagUpdate::All,
        });
        assert!(
            sbb_code.windows(sbb.len()).any(|bytes| bytes == sbb),
            "missing alias-safe {width:?} SBB {sbb:02X?}: {sbb_code:02X?}"
        );
    }
}
#[test]
fn lower_cwd_cdq_cqo_emits_exact_encodings_and_rejects_malformed_shapes() {
    let gpr = |reg| VReg::Arch(ArchReg::X86(reg));

    for (name, width, expected) in [
        ("CWD", OpWidth::W16, &[0x66, 0x99][..]),
        ("CDQ", OpWidth::W32, &[0x99][..]),
        ("CQO", OpWidth::W64, &[0x48, 0x99][..]),
    ] {
        let code = lower_single_op(OpKind::Cwd {
            dst: gpr(X86Reg::Rdx),
            src: gpr(X86Reg::Rax),
            width,
        });
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    for (name, op) in [
        (
            "wrong source",
            OpKind::Cwd {
                dst: gpr(X86Reg::Rdx),
                src: gpr(X86Reg::Rcx),
                width: OpWidth::W64,
            },
        ),
        (
            "wrong destination",
            OpKind::Cwd {
                dst: gpr(X86Reg::Rcx),
                src: gpr(X86Reg::Rax),
                width: OpWidth::W32,
            },
        ),
        (
            "unsupported width",
            OpKind::Cwd {
                dst: gpr(X86Reg::Rdx),
                src: gpr(X86Reg::Rax),
                width: OpWidth::W8,
            },
        ),
    ] {
        assert!(
            matches!(
                lower_single_op_err(op),
                LowerError::InvalidOperand { .. } | LowerError::UnsupportedOp { .. }
            ),
            "{name}"
        );
    }
}
#[test]
fn lower_and_not_preserves_aliases_and_partial_flag_contracts() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let defined = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);

    let flagful = lower_single_op(OpKind::AndNot {
        dst: r8,
        src1: rax,
        src2: SrcOperand::Reg(rbx),
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(defined),
    });
    let core = [
        0x9C, 0x50, 0x49, 0x89, 0xD8, 0x49, 0xF7, 0xD0, 0x4C, 0x23, 0x04, 0x24, 0x48, 0x8D, 0x64,
        0x24, 0x08,
    ];
    assert!(
        flagful.windows(core.len()).any(|window| window == core),
        "flagful ANDN core lowering: {flagful:02X?}"
    );

    let nf_alias = lower_single_op(OpKind::AndNot {
        dst: rax,
        src1: rax,
        src2: SrcOperand::Reg(rcx),
        width: OpWidth::W32,
        flags: FlagUpdate::None,
    });
    let alias_core = [
        0x9C, 0x50, 0x89, 0xC8, 0xF7, 0xD0, 0x23, 0x04, 0x24, 0x48, 0x8D, 0x64, 0x24, 0x08, 0x9D,
    ];
    assert!(
        nf_alias
            .windows(alias_core.len())
            .any(|window| window == alias_core),
        "NF aliased ANDN lowering: {nf_alias:02X?}"
    );

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
    let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
    let r31 = VReg::Arch(ArchReg::X86(X86Reg::R31));
    for (name, op, expected) in [
        (
            "state-backed flagful qword",
            OpKind::AndNot {
                dst: rsp,
                src1: rbp,
                src2: SrcOperand::Reg(r16),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(defined),
            },
            &[0x4C, 0x89, 0xC2, 0x48, 0xF7, 0xD2, 0x48, 0x21, 0xFA][..],
        ),
        (
            "state-backed NF dword",
            OpKind::AndNot {
                dst: r31,
                src1: rsp,
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            &[0x44, 0x89, 0xC2, 0xF7, 0xD2, 0x21, 0xFA][..],
        ),
        (
            "state-backed NF all operands alias",
            OpKind::AndNot {
                dst: r16,
                src1: r16,
                src2: SrcOperand::Reg(r16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            &[0x4C, 0x89, 0xC2, 0x48, 0xF7, 0xD2, 0x48, 0x21, 0xFA][..],
        ),
    ] {
        let code = lower_single_op(op);
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name}: missing alias-safe baseline core {expected:02X?} in {code:02X?}"
        );
        assert!(
            code.contains(&0x9C) && code.contains(&0x9D),
            "{name}: flags must be saved and restored or merged"
        );
    }

    for malformed in [
        OpKind::AndNot {
            dst: rax,
            src1: rcx,
            src2: SrcOperand::Reg(rbx),
            width: OpWidth::W16,
            flags: FlagUpdate::Specific(defined),
        },
        OpKind::AndNot {
            dst: rax,
            src1: rcx,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(defined),
        },
        OpKind::AndNot {
            dst: rax,
            src1: rcx,
            src2: SrcOperand::Reg(rbx),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        },
        OpKind::AndNot {
            dst: r16,
            src1: rsp,
            src2: SrcOperand::Reg(rbp),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        OpKind::AndNot {
            dst: r31,
            src1: rsp,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::AndNot {
            dst: r31,
            src1: VReg::Virtual(crate::smir::ir::types::VirtualId(7)),
            src2: SrcOperand::Reg(rbp),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::AndNot {
            dst: r31,
            src1: rbp,
            src2: SrcOperand::Reg(r16),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    ] {
        assert!(matches!(
            lower_single_op_err(malformed),
            LowerError::InvalidOperand { .. }
        ));
    }
    assert!(matches!(
        lower_single_hinted_op_err(
            OpKind::AndNot {
                dst: r16,
                src1: rsp,
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_x86_fences_emits_exact_baseline_encodings() {
    for (kind, expected) in [
        (FenceKind::LoadLoad, [0x0F, 0xAE, 0xE8]),
        (FenceKind::Full, [0x0F, 0xAE, 0xF0]),
        (FenceKind::StoreStore, [0x0F, 0xAE, 0xF8]),
    ] {
        let code = lower_single_op(OpKind::Fence { kind });
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{kind:?}: missing {expected:02X?} in {code:02X?}"
        );
    }
    for kind in [
        FenceKind::LoadStore,
        FenceKind::StoreLoad,
        FenceKind::ISync,
        FenceKind::DSync,
    ] {
        assert!(matches!(
            lower_single_op_err(OpKind::Fence { kind }),
            LowerError::UnsupportedOp { .. }
        ));
    }
}
#[test]
fn lower_cldemote_is_an_exact_noop_and_rejects_fault_capable_cache_forms() {
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let cldemote = lower_single_op(OpKind::X86CacheControl {
        addr: Address::Direct(rbx),
        kind: X86CacheControlKind::Cldemote,
    });
    assert!(
        !cldemote.windows(2).any(|bytes| bytes == [0x0F, 0x1C]),
        "ignored CLDEMOTE must not expose a guest address to the host cache"
    );

    for kind in [
        X86CacheControlKind::Clflush,
        X86CacheControlKind::Clflushopt,
        X86CacheControlKind::Clwb,
    ] {
        assert!(matches!(
            lower_single_op_err(OpKind::X86CacheControl {
                addr: Address::Direct(rbx),
                kind,
            }),
            LowerError::UnsupportedOp { .. }
        ));
    }
}
#[test]
fn lower_xgetbv_requires_architectural_implicit_registers() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let code = lower_single_op(OpKind::X86XGetBv {
        dst_low: rax,
        dst_high: rdx,
        selector: rcx,
    });
    let xcr0_offset = (X86_GUEST_XCR0_OFFSET as u32).to_le_bytes();
    let xgetbv1_offset = (X86_GUEST_XGETBV1_OFFSET as u32).to_le_bytes();
    assert!(
        code.windows(4).any(|bytes| bytes == xcr0_offset),
        "XGETBV must read the state-backed XCR0 slot"
    );
    assert!(
        code.windows(4).any(|bytes| bytes == xgetbv1_offset),
        "XGETBV(1) must read the state-backed XINUSE slot"
    );

    for malformed in [
        OpKind::X86XGetBv {
            dst_low: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            dst_high: rdx,
            selector: rcx,
        },
        OpKind::X86XGetBv {
            dst_low: rax,
            dst_high: VReg::Arch(ArchReg::X86(X86Reg::R9)),
            selector: rcx,
        },
        OpKind::X86XGetBv {
            dst_low: rax,
            dst_high: rdx,
            selector: VReg::Arch(ArchReg::X86(X86Reg::R10)),
        },
    ] {
        assert!(matches!(
            lower_single_op_err(malformed),
            LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_egpr_add_bails_instead_of_allocating_host_alias() {
    let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Add {
            dst: r16,
            src1: r16,
            src2: SrcOperand::Reg(rax),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = X86_64Lowerer::new();
    assert!(
        lowerer.lower_function(&func).is_err(),
        "unsupported EGPR ALU must bail rather than alias a legacy host GPR"
    );
}
#[test]
fn test_lower_simple_function() {
    // Create a simple function: return 42
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

    let v0 = builder.alloc_vreg();

    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: v0,
            src: SrcOperand::imm(42),
            width: OpWidth::W64,
        },
    );

    builder.set_terminator(Terminator::Return { values: vec![v0] });

    let func = builder.finish();

    // Lower it
    let mut lowerer = X86_64Lowerer::new();
    let result = lowerer.lower_function(&func).unwrap();

    assert!(result.code_size > 0);

    let code = lowerer.finalize().unwrap();
    // Should start with PUSH RBP; MOV RBP, RSP
    assert!(code.len() >= 4);
    assert_eq!(code[0], 0x55); // PUSH RBP
}
#[test]
fn test_lower_add() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

    let v0 = builder.alloc_vreg();
    let v1 = builder.alloc_vreg();
    let v2 = builder.alloc_vreg();

    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: v0,
            src: SrcOperand::imm(10),
            width: OpWidth::W64,
        },
    );

    builder.push_op(
        0x1004,
        OpKind::Mov {
            dst: v1,
            src: SrcOperand::imm(20),
            width: OpWidth::W64,
        },
    );

    builder.push_op(
        0x1008,
        OpKind::Add {
            dst: v2,
            src1: v0,
            src2: SrcOperand::Reg(v1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );

    builder.set_terminator(Terminator::Return { values: vec![v2] });

    let func = builder.finish();

    let mut lowerer = X86_64Lowerer::new();
    let result = lowerer.lower_function(&func).unwrap();

    assert!(result.code_size > 0);
}
#[test]
fn test_x86_cond_invert() {
    assert_eq!(X86Cond::E.invert(), X86Cond::Ne);
    assert_eq!(X86Cond::L.invert(), X86Cond::Ge);
    assert_eq!(X86Cond::B.invert(), X86Cond::Ae);
}
#[test]
fn test_lower_div_unsigned() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

    let dividend = builder.alloc_vreg();
    let divisor = builder.alloc_vreg();
    let quotient = builder.alloc_vreg();
    let remainder = builder.alloc_vreg();

    // dividend = 100
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: dividend,
            src: SrcOperand::imm(100),
            width: OpWidth::W64,
        },
    );

    // divisor = 7
    builder.push_op(
        0x1004,
        OpKind::Mov {
            dst: divisor,
            src: SrcOperand::imm(7),
            width: OpWidth::W64,
        },
    );

    // (quotient, remainder) = dividend / divisor
    builder.push_op(
        0x1008,
        OpKind::DivU {
            quot: quotient,
            rem: Some(remainder),
            src1: dividend,
            src2: SrcOperand::Reg(divisor),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );

    builder.set_terminator(Terminator::Return {
        values: vec![quotient],
    });

    let func = builder.finish();

    let mut lowerer = X86_64Lowerer::new();
    let result = lowerer.lower_function(&func).unwrap();

    assert!(result.code_size > 0);
    let code = lowerer.finalize().unwrap();
    // Should contain DIV instruction (F7 /6)
    // Look for the pattern in the generated code
    assert!(!code.is_empty());
}
#[test]
fn lower_divu_flags_none_saves_flags_before_zeroing_high_half() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

    let dividend = builder.alloc_vreg();
    let divisor = builder.alloc_vreg();
    let quotient = builder.alloc_vreg();
    let remainder = builder.alloc_vreg();

    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: dividend,
            src: SrcOperand::imm(100),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1004,
        OpKind::Mov {
            dst: divisor,
            src: SrcOperand::imm(7),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1008,
        OpKind::DivU {
            quot: quotient,
            rem: Some(remainder),
            src1: dividend,
            src2: SrcOperand::Reg(divisor),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return {
        values: vec![quotient],
    });

    let func = builder.finish();
    let mut lowerer = X86_64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let push = code.iter().position(|byte| *byte == 0x9C).expect("pushfq");
    let zero_rdx = code
        .windows(3)
        .position(|window| window == [0x48, 0x31, 0xD2])
        .expect("xor rdx, rdx");
    let pop = code.iter().position(|byte| *byte == 0x9D).expect("popfq");

    assert!(push < zero_rdx, "pushfq must precede flag-clobbering zero");
    assert!(
        zero_rdx < pop,
        "popfq must restore flags after divide setup"
    );
}
#[test]
fn lower_x86_byte_division_uses_ax_without_touching_rdx() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));

    for signed in [false, true] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            if signed {
                OpKind::DivS {
                    quot: rax,
                    rem: None,
                    src1: rax,
                    src2: SrcOperand::Reg(rcx),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                }
            } else {
                OpKind::DivU {
                    quot: rax,
                    rem: None,
                    src1: rax,
                    src2: SrcOperand::Reg(rcx),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                }
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.lower_function(&builder.finish()).unwrap();
        let code = lowerer.finalize().unwrap();
        let expected = if signed {
            [0xF6, 0xF9] // idiv cl
        } else {
            [0xF6, 0xF1] // div cl
        };
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "missing byte division for signed={signed}: {code:02X?}"
        );
        assert!(
            !code.windows(3).any(|window| window == [0x48, 0x31, 0xD2]),
            "byte division must not zero RDX: {code:02X?}"
        );
        assert!(
            !code.windows(3).any(|window| window == [0x99, 0xF6, 0xF9]),
            "byte division must not sign-extend through RDX: {code:02X?}"
        );
    }
}
#[test]
fn test_lower_div_signed() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

    let dividend = builder.alloc_vreg();
    let divisor = builder.alloc_vreg();
    let quotient = builder.alloc_vreg();

    // dividend = -100
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: dividend,
            src: SrcOperand::imm(-100i64),
            width: OpWidth::W64,
        },
    );

    // divisor = 7
    builder.push_op(
        0x1004,
        OpKind::Mov {
            dst: divisor,
            src: SrcOperand::imm(7),
            width: OpWidth::W64,
        },
    );

    // quotient = dividend / divisor (signed)
    builder.push_op(
        0x1008,
        OpKind::DivS {
            quot: quotient,
            rem: None,
            src1: dividend,
            src2: SrcOperand::Reg(divisor),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );

    builder.set_terminator(Terminator::Return {
        values: vec![quotient],
    });

    let func = builder.finish();

    let mut lowerer = X86_64Lowerer::new();
    let result = lowerer.lower_function(&func).unwrap();

    assert!(result.code_size > 0);
    let code = lowerer.finalize().unwrap();
    // Should contain CQO (48 99) and IDIV (F7 /7) instructions
    assert!(!code.is_empty());
}
#[test]
fn test_emit_div_instructions() {
    // Test DIV instruction encoding
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_div(PhysReg::Rcx, OpWidth::W64);
    }
    // DIV RCX = 48 F7 F1
    assert_eq!(buf.data(), &[0x48, 0xF7, 0xF1]);

    // Test IDIV instruction encoding
    let mut buf2 = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf2);
        emit.emit_idiv(PhysReg::Rbx, OpWidth::W64);
    }
    // IDIV RBX = 48 F7 FB
    assert_eq!(buf2.data(), &[0x48, 0xF7, 0xFB]);

    // Test CQO instruction encoding
    let mut buf3 = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf3);
        emit.emit_cqo();
    }
    // CQO = 48 99
    assert_eq!(buf3.data(), &[0x48, 0x99]);
}
