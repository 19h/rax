//! Fail-closed native admission for LAR/LSL.

use super::*;
use crate::smir::ir::ops::{X86SelectorQueryKind, X86SelectorQueryOp, X86SelectorQuerySource};
use crate::smir::lower::x86_64::x86_selector_query_shape_valid;

fn register(dst: VReg, src: VReg, width: OpWidth, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86SelectorQuery(X86SelectorQueryOp {
        kind: X86SelectorQueryKind::AccessRights,
        dst,
        source: X86SelectorQuerySource::Register { src },
        width,
        requires_apx,
        next_pc,
    })
}

fn memory(dst: VReg, addr: Address, width: OpWidth, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86SelectorQuery(X86SelectorQueryOp {
        kind: X86SelectorQueryKind::Limit,
        dst,
        source: X86SelectorQuerySource::Memory {
            addr,
            stack_segment: false,
        },
        width,
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
    x86_selector_query_shape_valid(&function.blocks[0].ops[0])
}

fn gate(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn selector_query_gate_requires_implicit_descriptor_memory_for_every_source() {
    for op in [
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::Rax),
            OpWidth::W32,
            false,
            0x1003,
        ),
        register(
            x86(X86Reg::Rsp),
            x86(X86Reg::Rbp),
            OpWidth::W16,
            false,
            0x1004,
        ),
        register(
            x86(X86Reg::R30),
            x86(X86Reg::R31),
            OpWidth::W64,
            true,
            0x1004,
        ),
        memory(
            x86(X86Reg::Rdx),
            Address::Absolute(0x4000),
            OpWidth::W32,
            false,
            0x1007,
        ),
        memory(
            x86(X86Reg::R15),
            Address::Direct(x86(X86Reg::Rsp)),
            OpWidth::W64,
            false,
            0x1004,
        ),
        memory(
            x86(X86Reg::R27),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::R25)),
                index: x86(X86Reg::R26),
                scale: 8,
                disp: 0x20,
                disp_size: DispSize::Disp32,
            },
            OpWidth::W32,
            true,
            0x1009,
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
fn selector_query_gate_rejects_malformed_registers_widths_addresses_apx_and_frontiers() {
    for malformed in [
        register(VReg::virt(0), x86(X86Reg::Rax), OpWidth::W32, false, 0x1003),
        register(arm_x(0), x86(X86Reg::Rax), OpWidth::W32, false, 0x1003),
        register(x86(X86Reg::Rax), VReg::virt(0), OpWidth::W32, false, 0x1003),
        register(
            x86(X86Reg::R16),
            x86(X86Reg::Rax),
            OpWidth::W32,
            false,
            0x1004,
        ),
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::R31),
            OpWidth::W64,
            false,
            0x1004,
        ),
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            OpWidth::W8,
            false,
            0x1003,
        ),
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            OpWidth::W128,
            false,
            0x1003,
        ),
        memory(
            x86(X86Reg::Rax),
            Address::Direct(VReg::virt(0)),
            OpWidth::W32,
            false,
            0x1003,
        ),
        memory(
            x86(X86Reg::Rax),
            Address::Direct(x86(X86Reg::R31)),
            OpWidth::W32,
            false,
            0x1004,
        ),
        memory(
            x86(X86Reg::Rax),
            Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
                x86(X86Reg::Rax),
            ))))),
            OpWidth::W32,
            false,
            0x1004,
        ),
        memory(
            x86(X86Reg::Rax),
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rax)),
                index: x86(X86Reg::Rcx),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            OpWidth::W32,
            false,
            0x1004,
        ),
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            0x1002,
        ),
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            0x1010,
        ),
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            0x0FFF,
        ),
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

    let mut hinted = function(register(
        x86(X86Reg::Rax),
        x86(X86Reg::Rcx),
        OpWidth::W32,
        false,
        0x1003,
    ));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_selector_query_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));
}

#[test]
fn non_x86_native_gates_reject_selector_queries() {
    for op in [
        register(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            OpWidth::W32,
            false,
            0x1003,
        ),
        memory(
            x86(X86Reg::Rdx),
            Address::Direct(x86(X86Reg::Rsp)),
            OpWidth::W64,
            false,
            0x1004,
        ),
    ] {
        assert!(!aarch64_gate(vec![op.clone()], true));
        assert!(!aarch32_gate_with_mem(vec![op], true));
    }
}
