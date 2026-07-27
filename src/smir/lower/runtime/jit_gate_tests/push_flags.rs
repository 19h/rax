//! Native admission for `PUSHF`/`PUSHFQ`.

use super::*;
use crate::smir::lower::runtime::x86_jit_push_flags_sequence_len;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn virt(id: u32) -> VReg {
    VReg::Virtual(crate::smir::ir::types::VirtualId(id))
}

const PC: u64 = 0x1000;

fn sub_rsp(delta: i64, flags: FlagUpdate) -> OpKind {
    OpKind::Sub {
        dst: x86(X86Reg::Rsp),
        src1: x86(X86Reg::Rsp),
        src2: SrcOperand::Imm(delta),
        width: OpWidth::W64,
        flags,
    }
}

fn store_stack(src: VReg, width: MemWidth) -> OpKind {
    OpKind::Store {
        src,
        addr: Address::Direct(x86(X86Reg::Rsp)),
        width,
    }
}

fn push_flags(delta: i64, width: MemWidth) -> Vec<OpKind> {
    vec![
        OpKind::ReadFlags { dst: virt(0) },
        sub_rsp(delta, FlagUpdate::None),
        store_stack(virt(0), width),
    ]
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
    x86_jit_push_flags_sequence_len(block, 0, true, &definitions, &uses)
}

fn gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(&function(ops), &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn flag_pushes_are_admitted_only_under_memory_jit() {
    for (name, delta, width) in [("pushfq", 8, MemWidth::B8), ("pushfw", 2, MemWidth::B2)] {
        let ops = push_flags(delta, width);
        assert_eq!(sequence_len(ops.clone()), Some(3), "{name}");
        assert!(gate(ops.clone(), true), "{name} must be admitted");
        assert!(!gate(ops, false), "{name} must need memory JIT");
    }
}

#[test]
fn unmodeled_flag_push_shapes_fail_closed() {
    for (name, ops) in [
        (
            "the stored width must match the decrement",
            vec![
                OpKind::ReadFlags { dst: virt(0) },
                sub_rsp(8, FlagUpdate::None),
                store_stack(virt(0), MemWidth::B2),
            ],
        ),
        (
            "the decrement must be an architectural push size",
            vec![
                OpKind::ReadFlags { dst: virt(0) },
                sub_rsp(4, FlagUpdate::None),
                store_stack(virt(0), MemWidth::B8),
            ],
        ),
        (
            "the decrement must publish no flags",
            vec![
                OpKind::ReadFlags { dst: virt(0) },
                sub_rsp(8, FlagUpdate::All),
                store_stack(virt(0), MemWidth::B8),
            ],
        ),
        (
            "the stack write must target the new stack top",
            vec![
                OpKind::ReadFlags { dst: virt(0) },
                sub_rsp(8, FlagUpdate::None),
                OpKind::Store {
                    src: virt(0),
                    addr: Address::Direct(x86(X86Reg::Rbx)),
                    width: MemWidth::B8,
                },
            ],
        ),
        (
            "an architectural destination is not this shape",
            vec![
                OpKind::ReadFlags {
                    dst: x86(X86Reg::Rax),
                },
                sub_rsp(8, FlagUpdate::None),
                store_stack(x86(X86Reg::Rax), MemWidth::B8),
            ],
        ),
        (
            "the image must have exactly one consumer",
            vec![
                OpKind::ReadFlags { dst: virt(0) },
                sub_rsp(8, FlagUpdate::None),
                store_stack(virt(0), MemWidth::B8),
                OpKind::Mov {
                    dst: x86(X86Reg::Rcx),
                    src: SrcOperand::Reg(virt(0)),
                    width: OpWidth::W64,
                },
            ],
        ),
    ] {
        assert_eq!(sequence_len(ops.clone()), None, "{name}");
        assert!(!gate(ops, true), "{name} must be rejected");
    }

    // A hinted ReadFlags leaves the modeled shape.
    let mut hinted = function(push_flags(8, MemWidth::B8));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let block = hinted.entry_block().unwrap();
    let (definitions, uses) = counts(block);
    assert_eq!(
        x86_jit_push_flags_sequence_len(block, 0, true, &definitions, &uses),
        None
    );
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));

    // Operations belonging to different guest instructions never fuse.
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, OpKind::ReadFlags { dst: virt(0) });
    builder.push_op(PC + 1, sub_rsp(8, FlagUpdate::None));
    builder.push_op(PC + 1, store_stack(virt(0), MemWidth::B8));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let split = builder.finish();
    let block = split.entry_block().unwrap();
    let (definitions, uses) = counts(block);
    assert_eq!(
        x86_jit_push_flags_sequence_len(block, 0, true, &definitions, &uses),
        None
    );
}

#[test]
fn a_flag_push_region_survives_o2_and_stays_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, OpKind::ReadFlags { dst: virt(0) });
    builder.push_op(PC, sub_rsp(8, FlagUpdate::None));
    builder.push_op(PC, store_stack(virt(0), MemWidth::B8));
    builder.push_op(
        PC + 1,
        OpKind::Xor {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::Rax)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
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
            .any(|op| matches!(op.kind, OpKind::ReadFlags { .. })),
        "O2 must retain the flag read"
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
