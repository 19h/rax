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
