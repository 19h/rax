//! Fault-precise helper-backed native lowering for far RET (`CA`/`CB`).

use super::*;
use crate::smir::ir::ops::X86FarReturnOp;
use crate::smir::lower::X86_GUEST_FAR_RETURN_FN_OFFSET;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn far_return(width: OpWidth, pop_bytes: u16, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86FarReturn(X86FarReturnOp {
        target: x86(X86Reg::Rip),
        offset_width: width,
        pop_bytes,
        requires_apx,
        next_pc,
    })
}

fn lower(
    kind: OpKind,
    mem_helpers: bool,
    fault_guards: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::IndirectBranch {
        target: x86(X86Reg::Rip),
        possible_targets: vec![],
    });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_far_return_requires_guards_helpers_serializes_and_calls_guest_helper() {
    let op = far_return(OpWidth::W64, 0x1234, true, 0x1005);
    assert!(matches!(
        lower(op.clone(), true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(op.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (code, _) = lower(op, true, true).expect("guarded far-RET lowering");
    assert!(
        code.windows(4)
            .any(|window| window == (X86_GUEST_FAR_RETURN_FN_OFFSET as u32).to_le_bytes()),
        "missing far-RET helper offset: {code:02X?}"
    );
    assert!(code.windows(2).any(|window| window == [0x0F, 0xA2]));
    assert!(
        code.windows(4)
            .any(|window| window == 0x1234_0006_u32.to_le_bytes()),
        "helper must receive width, APX, and imm16: {code:02X?}"
    );
    assert!(
        code.windows(4)
            .any(|window| window == 0x1000_u32.to_le_bytes()),
        "helper failure must restart at the faulting PC: {code:02X?}"
    );
}

#[test]
fn lower_far_return_rejects_malformed_nonterminal_and_duplicate_ownership() {
    for malformed in [
        far_return(OpWidth::W8, 0, false, 0x1001),
        far_return(OpWidth::W64, 0, false, 0x1000),
        far_return(OpWidth::W64, 0, false, 0x1010),
        far_return(OpWidth::W64, 1, false, 0x1001),
        far_return(OpWidth::W64, 0, true, 0x1002),
        OpKind::X86FarReturn(X86FarReturnOp {
            target: x86(X86Reg::Rbx),
            offset_width: OpWidth::W64,
            pop_bytes: 0,
            requires_apx: false,
            next_pc: 0x1001,
        }),
    ] {
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let exact = far_return(OpWidth::W64, 0, false, 0x1001);
    for (ops, target) in [
        (vec![exact.clone(), OpKind::Nop], x86(X86Reg::Rip)),
        (vec![exact.clone()], x86(X86Reg::Rbx)),
        (vec![exact.clone(), exact.clone()], x86(X86Reg::Rip)),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for op in ops {
            builder.push_op(0x1000, op);
        }
        builder.set_terminator(Terminator::IndirectBranch {
            target,
            possible_targets: vec![],
        });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_jit_fault_deopt_guards(true);
        assert!(matches!(
            lowerer.lower_function(&builder.finish()),
            Err(LowerError::InvalidOperand { .. })
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct FarReturnContext {
    calls: u64,
    encoding: u32,
    target: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn far_return_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    encoding: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut FarReturnContext) };
    context.calls += 1;
    context.encoding = encoding;
    if context.ok == 0 {
        return 0;
    }
    state.gpr[4] = 0xCAFE_BABE;
    state.exit_pc = context.target;
    state.cs_l = 0;
    state.cpl = 3;
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(ok: bool) -> (crate::smir::lower::runtime::GuestRegs, FarReturnContext) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(far_return(OpWidth::W16, 0xCAFE, true, 0x1005), true, true)
        .expect("lower executable far RET");
    let exec = ExecMem::new(&code).expect("map executable far RET");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.rflags = 0x2 | 0x08C5 | (1 << 10);
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.far_return_fn = far_return_helper as usize as u64;
    let mut context = FarReturnContext {
        target: 0xFFFF_8000_1234_5678,
        ok: u64::from(ok),
        ..FarReturnContext::default()
    };
    state.ctx = (&mut context as *mut FarReturnContext) as u64;
    exec.run(entry, &mut state);
    (state, context)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_far_return_passes_encoding_and_commits_helper_state() {
    let (state, context) = execute(true);
    assert_eq!(context.calls, 1);
    assert_eq!(context.encoding, 0xCAFE_0004);
    assert_eq!(state.gpr[4], 0xCAFE_BABE);
    assert_eq!(state.exit_pc, context.target);
    assert_eq!(state.cs_l, 0);
    assert_eq!(state.cpl, 3);
    assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_far_return_failure_restores_scalar_state_and_restarts_exactly() {
    let (state, context) = execute(false);
    assert_eq!(context.calls, 1);
    assert_eq!(state.exit_pc, 0x1000);
    for (index, value) in state.gpr.iter().enumerate() {
        assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
}
