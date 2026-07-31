//! Packed integer minimum/maximum optimizer fault-boundary tests.

use super::*;
use crate::smir::optimize::tests::*;
use crate::smir::optimize::*;

#[test]
fn optimizer_preserves_evex_packed_minmax_memory_fault_boundaries() {
    use crate::smir::ir::types::{SourceArch, VecWidth, X86Reg};
    use crate::smir::ir::{FunctionBuilder, SmirFunction};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    fn optimized(bytes: &[u8]) -> SmirFunction {
        let mut lifter = X86_64Lifter::new();
        let mut lctx = LiftContext::new(SourceArch::X86_64);
        let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut func = builder.finish();
        func.blocks[0].ops = result.ops;
        optimize_function(&mut func, OptLevel::O2);
        func
    }

    for (bytes, elem, mem_width, expected_loads) in [
        (
            &[0x62, 0xF1, 0x75, 0x49, 0xDA, 0x00][..],
            VecElementType::I8,
            MemWidth::B1,
            64usize,
        ),
        (
            &[0x62, 0xF1, 0x75, 0x49, 0xEA, 0x00][..],
            VecElementType::I16,
            MemWidth::B2,
            32usize,
        ),
        (
            &[0x62, 0xF2, 0x75, 0x49, 0x38, 0x00][..],
            VecElementType::I8,
            MemWidth::B1,
            64usize,
        ),
        (
            &[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x00][..],
            VecElementType::I64,
            MemWidth::B8,
            1usize,
        ),
    ] {
        let evex_minmax = optimized(bytes);
        let ops = &evex_minmax.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad { width, .. } if width == mem_width
                ))
                .count(),
            expected_loads,
            "EVEX packed min/max lost its E4 memory-access contract"
        );
        assert!(!ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load { .. } | OpKind::VLoad { .. } | OpKind::X86CheckAlignment { .. }
        )));
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad { width, .. } if width == mem_width
                )
            })
            .unwrap();
        let compare = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::VCmp { elem: actual, .. } if actual == elem))
            .unwrap();
        let select = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBitSelect {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: actual,
                        ..
                    } if actual == elem
                )
            })
            .unwrap();
        assert!(last_load < compare && compare < select && select < destination_write);
    }
}
