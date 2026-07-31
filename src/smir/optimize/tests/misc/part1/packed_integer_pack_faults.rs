//! Optimizer fault-frontier tests for saturating integer packs.

use super::*;
use crate::smir::ir::types::SourceArch;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::optimize::tests::*;
use crate::smir::optimize::*;

fn optimized(bytes: &[u8], level: OptLevel) -> SmirFunction {
    let mut lifter = X86_64Lifter::new();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter.lift_insn(0x1000, bytes, &mut context).unwrap();
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops = result.ops;
    optimize_function(&mut function, level);
    function
}

#[test]
fn optimizer_preserves_saturating_pack_complete_memory_fault_frontiers() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let vex_pack = optimized(&[0xC5, 0xF5, 0x63, 0x00], level);
        let ops = &vex_pack.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("faulting VPACKSSWB source load must survive optimization");
        let pack = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VPackSat {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                        src_elem: VecElementType::I16,
                        src_lanes: 16,
                        block_lanes: 8,
                        ..
                    }
                )
            })
            .expect("VPACKSSWB architectural pack write must survive optimization");
        assert!(
            load < pack,
            "VPACKSSWB changed its destination before the memory fault boundary at {level:?}"
        );

        let evex_pack = optimized(&[0x62, 0xF1, 0x75, 0x49, 0x6B, 0x00], level);
        let ops = &evex_pack.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .expect("E4NF VPACKSSDW complete source load must survive optimization");
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
            "optimizer introduced fault suppression for E4NF VPACKSSDW at {level:?}"
        );
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("masked EVEX pack destination writes must survive optimization");
        assert!(
            load < destination_write,
            "EVEX pack committed before its complete E4NF access at {level:?}"
        );

        let broadcast = optimized(&[0x62, 0xF1, 0x75, 0x59, 0x6B, 0x00], level);
        let ops = &broadcast.blocks[0].ops;
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
            .expect("E4NF VPACKSSDW broadcast scalar load must survive optimization");
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast {
                elem: VecElementType::I32,
                lanes: 16,
                ..
            }
        )));
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: VecElementType::I16,
                        ..
                    }
                )
            })
            .expect("masked broadcast pack destination writes must survive optimization");
        assert!(
            load < destination_write,
            "EVEX broadcast pack committed before its E4NF scalar access at {level:?}"
        );
    }
}
