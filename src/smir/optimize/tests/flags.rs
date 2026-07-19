//! tests::flags tests

use super::*;
use crate::smir::optimize::*;

#[test]
fn x86_string_compare_flag_metadata_handles_zero_count_rep() {
    let plain = string_compare(X86RepMode::None);
    assert_eq!(plain.flags_written(), FlagSet::ALL_X86);
    assert_eq!(plain.flags_must_write(), FlagSet::ALL_X86);

    for rep in [X86RepMode::Repe, X86RepMode::Repne] {
        let repeated = string_compare(rep);
        assert_eq!(repeated.flags_written(), FlagSet::ALL_X86);
        assert_eq!(repeated.flags_must_write(), FlagSet::EMPTY);
    }
}
#[test]
fn x86_bls_metadata_tracks_source_and_partial_flags() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let src = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let defined = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    let bls = OpKind::X86Bls {
        dst,
        src,
        width: OpWidth::W64,
        kind: X86BlsKind::Blsmsk,
        flags: FlagUpdate::Specific(defined),
    };
    assert_eq!(bls.dests(), vec![dst]);
    assert_eq!(bls.source_vregs(), vec![src]);
    assert_eq!(bls.flags_written(), defined);
    assert_eq!(bls.flags_must_write(), defined);
    assert!(op_fully_defines(&bls));

    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(0, bls));
    block.set_terminator(Terminator::Return { values: vec![] });
    assert_eq!(dead_flag_elimination(&mut block), 1);
    assert!(matches!(
        block.ops[0].kind,
        OpKind::X86Bls {
            flags: FlagUpdate::None,
            ..
        }
    ));
}
#[test]
fn mov_from_arm_nzcv_keeps_prior_flag_update_live() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    let cmp_result = VReg::virt(0);
    let src1 = VReg::virt(1);
    let cmp_nzcv = VReg::virt(2);
    let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));

    block.push_op(make_op(
        0,
        OpKind::Sub {
            dst: cmp_result,
            src1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::Mov {
            dst: cmp_nzcv,
            src: SrcOperand::Reg(nzcv),
            width: OpWidth::W32,
        },
    ));
    block.set_terminator(Terminator::Return {
        values: vec![cmp_nzcv],
    });

    let eliminated = dead_flag_elimination(&mut block);
    assert_eq!(eliminated, 0);
    let OpKind::Sub { flags, .. } = &block.ops[0].kind else {
        panic!("expected compare op");
    };
    assert_eq!(*flags, FlagUpdate::All);

    let removed = dead_code_elimination(&mut block);
    assert_eq!(removed, 0);
}
#[test]
fn optimize_function_preserves_cond_compare_flags_for_nzcv_select() {
    use crate::smir::ir::FunctionBuilder;

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let cond = builder.alloc_vreg();
    let cmp_result = builder.alloc_vreg();
    let cmp_nzcv = builder.alloc_vreg();
    let final_nzcv = builder.alloc_vreg();
    let nzcv = VReg::Arch(ArchReg::Arm(ArmReg::Nzcv));

    builder.push_op(
        0x1000,
        OpKind::TestCondition {
            dst: cond,
            cond: Condition::Eq,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Sub {
            dst: cmp_result,
            src1: VReg::Arch(ArchReg::Arm(ArmReg::X(1))),
            src2: SrcOperand::Reg(VReg::Arch(ArchReg::Arm(ArmReg::X(2)))),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: cmp_nzcv,
            src: SrcOperand::Reg(nzcv),
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Select {
            dst: final_nzcv,
            cond,
            src_true: cmp_nzcv,
            src_false: VReg::Imm(0x4000_0000),
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: nzcv,
            src: SrcOperand::Reg(final_nzcv),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut func = builder.finish();
    optimize_function(&mut func, OptLevel::O2);

    assert!(
        func.blocks[0].ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Sub {
                flags: FlagUpdate::All,
                ..
            }
        )),
        "conditional compare must keep the flag-producing compare op"
    );
}
// Regression for issue #23: a non-LOCK memory XADD lifts to a flag-free,
// store-feeding Add, then the Store, then the source writeback, then a
// flag-producing Add that commits the arithmetic flags only AFTER the store
// has retired. The optimizer must preserve that ordering: it may neither sink
// the flag-producing Add before the Store (which would re-expose flags on a
// faulting store) nor drop it while its flags are live. This optimizes a real
// lifted XADD with all flags live-out (Return frontier) and asserts the
// flag-producing Add survives and stays after the Store.
#[test]
fn issue_23_optimizer_keeps_xadd_flag_add_after_store() {
    use crate::smir::ir::FunctionBuilder;
    use crate::smir::ir::types::SourceArch;
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    // xadd dword ptr [rax], ecx (0F C1 08): a non-LOCK memory XADD.
    let mut lifter = X86_64Lifter::new();
    let mut lctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, &[0x0F, 0xC1, 0x08], &mut lctx)
        .unwrap();

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for op in result.ops {
        builder.push_op(op.guest_pc, op.kind);
    }
    // A Return frontier exit makes every architectural flag live-out, so the
    // flag-producing Add must be kept.
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut func = builder.finish();

    optimize_function(&mut func, OptLevel::O2);

    let ops = &func.blocks[0].ops;
    let store_pos = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::Store { .. }))
        .expect("memory XADD must keep its store");
    let flag_add_positions: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| {
            matches!(
                op.kind,
                OpKind::Add {
                    flags: FlagUpdate::All,
                    ..
                }
            )
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        flag_add_positions.len(),
        1,
        "exactly one flag-producing Add must survive (not fused/duplicated)"
    );
    assert!(
        flag_add_positions[0] > store_pos,
        "the flag-producing Add must remain AFTER the store so a faulting \
             store cannot commit flags (store at {store_pos}, flag add at {})",
        flag_add_positions[0],
    );
}
#[test]
fn optimizer_keeps_generic_memory_rmw_flag_commits_after_store() {
    use crate::smir::ir::FunctionBuilder;
    use crate::smir::ir::types::SourceArch;
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    for (name, bytes) in [
        ("add", &[0x01, 0x08][..]),
        ("adc immediate", &[0x83, 0x10, 0x01][..]),
        ("shift", &[0xC1, 0x20, 0x01][..]),
        ("rcr", &[0x48, 0xD3, 0x18][..]),
        ("neg", &[0xF7, 0x18][..]),
        ("inc", &[0x48, 0xFF, 0x00][..]),
    ] {
        let mut lifter = X86_64Lifter::new();
        let mut lctx = LiftContext::new(SourceArch::X86_64);
        let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for op in result.ops {
            builder.push_op(op.guest_pc, op.kind);
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut func = builder.finish();
        optimize_function(&mut func, OptLevel::O2);

        let ops = &func.blocks[0].ops;
        let store = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::Store { .. }))
            .unwrap_or_else(|| panic!("{name}: store removed"));
        assert!(
            ops[..store]
                .iter()
                .all(|op| op.kind.flags_written().is_empty()),
            "{name}: optimizer exposed flags before store: {ops:?}",
        );
        assert!(
            ops[store + 1..]
                .iter()
                .any(|op| !op.kind.flags_written().is_empty()),
            "{name}: optimizer removed post-store flag commit: {ops:?}",
        );
    }
}
#[test]
fn optimizer_keeps_locked_memory_rmw_flag_commits_after_atomic_write() {
    use crate::smir::ir::FunctionBuilder;
    use crate::smir::ir::types::SourceArch;
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    for (name, bytes) in [
        ("lock add", &[0xF0, 0x01, 0x08][..]),
        ("lock adc", &[0xF0, 0x83, 0x10, 0x01][..]),
        ("lock inc", &[0xF0, 0x48, 0xFF, 0x00][..]),
        ("lock neg", &[0xF0, 0xF7, 0x18][..]),
    ] {
        let mut lifter = X86_64Lifter::new();
        let mut lctx = LiftContext::new(SourceArch::X86_64);
        let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for op in result.ops {
            builder.push_op(op.guest_pc, op.kind);
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut func = builder.finish();
        optimize_function(&mut func, OptLevel::O2);

        let ops = &func.blocks[0].ops;
        let atomic = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::AtomicRmw { .. }))
            .unwrap_or_else(|| panic!("{name}: atomic write removed"));
        assert!(
            ops[..atomic]
                .iter()
                .all(|op| op.kind.flags_written().is_empty()),
            "{name}: optimizer exposed flags before atomic write: {ops:?}",
        );
        assert!(
            ops[atomic + 1..]
                .iter()
                .any(|op| !op.kind.flags_written().is_empty()),
            "{name}: optimizer removed post-atomic flag commit: {ops:?}",
        );
    }
}
