//! Fail-closed native admission for x86 MONITOR/MWAIT.

use super::*;
use crate::smir::ir::ops::X86MonitorMwaitOp;
use crate::smir::lower::x86_64::x86_monitor_mwait_shape_valid;

fn monitor_mwait(rcx: VReg, hint: VReg, addr: Option<Address>) -> OpKind {
    OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx,
        hint,
        addr,
        stack_segment: false,
    })
}

fn stack_monitor(addr: Address) -> OpKind {
    OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx: x86(X86Reg::Rcx),
        hint: x86(X86Reg::Rdx),
        addr: Some(addr),
        stack_segment: true,
    })
}

fn function(kind: OpKind) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn x86_gate_with_mem(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn x86_monitor_mwait_gate_distinguishes_memory_and_nonmemory_forms() {
    let mwait = monitor_mwait(x86(X86Reg::Rcx), x86(X86Reg::Rax), None);
    assert!(mwait.is_jit_safe());
    assert!(x86_monitor_mwait_shape_valid(&mwait));
    assert!(x86_gate_with_mem(mwait.clone(), false));
    assert!(x86_gate_with_mem(mwait, true));

    let monitor = monitor_mwait(
        x86(X86Reg::Rcx),
        x86(X86Reg::Rdx),
        Some(Address::Direct(x86(X86Reg::Rax))),
    );
    assert!(monitor.is_jit_safe());
    assert!(x86_monitor_mwait_shape_valid(&monitor));
    assert!(!x86_gate_with_mem(monitor.clone(), false));
    assert!(x86_gate_with_mem(monitor, true));
}

#[test]
fn x86_monitor_mwait_gate_rejects_malformed_ir_fail_closed() {
    let malformed_rcx = monitor_mwait(x86(X86Reg::Rax), x86(X86Reg::Rax), None);
    assert!(
        malformed_rcx.is_jit_safe(),
        "class whitelist is shape-agnostic"
    );
    assert!(!x86_monitor_mwait_shape_valid(&malformed_rcx));
    assert!(!x86_gate_with_mem(malformed_rcx, true));

    let malformed_hint = monitor_mwait(x86(X86Reg::Rcx), x86(X86Reg::Rdx), None);
    assert!(!x86_monitor_mwait_shape_valid(&malformed_hint));
    assert!(!x86_gate_with_mem(malformed_hint, true));

    let malformed_mwait_segment = OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx: x86(X86Reg::Rcx),
        hint: x86(X86Reg::Rax),
        addr: None,
        stack_segment: true,
    });
    assert!(!x86_monitor_mwait_shape_valid(&malformed_mwait_segment));
    assert!(!x86_gate_with_mem(malformed_mwait_segment, true));

    for addr in [
        Address::Direct(VReg::Virtual(VirtualId(0))),
        Address::Direct(x86(X86Reg::Rbx)),
        Address::Absolute(0x1000),
        Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rax)),
            index: x86(X86Reg::Rdx),
            scale: 3,
            disp: 0,
            disp_size: DispSize::Auto,
        },
        Address::SegmentRel {
            segment: x86(X86Reg::Rax),
            base: Some(x86(X86Reg::Rbx)),
            index: None,
            scale: 1,
            disp: 0,
        },
    ] {
        let malformed = monitor_mwait(x86(X86Reg::Rcx), x86(X86Reg::Rdx), Some(addr));
        assert!(!x86_monitor_mwait_shape_valid(&malformed));
        assert!(!x86_gate_with_mem(malformed, true));
    }
}

#[test]
fn x86_monitor_mwait_gate_accepts_addr32_and_segment_relative_monitor() {
    for addr in [
        Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rax)))),
        Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::Rax)),
            index: None,
            scale: 1,
            disp: 0,
        },
        Address::X86Addr32(Box::new(Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::Rax)),
            index: None,
            scale: 1,
            disp: 0,
        })),
    ] {
        let kind = monitor_mwait(x86(X86Reg::Rcx), x86(X86Reg::Rdx), Some(addr));
        assert!(x86_gate_with_mem(kind, true));
    }

    let ss = stack_monitor(Address::Direct(x86(X86Reg::Rax)));
    assert!(x86_monitor_mwait_shape_valid(&ss));
    assert!(x86_gate_with_mem(ss, true));

    let malformed_ss = stack_monitor(Address::SegmentRel {
        segment: x86(X86Reg::FsBase),
        base: Some(x86(X86Reg::Rax)),
        index: None,
        scale: 1,
        disp: 0,
    });
    assert!(!x86_monitor_mwait_shape_valid(&malformed_ss));
    assert!(!x86_gate_with_mem(malformed_ss, true));
}

#[test]
fn x86_monitor_mwait_gate_rejects_cross_host_execution() {
    for kind in [
        monitor_mwait(x86(X86Reg::Rcx), x86(X86Reg::Rax), None),
        monitor_mwait(
            x86(X86Reg::Rcx),
            x86(X86Reg::Rdx),
            Some(Address::Direct(x86(X86Reg::Rax))),
        ),
    ] {
        assert!(!x86_aarch64_gate(vec![kind.clone()]));
        assert!(!x86_aarch64_scalar_shape_valid(&kind));
    }
}

#[test]
fn x86_monitor_mwait_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        monitor_mwait(
            x86(X86Reg::Rcx),
            x86(X86Reg::Rdx),
            Some(Address::Direct(x86(X86Reg::Rax))),
        ),
    );
    builder.push_op(
        0x1003,
        monitor_mwait(x86(X86Reg::Rcx), x86(X86Reg::Rax), None),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert_eq!(
        function.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86MonitorMwait(..)))
            .count(),
        2
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
