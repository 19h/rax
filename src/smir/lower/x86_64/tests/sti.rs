//! Fault-precise, state-backed native lowering coverage for x86 STI.

use super::*;
use crate::isa::x86_64::execute::system::{X86StiEffect, X86StiState, evaluate_x86_sti};
use crate::isa::x86_64::flags;
use crate::smir::lower::X86_GUEST_STI_FN_OFFSET;

fn kind(requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Sti {
        requires_apx,
        next_pc,
    }
}

fn lower_sti(op: OpKind, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
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
fn lower_sti_requires_guards_calls_helper_and_encodes_both_precise_frontiers() {
    assert!(matches!(
        lower_sti(kind(false, 0x1001), false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    for (requires_apx, next_pc) in [(false, 0x1001), (true, 0x1003)] {
        let (code, _) = lower_sti(kind(requires_apx, next_pc), true)
            .expect("guarded helper-backed STI lowering");
        assert!(
            code.windows(4)
                .any(|window| window == (X86_GUEST_STI_FN_OFFSET as u32).to_le_bytes()),
            "missing STI helper offset: {code:02X?}"
        );
        assert!(
            code.windows(4)
                .any(|window| window == 0x1000_u32.to_le_bytes()),
            "fault exit must retain the original PC"
        );
        assert!(
            code.windows(4)
                .any(|window| window == (next_pc as u32).to_le_bytes()),
            "success exit must use the exact next PC"
        );
    }
}

#[test]
fn lower_sti_rejects_every_non_lifter_shape() {
    for malformed in [
        kind(false, 0x1000),
        kind(false, 0x1010),
        kind(false, 0x0FFF),
        kind(true, 0x1002),
        kind(true, 0x1010),
    ] {
        assert!(matches!(
            lower_sti(malformed, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind(false, 0x1001));
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
unsafe extern "C" fn sti_stub(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    requires_apx: u64,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if requires_apx != 0 && state.apx_enabled == 0 {
        return 0;
    }
    let Ok(cpl) = u8::try_from(state.cpl) else {
        return 0;
    };
    let Ok(effect) = evaluate_x86_sti(X86StiState {
        cr0: state.cr0,
        cr4: state.cr4,
        rflags: state.interrupt_flags,
        cpl,
    }) else {
        return 0;
    };
    match effect {
        X86StiEffect::SetIf { inhibit_interrupts } => {
            state.interrupt_flags |= flags::bits::IF;
            state.interrupt_inhibit = u64::from(inhibit_interrupts);
        }
        X86StiEffect::SetVif => {
            state.interrupt_flags |= flags::bits::VIF;
            state.interrupt_inhibit = 0;
        }
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
        .expect("lower helper-backed STI sequence");
    let code = lowerer.finalize().expect("finalize STI sequence");
    let exec = ExecMem::new(&code).expect("map STI sequence");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    regs.ac_flag = 1;
    regs.interrupt_flags = flags::bits::VIP;
    regs.exit_pc = 0xDEAD_BEEF;
    regs.sti_fn = sti_stub as usize as u64;
    configure(&mut regs);
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_sti_commits_exact_if_or_vif_shadow_and_preserves_scalar_state() {
    struct Case {
        name: &'static str,
        requires_apx: bool,
        initial_interrupt: u64,
        configure: fn(&mut crate::smir::lower::runtime::GuestRegs),
        set: u64,
        inhibit: u64,
    }
    let cases = [
        Case {
            name: "real-if-zero",
            requires_apx: false,
            initial_interrupt: 0,
            configure: |regs| {
                regs.cr0 = 0;
                regs.cpl = 3;
            },
            set: flags::bits::IF,
            inhibit: 1,
        },
        Case {
            name: "real-if-one",
            requires_apx: false,
            initial_interrupt: flags::bits::IF,
            configure: |regs| {
                regs.cr0 = 0;
                regs.cpl = 3;
            },
            set: flags::bits::IF,
            inhibit: 0,
        },
        Case {
            name: "protected-cpl0",
            requires_apx: false,
            initial_interrupt: 0,
            configure: |regs| {
                regs.cr0 = 1;
                regs.cpl = 0;
            },
            set: flags::bits::IF,
            inhibit: 1,
        },
        Case {
            name: "protected-pvi",
            requires_apx: false,
            initial_interrupt: 0,
            configure: |regs| {
                regs.cr0 = 1;
                regs.cr4 = 1 << 1;
                regs.cpl = 3;
            },
            set: flags::bits::VIF,
            inhibit: 0,
        },
        Case {
            name: "virtual-8086-vme",
            requires_apx: false,
            initial_interrupt: flags::bits::VM,
            configure: |regs| {
                regs.cr0 = 1;
                regs.cr4 = 1;
                regs.cpl = 3;
            },
            set: flags::bits::VIF,
            inhibit: 0,
        },
        Case {
            name: "rex2-apx",
            requires_apx: true,
            initial_interrupt: 0,
            configure: |regs| {
                regs.cr0 = 1;
                regs.cpl = 0;
                regs.apx_enabled = 1;
            },
            set: flags::bits::IF,
            inhibit: 1,
        },
    ];

    for case in cases {
        let regs = execute_native(
            &[(
                0x1000,
                kind(
                    case.requires_apx,
                    0x1000 + if case.requires_apx { 3 } else { 1 },
                ),
            )],
            |regs| {
                regs.interrupt_flags = case.initial_interrupt;
                (case.configure)(regs);
            },
        );
        assert_eq!(
            regs.interrupt_flags,
            case.initial_interrupt | case.set,
            "{}",
            case.name
        );
        assert_eq!(regs.interrupt_inhibit, case.inhibit, "{}", case.name);
        assert_eq!(
            regs.exit_pc,
            0x1000 + if case.requires_apx { 3 } else { 1 },
            "{}",
            case.name
        );
        for (index, actual) in regs.gpr.iter().enumerate() {
            assert_eq!(
                *actual,
                0xA500_0000_0000_0000 | index as u64,
                "{}",
                case.name
            );
        }
        assert_eq!(
            regs.rflags & (0x08D5 | flags::bits::DF),
            0x08D5 | flags::bits::DF
        );
        assert_eq!(regs.ac_flag, 1);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_sti_failure_is_precise_noncommitting_and_apx_precedes_privilege() {
    for (name, requires_apx, configure) in [
        (
            "general-protection",
            false,
            (|regs: &mut crate::smir::lower::runtime::GuestRegs| {
                regs.cr0 = 1;
                regs.cpl = 3;
            }) as fn(&mut crate::smir::lower::runtime::GuestRegs),
        ),
        (
            "pvi-vip",
            false,
            |regs: &mut crate::smir::lower::runtime::GuestRegs| {
                regs.cr0 = 1;
                regs.cr4 = 1 << 1;
                regs.cpl = 3;
                regs.interrupt_flags |= flags::bits::VIP;
            },
        ),
        (
            "apx-before-general-protection",
            true,
            |regs: &mut crate::smir::lower::runtime::GuestRegs| {
                regs.cr0 = 1;
                regs.cpl = 3;
                regs.apx_enabled = 0;
            },
        ),
    ] {
        let initial = flags::bits::VIP;
        let regs = execute_native(&[(0x2345, kind(requires_apx, 0x2348))], |regs| {
            regs.interrupt_flags = initial;
            regs.interrupt_inhibit = 0xA5;
            configure(regs);
        });
        assert_eq!(regs.exit_pc, 0x2345, "{name}");
        assert_eq!(regs.interrupt_flags, initial, "{name}");
        assert_eq!(regs.interrupt_inhibit, 0xA5, "{name}");
        for (index, actual) in regs.gpr.iter().enumerate() {
            assert_eq!(*actual, 0xA500_0000_0000_0000 | index as u64, "{name}");
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_sti_success_exits_before_any_following_native_operation() {
    let regs = execute_native(
        &[
            (0x1000, kind(false, 0x1001)),
            (
                0x1001,
                OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                    src: SrcOperand::Imm(0x1234),
                    width: OpWidth::W64,
                },
            ),
        ],
        |regs| {
            regs.cr0 = 0;
            regs.interrupt_flags = 0;
        },
    );
    assert_eq!(regs.exit_pc, 0x1001);
    assert_eq!(regs.gpr[0], 0xA500_0000_0000_0000);
    assert_eq!(regs.interrupt_flags & flags::bits::IF, flags::bits::IF);
    assert_eq!(regs.interrupt_inhibit, 1);
}
