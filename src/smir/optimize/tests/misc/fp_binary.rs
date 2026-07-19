//! Exact x86 scalar floating-point optimizer frontier tests.

use super::*;
use crate::smir::ir::types::{FpRoundMode, SourceArch, X86FpBinaryOp, X86Reg};
use crate::smir::ir::{FunctionBuilder, SmirFunction};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::tests::*;

fn optimized(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::new();
    let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
    let lifted = lifter.lift_insn(0x1000, bytes, &mut lift_ctx).unwrap();
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops = lifted.ops;
    optimize_function(&mut function, OptLevel::O2);
    function
}

#[test]
fn optimizer_preserves_legacy_and_evex_scalar_min_memory_frontiers() {
    let legacy_min = optimized(&[0xF3, 0x0F, 0x5D, 0x00]);
    let ops = &legacy_min.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("faulting MINSS load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    ..
                }
            )
        })
        .expect("MINSS destination merge must survive optimization");
    assert!(load < destination_write);
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VX86MinMax {
            min: true,
            lanes: 1,
            ..
        }
    )));

    let evex_min = optimized(&[0x62, 0xF1, 0x7E, 0x09, 0x5D, 0x10]);
    let ops = &evex_min.blocks[0].ops;
    let pred_load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("masked EVEX VMINSS conditional load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    ..
                }
            )
        })
        .expect("masked EVEX VMINSS destination write must survive optimization");
    assert!(pred_load < destination_write);
    assert!(ops.iter().any(|op| matches!(
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
        ops.iter()
            .any(|op| matches!(op.kind, OpKind::Select { .. }))
    );
}
