//! Native admission for the legacy Group-2 `/6` SAL alias.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, X86OpHint};
use crate::smir::ir::types::{ArchReg, FunctionId, OpWidth, SrcOperand, VReg, VirtualId, X86Reg};
use crate::smir::ir::{FunctionBuilder, SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding,
};
use crate::smir::lower::x86_64::{X86_64Lowerer, x86_shift_group6_shape_valid};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x6006;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn lifted_function(bytes: &[u8]) -> SmirFunction {
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
    let result = X86_64Lifter::strict()
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("complete Group-2 instruction"),
    );
    function
}

fn hinted_function(kind: OpKind) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, kind);
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::ShiftGroup6);
    function
}

fn assert_lowers(function: &SmirFunction, label: &str) {
    let mut lowerer = X86_64Lowerer::new();
    lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    assert!(!lowerer.finalize().unwrap().is_empty(), "{label}");
}

#[test]
fn register_sal_aliases_admit_and_lower_at_o0_o1_o2() {
    let cases: &[(&str, &[u8])] = &[
        ("sal al,0", &[0xC0, 0xF0, 0x00]),
        ("sal bl,255", &[0xC0, 0xF3, 0xFF]),
        ("sal r8b,31", &[0x41, 0xC0, 0xF0, 0x1F]),
        ("sal r15b,1", &[0x41, 0xD0, 0xF7]),
        ("sal cl,cl", &[0xD2, 0xF1]),
        ("sal ax,32", &[0x66, 0xC1, 0xF0, 0x20]),
        ("sal sp,1", &[0x66, 0xD1, 0xF4]),
        ("sal bp,cl", &[0x66, 0xD3, 0xF5]),
        ("sal eax,32", &[0xC1, 0xF0, 0x20]),
        ("sal esp,1", &[0xD1, 0xF4]),
        ("sal r15d,cl", &[0x41, 0xD3, 0xF7]),
        ("sal rax,64", &[0x48, 0xC1, 0xF0, 0x40]),
        ("sal rsp,1", &[0x48, 0xD1, 0xF4]),
        ("sal rbp,cl", &[0x48, 0xD3, 0xF5]),
        ("sal rcx,cl", &[0x48, 0xD3, 0xF1]),
        ("sal r15,63", &[0x49, 0xC1, 0xF7, 0x3F]),
        ("inert REP sal al,1", &[0xF3, 0xC0, 0xF0, 0x01]),
        ("addr32 sal r15b,1", &[0x67, 0x41, 0xD0, 0xF7]),
    ];

    let mut profiles = 0usize;
    for (name, bytes) in cases {
        let original = lifted_function(bytes);
        for level in LEVELS {
            let mut function = original.clone();
            optimize_function(&mut function, level);
            let [op] = function.blocks[0].ops.as_slice() else {
                panic!(
                    "{name} {level:?}: unexpected ops {:?}",
                    function.blocks[0].ops
                )
            };
            assert_eq!(
                op.x86_hint,
                Some(X86OpHint::ShiftGroup6),
                "{name} {level:?}"
            );
            assert!(x86_shift_group6_shape_valid(op), "{name} {level:?}: {op:?}");
            assert!(
                !op.is_jit_safe(),
                "generic cross-host admission stays closed"
            );
            assert!(is_native_clobber_safe(&function), "{name} {level:?}");
            assert!(
                !is_x86_aarch64_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                ),
                "{name} {level:?}: AArch64 host must retain fallback"
            );
            assert_lowers(&function, &format!("{name} {level:?}"));
            profiles += 1;
        }
    }
    assert_eq!(profiles, cases.len() * LEVELS.len());
}

#[test]
fn memory_sal_aliases_remain_interpreter_only_even_with_memory_jit() {
    for bytes in [
        &[0xC0, 0x30, 0x01][..],
        &[0x66, 0xD1, 0x30][..],
        &[0x48, 0xD3, 0x30][..],
    ] {
        for level in LEVELS {
            let mut function = lifted_function(bytes);
            optimize_function(&mut function, level);
            assert!(!is_native_clobber_safe(&function), "{bytes:02X?} {level:?}");
            assert!(
                !is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "{bytes:02X?} {level:?}: `/6` memory RMW must fail closed"
            );
        }
    }
}

#[test]
fn malformed_group6_hints_fail_both_gate_and_lowerer() {
    let rax = x86(X86Reg::Rax);
    let rbx = x86(X86Reg::Rbx);
    let rcx = x86(X86Reg::Rcx);
    let shl = |dst, src, amount, width, flags| OpKind::Shl {
        dst,
        src,
        amount,
        width,
        flags,
    };

    let cases = [
        (
            "different source",
            shl(
                rax,
                rbx,
                SrcOperand::Reg(rcx),
                OpWidth::W64,
                FlagUpdate::All,
            ),
        ),
        (
            "wrong count register",
            shl(
                rax,
                rax,
                SrcOperand::Reg(rbx),
                OpWidth::W64,
                FlagUpdate::All,
            ),
        ),
        (
            "negative immediate",
            shl(rax, rax, SrcOperand::Imm(-1), OpWidth::W64, FlagUpdate::All),
        ),
        (
            "oversized immediate",
            shl(
                rax,
                rax,
                SrcOperand::Imm(256),
                OpWidth::W64,
                FlagUpdate::All,
            ),
        ),
        (
            "imm64",
            shl(
                rax,
                rax,
                SrcOperand::Imm64(1),
                OpWidth::W64,
                FlagUpdate::All,
            ),
        ),
        (
            "suppressed flags",
            shl(
                rax,
                rax,
                SrcOperand::Reg(rcx),
                OpWidth::W64,
                FlagUpdate::None,
            ),
        ),
        (
            "W128",
            shl(
                rax,
                rax,
                SrcOperand::Reg(rcx),
                OpWidth::W128,
                FlagUpdate::All,
            ),
        ),
        (
            "APX register",
            shl(
                x86(X86Reg::R16),
                x86(X86Reg::R16),
                SrcOperand::Reg(rcx),
                OpWidth::W64,
                FlagUpdate::All,
            ),
        ),
        (
            "virtual register",
            shl(
                VReg::Virtual(VirtualId(0)),
                VReg::Virtual(VirtualId(0)),
                SrcOperand::Reg(rcx),
                OpWidth::W64,
                FlagUpdate::All,
            ),
        ),
        (
            "nonidentity no-op MOV",
            OpKind::Mov {
                dst: rax,
                src: SrcOperand::Reg(rbx),
                width: OpWidth::W64,
            },
        ),
        (
            "immediate MOV",
            OpKind::Mov {
                dst: rax,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ),
        (
            "unrelated operation",
            OpKind::Add {
                dst: rax,
                src1: rax,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
    ];

    for (name, kind) in cases {
        let function = hinted_function(kind);
        let op = &function.blocks[0].ops[0];
        assert!(!x86_shift_group6_shape_valid(op), "{name}: {op:?}");
        assert!(!is_native_clobber_safe(&function), "{name}");
        let mut lowerer = X86_64Lowerer::new();
        assert!(lowerer.lower_function(&function).is_err(), "{name}");
    }
}
