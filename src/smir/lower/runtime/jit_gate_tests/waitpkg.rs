//! Fail-closed native admission for x86 WAITPKG.

use super::*;
use crate::smir::ir::ops::X86WaitPkgOp;
use crate::smir::lower::x86_64::x86_waitpkg_shape_valid;

fn wait(control: VReg, timed_pause: bool) -> OpKind {
    OpKind::X86WaitPkg(if timed_pause {
        X86WaitPkgOp::Tpause {
            control,
            deadline_low: x86(X86Reg::Rax),
            deadline_high: x86(X86Reg::Rdx),
        }
    } else {
        X86WaitPkgOp::Umwait {
            control,
            deadline_low: x86(X86Reg::Rax),
            deadline_high: x86(X86Reg::Rdx),
        }
    })
}

fn monitor(addr: Address, stack_segment: bool) -> OpKind {
    OpKind::X86WaitPkg(X86WaitPkgOp::Umonitor {
        addr,
        stack_segment,
    })
}

fn function(kind: OpKind) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn shape_valid(kind: OpKind) -> bool {
    x86_waitpkg_shape_valid(&function(kind).blocks[0].ops[0])
}

fn x86_gate_with_mem(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn x86_waitpkg_gate_distinguishes_monitor_memory_from_waits() {
    assert_eq!(
        x86_flag_defs(&monitor(Address::Direct(x86(X86Reg::Rbx)), false)),
        FlagSet::EMPTY
    );
    for timed_pause in [false, true] {
        for index in 0..32 {
            let control = X86Reg::gpr(index);
            let kind = wait(x86(control), timed_pause);
            assert!(kind.is_jit_safe());
            assert_eq!(x86_flag_defs(&kind), FlagSet::ALL_X86);
            assert!(shape_valid(kind.clone()));
            assert!(x86_gate_with_mem(kind.clone(), false));
            assert!(x86_gate_with_mem(kind, true));
        }
    }

    let kind = monitor(Address::Direct(x86(X86Reg::R31)), false);
    assert!(kind.is_jit_safe());
    assert!(shape_valid(kind.clone()));
    assert!(!x86_gate_with_mem(kind.clone(), false));
    assert!(x86_gate_with_mem(kind, true));
}

#[test]
fn x86_waitpkg_gate_rejects_malformed_ir_fail_closed() {
    for malformed in [
        OpKind::X86WaitPkg(X86WaitPkgOp::Umwait {
            control: VReg::Virtual(VirtualId(0)),
            deadline_low: x86(X86Reg::Rax),
            deadline_high: x86(X86Reg::Rdx),
        }),
        OpKind::X86WaitPkg(X86WaitPkgOp::Tpause {
            control: x86(X86Reg::Rcx),
            deadline_low: x86(X86Reg::Rbx),
            deadline_high: x86(X86Reg::Rdx),
        }),
        monitor(Address::Absolute(0x1000), false),
        monitor(Address::Direct(VReg::Virtual(VirtualId(0))), false),
        monitor(
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rax)),
                index: None,
                scale: 1,
                disp: 0,
            },
            true,
        ),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!shape_valid(malformed.clone()));
        assert!(!x86_gate_with_mem(malformed, true));
    }
}

#[test]
fn x86_waitpkg_gate_accepts_addr32_segments_and_rejects_cross_host_execution() {
    for kind in [
        monitor(
            Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rsp)))),
            false,
        ),
        monitor(
            Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::R15)),
                index: None,
                scale: 1,
                disp: 0,
            },
            false,
        ),
        monitor(Address::Direct(x86(X86Reg::Rbx)), true),
        wait(x86(X86Reg::R31), false),
        wait(x86(X86Reg::R16), true),
    ] {
        assert!(shape_valid(kind.clone()));
        assert!(x86_gate_with_mem(kind.clone(), true));
        assert!(!x86_aarch64_gate(vec![kind.clone()]));
        assert!(!x86_aarch64_scalar_shape_valid(&kind));
    }
}

#[test]
fn x86_waitpkg_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, monitor(Address::Direct(x86(X86Reg::Rbx)), false));
    builder.push_op(0x1004, wait(x86(X86Reg::Rcx), false));
    builder.push_op(0x1008, wait(x86(X86Reg::R16), true));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert_eq!(
        function.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86WaitPkg(..)))
            .count(),
        3
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}

#[test]
fn x86_waitpkg_flag_definitions_terminate_upstream_native_flag_liveness() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Adc {
            dst: x86(X86Reg::Rbx),
            src1: x86(X86Reg::Rbx),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(0x1004, wait(x86(X86Reg::Rdx), false));
    builder.set_terminator(Terminator::Return { values: vec![] });

    assert!(is_native_clobber_safe_excluding(
        &builder.finish(),
        &std::collections::HashMap::new(),
        false,
    ));
}
