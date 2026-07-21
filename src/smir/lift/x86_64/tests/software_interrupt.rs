//! Strict lift, canonical interpretation, optimizer, oracle-style non-strict,
//! and exact frontier coverage for `INT imm8` (`CD ib`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_software_interrupt(result: &LiftResult) -> (u8, u64, u64, bool) {
    assert!(result.ops.is_empty());
    assert!(result.branch_targets.is_empty());
    match result.control_flow {
        ControlFlow::Trap {
            kind:
                TrapKind::X86SoftwareInterrupt {
                    vector,
                    fault_pc,
                    return_pc,
                    requires_apx,
                },
        } => (vector, fault_pc, return_pc, requires_apx),
        ref other => panic!("expected exact x86 software-interrupt trap, got {other:?}"),
    }
}

fn software_interrupt_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict INT imm8 lift");
    let ControlFlow::Trap { kind } = result.control_flow else {
        panic!("INT imm8 must terminate with a trap")
    };
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap { kind });
    block
}

#[test]
fn int_imm8_strictly_lifts_all_vectors_and_ignored_prefix_classes() {
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
        for vector in 0u8..=u8::MAX {
            let mut bytes = prefixes.to_vec();
            bytes.extend_from_slice(&[0xCD, vector]);
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("vector {vector:02X}, prefixes {prefixes:02X?}: {error:?}")
            });
            assert_eq!(result.bytes_consumed, bytes.len());
            assert_eq!(
                exact_software_interrupt(&result),
                (vector, 0x1000, 0x1000 + bytes.len() as u64, false)
            );
        }
    }
}

#[test]
fn int_imm8_rex2_records_apx_dependency_and_rejects_illegal_or_incomplete_forms() {
    for bytes in [
        &[0xD5, 0x00, 0xCD, 0x00][..],
        &[0xD5, 0x00, 0xCD, 0xFF],
        &[0x66, 0x67, 0xF2, 0x2E, 0xD5, 0x00, 0xCD, 0x80],
    ] {
        let result = lift_single(bytes).expect("REX2 INT imm8 must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(
            exact_software_interrupt(&result),
            (
                *bytes.last().unwrap(),
                0x1000,
                0x1000 + bytes.len() as u64,
                true,
            )
        );
    }

    for bytes in [
        &[0xF0, 0xCD, 0x80][..],
        &[0xF0, 0xD5, 0x00, 0xCD, 0x80],
        &[0x48, 0xD5, 0x00, 0xCD, 0x80],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    for bytes in [&[0xCD][..], &[0x66, 0xCD], &[0xD5, 0x00, 0xCD]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Incomplete { .. })
        ));
    }
}

#[test]
fn int_imm8_non_strict_oracle_path_preserves_full_encoding_and_terminal_payload() {
    let bytes = [0x66, 0xCD, 0x80];
    let mut lifter = X86_64Lifter::new();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, &bytes, &mut context)
        .expect("non-strict INT imm8 lift");
    assert_eq!(result.bytes_consumed, bytes.len());
    assert_eq!(
        exact_software_interrupt(&result),
        (0x80, 0x1000, 0x1003, false)
    );
}

#[test]
fn int_imm8_canonical_interpreter_reports_exact_pending_delivery() {
    let mut context = SmirContext::new_x86_64();
    context.pc = 0x1000;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &software_interrupt_block(&[0x66, 0xCD, 0x80]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::X86SoftwareInterrupt {
            vector: 0x80,
            fault_pc: 0x1000,
            return_pc: 0x1003,
        })
    ));
}

#[test]
fn int_imm8_canonical_interpreter_gates_rex2_on_apx_before_delivery() {
    let block = software_interrupt_block(&[0xD5, 0x00, 0xCD, 0x21]);
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
        BlockResult::Exit(ExitReason::X86SoftwareInterrupt {
            vector: 0x21,
            fault_pc: 0x1000,
            return_pc: 0x1004,
        })
    ));
}

#[test]
fn int_imm8_canonical_interpreter_rejects_an_x86_trap_in_non_x86_state() {
    let mut context = SmirContext::new_aarch64();
    context.pc = 0x1000;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &software_interrupt_block(&[0xCD, 0x80]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
}

#[test]
fn int_imm8_optimizer_preserves_exact_terminal_payload() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(SourceArch::X86_64);
        let mut function = lifter
            .lift_function(
                0x1000,
                &TestMemory::new(0x1000, vec![0xCD, 0x80]),
                &mut context,
            )
            .expect("strict INT imm8 function lift");
        optimize_function(&mut function, level);
        assert!(matches!(
            function.blocks[0].terminator,
            Terminator::Trap {
                kind: TrapKind::X86SoftwareInterrupt {
                    vector: 0x80,
                    fault_pc: 0x1000,
                    return_pc: 0x1002,
                    requires_apx: false,
                }
            }
        ));
    }
}

#[test]
fn int_imm8_interpreter_frontier_preserves_supported_native_prefix_at_exact_pc() {
    let code = vec![0x48, 0x83, 0xC0, 0x01, 0xCD, 0x80]; // ADD RAX,1; INT 80h
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
        .expect("INT imm8 frontier function lift");

    let prefix = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1800)
        .expect("supported prefix block");
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1804)
        .expect("exact INT imm8 frontier block");
    assert!(!prefix.ops.is_empty());
    assert!(matches!(
        prefix.terminator,
        Terminator::Branch { target } if target == frontier.id
    ));
    assert!(frontier.ops.is_empty());
    assert!(matches!(frontier.terminator, Terminator::Return { .. }));

    let mut entry_lifter = X86_64Lifter::strict();
    entry_lifter.set_interpreter_frontiers(true);
    let mut entry_context = LiftContext::new(SourceArch::X86_64);
    let entry = entry_lifter
        .lift_function(
            0x2000,
            &TestMemory::new(0x2000, vec![0xCD, 0x80]),
            &mut entry_context,
        )
        .expect("entry INT imm8 frontier function lift");
    assert_eq!(entry.blocks.len(), 1);
    assert_eq!(entry.blocks[0].guest_pc, 0x2000);
    assert!(entry.blocks[0].ops.is_empty());
    assert!(matches!(
        entry.blocks[0].terminator,
        Terminator::Return { .. }
    ));
}
