//! Fail-closed native admission and helper ABI for x86 SLDT/STR.

use super::*;
use crate::smir::ir::ops::{X86SystemSelector, X86SystemSelectorStoreOp, X86SystemSelectorTarget};
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::x86_system_selector_store_shape_valid;
use crate::smir::lower::{
    X86_GUEST_DESCRIPTOR_LOAD_FN_OFFSET, X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
};

fn register(dst: VReg, width: OpWidth, requires_apx: bool) -> OpKind {
    OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        selector: X86SystemSelector::Ldtr,
        target: X86SystemSelectorTarget::Register { dst, width },
        requires_apx,
    })
}

fn memory(addr: Address, requires_apx: bool) -> OpKind {
    OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
        selector: X86SystemSelector::Tr,
        target: X86SystemSelectorTarget::Memory { addr },
        requires_apx,
    })
}

fn function(kind: OpKind) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn shape_valid(kind: OpKind) -> bool {
    let function = function(kind);
    x86_system_selector_store_shape_valid(&function.blocks[0].ops[0])
}

fn gate(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn selector_helper_offset_is_append_only_and_matches_guest_layout() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, system_selector_fn),
        X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_SYSTEM_SELECTOR_FN_OFFSET,
        X86_GUEST_DESCRIPTOR_LOAD_FN_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().system_selector_fn, 0);
}

#[test]
fn x86_selector_gate_admits_exact_register_widths_stack_aliases_and_egprs() {
    for index in [0_u8, 4, 5, 8, 15, 16, 31] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let op = register(x86(X86Reg::gpr(index)), width, index >= 16);
            assert!(op.is_jit_safe(), "class whitelist: {op:?}");
            assert!(shape_valid(op.clone()), "{op:?}");
            assert!(gate(op, false), "index={index}, width={width:?}");
        }
    }

    // REX2 may encode a legacy GPR and still requires the dynamic APX guard.
    assert!(gate(register(x86(X86Reg::Rax), OpWidth::W32, true), false));
}

#[test]
fn x86_selector_gate_requires_memory_helpers_and_accepts_state_backed_addresses() {
    for (addr, requires_apx) in [
        (Address::Absolute(0x4000), false),
        (Address::Direct(x86(X86Reg::Rsp)), false),
        (
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::R31),
                scale: 4,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
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
            true,
        ),
    ] {
        let op = memory(addr, requires_apx);
        assert!(shape_valid(op.clone()), "{op:?}");
        assert!(!gate(op.clone(), false), "{op:?}");
        assert!(gate(op, true));
    }
}

#[test]
fn x86_selector_gate_rejects_malformed_and_hinted_ir_fail_closed() {
    for malformed in [
        register(VReg::virt(0), OpWidth::W64, false),
        register(arm_x(0), OpWidth::W32, false),
        register(x86(X86Reg::Rax), OpWidth::W8, false),
        register(x86(X86Reg::Rax), OpWidth::W128, false),
        register(x86(X86Reg::R16), OpWidth::W64, false),
        memory(Address::Direct(VReg::virt(1)), false),
        memory(Address::Direct(arm_x(0)), false),
        memory(Address::Direct(x86(X86Reg::R31)), false),
        memory(
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
        ),
        memory(
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            false,
        ),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!shape_valid(malformed.clone()), "{malformed:?}");
        assert!(!gate(malformed, true));
    }

    let mut hinted = function(register(x86(X86Reg::Rax), OpWidth::W32, false));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_system_selector_store_shape_valid(
        &hinted.blocks[0].ops[0]
    ));
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));
}

#[test]
fn x86_selector_gate_rejects_both_aarch64_host_paths() {
    for op in [
        register(x86(X86Reg::Rax), OpWidth::W32, false),
        memory(Address::Absolute(0x4000), false),
    ] {
        let function = function(op.clone());
        assert!(!is_x86_aarch64_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
        ));
        assert!(!x86_aarch64_scalar_shape_valid(&op));
        assert!(!aarch64_gate(vec![op], true));
    }
}

#[test]
fn x86_selector_survives_o2_and_remains_admitted_with_memory_helpers() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, register(x86(X86Reg::Rbp), OpWidth::W16, false));
    builder.push_op(0x1003, memory(Address::Direct(x86(X86Reg::Rsp)), false));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert_eq!(
        function.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86SystemSelectorStore(..)))
            .count(),
        2
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
