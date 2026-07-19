//! evex::fp tests

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_evex_map6_fp16_fma_covers_all_families_masks_memory_broadcast_and_rounding() {
    for (order_bits, expected_order) in [
        (0x90u8, X86FmaOrder::Order132),
        (0xA0, X86FmaOrder::Order213),
        (0xB0, X86FmaOrder::Order231),
    ] {
        for (low, expected_kind) in [
            (0x06u8, X86FmaKind::AddSub),
            (0x07, X86FmaKind::SubAdd),
            (0x08, X86FmaKind::Add),
            (0x09, X86FmaKind::Add),
            (0x0A, X86FmaKind::Sub),
            (0x0B, X86FmaKind::Sub),
            (0x0C, X86FmaKind::NegativeMultiplyAdd),
            (0x0D, X86FmaKind::NegativeMultiplyAdd),
            (0x0E, X86FmaKind::NegativeMultiplySub),
            (0x0F, X86FmaKind::NegativeMultiplySub),
        ] {
            let bytes = [0x62, 0xF6, 0x7D, 0x09, order_bits | low, 0xC8];
            let lifted = lift_single(&bytes).unwrap();
            let scalar = matches!(low, 0x09 | 0x0B | 0x0D | 0x0F);
            assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86FP16Fma {
                    kind,
                    order,
                    round: FpRoundMode::Dynamic,
                    lanes,
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    ..
                } if kind == expected_kind
                    && order == expected_order
                    && lanes == if scalar { 1 } else { 8 }
            )));
        }
    }

    let packed = lift_single(&[0x62, 0xF6, 0x7D, 0x49, 0x98, 0xC8]).unwrap();
    assert!(packed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FP16Fma {
            dst: VReg::Virtual(_),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
            src3: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
            lanes: 32,
            ..
        }
    )));
    assert_eq!(
        packed
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::Select {
                    width: OpWidth::W16,
                    ..
                }
            ))
            .count(),
        32
    );

    let full_memory = lift_single(&[0x62, 0xF6, 0x7D, 0x49, 0x98, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF6, 0x7D, 0x59, 0xA8, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 2, .. },
            ..
        }
    )));

    let scalar_memory = lift_single(&[0x62, 0xF6, 0x7D, 0x09, 0xB9, 0x48, 0x7F]).unwrap();
    assert!(scalar_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 254, .. },
            width: MemWidth::B2,
            ..
        }
    )));

    for (p2, expected_round) in [
        (0x18, FpRoundMode::RoundNearest),
        (0x38, FpRoundMode::RoundDown),
        (0x58, FpRoundMode::RoundUp),
        (0x78, FpRoundMode::RoundTowardZero),
    ] {
        for opcode in [0x98u8, 0x99] {
            let lifted = lift_single(&[0x62, 0xF6, 0x7D, p2, opcode, 0xC8]).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86FP16Fma {
                    round,
                    lanes,
                    ..
                } if round == expected_round && lanes == if opcode == 0x99 { 1 } else { 32 }
            )));
        }
    }

    // Scalar LLIG is ignored when EVEX.b does not select embedded rounding.
    assert!(lift_single(&[0x62, 0xF6, 0x7D, 0x68, 0x99, 0xC8]).is_ok());
    for invalid in [
        &[0x62, 0xF6, 0xFD, 0x09, 0x98, 0xC8][..], // W1
        &[0x62, 0xF6, 0x7C, 0x09, 0x98, 0xC8][..], // missing 66
        &[0x62, 0xF6, 0x7D, 0x68, 0x98, 0xC8][..], // packed L'L=3
        &[0x62, 0xF6, 0x7D, 0x18, 0x99, 0x08][..], // scalar EVEX.b memory
        &[0x62, 0xF6, 0x7D, 0x88, 0x98, 0xC8][..], // {z} with K0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_fp16_complex_covers_algebra_variants_pairs_masks_memory_and_invalid_aliases() {
    for (pp_byte, conjugate) in [(0x6Eu8, false), (0x6F, true)] {
        for (opcode, scalar, accumulate) in [
            (0x56u8, false, true),
            (0x57, true, true),
            (0xD6, false, false),
            (0xD7, true, false),
        ] {
            let bytes = [0x62, 0xF6, pp_byte, 0x08, opcode, 0xCB];
            let lifted = lift_single(&bytes).unwrap();
            assert_eq!(lifted.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert!(matches!(
                lifted.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86FP16Complex {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                        width: VecWidth::V128,
                        pairs,
                        scalar: actual_scalar,
                        accumulate: actual_accumulate,
                        conjugate: actual_conjugate,
                        round: FpRoundMode::Dynamic,
                        ..
                    },
                    ..
                }] if *pairs == if scalar { 1 } else { 4 }
                    && *actual_scalar == scalar
                    && *actual_accumulate == accumulate
                    && *actual_conjugate == conjugate
            ));
        }
    }

    for (p2, expected_width, expected_pairs) in [
        (0x08u8, VecWidth::V128, 4),
        (0x28, VecWidth::V256, 8),
        (0x48, VecWidth::V512, 16),
    ] {
        let lifted = lift_single(&[0x62, 0xF6, 0x6E, p2, 0xD6, 0xCB]).unwrap();
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86FP16Complex {
                    width,
                    pairs,
                    scalar: false,
                    ..
                },
                ..
            }] if *width == expected_width && *pairs == expected_pairs
        ));
    }

    let full_memory = lift_single(&[0x62, 0xF6, 0x6E, 0x49, 0x56, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF6, 0x6F, 0x59, 0xD6, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));

    let scalar_memory = lift_single(&[0x62, 0xF6, 0x6E, 0x09, 0xD7, 0x48, 0x7F]).unwrap();
    assert!(scalar_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 508, .. },
            width: MemWidth::B4,
            ..
        }
    )));

    for (p2, expected_round) in [
        (0x12u8, FpRoundMode::RoundNearest),
        (0x32, FpRoundMode::RoundDown),
        (0x52, FpRoundMode::RoundUp),
        (0x72, FpRoundMode::RoundTowardZero),
    ] {
        let packed = lift_single(&[0x62, 0xA6, 0x6E, p2, 0x56, 0xCB]).unwrap();
        assert!(matches!(
            packed.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86FP16Complex {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                    width: VecWidth::V512,
                    pairs: 16,
                    round,
                    ..
                },
                ..
            }] if *round == expected_round
        ));
    }

    // Scalar LLIG is ignored unless EVEX.b selects embedded rounding.
    assert!(lift_single(&[0x62, 0xF6, 0x6E, 0x68, 0xD7, 0xCB]).is_ok());
    for invalid in [
        &[0x62, 0xF6, 0xEE, 0x08, 0xD6, 0xCB][..], // W1
        &[0x62, 0xF6, 0x6D, 0x08, 0xD6, 0xCB][..], // wrong mandatory prefix
        &[0x62, 0xF6, 0x6E, 0x68, 0xD6, 0xCB][..], // packed L'L=3
        &[0x62, 0xF6, 0x6E, 0x18, 0xD7, 0x08][..], // scalar EVEX.b memory
        &[0x62, 0xF6, 0x6E, 0x88, 0xD6, 0xCB][..], // {z} with K0
        &[0x62, 0xF6, 0x6E, 0x08, 0xD6, 0xD3][..], // destination aliases src1
        &[0x62, 0xF6, 0x6E, 0x08, 0xD6, 0xC9][..], // destination aliases src2
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_legacy_vex_evex_scalar_and_packed_sqrt() {
    let legacy = lift_single(&[0xF3, 0x0F, 0x51, 0xC1]).unwrap();
    assert!(legacy.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VUnary {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            elem: VecElementType::F32,
            lanes: 1,
            op: VecUnaryOp::FSqrt,
            ..
        }
    )));
    assert!(matches!(
        legacy.ops.last().unwrap().kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            ..
        }
    ));

    for (bytes, elem, lanes) in [
        (&[0x0F, 0x51, 0xC1][..], VecElementType::F32, 4),
        (&[0x66, 0x0F, 0x51, 0xC1][..], VecElementType::F64, 2),
        (&[0xC5, 0xF8, 0x51, 0xC1][..], VecElementType::F32, 4),
        (&[0xC5, 0xF9, 0x51, 0xC1][..], VecElementType::F64, 2),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VUnary {
                elem: actual_elem,
                lanes: actual_lanes,
                op: VecUnaryOp::FSqrt,
                ..
            } if actual_elem == elem && actual_lanes == lanes
        )));
    }
    let legacy_packed = lift_single(&[0x0F, 0x51, 0xC1]).unwrap();
    let computed = legacy_packed
        .ops
        .iter()
        .find_map(|op| match op.kind {
            OpKind::VUnary {
                dst,
                op: VecUnaryOp::FSqrt,
                ..
            } => Some(dst),
            _ => None,
        })
        .unwrap();
    assert!(matches!(computed, VReg::Virtual(_)));
    assert_eq!(
        legacy_packed
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    ..
                }
            ))
            .count(),
        4,
        "legacy packed SQRT must merge only the four XMM lanes"
    );

    let vex = lift_single(&[0xC5, 0xF2, 0x51, 0xC2]).unwrap();
    assert!(vex.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VUnary {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            elem: VecElementType::F32,
            lanes: 1,
            op: VecUnaryOp::FSqrt,
            ..
        }
    )));
    assert!(vex.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            lane: 3,
            ..
        }
    )));

    let evex = lift_single(&[0x62, 0xF1, 0x7E, 0x09, 0x51, 0xD1]).unwrap();
    assert!(evex.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::And {
            src1: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
            ..
        }
    )));
    assert!(evex.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VUnary {
            op: VecUnaryOp::FSqrt,
            lanes: 1,
            ..
        }
    )));
    assert!(
        evex.ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::Select { .. }))
    );

    let masked_memory = lift_single(&[0x62, 0xF1, 0x7E, 0x09, 0x51, 0x10]).unwrap();
    assert!(masked_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            width: MemWidth::B4,
            ..
        }
    )));
    let compressed_packed = lift_single(&[0x62, 0xF1, 0x7C, 0x48, 0x51, 0x50, 0x01]).unwrap();
    assert!(compressed_packed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            width: VecWidth::V512,
            ..
        }
    )));

    for (bytes, elem, lanes, dst) in [
        (
            &[0x62, 0xF1, 0x7C, 0x49, 0x51, 0xE0][..],
            VecElementType::F32,
            16,
            4,
        ), // VSQRTPS ZMM4{k1},ZMM0
        (
            &[0x62, 0xF1, 0xFD, 0xCA, 0x51, 0xF9][..],
            VecElementType::F64,
            8,
            7,
        ), // VSQRTPD ZMM7{k2}{z},ZMM1
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VUnary {
                dst: VReg::Virtual(_),
                src: VReg::Virtual(_),
                elem: actual_elem,
                lanes: actual_lanes,
                op: VecUnaryOp::FSqrt,
            } if actual_elem == elem && actual_lanes == lanes
        )));
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(actual_dst))),
                        elem: actual_elem,
                        ..
                    } if actual_dst == dst && actual_elem == elem
                ))
                .count(),
            usize::from(lanes),
            "masked packed square root must select every destination lane"
        );
    }

    for (bytes, width, lanes) in [
        (&[0x62, 0xF1, 0x7C, 0x49, 0x51, 0x10][..], MemWidth::B4, 16), // VSQRTPS ZMM2{k1},[RAX]
        (&[0x62, 0xF1, 0xFD, 0x4A, 0x51, 0x18][..], MemWidth::B8, 8),  // VSQRTPD ZMM3{k2},[RAX]
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: actual_width,
                        ..
                    } if actual_width == width
                ))
                .count(),
            lanes,
            "masked packed square root needs one fault-suppressing load per lane"
        );
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VLoad { .. } | OpKind::Load { .. }))
        );
    }

    let compressed_masked = lift_single(&[0x62, 0xF1, 0x7C, 0x49, 0x51, 0x50, 0x01]).unwrap();
    assert!(compressed_masked.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));
    for bytes in [
        &[0x62, 0xF1, 0xFC, 0x48, 0x51, 0xC1][..], // VSQRTPS with W=1
        &[0x62, 0xF1, 0x7D, 0x48, 0x51, 0xC1][..], // VSQRTPD with W=0
        &[0x62, 0xF1, 0x7C, 0xC8, 0x51, 0xC1][..], // VSQRTPS {z} with k0
        &[0x62, 0xF1, 0x7C, 0x68, 0x51, 0xC1][..], // VSQRTPS with reserved L'L=3
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
    for bytes in [
        &[0x62, 0xF1, 0x7C, 0x58, 0x51, 0x08][..], // VSQRTPS m32bcst
        &[0x62, 0xF1, 0x7C, 0x18, 0x51, 0xC1][..], // VSQRTPS embedded rounding
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Unsupported { .. })
        ));
    }
    assert!(matches!(
        lift_single(&[0xF0, 0xF3, 0x0F, 0x51, 0xC1]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_legacy_vex_evex_scalar_fp_to_integer_conversions() {
    for (bytes, dst, elem, width, truncate) in [
        (
            &[0xF3, 0x0F, 0x2D, 0xC1][..],
            X86Reg::Rax,
            VecElementType::F32,
            OpWidth::W32,
            false,
        ),
        (
            &[0xF2, 0x48, 0x0F, 0x2C, 0xC1][..],
            X86Reg::Rax,
            VecElementType::F64,
            OpWidth::W64,
            true,
        ),
        (
            &[0xC4, 0x61, 0xFA, 0x2D, 0xC9][..],
            X86Reg::R9,
            VecElementType::F32,
            OpWidth::W64,
            false,
        ),
        (
            &[0xC4, 0x61, 0xFB, 0x2C, 0xD1][..],
            X86Reg::R10,
            VecElementType::F64,
            OpWidth::W64,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86FpToInt {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                elem: actual_elem,
                int_width: actual_width,
                truncate: actual_truncate,
                ..
            } if actual_dst == dst && actual_elem == elem && actual_width == width
                && actual_truncate == truncate
        ));
    }

    let memory = lift_single(&[0xF2, 0x0F, 0x2D, 0x00]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B8,
            ..
        }
    )));
    assert!(matches!(
        memory.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            src: VReg::Virtual(_),
            elem: VecElementType::F64,
            ..
        }
    ));

    let evex_high = lift_single(&[0x62, 0x31, 0xFE, 0x08, 0x2D, 0xD1]).unwrap();
    assert!(matches!(
        evex_high.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::R10)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            elem: VecElementType::F32,
            int_width: OpWidth::W64,
            signed: true,
            truncate: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        }
    ));

    for (bytes, elem, width, round, sae) in [
        (
            &[0x62, 0xF1, 0xEE, 0x78, 0x7B, 0xC8][..],
            VecElementType::F32,
            OpWidth::W64,
            FpRoundMode::RoundTowardZero,
            true,
        ),
        (
            &[0x62, 0xF1, 0xEF, 0x38, 0x7B, 0xC8][..],
            VecElementType::F64,
            OpWidth::W64,
            FpRoundMode::RoundDown,
            true,
        ),
        (
            &[0x62, 0xF1, 0x6F, 0x58, 0x7B, 0xC8][..],
            VecElementType::F64,
            OpWidth::W32,
            FpRoundMode::Dynamic,
            false,
        ),
    ] {
        let unsigned = lift_single(bytes).unwrap();
        assert!(matches!(
            unsigned.ops.last().unwrap().kind,
            OpKind::X86IntToFp {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                elem: actual_elem,
                int_width,
                signed: false,
                round: actual_round,
                suppress_exceptions,
                zero_upper: true,
            } if actual_elem == elem
                && int_width == width
                && actual_round == round
                && suppress_exceptions == sae
        ));
    }

    let unsigned_memory = lift_single(&[0x62, 0xF1, 0x6E, 0x08, 0x7B, 0x48, 0x7F]).unwrap();
    assert!(unsigned_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 508, .. },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
            ..
        }
    )));

    let evex_er = lift_single(&[0x62, 0xF1, 0x7E, 0x38, 0x2D, 0xC3]).unwrap();
    assert!(matches!(
        evex_er.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            elem: VecElementType::F32,
            signed: true,
            truncate: false,
            round: FpRoundMode::RoundDown,
            suppress_exceptions: true,
            ..
        }
    ));

    for (bytes, elem, width, truncate, round, sae) in [
        (
            &[0x62, 0xF1, 0x7E, 0x08, 0x79, 0xC3][..],
            VecElementType::F32,
            OpWidth::W32,
            false,
            FpRoundMode::Dynamic,
            false,
        ),
        (
            &[0x62, 0xF1, 0xFF, 0x18, 0x78, 0xC3][..],
            VecElementType::F64,
            OpWidth::W64,
            true,
            FpRoundMode::RoundTowardZero,
            true,
        ),
    ] {
        let unsigned = lift_single(bytes).unwrap();
        assert!(matches!(
            unsigned.ops.last().unwrap().kind,
            OpKind::X86FpToInt {
                elem: actual_elem,
                int_width,
                signed: false,
                truncate: actual_truncate,
                round: actual_round,
                suppress_exceptions,
                ..
            } if actual_elem == elem
                && int_width == width
                && actual_truncate == truncate
                && actual_round == round
                && suppress_exceptions == sae
        ));
    }

    let unsigned_memory = lift_single(&[0x62, 0xF1, 0x7E, 0x08, 0x79, 0x40, 0x7F]).unwrap();
    assert!(unsigned_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 508, .. },
            width: MemWidth::B4,
            ..
        }
    )));

    for bytes in [
        &[0x0F, 0x2D, 0xC1][..],                   // missing F2/F3
        &[0xF0, 0xF3, 0x0F, 0x2D, 0xC1][..],       // LOCK
        &[0xC5, 0xF2, 0x2D, 0xC1][..],             // reserved VEX.vvvv
        &[0xC5, 0xFA, 0x79, 0xC1][..],             // unsigned is EVEX-only
        &[0x62, 0xE1, 0x7E, 0x08, 0x2D, 0xC1][..], // EVEX GPR R'
        &[0x62, 0xF1, 0x7E, 0x18, 0x79, 0x00][..], // EVEX.b memory
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_vex_evex_scalar_integer_to_fp_conversions() {
    for (bytes, dst, merge, src, elem, width, zero_upper) in [
        (
            &[0xF3, 0x0F, 0x2A, 0xC8][..],
            X86Reg::Xmm(1),
            X86Reg::Xmm(1),
            X86Reg::Rax,
            VecElementType::F32,
            OpWidth::W32,
            false,
        ),
        (
            &[0xF2, 0x48, 0x0F, 0x2A, 0xC8][..],
            X86Reg::Xmm(1),
            X86Reg::Xmm(1),
            X86Reg::Rax,
            VecElementType::F64,
            OpWidth::W64,
            false,
        ),
        (
            &[0xC4, 0xC1, 0xEA, 0x2A, 0xC9][..],
            X86Reg::Xmm(1),
            X86Reg::Xmm(2),
            X86Reg::R9,
            VecElementType::F32,
            OpWidth::W64,
            true,
        ),
        (
            &[0xC4, 0xC1, 0xE3, 0x2A, 0xD2][..],
            X86Reg::Xmm(2),
            X86Reg::Xmm(3),
            X86Reg::R10,
            VecElementType::F64,
            OpWidth::W64,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86IntToFp {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                merge: VReg::Arch(ArchReg::X86(actual_merge)),
                src: VReg::Arch(ArchReg::X86(actual_src)),
                elem: actual_elem,
                int_width: actual_width,
                signed: true,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: actual_zero,
            } if actual_dst == dst && actual_merge == merge && actual_src == src
                && actual_elem == elem && actual_width == width && actual_zero == zero_upper
        ));
    }

    let evex_high = lift_single(&[0x62, 0xC1, 0xFE, 0x00, 0x2A, 0xCA]).unwrap();
    assert!(matches!(
        evex_high.ops.last().unwrap().kind,
        OpKind::X86IntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
            src: VReg::Arch(ArchReg::X86(X86Reg::R10)),
            elem: VecElementType::F32,
            int_width: OpWidth::W64,
            signed: true,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            zero_upper: true,
        }
    ));

    for (bytes, width, offset) in [
        (
            &[0x62, 0xE1, 0x7E, 0x00, 0x2A, 0x48, 0x10][..],
            MemWidth::B4,
            64i64,
        ),
        (
            &[0x62, 0xE1, 0xFE, 0x00, 0x2A, 0x48, 0x08][..],
            MemWidth::B8,
            64i64,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset {
                    offset: actual_offset,
                    disp_size: DispSize::Disp8,
                    ..
                },
                width: actual_width,
                sign: SignExtend::Sign,
                ..
            } if actual_offset == offset && actual_width == width
        )));
    }

    for bytes in [
        &[0x0F, 0x2A, 0xC8][..],                   // missing F2/F3
        &[0xF0, 0xF3, 0x0F, 0x2A, 0xC8][..],       // LOCK
        &[0xC5, 0xEA, 0x7B, 0xC8][..],             // unsigned is EVEX-only
        &[0x62, 0xF1, 0x6E, 0x18, 0x7B, 0x00][..], // EVEX.b memory
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_vex_evex_scalar_fp_precision_conversions() {
    for (bytes, dst, merge, src, from, to, zero_upper) in [
        (
            &[0xF3, 0x0F, 0x5A, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            VecElementType::F32,
            VecElementType::F64,
            false,
        ),
        (
            &[0xF2, 0x0F, 0x5A, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            VecElementType::F64,
            VecElementType::F32,
            false,
        ),
        (
            &[0xC5, 0xF2, 0x5A, 0xC2][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            X86Reg::Xmm(2),
            VecElementType::F32,
            VecElementType::F64,
            true,
        ),
        (
            &[0xC5, 0xF3, 0x5A, 0xC2][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            X86Reg::Xmm(2),
            VecElementType::F64,
            VecElementType::F32,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86FpConvert {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                merge: VReg::Arch(ArchReg::X86(actual_merge)),
                src: VReg::Arch(ArchReg::X86(actual_src)),
                mask: None,
                from: actual_from,
                to: actual_to,
                mask_zeroing: false,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: actual_zero,
            } if actual_dst == dst && actual_merge == merge && actual_src == src
                && actual_from == from && actual_to == to && actual_zero == zero_upper
        ));
    }

    let high = lift_single(&[0x62, 0xA1, 0x7E, 0x00, 0x5A, 0xD1]).unwrap();
    assert!(matches!(
        high.ops.last().unwrap().kind,
        OpKind::X86FpConvert {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            mask: None,
            from: VecElementType::F32,
            to: VecElementType::F64,
            mask_zeroing: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            zero_upper: true,
        }
    ));

    let compressed = lift_single(&[0x62, 0xE1, 0xFF, 0x00, 0x5A, 0x50, 0x08]).unwrap();
    assert!(compressed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            width: MemWidth::B8,
            ..
        }
    )));

    for bytes in [
        &[0xF0, 0xF3, 0x0F, 0x5A, 0xC1][..],       // LOCK
        &[0x62, 0xF1, 0xFE, 0x00, 0x5A, 0xC1][..], // VCVTSS2SD W=1
        &[0x62, 0xF1, 0x7F, 0x00, 0x5A, 0xC1][..], // VCVTSD2SS W=0
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_evex_packed_fp_arithmetic_covers_ops_masks_e4_broadcast_high_regs_and_invalids() {
    for (bytes, expected) in [
        (&[0x62, 0xA1, 0x7C, 0xC3, 0x58, 0xCA][..], "add"),
        (&[0x62, 0xF1, 0xED, 0x2A, 0x59, 0xCB][..], "mul"),
        (&[0x62, 0xF1, 0x6C, 0xC9, 0x5C, 0x48, 0x02][..], "sub"),
        (&[0x62, 0xF1, 0xED, 0x59, 0x5E, 0x48, 0x08][..], "div"),
        (&[0x62, 0xF1, 0x7C, 0x49, 0x5D, 0xCB][..], "min"),
        (&[0x62, 0xF1, 0xFD, 0x49, 0x5F, 0xCB][..], "max"),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(lifted.ops.iter().any(|op| match (&op.kind, expected) {
            (OpKind::VAdd { .. }, "add")
            | (OpKind::VMul { .. }, "mul")
            | (OpKind::VSub { .. }, "sub")
            | (OpKind::VDiv { .. }, "div") => true,
            (OpKind::VX86MinMax { min: true, .. }, "min")
            | (OpKind::VX86MinMax { min: false, .. }, "max") => true,
            _ => false,
        }));
    }

    let high = lift_single(&[0x62, 0xA1, 0x7C, 0xC3, 0x58, 0xCA]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VAdd {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            elem: VecElementType::F32,
            lanes: 16,
            ..
        }
    )));
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            elem: VecElementType::F32,
            lane: 15,
            ..
        }
    )));

    let full_memory = lift_single(&[0x62, 0xF1, 0x6C, 0xC9, 0x5C, 0x48, 0x02]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset {
                offset: 128,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF1, 0xED, 0x59, 0x5E, 0x48, 0x08]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        8
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));

    for bytes in [
        &[0x62, 0xF1, 0xFC, 0x48, 0x58, 0xC1][..], // VADDPS requires W0
        &[0x62, 0xF1, 0x7D, 0x48, 0x59, 0xC1][..], // VMULPD requires W1
        &[0x62, 0xF1, 0x7C, 0x88, 0x5C, 0xC1][..], // {z} requires a mask
        &[0x62, 0xF1, 0x7C, 0x68, 0x5E, 0xC1][..], // L'L=3 without ER
        &[0x62, 0xF1, 0x7C, 0x38, 0x5D, 0xC1][..], // ER requires VL=512
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_evex_packed_fp_precision_masks_broadcast_rounding_and_high_registers() {
    for (bytes, dst, src, mask, from, to, lanes, dst_width, zeroing, round) in [
        (
            &[0x62, 0xF1, 0x7C, 0xC9, 0x5A, 0xC1][..],
            X86Reg::Zmm(0),
            X86Reg::Ymm(1),
            X86Reg::K(1),
            VecElementType::F32,
            VecElementType::F64,
            8,
            VecWidth::V512,
            true,
            FpRoundMode::Dynamic,
        ),
        (
            &[0x62, 0xF1, 0xFD, 0xCC, 0x5A, 0xEE][..],
            X86Reg::Ymm(5),
            X86Reg::Zmm(6),
            X86Reg::K(4),
            VecElementType::F64,
            VecElementType::F32,
            8,
            VecWidth::V256,
            true,
            FpRoundMode::Dynamic,
        ),
        (
            &[0x62, 0xA1, 0x7C, 0x4B, 0x5A, 0xD1][..],
            X86Reg::Zmm(18),
            X86Reg::Ymm(17),
            X86Reg::K(3),
            VecElementType::F32,
            VecElementType::F64,
            8,
            VecWidth::V512,
            false,
            FpRoundMode::Dynamic,
        ),
        (
            &[0x62, 0xA1, 0xFD, 0xCD, 0x5A, 0xDC][..],
            X86Reg::Ymm(19),
            X86Reg::Zmm(20),
            X86Reg::K(5),
            VecElementType::F64,
            VecElementType::F32,
            8,
            VecWidth::V256,
            true,
            FpRoundMode::Dynamic,
        ),
        (
            &[0x62, 0xF1, 0xFD, 0x39, 0x5A, 0xC1][..],
            X86Reg::Ymm(0),
            X86Reg::Zmm(1),
            X86Reg::K(1),
            VecElementType::F64,
            VecElementType::F32,
            8,
            VecWidth::V256,
            false,
            FpRoundMode::RoundDown,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86PackedFpConvert {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                src: VReg::Arch(ArchReg::X86(actual_src)),
                mask: Some(VReg::Arch(ArchReg::X86(actual_mask))),
                from: actual_from,
                to: actual_to,
                lanes: actual_lanes,
                dst_width: actual_width,
                mask_zeroing: actual_zeroing,
                zero_upper: true,
                round: actual_round,
                suppress_exceptions: actual_suppress,
                report_fp16_denormal: false,
            } if actual_dst == dst && actual_src == src && actual_mask == mask
                && actual_from == from && actual_to == to && actual_lanes == lanes
                && actual_width == dst_width && actual_zeroing == zeroing
                && actual_round == round
                && actual_suppress == (round != FpRoundMode::Dynamic)
        ));
    }

    for (bytes, expected_offset, pred_loads) in [
        (
            &[0x62, 0xF1, 0x7C, 0x4B, 0x5A, 0x50, 0x02][..],
            64i64,
            8usize,
        ),
        (&[0x62, 0xF1, 0x7C, 0xDA, 0x5A, 0x60, 0x08][..], 32, 1),
        (&[0x62, 0xF1, 0xFD, 0x4D, 0x5A, 0x78, 0x02][..], 128, 8),
        (&[0x62, 0xF1, 0xFD, 0xDA, 0x5A, 0x48, 0x08][..], 64, 1),
    ] {
        let result = lift_single(bytes).unwrap();
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

    for (byte4, expected_round) in [
        (0x19, FpRoundMode::RoundNearest),
        (0x39, FpRoundMode::RoundDown),
        (0x59, FpRoundMode::RoundUp),
        (0x79, FpRoundMode::RoundTowardZero),
    ] {
        let result = lift_single(&[0x62, 0xF1, 0xFD, byte4, 0x5A, 0xC1]).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86PackedFpConvert {
                lanes: 8,
                round,
                ..
            } if round == expected_round
        ));
    }

    for bytes in [
        &[0x62, 0xF1, 0x7C, 0xC8, 0x5A, 0xC1][..], // {z} with k0
        &[0x62, 0xF1, 0xFC, 0x48, 0x5A, 0xC1][..], // VCVTPS2PD W=1
        &[0x62, 0xF1, 0x7D, 0x48, 0x5A, 0xC1][..], // VCVTPD2PS W=0
        &[0x62, 0xF1, 0x7C, 0x58, 0x5A, 0xC1][..], // widening EVEX.b reg
        &[0x62, 0xF1, 0x7C, 0x68, 0x5A, 0xC1][..], // reserved EVEX.L'L=3
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
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
            &[0x62, 0xF5, 0x7C, 0x59, 0x5A, 0xC0][..],
            VecElementType::F16,
            VecElementType::F64,
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
        &[0x62, 0xF5, 0x7C, 0x19, 0x5A, 0xC0][..], // packed SAE requires VL=512
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
#[test]
fn lift_evex_fp_class_covers_widths_masks_daz_fault_suppression_and_invalids() {
    for (p2, width) in [
        (0x08, VecWidth::V128),
        (0x28, VecWidth::V256),
        (0x48, VecWidth::V512),
    ] {
        for (p1, elem) in [
            (0x7C, VecElementType::F16),
            (0x7D, VecElementType::F32),
            (0xFD, VecElementType::F64),
        ] {
            let bytes = [0x62, 0xF3, p1, p2, 0x66, 0xD1, 0xFF];
            let result = lift_single(&bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len());
            if elem == VecElementType::F16 {
                assert!(
                    !result
                        .ops
                        .iter()
                        .any(|op| matches!(op.kind, OpKind::X86VectorFpCompare { .. }))
                );
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::VCmp {
                        elem: VecElementType::I16,
                        lanes,
                        ..
                    } if u32::from(lanes) == width.lanes(elem)
                )));
            } else {
                assert!(result.ops.iter().any(|op| matches!(
                    op.kind,
                    OpKind::X86VectorFpCompare {
                        elem: actual,
                        width: actual_width,
                        lanes,
                        predicate: 0,
                        suppress_exceptions: true,
                        ..
                    } if actual == elem
                        && actual_width == width
                        && u32::from(lanes) == width.lanes(elem)
                )));
            }
            assert!(matches!(
                result.ops.last().map(|op| &op.kind),
                Some(OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(2))),
                    width: OpWidth::W64,
                    ..
                })
            ));
        }
    }

    let high = lift_single(&[0x62, 0xB3, 0x7D, 0x4B, 0x66, 0xD1, 0xFF]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86VectorFpCompare {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            elem: VecElementType::F32,
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(matches!(
        high.ops.last().map(|op| &op.kind),
        Some(OpKind::And {
            dst: VReg::Arch(ArchReg::X86(X86Reg::K(2))),
            src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
            flags: FlagUpdate::None,
            ..
        })
    ));

    // Type E4 masking suppresses each inactive broadcast access. The
    // compressed displacement is scaled by the 8-byte broadcast tuple.
    let broadcast = lift_single(&[0x62, 0xF3, 0xFD, 0x5D, 0x66, 0x60, 0x01, 0x20]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset {
                offset: 8,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        8
    );

    let fp16_broadcast = lift_single(&[0x62, 0xF3, 0x7C, 0x5D, 0x66, 0x60, 0x01, 0x20]).unwrap();
    assert_eq!(
        fp16_broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );

    let scalar = lift_single(&[0x62, 0xF3, 0x7D, 0x0F, 0x67, 0x70, 0x01, 0x7F]).unwrap();
    assert!(scalar.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        scalar.ops.last().map(|op| &op.kind),
        Some(OpKind::And {
            dst: VReg::Arch(ArchReg::X86(X86Reg::K(6))),
            src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(7)))),
            ..
        })
    ));

    // LLIG scalar forms accept every encoded L'L value and still classify
    // exactly one F32/F64 lane.
    for p2 in [0x08, 0x28, 0x48, 0x68] {
        let result = lift_single(&[0x62, 0xF3, 0xFD, p2, 0x67, 0xCA, 0x80]).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86VectorFpCompare {
                elem: VecElementType::F64,
                width: VecWidth::V128,
                lanes: 1,
                ..
            }
        )));
    }

    for bytes in [
        &[0x62, 0xF3, 0x7E, 0x08, 0x66, 0xC8, 0][..], // pp invalid
        &[0x62, 0xF3, 0xFC, 0x08, 0x66, 0xC8, 0][..], // FP16 W=1
        &[0x62, 0xF3, 0x71, 0x08, 0x66, 0xC8, 0][..], // vvvv reserved
        &[0x62, 0xF3, 0x7D, 0x00, 0x66, 0xC8, 0][..], // V' reserved
        &[0x62, 0xF3, 0x7D, 0x88, 0x66, 0xC8, 0][..], // EVEX.z reserved
        &[0x62, 0xF3, 0x7D, 0x68, 0x66, 0xC8, 0][..], // packed L'L=3
        &[0x62, 0xF3, 0x7D, 0x58, 0x66, 0xC8, 0][..], // packed b register
        &[0x62, 0xE3, 0x7D, 0x08, 0x66, 0xC8, 0][..], // destination k8+
        &[0x62, 0xF3, 0x7D, 0x18, 0x67, 0x08, 0][..], // scalar EVEX.b
        &[0x62, 0xF3, 0x7D, 0x08, 0x66][..],          // missing ModR/M
        &[0x62, 0xF3, 0x7D, 0x08, 0x66, 0xC8][..],    // missing imm8
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid EVEX FPCLASS accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_evex_map5_fp16_arithmetic_covers_register_memory_and_broadcast_forms() {
    for (bytes, expected_op) in [
        (&[0x62, 0xF5, 0x6C, 0xCC, 0x58, 0xCB][..], Avx10FP16Op::Add),
        (&[0x62, 0xA5, 0x74, 0x27, 0x59, 0xC2][..], Avx10FP16Op::Mul),
        (&[0x62, 0xD5, 0x3C, 0x8B, 0x5C, 0xF9][..], Avx10FP16Op::Sub),
        (&[0x62, 0xF5, 0x6C, 0x08, 0x5D, 0xCB][..], Avx10FP16Op::Min),
        (&[0x62, 0xF5, 0x54, 0x48, 0x5E, 0xE6][..], Avx10FP16Op::Div),
        (&[0x62, 0xF5, 0x6C, 0x29, 0x5F, 0xCB][..], Avx10FP16Op::Max),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert_eq!(lifted.ops.len(), 1);
        assert!(matches!(
            lifted.ops[0].kind,
            OpKind::VFP16Arith { op, .. } if op == expected_op
        ));
    }
    let add = lift_single(&[0x62, 0xF5, 0x6C, 0xCC, 0x58, 0xCB]).unwrap();
    assert!(matches!(
        add.ops[0].kind,
        OpKind::VFP16Arith {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
            op: Avx10FP16Op::Add,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V512,
            zeroing: true,
        }
    ));

    let full_memory = lift_single(&[0x62, 0xF5, 0x6C, 0x48, 0x58, 0x48, 0x01]).unwrap();
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset { offset: 64, .. },
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(matches!(
        full_memory.ops.last().map(|op| &op.kind),
        Some(OpKind::VFP16Arith {
            src2: VReg::Virtual(_),
            mask: None,
            op: Avx10FP16Op::Add,
            width: VecWidth::V512,
            ..
        })
    ));

    for (p2, width, tuple_bytes, lanes) in [
        (0x08, VecWidth::V128, 16, 8),
        (0x28, VecWidth::V256, 32, 16),
        (0x48, VecWidth::V512, 64, 32),
    ] {
        let full = lift_single(&[0x62, 0xF5, 0x6C, p2, 0x58, 0x48, 0x01]).unwrap();
        assert!(full.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset, .. },
                width: actual_width,
                ..
            } if offset == tuple_bytes && actual_width == width
        )));

        let broadcast = lift_single(&[0x62, 0xF5, 0x6C, p2 | 0x10, 0x59, 0x48, 0x01]).unwrap();
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset { offset: 2, .. },
                width: MemWidth::B2,
                ..
            }
        )));
        assert!(broadcast.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::F16,
                lanes: actual_lanes,
                ..
            } if actual_lanes == lanes
        )));
    }

    let broadcast = lift_single(&[0x62, 0xF5, 0x6C, 0x58, 0x59, 0x48, 0x01]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 2, .. },
            width: MemWidth::B2,
            ..
        }
    )));
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            elem: VecElementType::F16,
            lanes: 32,
            ..
        }
    )));

    let masked_full = lift_single(&[0x62, 0xF5, 0x6C, 0x4C, 0x5C, 0x48, 0x01]).unwrap();
    assert!(masked_full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));
    assert_eq!(
        masked_full
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );
    assert!(matches!(
        masked_full.ops.last().map(|op| &op.kind),
        Some(OpKind::VFP16Arith {
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
            op: Avx10FP16Op::Sub,
            ..
        })
    ));

    let masked_broadcast = lift_single(&[0x62, 0xF5, 0x6C, 0x5C, 0x5E, 0x48, 0x01]).unwrap();
    assert!(masked_broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 2, .. },
            ..
        }
    )));
    assert_eq!(
        masked_broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 0, .. },
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );

    let min_broadcast = lift_single(&[0x62, 0xF5, 0x6C, 0x5A, 0x5D, 0x48, 0x01]).unwrap();
    assert_eq!(
        min_broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );
    assert!(matches!(
        min_broadcast.ops.last().map(|op| &op.kind),
        Some(OpKind::VFP16Arith {
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
            op: Avx10FP16Op::Min,
            width: VecWidth::V512,
            ..
        })
    ));

    let scalar_max = lift_single(&[0x62, 0xF5, 0x6E, 0x09, 0x5F, 0x48, 0x7F]).unwrap();
    assert!(scalar_max.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 254, .. },
            width: MemWidth::B2,
            ..
        }
    )));
    assert!(scalar_max.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VFP16Arith {
            op: Avx10FP16Op::Max,
            width: VecWidth::V128,
            ..
        }
    )));

    for (p2, round) in [
        (0x18, FpRoundMode::RoundNearest),
        (0x38, FpRoundMode::RoundDown),
        (0x58, FpRoundMode::RoundUp),
        (0x78, FpRoundMode::RoundTowardZero),
    ] {
        let embedded = lift_single(&[0x62, 0xF5, 0x6C, p2, 0x58, 0xCB]).unwrap();
        assert!(matches!(
            embedded.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VFP16Arith {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                    mask: None,
                    round: actual_round,
                    width: VecWidth::V512,
                    ..
                },
                ..
            }] if *actual_round == round
        ));
    }
    let masked_embedded = lift_single(&[0x62, 0xF5, 0x6C, 0xDC, 0x58, 0xCB]).unwrap();
    assert!(matches!(
        masked_embedded.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VFP16Arith {
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
                round: FpRoundMode::RoundUp,
                width: VecWidth::V512,
                zeroing: true,
                ..
            },
            ..
        }]
    ));
    let min_sae = lift_single(&[0x62, 0xF5, 0x6C, 0x18, 0x5D, 0xCB]).unwrap();
    assert!(matches!(
        min_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VFP16Arith {
                op: Avx10FP16Op::Min,
                round: FpRoundMode::RoundNearest,
                width: VecWidth::V512,
                ..
            },
            ..
        }]
    ));
    let scalar_max_sae = lift_single(&[0x62, 0xF5, 0x6E, 0x78, 0x5F, 0xCB]).unwrap();
    assert!(scalar_max_sae.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VFP16Arith {
            op: Avx10FP16Op::Max,
            round: FpRoundMode::RoundTowardZero,
            ..
        }
    )));

    for invalid in [
        &[0x62, 0xF5, 0xEC, 0x4C, 0x58, 0xCB][..], // W=1
        &[0x62, 0xF5, 0x6C, 0xEC, 0x58, 0xCB][..], // L'L=3 without EVEX.b
        &[0x62, 0xF5, 0x6C, 0x78, 0x58, 0x08][..], // L'L=3 memory broadcast
        &[0x62, 0xF5, 0x6C, 0xC8, 0x58, 0xCB][..], // {z} without mask
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_fp16_sqrt_covers_packed_scalar_masks_rounding_and_fault_suppression() {
    let packed = lift_single(&[0x62, 0xA5, 0x7C, 0xCB, 0x51, 0xCB]).unwrap();
    assert_eq!(packed.bytes_consumed, 6);
    assert!(matches!(
        packed.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VFP16Arith {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
                op: Avx10FP16Op::Sqrt,
                round: FpRoundMode::Dynamic,
                width: VecWidth::V512,
                zeroing: true,
            },
            ..
        }]
    ));

    for (bytes, width) in [
        (&[0x62, 0xF5, 0x7C, 0x08, 0x51, 0xCB][..], VecWidth::V128),
        (&[0x62, 0x55, 0x7C, 0x28, 0x51, 0xC2][..], VecWidth::V256),
        (&[0x62, 0xF5, 0x7C, 0x48, 0x51, 0xCB][..], VecWidth::V512),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::VFP16Arith {
                op: Avx10FP16Op::Sqrt,
                width: actual,
                ..
            }) if *actual == width
        ));
    }

    let full = lift_single(&[0x62, 0xF5, 0x7C, 0x48, 0x51, 0x48, 0x01]).unwrap();
    assert!(full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset { offset: 64, .. },
            width: VecWidth::V512,
            ..
        }
    )));

    // A broadcast memory operand is one scalar read gated by the OR of all
    // architectural lane-mask bits, not one observable read per lane.
    let broadcast = lift_single(&[0x62, 0xF5, 0x7C, 0x5A, 0x51, 0x48, 0x20]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 64, .. },
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        1,
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            elem: VecElementType::F16,
            lanes: 32,
            ..
        }
    )));

    let packed_round = lift_single(&[0x62, 0xF5, 0x7C, 0x9C, 0x51, 0xCB]).unwrap();
    assert!(matches!(
        packed_round.ops.last().map(|op| &op.kind),
        Some(OpKind::VFP16Arith {
            round: FpRoundMode::RoundNearest,
            width: VecWidth::V512,
            ..
        })
    ));

    let scalar = lift_single(&[0x62, 0xA5, 0x6E, 0x83, 0x51, 0xCB]).unwrap();
    assert_eq!(scalar.bytes_consumed, 6);
    assert!(scalar.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VFP16Arith {
            dst: VReg::Virtual(_),
            src1: VReg::Virtual(_),
            src2: VReg::Virtual(_),
            mask: None,
            op: Avx10FP16Op::Sqrt,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            zeroing: false,
        }
    )));
    assert_eq!(
        scalar
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                    elem: VecElementType::F16,
                    lane: 1..=7,
                    ..
                }
            ))
            .count(),
        7,
    );
    assert!(scalar.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            elem: VecElementType::F16,
            lane: 7,
            ..
        }
    )));

    let scalar_memory = lift_single(&[0x62, 0xF5, 0x6E, 0x0A, 0x51, 0x48, 0x7F]).unwrap();
    assert_eq!(
        scalar_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        1,
    );

    let scalar_round = lift_single(&[0x62, 0xF5, 0x6E, 0x3C, 0x51, 0xCB]).unwrap();
    assert!(scalar_round.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VFP16Arith {
            round: FpRoundMode::RoundDown,
            ..
        }
    )));
    // LLIG is ignored when EVEX.b does not encode a rounding control.
    assert!(lift_single(&[0x62, 0xA5, 0x6E, 0x63, 0x51, 0xCB]).is_ok());

    for invalid in [
        &[0x62, 0xF5, 0xFC, 0x48, 0x51, 0xCB][..], // packed W=1
        &[0x62, 0xF5, 0x7C, 0x68, 0x51, 0xCB][..], // packed L'L=3
        &[0x62, 0xF5, 0x74, 0x48, 0x51, 0xCB][..], // packed reserved vvvv
        &[0x62, 0xF5, 0x7C, 0x40, 0x51, 0xCB][..], // packed reserved V'
        &[0x62, 0xF5, 0x7C, 0x88, 0x51, 0xCB][..], // packed {z} with k0
        &[0x62, 0xF5, 0x7D, 0x48, 0x51, 0xCB][..], // packed pp != NP
        &[0x62, 0xF5, 0xEE, 0x08, 0x51, 0xCB][..], // scalar W=1
        &[0x62, 0xF5, 0x6E, 0x18, 0x51, 0x08][..], // scalar EVEX.b memory
        &[0x62, 0xF5, 0x6E, 0x88, 0x51, 0xCB][..], // scalar {z} with k0
        &[0x62, 0xF5, 0x6F, 0x08, 0x51, 0xCB][..], // scalar pp != F3
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_fp16_flag_compare_covers_signaling_sae_high_regs_memory_and_invalids() {
    for (bytes, signaling) in [
        (&[0x62, 0xF5, 0x7C, 0x08, 0x2E, 0xD3][..], false),
        (&[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0xD3][..], true),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().unwrap().kind,
            OpKind::X86FpCompare {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                elem: VecElementType::F16,
                signaling: actual,
            } if actual == signaling
        ));
    }

    let high = lift_single(&[0x62, 0xA5, 0x7C, 0x08, 0x2E, 0xD3]).unwrap();
    assert!(matches!(
        high.ops.last().unwrap().kind,
        OpKind::X86FpCompare {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
            elem: VecElementType::F16,
            signaling: false,
        }
    ));

    let memory = lift_single(&[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0x50, 0x7F]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 254, .. },
            width: MemWidth::B2,
            ..
        }
    )));
    assert!(matches!(
        memory.ops.last().unwrap().kind,
        OpKind::X86FpCompare {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            elem: VecElementType::F16,
            signaling: true,
            ..
        }
    ));

    assert!(lift_single(&[0x62, 0xF5, 0x7C, 0x18, 0x2E, 0xD3]).is_ok()); // {sae}
    assert!(lift_single(&[0x62, 0xF5, 0x7C, 0x68, 0x2E, 0xD3]).is_ok()); // LLIG

    for invalid in [
        &[0x62, 0xF5, 0xFC, 0x08, 0x2E, 0xD3][..], // W=1
        &[0x62, 0xF5, 0x7E, 0x08, 0x2E, 0xD3][..], // pp != NP
        &[0x62, 0xF5, 0x6C, 0x08, 0x2E, 0xD3][..], // reserved vvvv
        &[0x62, 0xF5, 0x7C, 0x00, 0x2E, 0xD3][..], // reserved V'
        &[0x62, 0xF5, 0x7C, 0x0A, 0x2E, 0xD3][..], // reserved opmask
        &[0x62, 0xF5, 0x7C, 0x88, 0x2E, 0xD3][..], // reserved zeroing
        &[0x62, 0xF5, 0x7C, 0x18, 0x2E, 0x10][..], // EVEX.b memory
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_fp16_to_int_covers_signedness_rounding_widths_and_invalids() {
    for (bytes, signed, truncate) in [
        (&[0x62, 0xF5, 0x7E, 0x08, 0x2D, 0xC3][..], true, false),
        (&[0x62, 0xF5, 0x7E, 0x08, 0x2C, 0xC3][..], true, true),
        (&[0x62, 0xF5, 0x7E, 0x08, 0x79, 0xC3][..], false, false),
        (&[0x62, 0xF5, 0x7E, 0x08, 0x78, 0xC3][..], false, true),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().unwrap().kind,
            OpKind::X86FpToInt {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                elem: VecElementType::F16,
                int_width: OpWidth::W32,
                signed: actual_signed,
                truncate: actual,
                round,
                suppress_exceptions: false,
            } if actual_signed == signed
                && actual == truncate
                && round == if truncate {
                    FpRoundMode::RoundTowardZero
                } else {
                    FpRoundMode::Dynamic
                }
        ));
    }

    let high = lift_single(&[0x62, 0x35, 0xFE, 0x08, 0x2D, 0xC3]).unwrap();
    assert!(matches!(
        high.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
            elem: VecElementType::F16,
            int_width: OpWidth::W64,
            signed: true,
            truncate: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        }
    ));

    let memory = lift_single(&[0x62, 0xF5, 0x7E, 0x08, 0x79, 0x40, 0x7F]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 254, .. },
            width: MemWidth::B2,
            ..
        }
    )));
    assert!(matches!(
        memory.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            src: VReg::Virtual(_),
            elem: VecElementType::F16,
            signed: false,
            ..
        }
    ));

    for (l_bits, expected) in [
        (0u8, FpRoundMode::RoundNearest),
        (1, FpRoundMode::RoundDown),
        (2, FpRoundMode::RoundUp),
        (3, FpRoundMode::RoundTowardZero),
    ] {
        let b3 = 0x18 | (l_bits << 5);
        let rounded = lift_single(&[0x62, 0xF5, 0x7E, b3, 0x79, 0xC3]).unwrap();
        assert!(matches!(
            rounded.ops.last().unwrap().kind,
            OpKind::X86FpToInt {
                signed: false,
                round,
                suppress_exceptions: true,
                ..
            } if round == expected
        ));
    }

    let trunc_sae = lift_single(&[0x62, 0xF5, 0x7E, 0x78, 0x78, 0xC3]).unwrap();
    assert!(matches!(
        trunc_sae.ops.last().unwrap().kind,
        OpKind::X86FpToInt {
            signed: false,
            truncate: true,
            round: FpRoundMode::RoundTowardZero,
            suppress_exceptions: true,
            ..
        }
    ));
    assert!(lift_single(&[0x62, 0xF5, 0x7E, 0x68, 0x2D, 0xC3]).is_ok()); // LLIG

    for invalid in [
        &[0x62, 0xF5, 0x7C, 0x08, 0x2D, 0xC3][..], // pp != F3
        &[0x62, 0xF5, 0x6E, 0x08, 0x2D, 0xC3][..], // reserved vvvv
        &[0x62, 0xF5, 0x7E, 0x00, 0x2D, 0xC3][..], // reserved V'
        &[0x62, 0xF5, 0x7E, 0x09, 0x2D, 0xC3][..], // reserved opmask
        &[0x62, 0xF5, 0x7E, 0x88, 0x2D, 0xC3][..], // reserved zeroing
        &[0x62, 0xE5, 0x7E, 0x08, 0x2D, 0xC3][..], // no GPR bit 4
        &[0x62, 0xF5, 0x7E, 0x18, 0x2D, 0x00][..], // EVEX.b memory
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_int_to_fp16_covers_signedness_rounding_merge_memory_and_invalids() {
    for (bytes, signed, width) in [
        (
            &[0x62, 0xF5, 0x6E, 0x08, 0x2A, 0xC8][..],
            true,
            OpWidth::W32,
        ),
        (
            &[0x62, 0xF5, 0xEE, 0x08, 0x7B, 0xC8][..],
            false,
            OpWidth::W64,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().unwrap().kind,
            OpKind::X86IntToFp {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                elem: VecElementType::F16,
                int_width,
                signed: actual_signed,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: true,
            } if actual_signed == signed && int_width == width
        ));
    }

    let high = lift_single(&[0x62, 0xC5, 0xE6, 0x00, 0x7B, 0xC8]).unwrap();
    assert!(matches!(
        high.ops.last().unwrap().kind,
        OpKind::X86IntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
            src: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            elem: VecElementType::F16,
            int_width: OpWidth::W64,
            signed: false,
            ..
        }
    ));

    let memory = lift_single(&[0x62, 0xF5, 0xEE, 0x08, 0x7B, 0x48, 0x7F]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 1016, .. },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
            ..
        }
    )));

    for (l_bits, expected) in [
        (0u8, FpRoundMode::RoundNearest),
        (1, FpRoundMode::RoundDown),
        (2, FpRoundMode::RoundUp),
        (3, FpRoundMode::RoundTowardZero),
    ] {
        let b3 = 0x18 | (l_bits << 5);
        let rounded = lift_single(&[0x62, 0xF5, 0xEE, b3, 0x7B, 0xC8]).unwrap();
        assert!(matches!(
            rounded.ops.last().unwrap().kind,
            OpKind::X86IntToFp {
                signed: false,
                round,
                suppress_exceptions: true,
                ..
            } if round == expected
        ));
    }

    assert!(lift_single(&[0x62, 0xF5, 0x6E, 0x68, 0x2A, 0xC8]).is_ok()); // LLIG
    for invalid in [
        &[0x62, 0xF5, 0x6C, 0x08, 0x2A, 0xC8][..], // pp != F3
        &[0x62, 0xF5, 0x6E, 0x09, 0x2A, 0xC8][..], // reserved opmask
        &[0x62, 0xF5, 0x6E, 0x88, 0x2A, 0xC8][..], // reserved zeroing
        &[0x62, 0xB5, 0x6E, 0x08, 0x7B, 0xC8][..], // no GPR bit 4
        &[0x62, 0xF5, 0x6E, 0x18, 0x7B, 0x00][..], // EVEX.b memory
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_packed_int_to_fp16_covers_all_types_widths_masks_broadcast_er_and_invalids() {
    for (bytes, elem, signed, lanes, src_width, dst_width, round, sae) in [
        (
            &[0x62, 0xF5, 0x7C, 0x8A, 0x5B, 0xCB][..],
            VecElementType::I32,
            true,
            4,
            VecWidth::V128,
            VecWidth::V64,
            FpRoundMode::Dynamic,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x2A, 0x5B, 0xCB][..],
            VecElementType::I32,
            true,
            8,
            VecWidth::V256,
            VecWidth::V128,
            FpRoundMode::Dynamic,
            false,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x5A, 0x5B, 0xCB][..],
            VecElementType::I32,
            true,
            16,
            VecWidth::V512,
            VecWidth::V256,
            FpRoundMode::RoundUp,
            true,
        ),
        (
            &[0x62, 0xA5, 0xFC, 0xBB, 0x5B, 0xCA][..],
            VecElementType::I64,
            true,
            8,
            VecWidth::V512,
            VecWidth::V128,
            FpRoundMode::RoundDown,
            true,
        ),
        (
            &[0x62, 0xF5, 0x7F, 0xAD, 0x7A, 0xE6][..],
            VecElementType::I32,
            false,
            8,
            VecWidth::V256,
            VecWidth::V128,
            FpRoundMode::Dynamic,
            false,
        ),
        (
            &[0x62, 0xF5, 0xFF, 0x7D, 0x7A, 0xE6][..],
            VecElementType::I64,
            false,
            8,
            VecWidth::V512,
            VecWidth::V128,
            FpRoundMode::RoundTowardZero,
            true,
        ),
        (
            &[0x62, 0xD5, 0x7E, 0x9E, 0x7D, 0xF8][..],
            VecElementType::I16,
            true,
            32,
            VecWidth::V512,
            VecWidth::V512,
            FpRoundMode::RoundNearest,
            true,
        ),
        (
            &[0x62, 0xD5, 0x7F, 0x2E, 0x7D, 0xF8][..],
            VecElementType::I16,
            false,
            16,
            VecWidth::V256,
            VecWidth::V256,
            FpRoundMode::Dynamic,
            false,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::X86PackedIntToFp16 {
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(_)))),
                int_elem,
                signed: actual_signed,
                lanes: actual_lanes,
                src_width: actual_src_width,
                dst_width: actual_dst_width,
                round: actual_round,
                suppress_exceptions,
                zero_upper: true,
                ..
            }) if *int_elem == elem
                && *actual_signed == signed
                && *actual_lanes == lanes
                && *actual_src_width == src_width
                && *actual_dst_width == dst_width
                && *actual_round == round
                && *suppress_exceptions == sae
        ));
    }

    let high = lift_single(&[0x62, 0xA5, 0xFC, 0xBB, 0x5B, 0xCA]).unwrap();
    assert!(matches!(
        high.ops.last().map(|op| &op.kind),
        Some(OpKind::X86PackedIntToFp16 {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
            mask_zeroing: true,
            ..
        })
    ));

    let full = lift_single(&[0x62, 0xF5, 0x7C, 0x8A, 0x5B, 0x48, 0x7F]).unwrap();
    assert!(full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 2032, .. },
            ..
        }
    )));
    assert_eq!(
        full.ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4,
    );

    let broadcast = lift_single(&[0x62, 0xF5, 0x7C, 0x9A, 0x5B, 0x48, 0x7F]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 508, .. },
            ..
        }
    )));
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 0, .. },
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4,
    );

    for invalid in [
        &[0x62, 0xF5, 0x74, 0x08, 0x5B, 0xCB][..], // reserved vvvv
        &[0x62, 0xF5, 0x7C, 0x00, 0x5B, 0xCB][..], // reserved V'
        &[0x62, 0xF5, 0x7C, 0x88, 0x5B, 0xCB][..], // {z} with k0
        &[0x62, 0xF5, 0x7C, 0x68, 0x5B, 0xCB][..], // reserved L'L=3 without ER
        &[0x62, 0xF5, 0x7F, 0x08, 0x5B, 0xCB][..], // unsupported pp/opcode pair
        &[0x62, 0xF5, 0x7C, 0x08, 0x7A, 0xCB][..], // VCVTUDQ2PH pp != F2
        &[0x62, 0xF5, 0xFF, 0x08, 0x7D, 0xCB][..], // word conversion W=1
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_packed_fp16_to_int_covers_all_families_tuples_masks_er_sae_and_invalids() {
    for (bytes, elem, signed, truncate, lanes, src_width) in [
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x5B, 0xC8][..],
            VecElementType::I32,
            true,
            false,
            4,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7E, 0x09, 0x5B, 0xC8][..],
            VecElementType::I32,
            true,
            true,
            4,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x7B, 0xC8][..],
            VecElementType::I64,
            true,
            false,
            2,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x7A, 0xC8][..],
            VecElementType::I64,
            true,
            true,
            2,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x09, 0x79, 0xC8][..],
            VecElementType::I32,
            false,
            false,
            4,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x09, 0x78, 0xC8][..],
            VecElementType::I32,
            false,
            true,
            4,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x79, 0xC8][..],
            VecElementType::I64,
            false,
            false,
            2,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x78, 0xC8][..],
            VecElementType::I64,
            false,
            true,
            2,
            VecWidth::V64,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x7D, 0xC8][..],
            VecElementType::I16,
            true,
            false,
            8,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x09, 0x7C, 0xC8][..],
            VecElementType::I16,
            true,
            true,
            8,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x09, 0x7D, 0xC8][..],
            VecElementType::I16,
            false,
            false,
            8,
            VecWidth::V128,
        ),
        (
            &[0x62, 0xF5, 0x7C, 0x09, 0x7C, 0xC8][..],
            VecElementType::I16,
            false,
            true,
            8,
            VecWidth::V128,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::X86PackedFp16ToInt {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                int_elem,
                signed: actual_signed,
                truncate: actual_truncate,
                lanes: actual_lanes,
                src_width: actual_src_width,
                dst_width: VecWidth::V128,
                round,
                suppress_exceptions: false,
                zero_upper: true,
                ..
            }) if *int_elem == elem
                && *actual_signed == signed
                && *actual_truncate == truncate
                && *actual_lanes == lanes
                && *actual_src_width == src_width
                && *round == if truncate { FpRoundMode::RoundTowardZero } else { FpRoundMode::Dynamic }
        ));
    }

    for (bytes, truncate, round) in [
        (
            &[0x62, 0xF5, 0x7D, 0x18, 0x5B, 0xCA][..],
            false,
            FpRoundMode::RoundNearest,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x38, 0x7B, 0xDC][..],
            false,
            FpRoundMode::RoundDown,
        ),
        (
            &[0x62, 0xF5, 0x7D, 0x58, 0x7D, 0xEE][..],
            false,
            FpRoundMode::RoundUp,
        ),
        (
            &[0x62, 0xF5, 0x7E, 0x18, 0x5B, 0xCA][..],
            true,
            FpRoundMode::RoundTowardZero,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::X86PackedFp16ToInt {
                dst_width: VecWidth::V512,
                truncate: actual_truncate,
                round: actual_round,
                suppress_exceptions: true,
                ..
            }) if *actual_truncate == truncate && *actual_round == round
        ));
    }

    let full = lift_single(&[0x62, 0xF5, 0x7D, 0x09, 0x7B, 0x48, 0x7F]).unwrap();
    assert!(full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 508, .. },
            ..
        }
    )));
    assert_eq!(
        full.ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        2
    );

    let broadcast = lift_single(&[0x62, 0xF5, 0x7D, 0x19, 0x7B, 0x48, 0x7F]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        1
    );

    for invalid in [
        &[0x62, 0xF5, 0x75, 0x09, 0x5B, 0xC8][..], // reserved vvvv
        &[0x62, 0xF5, 0x7D, 0x01, 0x5B, 0xC8][..], // reserved V'
        &[0x62, 0xF5, 0x7D, 0x88, 0x5B, 0xC8][..], // {z} with k0
        &[0x62, 0xF5, 0x7D, 0x68, 0x5B, 0xC8][..], // reserved L'L=3 without ER
        &[0x62, 0xF5, 0xFD, 0x09, 0x5B, 0xC8][..], // W=1
        &[0x62, 0xF5, 0x7F, 0x09, 0x5B, 0xC8][..], // unsupported pp/opcode pair
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_fp16_scalar_arithmetic_covers_ops_masks_rounding_aliases_and_memory() {
    for (bytes, expected) in [
        (&[0x62, 0xF5, 0x6E, 0x08, 0x58, 0xCB][..], Avx10FP16Op::Add),
        (&[0x62, 0xF5, 0x56, 0x8A, 0x59, 0xE6][..], Avx10FP16Op::Mul),
        (&[0x62, 0xA5, 0x6E, 0x03, 0x5C, 0xCB][..], Avx10FP16Op::Sub),
        (&[0x62, 0xF5, 0x6E, 0x0A, 0x5E, 0xCB][..], Avx10FP16Op::Div),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VFP16Arith {
                dst: VReg::Virtual(_),
                src1: VReg::Virtual(_),
                src2: VReg::Virtual(_),
                mask: None,
                op,
                round: FpRoundMode::Dynamic,
                width: VecWidth::V128,
                zeroing: false,
            } if op == expected
        )));
    }

    let high = lift_single(&[0x62, 0xA5, 0x6E, 0x03, 0x5C, 0xCB]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            lane: 0,
            elem: VecElementType::F16,
            ..
        }
    )));
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
            lane: 0,
            elem: VecElementType::F16,
            ..
        }
    )));
    assert_eq!(
        high.ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                    lane: 1..=7,
                    elem: VecElementType::F16,
                    ..
                }
            ))
            .count(),
        7,
    );
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            lane: 7,
            elem: VecElementType::F16,
            ..
        }
    )));

    let memory = lift_single(&[0x62, 0xF5, 0x6E, 0x0A, 0x5E, 0x48, 0x7F]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 254, .. },
            width: MemWidth::B2,
            ..
        }
    )));
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Mov {
            src: SrcOperand::Imm(0x3C00),
            width: OpWidth::W16,
            ..
        }
    )));

    for (p2, round) in [
        (0x1C, FpRoundMode::RoundNearest),
        (0x3C, FpRoundMode::RoundDown),
        (0x5C, FpRoundMode::RoundUp),
        (0x7C, FpRoundMode::RoundTowardZero),
    ] {
        let lifted = lift_single(&[0x62, 0xF5, 0x6E, p2, 0x58, 0xCB]).unwrap();
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VFP16Arith {
                round: actual,
                ..
            } if actual == round
        )));
    }
    // LLIG does not constrain L'L unless EVEX.b carries rounding control.
    assert!(lift_single(&[0x62, 0xF5, 0x6E, 0x68, 0x59, 0xCB]).is_ok());

    for invalid in [
        &[0x62, 0xF5, 0xEE, 0x08, 0x58, 0xCB][..], // W=1
        &[0x62, 0xF5, 0x6E, 0x18, 0x59, 0x08][..], // EVEX.b memory
        &[0x62, 0xF5, 0x6E, 0x88, 0x5C, 0xCB][..], // {z} with k0
        &[0x62, 0xF5, 0x6F, 0x08, 0x5E, 0xCB][..], // pp != F3
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_unaligned_fp_moves_covers_masks_stores_high_regs_and_invalids() {
    for (bytes, elem, lanes, dst) in [
        (
            &[0x62, 0xF1, 0x7C, 0x49, 0x10, 0xD1][..],
            VecElementType::F32,
            16,
            X86Reg::Zmm(2),
        ),
        (
            &[0x62, 0xF1, 0xFD, 0xCA, 0x10, 0xD1][..],
            VecElementType::F64,
            8,
            X86Reg::Zmm(2),
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(
            lifted
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        elem: actual_elem,
                        ..
                    } if actual_dst == dst && actual_elem == elem
                ))
                .count(),
            lanes
        );
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Shr {
                src: VReg::Arch(ArchReg::X86(X86Reg::K(_))),
                ..
            }
        )));
    }

    let high = lift_single(&[0x62, 0xA1, 0x7C, 0x49, 0x10, 0xC8]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
            elem: VecElementType::F32,
            ..
        }
    )));
    assert_eq!(
        high.ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    elem: VecElementType::F32,
                    ..
                }
            ))
            .count(),
        16
    );

    for (bytes, width, lanes) in [
        (&[0x62, 0xF1, 0x7C, 0x49, 0x10, 0x10][..], MemWidth::B4, 16),
        (&[0x62, 0xF1, 0xFD, 0x4A, 0x10, 0x10][..], MemWidth::B8, 8),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(
            lifted
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: actual_width,
                        ..
                    } if actual_width == width
                ))
                .count(),
            lanes
        );
        assert!(
            !lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VLoad { .. } | OpKind::Load { .. }))
        );
    }

    for (bytes, width, lanes) in [
        (&[0x62, 0xF1, 0x7C, 0x49, 0x11, 0x08][..], MemWidth::B4, 16),
        (&[0x62, 0xF1, 0xFD, 0x4A, 0x11, 0x08][..], MemWidth::B8, 8),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(
            lifted
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredStore {
                        width: actual_width,
                        ..
                    } if actual_width == width
                ))
                .count(),
            lanes
        );
        assert!(
            !lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VStore { .. } | OpKind::Store { .. }))
        );
    }

    let register_store = lift_single(&[0x62, 0xF1, 0x7C, 0xC9, 0x11, 0xD1]).unwrap();
    assert!(register_store.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            elem: VecElementType::F32,
            ..
        }
    )));
    assert_eq!(
        register_store
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    elem: VecElementType::F32,
                    ..
                }
            ))
            .count(),
        16
    );

    let high_store = lift_single(&[0x62, 0xC1, 0xFD, 0x4C, 0x11, 0x29]).unwrap();
    assert!(high_store.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R9))),
            ..
        }
    )));
    assert_eq!(
        high_store
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(21))),
                    elem: VecElementType::F64,
                    ..
                }
            ))
            .count(),
        8
    );

    for bytes in [
        &[0x62, 0xF1, 0x7C, 0x49, 0x10, 0x50, 0x01][..],
        &[0x62, 0xF1, 0x7C, 0x49, 0x11, 0x48, 0x01][..],
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Lea {
                addr: Address::BaseOffset {
                    offset: 64,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            }
        )));
    }

    for bytes in [
        &[0x62, 0xF1, 0x7C, 0xC8, 0x10, 0xC1][..], // {z} with k0
        &[0x62, 0xF1, 0x7C, 0xC9, 0x11, 0x08][..], // {z} memory store
        &[0x62, 0xF1, 0xFC, 0x48, 0x10, 0xC1][..], // VMOVUPS with W=1
        &[0x62, 0xF1, 0x7D, 0x48, 0x10, 0xC1][..], // VMOVUPD with W=0
        &[0x62, 0xF1, 0x7C, 0x68, 0x10, 0xC1][..], // reserved L'L=3
        &[0x62, 0xF1, 0x7C, 0x58, 0x10, 0xC1][..], // reserved EVEX.b
        &[0x62, 0xF1, 0x74, 0x48, 0x10, 0xC1][..], // reserved vvvv
        &[0x62, 0xF1, 0x7C, 0x40, 0x10, 0xC1][..], // reserved V'
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid masked VMOVUP* encoding accepted: {bytes:02X?}"
        );
    }
}
#[test]
fn lift_legacy_vex_evex_fp_unpack_family() {
    for (bytes, elem, width, high) in [
        (
            &[0x0F, 0x14, 0xCA][..],
            VecElementType::F32,
            VecWidth::V128,
            false,
        ),
        (
            &[0x66, 0x0F, 0x15, 0x18][..],
            VecElementType::F64,
            VecWidth::V128,
            true,
        ),
        (
            &[0xC5, 0xEC, 0x14, 0xCB][..],
            VecElementType::F32,
            VecWidth::V256,
            false,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        let expected_first = if high { width.lanes(elem) as u8 / 2 } else { 0 };
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Mov { src: SrcOperand::Imm(index), .. } if index == i64::from(expected_first)
        )));
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VShuffle { elem: actual, lanes, .. }
                if actual == elem && lanes == width.lanes(elem) as u8
        )));
    }
    let evex = lift_single(&[0x62, 0xA1, 0xED, 0x83, 0x15, 0xCB]).unwrap();
    assert!(evex.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VShuffle {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            src2: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(19)))),
            elem: VecElementType::F64,
            ..
        }
    )));
    let broadcast = lift_single(&[0x62, 0xE1, 0x6C, 0x53, 0x14, 0x48, 0x10]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 64, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(
        !broadcast
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );
    for bytes in [
        &[0xF3, 0x0F, 0x14, 0xC1][..],
        &[0xF0, 0x66, 0x0F, 0x15, 0xC1][..],
        &[0x62, 0xF1, 0x6C, 0x18, 0x14, 0xC1][..],
        &[0x62, 0xF1, 0xEC, 0x08, 0x14, 0xC1][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_vex_evex_fp_compare_family() {
    for (bytes, elem, lanes, predicate, scalar, zero_upper) in [
        (
            &[0x0F, 0xC2, 0xD1, 0x07][..],
            VecElementType::F32,
            4,
            7,
            false,
            false,
        ),
        (
            &[0x66, 0x0F, 0xC2, 0xEC, 0x03][..],
            VecElementType::F64,
            2,
            3,
            false,
            false,
        ),
        (
            &[0xF3, 0x0F, 0xC2, 0x18, 0x01][..],
            VecElementType::F32,
            1,
            1,
            true,
            false,
        ),
        (
            &[0xF2, 0x0F, 0xC2, 0xF7, 0x06][..],
            VecElementType::F64,
            1,
            6,
            true,
            false,
        ),
        (
            &[0xC5, 0xEC, 0xC2, 0xCB, 0x1F][..],
            VecElementType::F32,
            8,
            31,
            false,
            true,
        ),
        (
            &[0xC5, 0xEB, 0xC2, 0xCB, 0x11][..],
            VecElementType::F64,
            1,
            17,
            true,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86VectorFpCompare {
                elem: actual_elem,
                lanes: actual_lanes,
                predicate: actual_predicate,
                scalar: actual_scalar,
                mask_destination: false,
                zero_upper: actual_zero_upper,
                ..
            } if actual_elem == elem
                && actual_lanes == lanes
                && actual_predicate == predicate
                && actual_scalar == scalar
                && actual_zero_upper == zero_upper
        )));
    }

    let high = lift_single(&[0x62, 0xB1, 0x6C, 0x40, 0xC2, 0xCB, 0x0E]).unwrap();
    assert!(matches!(
        high.ops.last().unwrap().kind,
        OpKind::X86VectorFpCompare {
            dst: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            predicate: 14,
            mask_destination: true,
            suppress_exceptions: false,
            ..
        }
    ));

    for (bytes, scalar, width, lanes) in [
        (
            &[0x62, 0xF3, 0x7C, 0x09, 0xC2, 0xC8, 0x03][..],
            false,
            VecWidth::V128,
            8,
        ),
        (
            &[0x62, 0xF3, 0x7E, 0x69, 0xC2, 0xC8, 0x03][..],
            true,
            VecWidth::V128,
            1,
        ),
    ] {
        let fp16 = lift_single(bytes).unwrap();
        assert_eq!(fp16.bytes_consumed, bytes.len());
        assert!(matches!(
            fp16.ops.last().unwrap().kind,
            OpKind::X86VectorFpCompare {
                dst: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                elem: VecElementType::F16,
                width: actual_width,
                lanes: actual_lanes,
                predicate: 3,
                scalar: actual_scalar,
                mask_destination: true,
                suppress_exceptions: false,
                ..
            } if actual_scalar == scalar && actual_width == width && actual_lanes == lanes
        ));
    }

    let fp16_high = lift_single(&[0x62, 0xB3, 0x6C, 0x40, 0xC2, 0xCB, 0x0E]).unwrap();
    assert!(matches!(
        fp16_high.ops.last().unwrap().kind,
        OpKind::X86VectorFpCompare {
            dst: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
            elem: VecElementType::F16,
            width: VecWidth::V512,
            lanes: 32,
            predicate: 14,
            ..
        }
    ));

    let fp16_broadcast = lift_single(&[0x62, 0xF3, 0x7C, 0x19, 0xC2, 0x08, 0x03]).unwrap();
    assert_eq!(
        fp16_broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        8
    );
    assert!(matches!(
        fp16_broadcast.ops.last().unwrap().kind,
        OpKind::X86VectorFpCompare {
            elem: VecElementType::F16,
            width: VecWidth::V128,
            lanes: 8,
            ..
        }
    ));

    for bytes in [
        &[0x62, 0xF3, 0x7C, 0x18, 0xC2, 0xC8, 0x03][..],
        &[0x62, 0xF3, 0x7E, 0x78, 0xC2, 0xC8, 0x03][..],
    ] {
        let fp16_sae = lift_single(bytes).unwrap();
        assert!(matches!(
            fp16_sae.ops.last().unwrap().kind,
            OpKind::X86VectorFpCompare {
                elem: VecElementType::F16,
                suppress_exceptions: true,
                ..
            }
        ));
    }

    let masked_broadcast = lift_single(&[0x62, 0xF1, 0x6C, 0x52, 0xC2, 0x58, 0x10, 0x03]).unwrap();
    assert_eq!(
        masked_broadcast
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .count(),
        16
    );
    assert!(masked_broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));
    assert!(masked_broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        masked_broadcast.ops.last().unwrap().kind,
        OpKind::X86VectorFpCompare {
            dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
            predicate: 3,
            ..
        }
    ));

    for bytes in [
        &[0x62, 0xB1, 0x6E, 0x12, 0xC2, 0xDB, 0x05][..],
        &[0x62, 0xB1, 0xEF, 0x15, 0xC2, 0xE3, 0x08][..],
        &[0x62, 0xB1, 0x6C, 0x10, 0xC2, 0xCB, 0x09][..],
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(matches!(
            lifted.ops.last().unwrap().kind,
            OpKind::X86VectorFpCompare {
                suppress_exceptions: true,
                mask_destination: true,
                ..
            }
        ));
    }

    for bytes in [
        &[0xF0, 0x0F, 0xC2, 0xC1, 0][..],             // LOCK
        &[0x0F, 0xC2, 0xC1][..],                      // missing immediate
        &[0x0F, 0xC2, 0xC1, 8][..],                   // reserved legacy predicate
        &[0xC5, 0xF0, 0xC2, 0xC1, 0x20][..],          // reserved VEX predicate
        &[0x62, 0xF1, 0x6C, 0x88, 0xC2, 0xC1, 0][..], // EVEX.z
        &[0x62, 0xF1, 0x6C, 0x38, 0xC2, 0xC1, 0][..], // packed SAE with nonzero L'L
        &[0x62, 0xF1, 0x6E, 0x1A, 0xC2, 0x18, 0][..], // scalar memory EVEX.b
        &[0xC4, 0xE3, 0x78, 0xC2, 0xC1, 0][..],       // FP16 compare is EVEX-only
        &[0x62, 0xF3, 0x7D, 0x08, 0xC2, 0xC1, 0][..], // FP16 pp=66
        &[0x62, 0xF3, 0xFC, 0x08, 0xC2, 0xC1, 0][..], // FP16 W=1
        &[0x62, 0xF3, 0x7C, 0x88, 0xC2, 0xC1, 0][..], // FP16 EVEX.z
        &[0x62, 0xF3, 0x7C, 0x38, 0xC2, 0xC1, 0][..], // FP16 packed SAE, L'L!=0
        &[0x62, 0xF3, 0x7E, 0x18, 0xC2, 0x01, 0][..], // FP16 scalar memory EVEX.b
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid FP compare encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_evex_reduce_covers_all_formats_controls_masks_sae_and_memory() {
    for (bytes, elem, width, lanes, scalar, imm) in [
        (
            &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x53][..],
            VecElementType::F32,
            VecWidth::V128,
            4,
            false,
            0x53,
        ),
        (
            &[0x62, 0xF3, 0xFD, 0x28, 0x56, 0xCB, 0xA7][..],
            VecElementType::F64,
            VecWidth::V256,
            4,
            false,
            0xA7,
        ),
        (
            &[0x62, 0xF3, 0x7C, 0x48, 0x56, 0xCB, 0xB9][..],
            VecElementType::F16,
            VecWidth::V512,
            32,
            false,
            0xB9,
        ),
        (
            &[0x62, 0xF3, 0x6D, 0x08, 0x57, 0xCB, 0x4D][..],
            VecElementType::F32,
            VecWidth::V128,
            1,
            true,
            0x4D,
        ),
        (
            &[0x62, 0xF3, 0xED, 0x08, 0x57, 0xCB, 0x21][..],
            VecElementType::F64,
            VecWidth::V128,
            1,
            true,
            0x21,
        ),
        (
            &[0x62, 0xF3, 0x6C, 0x08, 0x57, 0xCB, 0x10][..],
            VecElementType::F16,
            VecWidth::V128,
            1,
            true,
            0x10,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::X86Reduce {
                elem: actual_elem,
                width: actual_width,
                lanes: actual_lanes,
                imm: actual_imm,
                scalar: actual_scalar,
                suppress_exceptions: false,
                ..
            }) if *actual_elem == elem
                && *actual_width == width
                && *actual_lanes == lanes
                && *actual_imm == imm
                && *actual_scalar == scalar
        ));
    }

    let packed_sae = lift_single(&[0x62, 0xA3, 0x7C, 0x9A, 0x56, 0xCB, 0xB9]).unwrap();
    assert!(matches!(
        packed_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Reduce {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                elem: VecElementType::F16,
                width: VecWidth::V512,
                lanes: 32,
                imm: 0xB9,
                scalar: false,
                mask_zeroing: true,
                suppress_exceptions: true,
            },
            x86_hint: Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::None,
                opcode: 0x56,
                width: VecWidth::V512,
                w: false,
            }),
            ..
        }]
    ));

    let scalar_sae = lift_single(&[0x62, 0xA3, 0x6D, 0x92, 0x57, 0xCB, 0x4D]).unwrap();
    assert!(matches!(
        scalar_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Reduce {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                elem: VecElementType::F32,
                imm: 0x4D,
                scalar: true,
                mask_zeroing: true,
                suppress_exceptions: true,
                ..
            },
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF3, 0x7D, 0x4A, 0x56, 0x48, 0x01, 0x53]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF3, 0x7C, 0x5A, 0x56, 0x48, 0x01, 0x33]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .count(),
        1
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 2, .. },
            width: MemWidth::B2,
            ..
        }
    )));

    let scalar_memory = lift_single(&[0x62, 0xF3, 0x6D, 0x8A, 0x57, 0x48, 0x01, 0x4D]).unwrap();
    assert!(scalar_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        scalar_memory.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Reduce {
            merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
            src: VReg::Virtual(_),
            mask_zeroing: true,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF3, 0x7E, 0x08, 0x56, 0xCB, 0x53][..], // pp=F3
        &[0x62, 0xF3, 0xFC, 0x08, 0x56, 0xCB, 0x53][..], // FP16 W=1
        &[0x62, 0xF3, 0x75, 0x08, 0x56, 0xCB, 0x53][..], // packed reserved vvvv
        &[0x62, 0xF3, 0x7D, 0x00, 0x56, 0xCB, 0x53][..], // packed reserved V'
        &[0x62, 0xF3, 0x7D, 0x68, 0x56, 0xCB, 0x53][..], // packed L'L=3
        &[0x62, 0xF3, 0x6D, 0x18, 0x57, 0x08, 0x4D][..], // scalar EVEX.b memory
        &[0x62, 0xF3, 0x6D, 0x88, 0x57, 0xCB, 0x4D][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
    assert!(matches!(
        lift_single(&[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB]),
        Err(LiftError::Incomplete { .. })
    ));
}
#[test]
fn lift_evex_range_covers_formats_controls_masks_sae_and_memory() {
    for (bytes, elem, width, lanes, scalar, imm) in [
        (
            &[0x62, 0xF3, 0x6D, 0x08, 0x50, 0xCB, 0x05][..],
            VecElementType::F32,
            VecWidth::V128,
            4,
            false,
            0x05,
        ),
        (
            &[0x62, 0xF3, 0xED, 0x28, 0x50, 0xCB, 0x0D][..],
            VecElementType::F64,
            VecWidth::V256,
            4,
            false,
            0x0D,
        ),
        (
            &[0x62, 0xF3, 0x6D, 0x48, 0x50, 0xCB, 0x02][..],
            VecElementType::F32,
            VecWidth::V512,
            16,
            false,
            0x02,
        ),
        (
            &[0x62, 0xF3, 0x6D, 0x08, 0x51, 0xCB, 0x05][..],
            VecElementType::F32,
            VecWidth::V128,
            1,
            true,
            0x05,
        ),
        (
            &[0x62, 0xF3, 0xED, 0x08, 0x51, 0xCB, 0x0D][..],
            VecElementType::F64,
            VecWidth::V128,
            1,
            true,
            0x0D,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::X86Range {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1) | X86Reg::Ymm(1) | X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2) | X86Reg::Ymm(2) | X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3) | X86Reg::Ymm(3) | X86Reg::Zmm(3))),
                elem: actual_elem,
                width: actual_width,
                lanes: actual_lanes,
                imm: actual_imm,
                scalar: actual_scalar,
                suppress_exceptions: false,
                ..
            }) if *actual_elem == elem
                && *actual_width == width
                && *actual_lanes == lanes
                && *actual_imm == imm
                && *actual_scalar == scalar
        ));
    }

    let packed_sae = lift_single(&[0x62, 0xA3, 0x6D, 0x92, 0x50, 0xCB, 0x0F]).unwrap();
    assert!(matches!(
        packed_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Range {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                elem: VecElementType::F32,
                width: VecWidth::V512,
                lanes: 16,
                imm: 0x0F,
                scalar: false,
                mask_zeroing: true,
                suppress_exceptions: true,
            },
            x86_hint: Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0x50,
                width: VecWidth::V512,
                w: false,
            }),
            ..
        }]
    ));

    let scalar_sae = lift_single(&[0x62, 0xA3, 0xED, 0x92, 0x51, 0xCB, 0x0D]).unwrap();
    assert!(matches!(
        scalar_sae.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Range {
            elem: VecElementType::F64,
            scalar: true,
            suppress_exceptions: true,
            ..
        })
    ));

    let full_memory = lift_single(&[0x62, 0xF3, 0x6D, 0x4A, 0x50, 0x48, 0x01, 0x05]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF3, 0xED, 0x5A, 0x50, 0x48, 0x01, 0x05]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );
    let scalar_memory = lift_single(&[0x62, 0xF3, 0x6D, 0x8A, 0x51, 0x48, 0x01, 0x05]).unwrap();
    assert!(scalar_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));

    for invalid in [
        &[0x62, 0xF3, 0x6E, 0x08, 0x50, 0xCB, 0x05][..], // pp=F3
        &[0x62, 0xF3, 0x6D, 0x08, 0x50, 0xCB, 0x10][..], // imm[7:4] != 0
        &[0x62, 0xF3, 0x6D, 0x68, 0x50, 0xCB, 0x05][..], // packed L'L=3
        &[0x62, 0xF3, 0x6D, 0x18, 0x51, 0x08, 0x05][..], // scalar EVEX.b memory
        &[0x62, 0xF3, 0x6D, 0x88, 0x51, 0xCB, 0x05][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
    assert!(matches!(
        lift_single(&[0x62, 0xF3, 0x6D, 0x08, 0x50, 0xCB]),
        Err(LiftError::Incomplete { .. })
    ));
}
#[test]
fn lift_evex_fixup_imm_covers_formats_exceptions_masks_sae_and_memory() {
    for (bytes, elem, width, lanes, scalar, imm) in [
        (
            &[0x62, 0xF3, 0x6D, 0x08, 0x54, 0xCB, 0x00][..],
            VecElementType::F32,
            VecWidth::V128,
            4,
            false,
            0x00,
        ),
        (
            &[0x62, 0xF3, 0xED, 0x28, 0x54, 0xCB, 0xFF][..],
            VecElementType::F64,
            VecWidth::V256,
            4,
            false,
            0xFF,
        ),
        (
            &[0x62, 0xF3, 0x6D, 0x48, 0x54, 0xCB, 0xA5][..],
            VecElementType::F32,
            VecWidth::V512,
            16,
            false,
            0xA5,
        ),
        (
            &[0x62, 0xF3, 0x6D, 0x08, 0x55, 0xCB, 0x5A][..],
            VecElementType::F32,
            VecWidth::V128,
            1,
            true,
            0x5A,
        ),
        (
            &[0x62, 0xF3, 0xED, 0x08, 0x55, 0xCB, 0xC3][..],
            VecElementType::F64,
            VecWidth::V128,
            1,
            true,
            0xC3,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::X86FixupImm {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1) | X86Reg::Ymm(1) | X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2) | X86Reg::Ymm(2) | X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3) | X86Reg::Ymm(3) | X86Reg::Zmm(3))),
                elem: actual_elem,
                width: actual_width,
                lanes: actual_lanes,
                imm: actual_imm,
                scalar: actual_scalar,
                suppress_exceptions: false,
                ..
            }) if *actual_elem == elem
                && *actual_width == width
                && *actual_lanes == lanes
                && *actual_imm == imm
                && *actual_scalar == scalar
        ));
    }

    let packed_sae = lift_single(&[0x62, 0xA3, 0x6D, 0x92, 0x54, 0xCB, 0xFF]).unwrap();
    assert!(matches!(
        packed_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FixupImm {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                elem: VecElementType::F32,
                width: VecWidth::V512,
                lanes: 16,
                imm: 0xFF,
                scalar: false,
                mask_zeroing: true,
                suppress_exceptions: true,
            },
            x86_hint: Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0x54,
                width: VecWidth::V512,
                w: false,
            }),
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF3, 0x6D, 0x4A, 0x54, 0x48, 0x01, 0x33]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    let broadcast = lift_single(&[0x62, 0xF3, 0xED, 0x5A, 0x54, 0x48, 0x01, 0x44]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );

    // Scalar EVEX.b is SAE even for a memory table; it is not broadcast.
    let scalar_memory_sae = lift_single(&[0x62, 0xF3, 0x6D, 0x18, 0x55, 0x48, 0x01, 0x77]).unwrap();
    assert!(scalar_memory_sae.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        scalar_memory_sae.ops.last().map(|op| &op.kind),
        Some(OpKind::X86FixupImm {
            src2: VReg::Virtual(_),
            scalar: true,
            suppress_exceptions: true,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF3, 0x6E, 0x08, 0x54, 0xCB, 0x33][..], // pp=F3
        &[0x62, 0xF3, 0x6D, 0x68, 0x54, 0xCB, 0x33][..], // packed L'L=3
        &[0x62, 0xF3, 0x6D, 0x88, 0x55, 0xCB, 0x33][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
    assert!(matches!(
        lift_single(&[0x62, 0xF3, 0x6D, 0x08, 0x54, 0xCB]),
        Err(LiftError::Incomplete { .. })
    ));
}
#[test]
fn lift_evex_exp2_covers_formats_masks_sae_broadcast_and_reserved_fields() {
    for (bytes, elem, lanes) in [
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0xC8, 0xCB][..],
            VecElementType::F32,
            16,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x48, 0xC8, 0xCB][..],
            VecElementType::F64,
            8,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Exp2 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
                    mask: None,
                    elem: actual_elem,
                    width: VecWidth::V512,
                    lanes: actual_lanes,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xC8,
                    width: VecWidth::V512,
                    ..
                }),
                ..
            }] if *actual_elem == elem && *actual_lanes == lanes
        ));
    }

    let sae = lift_single(&[0x62, 0xA2, 0x7D, 0x99, 0xC8, 0xCB]).unwrap();
    assert!(matches!(
        sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Exp2 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                mask_zeroing: true,
                suppress_exceptions: true,
                ..
            },
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0xC8, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF2, 0xFD, 0x59, 0xC8, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(matches!(
        broadcast.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Exp2 {
            src: VReg::Virtual(_),
            elem: VecElementType::F64,
            suppress_exceptions: false,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF2, 0x7C, 0x48, 0xC8, 0xCB][..], // pp != 66
        &[0x62, 0xF2, 0x75, 0x48, 0xC8, 0xCB][..], // reserved vvvv
        &[0x62, 0xF2, 0x7D, 0x40, 0xC8, 0xCB][..], // reserved V'
        &[0x62, 0xF2, 0x7D, 0x08, 0xC8, 0xCB][..], // non-SAE VL128
        &[0x62, 0xF2, 0x7D, 0x28, 0xC8, 0xCB][..], // non-SAE VL256
        &[0x62, 0xF2, 0x7D, 0x68, 0xC8, 0xCB][..], // non-SAE L'L=3
        &[0x62, 0xF2, 0x7D, 0x19, 0xC8, 0x08][..], // broadcast requires VL512
        &[0x62, 0xF2, 0x7D, 0xC8, 0xC8, 0xCB][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_recip14_covers_widths_scalar_masks_broadcast_and_reserved_fields() {
    for (bytes, elem, width, lanes, scalar) in [
        (
            &[0x62, 0xF2, 0x7D, 0x08, 0x4C, 0xCB][..],
            VecElementType::F32,
            VecWidth::V128,
            4,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x28, 0x4C, 0xCB][..],
            VecElementType::F64,
            VecWidth::V256,
            4,
            false,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0x4C, 0xCB][..],
            VecElementType::F32,
            VecWidth::V512,
            16,
            false,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB][..],
            VecElementType::F32,
            VecWidth::V128,
            1,
            true,
        ),
        (
            &[0x62, 0xF2, 0xED, 0x68, 0x4D, 0xCB][..],
            VecElementType::F64,
            VecWidth::V128,
            1,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Recip14 {
                    dst: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(1) | X86Reg::Ymm(1) | X86Reg::Zmm(1)
                    )),
                    merge,
                    src: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(3) | X86Reg::Ymm(3) | X86Reg::Zmm(3)
                    )),
                    mask: None,
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    scalar: actual_scalar,
                    mask_zeroing: false,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                    ..
                }),
                ..
            }] if *actual_elem == elem
                && *actual_width == width
                && *actual_lanes == lanes
                && *actual_scalar == scalar
                && *actual_opcode == if scalar { 0x4D } else { 0x4C }
                && merge.is_some() == scalar
        ));
    }

    let masked = lift_single(&[0x62, 0xA2, 0xFD, 0xCA, 0x4C, 0xCB]).unwrap();
    assert!(matches!(
        masked.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Recip14 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                mask_zeroing: true,
                ..
            },
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF2, 0x7D, 0x09, 0x4C, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 16, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF2, 0xFD, 0x59, 0x4C, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );

    let scalar_memory = lift_single(&[0x62, 0xF2, 0x6D, 0x09, 0x4D, 0x48, 0x01]).unwrap();
    assert!(scalar_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        scalar_memory.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Recip14 {
            merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
            src: VReg::Virtual(_),
            scalar: true,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF2, 0x7C, 0x48, 0x4C, 0xCB][..], // pp != 66
        &[0x62, 0xF2, 0x75, 0x48, 0x4C, 0xCB][..], // packed reserved vvvv
        &[0x62, 0xF2, 0x7D, 0x40, 0x4C, 0xCB][..], // packed reserved V'
        &[0x62, 0xF2, 0x7D, 0x68, 0x4C, 0xCB][..], // packed L'L=3
        &[0x62, 0xF2, 0x7D, 0x58, 0x4C, 0xCB][..], // register EVEX.b
        &[0x62, 0xF2, 0x6D, 0x18, 0x4D, 0xCB][..], // scalar register EVEX.b
        &[0x62, 0xF2, 0x6D, 0x18, 0x4D, 0x08][..], // scalar memory EVEX.b
        &[0x62, 0xF2, 0x7D, 0x88, 0x4C, 0xCB][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_rsqrt14_covers_widths_scalar_masks_broadcast_and_reserved_fields() {
    for (bytes, elem, width, lanes, scalar) in [
        (
            &[0x62, 0xF2, 0x7D, 0x08, 0x4E, 0xCB][..],
            VecElementType::F32,
            VecWidth::V128,
            4,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x28, 0x4E, 0xCB][..],
            VecElementType::F64,
            VecWidth::V256,
            4,
            false,
        ),
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0x4E, 0xCB][..],
            VecElementType::F32,
            VecWidth::V512,
            16,
            false,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0x4F, 0xCB][..],
            VecElementType::F32,
            VecWidth::V128,
            1,
            true,
        ),
        (
            &[0x62, 0xF2, 0xED, 0x68, 0x4F, 0xCB][..],
            VecElementType::F64,
            VecWidth::V128,
            1,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Rsqrt14 {
                    dst: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(1) | X86Reg::Ymm(1) | X86Reg::Zmm(1)
                    )),
                    merge,
                    src: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(3) | X86Reg::Ymm(3) | X86Reg::Zmm(3)
                    )),
                    mask: None,
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    scalar: actual_scalar,
                    mask_zeroing: false,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                    ..
                }),
                ..
            }] if *actual_elem == elem
                && *actual_width == width
                && *actual_lanes == lanes
                && *actual_scalar == scalar
                && *actual_opcode == if scalar { 0x4F } else { 0x4E }
                && merge.is_some() == scalar
        ));
    }

    let masked = lift_single(&[0x62, 0xA2, 0xFD, 0xCA, 0x4E, 0xCB]).unwrap();
    assert!(matches!(
        masked.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Rsqrt14 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                mask_zeroing: true,
                ..
            },
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF2, 0x7D, 0x09, 0x4E, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 16, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF2, 0xFD, 0x59, 0x4E, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );

    let scalar_memory = lift_single(&[0x62, 0xF2, 0x6D, 0x09, 0x4F, 0x48, 0x01]).unwrap();
    assert!(scalar_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        scalar_memory.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Rsqrt14 {
            merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
            src: VReg::Virtual(_),
            scalar: true,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF2, 0x7C, 0x48, 0x4E, 0xCB][..], // pp != 66
        &[0x62, 0xF2, 0x75, 0x48, 0x4E, 0xCB][..], // packed reserved vvvv
        &[0x62, 0xF2, 0x7D, 0x40, 0x4E, 0xCB][..], // packed reserved V'
        &[0x62, 0xF2, 0x7D, 0x68, 0x4E, 0xCB][..], // packed L'L=3
        &[0x62, 0xF2, 0x7D, 0x58, 0x4E, 0xCB][..], // register EVEX.b
        &[0x62, 0xF2, 0x6D, 0x18, 0x4F, 0xCB][..], // scalar register EVEX.b
        &[0x62, 0xF2, 0x6D, 0x18, 0x4F, 0x08][..], // scalar memory EVEX.b
        &[0x62, 0xF2, 0x7D, 0x88, 0x4E, 0xCB][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_fp16_approx_covers_widths_scalar_masks_broadcast_and_reserved_fields() {
    for (bytes, width, lanes, scalar, rsqrt) in [
        (
            &[0x62, 0xF6, 0x7D, 0x08, 0x4C, 0xCB][..],
            VecWidth::V128,
            8,
            false,
            false,
        ),
        (
            &[0x62, 0xF6, 0x7D, 0x28, 0x4C, 0xCB][..],
            VecWidth::V256,
            16,
            false,
            false,
        ),
        (
            &[0x62, 0xF6, 0x7D, 0x48, 0x4E, 0xCB][..],
            VecWidth::V512,
            32,
            false,
            true,
        ),
        (
            &[0x62, 0xF6, 0x6D, 0x08, 0x4D, 0xCB][..],
            VecWidth::V128,
            1,
            true,
            false,
        ),
        (
            &[0x62, 0xF6, 0x6D, 0x68, 0x4F, 0xCB][..],
            VecWidth::V128,
            1,
            true,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        let kind = &lifted.ops.last().expect("FP16 approximate op").kind;
        let (actual_width, actual_lanes, actual_scalar, merge) = match kind {
            OpKind::X86RecipFp16 {
                width,
                lanes,
                scalar,
                merge,
                ..
            } if !rsqrt => (*width, *lanes, *scalar, merge),
            OpKind::X86RsqrtFp16 {
                width,
                lanes,
                scalar,
                merge,
                ..
            } if rsqrt => (*width, *lanes, *scalar, merge),
            other => panic!("unexpected FP16 approximation op: {other:?}"),
        };
        assert_eq!(actual_width, width);
        assert_eq!(actual_lanes, lanes);
        assert_eq!(actual_scalar, scalar);
        assert_eq!(merge.is_some(), scalar);
    }

    let masked = lift_single(&[0x62, 0xA6, 0x7D, 0xCA, 0x4E, 0xCB]).unwrap();
    assert!(matches!(
        masked.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86RsqrtFp16 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                mask_zeroing: true,
                ..
            },
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF6, 0x7D, 0x09, 0x4C, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        8
    );
    let broadcast = lift_single(&[0x62, 0xF6, 0x7D, 0x59, 0x4E, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        1
    );
    let scalar_memory = lift_single(&[0x62, 0xF6, 0x6D, 0x09, 0x4D, 0x48, 0x01]).unwrap();
    assert!(matches!(
        scalar_memory.ops.last().map(|op| &op.kind),
        Some(OpKind::X86RecipFp16 {
            merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
            src: VReg::Virtual(_),
            scalar: true,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF6, 0x7C, 0x48, 0x4C, 0xCB][..], // pp != 66
        &[0x62, 0xF6, 0xFD, 0x48, 0x4C, 0xCB][..], // W1
        &[0x62, 0xF6, 0x75, 0x48, 0x4C, 0xCB][..], // packed reserved vvvv
        &[0x62, 0xF6, 0x7D, 0x40, 0x4C, 0xCB][..], // packed reserved V'
        &[0x62, 0xF6, 0x7D, 0x68, 0x4C, 0xCB][..], // packed L'L=3
        &[0x62, 0xF6, 0x7D, 0x58, 0x4C, 0xCB][..], // register EVEX.b
        &[0x62, 0xF6, 0x6D, 0x18, 0x4D, 0xCB][..], // scalar register EVEX.b
        &[0x62, 0xF6, 0x6D, 0x18, 0x4D, 0x08][..], // scalar memory EVEX.b
        &[0x62, 0xF6, 0x7D, 0x88, 0x4C, 0xCB][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_recip28_covers_packed_scalar_masks_sae_broadcast_and_reserved_fields() {
    for (bytes, elem, lanes, scalar) in [
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0xCA, 0xCB][..],
            VecElementType::F32,
            16,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x48, 0xCA, 0xCB][..],
            VecElementType::F64,
            8,
            false,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0xCB, 0xCB][..],
            VecElementType::F32,
            1,
            true,
        ),
        (
            &[0x62, 0xF2, 0xED, 0x68, 0xCB, 0xCB][..],
            VecElementType::F64,
            1,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Recip28 {
                    dst: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(1) | X86Reg::Zmm(1)
                    )),
                    merge,
                    src: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(3) | X86Reg::Zmm(3)
                    )),
                    mask: None,
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    scalar: actual_scalar,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                    ..
                }),
                ..
            }] if *actual_elem == elem
                && *actual_width == if scalar { VecWidth::V128 } else { VecWidth::V512 }
                && *actual_lanes == lanes
                && *actual_scalar == scalar
                && *actual_opcode == if scalar { 0xCB } else { 0xCA }
                && merge.is_some() == scalar
        ));
    }

    let packed_sae = lift_single(&[0x62, 0xA2, 0x7D, 0x99, 0xCA, 0xCB]).unwrap();
    assert!(matches!(
        packed_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Recip28 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                mask_zeroing: true,
                suppress_exceptions: true,
                ..
            },
            ..
        }]
    ));

    let scalar_sae = lift_single(&[0x62, 0xA2, 0xED, 0x92, 0xCB, 0xCB]).unwrap();
    assert!(matches!(
        scalar_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Recip28 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                scalar: true,
                mask_zeroing: true,
                suppress_exceptions: true,
                ..
            },
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0xCA, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF2, 0xFD, 0x59, 0xCA, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );

    // For scalar VRCP28, EVEX.b is SAE for a memory source rather than
    // broadcast. EVEX.L'L is ignored and the load remains one element.
    let scalar_memory_sae = lift_single(&[0x62, 0xF2, 0x6D, 0x78, 0xCB, 0x48, 0x01]).unwrap();
    assert!(scalar_memory_sae.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        scalar_memory_sae.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Recip28 {
            src: VReg::Virtual(_),
            scalar: true,
            suppress_exceptions: true,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF2, 0x7C, 0x48, 0xCA, 0xCB][..], // pp != 66
        &[0x62, 0xF2, 0x75, 0x48, 0xCA, 0xCB][..], // packed reserved vvvv
        &[0x62, 0xF2, 0x7D, 0x40, 0xCA, 0xCB][..], // packed reserved V'
        &[0x62, 0xF2, 0x7D, 0x08, 0xCA, 0xCB][..], // packed non-SAE VL128
        &[0x62, 0xF2, 0x7D, 0x28, 0xCA, 0xCB][..], // packed non-SAE VL256
        &[0x62, 0xF2, 0x7D, 0x68, 0xCA, 0xCB][..], // packed non-SAE L'L=3
        &[0x62, 0xF2, 0x7D, 0x19, 0xCA, 0x08][..], // packed broadcast VL128
        &[0x62, 0xF2, 0x7D, 0xC8, 0xCA, 0xCB][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_rsqrt28_covers_packed_scalar_masks_sae_broadcast_and_reserved_fields() {
    for (bytes, elem, lanes, scalar) in [
        (
            &[0x62, 0xF2, 0x7D, 0x48, 0xCC, 0xCB][..],
            VecElementType::F32,
            16,
            false,
        ),
        (
            &[0x62, 0xF2, 0xFD, 0x48, 0xCC, 0xCB][..],
            VecElementType::F64,
            8,
            false,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0xCD, 0xCB][..],
            VecElementType::F32,
            1,
            true,
        ),
        (
            &[0x62, 0xF2, 0xED, 0x68, 0xCD, 0xCB][..],
            VecElementType::F64,
            1,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Rsqrt28 {
                    dst: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(1) | X86Reg::Zmm(1)
                    )),
                    merge,
                    src: VReg::Arch(ArchReg::X86(
                        X86Reg::Xmm(3) | X86Reg::Zmm(3)
                    )),
                    mask: None,
                    elem: actual_elem,
                    width: actual_width,
                    lanes: actual_lanes,
                    scalar: actual_scalar,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                },
                x86_hint: Some(X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                    ..
                }),
                ..
            }] if *actual_elem == elem
                && *actual_width == if scalar { VecWidth::V128 } else { VecWidth::V512 }
                && *actual_lanes == lanes
                && *actual_scalar == scalar
                && *actual_opcode == if scalar { 0xCD } else { 0xCC }
                && merge.is_some() == scalar
        ));
    }

    let packed_sae = lift_single(&[0x62, 0xA2, 0x7D, 0x99, 0xCC, 0xCB]).unwrap();
    assert!(matches!(
        packed_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Rsqrt28 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                merge: None,
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                mask_zeroing: true,
                suppress_exceptions: true,
                ..
            },
            ..
        }]
    ));

    let scalar_sae = lift_single(&[0x62, 0xA2, 0xED, 0x92, 0xCD, 0xCB]).unwrap();
    assert!(matches!(
        scalar_sae.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Rsqrt28 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(18)))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                scalar: true,
                mask_zeroing: true,
                suppress_exceptions: true,
                ..
            },
            ..
        }]
    ));

    let full_memory = lift_single(&[0x62, 0xF2, 0x7D, 0x49, 0xCC, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(full_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let broadcast = lift_single(&[0x62, 0xF2, 0xFD, 0x59, 0xCC, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        1
    );

    // For scalar VRSQRT28, EVEX.b is SAE for a memory source rather than
    // broadcast. EVEX.L'L is ignored and the load remains one element.
    let scalar_memory_sae = lift_single(&[0x62, 0xF2, 0x6D, 0x78, 0xCD, 0x48, 0x01]).unwrap();
    assert!(scalar_memory_sae.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(matches!(
        scalar_memory_sae.ops.last().map(|op| &op.kind),
        Some(OpKind::X86Rsqrt28 {
            src: VReg::Virtual(_),
            scalar: true,
            suppress_exceptions: true,
            ..
        })
    ));

    for invalid in [
        &[0x62, 0xF2, 0x7C, 0x48, 0xCC, 0xCB][..], // pp != 66
        &[0x62, 0xF2, 0x75, 0x48, 0xCC, 0xCB][..], // packed reserved vvvv
        &[0x62, 0xF2, 0x7D, 0x40, 0xCC, 0xCB][..], // packed reserved V'
        &[0x62, 0xF2, 0x7D, 0x08, 0xCC, 0xCB][..], // packed non-SAE VL128
        &[0x62, 0xF2, 0x7D, 0x28, 0xCC, 0xCB][..], // packed non-SAE VL256
        &[0x62, 0xF2, 0x7D, 0x68, 0xCC, 0xCB][..], // packed non-SAE L'L=3
        &[0x62, 0xF2, 0x7D, 0x19, 0xCC, 0x08][..], // packed broadcast VL128
        &[0x62, 0xF2, 0x7D, 0xC8, 0xCC, 0xCB][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
}
#[test]
fn lift_evex_scale_f_covers_formats_rounding_masks_and_memory() {
    for (bytes, elem, width, lanes, scalar) in [
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0x2C, 0xCB][..],
            VecElementType::F32,
            VecWidth::V128,
            4,
            false,
        ),
        (
            &[0x62, 0xF2, 0xED, 0x28, 0x2C, 0xCB][..],
            VecElementType::F64,
            VecWidth::V256,
            4,
            false,
        ),
        (
            &[0x62, 0xF6, 0x6D, 0x48, 0x2C, 0xCB][..],
            VecElementType::F16,
            VecWidth::V512,
            32,
            false,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0x2D, 0xCB][..],
            VecElementType::F32,
            VecWidth::V128,
            1,
            true,
        ),
        (
            &[0x62, 0xF2, 0xED, 0x08, 0x2D, 0xCB][..],
            VecElementType::F64,
            VecWidth::V128,
            1,
            true,
        ),
        (
            &[0x62, 0xF6, 0x6D, 0x08, 0x2D, 0xCB][..],
            VecElementType::F16,
            VecWidth::V128,
            1,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(matches!(
            lifted.ops.last().map(|op| &op.kind),
            Some(OpKind::X86ScaleF {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1) | X86Reg::Ymm(1) | X86Reg::Zmm(1))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2) | X86Reg::Ymm(2) | X86Reg::Zmm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3) | X86Reg::Ymm(3) | X86Reg::Zmm(3))),
                elem: actual_elem,
                width: actual_width,
                lanes: actual_lanes,
                scalar: actual_scalar,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                ..
            }) if *actual_elem == elem
                && *actual_width == width
                && *actual_lanes == lanes
                && *actual_scalar == scalar
        ));
    }

    let packed_er = lift_single(&[0x62, 0xA6, 0x6D, 0x92, 0x2C, 0xCB]).unwrap();
    assert!(matches!(
        packed_er.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86ScaleF {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                elem: VecElementType::F16,
                width: VecWidth::V512,
                lanes: 32,
                scalar: false,
                mask_zeroing: true,
                round: FpRoundMode::RoundNearest,
                suppress_exceptions: true,
            },
            x86_hint: Some(X86OpHint::EvexOp {
                map: X86VecMap::Map6,
                pp: X86SsePrefix::OpSize,
                opcode: 0x2C,
                width: VecWidth::V512,
                w: false,
            }),
            ..
        }]
    ));

    let scalar_er = lift_single(&[0x62, 0xA2, 0x6D, 0xF2, 0x2D, 0xCB]).unwrap();
    assert!(matches!(
        scalar_er.ops.last().map(|op| &op.kind),
        Some(OpKind::X86ScaleF {
            scalar: true,
            round: FpRoundMode::RoundTowardZero,
            suppress_exceptions: true,
            ..
        })
    ));

    let full_memory = lift_single(&[0x62, 0xF2, 0x7D, 0x4A, 0x2C, 0x48, 0x01]).unwrap();
    assert_eq!(
        full_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    let broadcast = lift_single(&[0x62, 0xF6, 0x7D, 0x5A, 0x2C, 0x48, 0x01]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        1
    );
    let scalar_memory = lift_single(&[0x62, 0xF2, 0x6D, 0x8A, 0x2D, 0x48, 0x01]).unwrap();
    assert!(scalar_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            addr: Address::BaseOffset { offset: 4, .. },
            width: MemWidth::B4,
            ..
        }
    )));

    for invalid in [
        &[0x62, 0xF2, 0x6E, 0x08, 0x2C, 0xCB][..], // pp=F3
        &[0x62, 0xF6, 0xED, 0x08, 0x2C, 0xCB][..], // FP16 W=1
        &[0x62, 0xF2, 0x6D, 0x68, 0x2C, 0xCB][..], // packed L'L=3 without ER
        &[0x62, 0xF2, 0x6D, 0x18, 0x2D, 0x08][..], // scalar EVEX.b memory
        &[0x62, 0xF2, 0x6D, 0x88, 0x2D, 0xCB][..], // {z} with k0
    ] {
        assert!(lift_single(invalid).is_err(), "accepted {invalid:02X?}");
    }
    assert!(matches!(
        lift_single(&[0x62, 0xF2, 0x6D, 0x08, 0x2C]),
        Err(LiftError::Incomplete { .. })
    ));
}
