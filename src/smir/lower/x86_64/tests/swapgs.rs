//! Fault-precise state-backed native lowering for SWAPGS.

use super::*;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn swapgs_kind() -> OpKind {
    OpKind::X86SwapGs {
        gs_base: x86(X86Reg::GsBase),
        kernel_gs_base: x86(X86Reg::KernelGsBase),
    }
}

fn lower_swapgs(
    kinds: impl IntoIterator<Item = OpKind>,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (index, kind) in kinds.into_iter().enumerate() {
        builder.push_op(0x1000 + index as u64 * 3, kind);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_swapgs_requires_precise_jit_fault_guards() {
    assert!(matches!(
        lower_swapgs([swapgs_kind()], false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    lower_swapgs([swapgs_kind()], true).expect("guarded SWAPGS lowering");
}

#[test]
fn lower_swapgs_rejects_every_malformed_ir_shape() {
    for malformed in [
        OpKind::X86SwapGs {
            gs_base: x86(X86Reg::KernelGsBase),
            kernel_gs_base: x86(X86Reg::GsBase),
        },
        OpKind::X86SwapGs {
            gs_base: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
            kernel_gs_base: x86(X86Reg::KernelGsBase),
        },
        OpKind::X86SwapGs {
            gs_base: x86(X86Reg::GsBase),
            kernel_gs_base: x86(X86Reg::FsBase),
        },
    ] {
        assert!(!x86_swapgs_shape_valid(&malformed));
        assert!(matches!(
            lower_swapgs([malformed], true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[test]
fn lower_swapgs_never_emits_the_privileged_host_instruction() {
    let (code, _) = lower_swapgs([swapgs_kind()], true).expect("lower state-backed SWAPGS");
    assert!(
        !code.windows(3).any(|window| window == [0x0F, 0x01, 0xF8]),
        "guest SWAPGS must not execute against host GS state: {code:02X?}"
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    count: usize,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let kinds = std::iter::repeat_with(swapgs_kind).take(count);
    let (code, entry) = lower_swapgs(kinds, true).expect("lower guarded SWAPGS");
    let exec = ExecMem::new(&code).expect("map guarded SWAPGS");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.exit_pc = 0xDEAD_BEEF;
    configure(&mut regs);
    exec.run(entry, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_swapgs_exchanges_only_guest_state_and_preserves_gprs_and_flags() {
    let old_gs = 0x0000_7FFF_1234_5000;
    let old_kernel = 0xFFFF_8000_ABCD_E000;
    let regs = execute_native(1, |regs| {
        regs.gs_base = old_gs;
        regs.kernel_gs_base = old_kernel;
    });

    assert_eq!(regs.gs_base, old_kernel);
    assert_eq!(regs.kernel_gs_base, old_gs);
    for (index, value) in regs.gpr.iter().enumerate() {
        assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_swapgs_is_an_involution() {
    let old_gs = 0x1111_2222_3333_4444;
    let old_kernel = 0xAAAA_BBBB_CCCC_DDDD;
    let regs = execute_native(2, |regs| {
        regs.gs_base = old_gs;
        regs.kernel_gs_base = old_kernel;
    });
    assert_eq!(regs.gs_base, old_gs);
    assert_eq!(regs.kernel_gs_base, old_kernel);
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_swapgs_cpl_fault_handoff_is_precise_and_noncommitting() {
    let old_gs = 0x1234;
    let old_kernel = 0xFFFF_8000_0000_5678;
    let regs = execute_native(1, |regs| {
        regs.cpl = 3;
        regs.gs_base = old_gs;
        regs.kernel_gs_base = old_kernel;
    });

    assert_eq!(regs.exit_pc, 0x1000);
    assert_eq!(regs.gs_base, old_gs);
    assert_eq!(regs.kernel_gs_base, old_kernel);
    for (index, value) in regs.gpr.iter().enumerate() {
        assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
}
