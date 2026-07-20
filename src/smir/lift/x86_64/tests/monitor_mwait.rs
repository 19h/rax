//! Strict lift and canonical interpreter coverage for MONITOR/MWAIT.

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

fn exact_monitor_mwait(result: &LiftResult) -> &X86MonitorMwaitOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86MonitorMwait(op) => op,
        other => panic!("expected one exact X86MonitorMwait op, got {other:?}"),
    }
}

fn block_for(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict MONITOR/MWAIT lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

#[test]
fn monitor_and_mwait_strictly_lift_as_exact_straight_line_ops() {
    let monitor = lift_single(&[0x0F, 0x01, 0xC8]).expect("strict MONITOR lift");
    assert_eq!(monitor.bytes_consumed, 3);
    assert!(matches!(monitor.control_flow, ControlFlow::Fallthrough));
    let op = exact_monitor_mwait(&monitor);
    assert_eq!(op.rcx, x86(X86Reg::Rcx));
    assert_eq!(op.hint, x86(X86Reg::Rdx));
    assert!(!op.stack_segment);
    assert!(matches!(&op.addr, Some(Address::Direct(rax)) if *rax == x86(X86Reg::Rax)));

    let mwait = lift_single(&[0x0F, 0x01, 0xC9]).expect("strict MWAIT lift");
    assert_eq!(mwait.bytes_consumed, 3);
    assert!(matches!(mwait.control_flow, ControlFlow::Fallthrough));
    let op = exact_monitor_mwait(&mwait);
    assert_eq!(op.rcx, x86(X86Reg::Rcx));
    assert_eq!(op.hint, x86(X86Reg::Rax));
    assert!(!op.stack_segment);
    assert!(op.addr.is_none());
}

#[test]
fn monitor_lift_preserves_addr32_and_fs_gs_segment_semantics() {
    let addr32 = lift_single(&[0x67, 0x0F, 0x01, 0xC8]).unwrap();
    let addr = &exact_monitor_mwait(&addr32).addr;
    assert!(matches!(
        addr,
        Some(Address::X86Addr32(inner))
            if matches!(inner.as_ref(), Address::Direct(rax) if *rax == x86(X86Reg::Rax))
    ));

    for (prefix, segment) in [(0x64, X86Reg::FsBase), (0x65, X86Reg::GsBase)] {
        let lifted = lift_single(&[prefix, 0x0F, 0x01, 0xC8]).unwrap();
        let addr = &exact_monitor_mwait(&lifted).addr;
        assert!(matches!(
            addr,
            Some(Address::SegmentRel {
                segment: got_segment,
                base: Some(base),
                index: None,
                scale: 1,
                disp: 0,
            }) if *got_segment == x86(segment) && *base == x86(X86Reg::Rax)
        ));
    }

    let addr32_fs = lift_single(&[0x64, 0x67, 0x0F, 0x01, 0xC8]).unwrap();
    let addr = &exact_monitor_mwait(&addr32_fs).addr;
    assert!(matches!(
        addr,
        Some(Address::X86Addr32(inner))
            if matches!(
                inner.as_ref(),
                Address::SegmentRel {
                    segment,
                    base: Some(base),
                    index: None,
                    scale: 1,
                    disp: 0,
                } if *segment == x86(X86Reg::FsBase) && *base == x86(X86Reg::Rax)
            )
    ));

    let ss = lift_single(&[0x36, 0x0F, 0x01, 0xC8]).unwrap();
    let op = exact_monitor_mwait(&ss);
    assert!(op.stack_segment);
    assert!(matches!(&op.addr, Some(Address::Direct(rax)) if *rax == x86(X86Reg::Rax)));
}

#[test]
fn monitor_mwait_ignore_documented_prefixes_and_reject_lock() {
    for opcode in [0xC8, 0xC9] {
        for prefix in [0x66, 0x67, 0x48, 0xF2, 0xF3] {
            let bytes = [prefix, 0x0F, 0x01, opcode];
            let result = lift_single(&bytes).expect("architecturally ignored prefix");
            assert_eq!(result.bytes_consumed, bytes.len());
            exact_monitor_mwait(&result);
        }
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x01, opcode]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn monitor_mwait_metadata_exposes_faulting_read_and_implicit_inputs() {
    let monitor_lift = lift_single(&[0x64, 0x0F, 0x01, 0xC8]).unwrap();
    let monitor = exact_monitor_mwait(&monitor_lift);
    let monitor_kind = &monitor_lift.ops[0].kind;
    assert_eq!(monitor.rcx, x86(X86Reg::Rcx));
    assert_eq!(monitor.hint, x86(X86Reg::Rdx));
    assert_eq!(
        monitor_kind.source_vregs(),
        vec![
            x86(X86Reg::Rcx),
            x86(X86Reg::Rdx),
            x86(X86Reg::FsBase),
            x86(X86Reg::Rax)
        ]
    );
    assert!(monitor_kind.dests().is_empty());
    assert!(monitor_kind.has_side_effects());
    assert!(monitor_kind.reads_memory());
    assert!(!monitor_kind.writes_memory());
    assert!(monitor_lift.ops[0].is_jit_safe());

    let mwait = lift_single(&[0x0F, 0x01, 0xC9]).unwrap();
    let mwait_kind = &mwait.ops[0].kind;
    assert_eq!(
        mwait_kind.source_vregs(),
        vec![x86(X86Reg::Rcx), x86(X86Reg::Rax)]
    );
    assert!(mwait_kind.dests().is_empty());
    assert!(mwait_kind.has_side_effects());
    assert!(!mwait_kind.reads_memory());
    assert!(!mwait_kind.writes_memory());
    assert!(mwait.ops[0].is_jit_safe());
}

#[test]
fn monitor_mwait_interpreter_success_preserves_registers_and_flags() {
    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: true,
        pf: false,
        af: true,
        df: true,
        ac: true,
    };
    let mut context = SmirContext::new_x86_64();
    context.flags.materialized = flags;
    context.write_vreg(x86(X86Reg::Rax), 0x4008);
    context.write_vreg(x86(X86Reg::Rcx), 0);
    context.write_vreg(x86(X86Reg::Rdx), 0xA5A5_5A5A_1122_3344);
    let mut memory = FlatMemory::with_base(0x4000, 0x10);
    memory.load(8, &[0xA5]);

    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &block_for(&[0x0F, 0x01, 0xC8]),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x4008);
    assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), 0);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0xA5A5_5A5A_1122_3344);
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());

    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &block_for(&[0x0F, 0x01, 0xC9]),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
}

#[test]
fn monitor_mwait_interpreter_fault_priority_is_cpl_then_rcx_then_memory() {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
        unreachable!()
    };
    x86_state.cpl = 3;
    context.write_vreg(x86(X86Reg::Rax), 0xDEAD_BEEF);
    context.write_vreg(x86(X86Reg::Rcx), 1);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &block_for(&[0x0F, 0x01, 0xC8]),
        ),
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));

    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86(X86Reg::Rax), 0xDEAD_BEEF);
    context.write_vreg(x86(X86Reg::Rcx), 1);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &block_for(&[0x0F, 0x01, 0xC8]),
        ),
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));

    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86(X86Reg::Rax), 0x0000_8000_0000_0000);
    context.write_vreg(x86(X86Reg::Rcx), 0);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &block_for(&[0x36, 0x0F, 0x01, 0xC8]),
        ),
        BlockResult::Exit(ExitReason::StackSegment {
            addr: 0x1000,
            error_code: 0
        })
    ));

    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86(X86Reg::Rax), 0x0000_8000_0000_0000);
    context.write_vreg(x86(X86Reg::Rcx), 0);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &block_for(&[0x0F, 0x01, 0xC8]),
        ),
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));

    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86(X86Reg::Rax), 0xDEAD_BEEF);
    context.write_vreg(x86(X86Reg::Rcx), 0);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &block_for(&[0x0F, 0x01, 0xC8]),
        ),
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0xDEAD_BEEF,
            write: false
        })
    ));
}

#[test]
fn monitor_and_mwait_survive_o2_as_observable_operations() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86MonitorMwait(X86MonitorMwaitOp {
            rcx: x86(X86Reg::Rcx),
            hint: x86(X86Reg::Rdx),
            addr: Some(Address::Direct(x86(X86Reg::Rax))),
            stack_segment: false,
        }),
    );
    builder.push_op(
        0x1003,
        OpKind::X86MonitorMwait(X86MonitorMwaitOp {
            rcx: x86(X86Reg::Rcx),
            hint: x86(X86Reg::Rax),
            addr: None,
            stack_segment: false,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    let operations: Vec<_> = function
        .entry_block()
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::X86MonitorMwait(X86MonitorMwaitOp { addr, .. }) => Some(addr.is_some()),
            _ => None,
        })
        .collect();
    assert_eq!(operations, vec![true, false]);
}
