//! part1 part 2 tests

use super::*;
use crate::smir::optimize::tests::*;
use crate::smir::optimize::*;

#[test]
fn o2_removes_internal_bit_test_overwritten_before_frontier() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Bt {
            src: rax,
            index: SrcOperand::Imm(7),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1001,
        OpKind::Cmp {
            src1: rax,
            src2: SrcOperand::Reg(rcx),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();

    let stats = optimize_function_with_stats(&mut function, OptLevel::O2);
    assert_eq!(stats.dead_flags_eliminated, 1);
    assert!(
        !function.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::Bt { .. } | OpKind::Nop))
    );
    assert!(
        function.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::Cmp { .. }))
    );
}
