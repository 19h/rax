//! Strict lift, metadata, optimizer, and interpreter coverage for MOV-to-CR.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::X86ControlReg;
use crate::smir::optimize::{OptLevel, optimize_function};

fn write_control_kind(src: u8, control: X86ControlReg, next_pc: u64) -> OpKind {
    OpKind::X86WriteControl {
        src: x86_gpr(src),
        control,
        next_pc,
    }
}

fn execute_write_control(
    src: u8,
    control: X86ControlReg,
    value: u64,
    configure: impl FnOnce(&mut crate::smir::ir::context::X86RegState),
) -> (BlockResult, SmirContext) {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, write_control_kind(src, control, 0x1003));
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
fn mov_to_control_register_strictly_lifts_every_register_and_extension() {
    let cases: &[(&[u8], X86ControlReg, u8)] = &[
        (&[0x0F, 0x22, 0xC0], X86ControlReg::Cr0, 0),
        (&[0x0F, 0x22, 0xD1], X86ControlReg::Cr2, 1),
        (&[0x0F, 0x22, 0xDA], X86ControlReg::Cr3, 2),
        (&[0x0F, 0x22, 0xE3], X86ControlReg::Cr4, 3),
        (&[0x44, 0x0F, 0x22, 0xC4], X86ControlReg::Cr8, 4),
        (&[0x45, 0x0F, 0x22, 0xC7], X86ControlReg::Cr8, 15),
        (&[0x49, 0x0F, 0x22, 0xE6], X86ControlReg::Cr4, 14),
    ];

    for (bytes, expected_control, expected_src) in cases {
        let result = lift_single(bytes).expect("strict MOV-to-CR lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86WriteControl {
                    src,
                    control,
                    next_pc,
                },
                guest_pc: 0x1000,
                ..
            }] if *src == x86_gpr(*expected_src)
                && control == expected_control
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }
}

#[test]
fn mov_to_control_register_ignores_mod_bits_without_consuming_an_address() {
    for modrm in [0x00, 0x40, 0x80, 0xC0] {
        let bytes = [0x0F, 0x22, modrm, 0x25, 0xAA, 0xBB, 0xCC, 0xDD];
        let result = lift_single(&bytes).expect("ModR/M.mod is ignored");
        assert_eq!(result.bytes_consumed, 3, "ModR/M={modrm:#04x}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86WriteControl {
                    src,
                    control: X86ControlReg::Cr0,
                    next_pc: 0x1003,
                },
                ..
            }] if *src == x86_gpr(0)
        ));
    }
}

#[test]
fn mov_to_control_register_accepts_ignored_prefixes_and_models_invalid_encodings() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // neutral REX and ignored REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        let bytes = [prefix, 0x0F, 0x22, 0xC0];
        let result = lift_single(&bytes).expect("ignored MOV-to-CR prefix");
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:#04x}");
        assert!(matches!(
            result.ops[0].kind,
            OpKind::X86WriteControl {
                control: X86ControlReg::Cr0,
                next_pc: 0x1004,
                ..
            }
        ));
    }

    for bytes in [
        &[0x0F, 0x22, 0xC8][..],   // CR1
        &[0x0F, 0x22, 0xE8],       // CR5
        &[0x0F, 0x22, 0xF0],       // CR6
        &[0x0F, 0x22, 0xF8],       // CR7
        &[0x44, 0x0F, 0x22, 0xC8], // CR9
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

    for bytes in [&[0xF0, 0x0F, 0x22, 0xC0][..], &[0xD5, 0x80, 0x22, 0xC0]] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn mov_to_control_register_metadata_is_source_stateful_and_jit_safe() {
    let op = write_control_kind(7, X86ControlReg::Cr3, 0x1003);
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
fn mov_to_control_register_interpreter_commits_normalized_state_and_preserves_flags() {
    let cases = [
        (X86ControlReg::Cr0, 1 | (1 << 6), (1 << 4) | 1),
        (
            X86ControlReg::Cr2,
            0xAAAA_BBBB_CCCC_DDDD,
            0xAAAA_BBBB_CCCC_DDDD,
        ),
        (
            X86ControlReg::Cr3,
            0x0000_1234_5678_9FFF,
            0x0000_1234_5678_9018,
        ),
        (
            X86ControlReg::Cr4,
            (1 << 5) | (1 << 18) | (1 << 21),
            (1 << 5) | (1 << 18) | (1 << 21),
        ),
        (X86ControlReg::Cr8, 0xF, 0xF),
    ];

    for (index, (control, value, expected)) in cases.into_iter().enumerate() {
        let (result, context) = execute_write_control(index as u8, control, value, |x86| {
            x86.cr0 = 1 | (1 << 4);
            x86.cr4 = 1 << 5;
            x86.cpl = 0;
            x86.rflags = 0x0004_0ED7;
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        let actual = match control {
            X86ControlReg::Cr0 => x86.cr0,
            X86ControlReg::Cr2 => x86.cr2,
            X86ControlReg::Cr3 => x86.cr3,
            X86ControlReg::Cr4 => x86.cr4,
            X86ControlReg::Cr8 => x86.cr8,
        };
        assert_eq!(actual, expected, "{control:?}");
        assert_eq!(x86.rflags, 0x0004_0ED7, "{control:?}: RFLAGS");
    }

    let (result, context) = execute_write_control(
        0,
        X86ControlReg::Cr3,
        (1 << 63) | 0x0000_1234_5678_9ABC,
        |x86| {
            x86.cr0 = 1;
            x86.cr4 = (1 << 5) | (1 << 17);
            x86.efer = 1 << 10;
        },
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr3, 0x0000_1234_5678_9ABC);
}

#[test]
fn mov_to_control_register_interpreter_faults_are_dynamic_and_noncommitting() {
    let sentinel = 0x2222_3333_4444_5555;
    let (privilege, context) = execute_write_control(0, X86ControlReg::Cr2, 0xAAAA, |x86| {
        x86.cr0 = 1;
        x86.cpl = 3;
        x86.cr2 = sentinel;
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
    assert_eq!(x86.cr2, sentinel);

    for (control, value) in [
        (X86ControlReg::Cr0, 1 << 31),
        (X86ControlReg::Cr3, 1 << 48),
        (X86ControlReg::Cr4, 1 << 15),
        (X86ControlReg::Cr8, 0x10),
    ] {
        let (result, context) = execute_write_control(0, control, value, |x86| {
            x86.cr0 = 1 | (1 << 4);
            x86.cr2 = sentinel;
            x86.cr3 = sentinel & !0xFFF;
            x86.cr4 = 1 << 5;
            x86.cr8 = 7;
            x86.efer = 0;
            x86.cpl = 0;
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
        assert_eq!(x86.cr0, 1 | (1 << 4), "{control:?}: CR0");
        assert_eq!(x86.cr2, sentinel, "{control:?}: CR2");
        assert_eq!(x86.cr3, sentinel & !0xFFF, "{control:?}: CR3");
        assert_eq!(x86.cr4, 1 << 5, "{control:?}: CR4");
        assert_eq!(x86.cr8, 7, "{control:?}: CR8");
    }

    let (real_mode, context) = execute_write_control(0, X86ControlReg::Cr2, 0xAAAA, |x86| {
        x86.cr0 = 0;
        x86.cpl = 3;
        x86.cr2 = sentinel;
    });
    assert!(matches!(real_mode, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr2, 0xAAAA);
}

#[test]
fn mov_to_control_register_interpreter_models_ia32e_transitions() {
    let (enter, context) = execute_write_control(0, X86ControlReg::Cr0, (1 << 31) | 1, |x86| {
        x86.cr0 = 1;
        x86.cr4 = 1 << 5;
        x86.efer = 1 << 8;
        x86.cs_l = false;
        x86.tr_type = 9;
    });
    assert!(matches!(enter, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_ne!(x86.efer & (1 << 10), 0);

    let (leave, context) = execute_write_control(0, X86ControlReg::Cr0, 1, |x86| {
        x86.cr0 = (1 << 31) | (1 << 4) | 1;
        x86.cr4 = 1 << 5;
        x86.efer = (1 << 10) | (1 << 8);
        x86.cs_l = false;
        x86.tr_type = 9;
    });
    assert!(matches!(leave, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.efer & (1 << 10), 0);

    for (name, cs_l, tr_type) in [("64-bit CS", true, 9), ("16-bit TSS", false, 3)] {
        let (fault, context) = execute_write_control(0, X86ControlReg::Cr0, (1 << 31) | 1, |x86| {
            x86.cr0 = 1;
            x86.cr4 = 1 << 5;
            x86.efer = 1 << 8;
            x86.cs_l = cs_l;
            x86.tr_type = tr_type;
        });
        assert!(
            matches!(
                fault,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0
                })
            ),
            "{name}"
        );
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.cr0, 1, "{name}");
        assert_eq!(x86.efer, 1 << 8, "{name}");
    }
}

#[test]
fn mov_to_control_register_survives_o2_as_stateful_serializing_writes() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, write_control_kind(0, X86ControlReg::Cr2, 0x1003));
    builder.push_op(0x1003, write_control_kind(1, X86ControlReg::Cr8, 0x1007));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86WriteControl { .. }))
            .count(),
        2,
        "state writes, dynamic faults, serialization, and handoff prohibit elimination"
    );
}
