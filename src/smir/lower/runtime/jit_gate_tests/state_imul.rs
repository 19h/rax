//! State-backed register IMUL admission and fail-closed coverage.

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, OpId, OpWidth, ShiftOp, SrcOperand, VReg, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator};
use crate::smir::lower::runtime::is_native_clobber_safe;
use crate::smir::lower::x86_64::{x86_state_imul_candidate, x86_state_imul_valid};

const PC: u64 = 0x494D_554C;

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn state_backed(index: u8) -> bool {
    matches!(index, 4 | 5 | 16..=31)
}

fn function(op: SmirOp) -> SmirFunction {
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops.push(op);
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

#[test]
fn state_imul_validator_exhausts_architectural_register_products() {
    let mut truncated = 0usize;
    for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        for flags in [FlagUpdate::None, FlagUpdate::All] {
            for dst in 0u8..32 {
                for src1 in 0u8..32 {
                    for src2 in 0u8..32 {
                        let op = SmirOp::new(
                            OpId(0),
                            PC,
                            OpKind::MulS {
                                dst_lo: gpr(dst),
                                dst_hi: None,
                                src1: gpr(src1),
                                src2: SrcOperand::Reg(gpr(src2)),
                                width,
                                flags,
                            },
                        );
                        let expected =
                            state_backed(dst) || state_backed(src1) || state_backed(src2);
                        assert_eq!(x86_state_imul_candidate(&op), expected);
                        assert_eq!(x86_state_imul_valid(&op), expected);
                        truncated += usize::from(expected);
                    }
                }
            }
        }
    }
    assert_eq!(truncated, 180_144);

    let mut implicit = 0usize;
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        for flags in [FlagUpdate::None, FlagUpdate::All] {
            for source in 0u8..32 {
                let op = SmirOp::new(
                    OpId(0),
                    PC,
                    OpKind::MulS {
                        dst_lo: gpr(0),
                        dst_hi: (width != OpWidth::W8).then_some(gpr(2)),
                        src1: gpr(0),
                        src2: SrcOperand::Reg(gpr(source)),
                        width,
                        flags,
                    },
                );
                let expected = state_backed(source);
                assert_eq!(x86_state_imul_candidate(&op), expected);
                assert_eq!(x86_state_imul_valid(&op), expected);
                implicit += usize::from(expected);
            }
        }
    }
    assert_eq!(implicit, 144);
}

#[test]
fn state_imul_immediate_validator_exhausts_destination_source_aliases() {
    let mut admitted = 0usize;
    for (width, hint, immediate) in [
        (OpWidth::W16, X86OpHint::ImulImm8, -128),
        (OpWidth::W16, X86OpHint::ImulImm32, i16::MAX as i64),
        (OpWidth::W32, X86OpHint::ImulImm8, i8::MAX as i64),
        (OpWidth::W32, X86OpHint::ImulImm32, i32::MIN as i64),
        (OpWidth::W64, X86OpHint::ImulImm8, -1),
        (OpWidth::W64, X86OpHint::ImulImm32, i32::MAX as i64),
    ] {
        for flags in [FlagUpdate::None, FlagUpdate::All] {
            for dst in 0u8..32 {
                for src1 in 0u8..32 {
                    let mut op = SmirOp::new(
                        OpId(0),
                        PC,
                        OpKind::MulS {
                            dst_lo: gpr(dst),
                            dst_hi: None,
                            src1: gpr(src1),
                            src2: SrcOperand::Imm(immediate),
                            width,
                            flags,
                        },
                    );
                    op.x86_hint = Some(hint);
                    let expected = state_backed(dst) || state_backed(src1);
                    assert_eq!(x86_state_imul_candidate(&op), expected);
                    assert_eq!(x86_state_imul_valid(&op), expected);
                    admitted += usize::from(expected);
                }
            }
        }
    }
    assert_eq!(admitted, 9_936);
}

#[test]
fn state_imul_gate_admits_valid_and_rejects_malformed_candidates() {
    let valid = [
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::MulS {
                dst_lo: gpr(4),
                dst_hi: None,
                src1: gpr(5),
                src2: SrcOperand::Reg(gpr(16)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::MulS {
                dst_lo: gpr(0),
                dst_hi: None,
                src1: gpr(0),
                src2: SrcOperand::Reg(gpr(4)),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::MulS {
                dst_lo: gpr(0),
                dst_hi: Some(gpr(2)),
                src1: gpr(0),
                src2: SrcOperand::Reg(gpr(5)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
    ];
    for op in valid {
        assert!(x86_state_imul_valid(&op));
        let function = function(op);
        assert!(is_native_clobber_safe(&function));
    }

    let virtual_reg = VReg::Virtual(crate::smir::ir::types::VirtualId(0));
    let malformed = [
        (
            "truncated W8",
            SmirOp::new(
                OpId(0),
                PC,
                OpKind::MulS {
                    dst_lo: gpr(4),
                    dst_hi: None,
                    src1: gpr(5),
                    src2: SrcOperand::Reg(gpr(6)),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                },
            ),
        ),
        (
            "wrong implicit high destination",
            SmirOp::new(
                OpId(0),
                PC,
                OpKind::MulS {
                    dst_lo: gpr(0),
                    dst_hi: Some(gpr(4)),
                    src1: gpr(0),
                    src2: SrcOperand::Reg(gpr(5)),
                    width: OpWidth::W16,
                    flags: FlagUpdate::All,
                },
            ),
        ),
        (
            "virtual source",
            SmirOp::new(
                OpId(0),
                PC,
                OpKind::MulS {
                    dst_lo: gpr(4),
                    dst_hi: None,
                    src1: gpr(5),
                    src2: SrcOperand::Reg(virtual_reg),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
        ),
        (
            "shifted source",
            SmirOp::new(
                OpId(0),
                PC,
                OpKind::MulS {
                    dst_lo: gpr(0),
                    dst_hi: None,
                    src1: gpr(1),
                    src2: SrcOperand::Shifted {
                        reg: gpr(4),
                        shift: ShiftOp::Lsl,
                        amount: 1,
                    },
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
        ),
        (
            "partial flags",
            SmirOp::new(
                OpId(0),
                PC,
                OpKind::MulS {
                    dst_lo: gpr(4),
                    dst_hi: None,
                    src1: gpr(5),
                    src2: SrcOperand::Reg(gpr(6)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::Specific(FlagSet::CF),
                },
            ),
        ),
    ];
    for (name, op) in malformed {
        assert!(x86_state_imul_candidate(&op), "{name}");
        assert!(!x86_state_imul_valid(&op), "{name}");
        assert!(!is_native_clobber_safe(&function(op)), "{name}");
    }

    for (name, width, hint, immediate) in [
        ("missing immediate hint", OpWidth::W64, None, 7),
        (
            "imm8 overflow",
            OpWidth::W64,
            Some(X86OpHint::ImulImm8),
            128,
        ),
        (
            "imm16 overflow",
            OpWidth::W16,
            Some(X86OpHint::ImulImm32),
            0x8000,
        ),
        (
            "imm32 overflow",
            OpWidth::W64,
            Some(X86OpHint::ImulImm32),
            i64::from(i32::MAX) + 1,
        ),
    ] {
        let mut op = SmirOp::new(
            OpId(0),
            PC,
            OpKind::MulS {
                dst_lo: gpr(4),
                dst_hi: None,
                src1: gpr(5),
                src2: SrcOperand::Imm(immediate),
                width,
                flags: FlagUpdate::All,
            },
        );
        op.x86_hint = hint;
        assert!(x86_state_imul_candidate(&op), "{name}");
        assert!(!x86_state_imul_valid(&op), "{name}");
        assert!(!is_native_clobber_safe(&function(op)), "{name}");
    }
}
