//! Native admission for memory-operand pushes.

use super::*;
use crate::smir::lower::runtime::x86_jit_push_memory_sequence_len;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn virt(id: u32) -> VReg {
    VReg::Virtual(crate::smir::ir::types::VirtualId(id))
}

const PC: u64 = 0x1000;

fn source_addr() -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 24,
        disp_size: DispSize::Disp8,
    }
}

fn load(dst: VReg, width: MemWidth, sign: SignExtend) -> OpKind {
    OpKind::Load {
        dst,
        addr: source_addr(),
        width,
        sign,
    }
}

fn sub_rsp(delta: i64) -> OpKind {
    OpKind::Sub {
        dst: x86(X86Reg::Rsp),
        src1: x86(X86Reg::Rsp),
        src2: SrcOperand::Imm(delta),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    }
}

fn store_stack(src: VReg, width: MemWidth) -> OpKind {
    OpKind::Store {
        src,
        addr: Address::Direct(x86(X86Reg::Rsp)),
        width,
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
    x86_jit_push_memory_sequence_len(block, 0, true, &definitions, &uses)
}

fn gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(&function(ops), &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn memory_pushes_are_admitted_for_every_lifted_width() {
    for (name, source, delta, push) in [
        ("push qword [rbx+24]", MemWidth::B8, 8, MemWidth::B8),
        ("zero-extended dword source", MemWidth::B4, 8, MemWidth::B8),
        ("zero-extended byte source", MemWidth::B1, 8, MemWidth::B8),
        ("push word [rbx+24]", MemWidth::B2, 2, MemWidth::B2),
    ] {
        let ops = vec![
            load(virt(0), source, SignExtend::Zero),
            sub_rsp(delta),
            store_stack(virt(0), push),
        ];
        assert_eq!(sequence_len(ops.clone()), Some(3), "{name}");
        assert!(gate(ops.clone(), true), "{name} must be admitted");
        assert!(!gate(ops, false), "{name} must need memory JIT");
    }
}

#[test]
fn unmodeled_memory_push_shapes_fail_closed() {
    for (name, ops) in [
        (
            "a source wider than the stack slot would drop bits",
            vec![
                load(virt(0), MemWidth::B8, SignExtend::Zero),
                sub_rsp(2),
                store_stack(virt(0), MemWidth::B2),
            ],
        ),
        (
            "sign-extending source reads are outside the shape",
            vec![
                load(virt(0), MemWidth::B4, SignExtend::Sign),
                sub_rsp(8),
                store_stack(virt(0), MemWidth::B8),
            ],
        ),
        (
            "the decrement must match the stored width",
            vec![
                load(virt(0), MemWidth::B8, SignExtend::Zero),
                sub_rsp(2),
                store_stack(virt(0), MemWidth::B8),
            ],
        ),
        (
            "the decrement must be an architectural push size",
            vec![
                load(virt(0), MemWidth::B8, SignExtend::Zero),
                sub_rsp(16),
                store_stack(virt(0), MemWidth::B8),
            ],
        ),
        (
            "the stack write must target the new stack top",
            vec![
                load(virt(0), MemWidth::B8, SignExtend::Zero),
                sub_rsp(8),
                OpKind::Store {
                    src: virt(0),
                    addr: Address::Direct(x86(X86Reg::Rbx)),
                    width: MemWidth::B8,
                },
            ],
        ),
        (
            "the decrement must publish no flags",
            vec![
                load(virt(0), MemWidth::B8, SignExtend::Zero),
                OpKind::Sub {
                    dst: x86(X86Reg::Rsp),
                    src1: x86(X86Reg::Rsp),
                    src2: SrcOperand::Imm(8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
                store_stack(virt(0), MemWidth::B8),
            ],
        ),
        (
            "the staged value must have exactly one consumer",
            vec![
                load(virt(0), MemWidth::B8, SignExtend::Zero),
                sub_rsp(8),
                store_stack(virt(0), MemWidth::B8),
                OpKind::Mov {
                    dst: x86(X86Reg::Rcx),
                    src: SrcOperand::Reg(virt(0)),
                    width: OpWidth::W64,
                },
            ],
        ),
    ] {
        assert_eq!(sequence_len(ops), None, "{name}");
    }

    // Operations belonging to different guest instructions never fuse.
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, load(virt(0), MemWidth::B8, SignExtend::Zero));
    builder.push_op(PC + 4, sub_rsp(8));
    builder.push_op(PC + 4, store_stack(virt(0), MemWidth::B8));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let split = builder.finish();
    let block = split.entry_block().unwrap();
    let (definitions, uses) = counts(block);
    assert_eq!(
        x86_jit_push_memory_sequence_len(block, 0, true, &definitions, &uses),
        None
    );
}

#[test]
fn a_memory_push_region_survives_o2_and_stays_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, load(virt(0), MemWidth::B8, SignExtend::Zero));
    builder.push_op(PC, sub_rsp(8));
    builder.push_op(PC, store_stack(virt(0), MemWidth::B8));
    builder.push_op(
        PC + 3,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Imm(1),
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
    assert!(!is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        false,
    ));
}
