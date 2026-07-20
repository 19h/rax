//! Strict lift, metadata, optimizer, and canonical interpreter coverage for
//! CLTS.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn clts_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict CLTS lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_clts(
    configure: impl FnOnce(&mut crate::smir::ir::context::X86RegState),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    configure(x86);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &clts_block(&[0x0F, 0x06]),
    );
    (result, context)
}

#[test]
fn clts_strictly_lifts_to_one_exact_state_operation() {
    let result = lift_single(&[0x0F, 0x06]).expect("CLTS must strictly lift");
    assert_eq!(result.bytes_consumed, 2);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Clts,
            guest_pc: 0x1000,
            ..
        }]
    ));
}

#[test]
fn clts_ignores_every_non_lock_legacy_and_rex_prefix_class() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x41, 0x42, 0x44, 0x48, 0x4F, // representative REX forms
        0xF2, 0xF3, // repeat prefixes
    ] {
        let bytes = [prefix, 0x0F, 0x06];
        let result = lift_single(&bytes).expect("architecturally ignored CLTS prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:#04x}");
        assert!(matches!(result.ops[0].kind, OpKind::X86Clts));
    }

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x06]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn clts_metadata_is_operand_free_flag_neutral_stateful_and_jit_safe() {
    let op = lift_single(&[0x0F, 0x06]).unwrap().ops.remove(0);
    assert!(op.kind.source_vregs().is_empty());
    assert!(op.kind.dests().is_empty());
    assert!(op.kind.flags_read().is_empty());
    assert!(op.kind.flags_written().is_empty());
    assert!(op.kind.has_side_effects());
    assert!(!op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(op.kind.is_jit_safe());
    assert!(op.is_jit_safe());
}

#[test]
fn clts_interpreter_clears_only_ts_at_cpl0_and_in_real_mode() {
    for (name, cr0, cpl) in [
        ("protected-cpl0", 0xFFFF_FFFF | (1 << 3), 0),
        ("real-mode-stale-rpl3", 0xFFFF_FFFE | (1 << 3), 3),
    ] {
        let expected = cr0 & !(1 << 3);
        let (result, context) = execute_clts(|x86| {
            x86.cr0 = cr0;
            x86.cpl = cpl;
            x86.gpr[0] = 0xA5A5_5A5A_DEAD_BEEF;
            x86.rflags = 0x0004_0ED7;
        });
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.cr0, expected, "{name}: CR0");
        assert_eq!(x86.gpr[0], 0xA5A5_5A5A_DEAD_BEEF, "{name}: RAX");
        assert_eq!(x86.rflags, 0x0004_0ED7, "{name}: RFLAGS");
    }
}

#[test]
fn clts_interpreter_cpl3_fault_is_precise_and_noncommitting() {
    let initial = 0x8005_003B;
    let (result, context) = execute_clts(|x86| {
        x86.cr0 = initial;
        x86.cpl = 3;
    });
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr0, initial);
}

#[test]
fn clts_survives_o2_as_an_ordered_architectural_state_write() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::X86Clts);
    builder.push_op(0x1002, OpKind::X86Clts);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let original = builder.finish();
    let mut optimized = original.clone();
    optimize_function(&mut optimized, OptLevel::O2);

    assert_eq!(
        optimized
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Clts))
            .count(),
        2,
        "optimizer must not discard or merge potentially faulting CLTS operations"
    );

    for function in [&original, &optimized] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 0x8005_003B;
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Return { to: 0 })
        ));
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.cr0 & (1 << 3), 0);
    }
}
