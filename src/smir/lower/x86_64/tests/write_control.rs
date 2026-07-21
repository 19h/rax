//! Native x86-64 lowering coverage for MOV-to-control-register state.

use super::*;
use crate::smir::ir::ops::X86ControlReg;
use crate::smir::lower::X86_GUEST_CONTROL_WRITE_FN_OFFSET;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn kind(src: VReg, control: X86ControlReg, next_pc: u64) -> OpKind {
    OpKind::X86WriteControl {
        src,
        control,
        next_pc,
    }
}

fn lower_write_control(op: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
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
fn lower_write_control_requires_guards_calls_helper_and_serializes_selectively() {
    assert!(matches!(
        lower_write_control(kind(x86(X86Reg::Rax), X86ControlReg::Cr0, 0x1003), false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for source in [X86Reg::R15, X86Reg::R31] {
        for control in [
            X86ControlReg::Cr0,
            X86ControlReg::Cr2,
            X86ControlReg::Cr3,
            X86ControlReg::Cr4,
            X86ControlReg::Cr8,
        ] {
            let (code, _) = lower_write_control(kind(x86(source), control, 0x1004), true)
                .expect("guarded MOV-to-CR lowering");
            assert!(
                !code.windows(2).any(|window| window == [0x0F, 0x22]),
                "guest MOV-to-CR must not execute on the host: {code:02X?}"
            );
            assert!(
                code.windows(4).any(|window| {
                    window == (X86_GUEST_CONTROL_WRITE_FN_OFFSET as u32).to_le_bytes()
                }),
                "missing canonical helper offset"
            );
            assert_eq!(
                code.windows(2).any(|window| window == [0x0F, 0xA2]),
                !matches!(control, X86ControlReg::Cr8),
                "CR0/2/3/4 writes serialize; CR8 does not"
            );
            assert!(
                code.windows(4)
                    .any(|window| window == 0x1000u32.to_le_bytes()),
                "fault exit must retain the original PC"
            );
            assert!(
                code.windows(4)
                    .any(|window| window == 0x1004u32.to_le_bytes()),
                "success exit must use the encoded next PC"
            );
        }
    }
}

#[test]
fn lower_write_control_rejects_every_non_lifter_operand_and_frontier_shape() {
    for malformed in [
        kind(VReg::virt(0), X86ControlReg::Cr0, 0x1003),
        kind(
            VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(0))),
            X86ControlReg::Cr2,
            0x1003,
        ),
        kind(x86(X86Reg::R16), X86ControlReg::Cr3, 0x1003),
        kind(VReg::Imm(0), X86ControlReg::Cr4, 0x1003),
        kind(x86(X86Reg::Rax), X86ControlReg::Cr8, 0x1002),
        kind(x86(X86Reg::Rax), X86ControlReg::Cr8, 0x1010),
        kind(x86(X86Reg::Rax), X86ControlReg::Cr8, 0x0FFF),
    ] {
        assert!(matches!(
            lower_write_control(malformed, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind(x86(X86Reg::Rax), X86ControlReg::Cr0, 0x1003));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn write_control_stub(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    control: u64,
    value: u64,
) -> u64 {
    const REJECT: u64 = 0xBAD0_BAD0_BAD0_BAD0;
    if value == REJECT {
        return 0;
    }
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    match control {
        0 => state.cr0 = value,
        2 => state.cr2 = value,
        3 => state.cr3 = value,
        4 => state.cr4 = value,
        8 => state.cr8 = value,
        _ => return 0,
    }
    1
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
        .expect("lower helper-backed MOV-to-CR sequence");
    let code = lowerer.finalize().expect("finalize MOV-to-CR sequence");
    let exec = ExecMem::new(&code).expect("map MOV-to-CR sequence");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF;
    regs.control_write_fn = write_control_stub as usize as u64;
    configure(&mut regs);
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_write_control_uses_all_selectors_stack_sources_and_exact_success_frontiers() {
    let cases = [
        (X86ControlReg::Cr0, 0, 0x1111_2222_3333_4444),
        (X86ControlReg::Cr2, 4, 0x2222_3333_4444_5555),
        (X86ControlReg::Cr3, 5, 0x3333_4444_5555_6666),
        (X86ControlReg::Cr4, 14, 0x4444_5555_6666_7777),
        (X86ControlReg::Cr8, 15, 0xD),
        (X86ControlReg::Cr2, 16, 0x1616_2222_3333_4444),
        (X86ControlReg::Cr8, 31, 0xF),
    ];

    for (control, source, value) in cases {
        let regs = execute_native(
            &[(0x1000, kind(x86(X86Reg::gpr(source)), control, 0x1004))],
            |regs| regs.gpr[usize::from(source)] = value,
        );
        let actual = match control {
            X86ControlReg::Cr0 => regs.cr0,
            X86ControlReg::Cr2 => regs.cr2,
            X86ControlReg::Cr3 => regs.cr3,
            X86ControlReg::Cr4 => regs.cr4,
            X86ControlReg::Cr8 => regs.cr8,
        };
        assert_eq!(actual, value, "{control:?} from GPR {source}");
        assert_eq!(regs.gpr[usize::from(source)], value, "source preserved");
        assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
        assert_eq!(regs.ac_flag, 1);
        assert_eq!(regs.exit_pc, 0x1004);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_write_control_failure_is_noncommitting_and_restarts_exactly() {
    const REJECT: u64 = 0xBAD0_BAD0_BAD0_BAD0;
    let regs = execute_native(
        &[(0x2345, kind(x86(X86Reg::Rbp), X86ControlReg::Cr3, 0x2348))],
        |regs| {
            regs.gpr[5] = REJECT;
            regs.cr3 = 0x1234_5000;
        },
    );
    assert_eq!(regs.exit_pc, 0x2345);
    assert_eq!(regs.cr3, 0x1234_5000);
    assert_eq!(regs.gpr[5], REJECT);
    assert_eq!(regs.rflags & (0x08D5 | (1 << 10)), 0x08D5 | (1 << 10));
    assert_eq!(regs.ac_flag, 1);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_write_control_success_ends_region_before_later_control_ops() {
    let regs = execute_native(
        &[
            (0x1000, kind(x86(X86Reg::Rax), X86ControlReg::Cr2, 0x1003)),
            (0x1003, kind(x86(X86Reg::Rbx), X86ControlReg::Cr8, 0x1006)),
        ],
        |regs| {
            regs.gpr[0] = 0x2222;
            regs.gpr[3] = 7;
            regs.cr2 = 0x1111;
            regs.cr8 = 1;
        },
    );
    assert_eq!(regs.exit_pc, 0x1003);
    assert_eq!(regs.cr2, 0x2222, "first write commits");
    assert_eq!(regs.cr8, 1, "later native op is unreachable");
}
