//! Fail-closed native admission and helper ABI for indirect far JMP.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86FarJumpOp};
use crate::smir::ir::types::OpId;
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::{x86_far_jump_shape_valid, x86_far_jump_terminal_shape_valid};
use crate::smir::lower::{X86_GUEST_FAR_JUMP_FN_OFFSET, X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET};

fn far_jump(
    addr: Address,
    target: VReg,
    width: OpWidth,
    requires_apx: bool,
    next_pc: u64,
) -> OpKind {
    OpKind::X86FarJump(X86FarJumpOp {
        addr,
        target,
        offset_width: width,
        requires_apx,
        stack_segment: false,
        next_pc,
    })
}

fn function(kind: OpKind, terminal_target: VReg) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::IndirectBranch {
        target: terminal_target,
        possible_targets: vec![],
    });
    builder.finish()
}

fn gate(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind, x86(X86Reg::Rip)),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn far_jump_helper_offset_is_append_only_and_matches_guest_layout() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, far_jump_fn),
        X86_GUEST_FAR_JUMP_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_FAR_JUMP_FN_OFFSET,
        X86_GUEST_SYSTEM_SELECTOR_LOAD_FN_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().far_jump_fn, 0);
}

#[test]
fn far_jump_gate_requires_memory_helpers_and_accepts_exact_state_addresses() {
    for (addr, width, requires_apx) in [
        (Address::Absolute(0x4000), OpWidth::W16, false),
        (Address::Direct(x86(X86Reg::Rsp)), OpWidth::W32, false),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::R31),
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
            OpWidth::W64,
            true,
        ),
        (
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::R16)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 2,
                disp: 0x40,
            })),
            OpWidth::W64,
            true,
        ),
    ] {
        let kind = far_jump(addr, x86(X86Reg::Rip), width, requires_apx, 0x1004);
        let function = function(kind.clone(), x86(X86Reg::Rip));
        let op = &function.blocks[0].ops[0];
        assert!(op.is_jit_safe(), "{op:?}");
        assert!(x86_far_jump_shape_valid(op), "{op:?}");
        assert!(x86_far_jump_terminal_shape_valid(&function.blocks[0]));
        assert!(x86_jit_op_uses_mem_helper(&op.kind));
        assert!(!gate(kind.clone(), false), "{kind:?}");
        assert!(gate(kind, true));
    }
}

#[test]
fn far_jump_gate_rejects_every_malformed_or_cross_host_shape() {
    let malformed = [
        far_jump(
            Address::Direct(VReg::virt(0)),
            x86(X86Reg::Rip),
            OpWidth::W64,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(arm_x(0)),
            x86(X86Reg::Rip),
            OpWidth::W64,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::R31)),
            x86(X86Reg::Rip),
            OpWidth::W64,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rbx),
            OpWidth::W64,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rip),
            OpWidth::W8,
            false,
            0x1003,
        ),
        far_jump(
            Address::Direct(x86(X86Reg::Rax)),
            x86(X86Reg::Rip),
            OpWidth::W64,
            false,
            0x1010,
        ),
    ];
    for kind in malformed {
        let function = function(kind.clone(), x86(X86Reg::Rip));
        assert!(!x86_far_jump_shape_valid(&function.blocks[0].ops[0]));
        assert!(!x86_far_jump_terminal_shape_valid(&function.blocks[0]));
        assert!(!gate(kind, true));
    }

    let exact = far_jump(
        Address::Direct(x86(X86Reg::Rax)),
        x86(X86Reg::Rip),
        OpWidth::W64,
        false,
        0x1003,
    );
    let wrong_terminal = function(exact.clone(), x86(X86Reg::Rbx));
    assert!(x86_far_jump_shape_valid(&wrong_terminal.blocks[0].ops[0]));
    assert!(!x86_far_jump_terminal_shape_valid(
        &wrong_terminal.blocks[0]
    ));
    assert!(!is_native_clobber_safe_excluding(
        &wrong_terminal,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut nonterminal = function(exact.clone(), x86(X86Reg::Rip));
    nonterminal.blocks[0]
        .ops
        .push(SmirOp::new(OpId(1), 0x1003, OpKind::Nop));
    assert!(!is_native_clobber_safe_excluding(
        &nonterminal,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut duplicate = function(exact.clone(), x86(X86Reg::Rip));
    duplicate.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(1), 0x1000, exact.clone()));
    assert!(x86_far_jump_terminal_shape_valid(&duplicate.blocks[0]));
    assert!(!is_native_clobber_safe_excluding(
        &duplicate,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut annotated = function(exact.clone(), x86(X86Reg::Rip));
    let Terminator::IndirectBranch {
        possible_targets, ..
    } = &mut annotated.blocks[0].terminator
    else {
        unreachable!()
    };
    possible_targets.push(BlockId(1));
    assert!(!x86_far_jump_terminal_shape_valid(&annotated.blocks[0]));
    assert!(!is_native_clobber_safe_excluding(
        &annotated,
        &std::collections::HashMap::new(),
        true,
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &function(exact.clone(), x86(X86Reg::Rip)),
        &std::collections::HashMap::new(),
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
}
