//! Native admission for memory-source compares against a state-backed GPR.

use super::*;
use crate::smir::ir::ops::X86AluEncoding;
use crate::smir::lower::runtime::x86_jit_mem_state_compare_sequence_len;

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

fn load(dst: VReg, width: MemWidth) -> OpKind {
    OpKind::Load {
        dst,
        addr: addr(),
        width,
        sign: SignExtend::Zero,
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
    x86_jit_mem_state_compare_sequence_len(block, 0, true, &definitions, &uses)
}

fn gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(&function(ops), &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn state_backed_compare_operands_are_admitted_in_both_orders_and_widths() {
    for (name, ops) in [
        (
            "cmp dword [rbx+16], ebp",
            vec![
                load(virt(0), MemWidth::B4),
                OpKind::Cmp {
                    src1: virt(0),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W32,
                },
            ],
        ),
        (
            "cmp rsp, qword [rbx+16]",
            vec![
                load(virt(0), MemWidth::B8),
                OpKind::Cmp {
                    src1: x86(X86Reg::Rsp),
                    src2: SrcOperand::Reg(virt(0)),
                    width: OpWidth::W64,
                },
            ],
        ),
        (
            "test byte [rbx+16], bpl",
            vec![
                load(virt(0), MemWidth::B1),
                OpKind::Test {
                    src1: virt(0),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W8,
                },
            ],
        ),
        (
            "test word [rbx+16], sp",
            vec![
                load(virt(0), MemWidth::B2),
                OpKind::Test {
                    src1: x86(X86Reg::Rsp),
                    src2: SrcOperand::Reg(virt(0)),
                    width: OpWidth::W16,
                },
            ],
        ),
        (
            "cmp dword [rbx+16], r16d",
            vec![
                load(virt(0), MemWidth::B4),
                OpKind::Cmp {
                    src1: virt(0),
                    src2: SrcOperand::Reg(x86(X86Reg::R16)),
                    width: OpWidth::W32,
                },
            ],
        ),
    ] {
        assert_eq!(sequence_len(ops.clone()), Some(2), "{name}");
        assert!(gate(ops.clone(), true), "{name} must be admitted");
        assert!(!gate(ops, false), "{name} must need memory JIT");
    }
}

#[test]
fn unmodeled_compare_shapes_fail_closed() {
    for (name, ops) in [
        (
            "identity operand keeps the generic fusion",
            vec![
                load(virt(0), MemWidth::B4),
                OpKind::Cmp {
                    src1: virt(0),
                    src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W32,
                },
            ],
        ),
        (
            "immediate operand keeps the generic fusion",
            vec![
                load(virt(0), MemWidth::B4),
                OpKind::Cmp {
                    src1: virt(0),
                    src2: SrcOperand::Imm(7),
                    width: OpWidth::W32,
                },
            ],
        ),
        (
            "operand width must match the access width",
            vec![
                load(virt(0), MemWidth::B4),
                OpKind::Cmp {
                    src1: virt(0),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W64,
                },
            ],
        ),
        (
            "sign-extending loads are outside the shape",
            vec![
                OpKind::Load {
                    dst: virt(0),
                    addr: addr(),
                    width: MemWidth::B4,
                    sign: SignExtend::Sign,
                },
                OpKind::Cmp {
                    src1: virt(0),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W32,
                },
            ],
        ),
        (
            "the loaded value must have exactly one consumer",
            vec![
                load(virt(0), MemWidth::B4),
                OpKind::Cmp {
                    src1: virt(0),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W32,
                },
                OpKind::Mov {
                    dst: x86(X86Reg::Rcx),
                    src: SrcOperand::Reg(virt(0)),
                    width: OpWidth::W32,
                },
            ],
        ),
    ] {
        assert_eq!(sequence_len(ops), None, "{name}");
    }

    // A hinted compare leaves the modeled shape.
    let mut hinted = function(vec![
        load(virt(0), MemWidth::B4),
        OpKind::Cmp {
            src1: virt(0),
            src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W32,
        },
    ]);
    hinted.blocks[0].ops[1].x86_hint = Some(X86OpHint::AluEncoding(X86AluEncoding::RegRm));
    let block = hinted.entry_block().unwrap();
    let (definitions, uses) = counts(block);
    assert_eq!(
        x86_jit_mem_state_compare_sequence_len(block, 0, true, &definitions, &uses),
        None
    );
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));
}

#[test]
fn a_frame_bounds_check_region_survives_o2_and_stays_admitted() {
    // cmp dword [rbx+16], ebp ; setb cl
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, load(virt(0), MemWidth::B4));
    builder.push_op(
        PC,
        OpKind::Cmp {
            src1: virt(0),
            src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        PC + 3,
        OpKind::SetCC {
            dst: x86(X86Reg::Rcx),
            cond: Condition::Ult,
            width: OpWidth::W8,
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
