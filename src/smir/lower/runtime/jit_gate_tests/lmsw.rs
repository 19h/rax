//! Fail-closed native admission for x86 LMSW operations.

use super::*;
use crate::smir::ir::ops::{X86LmswOp, X86LmswSource};
use crate::smir::lower::x86_64::x86_lmsw_shape_valid;

fn register(src: VReg, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Lmsw(X86LmswOp {
        source: X86LmswSource::Register { src },
        requires_apx,
        next_pc,
    })
}

fn memory(addr: Address, requires_apx: bool, next_pc: u64) -> OpKind {
    OpKind::X86Lmsw(X86LmswOp {
        source: X86LmswSource::Memory { addr },
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
    x86_lmsw_shape_valid(&function.blocks[0].ops[0])
}

fn x86_gate_with_mem(kind: OpKind, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &function(kind),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

#[test]
fn x86_lmsw_gate_admits_exact_register_shapes_stack_aliases_and_egprs() {
    for index in [0_u8, 4, 5, 8, 15, 16, 31] {
        let op = register(x86(X86Reg::gpr(index)), index >= 16, 0x1004);
        assert!(op.is_jit_safe(), "class whitelist: {op:?}");
        assert!(shape_valid(op.clone()), "{op:?}");
        assert!(x86_gate(op), "index={index}");
    }

    // REX2 may encode a legacy source and still needs a dynamic APX guard.
    assert!(x86_gate(register(x86(X86Reg::Rax), true, 0x1004)));
}

#[test]
fn x86_lmsw_gate_requires_memory_helpers_and_accepts_state_backed_addresses() {
    for (addr, requires_apx, next_pc) in [
        (Address::Absolute(0x4000), false, 0x1004),
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
            0x1005,
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
            0x1008,
        ),
    ] {
        let op = memory(addr, requires_apx, next_pc);
        assert!(shape_valid(op.clone()), "{op:?}");
        assert!(!x86_gate_with_mem(op.clone(), false), "{op:?}");
        assert!(x86_gate_with_mem(op.clone(), true), "{op:?}");
    }
}

#[test]
fn x86_lmsw_gate_rejects_malformed_ir_fail_closed() {
    for malformed in [
        register(VReg::virt(0), false, 0x1003),
        register(arm_x(0), false, 0x1003),
        register(x86(X86Reg::R16), false, 0x1004),
        register(x86(X86Reg::Rax), false, 0x1002),
        register(x86(X86Reg::Rax), false, 0x1010),
        register(x86(X86Reg::Rax), false, 0x0FFF),
        memory(Address::Direct(VReg::virt(1)), false, 0x1003),
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
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!shape_valid(malformed.clone()), "{malformed:?}");
        assert!(!x86_gate_with_mem(malformed, true));
    }

    let mut hinted = function(register(x86(X86Reg::Rax), false, 0x1003));
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_lmsw_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));
}

#[test]
fn x86_lmsw_gate_rejects_both_aarch64_host_paths() {
    for op in [
        register(x86(X86Reg::Rax), false, 0x1003),
        memory(Address::Absolute(0x4000), false, 0x1004),
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
fn x86_lmsw_survives_o2_and_remains_admitted_with_memory_helpers() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, register(x86(X86Reg::Rbp), false, 0x1003));
    builder.push_op(
        0x1003,
        memory(Address::Direct(x86(X86Reg::Rsp)), false, 0x1006),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert_eq!(
        function.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Lmsw(..)))
            .count(),
        2
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
