//! Fault-precise native lowering for MONITOR/MWAIT.

use super::*;
use crate::smir::ir::ops::X86MonitorMwaitOp;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn monitor(addr: Address) -> OpKind {
    OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx: x86(X86Reg::Rcx),
        hint: x86(X86Reg::Rdx),
        addr: Some(addr),
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

fn mwait() -> OpKind {
    OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx: x86(X86Reg::Rcx),
        hint: x86(X86Reg::Rax),
        addr: None,
        stack_segment: false,
    })
}

fn lower(
    kind: OpKind,
    mem_helpers: bool,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_monitor_mwait_requires_precise_guards_and_monitor_requires_memory_helpers() {
    assert!(matches!(
        lower(mwait(), false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    lower(mwait(), false, true).expect("guarded MWAIT lowering");

    let kind = monitor(Address::Direct(x86(X86Reg::Rax)));
    assert!(matches!(
        lower(kind.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    lower(kind, true, true).expect("helper-backed guarded MONITOR lowering");
    lower(stack_monitor(Address::Direct(x86(X86Reg::Rax))), true, true)
        .expect("SS-prefixed MONITOR lowering");
}

#[test]
fn lower_monitor_mwait_rejects_malformed_operands_and_addresses() {
    let malformed_rcx = OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx: x86(X86Reg::Rax),
        hint: x86(X86Reg::Rax),
        addr: None,
        stack_segment: false,
    });
    assert!(!x86_monitor_mwait_shape_valid(&malformed_rcx));
    assert!(matches!(
        lower(malformed_rcx, false, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    let malformed_hint = OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx: x86(X86Reg::Rcx),
        hint: x86(X86Reg::Rdx),
        addr: None,
        stack_segment: false,
    });
    assert!(!x86_monitor_mwait_shape_valid(&malformed_hint));
    assert!(matches!(
        lower(malformed_hint, false, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    let malformed_mwait_segment = OpKind::X86MonitorMwait(X86MonitorMwaitOp {
        rcx: x86(X86Reg::Rcx),
        hint: x86(X86Reg::Rax),
        addr: None,
        stack_segment: true,
    });
    assert!(!x86_monitor_mwait_shape_valid(&malformed_mwait_segment));
    assert!(matches!(
        lower(malformed_mwait_segment, false, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    let malformed_addr = monitor(Address::Direct(VReg::Virtual(
        crate::smir::ir::types::VirtualId(0),
    )));
    assert!(!x86_monitor_mwait_shape_valid(&malformed_addr));
    assert!(matches!(
        lower(malformed_addr, true, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    for malformed_addr in [
        Address::Direct(x86(X86Reg::Rbx)),
        Address::Absolute(0x1000),
        Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
            x86(X86Reg::Rax),
        ))))),
    ] {
        let malformed = monitor(malformed_addr);
        assert!(!x86_monitor_mwait_shape_valid(&malformed));
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let malformed_stack_segment = stack_monitor(Address::SegmentRel {
        segment: x86(X86Reg::FsBase),
        base: Some(x86(X86Reg::Rax)),
        index: None,
        scale: 1,
        disp: 0,
    });
    assert!(!x86_monitor_mwait_shape_valid(&malformed_stack_segment));
    assert!(matches!(
        lower(malformed_stack_segment, true, true),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[test]
fn lower_monitor_mwait_never_emits_host_power_management_instructions() {
    for (kind, mem_helpers) in [
        (mwait(), false),
        (monitor(Address::Direct(x86(X86Reg::Rax))), true),
        (
            monitor(Address::X86Addr32(Box::new(Address::Direct(x86(
                X86Reg::Rax,
            ))))),
            true,
        ),
    ] {
        let (code, _) = lower(kind, mem_helpers, true).expect("lower MONITOR/MWAIT");
        assert!(
            !code
                .windows(3)
                .any(|window| window == [0x0F, 0x01, 0xC8] || window == [0x0F, 0x01, 0xC9]),
            "guest power-management op must not execute on the host: {code:02X?}"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(mwait(), false, true).expect("lower guarded MWAIT");
    let exec = ExecMem::new(&code).expect("map guarded MWAIT");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.gpr[1] = 0;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.exit_pc = 0xDEAD_BEEF;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_mwait_success_preserves_every_gpr_and_flags() {
    let regs = execute_native(|_| {});
    for (index, value) in regs.gpr.iter().enumerate() {
        let expected = if index == 1 {
            0
        } else {
            0xA500_0000_0000_0000 | index as u64
        };
        assert_eq!(*value, expected, "GPR {index}");
    }
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_mwait_dynamic_fault_handoffs_are_precise_and_noncommitting() {
    for configure in [
        (3_u64, 0_u64, "CPL"),
        (0_u64, 1_u64, "RCX"),
        (3_u64, 1_u64, "CPL+RCX"),
    ] {
        let regs = execute_native(|regs| {
            regs.cpl = configure.0;
            regs.gpr[1] = configure.1;
        });
        assert_eq!(regs.exit_pc, 0x1000, "{} handoff", configure.2);
        assert_eq!(regs.cpl, configure.0);
        assert_eq!(regs.gpr[1], configure.1);
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}
