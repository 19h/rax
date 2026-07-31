//! evex::integer tests

use super::*;
use crate::smir::lift::x86_64::tests::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_legacy_vex_evex_scalar_and_packed_x86_minmax() {
    for (bytes, elem, lanes, min) in [
        (&[0xF3, 0x0F, 0x5D, 0xC1][..], VecElementType::F32, 1, true),
        (&[0xF2, 0x0F, 0x5F, 0xC1][..], VecElementType::F64, 1, false),
        (&[0x0F, 0x5D, 0xC1][..], VecElementType::F32, 4, true),
        (&[0x66, 0x0F, 0x5F, 0xC1][..], VecElementType::F64, 2, false),
        (&[0xC5, 0xF5, 0x5F, 0xC2][..], VecElementType::F64, 4, false),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86FpBinary {
                elem: actual_elem,
                lanes: actual_lanes,
                op: actual_op,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                ..
            } if actual_elem == elem
                && actual_lanes == lanes
                && actual_op == if min { X86FpBinaryOp::Min } else { X86FpBinaryOp::Max }
        )));
    }

    let vex_scalar = lift_single(&[0xC5, 0xF2, 0x5D, 0xC2]).unwrap();
    assert!(vex_scalar.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            elem: VecElementType::F32,
            lanes: 1,
            op: X86FpBinaryOp::Min,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            ..
        }
    )));

    let legacy_packed = lift_single(&[0x0F, 0x5D, 0xC1]).unwrap();
    assert!(legacy_packed.ops.iter().any(|op| matches!(
        op,
        SmirOp {
            kind: OpKind::X86FpBinary {
                dst: VReg::Virtual(_),
                elem: VecElementType::F32,
                lanes: 4,
                op: X86FpBinaryOp::Min,
                ..
            },
            x86_hint: Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x5D,
            }),
            ..
        }
    )));
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
        4
    );

    let masked_memory = lift_single(&[0x62, 0xF1, 0x7E, 0x09, 0x5D, 0x10]).unwrap();
    assert!(masked_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(masked_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            op: X86FpBinaryOp::Min,
            lanes: 1,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            ..
        }
    )));
    assert!(
        masked_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::Select { .. }))
    );

    let compressed = lift_single(&[0x62, 0xF1, 0x7C, 0x48, 0x5D, 0x50, 0x01]).unwrap();
    assert!(compressed.ops.iter().any(|op| matches!(
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
    assert!(compressed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            elem: VecElementType::F32,
            lanes: 16,
            op: X86FpBinaryOp::Min,
            ..
        }
    )));

    let high = lift_single(&[0x62, 0xA1, 0x7C, 0x40, 0x5D, 0xD1]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            op: X86FpBinaryOp::Min,
            ..
        }
    )));
    assert!(matches!(
        high.ops.last().unwrap().kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            width: VecWidth::V512,
            ..
        }
    ));

    for bytes in [
        &[0x62, 0xF1, 0xFE, 0x09, 0x5D, 0xD1][..], // VMINSS W=1
        &[0x62, 0xF1, 0x7F, 0x09, 0x5F, 0xD1][..], // VMAXSD W=0
        &[0x62, 0xF1, 0xFC, 0x48, 0x5D, 0xC1][..], // VMINPS W=1
        &[0x62, 0xF1, 0x7D, 0x48, 0x5F, 0xC1][..], // VMAXPD W=0
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_vex_and_evex_packed_integer_unpack_covers_all_elements_and_halves() {
    for (opcode, elem, high) in [
        (0x60, VecElementType::I8, false),
        (0x61, VecElementType::I16, false),
        (0x62, VecElementType::I32, false),
        (0x6C, VecElementType::I64, false),
        (0x68, VecElementType::I8, true),
        (0x69, VecElementType::I16, true),
        (0x6A, VecElementType::I32, true),
        (0x6D, VecElementType::I64, true),
    ] {
        if elem != VecElementType::I64 {
            let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
            assert!(matches!(
                mmx.ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86X87Control {
                            kind: X86X87ControlKind::EnterMmx,
                            ..
                        },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VInterleave {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                            src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                            elem: actual,
                            lanes,
                            block_lanes,
                            high: actual_high,
                        },
                        x86_hint: Some(X86OpHint::SseOp {
                            prefix: X86SsePrefix::None,
                            opcode: actual_opcode,
                        }),
                        ..
                    }
                ] if *actual == elem
                    && u32::from(*lanes) == VecWidth::V64.lanes(elem)
                    && u32::from(*block_lanes) == 8 / elem.bytes()
                    && *actual_high == high
                    && *actual_opcode == opcode
            ));
        }

        let legacy = lift_single(&[0x66, 0x0F, opcode, 0xC1]).unwrap();
        assert!(legacy.ops.iter().any(|op| matches!(
            (op.kind.clone(), op.x86_hint),
            (OpKind::VInterleave {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                elem: actual,
                lanes,
                block_lanes,
                high: actual_high,
            }, Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: actual_opcode,
            })) if actual == elem
                && u32::from(lanes) == VecWidth::V128.lanes(elem)
                && u32::from(block_lanes) == 16 / elem.bytes()
                && actual_high == high
                && actual_opcode == opcode
        )));
        assert_eq!(legacy.ops.len(), 1, "register legacy unpack is atomic");

        let vex128 = lift_single(&[0xC5, 0xF1, opcode, 0xC2]).unwrap();
        assert!(matches!(
            (
                vex128.ops.last().unwrap().kind.clone(),
                vex128.ops.last().unwrap().x86_hint
            ),
            (OpKind::VInterleave {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                elem: actual,
                high: actual_high,
                ..
            }, Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: actual_opcode,
                width: VecWidth::V128,
                ..
            })) if actual == elem && actual_high == high && actual_opcode == opcode
        ));
        let vex256 = lift_single(&[0xC5, 0xF5, opcode, 0x00]).unwrap();
        assert!(vex256.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        )));
        assert!(matches!(
            (
                vex256.ops.last().unwrap().kind.clone(),
                vex256.ops.last().unwrap().x86_hint
            ),
            (OpKind::VInterleave {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                src2: VReg::Virtual(_),
                lanes,
                block_lanes,
                high: actual_high,
                ..
            }, Some(X86OpHint::VexOp {
                opcode: actual_opcode,
                width: VecWidth::V256,
                ..
            })) if u32::from(lanes) == VecWidth::V256.lanes(elem)
                && u32::from(block_lanes) == 16 / elem.bytes()
                && actual_high == high
                && actual_opcode == opcode
        ));
    }

    let legacy_memory = lift_single(&[0x66, 0x0F, 0x60, 0x00]).unwrap();
    assert!(legacy_memory.ops.iter().any(|op| matches!(
        (op.kind.clone(), op.x86_hint),
        (
            OpKind::VInterleave {
                dst: VReg::Virtual(_),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Virtual(_),
                elem: VecElementType::I8,
                high: false,
                ..
            },
            Some(X86OpHint::SseOp { opcode: 0x60, .. })
        )
    )));
    assert!(legacy_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            ..
        }
    )));

    let mmx_low_memory = lift_single(&[0x0F, 0x60, 0x00]).unwrap();
    assert!(matches!(
        mmx_low_memory.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::Load {
                    width: MemWidth::B4,
                    sign: SignExtend::Zero,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::VBroadcast {
                    elem: VecElementType::I64,
                    lanes: 1,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::VInterleave {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    elem: VecElementType::I8,
                    lanes: 8,
                    block_lanes: 8,
                    high: false,
                    ..
                },
                ..
            }
        ]
    ));
    let mmx_high_memory = lift_single(&[0x0F, 0x68, 0x00]).unwrap();
    assert!(matches!(
        mmx_high_memory.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::VInterleave {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    elem: VecElementType::I8,
                    lanes: 8,
                    block_lanes: 8,
                    high: true,
                    ..
                },
                ..
            }
        ]
    ));

    for bytes in [
        &[0x0F, 0x6C, 0xC1][..],             // no MMX qword form
        &[0x0F, 0x6D, 0xC1][..],             // no MMX qword form
        &[0xF0, 0x66, 0x0F, 0x60, 0xC1][..], // LOCK
        &[0xF3, 0x66, 0x0F, 0x60, 0xC1][..], // conflicting mandatory prefix
        &[0xC5, 0xF0, 0x60, 0xC1][..],       // VEX.pp != 66
        &[0xC5, 0xF1, 0x60][..],             // missing ModR/M
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid packed unpack accepted: {bytes:02X?}",
        );
    }

    for (opcode, elem, w) in [
        (0x60, VecElementType::I8, 0x75),
        (0x61, VecElementType::I16, 0x75),
        (0x62, VecElementType::I32, 0x75),
        (0x6C, VecElementType::I64, 0xF5),
        (0x68, VecElementType::I8, 0x75),
        (0x69, VecElementType::I16, 0x75),
        (0x6A, VecElementType::I32, 0x75),
        (0x6D, VecElementType::I64, 0xF5),
    ] {
        let evex = lift_single(&[0x62, 0xF1, w, 0x09, opcode, 0xC2]).unwrap();
        assert!(evex.ops.iter().any(|op| matches!(
            (op.kind.clone(), op.x86_hint),
            (OpKind::VInterleave {
                dst: VReg::Virtual(_),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                elem: actual,
                ..
            }, Some(X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: actual_opcode,
                width: VecWidth::V128,
                ..
            })) if actual == elem && actual_opcode == opcode
        )));
        assert!(evex.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec,
                elem: actual,
                ..
            } if actual == elem && matches!(vec, VReg::Virtual(_))
        )));
    }

    let high_q_mem = lift_single(&[0x62, 0xF1, 0xF5, 0x49, 0x6D, 0x40, 0x01]).unwrap();
    assert!(high_q_mem.ops.iter().any(|op| matches!(
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
    assert!(
        !high_q_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    let high_regs = lift_single(&[0x62, 0xA1, 0x75, 0x00, 0x60, 0xC2]).unwrap();
    assert!(matches!(
        (
            high_regs.ops.last().unwrap().kind.clone(),
            high_regs.ops.last().unwrap().x86_hint
        ),
        (
            OpKind::VInterleave {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                ..
            },
            Some(X86OpHint::EvexOp {
                opcode: 0x60,
                width: VecWidth::V128,
                ..
            })
        )
    ));

    let byte_wig = lift_single(&[0x62, 0xF1, 0xF5, 0x08, 0x60, 0xC1]).unwrap();
    assert!(matches!(
        byte_wig.ops.last().and_then(|op| op.x86_hint),
        Some(X86OpHint::EvexOp { w: true, .. })
    ));

    for bytes in [
        &[0x62, 0xF1, 0xF5, 0x08, 0x62, 0xC1][..], // dword form W=1
        &[0x62, 0xF1, 0x75, 0x08, 0x6C, 0xC1][..], // qword form W=0
        &[0x62, 0xF1, 0x75, 0x88, 0x60, 0xC1][..], // {z} with k0
        &[0x62, 0xF1, 0x75, 0x18, 0x60, 0xC1][..], // EVEX.b reserved
        &[0x62, 0xF1, 0x75, 0x68, 0x60, 0xC1][..], // EVEX.L'L=3
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid EVEX packed unpack accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_original_packed_minmax_covers_legacy_vex_evex_e4_and_invalids() {
    for (opcode, elem, lane_op, signed) in [
        (0xDA, VecElementType::I8, VLaneOp::Min, false),
        (0xDE, VecElementType::I8, VLaneOp::Max, false),
        (0xEA, VecElementType::I16, VLaneOp::Min, true),
        (0xEE, VecElementType::I16, VLaneOp::Max, true),
    ] {
        let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        elem: actual_elem,
                        lanes,
                        op: actual_op,
                        signed: actual_signed,
                        set_ovf: false,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: actual_opcode,
                    }),
                    ..
                },
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    },
                    ..
                }
            ] if *actual_elem == elem
                && *lanes == VecWidth::V64.lanes(elem) as u8
                && *actual_op == lane_op
                && *actual_signed == signed
                && *actual_opcode == opcode
        ));
    }

    for (bytes, elem, lane_op, signed, dst, src1, src2, atomic) in [
        (
            &[0x66, 0x0F, 0xDA, 0xD1][..],
            VecElementType::I8,
            VLaneOp::Min,
            false,
            X86Reg::Xmm(2),
            X86Reg::Xmm(2),
            X86Reg::Xmm(1),
            true,
        ),
        (
            &[0x66, 0x0F, 0xDE, 0xE3][..],
            VecElementType::I8,
            VLaneOp::Max,
            false,
            X86Reg::Xmm(4),
            X86Reg::Xmm(4),
            X86Reg::Xmm(3),
            true,
        ),
        (
            &[0xC4, 0x41, 0x35, 0xEA, 0xC2][..],
            VecElementType::I16,
            VLaneOp::Min,
            true,
            X86Reg::Ymm(8),
            X86Reg::Ymm(9),
            X86Reg::Ymm(10),
            true,
        ),
        (
            &[0x62, 0xA1, 0x75, 0xC1, 0xEE, 0xC2][..],
            VecElementType::I16,
            VLaneOp::Max,
            true,
            X86Reg::Zmm(16),
            X86Reg::Zmm(17),
            X86Reg::Zmm(18),
            false,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        if atomic {
            assert_eq!(result.ops.len(), 1);
            assert!(matches!(
                result.ops[0].kind,
                OpKind::VLane {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src1: VReg::Arch(ArchReg::X86(actual_src1)),
                    src2: VReg::Arch(ArchReg::X86(actual_src2)),
                    elem: actual_elem,
                    op: actual_op,
                    signed: actual_signed,
                    set_ovf: false,
                    ..
                } if actual_dst == dst
                    && actual_src1 == src1
                    && actual_src2 == src2
                    && actual_elem == elem
                    && actual_op == lane_op
                    && actual_signed == signed
            ));
            assert!(result.ops[0].x86_hint.is_some());
        } else {
            let cond = match (lane_op, signed) {
                (VLaneOp::Min, true) => VecCmpCond::Lt,
                (VLaneOp::Min, false) => VecCmpCond::Ltu,
                (VLaneOp::Max, true) => VecCmpCond::Gt,
                (VLaneOp::Max, false) => VecCmpCond::Gtu,
                _ => unreachable!(),
            };
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VCmp {
                    src1: VReg::Arch(ArchReg::X86(actual_src1)),
                    src2: VReg::Arch(ArchReg::X86(actual_src2)),
                    cond: actual_cond,
                    elem: actual_elem,
                    ..
                } if actual_src1 == src1 && actual_src2 == src2
                    && actual_cond == cond && actual_elem == elem
            )));
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VBitSelect {
                    src_true: VReg::Arch(ArchReg::X86(actual_src1)),
                    src_false: VReg::Arch(ArchReg::X86(actual_src2)),
                    ..
                } if actual_src1 == src1 && actual_src2 == src2
            )));
            assert!(result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    ..
                } | OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    ..
                } if actual_dst == dst
            )));
        }
    }

    let legacy_memory = lift_single(&[0x66, 0x0F, 0xEA, 0x00]).unwrap();
    assert!(
        legacy_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
    );

    let mmx_memory = lift_single(&[0x0F, 0xEA, 0x40, 0x01]).unwrap();
    assert!(mmx_memory.ops.iter().any(|op| matches!(
        (&op.kind, op.x86_hint),
        (
            OpKind::VLoad {
                width: VecWidth::V64,
                ..
            },
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
        )
    )));
    assert!(mmx_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
            elem: VecElementType::I16,
            lanes: 4,
            op: VLaneOp::Min,
            signed: true,
            ..
        }
    )));
    assert!(
        !mmx_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    // Type E4: each active byte controls its corresponding source access.
    let masked_memory = lift_single(&[0x62, 0xE1, 0x75, 0x41, 0xDE, 0x40, 0x01]).unwrap();
    assert_eq!(
        masked_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            ))
            .count(),
        64,
    );
    assert!(masked_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    // W is ignored for all byte/word forms.
    assert!(lift_single(&[0x62, 0xA1, 0xF5, 0x41, 0xEA, 0xC2]).is_ok());
    for bytes in [
        &[0x0F, 0xDA][..],
        &[0xF3, 0x66, 0x0F, 0xDA, 0xC1][..],
        &[0xF0, 0x66, 0x0F, 0xEE, 0xC1][..],
        &[0xC5, 0xF0, 0xDA, 0xC2][..],
        &[0xC5, 0xF1, 0xEA][..],
        &[0x62, 0xA1, 0x75, 0xC0, 0xDE, 0xC2][..],
        &[0x62, 0xA1, 0x75, 0x51, 0xEA, 0xC2][..],
        &[0x62, 0xA1, 0x75, 0x68, 0xEE, 0xC2][..],
        &[0x62, 0xA1, 0x74, 0x41, 0xDA, 0xC2][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid original packed min/max accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_evex_packed_rotates_cover_directions_counts_widths_memory_and_invalids() {
    let cases = [
        (
            &[0x62, 0xF1, 0x75, 0x08, 0x72, 0xCA, 0x07][..],
            VecElementType::I32,
            VecWidth::V128,
            true,
            false,
        ),
        // Intel SDM Table 2-41: EVEX.R and EVEX.R' are ignored when
        // ModR/M.reg is the immediate rotate's opcode extension.
        (
            &[0x62, 0x71, 0x75, 0x08, 0x72, 0xCA, 0x07][..],
            VecElementType::I32,
            VecWidth::V128,
            true,
            false,
        ),
        (
            &[0x62, 0xE1, 0x75, 0x08, 0x72, 0xCA, 0x07][..],
            VecElementType::I32,
            VecWidth::V128,
            true,
            false,
        ),
        (
            &[0x62, 0x61, 0x75, 0x08, 0x72, 0xCA, 0x07][..],
            VecElementType::I32,
            VecWidth::V128,
            true,
            false,
        ),
        (
            &[0x62, 0xF1, 0x55, 0x09, 0x72, 0xC6, 0x05][..],
            VecElementType::I32,
            VecWidth::V128,
            false,
            false,
        ),
        (
            &[0x62, 0xD1, 0xF5, 0x47, 0x72, 0x4D, 0x01, 0x3F][..],
            VecElementType::I64,
            VecWidth::V512,
            true,
            false,
        ),
        (
            &[0x62, 0xD1, 0xDD, 0xB3, 0x72, 0x41, 0x7F, 0x11][..],
            VecElementType::I64,
            VecWidth::V256,
            false,
            false,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0x08, 0x15, 0xCB][..],
            VecElementType::I32,
            VecWidth::V128,
            true,
            true,
        ),
        (
            &[0x62, 0xA2, 0xDD, 0xA4, 0x15, 0xDD][..],
            VecElementType::I64,
            VecWidth::V256,
            true,
            true,
        ),
        (
            &[0x62, 0xF2, 0x4D, 0x5A, 0x14, 0x68, 0x01][..],
            VecElementType::I32,
            VecWidth::V512,
            false,
            true,
        ),
        (
            &[0x62, 0x02, 0x8D, 0xC7, 0x14, 0xFD][..],
            VecElementType::I64,
            VecWidth::V512,
            false,
            true,
        ),
    ];
    for (bytes, elem, width, left, variable) in cases {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        let rotate = lifted
            .ops
            .iter()
            .find_map(|op| match &op.kind {
                OpKind::X86PackedRotate {
                    count,
                    width: actual_width,
                    elem: actual_elem,
                    left: actual_left,
                    ..
                } => Some((count, actual_width, actual_elem, actual_left)),
                _ => None,
            })
            .expect("packed rotate IR");
        assert_eq!(*rotate.1, width);
        assert_eq!(*rotate.2, elem);
        assert_eq!(*rotate.3, left);
        assert_eq!(rotate.0.is_some(), variable);
    }

    // Type E4: aggregate the applicable mask bits and issue at most one
    // scalar read, with disp8 scaled by 4 bytes.
    let broadcast = lift_single(&[0x62, 0xF2, 0x4D, 0x5A, 0x14, 0x68, 0x01]).unwrap();
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
            ..
        }
    )));

    for bytes in [
        &[0xC5, 0xF1, 0x72, 0xCA, 0x07][..],          // EVEX-only
        &[0x62, 0xF1, 0x74, 0x08, 0x72, 0xCA, 7][..], // mandatory 66 absent
        &[0x62, 0xF1, 0x75, 0x68, 0x72, 0xCA, 7][..], // L'L=3
        &[0x62, 0xF1, 0x75, 0x88, 0x72, 0xCA, 7][..], // {z} with k0
        &[0x62, 0xF1, 0x75, 0x18, 0x72, 0xCA, 7][..], // EVEX.b on register
        &[0x62, 0xF2, 0x6C, 0x08, 0x15, 0xCB][..],    // variable form mandatory 66 absent
        &[0x62, 0xF2, 0x6D, 0x18, 0x15, 0xCB][..],    // variable EVEX.b on register
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted reserved packed-rotate encoding {bytes:02X?}"
        );
    }
}
#[test]
fn lift_evex_packed_funnel_shifts_cover_forms_elements_e4_memory_and_invalids() {
    let cases = [
        (
            &[0x62, 0xF3, 0xED, 0x08, 0x70, 0xCB, 0x07][..],
            VecElementType::I16,
            VecWidth::V128,
            true,
            false,
        ),
        (
            &[0x62, 0xF3, 0x55, 0xBA, 0x71, 0x60, 0x7F, 0x1F][..],
            VecElementType::I32,
            VecWidth::V256,
            true,
            false,
        ),
        (
            &[0x62, 0xA3, 0xED, 0x47, 0x71, 0xCB, 0x3F][..],
            VecElementType::I64,
            VecWidth::V512,
            true,
            false,
        ),
        (
            &[0x62, 0xA3, 0xD5, 0x03, 0x72, 0xE6, 0x11][..],
            VecElementType::I16,
            VecWidth::V128,
            false,
            false,
        ),
        (
            &[0x62, 0xF3, 0x6D, 0x28, 0x73, 0xCB, 0x27][..],
            VecElementType::I32,
            VecWidth::V256,
            false,
            false,
        ),
        (
            &[0x62, 0x43, 0x8D, 0xC1, 0x73, 0x7D, 0x01, 0x41][..],
            VecElementType::I64,
            VecWidth::V512,
            false,
            false,
        ),
        (
            &[0x62, 0xF2, 0xED, 0x08, 0x70, 0xCB][..],
            VecElementType::I16,
            VecWidth::V128,
            true,
            true,
        ),
        (
            &[0x62, 0xA2, 0x55, 0xA3, 0x71, 0xE6][..],
            VecElementType::I32,
            VecWidth::V256,
            true,
            true,
        ),
        (
            &[0x62, 0xE2, 0xED, 0x57, 0x71, 0x48, 0x7F][..],
            VecElementType::I64,
            VecWidth::V512,
            true,
            true,
        ),
        (
            &[0x62, 0xF2, 0xD5, 0x0A, 0x72, 0xE6][..],
            VecElementType::I16,
            VecWidth::V128,
            false,
            true,
        ),
        (
            &[0x62, 0xF2, 0x6D, 0x28, 0x73, 0xCB][..],
            VecElementType::I32,
            VecWidth::V256,
            false,
            true,
        ),
        (
            &[0x62, 0x42, 0x8D, 0xC1, 0x73, 0x7D, 0x01][..],
            VecElementType::I64,
            VecWidth::V512,
            false,
            true,
        ),
    ];
    for (bytes, elem, width, left, variable) in cases {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedFunnelShift {
                count,
                width: actual_width,
                elem: actual_elem,
                left: actual_left,
                ..
            } if actual_width == width
                && actual_elem == elem
                && actual_left == left
                && count.is_some() == variable
        )));
    }

    // Type E4: aggregate the applicable mask bits and issue at most one
    // scalar read, with disp8 scaled by 4 bytes.
    let broadcast = lift_single(&[0x62, 0xF3, 0x55, 0xBA, 0x71, 0x60, 0x7F, 0x1F]).unwrap();
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
            addr: Address::BaseOffset { offset: 508, .. },
            ..
        }
    )));

    for bytes in [
        &[0xC4, 0xE3, 0xED, 0x70, 0xCB, 7][..],       // EVEX-only
        &[0x62, 0xF3, 0xEC, 0x08, 0x70, 0xCB, 7][..], // mandatory 66 absent
        &[0x62, 0xF3, 0x6D, 0x08, 0x70, 0xCB, 7][..], // word form requires W=1
        &[0x62, 0xF3, 0xED, 0x68, 0x70, 0xCB, 7][..], // L'L=3
        &[0x62, 0xF3, 0xED, 0x88, 0x70, 0xCB, 7][..], // {z} with k0
        &[0x62, 0xF3, 0xED, 0x18, 0x70, 0xCB, 7][..], // EVEX.b on register
        &[0x62, 0xF3, 0xED, 0x18, 0x70, 0x08, 7][..], // word broadcast reserved
        &[0x62, 0xF3, 0xED, 0x08, 0x70, 0xCB][..],    // missing immediate
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "accepted reserved packed-funnel encoding {bytes:02X?}"
        );
    }
}
#[test]
fn lift_evex_multishift_qb_covers_widths_high_regs_e4nf_memory_and_invalids() {
    for bytes in [
        &[0x62, 0xF2, 0xED, 0x08, 0x83, 0xCB][..],
        &[0x62, 0xA2, 0xD5, 0xA3, 0x83, 0xE6][..],
        &[0x62, 0xC2, 0xED, 0x57, 0x83, 0x4D, 0x7F][..],
        &[0x62, 0x62, 0x8D, 0xC1, 0x83, 0x78, 0x01][..],
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(
            lifted
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86MultiShiftQB { .. }))
        );
    }

    let broadcast = lift_single(&[0x62, 0xC2, 0xED, 0x57, 0x83, 0x4D, 0x7F]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B8,
            ..
        }
    )));
    assert!(
        !broadcast
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 1016, .. },
            ..
        }
    )));

    let full = lift_single(&[0x62, 0x62, 0x8D, 0xC1, 0x83, 0x78, 0x01]).unwrap();
    assert!(full.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(
        !full
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    for bytes in [
        &[0xC4, 0xE2, 0xED, 0x83, 0xCB][..],       // EVEX-only
        &[0x62, 0xF2, 0xEC, 0x08, 0x83, 0xCB][..], // mandatory 66 absent
        &[0x62, 0xF2, 0x6D, 0x08, 0x83, 0xCB][..], // W=0
        &[0x62, 0xF2, 0xED, 0x68, 0x83, 0xCB][..], // L'L=3
        &[0x62, 0xF2, 0xED, 0x88, 0x83, 0xCB][..], // {z} with k0
        &[0x62, 0xF2, 0xED, 0x18, 0x83, 0xCB][..], // EVEX.b on register
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted reserved multishift encoding {bytes:02X?}"
        );
    }
}
#[test]
fn lift_evex_integer_narrow_covers_all_modes_ratios_registers_memory_and_invalids() {
    for high in [0x10u8, 0x20, 0x30] {
        for (low, src_elem, dst_elem) in [
            (0u8, VecElementType::I16, VecElementType::I8),
            (1, VecElementType::I32, VecElementType::I8),
            (2, VecElementType::I64, VecElementType::I8),
            (3, VecElementType::I32, VecElementType::I16),
            (4, VecElementType::I64, VecElementType::I16),
            (5, VecElementType::I64, VecElementType::I32),
        ] {
            let opcode = high | low;
            let bytes = [0x62, 0xF2, 0x7E, 0x09, opcode, 0xD1];
            let lifted = lift_single(&bytes).unwrap();
            assert!(matches!(
                lifted.ops.last().unwrap().kind,
                OpKind::X86NarrowInt {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    src_elem: actual_src,
                    dst_elem: actual_dst,
                    width: VecWidth::V128,
                    mode,
                    zeroing: false,
                } if actual_src == src_elem
                    && actual_dst == dst_elem
                    && mode == match high {
                        0x10 => X86NarrowMode::UnsignedSaturate,
                        0x20 => X86NarrowMode::SignedSaturate,
                        0x30 => X86NarrowMode::Truncate,
                        _ => unreachable!(),
                    }
            ));
        }
    }

    let high_register = lift_single(&[0x62, 0xA2, 0x7E, 0xCA, 0x30, 0xD1]).unwrap();
    assert!(matches!(
        high_register.ops.last().unwrap().kind,
        OpKind::X86NarrowInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(17))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            src_elem: VecElementType::I16,
            dst_elem: VecElementType::I8,
            width: VecWidth::V512,
            zeroing: true,
            ..
        }
    ));

    let memory = lift_single(&[0x62, 0xE2, 0x7E, 0x4A, 0x21, 0x50, 0x01]).unwrap();
    assert_eq!(
        memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredStore {
                    width: MemWidth::B1,
                    ..
                }
            ))
            .count(),
        16
    );
    assert!(!memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredStore {
            addr: Address::BaseIndexScale { .. },
            ..
        }
    )));

    for bytes in [
        &[0xC4, 0xE2, 0x7E, 0x21, 0xD1][..],       // EVEX-only
        &[0x62, 0xF2, 0xFE, 0x09, 0x21, 0xD1][..], // W=1
        &[0x62, 0xF2, 0x76, 0x09, 0x21, 0xD1][..], // EVEX.vvvv reserved
        &[0x62, 0xF2, 0x7E, 0x19, 0x21, 0xD1][..], // EVEX.b reserved
        &[0x62, 0xF2, 0x7E, 0x69, 0x21, 0xD1][..], // L'L=3
        &[0x62, 0xF2, 0x7E, 0x88, 0x21, 0xD1][..], // {z} with k0
        &[0x62, 0xE2, 0x7E, 0xCA, 0x21, 0x10][..], // memory destination cannot use {z}
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted reserved narrowing encoding {bytes:02X?}"
        );
    }
}
#[test]
fn lift_packed_immediate_shifts_covers_legacy_vex_evex_roles_memory_and_invalids() {
    for (bytes, width, elem, shift, byte_lane) in [
        (
            &[0xC4, 0xC1, 0x31, 0x71, 0xD2, 0x11][..],
            VecWidth::V128,
            VecElementType::I16,
            ShiftOp::Lsr,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x35, 0x71, 0xE2, 0x11][..],
            VecWidth::V256,
            VecElementType::I16,
            ShiftOp::Asr,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x31, 0x71, 0xF2, 0x11][..],
            VecWidth::V128,
            VecElementType::I16,
            ShiftOp::Lsl,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x35, 0x72, 0xD2, 0x21][..],
            VecWidth::V256,
            VecElementType::I32,
            ShiftOp::Lsr,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x31, 0x72, 0xE2, 0x21][..],
            VecWidth::V128,
            VecElementType::I32,
            ShiftOp::Asr,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x35, 0x72, 0xF2, 0x21][..],
            VecWidth::V256,
            VecElementType::I32,
            ShiftOp::Lsl,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x31, 0x73, 0xD2, 0x41][..],
            VecWidth::V128,
            VecElementType::I64,
            ShiftOp::Lsr,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x35, 0x73, 0xDA, 0x11][..],
            VecWidth::V256,
            VecElementType::I8,
            ShiftOp::Lsr,
            true,
        ),
        (
            &[0xC4, 0xC1, 0x31, 0x73, 0xF2, 0x41][..],
            VecWidth::V128,
            VecElementType::I64,
            ShiftOp::Lsl,
            false,
        ),
        (
            &[0xC4, 0xC1, 0x35, 0x73, 0xFA, 0x11][..],
            VecWidth::V256,
            VecElementType::I8,
            ShiftOp::Lsl,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(
            matches!(result.ops.as_slice(), [SmirOp { kind: OpKind::X86PackedShiftImm {
                dst: VReg::Arch(ArchReg::X86(dst)),
                src: VReg::Arch(ArchReg::X86(src)),
                width: actual_width,
                elem: actual_elem,
                shift: actual_shift,
                amount,
                byte_lane: actual_byte_lane,
            }, .. }] if *dst == if width == VecWidth::V128 { X86Reg::Xmm(9) } else { X86Reg::Ymm(9) }
                && *src == if width == VecWidth::V128 { X86Reg::Xmm(10) } else { X86Reg::Ymm(10) }
                && *actual_width == width && *actual_elem == elem && *actual_shift == shift
                && *amount == if elem == VecElementType::I16 { 0x11 } else if elem == VecElementType::I32 { 0x21 } else if byte_lane { 0x11 } else { 0x41 }
                && *actual_byte_lane == byte_lane)
        );
    }

    // W is ignored for this VEX.WIG family.
    assert!(lift_single(&[0xC4, 0xC1, 0xB1, 0x71, 0xD2, 0x01]).is_ok());

    let legacy = lift_single(&[0x66, 0x41, 0x0F, 0x71, 0xE1, 0x11]).unwrap();
    assert!(legacy.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedShiftImm {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            width: VecWidth::V128,
            elem: VecElementType::I16,
            shift: ShiftOp::Asr,
            amount: 17,
            byte_lane: false,
            ..
        }
    )));
    assert!(legacy.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            elem: VecElementType::I16,
            ..
        }
    )));
    let legacy_byte = lift_single(&[0x66, 0x41, 0x0F, 0x73, 0xF9, 0x01]).unwrap();
    assert!(legacy_byte.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedShiftImm {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            shift: ShiftOp::Lsl,
            amount: 1,
            byte_lane: true,
            ..
        }
    )));

    for (opcode, group, elem, shift) in [
        (0x71, 2, VecElementType::I16, ShiftOp::Lsr),
        (0x71, 4, VecElementType::I16, ShiftOp::Asr),
        (0x71, 6, VecElementType::I16, ShiftOp::Lsl),
        (0x72, 2, VecElementType::I32, ShiftOp::Lsr),
        (0x72, 4, VecElementType::I32, ShiftOp::Asr),
        (0x72, 6, VecElementType::I32, ShiftOp::Lsl),
        (0x73, 2, VecElementType::I64, ShiftOp::Lsr),
        (0x73, 6, VecElementType::I64, ShiftOp::Lsl),
    ] {
        let modrm = 0xC1 | (group << 3);
        let result = lift_single(&[0x0F, opcode, modrm, 0x11]).unwrap();
        assert!(matches!(
            result.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86PackedShiftImm {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        src: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        width: VecWidth::V64,
                        elem: actual_elem,
                        shift: actual_shift,
                        amount: 0x11,
                        byte_lane: false,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: actual_opcode,
                    }),
                    ..
                },
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    },
                    ..
                }
            ] if *actual_elem == elem && *actual_shift == shift && *actual_opcode == opcode
        ));
    }

    for (bytes, elem) in [
        (
            &[0x62, 0xB1, 0x7D, 0xC1, 0x72, 0xE2, 0x03][..],
            VecElementType::I32,
        ),
        (
            &[0x62, 0xB1, 0xFD, 0x41, 0x72, 0xE2, 0x04][..],
            VecElementType::I64,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86PackedShiftImm {
                src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                width: VecWidth::V512,
                elem: actual,
                shift: ShiftOp::Asr,
                ..
            } if actual == elem
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                elem: actual,
                ..
            } if actual == elem
        )));
    }

    let masked_dword_mem = lift_single(&[0x62, 0xF1, 0x7D, 0x49, 0x72, 0x50, 0x01, 0x03]).unwrap();
    assert_eq!(
        masked_dword_mem
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
        16,
    );
    assert!(masked_dword_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset { offset: 64, .. },
            ..
        }
    )));

    let masked_word_mem = lift_single(&[0x62, 0xF1, 0x7D, 0x49, 0x71, 0x60, 0x01, 0x03]).unwrap();
    assert!(masked_word_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(
        !masked_word_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    let broadcast = lift_single(&[0x62, 0xF1, 0x7D, 0x59, 0x72, 0x70, 0x04, 0x03]).unwrap();
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
        1,
    );
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            elem: VecElementType::I32,
            lanes: 16,
            ..
        }
    )));

    let byte_memory = lift_single(&[0x62, 0xF1, 0x7D, 0x48, 0x73, 0x78, 0x01, 0x01]).unwrap();
    assert!(byte_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset { offset: 64, .. },
            width: VecWidth::V512,
            ..
        }
    )));

    for bytes in [
        &[0xC4, 0xC1, 0x30, 0x71, 0xD2, 0x01][..],
        &[0xC4, 0xC1, 0x31, 0x71, 0xC2, 0x01][..],
        &[0xC4, 0xC1, 0x31, 0x72, 0xDA, 0x01][..],
        &[0xC4, 0xC1, 0x31, 0x73, 0xE2, 0x01][..],
        &[0xC4, 0xC1, 0x31, 0x71, 0x12, 0x01][..],
        &[0xC4, 0xC1, 0x31, 0x71, 0xD2][..],
        &[0x66, 0x0F, 0x71, 0x10, 0x01][..],
        &[0x0F, 0x73, 0xD8, 0x01][..],
        &[0x0F, 0x73, 0xF8, 0x01][..],
        &[0x62, 0xF1, 0x7D, 0xC0, 0x73, 0xD8, 0x01][..],
        &[0x62, 0xF1, 0x7D, 0x49, 0x73, 0xD8, 0x01][..],
        &[0x62, 0xF1, 0x7D, 0x58, 0x71, 0x10, 0x01][..],
        &[0x62, 0xF1, 0xFD, 0x48, 0x72, 0xD0, 0x01][..],
        &[0x62, 0xF1, 0x7D, 0x48, 0x73, 0xD0, 0x01][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid packed shift accepted: {bytes:02X?}"
        );
    }
}
#[test]
fn lift_evex_unaligned_integer_moves_covers_elements_stores_and_invalids() {
    for (bytes, elem, lanes, dst) in [
        (
            &[0x62, 0xF1, 0x7F, 0x49, 0x6F, 0xD1][..],
            VecElementType::I8,
            64,
            X86Reg::Zmm(2),
        ),
        (
            &[0x62, 0xF1, 0xFF, 0xCA, 0x6F, 0xE3][..],
            VecElementType::I16,
            32,
            X86Reg::Zmm(4),
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
    }

    for (bytes, elem, width, lanes) in [
        (
            &[0x62, 0xF1, 0x7F, 0x49, 0x6F, 0x10][..],
            VecElementType::I8,
            MemWidth::B1,
            64,
        ),
        (
            &[0x62, 0xF1, 0xFF, 0x4A, 0x6F, 0x10][..],
            VecElementType::I16,
            MemWidth::B2,
            32,
        ),
        (
            &[0x62, 0xF1, 0x7E, 0x4B, 0x6F, 0x10][..],
            VecElementType::I32,
            MemWidth::B4,
            16,
        ),
        (
            &[0x62, 0xF1, 0xFE, 0x4C, 0x6F, 0x10][..],
            VecElementType::I64,
            MemWidth::B8,
            8,
        ),
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
        assert_eq!(
            lifted
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                        elem: actual_elem,
                        ..
                    } if actual_elem == elem
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
        (&[0x62, 0xF1, 0x7F, 0x49, 0x7F, 0x08][..], MemWidth::B1, 64),
        (&[0x62, 0xF1, 0xFF, 0x4A, 0x7F, 0x08][..], MemWidth::B2, 32),
        (&[0x62, 0xF1, 0x7E, 0x4B, 0x7F, 0x08][..], MemWidth::B4, 16),
        (&[0x62, 0xF1, 0xFE, 0x4C, 0x7F, 0x08][..], MemWidth::B8, 8),
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

    let register_store = lift_single(&[0x62, 0xF1, 0x7F, 0xC9, 0x7F, 0xD1]).unwrap();
    assert!(register_store.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            elem: VecElementType::I8,
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
                    elem: VecElementType::I8,
                    ..
                }
            ))
            .count(),
        64
    );

    let high = lift_single(&[0x62, 0xA1, 0x7F, 0x49, 0x6F, 0xC8]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
            elem: VecElementType::I8,
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
                    elem: VecElementType::I8,
                    ..
                }
            ))
            .count(),
        64
    );

    let high_store = lift_single(&[0x62, 0xC1, 0xFE, 0x4C, 0x7F, 0x29]).unwrap();
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
                    elem: VecElementType::I64,
                    ..
                }
            ))
            .count(),
        8
    );

    for bytes in [
        &[0x62, 0xF1, 0xFF, 0x4A, 0x6F, 0x50, 0x01][..],
        &[0x62, 0xF1, 0x7E, 0x4B, 0x7F, 0x58, 0x01][..],
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
        &[0x62, 0xF1, 0x7F, 0xC8, 0x6F, 0xC1][..], // {z} with k0
        &[0x62, 0xF1, 0x7F, 0xC9, 0x7F, 0x08][..], // {z} memory store
        &[0x62, 0xF1, 0x7F, 0x69, 0x6F, 0xC1][..], // reserved L'L=3
        &[0x62, 0xF1, 0x7F, 0x59, 0x6F, 0xC1][..], // reserved EVEX.b
        &[0x62, 0xF1, 0x77, 0x49, 0x6F, 0xC1][..], // reserved vvvv
        &[0x62, 0xF1, 0x7F, 0x41, 0x6F, 0xC1][..], // reserved V'
        &[0x62, 0xF1, 0x7C, 0x49, 0x6F, 0xC1][..], // invalid mandatory prefix
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid masked VMOVDQU* encoding accepted: {bytes:02X?}"
        );
    }
}
