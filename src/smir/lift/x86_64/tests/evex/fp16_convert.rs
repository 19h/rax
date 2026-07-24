//! EVEX packed FP16 precision-conversion lifting tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_packed_fp16_precision_family_widths_masks_memory_er_sae_and_maps() {
    let vex = lift_single(&[0xC4, 0xE2, 0x79, 0x13, 0xC1]).unwrap();
    assert!(matches!(
        vex.ops.last().unwrap().kind,
        OpKind::X86PackedFpConvert {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            mask: None,
            from: VecElementType::F16,
            to: VecElementType::F32,
            lanes: 4,
            dst_width: VecWidth::V128,
            mask_zeroing: false,
            zero_upper: true,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            report_fp16_denormal: false,
        }
    ));
    let vex256 = lift_single(&[0xC4, 0xE2, 0x7D, 0x13, 0xC1]).unwrap();
    assert!(matches!(
        vex256.ops.last().unwrap().kind,
        OpKind::X86PackedFpConvert {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            from: VecElementType::F16,
            to: VecElementType::F32,
            lanes: 8,
            dst_width: VecWidth::V256,
            ..
        }
    ));

    for (bytes, from, to, dst, src, lanes, dst_width) in [
        (
            &[0x62, 0xF5, 0x7C, 0x09, 0x5A, 0xC8][..],
            VecElementType::F16,
            VecElementType::F64,
            X86Reg::Xmm(1),
            X86Reg::Xmm(0),
            2,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x09, 0x5A, 0xC8][..],
            VecElementType::F64,
            VecElementType::F16,
            X86Reg::Xmm(1),
            X86Reg::Xmm(0),
            2,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x09, 0x13, 0xC8][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Xmm(1),
            X86Reg::Xmm(0),
            4,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF6, 0x7D, 0x09, 0x13, 0xC8][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Xmm(1),
            X86Reg::Xmm(0),
            4,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x1D, 0xC8][..],
            VecElementType::F32,
            VecElementType::F16,
            X86Reg::Xmm(1),
            X86Reg::Xmm(0),
            4,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x29, 0x5A, 0xC8][..],
            VecElementType::F16,
            VecElementType::F64,
            X86Reg::Ymm(1),
            X86Reg::Xmm(0),
            4,
            VecWidth::V256,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x29, 0x5A, 0xC8][..],
            VecElementType::F64,
            VecElementType::F16,
            X86Reg::Xmm(1),
            X86Reg::Ymm(0),
            4,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x29, 0x13, 0xC8][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Ymm(1),
            X86Reg::Xmm(0),
            8,
            VecWidth::V256,
        ),
        (
            &[0x62, 0xF6, 0x7D, 0x29, 0x13, 0xC8][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Ymm(1),
            X86Reg::Xmm(0),
            8,
            VecWidth::V256,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x29, 0x1D, 0xC8][..],
            VecElementType::F32,
            VecElementType::F16,
            X86Reg::Xmm(1),
            X86Reg::Ymm(0),
            8,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x49, 0x5A, 0xC8][..],
            VecElementType::F16,
            VecElementType::F64,
            X86Reg::Zmm(1),
            X86Reg::Xmm(0),
            8,
            VecWidth::V512,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x49, 0x5A, 0xC8][..],
            VecElementType::F64,
            VecElementType::F16,
            X86Reg::Xmm(1),
            X86Reg::Zmm(0),
            8,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x49, 0x13, 0xC8][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Zmm(1),
            X86Reg::Ymm(0),
            16,
            VecWidth::V512,
        ),
        (
            &[0x62, 0xF6, 0x7D, 0x49, 0x13, 0xC8][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Zmm(1),
            X86Reg::Ymm(0),
            16,
            VecWidth::V512,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x49, 0x1D, 0xC8][..],
            VecElementType::F32,
            VecElementType::F16,
            X86Reg::Ymm(1),
            X86Reg::Zmm(0),
            16,
            VecWidth::V256,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86PackedFpConvert {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                src: VReg::Arch(ArchReg::X86(actual_src)),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                from: actual_from,
                to: actual_to,
                lanes: actual_lanes,
                dst_width: actual_width,
                mask_zeroing: false,
                zero_upper: true,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                report_fp16_denormal: actual_report,
            } if actual_dst == dst && actual_src == src && actual_from == from
                && actual_to == to && actual_lanes == lanes && actual_width == dst_width
                && actual_report == (from == VecElementType::F16
                    && to == VecElementType::F64)
        ));
    }

    for (bytes, from, to, dst, src, lanes, dst_width, report_fp16_denormal) in [
        (
            &[0x62, 0xA5, 0x7C, 0x4B, 0x5A, 0xD1][..],
            VecElementType::F16,
            VecElementType::F64,
            X86Reg::Zmm(18),
            X86Reg::Xmm(17),
            8,
            VecWidth::V512,
            true,
        ),
        (
            &[0x62, 0xA5, 0xFD, 0x4B, 0x5A, 0xD1][..],
            VecElementType::F64,
            VecElementType::F16,
            X86Reg::Xmm(18),
            X86Reg::Zmm(17),
            8,
            VecWidth::V128,
            false,
        ),
        (
            &[0x62, 0xA2, 0x7D, 0x4B, 0x13, 0xD1][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Zmm(18),
            X86Reg::Ymm(17),
            16,
            VecWidth::V512,
            false,
        ),
        (
            &[0x62, 0xA6, 0x7D, 0x4B, 0x13, 0xD1][..],
            VecElementType::F16,
            VecElementType::F32,
            X86Reg::Zmm(18),
            X86Reg::Ymm(17),
            16,
            VecWidth::V512,
            false,
        ),
        (
            &[0x62, 0xA5, 0x7D, 0x4B, 0x1D, 0xD1][..],
            VecElementType::F32,
            VecElementType::F16,
            X86Reg::Ymm(18),
            X86Reg::Zmm(17),
            16,
            VecWidth::V256,
            false,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86PackedFpConvert {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                src: VReg::Arch(ArchReg::X86(actual_src)),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
                from: actual_from,
                to: actual_to,
                lanes: actual_lanes,
                dst_width: actual_width,
                report_fp16_denormal: actual_report,
                ..
            } if actual_dst == dst && actual_src == src && actual_from == from
                && actual_to == to && actual_lanes == lanes && actual_width == dst_width
                && actual_report == report_fp16_denormal
        ));
    }

    for (bytes, expected_offset, pred_loads, report_fp16_denormal) in [
        // VCVTPH2PD full-memory tuple scales and broadcast tuple.
        (
            &[0x62, 0xF5, 0x7C, 0x09, 0x5A, 0x48, 0x08][..],
            32i64,
            2usize,
            true,
        ),
        (&[0x62, 0xF5, 0x7C, 0x29, 0x5A, 0x48, 0x08][..], 64, 4, true),
        (
            &[0x62, 0xF5, 0x7C, 0x49, 0x5A, 0x48, 0x08][..],
            128,
            8,
            true,
        ),
        (&[0x62, 0xF5, 0x7C, 0x19, 0x5A, 0x48, 0x08][..], 16, 1, true),
        // VCVTPD2PH full-memory tuple scales and broadcast tuple.
        (
            &[0x62, 0xF5, 0xFD, 0x09, 0x5A, 0x48, 0x08][..],
            128,
            2,
            false,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x29, 0x5A, 0x48, 0x08][..],
            256,
            4,
            false,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x49, 0x5A, 0x48, 0x08][..],
            512,
            8,
            false,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x19, 0x5A, 0x48, 0x08][..],
            64,
            1,
            false,
        ),
        // VCVTPH2PS full-memory tuple scales; EVEX.b is reserved.
        (
            &[0x62, 0xF2, 0x7D, 0x09, 0x13, 0x48, 0x08][..],
            64,
            4,
            false,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x29, 0x13, 0x48, 0x08][..],
            128,
            8,
            false,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x49, 0x13, 0x48, 0x08][..],
            256,
            16,
            false,
        ),
        // VCVTPH2PSX full-memory tuple scales and broadcast tuple.
        (
            &[0x62, 0xF6, 0x7D, 0x09, 0x13, 0x48, 0x08][..],
            64,
            4,
            false,
        ),
        (
            &[0x62, 0xF6, 0x7D, 0x29, 0x13, 0x48, 0x08][..],
            128,
            8,
            false,
        ),
        (
            &[0x62, 0xF6, 0x7D, 0x49, 0x13, 0x48, 0x08][..],
            256,
            16,
            false,
        ),
        (&[0x62, 0xF6, 0x7D, 0x19, 0x13, 0x48, 0x08][..], 16, 1, true),
        // VCVTPS2PHX full-memory tuple scales and broadcast tuple.
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x1D, 0x48, 0x08][..],
            128,
            4,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x29, 0x1D, 0x48, 0x08][..],
            256,
            8,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x49, 0x1D, 0x48, 0x08][..],
            512,
            16,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x19, 0x1D, 0x48, 0x08][..],
            32,
            1,
            false,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86PackedFpConvert {
                report_fp16_denormal: actual,
                ..
            } if actual == report_fp16_denormal
        ));
        assert!(
            result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::Lea {
                    addr: Address::BaseOffset {
                        offset,
                        disp_size: DispSize::Disp8,
                        ..
                    },
                    ..
                } if offset == expected_offset
            )) || result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset {
                        offset,
                        disp_size: DispSize::Disp8,
                        ..
                    },
                    ..
                } if offset == expected_offset
            ))
        );
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count(),
            pred_loads
        );
    }

    for (bytes, from, to, round) in [
        (
            // LLVM 21: vcvtph2pd zmm0, xmm0, {sae}.
            &[0x62, 0xF5, 0x7C, 0x18, 0x5A, 0xC0][..],
            VecElementType::F16,
            VecElementType::F64,
            FpRoundMode::Dynamic,
        ),
        (
            // LLVM 21: vcvtph2ps zmm0, ymm0, {sae}.
            &[0x62, 0xF2, 0x7D, 0x18, 0x13, 0xC0][..],
            VecElementType::F16,
            VecElementType::F32,
            FpRoundMode::Dynamic,
        ),
        (
            // LLVM 21: vcvtph2psx zmm0, ymm0, {sae}.
            &[0x62, 0xF6, 0x7D, 0x18, 0x13, 0xC0][..],
            VecElementType::F16,
            VecElementType::F32,
            FpRoundMode::Dynamic,
        ),
        (
            &[0x62, 0xF5, 0xFD, 0x59, 0x5A, 0xC0][..],
            VecElementType::F64,
            VecElementType::F16,
            FpRoundMode::RoundUp,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x59, 0x1D, 0xC0][..],
            VecElementType::F32,
            VecElementType::F16,
            FpRoundMode::RoundUp,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86PackedFpConvert {
                from: actual_from,
                to: actual_to,
                round: actual_round,
                suppress_exceptions: true,
                ..
            } if actual_from == from && actual_to == to && actual_round == round
        ));
    }

    for bytes in [
        &[0x62, 0xF5, 0xFD, 0x88, 0x5A, 0xC0][..], // {z} without a mask
        &[0x62, 0xF5, 0x7D, 0x09, 0x5A, 0xC0][..], // VCVTPD2PH W=0
        &[0x62, 0xF2, 0x7D, 0x19, 0x13, 0x00][..], // VCVTPH2PS has no broadcast
        &[0x62, 0xF5, 0x7C, 0x38, 0x5A, 0xC0][..], // widening SAE reserves L'L=01
        &[0x62, 0xF2, 0x7D, 0x58, 0x13, 0xC0][..], // widening SAE reserves L'L=10
        &[0x62, 0xF6, 0x7D, 0x78, 0x13, 0xC0][..], // widening SAE reserves L'L=11
        &[0x62, 0xF5, 0x75, 0x09, 0x1D, 0xC0][..], // reserved EVEX.vvvv
        &[0x62, 0xF6, 0x7D, 0x69, 0x13, 0xC0][..], // reserved EVEX.L'L=3
        &[0xC4, 0xE2, 0x71, 0x13, 0xC1][..],       // reserved VEX.vvvv
        &[0xC4, 0xE2, 0xF9, 0x13, 0xC1][..],       // VCVTPH2PS requires VEX.W=0
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
