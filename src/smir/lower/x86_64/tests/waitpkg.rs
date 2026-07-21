//! Fault-precise native lowering for UMONITOR/UMWAIT/TPAUSE.

use super::*;
use crate::smir::ir::ops::X86WaitPkgOp;
use crate::smir::ir::types::OpId;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn monitor(addr: Address, stack_segment: bool) -> OpKind {
    OpKind::X86WaitPkg(X86WaitPkgOp::Umonitor {
        addr,
        stack_segment,
    })
}

fn wait(control: X86Reg, timed_pause: bool) -> OpKind {
    let operands = (x86(control), x86(X86Reg::Rax), x86(X86Reg::Rdx));
    OpKind::X86WaitPkg(if timed_pause {
        X86WaitPkgOp::Tpause {
            control: operands.0,
            deadline_low: operands.1,
            deadline_high: operands.2,
        }
    } else {
        X86WaitPkgOp::Umwait {
            control: operands.0,
            deadline_low: operands.1,
            deadline_high: operands.2,
        }
    })
}

fn operation(kind: OpKind) -> SmirOp {
    SmirOp::new(OpId(0), 0x1000, kind)
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
fn lower_waitpkg_requires_only_the_helpers_and_guards_each_variant_needs() {
    for timed_pause in [false, true] {
        assert!(matches!(
            lower(wait(X86Reg::Rcx, timed_pause), false, false),
            Err(LowerError::UnsupportedOp { .. })
        ));
        lower(wait(X86Reg::Rcx, timed_pause), false, true)
            .expect("guarded deterministic wait lowering");
    }

    let umonitor = monitor(Address::Direct(x86(X86Reg::Rbx)), false);
    assert!(matches!(
        lower(umonitor.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    lower(umonitor, true, false).expect("helper-backed UMONITOR needs no dynamic guard");
}

#[test]
fn lower_waitpkg_rejects_every_malformed_operand_and_address_class() {
    let virtual_reg = VReg::Virtual(crate::smir::ir::types::VirtualId(0));
    for malformed in [
        OpKind::X86WaitPkg(X86WaitPkgOp::Umwait {
            control: virtual_reg,
            deadline_low: x86(X86Reg::Rax),
            deadline_high: x86(X86Reg::Rdx),
        }),
        OpKind::X86WaitPkg(X86WaitPkgOp::Umwait {
            control: x86(X86Reg::Rcx),
            deadline_low: x86(X86Reg::Rbx),
            deadline_high: x86(X86Reg::Rdx),
        }),
        OpKind::X86WaitPkg(X86WaitPkgOp::Tpause {
            control: x86(X86Reg::Rcx),
            deadline_low: x86(X86Reg::Rax),
            deadline_high: x86(X86Reg::Rbx),
        }),
        monitor(Address::Absolute(0x1000), false),
        monitor(Address::Direct(virtual_reg), false),
        monitor(
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
        ),
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
        assert!(!x86_waitpkg_shape_valid(&operation(malformed.clone())));
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[test]
fn lower_waitpkg_accepts_all_state_backed_gprs_and_never_executes_host_waitpkg() {
    for index in 0..32 {
        let control = X86Reg::gpr(index);
        for timed_pause in [false, true] {
            let kind = wait(control, timed_pause);
            assert!(x86_waitpkg_shape_valid(&operation(kind.clone())));
            let (code, _) = lower(kind, false, true).expect("lower wait GPR class");
            assert!(
                !code.windows(4).any(|window| {
                    matches!(window[0], 0x66 | 0xF2)
                        && window[1] == 0x0F
                        && window[2] == 0xAE
                        && matches!(window[3], 0xF0..=0xF7)
                }),
                "guest wait must not execute on the host: {code:02X?}"
            );
        }
    }

    for addr in [
        Address::Direct(x86(X86Reg::R31)),
        Address::X86Addr32(Box::new(Address::Direct(x86(X86Reg::Rsp)))),
        Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::R15)),
            index: None,
            scale: 1,
            disp: 0,
        },
    ] {
        let kind = monitor(addr, false);
        assert!(x86_waitpkg_shape_valid(&operation(kind.clone())));
        let (code, _) = lower(kind, true, false).expect("lower UMONITOR address class");
        assert!(
            !code.windows(4).any(|window| {
                window[0] == 0xF3
                    && window[1] == 0x0F
                    && window[2] == 0xAE
                    && matches!(window[3], 0xF0..=0xF7)
            }),
            "guest UMONITOR must not execute on the host: {code:02X?}"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    control: X86Reg,
    timed_pause: bool,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(wait(control, timed_pause), false, true).expect("lower WAITPKG");
    let exec = ExecMem::new(&code).expect("map WAITPKG");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.gpr[control.gpr_index().unwrap() as usize] = 1;
    // Apple host translation clears imported AF across the linux/amd64
    // bridge; retain every other status flag as a native differential input.
    regs.rflags = 0x2 | 0x08C5 | (1 << 10);
    regs.exit_pc = 0xDEAD_BEEF;
    regs.cr0 = 1;
    regs.cr4 = 0;
    regs.cpl = 3;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_waitpkg_clears_only_status_flags_and_preserves_all_gprs() {
    for control in [X86Reg::Rcx, X86Reg::Rsp, X86Reg::R16, X86Reg::R31] {
        for timed_pause in [false, true] {
            let regs = execute_native(control, timed_pause, |_| {});
            for (index, value) in regs.gpr.iter().enumerate() {
                let expected = if index == control.gpr_index().unwrap() as usize {
                    1
                } else {
                    0xA500_0000_0000_0000 | index as u64
                };
                assert_eq!(*value, expected, "GPR {index}, {control:?}");
            }
            assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 1 << 10);
            assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_waitpkg_dynamic_fault_handoffs_are_precise_and_noncommitting() {
    for timed_pause in [false, true] {
        for (control, cr0, cr4, cpl) in [
            (2_u64, 1_u64, 0_u64, 0_u64),
            (0, 1, 1 << 2, 3),
            (2, 1, 1 << 2, 3),
        ] {
            let regs = execute_native(X86Reg::Rcx, timed_pause, |regs| {
                regs.gpr[1] = control;
                regs.cr0 = cr0;
                regs.cr4 = cr4;
                regs.cpl = cpl;
            });
            assert_eq!(regs.exit_pc, 0x1000);
            assert_eq!(regs.gpr[1], control);
            assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08C5 | (1 << 10));
        }

        for (cr0, cr4, cpl) in [(0, 1 << 2, 3), (1, 0, 3), (1, 1 << 2, 0)] {
            let regs = execute_native(X86Reg::Rcx, timed_pause, |regs| {
                regs.cr0 = cr0;
                regs.cr4 = cr4;
                regs.cpl = cpl;
            });
            assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
            assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 1 << 10);
        }
    }
}
