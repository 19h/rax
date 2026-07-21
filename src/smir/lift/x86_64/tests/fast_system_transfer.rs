//! Strict lift, metadata, optimizer, and canonical-interpreter coverage for
//! Intel SYSENTER/SYSEXIT.

use super::*;
use crate::isa::x86_64::flags;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::{X86FastSystemTransferKind, X86FastSystemTransferOp};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_transfer(result: &LiftResult) -> &X86FastSystemTransferOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86FastSystemTransfer(transfer) => transfer,
        other => panic!("expected one exact X86FastSystemTransfer op, got {other:?}"),
    }
}

fn transfer_block(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).expect("strict fast-system-transfer lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute(bytes: &[u8], configure: impl FnOnce(&mut SmirContext)) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &transfer_block(bytes),
    );
    (result, context)
}

fn configure_common(context: &mut SmirContext) {
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cpl = 0;
    x86.rflags = 0x2
        | flags::bits::CF
        | flags::bits::DF
        | flags::bits::IF
        | flags::bits::VM
        | flags::bits::VIF
        | flags::bits::VIP
        | flags::bits::IOPL_MASK;
    x86.sysenter_cs = 8;
    x86.sysenter_esp = 0xFFFF_8000_0000_4000;
    x86.sysenter_eip = 0xFFFF_8000_0000_2000;
    x86.gpr[1] = 0xFFFF_8000_0000_8000;
    x86.gpr[2] = 0xFFFF_8000_0000_6000;
    x86.gpr[4] = 0xDEAD_BEEF_DEAD_BEEF;
    x86.rip = 0x1000;
    x86.cs_selector = 0x77;
    x86.ss_selector = 0x7F;
    x86.cs_cache.base = 0x1111;
    x86.ss_cache.base = 0x2222;
    context.flags.materialized = MaterializedFlags {
        cf: true,
        df: true,
        ..Default::default()
    };
    context.flags.set_lazy_add(u64::MAX, 1, 0, OpWidth::W64);
}

#[test]
fn sysenter_sysexit_strictly_lift_exact_dynamic_transfers_and_rex_w() {
    for (bytes, kind, operand64) in [
        (
            &[0x0F, 0x34][..],
            X86FastSystemTransferKind::Sysenter,
            false,
        ),
        (
            &[0x48, 0x0F, 0x34],
            X86FastSystemTransferKind::Sysenter,
            false,
        ),
        (&[0x0F, 0x35], X86FastSystemTransferKind::Sysexit, false),
        (
            &[0x48, 0x0F, 0x35],
            X86FastSystemTransferKind::Sysexit,
            true,
        ),
    ] {
        let result = lift_single(bytes).expect("strict SYSENTER/SYSEXIT lift");
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.branch_targets.is_empty());
        let transfer = exact_transfer(&result);
        assert_eq!(transfer.kind, kind);
        assert_eq!(transfer.target, VReg::Arch(ArchReg::X86(X86Reg::Rip)));
        assert_eq!(transfer.stack_pointer, x86_gpr(4));
        assert_eq!(transfer.return_target, x86_gpr(2));
        assert_eq!(transfer.return_stack_pointer, x86_gpr(1));
        assert_eq!(transfer.operand64, operand64);
        assert_eq!(transfer.next_pc, 0x1000 + bytes.len() as u64);
        assert!(matches!(
            result.control_flow,
            ControlFlow::IndirectBranch { target }
                if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
        ));
    }
}

#[test]
fn sysenter_sysexit_prefix_space_is_exact_and_rex2_row_is_reserved() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, 0xF2, 0xF3, // size/repeat prefixes
        0x40, 0x41, 0x42, 0x44, 0x47, // non-W REX payloads
    ] {
        for opcode in [0x34, 0x35] {
            let bytes = [prefix, 0x0F, opcode];
            let result = lift_single(&bytes).expect("architecturally ignored prefix");
            assert_eq!(result.bytes_consumed, 3, "{bytes:02X?}");
            assert!(!exact_transfer(&result).operand64, "{bytes:02X?}");
        }
    }
    for rex_w in [0x48, 0x49, 0x4F] {
        assert!(!exact_transfer(&lift_single(&[rex_w, 0x0F, 0x34]).unwrap()).operand64);
        assert!(exact_transfer(&lift_single(&[rex_w, 0x0F, 0x35]).unwrap()).operand64);
    }
    for opcode in [0x34, 0x35] {
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, opcode]),
            Err(LiftError::InvalidEncoding { .. })
        ));
        for payload in 0x80_u8..=u8::MAX {
            let result = lift_single(&[0xD5, payload, opcode]).expect("reserved REX2 map-1 row");
            assert_invalid_opcode_trap(&result, 3);
        }
    }
}

#[test]
fn fast_system_transfer_survives_interpreter_frontier_mode_and_has_exact_metadata() {
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    for (address, bytes) in [
        (0x1800, &[0x0F, 0x34][..]),
        (0x1900, &[0x48, 0x0F, 0x35][..]),
    ] {
        let mut context = LiftContext::new(SourceArch::X86_64);
        let function = lifter
            .lift_function(
                address,
                &TestMemory::new(address, bytes.to_vec()),
                &mut context,
            )
            .expect("typed dynamic transition must remain native-visible");
        assert_eq!(function.blocks.len(), 1);
        assert!(matches!(
            function.blocks[0].ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86FastSystemTransfer(_),
                ..
            }]
        ));
        assert!(matches!(
            function.blocks[0].terminator,
            Terminator::IndirectBranch { target, ref possible_targets }
                if target == VReg::Arch(ArchReg::X86(X86Reg::Rip))
                    && possible_targets.is_empty()
        ));
    }

    let sysenter = lift_single(&[0x0F, 0x34]).unwrap().ops.remove(0);
    assert!(sysenter.kind.source_vregs().is_empty());
    assert_eq!(
        sysenter.kind.dests(),
        vec![
            VReg::Arch(ArchReg::X86(X86Reg::Rip)),
            VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
        ]
    );
    let sysexit = lift_single(&[0x48, 0x0F, 0x35]).unwrap().ops.remove(0);
    assert_eq!(sysexit.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(2)]);
    for op in [sysenter, sysexit] {
        assert!(op.kind.flags_read().is_empty());
        assert!(op.kind.flags_written().is_empty());
        assert!(op.kind.has_side_effects());
        assert!(!op.kind.reads_memory());
        assert!(!op.kind.writes_memory());
        assert!(op.is_jit_safe());
    }
}

#[test]
fn sysenter_interpreter_commits_intel_ia32e_fixed_segments_and_clears_if_vm() {
    let (result, context) = execute(&[0x0F, 0x34], configure_common);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.rip, 0xFFFF_8000_0000_2000);
    assert_eq!(x86.gpr[4], 0xFFFF_8000_0000_4000);
    assert_eq!(x86.cpl, 0);
    assert_eq!(x86.cs_selector, 8);
    assert_eq!(x86.ss_selector, 16);
    assert_eq!(x86.rflags & (flags::bits::IF | flags::bits::VM), 0);
    assert_ne!(x86.rflags & flags::bits::IOPL_MASK, 0);
    assert_ne!(x86.rflags & flags::bits::VIF, 0);
    assert_ne!(x86.rflags & flags::bits::VIP, 0);
    assert_eq!(x86.cs_cache.base, 0);
    assert_eq!(x86.cs_cache.limit, 0xF_FFFF);
    assert_eq!(x86.cs_cache.type_, 0x0B);
    assert!(x86.cs_cache.present && x86.cs_cache.s && x86.cs_cache.l && x86.cs_cache.g);
    assert!(!x86.cs_cache.db);
    assert!(!x86.cs_cache.unusable);
    assert_eq!(x86.ss_cache.type_, 0x03);
    assert!(x86.ss_cache.present && x86.ss_cache.s && x86.ss_cache.db && x86.ss_cache.g);
    assert!(!x86.ss_cache.l);
    assert!(!x86.ss_cache.unusable);
    assert!(context.flags.lazy.is_some());
    assert!(context.flags.materialized.cf && context.flags.materialized.df);
}

#[test]
fn sysexit_interpreter_distinguishes_32_and_64_bit_return_forms() {
    for (bytes, rip, rsp, cs, ss, long, default_big) in [
        (
            &[0x0F, 0x35][..],
            0x0000_6000,
            0x0000_8000,
            0x1B,
            0x23,
            false,
            true,
        ),
        (
            &[0x48, 0x0F, 0x35][..],
            0xFFFF_8000_0000_6000,
            0xFFFF_8000_0000_8000,
            0x2B,
            0x33,
            true,
            false,
        ),
    ] {
        let (result, context) = execute(bytes, |context| {
            configure_common(context);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.rflags &= !flags::bits::VM;
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rip, rip);
        assert_eq!(x86.gpr[4], rsp);
        assert_eq!(x86.cpl, 3);
        assert_eq!(x86.cs_selector, cs);
        assert_eq!(x86.ss_selector, ss);
        assert_eq!(x86.cs_l, long);
        assert_eq!(x86.cs_cache.l, long);
        assert_eq!(x86.cs_cache.db, default_big);
        assert_eq!(x86.cs_cache.dpl, 3);
        assert_eq!(x86.ss_cache.dpl, 3);
        assert_ne!(x86.rflags & flags::bits::IF, 0);
        assert_ne!(x86.rflags & flags::bits::VIF, 0);
    }
}

#[test]
fn fast_system_transfer_faults_and_malformed_ir_are_precise_and_noncommitting() {
    for (name, bytes, configure) in [
        (
            "SYSENTER PE=0",
            &[0x0F, 0x34][..],
            (|context: &mut SmirContext| {
                configure_common(context);
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cr0 = 0;
            }) as fn(&mut SmirContext),
        ),
        ("SYSENTER null CS", &[0x0F, 0x34], |context| {
            configure_common(context);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.sysenter_cs = 3;
        }),
        ("SYSENTER noncanonical EIP", &[0x0F, 0x34], |context| {
            configure_common(context);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.sysenter_eip = 0x0000_8000_0000_0000;
        }),
        ("SYSEXIT CPL3", &[0x48, 0x0F, 0x35], |context| {
            configure_common(context);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cpl = 3;
        }),
        (
            "SYSEXITQ noncanonical RSP",
            &[0x48, 0x0F, 0x35],
            |context| {
                configure_common(context);
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.gpr[1] = 0x0000_8000_0000_0000;
            },
        ),
    ] {
        let (result, context) = execute(bytes, configure);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::GeneralProtection {
                addr: 0x1000,
                error_code: 0
            })
        ));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rip, 0x1000, "{name}");
        assert_eq!(x86.gpr[4], 0xDEAD_BEEF_DEAD_BEEF, "{name}");
        assert_eq!(x86.cs_selector, 0x77, "{name}");
        assert_eq!(x86.ss_selector, 0x7F, "{name}");
        assert_eq!(x86.cs_cache.base, 0x1111, "{name}");
        assert_eq!(x86.ss_cache.base, 0x2222, "{name}");
        assert!(context.flags.lazy.is_some(), "{name}");
    }

    let mut malformed = transfer_block(&[0x0F, 0x34]);
    let OpKind::X86FastSystemTransfer(transfer) = &mut malformed.ops[0].kind else {
        unreachable!()
    };
    transfer.stack_pointer = x86_gpr(5);
    let mut context = SmirContext::new_x86_64();
    configure_common(&mut context);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &malformed),
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));

    let mut non_x86 = SmirContext::new_aarch64();
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut non_x86,
            &mut FlatMemory::new(1),
            &transfer_block(&[0x0F, 0x34])
        ),
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
}

#[test]
fn fast_system_transfer_o2_preserves_order_and_terminal_effect() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86FastSystemTransfer(X86FastSystemTransferOp {
            kind: X86FastSystemTransferKind::Sysenter,
            target: VReg::Arch(ArchReg::X86(X86Reg::Rip)),
            stack_pointer: x86_gpr(4),
            return_target: x86_gpr(2),
            return_stack_pointer: x86_gpr(1),
            operand64: false,
            next_pc: 0x1002,
        }),
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let original = builder.finish();
    let mut optimized = original.clone();
    optimize_function(&mut optimized, OptLevel::O2);
    assert!(matches!(
        optimized.entry_block().unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86FastSystemTransfer(_),
            ..
        }]
    ));

    for function in [&original, &optimized] {
        let mut context = SmirContext::new_x86_64();
        configure_common(&mut context);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            function.entry_block().unwrap(),
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.rip, 0xFFFF_8000_0000_2000);
        assert_eq!(x86.gpr[4], 0xFFFF_8000_0000_4000);
    }
}
