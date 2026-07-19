//! tests::strength tests

use super::*;
use crate::smir::optimize::*;

#[test]
fn x86_implicit_division_metadata_tracks_the_complete_dividend() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));

    for signed in [false, true] {
        let divide = if signed {
            OpKind::DivS {
                quot: rax,
                rem: Some(rdx),
                src1: rax,
                src2: SrcOperand::Reg(rcx),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            }
        } else {
            OpKind::DivU {
                quot: rax,
                rem: Some(rdx),
                src1: rax,
                src2: SrcOperand::Reg(rcx),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            }
        };
        assert_eq!(divide.source_vregs(), vec![rax, rdx, rcx]);

        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: rdx,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        block.push_op(make_op(1, divide));
        block.set_terminator(Terminator::Return { values: vec![rax] });
        assert_eq!(
            dead_code_elimination(&mut block),
            0,
            "implicit RDX definition must remain live before {signed:?} division"
        );
    }

    let byte_divide = OpKind::DivU {
        quot: rax,
        rem: None,
        src1: rax,
        src2: SrcOperand::Reg(rcx),
        width: OpWidth::W8,
        flags: FlagUpdate::All,
    };
    assert_eq!(byte_divide.dests(), vec![rax]);
    assert_eq!(byte_divide.source_vregs(), vec![rax, rcx]);
}
#[test]
fn test_strength_reduction_mul() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);

    let v0 = VReg::virt(0);
    let v1 = VReg::virt(1);

    // mul v0, v1, 8 -> shl v0, v1, 3
    block.push_op(make_op(
        0,
        OpKind::MulU {
            dst_lo: v0,
            dst_hi: None,
            src1: v1,
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ));

    block.set_terminator(Terminator::Return { values: vec![v0] });

    let reductions = strength_reduction(&mut block);

    assert_eq!(reductions, 1);

    if let OpKind::Shl { amount, .. } = &block.ops[0].kind {
        assert!(matches!(amount, SrcOperand::Imm(3)));
    } else {
        panic!("Expected Shl operation");
    }
}
#[test]
fn test_strength_reduction_div() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);

    let v0 = VReg::virt(0);
    let v1 = VReg::virt(1);

    // div v0, v1, 16 -> shr v0, v1, 4
    block.push_op(make_op(
        0,
        OpKind::DivU {
            quot: v0,
            rem: None,
            src1: v1,
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ));

    block.set_terminator(Terminator::Return { values: vec![v0] });

    let reductions = strength_reduction(&mut block);

    assert_eq!(reductions, 1);

    if let OpKind::Shr { amount, .. } = &block.ops[0].kind {
        assert!(matches!(amount, SrcOperand::Imm(4)));
    } else {
        panic!("Expected Shr operation");
    }
}
