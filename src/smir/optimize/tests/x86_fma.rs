//! Optimizer contracts for x86 FMA status effects and source liveness.

use super::*;
use crate::smir::ir::ops::X86FmaOp;
use crate::smir::ir::types::{X86FmaKind, X86FmaOrder};
use crate::smir::optimize::dead_code_elimination;

fn fma(round: FpRoundMode, lanes: u8) -> X86FmaOp {
    X86FmaOp {
        dst: VReg::virt(3),
        src1: VReg::virt(0),
        src2: VReg::virt(1),
        src3: VReg::virt(2),
        mask: Some(VReg::virt(4)),
        elem: VecElementType::F32,
        kind: X86FmaKind::Add,
        order: X86FmaOrder::Order132,
        round,
        lanes,
    }
}

fn fma4(elem: VecElementType, lanes: u8) -> X86FmaOp {
    X86FmaOp {
        dst: VReg::virt(3),
        src1: VReg::virt(0),
        src2: VReg::virt(1),
        src3: VReg::virt(2),
        mask: None,
        elem,
        kind: X86FmaKind::Add,
        order: X86FmaOrder::Order123,
        round: FpRoundMode::Dynamic,
        lanes,
    }
}

#[test]
fn x86_fma_metadata_tracks_all_sources_destination_and_mxcsr_effects() {
    let dynamic = OpKind::X86Fma(fma(FpRoundMode::Dynamic, 4));
    assert_eq!(dynamic.dests(), vec![VReg::virt(3)]);
    assert_eq!(
        dynamic.source_vregs(),
        vec![VReg::virt(0), VReg::virt(1), VReg::virt(2), VReg::virt(4),]
    );
    assert!(dynamic.has_side_effects());
    assert!(!dynamic.is_jit_safe());
    assert!(!make_op(0, dynamic.clone()).is_jit_safe());

    let embedded = OpKind::X86Fma(fma(FpRoundMode::RoundNearest, 1));
    assert!(!embedded.has_side_effects());
    assert!(!embedded.is_jit_safe());
    assert!(!make_op(0, embedded.clone()).is_jit_safe());
    let malformed_embedded = OpKind::X86Fma(fma(FpRoundMode::RoundNearest, 4));
    assert!(malformed_embedded.has_side_effects());
    assert!(!malformed_embedded.is_jit_safe());
    assert!(!make_op(0, malformed_embedded).is_jit_safe());
}

#[test]
fn dead_code_elimination_preserves_dynamic_and_malformed_x86_fma_boundaries() {
    for operation in [
        OpKind::X86Fma(fma(FpRoundMode::Dynamic, 4)),
        OpKind::X86Fma(fma(FpRoundMode::RoundNearest, 4)),
    ] {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(0, operation));
        block.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(dead_code_elimination(&mut block), 0);
        assert_eq!(block.ops.len(), 1);
    }

    let mut embedded = SmirBlock::new(BlockId(0), 0x1000);
    embedded.push_op(make_op(
        0,
        OpKind::X86Fma(fma(FpRoundMode::RoundNearest, 1)),
    ));
    embedded.set_terminator(Terminator::Return { values: vec![] });
    assert_eq!(dead_code_elimination(&mut embedded), 1);
    assert!(embedded.ops.is_empty());
}

#[test]
fn x86_fma4_shape_and_optimizer_contracts_are_exact_and_fail_closed() {
    for (elem, lanes) in [
        (VecElementType::F32, 1),
        (VecElementType::F32, 4),
        (VecElementType::F32, 8),
        (VecElementType::F64, 1),
        (VecElementType::F64, 2),
        (VecElementType::F64, 4),
    ] {
        let exact = fma4(elem, lanes);
        assert!(exact.shape_valid(), "{elem:?} x {lanes}");
        let operation = OpKind::X86Fma(exact);
        assert_eq!(operation.dests(), vec![VReg::virt(3)]);
        assert_eq!(
            operation.source_vregs(),
            vec![VReg::virt(0), VReg::virt(1), VReg::virt(2)]
        );
        assert!(operation.has_side_effects());
        assert!(!operation.is_jit_safe());

        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(0, operation));
        block.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(dead_code_elimination(&mut block), 0);
        assert_eq!(block.ops.len(), 1);
    }

    let mut malformed = [
        fma4(VecElementType::F32, 16),
        fma4(VecElementType::F64, 8),
        X86FmaOp {
            mask: Some(VReg::virt(4)),
            ..fma4(VecElementType::F32, 4)
        },
        X86FmaOp {
            round: FpRoundMode::RoundNearest,
            ..fma4(VecElementType::F32, 4)
        },
    ];
    malformed[0].kind = X86FmaKind::AddSub;
    for fma in malformed {
        assert!(!fma.shape_valid());
        let operation = OpKind::X86Fma(fma);
        assert!(operation.has_side_effects());
        assert!(!operation.is_jit_safe());
    }
}
