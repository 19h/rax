//! Fault-precise, state-backed native lowering for CLAC/STAC.

use super::*;

fn lower_ac(
    values: impl IntoIterator<Item = bool>,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (index, value) in values.into_iter().enumerate() {
        builder.push_op(0x1000 + index as u64 * 3, OpKind::SetAC { value });
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

fn lower_ac_flags_roundtrip() -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let saved = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    builder.push_op(0x1000, OpKind::SetAC { value: true });
    builder.push_op(0x1003, OpKind::ReadFlags { dst: saved });
    builder.push_op(0x1004, OpKind::SetAC { value: false });
    builder.push_op(0x1007, OpKind::WriteFlags { src: saved });
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_clac_stac_require_precise_fault_guards_and_never_emit_host_forms() {
    assert!(matches!(
        lower_ac([true], false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    for value in [false, true] {
        let (code, _) = lower_ac([value], true).expect("guarded CLAC/STAC lowering");
        for host_form in [[0x0F, 0x01, 0xCA], [0x0F, 0x01, 0xCB]] {
            assert!(
                !code.windows(3).any(|window| window == host_form),
                "guest CLAC/STAC must not execute against host RFLAGS: {code:02X?}"
            );
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    values: impl IntoIterator<Item = bool>,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower_ac(values, true).expect("lower guarded CLAC/STAC");
    let exec = ExecMem::new(&code).expect("map guarded CLAC/STAC");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18);
    regs.exit_pc = 0xDEAD_BEEF;
    regs.cr0 = 1;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_clac_stac_commit_only_the_guest_ac_shadow() {
    for (value, initial) in [(false, 1), (true, 0)] {
        let regs = execute_native([value], |regs| regs.ac_flag = initial);
        assert_eq!(regs.ac_flag, u64::from(value));
        assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
        for (index, actual) in regs.gpr.iter().enumerate() {
            assert_eq!(*actual, 0xA500_0000_0000_0000 | index as u64);
        }
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(regs.rflags & (1 << 18), 0, "host AC must remain clear");
    }

    let regs = execute_native([true, false, true], |regs| regs.ac_flag = 0);
    assert_eq!(regs.ac_flag, 1, "state writes must remain in program order");
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_read_write_flags_roundtrip_guest_ac_without_loading_host_ac() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower_ac_flags_roundtrip().expect("lower AC flag-image roundtrip");
    let exec = ExecMem::new(&code).expect("map AC flag-image roundtrip");
    let mut regs = GuestRegs {
        cr0: 1,
        cpl: 0,
        rflags: 0x2 | 0x08D5,
        ..Default::default()
    };
    exec.run(entry, &mut regs);

    assert_ne!(regs.gpr[3] & (1 << 18), 0, "ReadFlags must merge guest AC");
    assert_eq!(regs.ac_flag, 1, "WriteFlags must restore guest AC");
    assert_eq!(regs.rflags & (1 << 18), 0, "host AC must remain clear");
    assert_eq!(regs.rflags & 0x08D5, 0x08D5);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_clac_stac_real_mode_bypasses_nonzero_effective_cpl() {
    let regs = execute_native([true], |regs| {
        regs.cr0 = 0;
        regs.cpl = 3;
        regs.ac_flag = 0;
    });
    assert_eq!(regs.ac_flag, 1);
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_clac_stac_privilege_fault_handoff_is_precise_and_noncommitting() {
    for (value, initial) in [(false, 1), (true, 0)] {
        let regs = execute_native([value], |regs| {
            regs.cr0 = 1;
            regs.cpl = 3;
            regs.ac_flag = initial;
        });
        assert_eq!(regs.exit_pc, 0x1000);
        assert_eq!(regs.ac_flag, initial);
        for (index, actual) in regs.gpr.iter().enumerate() {
            assert_eq!(*actual, 0xA500_0000_0000_0000 | index as u64);
        }
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    }
}
