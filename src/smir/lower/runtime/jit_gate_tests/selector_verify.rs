//! Fail-closed native admission for VERR/VERW.

use super::*;
use crate::smir::ir::ops::{X86SelectorVerifyKind, X86SelectorVerifyOp, X86SelectorVerifySource};
use crate::smir::lower::x86_64::x86_selector_verify_shape_valid;

fn register(src: VReg, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86SelectorVerify(X86SelectorVerifyOp {
        kind: X86SelectorVerifyKind::Read,
        source: X86SelectorVerifySource::Register { src },
        requires_apx,
        next_pc,
    })
}

fn memory(addr: Address, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86SelectorVerify(X86SelectorVerifyOp {
        kind: X86SelectorVerifyKind::Write,
        source: X86SelectorVerifySource::Memory {
            addr,
            stack_segment: false,
        },
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

fn shape_valid(kind: OpKind) -> bool {
    let function = function(kind);
    x86_selector_verify_shape_valid(&function.blocks[0].ops[0])
}

fn gate(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn selector_verify_gate_requires_implicit_descriptor_memory_for_every_source() {
    for op in [
        register(x86(X86Reg::Rax), false, 0x1003),
        register(x86(X86Reg::Rsp), false, 0x1003),
        register(x86(X86Reg::Rbp), false, 0x1003),
        register(x86(X86Reg::R31), true, 0x1004),
        memory(Address::Absolute(0x4000), false, 0x1007),
        memory(Address::Direct(x86(X86Reg::Rsp)), false, 0x1003),
        memory(
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R16)),
                index: x86(X86Reg::R31),
                scale: 8,
                disp: -8,
                disp_size: DispSize::Disp8,
            },
            true,
            0x1005,
        ),
    ] {
        assert!(op.is_jit_safe(), "class gate rejected {op:?}");
        assert!(shape_valid(op.clone()), "shape gate rejected {op:?}");
        assert!(
            x86_jit_op_uses_mem_helper(&op),
            "helper use missing: {op:?}"
        );
        assert!(
            !gate(op.clone(), false),
            "implicit descriptor read escaped: {op:?}"
        );
        assert!(gate(op, true));
    }
}

#[test]
fn selector_verify_gate_rejects_malformed_sources_apx_hints_and_frontiers() {
    for malformed in [
        register(VReg::virt(0), false, 0x1003),
        register(arm_x(0), false, 0x1003),
        register(x86(X86Reg::R16), false, 0x1004),
        memory(Address::Direct(VReg::virt(0)), false, 0x1003),
        memory(Address::Direct(x86(X86Reg::R31)), false, 0x1004),
        memory(
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            false,
            0x1004,
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
            0x1004,
        ),
        register(x86(X86Reg::Rax), false, 0x1002),
        register(x86(X86Reg::Rax), false, 0x1010),
        register(x86(X86Reg::Rax), false, 0x0FFF),
    ] {
        assert!(
            malformed.is_jit_safe(),
            "class whitelist unexpectedly changed"
        );
        assert!(
            !shape_valid(malformed.clone()),
            "malformed shape admitted: {malformed:?}"
        );
        assert!(!gate(malformed, true));
    }

    let mut hinted = function(register(x86(X86Reg::Rax), false, 0x1003));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_selector_verify_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));
}

#[test]
fn non_x86_native_gates_reject_selector_verification() {
    for op in [
        register(x86(X86Reg::Rax), false, 0x1003),
        memory(Address::Direct(x86(X86Reg::Rsp)), false, 0x1003),
    ] {
        assert!(!aarch64_gate(vec![op.clone()], true));
        assert!(!aarch32_gate_with_mem(vec![op], true));
    }
}
