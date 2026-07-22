//! Fail-closed native admission and ABI layout for x86 INVPCID.

use super::*;
use crate::smir::ir::ops::X86InvpcidOp;
use crate::smir::lower::runtime::{GuestRegs, x86_jit_op_uses_mem_helper};
use crate::smir::lower::x86_64::x86_invpcid_shape_valid;
use crate::smir::lower::{X86_GUEST_FAST_SYSTEM_TRANSFER_FN_OFFSET, X86_GUEST_INVPCID_FN_OFFSET};
use crate::smir::optimize::{OptLevel, optimize_function};

fn kind(invpcid_type: VReg, addr: Address, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Invpcid(X86InvpcidOp {
        invpcid_type,
        addr,
        requires_apx,
        stack_segment: false,
        next_pc,
    })
}

fn function(kind: OpKind) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn gate(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn invpcid_helper_offset_is_append_only_and_matches_guest_layout() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, invpcid_fn),
        X86_GUEST_INVPCID_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_INVPCID_FN_OFFSET,
        X86_GUEST_FAST_SYSTEM_TRANSFER_FN_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().invpcid_fn, 0);
}

#[test]
fn x86_invpcid_gate_requires_mmu_helpers_and_accepts_exact_stack_and_egpr_shapes() {
    for (type_reg, addr, requires_apx, next_pc) in [
        (
            x86(X86Reg::Rsp),
            Address::Direct(x86(X86Reg::Rbp)),
            false,
            0x1005,
        ),
        (
            x86(X86Reg::R31),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::R16),
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
            true,
            0x1008,
        ),
        (
            x86(X86Reg::R16),
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::R20)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 2,
                disp: 0x40,
            })),
            true,
            0x100A,
        ),
    ] {
        let op = kind(type_reg, addr, requires_apx, next_pc);
        let function = function(op.clone());
        assert!(op.is_jit_safe(), "{op:?}");
        assert!(op.reads_memory(), "{op:?}");
        assert!(x86_invpcid_shape_valid(&function.blocks[0].ops[0]));
        assert!(x86_jit_op_uses_mem_helper(&op));
        assert!(!gate(op.clone(), false), "{op:?}");
        assert!(gate(op, true));
    }
}

#[test]
fn x86_invpcid_gate_rejects_malformed_shapes_and_cross_hosts() {
    for malformed in [
        kind(
            VReg::virt(0),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::R16),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(VReg::virt(0)),
            false,
            0x1005,
        ),
        kind(x86(X86Reg::Rax), Address::Direct(arm_x(0)), false, 0x1005),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::R31)),
            false,
            0x1005,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::Rbx)),
            false,
            0x1004,
        ),
        kind(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::Rbx)),
            true,
            0x1005,
        ),
    ] {
        let function = function(malformed.clone());
        assert!(!x86_invpcid_shape_valid(&function.blocks[0].ops[0]));
        assert!(!gate(malformed, true));
    }

    let op = kind(
        x86(X86Reg::Rax),
        Address::Direct(x86(X86Reg::Rbx)),
        false,
        0x1005,
    );
    let function = function(op.clone());
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&op));
    assert!(!aarch64_gate(vec![op], false));
}

#[test]
fn x86_invpcid_survives_o2_and_remains_admitted_only_with_memory_helpers() {
    let mut function = function(kind(
        x86(X86Reg::Rdx),
        Address::BaseOffset {
            base: x86(X86Reg::Rax),
            offset: 0x20,
            disp_size: DispSize::Disp8,
        },
        false,
        0x1006,
    ));
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(function.blocks[0].ops.len(), 1);
    let op = &function.blocks[0].ops[0];
    assert!(matches!(op.kind, OpKind::X86Invpcid(..)));
    assert!(x86_invpcid_shape_valid(op));
    assert!(!is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        false,
    ));
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
