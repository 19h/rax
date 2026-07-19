//! EVEX square-root lifting tests.

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

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
        &[0x62, 0xF1, 0x7C, 0x78, 0x51, 0x08][..], // VSQRTPS m32bcst with L'L=3
        &[0x62, 0xF1, 0x7E, 0x18, 0x51, 0x08][..], // VSQRTSS with EVEX.b memory
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    for (bytes, elem, lanes, mem_width, compressed_offset) in [
        (
            &[0x62, 0xF1, 0x7C, 0x18, 0x51, 0x50, 0x10][..],
            VecElementType::F32,
            4,
            MemWidth::B4,
            64,
        ), // VSQRTPS XMM2,m32bcst[RAX+64]
        (
            &[0x62, 0xF1, 0x7C, 0x38, 0x51, 0x50, 0x10][..],
            VecElementType::F32,
            8,
            MemWidth::B4,
            64,
        ), // VSQRTPS YMM2,m32bcst[RAX+64]
        (
            &[0x62, 0xF1, 0x7C, 0x58, 0x51, 0x50, 0x10][..],
            VecElementType::F32,
            16,
            MemWidth::B4,
            64,
        ), // VSQRTPS ZMM2,m32bcst[RAX+64]
        (
            &[0x62, 0xF1, 0xFD, 0x18, 0x51, 0x50, 0x10][..],
            VecElementType::F64,
            2,
            MemWidth::B8,
            128,
        ), // VSQRTPD XMM2,m64bcst[RAX+128]
        (
            &[0x62, 0xF1, 0xFD, 0x38, 0x51, 0x50, 0x10][..],
            VecElementType::F64,
            4,
            MemWidth::B8,
            128,
        ), // VSQRTPD YMM2,m64bcst[RAX+128]
        (
            &[0x62, 0xF1, 0xFD, 0x58, 0x51, 0x50, 0x10][..],
            VecElementType::F64,
            8,
            MemWidth::B8,
            128,
        ), // VSQRTPD ZMM2,m64bcst[RAX+128]
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::Load { width, .. } if width == mem_width))
                .count(),
            1,
            "broadcast tuple must perform one scalar memory read"
        );
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load {
                addr: Address::BaseOffset {
                    offset,
                    disp_size: DispSize::Disp8,
                    ..
                },
                ..
            } if offset == compressed_offset
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: actual_elem,
                lanes: actual_lanes,
                ..
            } if actual_elem == elem && actual_lanes == lanes
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VUnary {
                elem: actual_elem,
                lanes: actual_lanes,
                op: VecUnaryOp::FSqrt,
                ..
            } if actual_elem == elem && actual_lanes == lanes
        )));
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VLoad { .. } | OpKind::PredLoad { .. }))
        );
    }

    for (bytes, elem, lanes, mem_width, compressed_offset) in [
        (
            &[0x62, 0xF1, 0x7C, 0x59, 0x51, 0x50, 0x10][..],
            VecElementType::F32,
            16,
            MemWidth::B4,
            64,
        ), // VSQRTPS ZMM2{k1},m32bcst[RAX+64]
        (
            &[0x62, 0xF1, 0xFD, 0x5A, 0x51, 0x50, 0x10][..],
            VecElementType::F64,
            8,
            MemWidth::B8,
            128,
        ), // VSQRTPD ZMM2{k2},m64bcst[RAX+128]
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width,
                        addr: Address::BaseOffset {
                            offset,
                            disp_size: DispSize::Disp8,
                            ..
                        },
                        ..
                    } if width == mem_width && offset == compressed_offset
                ))
                .count(),
            1,
            "masked broadcast must issue one aggregate-gated scalar read"
        );
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: actual_elem,
                lanes: actual_lanes,
                ..
            } if actual_elem == elem && actual_lanes == lanes
        )));
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. }))
        );
    }

    let fs_addr32 = lift_single(&[0x64, 0x67, 0x62, 0xF1, 0x7C, 0x58, 0x51, 0x08]).unwrap();
    assert!(fs_addr32.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::X86Addr32(ref inner),
            width: MemWidth::B4,
            ..
        } if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                ..
            }
        )
    )));

    assert!(matches!(
        lift_single(&[0xF0, 0xF3, 0x0F, 0x51, 0xC1]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn lift_evex_sqrt_embedded_rounding_and_sae_exact_shapes() {
    let modes = [
        (0x18, FpRoundMode::RoundNearest),
        (0x38, FpRoundMode::RoundDown),
        (0x58, FpRoundMode::RoundUp),
        (0x78, FpRoundMode::RoundTowardZero),
    ];

    for (p2, round) in modes {
        for (p1, elem, lanes) in [
            (0x7C, VecElementType::F32, 16),
            (0xFD, VecElementType::F64, 8),
        ] {
            let result = lift_single(&[0x62, 0xF1, p1, p2, 0x51, 0xC1]).unwrap();
            assert_eq!(result.bytes_consumed, 6);
            assert!(result.ops.iter().any(|op| matches!(
                (&op.kind, op.x86_hint),
                (
                    OpKind::X86Sqrt {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                        elem: actual_elem,
                        lanes: actual_lanes,
                        round: actual_round,
                        suppress_exceptions: true,
                    },
                    Some(X86OpHint::EvexOp {
                        width: VecWidth::V512,
                        ..
                    })
                ) if *actual_elem == elem && *actual_lanes == lanes && *actual_round == round
            )));
            assert!(
                !result
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::VUnary { .. }))
            );
        }

        for (p1, elem) in [(0x7E, VecElementType::F32), (0xFF, VecElementType::F64)] {
            let result = lift_single(&[0x62, 0xF1, p1, p2, 0x51, 0xC1]).unwrap();
            assert_eq!(result.bytes_consumed, 6);
            assert!(result.ops.iter().any(|op| matches!(
                (&op.kind, op.x86_hint),
                (
                    OpKind::X86Sqrt {
                        src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        elem: actual_elem,
                        lanes: 1,
                        round: actual_round,
                        suppress_exceptions: true,
                        ..
                    },
                    Some(X86OpHint::EvexOp {
                        width: VecWidth::V128,
                        ..
                    })
                ) if *actual_elem == elem && *actual_round == round
            )));
        }
    }

    // Writemasking remains explicit around the compute primitive. Inactive
    // elements are zero-sanitized before computation and merged afterward.
    let masked = lift_single(&[0x62, 0xF1, 0x7C, 0x99, 0x51, 0xE0]).unwrap();
    assert!(masked.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86Sqrt {
            dst: VReg::Virtual(_),
            src: VReg::Virtual(_),
            elem: VecElementType::F32,
            lanes: 16,
            round: FpRoundMode::RoundNearest,
            suppress_exceptions: true,
        }
    )));
    assert_eq!(
        masked
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(4))),
                    elem: VecElementType::F32,
                    ..
                }
            ))
            .count(),
        16
    );

    let high_masked = lift_single(&[0x62, 0x81, 0x7C, 0xDB, 0x51, 0xC8]).unwrap();
    assert!(high_masked.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(24))),
            elem: VecElementType::F32,
            ..
        }
    )));
    assert!(high_masked.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86Sqrt {
            elem: VecElementType::F32,
            lanes: 16,
            round: FpRoundMode::RoundUp,
            suppress_exceptions: true,
            ..
        }
    )));
    assert_eq!(
        high_masked
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                    ..
                }
            ))
            .count(),
        16
    );

    let scalar_high = lift_single(&[0x62, 0x81, 0xEF, 0x93, 0x51, 0xC8]).unwrap();
    assert!(scalar_high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86Sqrt {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(24))),
            elem: VecElementType::F64,
            lanes: 1,
            round: FpRoundMode::RoundNearest,
            suppress_exceptions: true,
            ..
        }
    )));
    assert!(scalar_high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            lane: 1,
            elem: VecElementType::F64,
            ..
        }
    )));
    assert!(scalar_high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            lane: 0,
            elem: VecElementType::F64,
            ..
        }
    )));
}
