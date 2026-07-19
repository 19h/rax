//! misc part 3 tests

use super::*;
use crate::smir::ir::types::DispSize;
use crate::smir::optimize::tests::*;
use crate::smir::optimize::*;

#[test]
fn test_optimize_function() {
    use crate::smir::ir::FunctionBuilder;

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

    let v0 = builder.alloc_vreg();
    let v1 = builder.alloc_vreg();
    let v2 = builder.alloc_vreg();
    let v3 = builder.alloc_vreg();

    // mov v0, 10
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: v0,
            src: SrcOperand::Imm(10),
            width: OpWidth::W64,
        },
    );

    // add v1, v0, 5 (with flags)
    builder.push_op(
        0x1004,
        OpKind::Add {
            dst: v1,
            src1: v0,
            src2: SrcOperand::Imm(5),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );

    // mov v2, 100 (dead)
    builder.push_op(
        0x1008,
        OpKind::Mov {
            dst: v2,
            src: SrcOperand::Imm(100),
            width: OpWidth::W64,
        },
    );

    // and v3, v1, 0 -> should fold to mov v3, 0
    builder.push_op(
        0x100c,
        OpKind::And {
            dst: v3,
            src1: v1,
            src2: SrcOperand::Imm(0),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );

    builder.set_terminator(Terminator::Return { values: vec![v3] });

    let mut func = builder.finish();

    let stats = optimize_function(&mut func, OptLevel::O2);

    // Should have optimizations applied
    assert!(stats.total() > 0);
}
#[test]
fn test_opt_stats() {
    let mut stats1 = OptStats::new();
    stats1.dead_flags_eliminated = 5;
    stats1.constants_propagated = 3;

    let mut stats2 = OptStats::new();
    stats2.dead_ops_eliminated = 2;
    stats2.expressions_folded = 1;

    stats1.merge(&stats2);

    assert_eq!(stats1.dead_flags_eliminated, 5);
    assert_eq!(stats1.constants_propagated, 3);
    assert_eq!(stats1.dead_ops_eliminated, 2);
    assert_eq!(stats1.expressions_folded, 1);
    assert_eq!(stats1.total(), 11);
}
#[test]
fn o2_preserves_aarch32_blx_lr_snapshot_and_link_write() {
    let snapshot = VReg::virt(0);
    let lr = VReg::Arch(ArchReg::Arm(ArmReg::X(14)));
    let entry = BlockId(0);
    let continuation = BlockId(1);
    let mut block = SmirBlock::new(entry, 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: snapshot,
            src: SrcOperand::Reg(lr),
            width: OpWidth::W32,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::Mov {
            dst: lr,
            src: SrcOperand::Imm(0x1004),
            width: OpWidth::W32,
        },
    ));
    block.set_terminator(Terminator::Call {
        target: CallTarget::IndirectInterworking(snapshot),
        args: Vec::new(),
        continuation,
    });
    let mut continuation_block = SmirBlock::new(continuation, 0x1004);
    continuation_block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), entry, 0x1000);
    function.add_block(block);
    function.add_block(continuation_block);

    optimize_function(&mut function, OptLevel::O2);
    let block = function.get_block(entry).unwrap();
    assert_eq!(block.ops.len(), 2);
    assert!(matches!(
        block.ops[0].kind,
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(source),
            width: OpWidth::W32,
        } if dst == snapshot && source == lr
    ));
    assert!(matches!(
        block.ops[1].kind,
        OpKind::Mov {
            dst,
            src: SrcOperand::Imm(0x1004),
            width: OpWidth::W32,
        } if dst == lr
    ));
}

#[test]
fn o2_preserves_every_addr32_memory_call_target_definition() {
    let base_snapshot = VReg::virt(0);
    let index_snapshot = VReg::virt(1);
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let entry = BlockId(0);
    let continuation = BlockId(1);
    let mut block = SmirBlock::new(entry, 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: base_snapshot,
            src: SrcOperand::Reg(rax),
            width: OpWidth::W32,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::Mov {
            dst: index_snapshot,
            src: SrcOperand::Reg(rcx),
            width: OpWidth::W32,
        },
    ));
    block.set_terminator(Terminator::Call {
        target: CallTarget::X86IndirectMemAddr32(Address::BaseIndexScale {
            base: Some(base_snapshot),
            index: index_snapshot,
            scale: 8,
            disp: -1,
            disp_size: DispSize::Disp8,
        }),
        args: Vec::new(),
        continuation,
    });
    let mut continuation_block = SmirBlock::new(continuation, 0x1004);
    continuation_block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), entry, 0x1000);
    function.add_block(block);
    function.add_block(continuation_block);

    optimize_function(&mut function, OptLevel::O2);

    let block = function.get_block(entry).unwrap();
    assert_eq!(block.ops.len(), 2);
    assert!(
        block
            .ops
            .iter()
            .any(|op| op.kind.dests() == vec![base_snapshot])
    );
    assert!(
        block
            .ops
            .iter()
            .any(|op| op.kind.dests() == vec![index_snapshot])
    );
}
