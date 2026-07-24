//! x86 FMA3 source-order and embedded-rounding lift contracts.

use super::*;
use crate::smir::ir::ops::X86FmaOp;

#[test]
fn evex_fma3_embedded_rounding_accepts_every_rc_and_implies_full_width() {
    for (p2, expected_round) in [
        (0x18, FpRoundMode::RoundNearest),
        (0x38, FpRoundMode::RoundDown),
        (0x58, FpRoundMode::RoundUp),
        (0x78, FpRoundMode::RoundTowardZero),
    ] {
        for (p1, elem, lanes) in [
            (0x75, VecElementType::F32, 16),
            (0xF5, VecElementType::F64, 8),
        ] {
            let lifted = lift_single(&[0x62, 0xF2, p1, p2, 0x98, 0xC2]).unwrap();
            assert!(lifted.ops.iter().any(|op| matches!(
                op,
                SmirOp {
                    kind: OpKind::X86Fma(X86FmaOp {
                        elem: actual_elem,
                        round,
                        lanes: actual_lanes,
                        ..
                    }),
                    x86_hint: Some(X86OpHint::EvexOp {
                        width: VecWidth::V512,
                        ..
                    }),
                    ..
                } if *actual_elem == elem && *round == expected_round && *actual_lanes == lanes
            )));
        }

        let scalar = lift_single(&[0x62, 0xF2, 0x75, p2, 0x99, 0xC2]).unwrap();
        assert!(scalar.ops.iter().any(|op| matches!(
            op,
            SmirOp {
                kind: OpKind::X86Fma(X86FmaOp {
                    elem: VecElementType::F32,
                    round,
                    lanes: 1,
                    ..
                }),
                x86_hint: Some(X86OpHint::EvexOp {
                    width: VecWidth::V128,
                    ..
                }),
                ..
            } if *round == expected_round
        )));
    }
}

#[test]
fn evex_scalar_fma3_rejects_memory_evex_b_because_broadcast_is_unsupported() {
    for (p1, opcode) in [(0x75, 0x99), (0xF5, 0x99)] {
        assert!(matches!(
            lift_single(&[0x62, 0xF2, p1, 0x18, opcode, 0x02]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn x86_fma3_ir_retains_architectural_sources_mask_kind_and_order() {
    let lifted = lift_single(&[0x62, 0xA2, 0x75, 0x43, 0xB6, 0xC2]).unwrap();
    assert!(lifted.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86Fma(X86FmaOp {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            src3: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
            elem: VecElementType::F32,
            kind: X86FmaKind::AddSub,
            order: X86FmaOrder::Order231,
            round: FpRoundMode::Dynamic,
            lanes: 16,
            ..
        })
    )));
    assert_eq!(
        lifted
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Fma(_)))
            .count(),
        1,
        "alternating FMA must aggregate exceptions in one semantic operation"
    );
}
