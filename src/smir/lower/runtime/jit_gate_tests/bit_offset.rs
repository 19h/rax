//! Native admission for `BT` with a register bit offset into memory.

use super::*;
use crate::smir::lower::runtime::x86_jit_mem_bit_offset_test_sequence_len;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn virt(id: u32) -> VReg {
    VReg::Virtual(crate::smir::ir::types::VirtualId(id))
}

const PC: u64 = 0x1000;

fn base_addr() -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::Rbx),
        offset: 512,
        disp_size: DispSize::Disp32,
    }
}

fn bit_test(width: OpWidth, mem_width: MemWidth, index: X86Reg) -> Vec<OpKind> {
    let (right, left, bits) = match width {
        OpWidth::W16 => (4i64, 1i64, 16i64),
        OpWidth::W32 => (5, 2, 32),
        OpWidth::W64 => (6, 3, 64),
        _ => unreachable!(),
    };
    vec![
        OpKind::SignExtend {
            dst: virt(0),
            src: x86(index),
            from_width: width,
            to_width: OpWidth::W64,
        },
        OpKind::Sar {
            dst: virt(1),
            src: virt(0),
            amount: SrcOperand::Imm(right),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Shl {
            dst: virt(2),
            src: virt(1),
            amount: SrcOperand::Imm(left),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Lea {
            dst: virt(3),
            addr: base_addr(),
        },
        OpKind::Add {
            dst: virt(4),
            src1: virt(3),
            src2: SrcOperand::Reg(virt(2)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::And {
            dst: virt(5),
            src1: x86(index),
            src2: SrcOperand::Imm(bits - 1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Load {
            dst: virt(6),
            addr: Address::Direct(virt(4)),
            width: mem_width,
            sign: SignExtend::Zero,
        },
        OpKind::Bt {
            src: virt(6),
            index: SrcOperand::Reg(virt(5)),
            width,
        },
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
    x86_jit_mem_bit_offset_test_sequence_len(block, 0, true, &definitions, &uses)
}

fn gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(&function(ops), &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn memory_bit_tests_are_admitted_for_every_operand_width() {
    for (name, width, mem_width) in [
        ("bt qword [mem],r64", OpWidth::W64, MemWidth::B8),
        ("bt dword [mem],r32", OpWidth::W32, MemWidth::B4),
        ("bt word [mem],r16", OpWidth::W16, MemWidth::B2),
    ] {
        let ops = bit_test(width, mem_width, X86Reg::Rcx);
        assert_eq!(sequence_len(ops.clone()), Some(8), "{name}");
        assert!(gate(ops.clone(), true), "{name} must be admitted");
        assert!(!gate(ops, false), "{name} must need memory JIT");
    }
}

#[test]
fn unmodeled_bit_offset_shapes_fail_closed() {
    // The element and byte scales must agree with the operand width; otherwise
    // the fused address would address the wrong element.
    let mut wrong_element_shift = bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx);
    wrong_element_shift[1] = OpKind::Sar {
        dst: virt(1),
        src: virt(0),
        amount: SrcOperand::Imm(5),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    };
    assert_eq!(sequence_len(wrong_element_shift), None);

    let mut wrong_byte_shift = bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx);
    wrong_byte_shift[2] = OpKind::Shl {
        dst: virt(2),
        src: virt(1),
        amount: SrcOperand::Imm(2),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    };
    assert_eq!(sequence_len(wrong_byte_shift), None);

    // The bit index must be masked to the operand width.
    let mut wrong_mask = bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx);
    wrong_mask[5] = OpKind::And {
        dst: virt(5),
        src1: x86(X86Reg::Rcx),
        src2: SrcOperand::Imm(31),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    };
    assert_eq!(sequence_len(wrong_mask), None);

    // The masked index must come from the same register as the scaled offset.
    let mut mismatched_register = bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx);
    mismatched_register[5] = OpKind::And {
        dst: virt(5),
        src1: x86(X86Reg::Rdx),
        src2: SrcOperand::Imm(63),
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    };
    assert_eq!(sequence_len(mismatched_register), None);

    // A state-backed offset register has no identity host register.
    assert_eq!(
        sequence_len(bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rbp)),
        None
    );

    // The base address must be one the helper can rebuild from GuestRegs.
    let mut virtual_base = bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx);
    virtual_base[3] = OpKind::Lea {
        dst: virt(3),
        addr: Address::Direct(virt(7)),
    };
    assert_eq!(sequence_len(virtual_base), None);

    // An extra consumer of any temporary is outside the fused shape.
    let mut extra_use = bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx);
    extra_use.push(OpKind::Mov {
        dst: x86(X86Reg::Rdx),
        src: SrcOperand::Reg(virt(6)),
        width: OpWidth::W64,
    });
    assert_eq!(sequence_len(extra_use), None);

    // Operations belonging to different guest instructions never fuse.
    let ops = bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx);
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for (index, op) in ops.into_iter().enumerate() {
        builder.push_op(PC + u64::from(index as u32 > 3), op);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let split = builder.finish();
    let block = split.entry_block().unwrap();
    let (definitions, uses) = counts(block);
    assert_eq!(
        x86_jit_mem_bit_offset_test_sequence_len(block, 0, true, &definitions, &uses),
        None
    );
}

#[test]
fn a_bitmap_probe_region_survives_o2_and_stays_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    for op in bit_test(OpWidth::W64, MemWidth::B8, X86Reg::Rcx) {
        builder.push_op(PC, op);
    }
    builder.push_op(
        PC + 4,
        OpKind::SetCC {
            dst: x86(X86Reg::Rax),
            cond: Condition::Ult,
            width: OpWidth::W8,
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
            .any(|op| matches!(op.kind, OpKind::Bt { .. })),
        "O2 must retain the bit test"
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
