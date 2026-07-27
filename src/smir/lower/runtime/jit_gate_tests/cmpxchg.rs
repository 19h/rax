//! Native admission for memory-destination `CMPXCHG`.

use super::*;
use crate::smir::lower::runtime::x86_jit_cmpxchg_sequence_len;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn virt(id: u32) -> VReg {
    VReg::Virtual(crate::smir::ir::types::VirtualId(id))
}

const PC: u64 = 0x1000;

fn addr() -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 16,
        disp_size: DispSize::Disp8,
    }
}

/// Build the lifted `CMPXCHG` body. `snapshots` selects how many of the two
/// leading MOVs optimization left in place; `write_back` keeps the accumulator
/// CMove.
fn cmpxchg(snapshots: u8, write_back: bool) -> Vec<OpKind> {
    let width = OpWidth::W64;
    let mut ops = Vec::new();
    let source = if snapshots >= 1 {
        ops.push(OpKind::Mov {
            dst: virt(0),
            src: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width,
        });
        virt(0)
    } else {
        x86(X86Reg::Rcx)
    };
    let accumulator = if snapshots >= 2 {
        ops.push(OpKind::Mov {
            dst: virt(1),
            src: SrcOperand::Reg(x86(X86Reg::Rax)),
            width,
        });
        virt(1)
    } else {
        x86(X86Reg::Rax)
    };
    ops.push(OpKind::Load {
        dst: virt(2),
        addr: addr(),
        width: MemWidth::B8,
        sign: SignExtend::Zero,
    });
    ops.push(OpKind::Cmp {
        src1: accumulator,
        src2: SrcOperand::Reg(virt(2)),
        width,
    });
    ops.push(OpKind::SetCC {
        dst: virt(3),
        cond: Condition::Eq,
        width: OpWidth::W8,
    });
    ops.push(OpKind::Select {
        dst: virt(4),
        cond: virt(3),
        src_true: source,
        src_false: virt(2),
        width,
    });
    ops.push(OpKind::PredStore {
        src: SrcOperand::Reg(virt(4)),
        cond: virt(3),
        addr: addr(),
        width: MemWidth::B8,
    });
    if write_back {
        ops.push(OpKind::CMove {
            dst: x86(X86Reg::Rax),
            src: virt(2),
            cond: Condition::Ne,
            width,
        });
    }
    ops
}

fn function(ops: Vec<OpKind>) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for op in ops {
        builder.push_op(PC, op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn counts(
    block: &crate::smir::ir::SmirBlock,
) -> (
    std::collections::HashMap<VReg, usize>,
    std::collections::HashMap<VReg, usize>,
) {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence_len(ops: Vec<OpKind>) -> Option<usize> {
    let function = function(ops);
    let block = function.entry_block().unwrap();
    let (definitions, uses) = counts(block);
    x86_jit_cmpxchg_sequence_len(block, 0, true, &definitions, &uses)
}

fn gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(&function(ops), &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn every_optimization_survivable_cmpxchg_shape_is_admitted() {
    for snapshots in [0u8, 1, 2] {
        for write_back in [false, true] {
            let ops = cmpxchg(snapshots, write_back);
            let expected = ops.len();
            assert_eq!(
                sequence_len(ops.clone()),
                Some(expected),
                "snapshots {snapshots} write-back {write_back}"
            );
            assert!(
                gate(ops.clone(), true),
                "snapshots {snapshots} write-back {write_back} must be admitted"
            );
            assert!(
                !gate(ops, false),
                "snapshots {snapshots} write-back {write_back} must need memory JIT"
            );
        }
    }
}

#[test]
fn unmodeled_cmpxchg_shapes_fail_closed() {
    // A state-backed replacement operand has no identity host register.
    let mut state_backed = cmpxchg(0, true);
    state_backed[3] = OpKind::Select {
        dst: virt(4),
        cond: virt(3),
        src_true: x86(X86Reg::Rbp),
        src_false: virt(2),
        width: OpWidth::W64,
    };
    assert_eq!(sequence_len(state_backed), None);

    // The predicated store must consume the selected value under the same
    // condition and at the same address.
    let mut wrong_condition = cmpxchg(0, true);
    wrong_condition[4] = OpKind::PredStore {
        src: SrcOperand::Reg(virt(4)),
        cond: virt(2),
        addr: addr(),
        width: MemWidth::B8,
    };
    assert_eq!(sequence_len(wrong_condition), None);

    let mut wrong_address = cmpxchg(0, true);
    wrong_address[4] = OpKind::PredStore {
        src: SrcOperand::Reg(virt(4)),
        cond: virt(3),
        addr: Address::Direct(x86(X86Reg::Rbx)),
        width: MemWidth::B8,
    };
    assert_eq!(sequence_len(wrong_address), None);

    // The comparison must test the loaded value.
    let mut wrong_compare = cmpxchg(0, true);
    wrong_compare[1] = OpKind::Cmp {
        src1: x86(X86Reg::Rax),
        src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
        width: OpWidth::W64,
    };
    assert_eq!(sequence_len(wrong_compare), None);

    // An extra consumer of the loaded value is outside the fused shape.
    let mut extra_use = cmpxchg(0, true);
    extra_use.push(OpKind::Mov {
        dst: x86(X86Reg::Rdx),
        src: SrcOperand::Reg(virt(2)),
        width: OpWidth::W64,
    });
    assert_eq!(sequence_len(extra_use), None);

    // Operations belonging to different guest instructions never fuse.
    let ops = cmpxchg(0, true);
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for (index, op) in ops.into_iter().enumerate() {
        builder.push_op(PC + u64::from(index as u32 > 2), op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let split = builder.finish();
    let block = split.entry_block().unwrap();
    let (definitions, uses) = counts(block);
    assert_eq!(
        x86_jit_cmpxchg_sequence_len(block, 0, true, &definitions, &uses),
        None
    );
}

#[test]
fn a_lifted_cmpxchg_region_survives_o2_and_stays_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for op in cmpxchg(2, true) {
        builder.push_op(PC, op);
    }
    builder.push_op(
        PC + 4,
        OpKind::Mov {
            dst: x86(X86Reg::Rdx),
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(
        function
            .blocks
            .iter()
            .flat_map(|block| &block.ops)
            .any(|op| matches!(op.kind, OpKind::PredStore { .. })),
        "O2 must retain the predicated store"
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
