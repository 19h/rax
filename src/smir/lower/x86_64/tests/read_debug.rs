//! Native x86-64 lowering coverage for MOV-from-debug-register state.

use super::*;
use crate::smir::ir::ops::X86DebugReg;
use crate::smir::lower::{
    X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_DR0_OFFSET,
    X86_GUEST_DR1_OFFSET, X86_GUEST_DR2_OFFSET, X86_GUEST_DR3_OFFSET, X86_GUEST_DR6_OFFSET,
    X86_GUEST_DR7_OFFSET,
};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn kind(dst: VReg, debug: X86DebugReg) -> OpKind {
    OpKind::X86ReadDebug { dst, debug }
}

fn lower_read_debug(op: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
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
fn lower_read_debug_requires_fault_guards_uses_state_and_never_emits_host_mov_dr() {
    assert!(matches!(
        lower_read_debug(kind(x86(X86Reg::Rax), X86DebugReg::Dr0), false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for (debug, offset) in [
        (X86DebugReg::Dr0, X86_GUEST_DR0_OFFSET),
        (X86DebugReg::Dr1, X86_GUEST_DR1_OFFSET),
        (X86DebugReg::Dr2, X86_GUEST_DR2_OFFSET),
        (X86DebugReg::Dr3, X86_GUEST_DR3_OFFSET),
        (X86DebugReg::Dr4, X86_GUEST_DR6_OFFSET),
        (X86DebugReg::Dr5, X86_GUEST_DR7_OFFSET),
        (X86DebugReg::Dr6, X86_GUEST_DR6_OFFSET),
        (X86DebugReg::Dr7, X86_GUEST_DR7_OFFSET),
    ] {
        let (code, _) = lower_read_debug(kind(x86(X86Reg::R15), debug), true)
            .expect("guarded MOV-from-DR lowering");
        assert!(
            !code.windows(2).any(|window| window == [0x0F, 0x21]),
            "guest MOV-from-DR must not execute on the host: {code:02X?}"
        );
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing GuestRegs {debug:?} offset {offset}"
        );
        for guard_offset in [
            X86_GUEST_CR0_OFFSET,
            X86_GUEST_CPL_OFFSET,
            X86_GUEST_DR7_OFFSET,
        ] {
            assert!(
                code.windows(4)
                    .any(|window| window == (guard_offset as u32).to_le_bytes()),
                "missing dynamic-guard offset {guard_offset}"
            );
        }
        assert_eq!(
            code.windows(4)
                .any(|window| window == (X86_GUEST_CR4_OFFSET as u32).to_le_bytes()),
            matches!(debug, X86DebugReg::Dr4 | X86DebugReg::Dr5),
            "only DR4/DR5 require the CR4.DE guard"
        );
        assert!(
            !code.windows(2).any(|window| window == [0x0F, 0xA2]),
            "MOV-from-DR is not serializing"
        );
    }
}

#[test]
fn lower_read_debug_rejects_every_non_lifter_operand_class() {
    for malformed in [
        kind(VReg::virt(0), X86DebugReg::Dr0),
        kind(
            VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(0))),
            X86DebugReg::Dr2,
        ),
        kind(x86(X86Reg::gpr(16)), X86DebugReg::Dr6),
        kind(VReg::Imm(0), X86DebugReg::Dr7),
    ] {
        assert!(!x86_read_debug_shape_valid(&malformed));
        assert!(matches!(
            lower_read_debug(malformed, true),
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
        .expect("lower guarded MOV-from-DR sequence");
    let code = lowerer.finalize().expect("finalize MOV-from-DR sequence");
    let exec = ExecMem::new(&code).expect("map MOV-from-DR sequence");
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
fn native_read_debug_covers_all_selectors_stack_aliases_and_flag_preservation() {
    let ops = [
        (0x1000, kind(x86(X86Reg::Rax), X86DebugReg::Dr0)),
        (0x1003, kind(x86(X86Reg::Rcx), X86DebugReg::Dr1)),
        (0x1006, kind(x86(X86Reg::Rdx), X86DebugReg::Dr2)),
        (0x1009, kind(x86(X86Reg::Rbx), X86DebugReg::Dr3)),
        (0x100C, kind(x86(X86Reg::Rsp), X86DebugReg::Dr4)),
        (0x100F, kind(x86(X86Reg::Rbp), X86DebugReg::Dr5)),
        (0x1012, kind(x86(X86Reg::R14), X86DebugReg::Dr6)),
        (0x1015, kind(x86(X86Reg::R15), X86DebugReg::Dr7)),
    ];
    let regs = execute_native(&ops, |regs| {
        regs.cr0 = 1;
        regs.cpl = 0;
        regs.cr4 = 0;
        regs.dr0 = 0x1111_2222_3333_4444;
        regs.dr1 = 0x2222_3333_4444_5555;
        regs.dr2 = 0x3333_4444_5555_6666;
        regs.dr3 = 0x4444_5555_6666_7777;
        regs.dr6 = 0xFFFF_0FF0;
        regs.dr7 = 0x400;
    });

    assert_eq!(regs.gpr[0], regs.dr0);
    assert_eq!(regs.gpr[1], regs.dr1);
    assert_eq!(regs.gpr[2], regs.dr2);
    assert_eq!(regs.gpr[3], regs.dr3);
    assert_eq!(regs.gpr[4], regs.dr6);
    assert_eq!(regs.gpr[5], regs.dr7);
    assert_eq!(regs.gpr[14], regs.dr6);
    assert_eq!(regs.gpr[15], regs.dr7);
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(regs.ac_flag, 1);
    assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_read_debug_guards_are_dynamic_precise_and_noncommitting() {
    let sentinel = 0xA500_0000_0000_0003;

    let privilege = execute_native(
        &[(0x1234, kind(x86(X86Reg::Rbx), X86DebugReg::Dr2))],
        |regs| {
            regs.cr0 = 1;
            regs.cpl = 3;
            regs.dr2 = 0x2222;
        },
    );
    assert_eq!(privilege.exit_pc, 0x1234);
    assert_eq!(privilege.gpr[3], sentinel);

    let de = execute_native(
        &[(0x2345, kind(x86(X86Reg::Rbx), X86DebugReg::Dr4))],
        |regs| {
            regs.cr0 = 1;
            regs.cpl = 0;
            regs.cr4 = 1 << 3;
            regs.dr6 = 0xFFFF_0FF0;
        },
    );
    assert_eq!(de.exit_pc, 0x2345);
    assert_eq!(de.gpr[3], sentinel);

    let general_detect = execute_native(
        &[(0x3456, kind(x86(X86Reg::Rbx), X86DebugReg::Dr0))],
        |regs| {
            regs.cr0 = 1;
            regs.cpl = 0;
            regs.dr0 = 0x1111;
            regs.dr6 = 0x400;
            regs.dr7 = 1 << 13;
        },
    );
    assert_eq!(general_detect.exit_pc, 0x3456);
    assert_eq!(general_detect.gpr[3], sentinel);
    assert_eq!(general_detect.dr6, 0x400, "native guard does not set BD");
    assert_eq!(
        general_detect.dr7,
        1 << 13,
        "native guard does not clear GD"
    );

    let real_mode = execute_native(
        &[(0x4567, kind(x86(X86Reg::Rbx), X86DebugReg::Dr2))],
        |regs| {
            regs.cr0 = 0;
            regs.cpl = 3;
            regs.dr2 = 0x2222;
        },
    );
    assert_eq!(real_mode.gpr[3], 0x2222);
    assert_eq!(real_mode.exit_pc, 0xDEAD_BEEF);
}
