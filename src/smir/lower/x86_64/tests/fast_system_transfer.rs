//! Fault-precise terminal native lowering for Intel SYSENTER/SYSEXIT.

use super::*;
use crate::smir::ir::ops::{X86FastSystemTransferKind, X86FastSystemTransferOp};
use crate::smir::lower::X86_GUEST_FAST_SYSTEM_TRANSFER_FN_OFFSET;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn transfer(kind: X86FastSystemTransferKind, operand64: bool, next_pc: u64) -> OpKind {
    OpKind::X86FastSystemTransfer(X86FastSystemTransferOp {
        kind,
        target: x86(X86Reg::Rip),
        stack_pointer: x86(X86Reg::Rsp),
        return_target: x86(X86Reg::Rdx),
        return_stack_pointer: x86(X86Reg::Rcx),
        operand64,
        next_pc,
    })
}

fn function(kind: OpKind, terminal_target: VReg) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::IndirectBranch {
        target: terminal_target,
        possible_targets: vec![],
    });
    builder.finish()
}

fn lower(function: &SmirFunction, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(function)?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_fast_system_transfer_requires_guards_calls_helper_and_owns_both_exits() {
    for (kind, operand64, next_pc) in [
        (X86FastSystemTransferKind::Sysenter, false, 0x1002),
        (X86FastSystemTransferKind::Sysexit, false, 0x1002),
        (X86FastSystemTransferKind::Sysexit, true, 0x1003),
    ] {
        let function = function(transfer(kind, operand64, next_pc), x86(X86Reg::Rip));
        let unguarded = lower(&function, false);
        assert!(
            matches!(unguarded, Err(LowerError::UnsupportedOp { .. })),
            "unexpected unguarded result: {unguarded:?}"
        );
        let (code, _) = lower(&function, true).expect("guarded fast-system-transfer lowering");
        assert!(
            code.windows(4).any(|window| {
                window == (X86_GUEST_FAST_SYSTEM_TRANSFER_FN_OFFSET as u32).to_le_bytes()
            }),
            "missing helper offset: {code:02X?}"
        );
        assert!(
            code.windows(2).any(|window| window == [0x0F, 0xA2]),
            "successful transfer must serialize: {code:02X?}"
        );
        assert!(
            code.windows(4)
                .any(|window| window == 0x1000_u32.to_le_bytes()),
            "failure must replay the exact source PC: {code:02X?}"
        );
    }
}

#[test]
fn lower_fast_system_transfer_rejects_every_non_lifter_shape_and_hint() {
    let mut malformed = Vec::new();
    for (kind, operand64, next_pc) in [
        (X86FastSystemTransferKind::Sysenter, true, 0x1002),
        (X86FastSystemTransferKind::Sysenter, false, 0x1001),
        (X86FastSystemTransferKind::Sysenter, false, 0x1010),
        (X86FastSystemTransferKind::Sysexit, false, 0x0FFF),
    ] {
        malformed.push(transfer(kind, operand64, next_pc));
    }
    for (field, value) in [
        (0, x86(X86Reg::Rbx)),
        (1, x86(X86Reg::Rbp)),
        (2, x86(X86Reg::Rax)),
        (3, x86(X86Reg::Rsi)),
    ] {
        let mut kind = transfer(X86FastSystemTransferKind::Sysexit, true, 0x1003);
        let OpKind::X86FastSystemTransfer(op) = &mut kind else {
            unreachable!()
        };
        match field {
            0 => op.target = value,
            1 => op.stack_pointer = value,
            2 => op.return_target = value,
            3 => op.return_stack_pointer = value,
            _ => unreachable!(),
        }
        malformed.push(kind);
    }
    for kind in malformed {
        let result = lower(&function(kind, x86(X86Reg::Rip)), true);
        assert!(
            matches!(result, Err(LowerError::InvalidOperand { .. })),
            "unexpected malformed result: {result:?}"
        );
    }

    let mut hinted = function(
        transfer(X86FastSystemTransferKind::Sysenter, false, 0x1002),
        x86(X86Reg::Rip),
    );
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(matches!(
        lower(&hinted, true),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[test]
fn lower_fast_system_transfer_rejects_nonterminal_mismatched_and_duplicate_ownership() {
    let exact = transfer(X86FastSystemTransferKind::Sysenter, false, 0x1002);

    let wrong_terminal = function(exact.clone(), x86(X86Reg::Rbx));
    let result = lower(&wrong_terminal, true);
    assert!(
        matches!(result, Err(LowerError::InvalidOperand { .. })),
        "unexpected mismatched-terminal result: {result:?}"
    );

    let mut annotated = function(exact.clone(), x86(X86Reg::Rip));
    let Terminator::IndirectBranch {
        possible_targets, ..
    } = &mut annotated.blocks[0].terminator
    else {
        unreachable!()
    };
    possible_targets.push(BlockId(1));
    assert!(matches!(
        lower(&annotated, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    for extra in [OpKind::Nop, exact.clone()] {
        let mut nonterminal = function(exact.clone(), x86(X86Reg::Rip));
        nonterminal.blocks[0]
            .ops
            .push(crate::smir::ir::ops::SmirOp::new(
                crate::smir::ir::types::OpId(1),
                0x1002,
                extra,
            ));
        assert!(matches!(
            lower(&nonterminal, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct HelperContext {
    calls: u64,
    kind: u64,
    operand64: u64,
    ok: bool,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    kind: u64,
    operand64: u64,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut HelperContext) };
    context.calls += 1;
    context.kind = kind;
    context.operand64 = operand64;
    if !context.ok {
        return 0;
    }
    state.gpr[4] = 0xFFFF_8000_0000_8000;
    state.exit_pc = 0xFFFF_8000_0000_6000;
    state.cpl = 3;
    state.cs_l = operand64;
    state.interrupt_flags = 0x200;
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native(
    kind: X86FastSystemTransferKind,
    operand64: bool,
    ok: bool,
) -> (crate::smir::lower::runtime::GuestRegs, HelperContext) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let next_pc = 0x1000 + if operand64 { 3 } else { 2 };
    let function = function(transfer(kind, operand64, next_pc), x86(X86Reg::Rip));
    let (code, entry) = lower(&function, true).expect("lower executable transfer");
    let exec = ExecMem::new(&code).expect("map executable transfer");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.rflags = 0x2 | 0x08C5 | (1 << 10);
    state.ac_flag = 1;
    state.interrupt_flags = 0x003A_3200;
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.fast_system_transfer_fn = helper as usize as u64;
    let mut context = HelperContext {
        ok,
        ..HelperContext::default()
    };
    state.ctx = (&mut context as *mut HelperContext) as u64;
    exec.run(entry, &mut state);
    (state, context)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_fast_system_transfer_passes_exact_discriminator_and_hands_off_dynamic_state() {
    for (kind, operand64, expected_kind) in [
        (X86FastSystemTransferKind::Sysenter, false, 0),
        (X86FastSystemTransferKind::Sysexit, false, 1),
        (X86FastSystemTransferKind::Sysexit, true, 1),
    ] {
        let (state, context) = execute_native(kind, operand64, true);
        assert_eq!(context.calls, 1);
        assert_eq!(context.kind, expected_kind);
        assert_eq!(context.operand64, u64::from(operand64));
        assert_eq!(state.gpr[4], 0xFFFF_8000_0000_8000);
        assert_eq!(state.exit_pc, 0xFFFF_8000_0000_6000);
        assert_eq!(state.cpl, 3);
        assert_eq!(state.cs_l, u64::from(operand64));
        assert_eq!(state.interrupt_flags, 0x200);
        for (index, actual) in state.gpr.iter().enumerate() {
            if index != 4 {
                assert_eq!(*actual, 0xA500_0000_0000_0000 | index as u64);
            }
        }
        assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
        assert_eq!(state.ac_flag, 1);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_fast_system_transfer_failure_restores_scalar_state_and_restarts_exactly() {
    let (state, context) = execute_native(X86FastSystemTransferKind::Sysexit, true, false);
    assert_eq!(context.calls, 1);
    assert_eq!(context.kind, 1);
    assert_eq!(context.operand64, 1);
    assert_eq!(state.exit_pc, 0x1000);
    for (index, actual) in state.gpr.iter().enumerate() {
        assert_eq!(*actual, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(state.interrupt_flags, 0x003A_3200);
    assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
    assert_eq!(state.ac_flag, 1);
}
