//! Strict lift, metadata, optimizer, and interpreter coverage for MOV-from-DR.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::X86DebugReg;
use crate::smir::optimize::{OptLevel, optimize_function};

fn read_debug_kind(dst: u8, debug: X86DebugReg) -> OpKind {
    OpKind::X86ReadDebug {
        dst: x86_gpr(dst),
        debug,
    }
}

fn execute_read_debug(
    dst: u8,
    debug: X86DebugReg,
    configure: impl FnOnce(&mut crate::smir::ir::context::X86RegState),
) -> (BlockResult, SmirContext) {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, read_debug_kind(dst, debug));
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    configure(x86);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    (result, context)
}

#[test]
fn mov_from_debug_register_strictly_lifts_every_selector_and_gpr_extension() {
    let cases: &[(&[u8], X86DebugReg, u8)] = &[
        (&[0x0F, 0x21, 0xC0], X86DebugReg::Dr0, 0),
        (&[0x0F, 0x21, 0xC9], X86DebugReg::Dr1, 1),
        (&[0x0F, 0x21, 0xD2], X86DebugReg::Dr2, 2),
        (&[0x0F, 0x21, 0xDB], X86DebugReg::Dr3, 3),
        (&[0x0F, 0x21, 0xE4], X86DebugReg::Dr4, 4),
        (&[0x0F, 0x21, 0xED], X86DebugReg::Dr5, 5),
        (&[0x0F, 0x21, 0xF6], X86DebugReg::Dr6, 6),
        (&[0x0F, 0x21, 0xFF], X86DebugReg::Dr7, 7),
        (&[0x41, 0x0F, 0x21, 0xC7], X86DebugReg::Dr0, 15),
        (&[0x49, 0x0F, 0x21, 0xFE], X86DebugReg::Dr7, 14),
        (&[0xD5, 0x90, 0x21, 0xC0], X86DebugReg::Dr0, 16),
        (&[0xD5, 0x91, 0x21, 0xFF], X86DebugReg::Dr7, 31),
    ];

    for (bytes, expected_debug, expected_dst) in cases {
        let result = lift_single(bytes).expect("strict MOV-from-DR lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        let ops = if bytes[0] == 0xD5 {
            assert_rex2_guarded_ops(&result, 1)
        } else {
            result.ops.as_slice()
        };
        assert!(matches!(
            ops,
            [SmirOp {
                kind: OpKind::X86ReadDebug { dst, debug },
                guest_pc: 0x1000,
                ..
            }] if *dst == x86_gpr(*expected_dst) && debug == expected_debug
        ));
    }
}

#[test]
fn mov_from_debug_register_ignores_mod_bits_without_consuming_an_address() {
    for modrm in [0x00, 0x40, 0x80, 0xC0] {
        let bytes = [0x0F, 0x21, modrm, 0x25, 0xAA, 0xBB, 0xCC, 0xDD];
        let result = lift_single(&bytes).expect("ModR/M.mod is ignored");
        assert_eq!(result.bytes_consumed, 3, "ModR/M={modrm:#04x}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86ReadDebug {
                    dst,
                    debug: X86DebugReg::Dr0,
                },
                ..
            }] if *dst == x86_gpr(0)
        ));
    }
}

#[test]
fn mov_from_debug_register_accepts_ignored_prefixes_and_traps_invalid_extensions() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // neutral REX and ignored REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        let bytes = [prefix, 0x0F, 0x21, 0xC0];
        let result = lift_single(&bytes).expect("ignored MOV-from-DR prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:#04x}");
        assert!(matches!(
            result.ops[0].kind,
            OpKind::X86ReadDebug {
                debug: X86DebugReg::Dr0,
                ..
            }
        ));
    }

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x21, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    for bytes in [
        &[0x44, 0x0F, 0x21, 0xC0][..], // REX.R selects DR8
        &[0xD5, 0x84, 0x21, 0xC0],     // REX2.R3 selects DR8
        &[0xD5, 0xC0, 0x21, 0xC0],     // REX2.R4 selects DR16
    ] {
        let result = lift_single(bytes).expect("nonexistent DR has explicit #UD trap");
        assert_invalid_opcode_trap(&result, bytes.len());
    }
}

#[test]
fn mov_from_debug_register_metadata_is_implicit_stateful_and_jit_safe() {
    let op = read_debug_kind(7, X86DebugReg::Dr3);
    assert!(op.source_vregs().is_empty());
    assert_eq!(op.dests(), vec![x86_gpr(7)]);
    assert!(op.flags_read().is_empty());
    assert!(op.flags_written().is_empty());
    assert!(op.has_side_effects());
    assert!(!op.reads_memory());
    assert!(!op.writes_memory());
    assert!(op.is_jit_safe());
    assert!(SmirOp::new(OpId(0), 0x1000, op).is_jit_safe());
}

#[test]
fn mov_from_debug_register_interpreter_reads_exact_state_aliases_and_preserves_flags() {
    let values = [
        (X86DebugReg::Dr0, 0x0000_1111_2222_3333),
        (X86DebugReg::Dr1, 0x0000_2222_3333_4444),
        (X86DebugReg::Dr2, 0x0000_3333_4444_5555),
        (X86DebugReg::Dr3, 0x0000_4444_5555_6666),
        (X86DebugReg::Dr4, 0xFFFF_0FF0),
        (X86DebugReg::Dr5, 0x400),
        (X86DebugReg::Dr6, 0xFFFF_0FF0),
        (X86DebugReg::Dr7, 0x400),
    ];
    for (index, (debug, expected)) in values.into_iter().enumerate() {
        let dst = index as u8;
        let (result, context) = execute_read_debug(dst, debug, |x86| {
            x86.cr0 = 1;
            x86.cpl = 0;
            x86.cr4 = 0;
            x86.dr0 = 0x0000_1111_2222_3333;
            x86.dr1 = 0x0000_2222_3333_4444;
            x86.dr2 = 0x0000_3333_4444_5555;
            x86.dr3 = 0x0000_4444_5555_6666;
            x86.dr6 = 0xFFFF_0FF0;
            x86.dr7 = 0x400;
            x86.rflags = 0x0004_0ED7;
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[usize::from(dst)], expected, "{debug:?}");
        assert_eq!(x86.rflags, 0x0004_0ED7, "{debug:?}: deterministic flags");
    }
}

#[test]
fn mov_from_debug_register_interpreter_models_de_cpl_and_general_detect_faults() {
    let sentinel = 0xA5A5_5A5A_DEAD_BEEF;

    let (de, context) = execute_read_debug(3, X86DebugReg::Dr4, |x86| {
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.cr4 = 1 << 3;
        x86.gpr[3] = sentinel;
    });
    assert!(matches!(
        de,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[3], sentinel);

    let (privilege, context) = execute_read_debug(3, X86DebugReg::Dr2, |x86| {
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.gpr[3] = sentinel;
    });
    assert!(matches!(
        privilege,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[3], sentinel);

    let (general_detect, context) = execute_read_debug(3, X86DebugReg::Dr4, |x86| {
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.cr4 = 1 << 3;
        x86.dr6 = 0x400;
        x86.dr7 = 1 << 13;
        x86.gpr[3] = sentinel;
    });
    assert!(matches!(
        general_detect,
        BlockResult::Exit(ExitReason::Debug { addr: 0x1000 })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[3], sentinel);
    assert_ne!(x86.dr6 & (1 << 13), 0, "DR6.BD is set before #DB");
    assert_ne!(
        x86.dr7 & (1 << 13),
        0,
        "GD clears only when the #DB handler is entered"
    );

    let (real_mode, context) = execute_read_debug(3, X86DebugReg::Dr2, |x86| {
        x86.cr0 = 0;
        x86.cpl = 3;
        x86.dr2 = 0x2222;
        x86.gpr[3] = sentinel;
    });
    assert!(matches!(real_mode, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[3], 0x2222);
}

#[test]
fn mov_from_debug_register_survives_o2_when_its_destination_is_dead() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, read_debug_kind(0, X86DebugReg::Dr0));
    builder.push_op(0x1003, read_debug_kind(0, X86DebugReg::Dr7));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86ReadDebug { .. }))
            .count(),
        2,
        "dynamic faults prohibit dead-read elimination"
    );
}
