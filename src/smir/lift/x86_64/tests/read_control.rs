//! Strict lift, metadata, optimizer, and interpreter coverage for MOV-from-CR.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::X86ControlReg;
use crate::smir::optimize::{OptLevel, optimize_function};

fn read_control_kind(dst: u8, control: X86ControlReg) -> OpKind {
    OpKind::X86ReadControl {
        dst: x86_gpr(dst),
        control,
    }
}

fn execute_read_control(
    dst: u8,
    control: X86ControlReg,
    configure: impl FnOnce(&mut crate::smir::ir::context::X86RegState),
) -> (BlockResult, SmirContext) {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, read_control_kind(dst, control));
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
fn mov_from_control_register_strictly_lifts_every_register_and_extension() {
    let cases: &[(&[u8], X86ControlReg, u8)] = &[
        (&[0x0F, 0x20, 0xC0], X86ControlReg::Cr0, 0),
        (&[0x0F, 0x20, 0xD1], X86ControlReg::Cr2, 1),
        (&[0x0F, 0x20, 0xDA], X86ControlReg::Cr3, 2),
        (&[0x0F, 0x20, 0xE3], X86ControlReg::Cr4, 3),
        (&[0x44, 0x0F, 0x20, 0xC4], X86ControlReg::Cr8, 4),
        (&[0x45, 0x0F, 0x20, 0xC7], X86ControlReg::Cr8, 15),
        (&[0x49, 0x0F, 0x20, 0xE6], X86ControlReg::Cr4, 14),
    ];

    for (bytes, expected_control, expected_dst) in cases {
        let result = lift_single(bytes).expect("strict MOV-from-CR lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86ReadControl { dst, control },
                guest_pc: 0x1000,
                ..
            }] if *dst == x86_gpr(*expected_dst) && control == expected_control
        ));
    }
}

#[test]
fn mov_from_control_register_ignores_mod_bits_without_consuming_an_address() {
    for modrm in [0x00, 0x40, 0x80, 0xC0] {
        let bytes = [0x0F, 0x20, modrm, 0x25, 0xAA, 0xBB, 0xCC, 0xDD];
        let result = lift_single(&bytes).expect("ModR/M.mod is ignored");
        assert_eq!(result.bytes_consumed, 3, "ModR/M={modrm:#04x}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86ReadControl {
                    dst,
                    control: X86ControlReg::Cr0,
                },
                ..
            }] if *dst == x86_gpr(0)
        ));
    }
}

#[test]
fn mov_from_control_register_ignores_non_lock_legacy_size_and_repeat_prefixes() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // neutral REX and ignored REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        let bytes = [prefix, 0x0F, 0x20, 0xC0];
        let result = lift_single(&bytes).expect("ignored MOV-from-CR prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:#04x}");
        assert!(matches!(
            result.ops[0].kind,
            OpKind::X86ReadControl {
                control: X86ControlReg::Cr0,
                ..
            }
        ));
    }
}

#[test]
fn mov_from_control_register_models_reserved_numbers_as_ud_and_rejects_lock_rex2() {
    for bytes in [
        &[0x0F, 0x20, 0xC8][..],   // CR1
        &[0x0F, 0x20, 0xE8],       // CR5
        &[0x0F, 0x20, 0xF0],       // CR6
        &[0x0F, 0x20, 0xF8],       // CR7
        &[0x44, 0x0F, 0x20, 0xC8], // CR9
    ] {
        let result = lift_single(bytes).expect("reserved CR number has explicit trap");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.is_empty());
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }

    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x20, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
    assert!(matches!(
        lift_single(&[0xD5, 0x80, 0x20, 0xC0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn mov_from_control_register_metadata_is_implicit_stateful_and_jit_safe() {
    let op = read_control_kind(7, X86ControlReg::Cr3);
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
fn mov_from_control_register_interpreter_reads_exact_state_and_preserves_flags() {
    let values = [
        (X86ControlReg::Cr0, 0x8005_0033),
        (X86ControlReg::Cr2, 0x2222_3333_4444_5555),
        (X86ControlReg::Cr3, 0x0000_1234_5000_0ABC),
        (X86ControlReg::Cr4, 0x0000_0000_0044_06F0),
        (X86ControlReg::Cr8, 0xD),
    ];
    for (index, (control, expected)) in values.into_iter().enumerate() {
        let dst = [0, 4, 5, 14, 15][index];
        let (result, context) = execute_read_control(dst, control, |x86| {
            x86.cr0 = 0x8005_0033;
            x86.cr2 = 0x2222_3333_4444_5555;
            x86.cr3 = 0x0000_1234_5000_0ABC;
            x86.cr4 = 0x0000_0000_0044_06F0;
            x86.cr8 = 0xD;
            x86.cpl = 0;
            x86.rflags = 0x0004_0ED7;
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.gpr[usize::from(dst)], expected, "{control:?}");
        assert_eq!(x86.rflags, 0x0004_0ED7, "{control:?}: RFLAGS");
    }
}

#[test]
fn mov_from_control_register_interpreter_privilege_check_is_dynamic_and_noncommitting() {
    let sentinel = 0xA5A5_5A5A_DEAD_BEEF;
    let (fault, context) = execute_read_control(3, X86ControlReg::Cr2, |x86| {
        x86.cr0 = 1;
        x86.cr2 = 0x2222;
        x86.cpl = 3;
        x86.gpr[3] = sentinel;
    });
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[3], sentinel);

    let (real_mode, context) = execute_read_control(3, X86ControlReg::Cr2, |x86| {
        x86.cr0 = 0;
        x86.cr2 = 0x2222;
        x86.cpl = 3;
        x86.gpr[3] = sentinel;
    });
    assert!(matches!(real_mode, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr[3], 0x2222);
}

#[test]
fn mov_from_control_register_survives_o2_when_its_destination_is_dead() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, read_control_kind(0, X86ControlReg::Cr0));
    builder.push_op(0x1003, read_control_kind(0, X86ControlReg::Cr8));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86ReadControl { .. }))
            .count(),
        2,
        "potential faults and serialization prohibit dead-read elimination"
    );
}
