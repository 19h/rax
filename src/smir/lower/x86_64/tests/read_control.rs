//! Native x86-64 lowering coverage for MOV-from-control-register state.

use super::*;
use crate::smir::ir::ops::X86ControlReg;
use crate::smir::lower::{X86_GUEST_CR2_OFFSET, X86_GUEST_CR3_OFFSET, X86_GUEST_CR8_OFFSET};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn kind(dst: VReg, control: X86ControlReg) -> OpKind {
    OpKind::X86ReadControl { dst, control }
}

fn lower_read_control(op: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, op);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_read_control_requires_fault_guards_uses_state_and_never_emits_host_mov_cr() {
    assert!(matches!(
        lower_read_control(kind(x86(X86Reg::Rax), X86ControlReg::Cr0), false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for (control, offset) in [
        (X86ControlReg::Cr0, X86_GUEST_CR0_OFFSET),
        (X86ControlReg::Cr2, X86_GUEST_CR2_OFFSET),
        (X86ControlReg::Cr3, X86_GUEST_CR3_OFFSET),
        (X86ControlReg::Cr4, X86_GUEST_CR4_OFFSET),
        (X86ControlReg::Cr8, X86_GUEST_CR8_OFFSET),
    ] {
        let (code, _) = lower_read_control(kind(x86(X86Reg::R15), control), true)
            .expect("guarded MOV-from-CR lowering");
        assert!(
            !code.windows(2).any(|window| window == [0x0F, 0x20]),
            "guest MOV-from-CR must not execute on the host: {code:02X?}"
        );
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing GuestRegs {control:?} offset {offset}"
        );
        for guard_offset in [X86_GUEST_CR0_OFFSET, X86_GUEST_CPL_OFFSET] {
            assert!(
                code.windows(4)
                    .any(|window| window == (guard_offset as u32).to_le_bytes()),
                "missing privilege-guard offset {guard_offset}"
            );
        }
        let has_cpuid = code.windows(2).any(|window| window == [0x0F, 0xA2]);
        assert_eq!(
            has_cpuid,
            !matches!(control, X86ControlReg::Cr8),
            "CR0/2/3/4 reads serialize; CR8 does not"
        );
    }
}

#[test]
fn lower_read_control_rejects_every_non_lifter_operand_class() {
    for malformed in [
        kind(VReg::virt(0), X86ControlReg::Cr0),
        kind(
            VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(0))),
            X86ControlReg::Cr2,
        ),
        kind(x86(X86Reg::gpr(16)), X86ControlReg::Cr3),
        kind(VReg::Imm(0), X86ControlReg::Cr4),
    ] {
        assert!(!x86_read_control_shape_valid(&malformed));
        assert!(matches!(
            lower_read_control(malformed, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    ops: &[(u64, OpKind)],
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (pc, op) in ops {
        builder.push_op(*pc, op.clone());
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower guarded MOV-from-CR sequence");
    let code = lowerer.finalize().expect("finalize MOV-from-CR sequence");
    let exec = ExecMem::new(&code).expect("map MOV-from-CR sequence");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF;
    configure(&mut regs);
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_read_control_covers_all_registers_stack_aliases_and_flag_preservation() {
    let ops = [
        (0x1000, kind(x86(X86Reg::Rax), X86ControlReg::Cr0)),
        (0x1003, kind(x86(X86Reg::Rsp), X86ControlReg::Cr2)),
        (0x1006, kind(x86(X86Reg::Rbp), X86ControlReg::Cr3)),
        (0x1009, kind(x86(X86Reg::R14), X86ControlReg::Cr4)),
        (0x100C, kind(x86(X86Reg::R15), X86ControlReg::Cr8)),
    ];
    let regs = execute_native(&ops, |regs| {
        regs.cr0 = 0x8005_0033;
        regs.cr2 = 0x2222_3333_4444_5555;
        regs.cr3 = 0x0000_1234_5000_0ABC;
        regs.cr4 = 0x0000_0000_0044_06F0;
        regs.cr8 = 0xD;
        regs.cpl = 0;
    });

    assert_eq!(regs.gpr[0], regs.cr0);
    assert_eq!(regs.gpr[4], regs.cr2);
    assert_eq!(regs.gpr[5], regs.cr3);
    assert_eq!(regs.gpr[14], regs.cr4);
    assert_eq!(regs.gpr[15], regs.cr8);
    assert_eq!(regs.gpr[3], 0xA500_0000_0000_0003);
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(regs.ac_flag, 1);
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_read_control_privilege_guard_is_dynamic_precise_and_noncommitting() {
    let op = [(0x1234, kind(x86(X86Reg::Rbx), X86ControlReg::Cr2))];
    let fault = execute_native(&op, |regs| {
        regs.cr0 = 1;
        regs.cr2 = 0x2222;
        regs.cpl = 3;
    });
    assert_eq!(fault.exit_pc, 0x1234);
    assert_eq!(fault.gpr[3], 0xA500_0000_0000_0003);
    assert_eq!(fault.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));

    let real_mode = execute_native(&op, |regs| {
        regs.cr0 = 0;
        regs.cr2 = 0x2222;
        regs.cpl = 3;
    });
    assert_eq!(real_mode.gpr[3], 0x2222);
    assert_eq!(real_mode.exit_pc, 0xDEAD_BEEF);
}
