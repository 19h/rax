//! Native x86-64 lowering coverage for MOV-to-debug-register state.

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

fn kind(src: VReg, debug: X86DebugReg) -> OpKind {
    OpKind::X86WriteDebug { src, debug }
}

fn lower_write_debug(op: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
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
fn lower_write_debug_requires_guards_uses_state_and_serializes_without_host_mov_dr() {
    assert!(matches!(
        lower_write_debug(kind(x86(X86Reg::Rax), X86DebugReg::Dr0), false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for source in [X86Reg::R15, X86Reg::R31] {
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
            let (code, _) = lower_write_debug(kind(x86(source), debug), true)
                .expect("guarded MOV-to-DR lowering");
            assert!(
                !code.windows(2).any(|window| window == [0x0F, 0x23]),
                "guest MOV-to-DR must not execute on the host: {code:02X?}"
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
            assert_eq!(
                code.windows(4)
                    .any(|window| window == [0x48, 0xC1, 0xE9, 0x20]),
                matches!(
                    debug,
                    X86DebugReg::Dr4 | X86DebugReg::Dr5 | X86DebugReg::Dr6 | X86DebugReg::Dr7
                ),
                "only effective DR6/DR7 require the high-half guard"
            );
            assert!(
                code.windows(2).any(|window| window == [0x0F, 0xA2]),
                "MOV-to-DR must emit a portable serializing barrier"
            );
        }
    }
}

#[test]
fn lower_write_debug_rejects_every_non_lifter_operand_class() {
    for malformed in [
        kind(VReg::virt(0), X86DebugReg::Dr0),
        kind(
            VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(0))),
            X86DebugReg::Dr2,
        ),
        kind(VReg::Imm(0), X86DebugReg::Dr7),
    ] {
        assert!(!x86_write_debug_shape_valid(&malformed));
        assert!(matches!(
            lower_write_debug(malformed, true),
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
        .expect("lower guarded MOV-to-DR sequence");
    let code = lowerer.finalize().expect("finalize MOV-to-DR sequence");
    let exec = ExecMem::new(&code).expect("map MOV-to-DR sequence");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF;
    regs.cr0 = 1;
    regs.cpl = 0;
    regs.cr4 = 0;
    regs.dr6 = 0x400;
    regs.dr7 = 0x400;
    configure(&mut regs);
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_write_debug_covers_all_selectors_stack_sources_and_flag_preservation() {
    let cases = [
        (X86DebugReg::Dr0, 0, 0x1111_2222_3333_4444),
        (X86DebugReg::Dr1, 1, 0x2222_3333_4444_5555),
        (X86DebugReg::Dr2, 2, 0x3333_4444_5555_6666),
        (X86DebugReg::Dr3, 3, 0x4444_5555_6666_7777),
        (X86DebugReg::Dr4, 4, 0x0000_0000_FFFF_0FF0),
        (X86DebugReg::Dr5, 5, 0x0000_0000_0000_0400),
        (X86DebugReg::Dr6, 14, 0x0000_0000_1234_5678),
        (X86DebugReg::Dr7, 15, 0x0000_0000_0000_0400),
        (X86DebugReg::Dr0, 16, 0x1616_2222_3333_4444),
        (X86DebugReg::Dr3, 31, 0x3131_2222_3333_4444),
    ];

    for (debug, source, value) in cases {
        let regs = execute_native(&[(0x1000, kind(x86(X86Reg::gpr(source)), debug))], |regs| {
            regs.gpr[usize::from(source)] = value
        });
        let actual = match debug {
            X86DebugReg::Dr0 => regs.dr0,
            X86DebugReg::Dr1 => regs.dr1,
            X86DebugReg::Dr2 => regs.dr2,
            X86DebugReg::Dr3 => regs.dr3,
            X86DebugReg::Dr4 | X86DebugReg::Dr6 => regs.dr6,
            X86DebugReg::Dr5 | X86DebugReg::Dr7 => regs.dr7,
        };
        assert_eq!(actual, value, "{debug:?} from GPR {source}");
        assert_eq!(regs.gpr[usize::from(source)], value, "source preserved");
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(regs.ac_flag, 1);
        assert_eq!(regs.exit_pc, 0xDEAD_BEEF);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_write_debug_guards_are_dynamic_precise_and_noncommitting() {
    let privilege = execute_native(
        &[(0x1234, kind(x86(X86Reg::Rbx), X86DebugReg::Dr2))],
        |regs| {
            regs.cpl = 3;
            regs.gpr[3] = 0x2222;
            regs.dr2 = 0x3333;
        },
    );
    assert_eq!(privilege.exit_pc, 0x1234);
    assert_eq!(privilege.dr2, 0x3333);

    let de = execute_native(
        &[(0x2345, kind(x86(X86Reg::Rbx), X86DebugReg::Dr4))],
        |regs| {
            regs.cr4 = 1 << 3;
            regs.gpr[3] = 0x2222;
            regs.dr6 = 0x400;
        },
    );
    assert_eq!(de.exit_pc, 0x2345);
    assert_eq!(de.dr6, 0x400);

    let general_detect = execute_native(
        &[(0x3456, kind(x86(X86Reg::Rbx), X86DebugReg::Dr0))],
        |regs| {
            regs.gpr[3] = 0x2222;
            regs.dr0 = 0x1111;
            regs.dr6 = 0x400;
            regs.dr7 = 1 << 13;
        },
    );
    assert_eq!(general_detect.exit_pc, 0x3456);
    assert_eq!(general_detect.dr0, 0x1111);
    assert_eq!(general_detect.dr6, 0x400, "native guard does not set BD");
    assert_eq!(
        general_detect.dr7,
        1 << 13,
        "native guard does not clear GD"
    );

    for debug in [
        X86DebugReg::Dr4,
        X86DebugReg::Dr5,
        X86DebugReg::Dr6,
        X86DebugReg::Dr7,
    ] {
        let high = execute_native(&[(0x4567, kind(x86(X86Reg::Rbx), debug))], |regs| {
            regs.gpr[3] = 0x0000_0001_0000_0000
        });
        assert_eq!(high.exit_pc, 0x4567, "{debug:?}");
        assert_eq!(high.dr6, 0x400, "{debug:?}");
        assert_eq!(high.dr7, 0x400, "{debug:?}");
    }

    let real_mode = execute_native(
        &[(0x5678, kind(x86(X86Reg::Rbx), X86DebugReg::Dr2))],
        |regs| {
            regs.cr0 = 0;
            regs.cpl = 3;
            regs.gpr[3] = 0x2222;
        },
    );
    assert_eq!(real_mode.dr2, 0x2222);
    assert_eq!(real_mode.exit_pc, 0xDEAD_BEEF);

    let high_dr0 = execute_native(
        &[(0x6789, kind(x86(X86Reg::Rbx), X86DebugReg::Dr0))],
        |regs| regs.gpr[3] = 0xFFFF_FFFF_0000_0000,
    );
    assert_eq!(high_dr0.dr0, 0xFFFF_FFFF_0000_0000);
    assert_eq!(high_dr0.exit_pc, 0xDEAD_BEEF);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_write_debug_new_gd_value_guards_the_next_access_at_its_frontier() {
    let regs = execute_native(
        &[
            (0x1000, kind(x86(X86Reg::Rax), X86DebugReg::Dr7)),
            (0x1003, kind(x86(X86Reg::Rbx), X86DebugReg::Dr0)),
        ],
        |regs| {
            regs.gpr[0] = 1 << 13;
            regs.gpr[3] = 0x2222;
            regs.dr0 = 0x1111;
            regs.dr6 = 0x400;
            regs.dr7 = 0x400;
        },
    );

    assert_eq!(regs.exit_pc, 0x1003);
    assert_eq!(regs.dr7, 1 << 13, "first write commits before the frontier");
    assert_eq!(regs.dr0, 0x1111, "second write does not commit");
    assert_eq!(regs.dr6, 0x400, "direct replay owns DR6.BD");
    assert_eq!(regs.gpr[0], 1 << 13);
    assert_eq!(regs.gpr[3], 0x2222);
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
}
