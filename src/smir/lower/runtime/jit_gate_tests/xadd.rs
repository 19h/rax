//! Exhaustive register-form XADD admission and fail-closed shape coverage.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{SmirOp, X86GprOperand, X86OpHint, X86XaddOp};
use crate::smir::ir::types::{BlockId, FunctionId, OpId, OpWidth, SourceArch, X86Reg};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding,
};
use crate::smir::lower::x86_64::{X86_64Lowerer, x86_xadd_shape_valid};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x5841_4400;
const REGISTER_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn kind(dst: X86GprOperand, src: X86GprOperand, width: OpWidth) -> OpKind {
    OpKind::X86Xadd(X86XaddOp {
        dst,
        src,
        width,
        flags: FlagUpdate::All,
    })
}

fn function(op: SmirOp, source: Option<&[u8]>) -> SmirFunction {
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops.push(op);
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    if let Some(source) = source {
        function.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(source).expect("complete XADD source"),
        );
    }
    function
}

#[test]
fn xadd_shape_is_target_specific_and_fails_closed() {
    for valid in [
        kind(
            X86GprOperand::low(X86Reg::Rax),
            X86GprOperand::low(X86Reg::R15),
            OpWidth::W64,
        ),
        kind(
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::high(X86Reg::Rbx),
            OpWidth::W8,
        ),
        kind(
            X86GprOperand::low(X86Reg::Rsp),
            X86GprOperand::low(X86Reg::Rbp),
            OpWidth::W16,
        ),
        kind(
            X86GprOperand::low(X86Reg::R16),
            X86GprOperand::low(X86Reg::R31),
            OpWidth::W32,
        ),
    ] {
        let op = SmirOp::new(OpId(0), PC, valid);
        assert!(!op.is_jit_safe(), "XADD is x86-target-specific");
        assert!(x86_xadd_shape_valid(&op));
        let function = function(op, None);
        assert!(is_native_clobber_safe(&function));
        assert!(!is_x86_aarch64_native_clobber_safe_excluding(
            &function,
            &HashMap::new(),
        ));
    }

    let malformed = [
        kind(
            X86GprOperand::low(X86Reg::Xmm(0)),
            X86GprOperand::low(X86Reg::Rax),
            OpWidth::W64,
        ),
        kind(
            X86GprOperand::high(X86Reg::Rsi),
            X86GprOperand::low(X86Reg::Rax),
            OpWidth::W8,
        ),
        kind(
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::low(X86Reg::R8),
            OpWidth::W8,
        ),
        kind(
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::high(X86Reg::Rbx),
            OpWidth::W16,
        ),
        OpKind::X86Xadd(X86XaddOp {
            dst: X86GprOperand::low(X86Reg::Rax),
            src: X86GprOperand::low(X86Reg::Rbx),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        }),
    ];
    for kind in malformed {
        let op = SmirOp::new(OpId(0), PC, kind);
        assert!(!x86_xadd_shape_valid(&op));
        assert!(!is_native_clobber_safe(&function(op, None)));
    }

    let mut hinted = SmirOp::new(
        OpId(0),
        PC,
        kind(
            X86GprOperand::low(X86Reg::Rax),
            X86GprOperand::low(X86Reg::Rbx),
            OpWidth::W64,
        ),
    );
    hinted.x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_xadd_shape_valid(&hinted));
    assert!(!is_native_clobber_safe(&function(hinted, None)));
}

#[test]
fn every_scanner_register_xadd_cell_survives_o2_and_lowers() {
    let mut seen = HashSet::<Vec<u8>>::new();
    for prefix in REGISTER_PREFIXES {
        for opcode in [0xC0, 0xC1] {
            for modrm in 0xC0..=0xFF {
                let mut bytes = prefix.to_vec();
                bytes.extend_from_slice(&[0x0F, opcode, modrm]);
                assert!(seen.insert(bytes.clone()), "duplicate source {bytes:02X?}");

                let result = X86_64Lifter::strict()
                    .lift_insn(PC, &bytes, &mut LiftContext::new(SourceArch::X86_64))
                    .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                assert!(
                    matches!(
                        result.control_flow,
                        ControlFlow::Fallthrough | ControlFlow::NextInsn
                    ),
                    "{bytes:02X?}"
                );

                let mut block = SmirBlock::new(BlockId(0), PC);
                block.ops = result.ops;
                block.set_terminator(Terminator::Return { values: Vec::new() });
                let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
                function.add_block(block);
                function
                    .x86_instruction_bytes
                    .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
                optimize_function(&mut function, OptLevel::O2);

                assert!(
                    is_native_clobber_safe_excluding(&function, &HashMap::new(), true),
                    "O2 gate rejected {bytes:02X?}: {:?}",
                    function.blocks[0].ops
                );
                let mut lowerer = X86_64Lowerer::new();
                lowerer
                    .lower_function(&function)
                    .unwrap_or_else(|error| panic!("lower {bytes:02X?}: {error:?}"));
                lowerer
                    .finalize()
                    .unwrap_or_else(|error| panic!("finalize {bytes:02X?}: {error:?}"));
            }
        }
    }
    assert_eq!(seen.len(), 1_792);
}
