//! Fault-precise helper-backed native lowering for indirect far CALL.

use super::*;
use crate::smir::ir::ops::X86FarCallOp;
use crate::smir::lower::X86_GUEST_FAR_CALL_FN_OFFSET;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn far_call(
    addr: Address,
    width: OpWidth,
    requires_apx: bool,
    stack_segment: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86FarCall(X86FarCallOp {
        addr,
        target: x86(X86Reg::Rip),
        offset_width: width,
        requires_apx,
        stack_segment,
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
fn lower_far_call_requires_guards_helpers_serializes_and_calls_guest_helper() {
    let op = far_call(
        Address::Direct(x86(X86Reg::Rax)),
        OpWidth::W64,
        false,
        false,
        0x1003,
    );
    assert!(matches!(
        lower(op.clone(), true, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower(op.clone(), false, true),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let (code, _) = lower(op, true, true).expect("guarded far-CALL lowering");
    assert!(
        code.windows(4)
            .any(|window| window == (X86_GUEST_FAR_CALL_FN_OFFSET as u32).to_le_bytes()),
        "missing far-CALL helper offset: {code:02X?}"
    );
    assert!(code.windows(2).any(|window| window == [0x0F, 0xA2]));
    assert!(
        code.windows(7)
            .any(|window| window == [0x48, 0xC7, 0xC1, 0x03, 0x10, 0x00, 0x00]),
        "helper must receive the exact return PC: {code:02X?}"
    );
    assert!(
        code.windows(4)
            .any(|window| window == 0x1000_u32.to_le_bytes()),
        "helper failure must restart at the faulting PC: {code:02X?}"
    );
}

#[test]
fn lower_far_call_rejects_malformed_nonterminal_and_duplicate_ownership() {
    for malformed in [
        far_call(
            Address::Direct(VReg::virt(0)),
            OpWidth::W64,
            false,
            false,
            0x1003,
        ),
        far_call(
            Address::Direct(x86(X86Reg::R31)),
            OpWidth::W64,
            false,
            false,
            0x1003,
        ),
        far_call(
            Address::Direct(x86(X86Reg::Rax)),
            OpWidth::W8,
            false,
            false,
            0x1003,
        ),
        far_call(
            Address::Direct(x86(X86Reg::Rax)),
            OpWidth::W64,
            false,
            false,
            0x1010,
        ),
    ] {
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let exact = far_call(
        Address::Direct(x86(X86Reg::Rax)),
        OpWidth::W64,
        false,
        false,
        0x1003,
    );
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
struct FarCallContext {
    calls: u64,
    address: u64,
    encoding: u32,
    return_pc: u64,
    target: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn far_call_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    address: u64,
    encoding: u32,
    return_pc: u64,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut FarCallContext) };
    context.calls += 1;
    context.address = address;
    context.encoding = encoding;
    context.return_pc = return_pc;
    if context.ok == 0 {
        return 0;
    }
    state.gpr[4] = state.gpr[4].wrapping_sub(16);
    state.exit_pc = context.target;
    state.cs_l = 1;
    state.cpl = 3;
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(ok: bool) -> (crate::smir::lower::runtime::GuestRegs, FarCallContext) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(
        far_call(
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: 0x28,
                disp_size: DispSize::Disp8,
            },
            OpWidth::W16,
            false,
            true,
            0x1004,
        ),
        true,
        true,
    )
    .expect("lower executable far CALL");
    let exec = ExecMem::new(&code).expect("map executable far CALL");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.rflags = 0x2 | 0x08C5 | (1 << 10);
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.far_call_fn = far_call_helper as usize as u64;
    let mut context = FarCallContext {
        target: 0xFFFF_8000_1234_5678,
        ok: u64::from(ok),
        ..FarCallContext::default()
    };
    state.ctx = (&mut context as *mut FarCallContext) as u64;
    exec.run(entry, &mut state);
    (state, context)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_far_call_passes_address_encoding_return_pc_and_commits_helper_rsp() {
    let (state, context) = execute(true);
    assert_eq!(context.calls, 1);
    assert_eq!(context.address, 0xA500_0000_0000_002C);
    assert_eq!(context.encoding, 8);
    assert_eq!(context.return_pc, 0x1004);
    assert_eq!(state.gpr[4], 0xA4FF_FFFF_FFFF_FFF4);
    assert_eq!(state.exit_pc, context.target);
    assert_eq!(state.cpl, 3);
    assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_far_call_failure_restores_scalar_state_and_restarts_exactly() {
    let (state, context) = execute(false);
    assert_eq!(context.calls, 1);
    assert_eq!(state.exit_pc, 0x1000);
    for (index, value) in state.gpr.iter().enumerate() {
        assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
}
