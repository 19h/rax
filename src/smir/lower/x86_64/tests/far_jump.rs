//! Fault-precise helper-backed native lowering for indirect far JMP.

use super::*;
use crate::smir::ir::ops::X86FarJumpOp;
use crate::smir::lower::X86_GUEST_FAR_JUMP_FN_OFFSET;

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn far_jump(addr: Address, width: OpWidth, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86FarJump(X86FarJumpOp {
        addr,
        target: x86(X86Reg::Rip),
        offset_width: width,
        requires_apx,
        stack_segment: false,
        next_pc,
    })
}

fn far_jump_with_stack_segment(
    addr: Address,
    width: OpWidth,
    requires_apx: bool,
    stack_segment: bool,
    next_pc: u64,
) -> OpKind {
    let mut op = far_jump(addr, width, requires_apx, next_pc);
    let OpKind::X86FarJump(jump) = &mut op else {
        unreachable!()
    };
    jump.stack_segment = stack_segment;
    op
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
fn lower_far_jump_requires_guards_helpers_serializes_and_calls_only_the_guest_helper() {
    let op = far_jump(
        Address::Direct(x86(X86Reg::Rax)),
        OpWidth::W64,
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
    let (code, _) = lower(op, true, true).expect("guarded far-JMP lowering");
    assert!(
        code.windows(4)
            .any(|window| window == (X86_GUEST_FAR_JUMP_FN_OFFSET as u32).to_le_bytes()),
        "missing far-JMP helper offset: {code:02X?}"
    );
    assert!(
        code.windows(2).any(|window| window == [0x0F, 0xA2]),
        "successful far JMP must serialize before native handoff: {code:02X?}"
    );
    assert!(
        code.windows(4)
            .any(|window| window == 0x1000_u32.to_le_bytes()),
        "helper failure must restart at the faulting guest PC: {code:02X?}"
    );
}

#[test]
fn lower_far_jump_rejects_every_non_lifter_shape_and_hint() {
    for malformed in [
        far_jump(Address::Direct(VReg::virt(0)), OpWidth::W64, false, 0x1003),
        far_jump(
            Address::Direct(VReg::Arch(ArchReg::Arm(crate::smir::ir::types::ArmReg::X(
                0,
            )))),
            OpWidth::W64,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::R31)),
            OpWidth::W64,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::Rax)),
            OpWidth::W8,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::Rax)),
            OpWidth::W64,
            false,
            0x1001,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::Rax)),
            OpWidth::W64,
            false,
            0x1010,
        ),
    ] {
        assert!(matches!(
            lower(malformed, true, true),
            Err(LowerError::InvalidOperand { .. })
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        far_jump(Address::Absolute(0x4000), OpWidth::W64, false, 0x1003),
    );
    builder.set_terminator(Terminator::IndirectBranch {
        target: x86(X86Reg::Rip),
        possible_targets: vec![],
    });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[test]
fn lower_far_jump_rejects_nonterminal_mismatched_and_duplicate_ownership() {
    let exact = far_jump(
        Address::Direct(x86(X86Reg::Rax)),
        OpWidth::W64,
        false,
        0x1003,
    );
    for (name, ops, terminal_target) in [
        (
            "nonterminal",
            vec![exact.clone(), OpKind::Nop],
            x86(X86Reg::Rip),
        ),
        ("mismatched target", vec![exact.clone()], x86(X86Reg::Rbx)),
        (
            "duplicate",
            vec![exact.clone(), exact.clone()],
            x86(X86Reg::Rip),
        ),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for op in ops {
            builder.push_op(0x1000, op);
        }
        builder.set_terminator(Terminator::IndirectBranch {
            target: terminal_target,
            possible_targets: vec![],
        });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_jit_fault_deopt_guards(true);
        assert!(
            matches!(
                lowerer.lower_function(&builder.finish()),
                Err(LowerError::InvalidOperand { .. })
            ),
            "{name}"
        );
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact);
    builder.set_terminator(Terminator::IndirectBranch {
        target: x86(X86Reg::Rip),
        possible_targets: vec![BlockId(1)],
    });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&builder.finish()),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Default)]
struct FarJumpContext {
    calls: u64,
    address: u64,
    encoding: u32,
    target: u64,
    ok: u64,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
unsafe extern "C" fn far_jump_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    address: u64,
    encoding: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut FarJumpContext) };
    context.calls += 1;
    context.address = address;
    context.encoding = encoding;
    if context.ok == 0 {
        return 0;
    }
    state.exit_pc = context.target;
    state.cs_l = 1;
    state.cpl = 3;
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute(
    addr: Address,
    width: OpWidth,
    requires_apx: bool,
    stack_segment: bool,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs, &mut FarJumpContext),
) -> (crate::smir::lower::runtime::GuestRegs, FarJumpContext) {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let (code, entry) = lower(
        far_jump_with_stack_segment(addr, width, requires_apx, stack_segment, 0x1004),
        true,
        true,
    )
    .expect("lower executable far JMP");
    let exec = ExecMem::new(&code).expect("map executable far JMP");
    let mut state = GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.rflags = 0x2 | 0x08C5 | (1 << 10);
    state.exit_pc = 0xDEAD_BEEF_DEAD_BEEF;
    state.apx_enabled = 1;
    state.far_jump_fn = far_jump_helper as usize as u64;
    let mut context = FarJumpContext {
        target: 0xFFFF_8000_1234_5678,
        ok: 1,
        ..FarJumpContext::default()
    };
    state.ctx = (&mut context as *mut FarJumpContext) as u64;
    configure(&mut state, &mut context);
    exec.run(entry, &mut state);
    (state, context)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_far_jump_computes_stack_and_egpr_addresses_encodes_width_and_hands_off_dynamically() {
    for (addr, width, apx, stack_segment, expected_address, expected_encoding) in [
        (
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: 0x28,
                disp_size: DispSize::Disp8,
            },
            OpWidth::W16,
            false,
            true,
            0xA500_0000_0000_002C,
            8,
        ),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R25)),
                index: x86(X86Reg::R26),
                scale: 8,
                disp: -16,
                disp_size: DispSize::Disp8,
            },
            OpWidth::W64,
            true,
            false,
            0x5000_u64.wrapping_add(4 * 8).wrapping_sub(16),
            6,
        ),
    ] {
        let (state, context) = execute(addr, width, apx, stack_segment, |state, _| {
            state.gpr[4] = 0xA500_0000_0000_0004;
            state.gpr[25] = 0x5000;
            state.gpr[26] = 4;
        });
        assert_eq!(context.calls, 1);
        assert_eq!(context.address, expected_address);
        assert_eq!(context.encoding, expected_encoding);
        assert_eq!(state.exit_pc, context.target);
        assert_eq!(state.cs_l, 1);
        assert_eq!(state.cpl, 3);
        assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_far_jump_helper_failure_restores_scalar_state_and_restarts_exactly() {
    let (state, context) = execute(
        Address::Direct(x86(X86Reg::Rbp)),
        OpWidth::W32,
        false,
        true,
        |_, context| context.ok = 0,
    );
    assert_eq!(context.calls, 1);
    assert_eq!(context.encoding, 9);
    assert_eq!(state.exit_pc, 0x1000);
    for (index, value) in state.gpr.iter().enumerate() {
        assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
    }
    assert_eq!(state.rflags & (0x08C5 | (1 << 10)), 0x08C5 | (1 << 10));
}
