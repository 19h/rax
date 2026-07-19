//! VEX/EVEX scalar FP32/FP64 move and arithmetic lifting tests.

use super::*;

#[test]
fn lift_vex_scalar_moves_and_arithmetic_merge_xmm_and_zero_upper_state() {
    let vaddss = lift_single(&[0xC5, 0xF2, 0x58, 0xC2]).unwrap();
    assert!(vaddss.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            mask: None,
            elem: VecElementType::F32,
            lanes: 1,
            op: X86FpBinaryOp::Add,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            ..
        }
    )));
    assert!(vaddss.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            lane: 3,
            elem: VecElementType::F32,
            ..
        }
    )));
    assert!(vaddss.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::F32,
            lanes: 1,
            ..
        }
    )));

    for (bytes, expected) in [
        (&[0xC5, 0xF2, 0x59, 0xC2][..], X86FpBinaryOp::Mul),
        (&[0xC5, 0xF3, 0x5C, 0xC2][..], X86FpBinaryOp::Sub),
        (&[0xC5, 0xF2, 0x5D, 0xC2][..], X86FpBinaryOp::Min),
        (&[0xC5, 0xF2, 0x5E, 0xC2][..], X86FpBinaryOp::Div),
        (&[0xC5, 0xF3, 0x5F, 0xC2][..], X86FpBinaryOp::Max),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(
            result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86FpBinary {
                    op,
                    lanes: 1,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                } if op == expected
            )),
            "{expected:?}: {:?}",
            result.ops,
        );
    }

    let vmovss = lift_single(&[0xC5, 0xFA, 0x10, 0xD1]).unwrap();
    assert!(vmovss.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            lane: 0,
            elem: VecElementType::F32,
            ..
        }
    )));
    assert!(vmovss.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            lane: 3,
            elem: VecElementType::F32,
            ..
        }
    )));
    assert!(vmovss.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            ..
        }
    )));

    let reverse = lift_single(&[0xC5, 0xF2, 0x11, 0xC2]).unwrap();
    assert!(reverse.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            lane: 0,
            ..
        }
    )));
    assert!(reverse.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            ..
        }
    )));

    let memory = lift_single(&[0xC5, 0xFA, 0x10, 0x00]).unwrap();
    assert!(matches!(
        memory.ops.first().unwrap().kind,
        OpKind::Load {
            width: MemWidth::B4,
            ..
        }
    ));
    assert!(matches!(
        memory.ops.last().unwrap().kind,
        OpKind::VBroadcast {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::F32,
            lanes: 1,
            ..
        }
    ));

    let packed = lift_single(&[0xC5, 0xF8, 0x10, 0xC1]).unwrap();
    assert!(matches!(
        packed.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VAnd {
                width: VecWidth::V128,
                ..
            },
            ..
        }]
    ));

    for bytes in [
        &[0xC5, 0xF2, 0x10, 0x00][..], // memory VMOVSS with non-reserved vvvv
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "unsupported/reserved scalar form accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_evex_scalar_masking_fault_suppression_and_high_registers() {
    let merge = lift_single(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0xD1]).unwrap();
    assert!(merge.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::And {
            src1: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
            src2: SrcOperand::Imm(1),
            flags: FlagUpdate::None,
            ..
        }
    )));
    assert!(merge.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            lane: 0,
            ..
        }
    )));
    assert!(
        merge
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::Select { .. }))
    );
    assert!(merge.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            mask: Some(_),
            elem: VecElementType::F32,
            lanes: 1,
            op: X86FpBinaryOp::Add,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
            ..
        }
    )));

    for (bytes, expected_elem, expected) in [
        (
            &[0x62, 0xF1, 0x7E, 0x0A, 0x59, 0xE1][..],
            VecElementType::F32,
            X86FpBinaryOp::Mul,
        ),
        (
            &[0x62, 0xF1, 0x7E, 0x8A, 0x5E, 0xE9][..],
            VecElementType::F32,
            X86FpBinaryOp::Div,
        ),
        (
            &[0x62, 0xF1, 0xFF, 0x09, 0x58, 0xD1][..],
            VecElementType::F64,
            X86FpBinaryOp::Add,
        ),
        (
            &[0x62, 0xF1, 0xFF, 0x89, 0x5C, 0xD9][..],
            VecElementType::F64,
            X86FpBinaryOp::Sub,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(
            result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86FpBinary {
                    elem,
                    lanes: 1,
                    op,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                } if elem == expected_elem && op == expected
            )),
            "{expected:?}: {:?}",
            result.ops,
        );
    }

    let zero = lift_single(&[0x62, 0xF1, 0x7E, 0x89, 0x5C, 0xD1]).unwrap();
    assert!(zero.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Mov {
            src: SrcOperand::Imm(0),
            width: OpWidth::W32,
            ..
        }
    )));
    assert!(!zero.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            lane: 0,
            ..
        }
    )));

    let masked_memory = lift_single(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0x10]).unwrap();
    assert!(masked_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(
        !masked_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::Load { .. }))
    );

    let compressed = lift_single(&[0x62, 0xF1, 0x7E, 0x08, 0x58, 0x57, 0x10]).unwrap();
    assert!(compressed.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            width: MemWidth::B4,
            ..
        }
    )));
    let compressed_sd = lift_single(&[0x62, 0xF1, 0xFF, 0x08, 0x58, 0x5F, 0x08]).unwrap();
    assert!(compressed_sd.ops.iter().any(|op| matches!(
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

    let high = lift_single(&[0x62, 0xA1, 0x7E, 0x00, 0x58, 0xD1]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            op: X86FpBinaryOp::Add,
            ..
        }
    )));
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            ..
        }
    )));

    let high_move = lift_single(&[0x62, 0xA1, 0x7E, 0x00, 0x10, 0xD1]).unwrap();
    assert!(high_move.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            lane: 0,
            ..
        }
    )));
    assert!(high_move.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            ..
        }
    )));

    for (p2, expected_round) in [
        (0x18, FpRoundMode::RoundNearest),
        (0x38, FpRoundMode::RoundDown),
        (0x58, FpRoundMode::RoundUp),
        (0x78, FpRoundMode::RoundTowardZero),
    ] {
        for (p1, opcode, expected_op) in [
            (0x7E, 0x58, X86FpBinaryOp::Add),
            (0x7E, 0x59, X86FpBinaryOp::Mul),
            (0x7E, 0x5C, X86FpBinaryOp::Sub),
            (0x7E, 0x5E, X86FpBinaryOp::Div),
            (0xFF, 0x58, X86FpBinaryOp::Add),
            (0xFF, 0x59, X86FpBinaryOp::Mul),
            (0xFF, 0x5C, X86FpBinaryOp::Sub),
            (0xFF, 0x5E, X86FpBinaryOp::Div),
        ] {
            let lifted = lift_single(&[0x62, 0xF1, p1, p2, opcode, 0xD1]).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86FpBinary {
                    op,
                    round,
                    suppress_exceptions: true,
                    lanes: 1,
                    ..
                } if op == expected_op && round == expected_round
            )));
        }
    }

    for p1 in [0x7E, 0xFF] {
        for (opcode, expected_op) in [(0x5D, X86FpBinaryOp::Min), (0x5F, X86FpBinaryOp::Max)] {
            let lifted = lift_single(&[0x62, 0xF1, p1, 0x78, opcode, 0xD1]).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86FpBinary {
                    op,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: true,
                    ..
                } if op == expected_op
            )));
        }
    }

    for bytes in [
        &[0x62, 0xF1, 0x7E, 0x88, 0x58, 0xD1][..], // {z} with k0
        &[0x62, 0xF1, 0x7E, 0x18, 0x58, 0x11][..], // EVEX.b with memory source
        &[0x62, 0xF1, 0xFE, 0x08, 0x58, 0xD1][..], // VADDSS with W=1
        &[0x62, 0xF1, 0x7F, 0x08, 0x58, 0xD1][..], // VADDSD with W=0
        &[0x62, 0xF1, 0xFE, 0x08, 0x10, 0xD1][..], // VMOVSS with W=1
        &[0x62, 0xF1, 0x7F, 0x08, 0x10, 0xD1][..], // VMOVSD with W=0
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
