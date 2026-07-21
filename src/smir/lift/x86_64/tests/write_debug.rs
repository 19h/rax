//! Strict lift, metadata, optimizer, and interpreter coverage for MOV-to-DR.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::X86DebugReg;
use crate::smir::optimize::{OptLevel, optimize_function};

fn write_debug_kind(src: u8, debug: X86DebugReg) -> OpKind {
    OpKind::X86WriteDebug {
        src: x86_gpr(src),
        debug,
    }
}

fn execute_write_debug(
    src: u8,
    debug: X86DebugReg,
    value: u64,
    configure: impl FnOnce(&mut crate::smir::ir::context::X86RegState),
) -> (BlockResult, SmirContext) {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, write_debug_kind(src, debug));
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    configure(x86);
    x86.gpr[usize::from(src)] = value;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    (result, context)
}

#[test]
fn mov_to_debug_register_strictly_lifts_every_selector_and_gpr_extension() {
    let cases: &[(&[u8], X86DebugReg, u8)] = &[
        (&[0x0F, 0x23, 0xC0], X86DebugReg::Dr0, 0),
        (&[0x0F, 0x23, 0xC9], X86DebugReg::Dr1, 1),
        (&[0x0F, 0x23, 0xD2], X86DebugReg::Dr2, 2),
        (&[0x0F, 0x23, 0xDB], X86DebugReg::Dr3, 3),
        (&[0x0F, 0x23, 0xE4], X86DebugReg::Dr4, 4),
        (&[0x0F, 0x23, 0xED], X86DebugReg::Dr5, 5),
        (&[0x0F, 0x23, 0xF6], X86DebugReg::Dr6, 6),
        (&[0x0F, 0x23, 0xFF], X86DebugReg::Dr7, 7),
        (&[0x41, 0x0F, 0x23, 0xC7], X86DebugReg::Dr0, 15),
        (&[0x49, 0x0F, 0x23, 0xFE], X86DebugReg::Dr7, 14),
        (&[0xD5, 0x90, 0x23, 0xC0], X86DebugReg::Dr0, 16),
        (&[0xD5, 0x91, 0x23, 0xFF], X86DebugReg::Dr7, 31),
    ];

    for (bytes, expected_debug, expected_src) in cases {
        let result = lift_single(bytes).expect("strict MOV-to-DR lift");
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
                kind: OpKind::X86WriteDebug { src, debug },
                guest_pc: 0x1000,
                ..
            }] if *src == x86_gpr(*expected_src) && debug == expected_debug
        ));
    }
}

#[test]
fn mov_to_debug_register_ignores_mod_bits_without_consuming_an_address() {
    for modrm in [0x00, 0x40, 0x80, 0xC0] {
        let bytes = [0x0F, 0x23, modrm, 0x25, 0xAA, 0xBB, 0xCC, 0xDD];
        let result = lift_single(&bytes).expect("ModR/M.mod is ignored");
        assert_eq!(result.bytes_consumed, 3, "ModR/M={modrm:#04x}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86WriteDebug {
                    src,
                    debug: X86DebugReg::Dr0,
                },
                ..
            }] if *src == x86_gpr(0)
        ));
    }
}

#[test]
fn mov_to_debug_register_accepts_ignored_prefixes_and_traps_invalid_extensions() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // neutral REX and ignored REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        let bytes = [prefix, 0x0F, 0x23, 0xC0];
        let result = lift_single(&bytes).expect("ignored MOV-to-DR prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:#04x}");
        assert!(matches!(
            result.ops[0].kind,
            OpKind::X86WriteDebug {
                debug: X86DebugReg::Dr0,
                ..
            }
        ));
    }

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x23, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    for bytes in [
        &[0x44, 0x0F, 0x23, 0xC0][..], // REX.R selects DR8
        &[0xD5, 0x84, 0x23, 0xC0],     // REX2.R3 selects DR8
        &[0xD5, 0xC0, 0x23, 0xC0],     // REX2.R4 selects DR16
    ] {
        let result = lift_single(bytes).expect("nonexistent DR has explicit #UD trap");
        assert_invalid_opcode_trap(&result, bytes.len());
    }
}

#[test]
fn mov_to_debug_register_metadata_is_source_stateful_and_jit_safe() {
    let op = write_debug_kind(7, X86DebugReg::Dr3);
    assert_eq!(op.source_vregs(), vec![x86_gpr(7)]);
    assert!(op.dests().is_empty());
    assert!(op.flags_read().is_empty());
    assert!(op.flags_written().is_empty());
    assert!(op.has_side_effects());
    assert!(!op.reads_memory());
    assert!(!op.writes_memory());
    assert!(op.is_jit_safe());
    assert!(SmirOp::new(OpId(0), 0x1000, op).is_jit_safe());
}

#[test]
fn mov_to_debug_register_interpreter_writes_exact_state_aliases_and_preserves_flags() {
    let cases = [
        (X86DebugReg::Dr0, 0x0000_1111_2222_3333),
        (X86DebugReg::Dr1, 0x0000_2222_3333_4444),
        (X86DebugReg::Dr2, 0x0000_3333_4444_5555),
        (X86DebugReg::Dr3, 0x0000_4444_5555_6666),
        (X86DebugReg::Dr4, 0x0000_0000_FFFF_0FF0),
        (X86DebugReg::Dr5, 0x0000_0000_0000_0400),
        (X86DebugReg::Dr6, 0x0000_0000_FFFF_0FF0),
        (X86DebugReg::Dr7, 0x0000_0000_0000_0400),
    ];
    for (index, (debug, value)) in cases.into_iter().enumerate() {
        let (result, context) = execute_write_debug(index as u8, debug, value, |x86| {
            x86.cr0 = 1;
            x86.cpl = 0;
            x86.cr4 = 0;
            x86.dr6 = 0x400;
            x86.dr7 = 0x400;
            x86.rflags = 0x0004_0ED7;
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        let actual = match debug {
            X86DebugReg::Dr0 => x86.dr0,
            X86DebugReg::Dr1 => x86.dr1,
            X86DebugReg::Dr2 => x86.dr2,
            X86DebugReg::Dr3 => x86.dr3,
            X86DebugReg::Dr4 | X86DebugReg::Dr6 => x86.dr6,
            X86DebugReg::Dr5 | X86DebugReg::Dr7 => x86.dr7,
        };
        assert_eq!(actual, value, "{debug:?}");
        assert_eq!(x86.rflags, 0x0004_0ED7, "{debug:?}: deterministic flags");
    }
}

#[test]
fn mov_to_debug_register_interpreter_models_fault_priority_and_noncommit() {
    let sentinel = 0x400;
    let high = 0x0000_0001_0000_0000;

    let (general_detect, context) = execute_write_debug(0, X86DebugReg::Dr4, high, |x86| {
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.cr4 = 1 << 3;
        x86.dr6 = sentinel;
        x86.dr7 = 1 << 13;
    });
    assert!(matches!(
        general_detect,
        BlockResult::Exit(ExitReason::Debug { addr: 0x1000 })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_ne!(x86.dr6 & (1 << 13), 0, "DR6.BD is set before #DB");
    assert_eq!(x86.dr6 & !(1 << 13), sentinel);
    assert_ne!(
        x86.dr7 & (1 << 13),
        0,
        "GD clears only when the #DB handler is entered"
    );

    let (de, context) = execute_write_debug(0, X86DebugReg::Dr4, high, |x86| {
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.cr4 = 1 << 3;
        x86.dr6 = sentinel;
    });
    assert!(matches!(
        de,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.dr6, sentinel);

    let (privilege, context) = execute_write_debug(0, X86DebugReg::Dr2, 0x2222, |x86| {
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.dr2 = sentinel;
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
    assert_eq!(x86.dr2, sentinel);

    let (real_mode, context) = execute_write_debug(0, X86DebugReg::Dr2, 0x2222, |x86| {
        x86.cr0 = 0;
        x86.cpl = 3;
        x86.dr2 = sentinel;
    });
    assert!(matches!(real_mode, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.dr2, 0x2222);
}

#[test]
fn mov_to_debug_register_interpreter_rejects_alias_high_halves_only() {
    let high = 0x0000_0001_0000_0000;
    for debug in [
        X86DebugReg::Dr4,
        X86DebugReg::Dr5,
        X86DebugReg::Dr6,
        X86DebugReg::Dr7,
    ] {
        let (result, context) = execute_write_debug(0, debug, high, |x86| {
            x86.cr0 = 1;
            x86.cpl = 0;
            x86.cr4 = 0;
            x86.dr6 = 0x400;
            x86.dr7 = 0x400;
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
        assert_eq!(x86.dr6, 0x400, "{debug:?}");
        assert_eq!(x86.dr7, 0x400, "{debug:?}");
    }

    let (result, context) = execute_write_debug(0, X86DebugReg::Dr0, high, |x86| {
        x86.cr0 = 1;
        x86.cpl = 0;
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.dr0, high);
}

#[test]
fn mov_to_debug_register_survives_o2_as_a_stateful_serializing_write() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, write_debug_kind(0, X86DebugReg::Dr0));
    builder.push_op(0x1003, write_debug_kind(1, X86DebugReg::Dr7));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86WriteDebug { .. }))
            .count(),
        2,
        "state writes, dynamic faults, and serialization prohibit elimination"
    );
}
