//! Strict lift, canonical interpretation, optimizer, and exact frontier
//! coverage for `INT3` (`CC`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_breakpoint(result: &LiftResult) -> (u64, u64, bool) {
    assert!(result.ops.is_empty());
    assert!(result.branch_targets.is_empty());
    match result.control_flow {
        ControlFlow::Trap {
            kind:
                TrapKind::X86Breakpoint {
                    fault_pc,
                    return_pc,
                    requires_apx,
                },
        } => (fault_pc, return_pc, requires_apx),
        ref other => panic!("expected exact x86 breakpoint trap, got {other:?}"),
    }
}

fn breakpoint_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict INT3 lift");
    let ControlFlow::Trap { kind } = result.control_flow else {
        panic!("INT3 must terminate with a trap")
    };
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap { kind });
    block
}

#[test]
fn int3_strictly_lifts_ignored_legacy_and_rex_prefixes_with_exact_return_pc() {
    const PREFIXES: &[&[u8]] = &[
        &[],
        &[0x26],
        &[0x2E],
        &[0x36],
        &[0x3E],
        &[0x64],
        &[0x65],
        &[0x66],
        &[0x67],
        &[0x40],
        &[0x41],
        &[0x42],
        &[0x44],
        &[0x48],
        &[0x4F],
        &[0xF2],
        &[0xF3],
        &[0x66, 0x67, 0xF3, 0x2E, 0x48],
    ];

    for &prefixes in PREFIXES {
        let mut bytes = prefixes.to_vec();
        bytes.push(0xCC);
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("prefixes {prefixes:02X?}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(
            exact_breakpoint(&result),
            (0x1000, 0x1000 + bytes.len() as u64, false)
        );
    }
}

#[test]
fn int3_all_rex2_map_zero_payloads_retain_apx_dependency() {
    for payload in 0_u8..=0x7F {
        let bytes = [0xD5, payload, 0xCC];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("REX2 payload {payload:#04x}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(exact_breakpoint(&result), (0x1000, 0x1003, true));
    }

    for bytes in [
        &[0xF0, 0xCC][..],
        &[0xF0, 0xD5, 0x00, 0xCC],
        &[0x48, 0xD5, 0x00, 0xCC],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn int3_and_int_vector_three_remain_distinct_terminal_events() {
    let int3 = lift_single(&[0xCC]).expect("INT3 lift");
    assert_eq!(exact_breakpoint(&int3), (0x1000, 0x1001, false));

    let int_3 = lift_single(&[0xCD, 0x03]).expect("INT 3 lift");
    assert!(matches!(
        int_3.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::X86SoftwareInterrupt {
                vector: 3,
                fault_pc: 0x1000,
                return_pc: 0x1002,
                requires_apx: false,
            }
        }
    ));
}

#[test]
fn int3_non_strict_oracle_path_preserves_full_encoding_and_terminal_payload() {
    let bytes = [0x66, 0xF3, 0x2E, 0x48, 0xCC];
    let mut lifter = X86_64Lifter::new();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, &bytes, &mut context)
        .expect("non-strict INT3 lift");
    assert_eq!(result.bytes_consumed, bytes.len());
    assert_eq!(exact_breakpoint(&result), (0x1000, 0x1005, false));
}

#[test]
fn int3_canonical_interpreter_reports_exact_pending_breakpoint_delivery() {
    let mut context = SmirContext::new_x86_64();
    context.pc = 0x1000;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &breakpoint_block(&[0x66, 0xCC]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::X86Breakpoint {
            fault_pc: 0x1000,
            return_pc: 0x1002,
        })
    ));
}

#[test]
fn int3_canonical_interpreter_gates_rex2_on_apx_before_breakpoint_delivery() {
    let block = breakpoint_block(&[0xD5, 0x00, 0xCC]);
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
        BlockResult::Exit(ExitReason::X86Breakpoint {
            fault_pc: 0x1000,
            return_pc: 0x1003,
        })
    ));
}

#[test]
fn int3_canonical_interpreter_rejects_an_x86_trap_in_non_x86_state() {
    let mut context = SmirContext::new_aarch64();
    context.pc = 0x1000;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &breakpoint_block(&[0xCC]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
}

#[test]
fn int3_optimizer_preserves_exact_terminal_payload() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(SourceArch::X86_64);
        let mut function = lifter
            .lift_function(0x1000, &TestMemory::new(0x1000, vec![0xCC]), &mut context)
            .expect("strict INT3 function lift");
        optimize_function(&mut function, level);
        assert!(matches!(
            function.blocks[0].terminator,
            Terminator::Trap {
                kind: TrapKind::X86Breakpoint {
                    fault_pc: 0x1000,
                    return_pc: 0x1001,
                    requires_apx: false,
                }
            }
        ));
    }
}

#[test]
fn int3_interpreter_frontier_preserves_supported_native_prefix_at_exact_pc() {
    let code = vec![0x48, 0x83, 0xC0, 0x01, 0xCC]; // ADD RAX,1; INT3
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
        .expect("INT3 frontier function lift");

    let prefix = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1800)
        .expect("supported prefix block");
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1804)
        .expect("exact INT3 frontier block");
    assert!(!prefix.ops.is_empty());
    assert!(matches!(
        prefix.terminator,
        Terminator::Branch { target } if target == frontier.id
    ));
    assert!(frontier.ops.is_empty());
    assert!(matches!(frontier.terminator, Terminator::Return { .. }));
}
