//! Strict lift, canonical interpretation, optimizer, and frontier coverage for
//! `INT1`/`ICEBP` (`F1`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_debug_trap(result: &LiftResult) -> (u64, u64, bool) {
    assert!(result.ops.is_empty());
    match result.control_flow {
        ControlFlow::Trap {
            kind:
                TrapKind::X86Debug {
                    fault_pc,
                    return_pc,
                    requires_apx,
                },
        } => (fault_pc, return_pc, requires_apx),
        ref other => panic!("expected exact x86 debug trap, got {other:?}"),
    }
}

fn icebp_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict INT1 lift");
    let ControlFlow::Trap { kind } = result.control_flow else {
        panic!("INT1 must terminate with a trap")
    };
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap { kind });
    block
}

#[test]
fn int1_strictly_lifts_ignored_legacy_and_rex_prefixes_with_exact_return_pc() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x66, 0x67, 0x40, 0x41, 0x42, 0x44, 0x48, 0x4F, 0xF2,
        0xF3,
    ] {
        let bytes = [prefix, 0xF1];
        let result = lift_single(&bytes).expect("architecturally ignored INT1 prefix");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(
            exact_debug_trap(&result),
            (0x1000, 0x1000 + bytes.len() as u64, false)
        );
    }

    let bytes = [0x66, 0xF3, 0x2E, 0x48, 0xF1];
    let result = lift_single(&bytes).expect("ordered ignored INT1 prefixes");
    assert_eq!(result.bytes_consumed, bytes.len());
    assert_eq!(exact_debug_trap(&result), (0x1000, 0x1005, false));
}

#[test]
fn int1_rex2_lift_records_apx_dependency_and_rejects_illegal_prefixes() {
    for bytes in [
        &[0xD5, 0x00, 0xF1][..],
        &[0x66, 0x67, 0xF2, 0x2E, 0xD5, 0x00, 0xF1],
    ] {
        let result = lift_single(bytes).expect("REX2 INT1 must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(
            exact_debug_trap(&result),
            (0x1000, 0x1000 + bytes.len() as u64, true)
        );
    }

    for bytes in [
        &[0xF0, 0xF1][..],
        &[0xF0, 0xD5, 0x00, 0xF1],
        &[0x48, 0xD5, 0x00, 0xF1],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn int1_canonical_interpreter_reports_post_instruction_debug_without_dr6_mutation() {
    const DR6_SENTINEL: u64 = 0xFFFF_0FF0;
    let mut context = SmirContext::new_x86_64();
    context.pc = 0x1000;
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.dr6 = DR6_SENTINEL;

    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &icebp_block(&[0x66, 0xF1]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Debug { addr: 0x1002 })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.dr6, DR6_SENTINEL);
}

#[test]
fn int1_canonical_interpreter_gates_rex2_on_apx_before_debug_delivery() {
    let block = icebp_block(&[0xD5, 0x00, 0xF1]);
    let mut context = SmirContext::new_x86_64();
    context.pc = 0x1000;

    let disabled =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    assert!(matches!(
        disabled,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));

    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.apx_enabled = true;
    let enabled =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    assert!(matches!(
        enabled,
        BlockResult::Exit(ExitReason::Debug { addr: 0x1003 })
    ));
}

#[test]
fn int1_canonical_interpreter_rejects_an_x86_trap_in_non_x86_state() {
    let mut context = SmirContext::new_aarch64();
    context.pc = 0x1000;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &icebp_block(&[0xF1]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
}

#[test]
fn int1_optimizer_preserves_exact_terminal_payload() {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let mut function = lifter
        .lift_function(0x1000, &TestMemory::new(0x1000, vec![0xF1]), &mut context)
        .expect("strict INT1 function lift");
    optimize_function(&mut function, OptLevel::O2);
    assert!(matches!(
        function.blocks[0].terminator,
        Terminator::Trap {
            kind: TrapKind::X86Debug {
                fault_pc: 0x1000,
                return_pc: 0x1001,
                requires_apx: false,
            }
        }
    ));
}

#[test]
fn int1_interpreter_frontier_preserves_supported_native_prefix_at_exact_pc() {
    let code = vec![0x48, 0x83, 0xC0, 0x01, 0xF1]; // ADD RAX,1; INT1
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
        .expect("INT1 frontier function lift");

    let prefix = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1800)
        .expect("supported prefix block");
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1804)
        .expect("exact INT1 frontier block");
    assert!(!prefix.ops.is_empty());
    assert!(matches!(
        prefix.terminator,
        Terminator::Branch { target } if target == frontier.id
    ));
    assert!(frontier.ops.is_empty());
    assert!(matches!(frontier.terminator, Terminator::Return { .. }));
}
