//! Fail-closed native admission and ABI layout for x86 SGDT/SIDT.

use super::*;
use crate::smir::ir::ops::{X86DescriptorTable, X86DescriptorTableStoreOp};
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::x86_descriptor_table_store_shape_valid;
use crate::smir::lower::{X86_GUEST_DESCRIPTOR_STORE_FN_OFFSET, X86_GUEST_PMC_FN_OFFSET};

fn store(addr: Address, requires_apx: bool) -> OpKind {
    OpKind::X86DescriptorTableStore(X86DescriptorTableStoreOp {
        addr,
        table: X86DescriptorTable::Gdt,
        requires_apx,
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
fn descriptor_store_helper_offset_is_append_only_and_matches_guest_layout() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, descriptor_store_fn),
        X86_GUEST_DESCRIPTOR_STORE_FN_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_DESCRIPTOR_STORE_FN_OFFSET,
        X86_GUEST_PMC_FN_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().descriptor_store_fn, 0);
}

#[test]
fn x86_descriptor_store_gate_requires_memory_helpers_and_accepts_exact_addresses() {
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
        let op = store(addr, requires_apx);
        let function = function(op.clone());
        assert!(op.is_jit_safe(), "{op:?}");
        assert!(x86_descriptor_table_store_shape_valid(
            &function.blocks[0].ops[0]
        ));
        assert!(!gate(op.clone(), false), "{op:?}");
        assert!(gate(op, true));
    }
}

#[test]
fn x86_descriptor_store_gate_rejects_malformed_ir_and_aarch64_hosts() {
    for malformed in [
        store(Address::Direct(VReg::virt(0)), false),
        store(Address::Direct(arm_x(0)), false),
        store(Address::Direct(x86(X86Reg::R31)), false),
        store(
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
        let function = function(malformed.clone());
        assert!(!x86_descriptor_table_store_shape_valid(
            &function.blocks[0].ops[0]
        ));
        assert!(!gate(malformed, true));
    }

    for op in [
        store(Address::Absolute(0x4000), false),
        store(Address::Direct(x86(X86Reg::Rsp)), false),
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
fn x86_descriptor_store_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, store(Address::Direct(x86(X86Reg::Rsp)), false));
    builder.push_op(
        0x1003,
        OpKind::X86DescriptorTableStore(X86DescriptorTableStoreOp {
            addr: Address::Direct(x86(X86Reg::R31)),
            table: X86DescriptorTable::Idt,
            requires_apx: true,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert_eq!(
        function.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86DescriptorTableStore(..)))
            .count(),
        2
    );
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true,
    ));
}
