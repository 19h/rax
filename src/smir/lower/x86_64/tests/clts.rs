//! Fault-precise, state-backed native lowering for CLTS.

use super::*;

fn lower_clts(count: usize, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for index in 0..count {
        builder.push_op(0x1000 + index as u64 * 2, OpKind::X86Clts);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_clts_requires_precise_fault_guards_and_never_emits_host_clts() {
    assert!(matches!(
        lower_clts(1, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (code, _) = lower_clts(1, true).expect("guarded CLTS lowering");
    assert!(
        !code.windows(2).any(|window| window == [0x0F, 0x06]),
        "guest CLTS must not execute the privileged host instruction: {code:02X?}"
    );
    for offset in [X86_GUEST_CR0_OFFSET, X86_GUEST_CPL_OFFSET] {
        let encoded = (offset as u32).to_le_bytes();
        assert!(
            code.windows(4).any(|window| window == encoded),
            "lowering must consult GuestRegs offset {offset}"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    count: usize,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower_clts(count, true).expect("lower guarded CLTS");
    let exec = ExecMem::new(&code).expect("map guarded CLTS");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_clts_clears_only_guest_ts_and_is_idempotent() {
    let initial = 0xFFFF_FFFF_FFFF_FFFF;
    let regs = execute_native(3, |regs| {
        regs.cr0 = initial;
        regs.cpl = 0;
    });
    assert_eq!(regs.cr0, initial & !(1 << 3));
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
    assert_eq!(regs.ac_flag, 1);
    for (index, value) in regs.gpr.iter().enumerate() {
        assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_clts_real_mode_bypasses_stale_nonzero_cpl() {
    let initial = 0x5003A;
    let regs = execute_native(1, |regs| {
        regs.cr0 = initial & !1;
        regs.cpl = 3;
    });
    assert_eq!(regs.cr0, (initial & !1) & !(1 << 3));
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_clts_privilege_fault_handoff_is_precise_and_noncommitting() {
    let initial = 0x8005_003B;
    let regs = execute_native(1, |regs| {
        regs.cr0 = initial;
        regs.cpl = 3;
    });
    assert_eq!(regs.exit_pc, 0x1000);
    assert_eq!(regs.cr0, initial);
    assert_eq!(regs.ac_flag, 1);
    for (index, value) in regs.gpr.iter().enumerate() {
        assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
}
