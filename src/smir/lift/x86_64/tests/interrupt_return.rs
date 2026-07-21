//! Strict lift, canonical interpretation, optimizer, oracle-style non-strict,
//! and exact frontier coverage for `IRET`/`IRETD`/`IRETQ` (`CF`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_interrupt_return(result: &LiftResult) -> (OpWidth, u64, bool) {
    assert!(result.ops.is_empty());
    assert!(result.branch_targets.is_empty());
    match result.control_flow {
        ControlFlow::Trap {
            kind:
                TrapKind::X86InterruptReturn {
                    width,
                    fault_pc,
                    requires_apx,
                },
        } => (width, fault_pc, requires_apx),
        ref other => panic!("expected exact x86 interrupt-return trap, got {other:?}"),
    }
}

fn interrupt_return_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict IRET-family lift");
    let ControlFlow::Trap { kind } = result.control_flow else {
        panic!("IRET family must terminate with a trap")
    };
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap { kind });
    block
}

#[test]
fn iret_family_strictly_lifts_exact_widths_and_ignored_prefix_classes() {
    let cases: &[(&str, &[u8], OpWidth)] = &[
        ("bare IRETD", &[0xCF], OpWidth::W32),
        ("ES override", &[0x26, 0xCF], OpWidth::W32),
        ("CS override", &[0x2E, 0xCF], OpWidth::W32),
        ("SS override", &[0x36, 0xCF], OpWidth::W32),
        ("DS override", &[0x3E, 0xCF], OpWidth::W32),
        ("FS override", &[0x64, 0xCF], OpWidth::W32),
        ("GS override", &[0x65, 0xCF], OpWidth::W32),
        ("address size", &[0x67, 0xCF], OpWidth::W32),
        ("REPNE", &[0xF2, 0xCF], OpWidth::W32),
        ("REP", &[0xF3, 0xCF], OpWidth::W32),
        ("REX", &[0x40, 0xCF], OpWidth::W32),
        ("REX payload", &[0x47, 0xCF], OpWidth::W32),
        ("IRET", &[0x66, 0xCF], OpWidth::W16),
        ("IRETQ", &[0x48, 0xCF], OpWidth::W64),
        ("66 then REX.W", &[0x66, 0x48, 0xCF], OpWidth::W64),
        ("REX.W then 66", &[0x48, 0x66, 0xCF], OpWidth::W16),
        ("last REX wins", &[0x48, 0x40, 0xCF], OpWidth::W32),
        (
            "ordered ignored prefixes then REX.W",
            &[0x66, 0x67, 0xF3, 0x2E, 0x48, 0xCF],
            OpWidth::W64,
        ),
    ];

    for &(name, bytes, width) in cases {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(
            exact_interrupt_return(&result),
            (width, 0x1000, false),
            "{name}"
        );
    }
}

#[test]
fn iret_family_rex2_exhaustively_records_map0_width_and_apx_dependency() {
    for payload in 0u8..=0x7F {
        let bytes = [0xD5, payload, 0xCF];
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("REX2 payload {payload:#04x}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(
            exact_interrupt_return(&result),
            (
                if payload & 0x08 != 0 {
                    OpWidth::W64
                } else {
                    OpWidth::W32
                },
                0x1000,
                true,
            ),
            "REX2 payload {payload:#04x}"
        );
    }

    for (bytes, width) in [
        (&[0x66, 0xD5, 0x00, 0xCF][..], OpWidth::W16),
        (&[0x66, 0xD5, 0x08, 0xCF], OpWidth::W64),
        (&[0x66, 0x67, 0xF2, 0x2E, 0xD5, 0x07, 0xCF], OpWidth::W16),
    ] {
        let result = lift_single(bytes).expect("legacy prefixes before REX2 IRET");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(exact_interrupt_return(&result), (width, 0x1000, true));
    }
}

#[test]
fn iret_family_rejects_lock_and_invalid_rex2_order_without_confusing_map1() {
    for bytes in [
        &[0xF0, 0xCF][..],
        &[0xF0, 0xD5, 0x00, 0xCF],
        &[0x48, 0xD5, 0x00, 0xCF],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    for bytes in [&[0xD5][..], &[0xD5, 0x00]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Incomplete { .. })
        ));
    }

    let map1 = lift_single(&[0xD5, 0x80, 0xCF]).expect("REX2 map-1 BSWAP");
    assert_eq!(map1.bytes_consumed, 3);
    assert!(matches!(map1.control_flow, ControlFlow::Fallthrough));
    assert!(!map1.ops.is_empty(), "map-1 CF must not decode as IRET");
}

#[test]
fn iret_family_non_strict_oracle_path_preserves_width_and_terminal_payload() {
    for (bytes, width) in [
        (&[0x66, 0xCF][..], OpWidth::W16),
        (&[0xCF][..], OpWidth::W32),
        (&[0x48, 0xCF][..], OpWidth::W64),
    ] {
        let mut lifter = X86_64Lifter::new();
        let mut context = LiftContext::new(SourceArch::X86_64);
        let result = lifter
            .lift_insn(0x1000, bytes, &mut context)
            .expect("non-strict IRET-family lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(exact_interrupt_return(&result), (width, 0x1000, false));
    }
}

#[test]
fn iret_family_canonical_interpreter_reports_exact_noncommitting_handoff() {
    let mut context = SmirContext::new_x86_64();
    context.pc = 0x1000;
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.gpr[4] = 0x1234_5678_9ABC_DEF0;
    x86.rflags = 0x0000_0000_0020_0247;

    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &interrupt_return_block(&[0x48, 0xCF]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::X86InterruptReturn {
            width: OpWidth::W64,
            fault_pc: 0x1000,
        })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[4], 0x1234_5678_9ABC_DEF0);
    assert_eq!(x86.rflags, 0x0000_0000_0020_0247);
}

#[test]
fn iret_family_canonical_interpreter_gates_rex2_on_apx_before_handoff() {
    let block = interrupt_return_block(&[0xD5, 0x08, 0xCF]);
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
        BlockResult::Exit(ExitReason::X86InterruptReturn {
            width: OpWidth::W64,
            fault_pc: 0x1000,
        })
    ));
}

#[test]
fn iret_family_canonical_interpreter_rejects_x86_handoff_in_non_x86_state() {
    let mut context = SmirContext::new_aarch64();
    context.pc = 0x1000;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &interrupt_return_block(&[0xCF]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
}

#[test]
fn iret_family_optimizer_preserves_exact_terminal_payload_for_all_widths() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for (bytes, width, requires_apx) in [
            (&[0x66, 0xCF][..], OpWidth::W16, false),
            (&[0xCF][..], OpWidth::W32, false),
            (&[0x48, 0xCF][..], OpWidth::W64, false),
            (&[0xD5, 0x08, 0xCF][..], OpWidth::W64, true),
        ] {
            let mut lifter = X86_64Lifter::strict();
            let mut context = LiftContext::new(SourceArch::X86_64);
            let mut function = lifter
                .lift_function(
                    0x1000,
                    &TestMemory::new(0x1000, bytes.to_vec()),
                    &mut context,
                )
                .expect("strict IRET-family function lift");
            optimize_function(&mut function, level);
            assert!(matches!(
                function.blocks[0].terminator,
                Terminator::Trap {
                    kind: TrapKind::X86InterruptReturn {
                        width: got_width,
                        fault_pc: 0x1000,
                        requires_apx: got_apx,
                    }
                } if got_width == width && got_apx == requires_apx
            ));
        }
    }
}

#[test]
fn iret_family_interpreter_frontier_preserves_supported_prefix_at_exact_pc() {
    let code = vec![0x48, 0x83, 0xC0, 0x01, 0x48, 0xCF]; // ADD RAX,1; IRETQ
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
        .expect("IRETQ frontier function lift");

    let prefix = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1800)
        .expect("supported prefix block");
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1804)
        .expect("exact IRETQ frontier block");
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
            &TestMemory::new(0x2000, vec![0x48, 0xCF]),
            &mut entry_context,
        )
        .expect("entry IRETQ frontier function lift");
    assert_eq!(entry.blocks.len(), 1);
    assert_eq!(entry.blocks[0].guest_pc, 0x2000);
    assert!(entry.blocks[0].ops.is_empty());
    assert!(matches!(
        entry.blocks[0].terminator,
        Terminator::Return { .. }
    ));
}
