//! Strict lift, metadata, interpretation, and optimization coverage for WAITPKG.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn exact_waitpkg(result: &LiftResult) -> &X86WaitPkgOp {
    match &result.ops.last().expect("WAITPKG operation").kind {
        OpKind::X86WaitPkg(op) => op,
        other => panic!("expected exact X86WaitPkg operation, got {other:?}"),
    }
}

fn block_for(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict WAITPKG lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

#[test]
fn every_canonical_waitpkg_register_selector_strictly_lifts() {
    for rm in 0..8 {
        let tpause_bytes = [0x66, 0x0F, 0xAE, 0xF0 | rm];
        let tpause = lift_single(&tpause_bytes).expect("canonical TPAUSE");
        assert_eq!(tpause.bytes_consumed, tpause_bytes.len());
        assert!(matches!(tpause.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_waitpkg(&tpause),
            X86WaitPkgOp::Tpause {
                control,
                deadline_low,
                deadline_high,
            } if *control == self::x86(X86Reg::gpr(rm))
                && *deadline_low == self::x86(X86Reg::Rax)
                && *deadline_high == self::x86(X86Reg::Rdx)
        ));

        let umwait_bytes = [0xF2, 0x0F, 0xAE, 0xF0 | rm];
        let umwait = lift_single(&umwait_bytes).expect("canonical UMWAIT");
        assert_eq!(umwait.bytes_consumed, umwait_bytes.len());
        assert!(matches!(
            exact_waitpkg(&umwait),
            X86WaitPkgOp::Umwait {
                control,
                deadline_low,
                deadline_high,
            } if *control == self::x86(X86Reg::gpr(rm))
                && *deadline_low == self::x86(X86Reg::Rax)
                && *deadline_high == self::x86(X86Reg::Rdx)
        ));

        let umonitor_bytes = [0xF3, 0x0F, 0xAE, 0xF0 | rm];
        let umonitor = lift_single(&umonitor_bytes).expect("canonical UMONITOR");
        assert_eq!(umonitor.bytes_consumed, umonitor_bytes.len());
        assert!(matches!(
            exact_waitpkg(&umonitor),
            X86WaitPkgOp::Umonitor {
                addr: Address::Direct(base),
                stack_segment: false,
            } if *base == self::x86(X86Reg::gpr(rm))
        ));
    }
}

#[test]
fn waitpkg_rex_and_rex2_forms_reach_every_extended_gpr_with_one_apx_guard() {
    for (mandatory, expected_wait) in [(0x66, false), (0xF2, true)] {
        for rm in 0..8 {
            let bytes = [mandatory, 0x41, 0x0F, 0xAE, 0xF0 | rm];
            let lifted = lift_single(&bytes).expect("REX WAITPKG form");
            let control = match exact_waitpkg(&lifted) {
                X86WaitPkgOp::Umwait { control, .. } if expected_wait => control,
                X86WaitPkgOp::Tpause { control, .. } if !expected_wait => control,
                other => panic!("unexpected WAITPKG kind: {other:?}"),
            };
            assert_eq!(*control, x86(X86Reg::gpr(8 + rm)));
            assert!(
                lifted
                    .ops
                    .iter()
                    .all(|op| !matches!(op.kind, OpKind::X86RequireApx))
            );
        }

        for bank in 0..2 {
            for rm in 0..8 {
                let bytes = [mandatory, 0xD5, 0x90 | bank, 0xAE, 0xF0 | rm];
                let lifted = lift_single(&bytes).expect("REX2 WAITPKG form");
                let control = match exact_waitpkg(&lifted) {
                    X86WaitPkgOp::Umwait { control, .. } if expected_wait => control,
                    X86WaitPkgOp::Tpause { control, .. } if !expected_wait => control,
                    other => panic!("unexpected WAITPKG kind: {other:?}"),
                };
                assert_eq!(*control, x86(X86Reg::gpr(16 + bank * 8 + rm)));
                assert_eq!(
                    lifted
                        .ops
                        .iter()
                        .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
                        .count(),
                    1
                );
            }
        }
    }

    for rm in 0..8 {
        let bytes = [0xF3, 0x41, 0x0F, 0xAE, 0xF0 | rm];
        let lifted = lift_single(&bytes).expect("REX UMONITOR form");
        assert!(matches!(
            exact_waitpkg(&lifted),
            X86WaitPkgOp::Umonitor {
                addr: Address::Direct(base),
                ..
            } if *base == x86(X86Reg::gpr(8 + rm))
        ));
        assert!(
            lifted
                .ops
                .iter()
                .all(|op| !matches!(op.kind, OpKind::X86RequireApx))
        );
    }

    for bank in 0..2 {
        for rm in 0..8 {
            let bytes = [0xF3, 0xD5, 0x90 | bank, 0xAE, 0xF0 | rm];
            let lifted = lift_single(&bytes).expect("REX2 UMONITOR form");
            assert!(matches!(
                exact_waitpkg(&lifted),
                X86WaitPkgOp::Umonitor {
                    addr: Address::Direct(base),
                    ..
                } if *base == x86(X86Reg::gpr(16 + bank * 8 + rm))
            ));
            assert_eq!(
                lifted
                    .ops
                    .iter()
                    .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn umonitor_lift_preserves_addr32_and_segment_semantics() {
    let addr32 = lift_single(&[0xF3, 0x67, 0x0F, 0xAE, 0xF3]).unwrap();
    assert!(matches!(
        exact_waitpkg(&addr32),
        X86WaitPkgOp::Umonitor {
            addr: Address::X86Addr32(inner),
            stack_segment: false,
        } if matches!(inner.as_ref(), Address::Direct(base) if *base == x86(X86Reg::Rbx))
    ));

    for (segment_prefix, segment) in [(0x64, X86Reg::FsBase), (0x65, X86Reg::GsBase)] {
        let lifted = lift_single(&[segment_prefix, 0xF3, 0x0F, 0xAE, 0xF3]).unwrap();
        assert!(matches!(
            exact_waitpkg(&lifted),
            X86WaitPkgOp::Umonitor {
                addr: Address::SegmentRel {
                    segment: got_segment,
                    base: Some(base),
                    index: None,
                    scale: 1,
                    disp: 0,
                },
                stack_segment: false,
            } if *got_segment == x86(segment) && *base == x86(X86Reg::Rbx)
        ));
    }

    let ss = lift_single(&[0x36, 0xF3, 0x0F, 0xAE, 0xF3]).unwrap();
    assert!(matches!(
        exact_waitpkg(&ss),
        X86WaitPkgOp::Umonitor {
            addr: Address::Direct(base),
            stack_segment: true,
        } if *base == x86(X86Reg::Rbx)
    ));
}

#[test]
fn waitpkg_metadata_tracks_memory_inputs_deadlines_flags_and_observability() {
    let monitor = lift_single(&[0x64, 0xF3, 0x0F, 0xAE, 0xF3]).unwrap();
    let monitor_op = monitor.ops.last().unwrap();
    assert_eq!(
        monitor_op.kind.source_vregs(),
        vec![x86(X86Reg::FsBase), x86(X86Reg::Rbx)]
    );
    assert!(monitor_op.kind.dests().is_empty());
    assert_eq!(monitor_op.kind.flags_written(), FlagSet::EMPTY);
    assert!(monitor_op.kind.has_side_effects());
    assert!(monitor_op.kind.reads_memory());
    assert!(!monitor_op.kind.writes_memory());
    assert!(monitor_op.is_jit_safe());

    for bytes in [&[0x66, 0x0F, 0xAE, 0xF1][..], &[0xF2, 0x0F, 0xAE, 0xF1]] {
        let wait = lift_single(bytes).unwrap();
        let wait_op = wait.ops.last().unwrap();
        assert_eq!(
            wait_op.kind.source_vregs(),
            vec![x86(X86Reg::Rcx), x86(X86Reg::Rax), x86(X86Reg::Rdx)]
        );
        assert!(wait_op.kind.dests().is_empty());
        assert_eq!(wait_op.kind.flags_written(), FlagSet::ALL_X86);
        assert!(wait_op.kind.has_side_effects());
        assert!(!wait_op.kind.reads_memory());
        assert!(!wait_op.kind.writes_memory());
        assert!(wait_op.is_jit_safe());
    }
}

#[test]
fn waitpkg_interpreter_success_probes_memory_and_clears_only_status_flags() {
    let initial_flags = MaterializedFlags {
        cf: true,
        zf: true,
        sf: true,
        of: true,
        pf: true,
        af: true,
        df: true,
        ac: true,
    };
    let mut context = SmirContext::new_x86_64();
    context.flags.materialized = initial_flags;
    context.write_vreg(x86(X86Reg::Rbx), 0x4008);
    context.write_vreg(x86(X86Reg::Rax), 0x1122_3344_5566_7788);
    context.write_vreg(x86(X86Reg::Rcx), 1);
    context.write_vreg(x86(X86Reg::Rdx), 0x8877_6655_4433_2211);
    let mut memory = FlatMemory::with_base(0x4000, 0x10);
    memory.load(8, &[0xA5]);

    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &block_for(&[0xF3, 0x0F, 0xAE, 0xF3]),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(
        context.flags.materialized.to_rflags(),
        initial_flags.to_rflags()
    );

    for bytes in [&[0x66, 0x0F, 0xAE, 0xF1][..], &[0xF2, 0x0F, 0xAE, 0xF1]] {
        context.flags.materialized = initial_flags;
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &block_for(bytes)),
            BlockResult::Exit(ExitReason::Halt)
        ));
        assert!(!context.flags.materialized.cf);
        assert!(!context.flags.materialized.pf);
        assert!(!context.flags.materialized.af);
        assert!(!context.flags.materialized.zf);
        assert!(!context.flags.materialized.sf);
        assert!(!context.flags.materialized.of);
        assert!(context.flags.materialized.df);
        assert!(context.flags.materialized.ac);
        assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x1122_3344_5566_7788);
        assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), 1);
        assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x8877_6655_4433_2211);
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn waitpkg_interpreter_faults_are_precise_and_noncommitting() {
    for (bytes, stack_segment) in [
        (&[0x36, 0xF3, 0x0F, 0xAE, 0xF0][..], true),
        (&[0xF3, 0x0F, 0xAE, 0xF0][..], false),
    ] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86(X86Reg::Rax), 0x0000_8000_0000_0000);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &block_for(bytes),
        );
        if stack_segment {
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::StackSegment {
                    addr: 0x1000,
                    error_code: 0,
                })
            ));
        } else {
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0,
                })
            ));
        }
    }

    let initial_flags = MaterializedFlags {
        cf: true,
        zf: true,
        sf: true,
        of: true,
        pf: true,
        af: true,
        df: true,
        ac: true,
    };
    for (control, tsd) in [(2_u64, false), (0, true)] {
        for bytes in [&[0x66, 0x0F, 0xAE, 0xF1][..], &[0xF2, 0x0F, 0xAE, 0xF1]] {
            let mut context = SmirContext::new_x86_64();
            context.flags.materialized = initial_flags;
            context.write_vreg(x86(X86Reg::Rcx), control);
            let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
                unreachable!()
            };
            x86_state.cr0 = 1;
            x86_state.cr4 = if tsd { 1 << 2 } else { 0 };
            x86_state.cpl = if tsd { 3 } else { 0 };
            assert!(matches!(
                SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut FlatMemory::new(1),
                    &block_for(bytes),
                ),
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0,
                })
            ));
            assert_eq!(
                context.flags.materialized.to_rflags(),
                initial_flags.to_rflags()
            );
        }
    }

    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86(X86Reg::Rax), 0xDEAD_BEEF);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &block_for(&[0xF3, 0x0F, 0xAE, 0xF0]),
        ),
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0xDEAD_BEEF,
            write: false,
        })
    ));
}

#[test]
fn waitpkg_operations_and_deadline_inputs_survive_o2() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86WaitPkg(X86WaitPkgOp::Umonitor {
            addr: Address::Direct(x86(X86Reg::Rbx)),
            stack_segment: false,
        }),
    );
    builder.push_op(
        0x1004,
        OpKind::X86WaitPkg(X86WaitPkgOp::Umwait {
            control: x86(X86Reg::Rcx),
            deadline_low: x86(X86Reg::Rax),
            deadline_high: x86(X86Reg::Rdx),
        }),
    );
    builder.push_op(
        0x1008,
        OpKind::X86WaitPkg(X86WaitPkgOp::Tpause {
            control: x86(X86Reg::R8),
            deadline_low: x86(X86Reg::Rax),
            deadline_high: x86(X86Reg::Rdx),
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86WaitPkg(..)))
            .count(),
        3
    );
}
