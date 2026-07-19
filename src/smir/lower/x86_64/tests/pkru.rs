//! Fault-precise state-backed native lowering for RDPKRU/WRPKRU.

use super::*;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn pkru_kind(write: bool) -> OpKind {
    OpKind::X86Pkru {
        eax: x86(X86Reg::Rax),
        ecx: x86(X86Reg::Rcx),
        edx: x86(X86Reg::Rdx),
        pkru: x86(X86Reg::Pkru),
        write,
    }
}

fn lower_pkru(kind: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_pkru_requires_precise_jit_fault_guards_and_never_emits_host_pkru() {
    assert!(matches!(
        lower_pkru(pkru_kind(false), false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    for write in [false, true] {
        let (code, _) = lower_pkru(pkru_kind(write), true).expect("guarded PKRU lowering");
        assert!(
            !code
                .windows(3)
                .any(|window| window == [0x0F, 0x01, if write { 0xEF } else { 0xEE }]),
            "guest PKRU operation must not access the host thread's PKRU"
        );
    }
}

#[test]
fn lower_pkru_rejects_every_malformed_implicit_operand() {
    for malformed in [
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rbx),
            ecx: x86(X86Reg::Rcx),
            edx: x86(X86Reg::Rdx),
            pkru: x86(X86Reg::Pkru),
            write: false,
        },
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rax),
            ecx: x86(X86Reg::Rbx),
            edx: x86(X86Reg::Rdx),
            pkru: x86(X86Reg::Pkru),
            write: false,
        },
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rax),
            ecx: x86(X86Reg::Rcx),
            edx: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
            pkru: x86(X86Reg::Pkru),
            write: true,
        },
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rax),
            ecx: x86(X86Reg::Rcx),
            edx: x86(X86Reg::Rdx),
            pkru: x86(X86Reg::FsBase),
            write: true,
        },
    ] {
        assert!(!x86_pkru_shape_valid(&malformed));
        assert!(matches!(
            lower_pkru(malformed, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    write: bool,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower_pkru(pkru_kind(write), true).expect("lower guarded PKRU");
    let exec = ExecMem::new(&code).expect("map guarded PKRU");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.gpr[1] = 0;
    regs.gpr[2] = 0;
    regs.cr4 = 1 << 22;
    regs.pkru = 0x1234_5678;
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.exit_pc = 0xDEAD_BEEF;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_rdpkru_reads_guest_state_zero_extends_and_preserves_every_other_input() {
    let regs = execute_native(false, |regs| {
        regs.gpr[0] = u64::MAX;
        regs.gpr[1] = 0xFFFF_FFFF_0000_0000;
        regs.gpr[2] = u64::MAX;
        regs.pkru = 0x89AB_CDEF;
    });
    assert_eq!(regs.gpr[0], 0x89AB_CDEF);
    assert_eq!(regs.gpr[1], 0xFFFF_FFFF_0000_0000);
    assert_eq!(regs.gpr[2], 0);
    assert_eq!(regs.pkru, 0x89AB_CDEF);
    for index in 3..32 {
        assert_eq!(regs.gpr[index], 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_wrpkru_writes_low_eax_and_ignores_high_selector_halves() {
    let regs = execute_native(true, |regs| {
        regs.gpr[0] = 0xFFFF_FFFF_89AB_CDEF;
        regs.gpr[1] = 0x1357_9BDF_0000_0000;
        regs.gpr[2] = 0x2468_ACE0_0000_0000;
    });
    assert_eq!(regs.pkru, 0x89AB_CDEF);
    assert_eq!(regs.gpr[0], 0xFFFF_FFFF_89AB_CDEF);
    assert_eq!(regs.gpr[1], 0x1357_9BDF_0000_0000);
    assert_eq!(regs.gpr[2], 0x2468_ACE0_0000_0000);
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_pkru_fault_guards_handoff_precisely_without_partial_commit() {
    let regs = execute_native(false, |regs| {
        regs.cr4 = 0;
        regs.gpr[0] = 0xA5A5;
        regs.gpr[2] = 0x5A5A;
    });
    assert_eq!(regs.exit_pc, 0x1000);
    assert_eq!(regs.gpr[0], 0xA5A5);
    assert_eq!(regs.gpr[2], 0x5A5A);
    assert_eq!(regs.pkru, 0x1234_5678);

    let regs = execute_native(false, |regs| {
        regs.gpr[0] = 0xA5A5;
        regs.gpr[1] = 1;
        regs.gpr[2] = 0x5A5A;
    });
    assert_eq!(regs.exit_pc, 0x1000);
    assert_eq!(regs.gpr[0], 0xA5A5);
    assert_eq!(regs.gpr[2], 0x5A5A);
    assert_eq!(regs.pkru, 0x1234_5678);

    for (ecx, edx) in [(1, 0), (0, 1), (1, 1)] {
        let regs = execute_native(true, |regs| {
            regs.gpr[0] = 0x89AB_CDEF;
            regs.gpr[1] = ecx;
            regs.gpr[2] = edx;
        });
        assert_eq!(regs.exit_pc, 0x1000);
        assert_eq!(regs.pkru, 0x1234_5678);
        assert_eq!(regs.gpr[0], 0x89AB_CDEF);
        assert_eq!(regs.gpr[1], ecx);
        assert_eq!(regs.gpr[2], edx);
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}
