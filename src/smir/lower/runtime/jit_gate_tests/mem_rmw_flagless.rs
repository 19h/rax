//! Native admission for memory read-modify-write forms whose architectural
//! flag update was proven dead.
//!
//! `lock`-free `add [mem],r`, `or [mem],imm`, `inc [mem]` and friends lift as
//! `Load; compute(flags=None); Store; replay(flags=All)`. Once optimization
//! proves the replay's flags dead it deletes that operation, and the surviving
//! three-operation form must still fuse instead of rejecting the hot region.

use super::*;
use crate::smir::lower::runtime::{
    x86_jit_mem_alu_rmw_sequence_len, x86_jit_mem_unary_rmw_sequence_len,
};

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
        offset: 8,
        disp_size: DispSize::Disp8,
    }
}

fn load(dst: VReg) -> OpKind {
    OpKind::Load {
        dst,
        addr: addr(),
        width: MemWidth::B4,
        sign: SignExtend::Zero,
    }
}

fn store(src: VReg) -> OpKind {
    OpKind::Store {
        src,
        addr: addr(),
        width: MemWidth::B4,
    }
}

fn or_op(dst: VReg, src1: VReg, flags: FlagUpdate) -> OpKind {
    OpKind::Or {
        dst,
        src1,
        src2: SrcOperand::Imm(2),
        width: OpWidth::W32,
        flags,
    }
}

fn dec_op(dst: VReg, src: VReg, flags: FlagUpdate) -> OpKind {
    OpKind::Dec {
        dst,
        src,
        width: OpWidth::W32,
        flags,
    }
}

fn function(ops: Vec<OpKind>) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for op in ops {
        builder.push_op(PC, op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn virtual_counts(
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

fn alu_len(ops: Vec<OpKind>) -> Option<usize> {
    let function = function(ops);
    let block = function.entry_block().unwrap();
    let (definitions, uses) = virtual_counts(block);
    x86_jit_mem_alu_rmw_sequence_len(block, 0, true, &definitions, &uses)
}

fn unary_len(ops: Vec<OpKind>) -> Option<usize> {
    let function = function(ops);
    let block = function.entry_block().unwrap();
    let (definitions, uses) = virtual_counts(block);
    x86_jit_mem_unary_rmw_sequence_len(block, 0, true, &definitions, &uses)
}

fn gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(&function(ops), &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn both_flag_publishing_and_flag_dead_rmw_shapes_are_recognized() {
    assert_eq!(
        alu_len(vec![
            load(virt(0)),
            or_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
            or_op(virt(2), virt(0), FlagUpdate::All),
        ]),
        Some(4)
    );
    assert_eq!(
        alu_len(vec![
            load(virt(0)),
            or_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
        ]),
        Some(3)
    );
    assert_eq!(
        unary_len(vec![
            load(virt(0)),
            dec_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
            dec_op(virt(2), virt(0), FlagUpdate::All),
        ]),
        Some(4)
    );
    assert_eq!(
        unary_len(vec![
            load(virt(0)),
            dec_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
        ]),
        Some(3)
    );
}

#[test]
fn unmodeled_rmw_shapes_still_fail_closed() {
    // A compute that publishes flags has no fused lowering: the fusion computes
    // speculatively before the store can fault.
    assert_eq!(
        alu_len(vec![
            load(virt(0)),
            or_op(virt(1), virt(0), FlagUpdate::All),
            store(virt(1)),
        ]),
        None
    );
    // An extra live use of the loaded value is not part of the fused shape.
    assert_eq!(
        alu_len(vec![
            load(virt(0)),
            or_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
            OpKind::Mov {
                dst: x86(X86Reg::Rax),
                src: SrcOperand::Reg(virt(0)),
                width: OpWidth::W32,
            },
        ]),
        None
    );
    // The store must consume the computed value, not the original one.
    assert_eq!(
        alu_len(vec![
            load(virt(0)),
            or_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(0)),
        ]),
        None
    );
    // A mismatched replay operation is neither the four- nor three-op form.
    assert_eq!(
        alu_len(vec![
            load(virt(0)),
            or_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
            OpKind::And {
                dst: virt(2),
                src1: virt(0),
                src2: SrcOperand::Imm(2),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
        ]),
        None
    );
}

#[test]
fn flag_dead_rmw_regions_are_admitted_only_under_memory_jit() {
    for ops in [
        vec![
            load(virt(0)),
            or_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
        ],
        vec![
            load(virt(0)),
            dec_op(virt(1), virt(0), FlagUpdate::None),
            store(virt(1)),
        ],
    ] {
        assert!(gate(ops.clone(), true));
        assert!(!gate(ops, false));
    }
}

#[test]
fn optimization_reduces_a_lifted_rmw_to_the_admitted_flag_dead_shape() {
    // The full lifted shape plus a later flag redefinition: O2 deletes the dead
    // replay, and the surviving three-operation form must stay admitted.
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, load(virt(0)));
    builder.push_op(PC, or_op(virt(1), virt(0), FlagUpdate::None));
    builder.push_op(PC, store(virt(1)));
    builder.push_op(PC, or_op(virt(2), virt(0), FlagUpdate::All));
    builder.push_op(
        PC + 4,
        OpKind::Test {
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::Rax)),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
