//! Fail-closed native admission and ABI layout for x86 INVLPG.

use super::*;
use crate::smir::ir::ops::X86InvlpgOp;
use crate::smir::lower::runtime::{GuestRegs, x86_jit_op_uses_mem_helper};
use crate::smir::lower::x86_64::x86_invlpg_shape_valid;
use crate::smir::lower::{X86_GUEST_INVLPG_FN_OFFSET, X86_GUEST_STI_FN_OFFSET};
use crate::smir::optimize::{OptLevel, optimize_function};

fn kind(addr: Address, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Invlpg(X86InvlpgOp {
        addr,
        requires_apx,
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
fn invlpg_helper_offset_is_append_only_and_matches_guest_layout() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, invlpg_fn),
        X86_GUEST_INVLPG_FN_OFFSET as usize
    );
    assert_eq!(X86_GUEST_INVLPG_FN_OFFSET, X86_GUEST_STI_FN_OFFSET + 8);
    assert_eq!(GuestRegs::default().invlpg_fn, 0);
}

#[test]
fn x86_invlpg_gate_accepts_exact_addresses_without_mmu_memory_helpers() {
    for (addr, requires_apx, next_pc) in [
        (Address::Absolute(0x4000), false, 0x1007),
        (Address::Direct(x86(X86Reg::Rsp)), false, 0x1003),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::R31),
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
            true,
            0x1006,
        ),
        (
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::R16)),
                index: Some(x86(X86Reg::Rcx)),
                scale: 2,
                disp: 0x40,
            })),
            true,
            0x1008,
        ),
    ] {
        let op = kind(addr, requires_apx, next_pc);
        let function = function(op.clone());
        assert!(op.is_jit_safe(), "{op:?}");
        assert!(x86_invlpg_shape_valid(&function.blocks[0].ops[0]));
        assert!(!x86_jit_op_uses_mem_helper(&op));
        assert!(gate(op.clone(), false), "{op:?}");
        assert!(gate(op, true));
    }
}

#[test]
fn x86_invlpg_gate_rejects_malformed_shapes_and_cross_hosts() {
    for malformed in [
        kind(Address::Direct(VReg::virt(0)), false, 0x1003),
        kind(Address::Direct(x86(X86Reg::Rax)), true, 0x1003),
        kind(Address::Direct(arm_x(0)), false, 0x1003),
        kind(Address::Direct(x86(X86Reg::R31)), false, 0x1004),
        kind(Address::Direct(x86(X86Reg::Rax)), false, 0x1002),
        kind(
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
            0x1004,
        ),
    ] {
        let function = function(malformed.clone());
        assert!(!x86_invlpg_shape_valid(&function.blocks[0].ops[0]));
        assert!(!gate(malformed, true));
    }

    let op = kind(Address::Direct(x86(X86Reg::Rax)), false, 0x1003);
    let function = function(op.clone());
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
    assert!(!x86_aarch64_scalar_shape_valid(&op));
    assert!(!aarch64_gate(vec![op], false));
}

#[test]
fn x86_invlpg_survives_o2_and_remains_admitted() {
    let mut function = function(kind(
        Address::BaseOffset {
            base: x86(X86Reg::Rax),
            offset: 0x20,
            disp_size: DispSize::Disp8,
        },
        false,
        0x1004,
    ));
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(function.blocks[0].ops.len(), 1);
    let op = &function.blocks[0].ops[0];
    assert!(matches!(op.kind, OpKind::X86Invlpg(..)));
    assert!(x86_invlpg_shape_valid(op));
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        false,
    ));
}
