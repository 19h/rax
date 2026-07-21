//! Strict lift, canonical interpretation, optimizer, oracle-style non-strict,
//! and exact interpreter-frontier coverage for string port I/O (`6C`--`6F`).

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::{X86Segment, X86StringIoKind};
use crate::smir::optimize::{OptLevel, optimize_function};

type StringIoShape = (
    X86StringIoKind,
    MemWidth,
    OpWidth,
    bool,
    X86Segment,
    u64,
    u64,
    bool,
);

fn exact_string_io(result: &LiftResult) -> StringIoShape {
    assert!(result.ops.is_empty());
    assert!(result.branch_targets.is_empty());
    match result.control_flow {
        ControlFlow::Trap {
            kind:
                TrapKind::X86StringIo {
                    kind,
                    width,
                    address_width,
                    repeated,
                    memory_segment,
                    fault_pc,
                    return_pc,
                    requires_apx,
                },
        } => (
            kind,
            width,
            address_width,
            repeated,
            memory_segment,
            fault_pc,
            return_pc,
            requires_apx,
        ),
        ref other => panic!("expected exact x86 string-I/O trap, got {other:?}"),
    }
}

fn string_io_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict string-I/O lift");
    let ControlFlow::Trap { kind } = result.control_flow else {
        panic!("string I/O must terminate with a typed trap")
    };
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap { kind });
    block
}

#[test]
fn string_io_strictly_lifts_all_width_address_rep_and_rex_controls() {
    let cases: &[(&str, &[u8], X86StringIoKind, MemWidth, OpWidth, bool)] = &[
        (
            "INSB",
            &[0x6C],
            X86StringIoKind::Ins,
            MemWidth::B1,
            OpWidth::W64,
            false,
        ),
        (
            "INSD",
            &[0x6D],
            X86StringIoKind::Ins,
            MemWidth::B4,
            OpWidth::W64,
            false,
        ),
        (
            "INSW",
            &[0x66, 0x6D],
            X86StringIoKind::Ins,
            MemWidth::B2,
            OpWidth::W64,
            false,
        ),
        (
            "REX.W does not promote INSD",
            &[0x48, 0x6D],
            X86StringIoKind::Ins,
            MemWidth::B4,
            OpWidth::W64,
            false,
        ),
        (
            "address-size INSD",
            &[0x67, 0x6D],
            X86StringIoKind::Ins,
            MemWidth::B4,
            OpWidth::W32,
            false,
        ),
        (
            "REP INSW addr32",
            &[0xF3, 0x67, 0x66, 0x6D],
            X86StringIoKind::Ins,
            MemWidth::B2,
            OpWidth::W32,
            true,
        ),
        (
            "REPNE is REP for INSB",
            &[0xF2, 0x6C],
            X86StringIoKind::Ins,
            MemWidth::B1,
            OpWidth::W64,
            true,
        ),
        (
            "OUTSB",
            &[0x6E],
            X86StringIoKind::Outs,
            MemWidth::B1,
            OpWidth::W64,
            false,
        ),
        (
            "OUTSD",
            &[0x6F],
            X86StringIoKind::Outs,
            MemWidth::B4,
            OpWidth::W64,
            false,
        ),
        (
            "OUTSW",
            &[0x66, 0x6F],
            X86StringIoKind::Outs,
            MemWidth::B2,
            OpWidth::W64,
            false,
        ),
        (
            "all REX bits ignored by OUTSD",
            &[0x4F, 0x6F],
            X86StringIoKind::Outs,
            MemWidth::B4,
            OpWidth::W64,
            false,
        ),
        (
            "REPNE OUTSW addr32",
            &[0xF2, 0x67, 0x66, 0x6F],
            X86StringIoKind::Outs,
            MemWidth::B2,
            OpWidth::W32,
            true,
        ),
    ];

    for &(name, bytes, kind, width, address_width, repeated) in cases {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(
            exact_string_io(&result),
            (
                kind,
                width,
                address_width,
                repeated,
                if kind == X86StringIoKind::Ins {
                    X86Segment::Es
                } else {
                    X86Segment::Ds
                },
                0x1000,
                0x1000 + bytes.len() as u64,
                false,
            ),
            "{name}"
        );
    }
}

#[test]
fn string_io_preserves_exact_effective_segment_contract() {
    for (prefix, segment) in [
        (0x26, X86Segment::Es),
        (0x2E, X86Segment::Cs),
        (0x36, X86Segment::Ss),
        (0x3E, X86Segment::Ds),
        (0x64, X86Segment::Fs),
        (0x65, X86Segment::Gs),
    ] {
        let outs = lift_single(&[prefix, 0x6E]).expect("segment-overridden OUTSB");
        assert_eq!(exact_string_io(&outs).4, segment, "prefix {prefix:#04x}");

        let ins = lift_single(&[prefix, 0x6C]).expect("segment-prefixed INSB");
        assert_eq!(
            exact_string_io(&ins).4,
            X86Segment::Es,
            "INS destination cannot be overridden by prefix {prefix:#04x}"
        );
    }

    let last_wins = lift_single(&[0x64, 0x65, 0x6F]).expect("last segment prefix wins");
    assert_eq!(exact_string_io(&last_wins).4, X86Segment::Gs);
}

#[test]
fn string_io_rex2_map0_is_exhaustive_apx_dependent_and_width_neutral() {
    for payload in 0u8..=0x7F {
        for opcode in 0x6C..=0x6F {
            let bytes = [0xD5, payload, opcode];
            let result = lift_single(&bytes).unwrap_or_else(|error| {
                panic!("REX2 payload {payload:#04x}, opcode {opcode:#04x}: {error:?}")
            });
            let shape = exact_string_io(&result);
            assert_eq!(result.bytes_consumed, 3);
            assert_eq!(
                shape.1,
                if opcode & 1 == 0 {
                    MemWidth::B1
                } else {
                    MemWidth::B4
                }
            );
            assert_eq!(shape.2, OpWidth::W64);
            assert!(shape.7, "REX2 payload {payload:#04x}");
        }
    }

    let overridden = lift_single(&[0x66, 0x67, 0xD5, 0x7F, 0x6F])
        .expect("legacy size prefixes before map-0 REX2 OUTSW");
    let shape = exact_string_io(&overridden);
    assert_eq!(shape.1, MemWidth::B2);
    assert_eq!(shape.2, OpWidth::W32);
    assert!(shape.7);
}

#[test]
fn string_io_rejects_lock_and_invalid_rex2_ordering() {
    for opcode in 0x6C..=0x6F {
        for bytes in [vec![0xF0, opcode], vec![0xF0, 0xD5, 0x00, opcode]] {
            assert!(matches!(
                lift_single(&bytes),
                Err(LiftError::InvalidEncoding { .. })
            ));
        }
        assert!(matches!(
            lift_single(&[0x48, 0xD5, 0x00, opcode]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }

    for bytes in [&[0xD5][..], &[0xD5, 0x00]] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::Incomplete { .. })
        ));
    }
}

#[test]
fn string_io_non_strict_oracle_path_preserves_typed_terminal_metadata() {
    let bytes = [0xF3, 0x64, 0x67, 0x66, 0x6F];
    let mut lifter = X86_64Lifter::new();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, &bytes, &mut context)
        .expect("non-strict REP FS:OUTSW lift");

    assert_eq!(result.bytes_consumed, bytes.len());
    assert_eq!(
        exact_string_io(&result),
        (
            X86StringIoKind::Outs,
            MemWidth::B2,
            OpWidth::W32,
            true,
            X86Segment::Fs,
            0x1000,
            0x1005,
            false,
        )
    );
}

#[test]
fn string_io_canonical_interpreter_reports_exact_noncommitting_handoff() {
    let mut context = SmirContext::new_x86_64();
    context.pc = 0x1000;
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.gpr[1] = 0x1234_5678_0000_0005;
    x86.gpr[2] = 0x03F8;
    x86.gpr[6] = 0x2000;
    x86.gpr[7] = 0x3000;
    x86.rflags = 0x0000_0000_0000_0CD7;

    let mut memory = FlatMemory::with_base(0x2000, 0x2000);
    memory.write(0x2000, &[0xA5, 0x5A]).unwrap();
    let mut before_memory = [0u8; 2];
    memory.read(0x2000, &mut before_memory).unwrap();
    let before_regs = match &context.arch_regs {
        ArchRegState::X86_64(x86) => (x86.gpr, x86.rflags),
        _ => unreachable!(),
    };

    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &string_io_block(&[0xF3, 0x64, 0x67, 0x66, 0x6F]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::X86StringIo {
            kind: X86StringIoKind::Outs,
            width: MemWidth::B2,
            address_width: OpWidth::W32,
            repeated: true,
            memory_segment: X86Segment::Fs,
            fault_pc: 0x1000,
            return_pc: 0x1005,
        })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!((x86.gpr, x86.rflags), before_regs);
    let mut after_memory = [0u8; 2];
    memory.read(0x2000, &mut after_memory).unwrap();
    assert_eq!(after_memory, before_memory);
}

#[test]
fn string_io_canonical_interpreter_gates_rex2_and_rejects_non_x86_state() {
    let block = string_io_block(&[0xD5, 0x00, 0x6C]);
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
        BlockResult::Exit(ExitReason::X86StringIo {
            kind: X86StringIoKind::Ins,
            width: MemWidth::B1,
            ..
        })
    ));

    let mut aarch64 = SmirContext::new_aarch64();
    aarch64.pc = 0x1000;
    let wrong_arch =
        SmirInterpreter::new().execute_block(&mut aarch64, &mut FlatMemory::new(1), &block);
    assert!(matches!(
        wrong_arch,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
}

#[test]
fn string_io_optimizer_preserves_exact_terminal_payload() {
    let cases = [
        (&[0x6C][..], X86StringIoKind::Ins, MemWidth::B1, false),
        (
            &[0xF2, 0x67, 0x66, 0x65, 0x6F][..],
            X86StringIoKind::Outs,
            MemWidth::B2,
            false,
        ),
        (
            &[0xD5, 0x00, 0x6D][..],
            X86StringIoKind::Ins,
            MemWidth::B4,
            true,
        ),
    ];

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for &(bytes, kind, width, requires_apx) in &cases {
            let mut lifter = X86_64Lifter::strict();
            let mut context = LiftContext::new(SourceArch::X86_64);
            let mut function = lifter
                .lift_function(
                    0x1000,
                    &TestMemory::new(0x1000, bytes.to_vec()),
                    &mut context,
                )
                .expect("strict string-I/O function lift");
            optimize_function(&mut function, level);
            assert!(matches!(
                function.blocks[0].terminator,
                Terminator::Trap {
                    kind: TrapKind::X86StringIo {
                        kind: got_kind,
                        width: got_width,
                        fault_pc: 0x1000,
                        requires_apx: got_apx,
                        ..
                    }
                } if got_kind == kind && got_width == width && got_apx == requires_apx
            ));
        }
    }
}

#[test]
fn string_io_interpreter_frontier_preserves_supported_native_prefix_at_exact_pc() {
    // ADD RAX,1; REP FS:addr32 OUTSW.
    let code = vec![0x48, 0x83, 0xC0, 0x01, 0xF3, 0x64, 0x67, 0x66, 0x6F];
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter
        .lift_function(0x1800, &TestMemory::new(0x1800, code), &mut context)
        .expect("string-I/O frontier function lift");

    let prefix = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1800)
        .expect("supported prefix block");
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x1804)
        .expect("exact string-I/O frontier block");
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
            &TestMemory::new(0x2000, vec![0x6C]),
            &mut entry_context,
        )
        .expect("entry string-I/O frontier function lift");
    assert_eq!(entry.blocks.len(), 1);
    assert_eq!(entry.blocks[0].guest_pc, 0x2000);
    assert!(entry.blocks[0].ops.is_empty());
    assert!(matches!(
        entry.blocks[0].terminator,
        Terminator::Return { .. }
    ));
}
