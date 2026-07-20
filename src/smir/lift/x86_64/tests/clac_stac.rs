//! Strict lift, metadata, optimizer, and canonical interpreter coverage for
//! CLAC/STAC.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_set_ac(result: &LiftResult) -> bool {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::SetAC { value } => *value,
        other => panic!("expected one exact SetAC op, got {other:?}"),
    }
}

fn ac_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict CLAC/STAC lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_ac(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &ac_block(bytes),
    );
    (result, context)
}

#[test]
fn clac_stac_strictly_lift_without_an_interpreter_frontier() {
    for (bytes, value) in [
        (&[0x0F, 0x01, 0xCA][..], false),
        (&[0x0F, 0x01, 0xCB][..], true),
    ] {
        let result = lift_single(bytes).expect("CLAC/STAC must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert_eq!(exact_set_ac(&result), value);
    }
}

#[test]
fn clac_stac_ignore_non_lock_legacy_and_rex_prefixes() {
    for modrm in [0xCA, 0xCB] {
        for prefix in [
            0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x66, 0x67, 0x40, 0x41, 0x42, 0x44, 0x48, 0x4F,
            0xF2, 0xF3,
        ] {
            let bytes = [prefix, 0x0F, 0x01, modrm];
            let result = lift_single(&bytes).expect("architecturally ignored CLAC/STAC prefix");
            assert_eq!(result.bytes_consumed, bytes.len());
            assert_eq!(exact_set_ac(&result), modrm == 0xCB);
        }
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x01, modrm]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn clac_stac_metadata_is_operand_free_status_neutral_and_stateful() {
    for bytes in [&[0x0F, 0x01, 0xCA][..], &[0x0F, 0x01, 0xCB]] {
        let op = lift_single(bytes).unwrap().ops.remove(0);
        assert!(op.kind.source_vregs().is_empty());
        assert!(op.kind.dests().is_empty());
        assert!(op.kind.flags_read().is_empty());
        assert!(op.kind.flags_written().is_empty());
        assert!(op.kind.has_side_effects());
        assert!(!op.kind.reads_memory());
        assert!(!op.kind.writes_memory());
        assert!(op.is_jit_safe());
    }
}

#[test]
fn clac_stac_interpreter_changes_only_ac_and_materializes_pending_status() {
    for (bytes, expected_ac) in [
        (&[0x0F, 0x01, 0xCA][..], false),
        (&[0x0F, 0x01, 0xCB][..], true),
    ] {
        let (result, context) = execute_ac(bytes, |context| {
            context.flags.materialized = MaterializedFlags {
                df: true,
                ac: !expected_ac,
                ..Default::default()
            };
            context.flags.set_lazy_add(u64::MAX, 1, 0, OpWidth::W64);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert!(context.flags.lazy.is_none());
        assert!(context.flags.materialized.cf);
        assert!(context.flags.materialized.zf);
        assert!(!context.flags.materialized.sf);
        assert!(!context.flags.materialized.of);
        assert!(context.flags.materialized.pf);
        assert!(context.flags.materialized.af);
        assert!(context.flags.materialized.df);
        assert_eq!(context.flags.materialized.ac, expected_ac);
    }
}

#[test]
fn clac_stac_interpreter_privilege_fault_is_ud_and_noncommitting() {
    for (bytes, initial_ac) in [
        (&[0x0F, 0x01, 0xCA][..], true),
        (&[0x0F, 0x01, 0xCB][..], false),
    ] {
        let (result, context) = execute_ac(bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.cpl = 3;
            context.flags.materialized.ac = initial_ac;
        });
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined {
                addr: 0x1000,
                opcode: 0
            })
        ));
        assert_eq!(context.flags.materialized.ac, initial_ac);
    }
}

#[test]
fn clac_stac_interpreter_real_mode_bypasses_stale_selector_privilege() {
    for (bytes, expected_ac) in [
        (&[0x0F, 0x01, 0xCA][..], false),
        (&[0x0F, 0x01, 0xCB][..], true),
    ] {
        let (result, context) = execute_ac(bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 0;
            x86.cpl = 3;
            context.flags.materialized.ac = !expected_ac;
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.flags.materialized.ac, expected_ac);
    }
}

#[test]
fn clac_stac_o2_preserves_order_results_and_fault_frontier() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (index, value) in [true, false, true].into_iter().enumerate() {
        builder.push_op(0x1000 + index as u64 * 3, OpKind::SetAC { value });
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let original = builder.finish();
    let mut optimized = original.clone();
    optimize_function(&mut optimized, OptLevel::O2);

    let values: Vec<_> = optimized
        .entry_block()
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::SetAC { value } => Some(*value),
            _ => None,
        })
        .collect();
    assert_eq!(values, [true, false, true]);

    let mut successful_rflags = Vec::new();
    for function in [&original, &optimized] {
        let mut context = SmirContext::new_x86_64();
        context.flags.materialized = MaterializedFlags {
            df: true,
            ..Default::default()
        };
        context.flags.set_lazy_add(u64::MAX, 1, 0, OpWidth::W64);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(8),
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Return { to: 0 })
        ));
        assert!(context.flags.materialized.ac);
        successful_rflags.push(context.flags.materialized.to_rflags());
    }
    assert_eq!(successful_rflags[0], successful_rflags[1]);

    for function in [&original, &optimized] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cpl = 3;
        context.flags.materialized.ac = false;
        context.flags.set_lazy_add(u64::MAX, 1, 0, OpWidth::W64);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(8),
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined {
                addr: 0x1000,
                opcode: 0
            })
        ));
        assert!(!context.flags.materialized.ac);
        assert!(
            context.flags.lazy.is_some(),
            "faulting SetAC must not commit"
        );
    }
}
