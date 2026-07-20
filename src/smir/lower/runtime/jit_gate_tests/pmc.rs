//! Fail-closed native admission for x86 RDPMC.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86ReadPmcOp};
use crate::smir::ir::types::OpId;
use crate::smir::lower::x86_64::x86_read_pmc_shape_valid;
use crate::smir::lower::{X86_GUEST_PMC_FN_OFFSET, X86_GUEST_SYSENTER_EIP_OFFSET};

fn pmc(selector: VReg, dst_lo: VReg, dst_hi: VReg) -> OpKind {
    OpKind::X86ReadPmc(X86ReadPmcOp {
        dst_lo,
        dst_hi,
        selector,
    })
}

fn exact_pmc() -> OpKind {
    pmc(x86(X86Reg::Rcx), x86(X86Reg::Rax), x86(X86Reg::Rdx))
}

fn smir_op(kind: OpKind) -> SmirOp {
    SmirOp::new(OpId(0), 0x1000, kind)
}

#[test]
fn pmc_helper_layout_is_exact_and_append_only() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, pmc_fn),
        X86_GUEST_PMC_FN_OFFSET as usize
    );
    assert_eq!(X86_GUEST_PMC_FN_OFFSET, X86_GUEST_SYSENTER_EIP_OFFSET + 8);
    assert_eq!(GuestRegs::default().pmc_fn, 0);
}

#[test]
fn x86_pmc_gate_admits_only_the_fixed_implicit_register_shape() {
    let exact = exact_pmc();
    assert!(exact.is_jit_safe());
    assert!(x86_read_pmc_shape_valid(&smir_op(exact.clone())));
    assert!(x86_gate(exact));

    for malformed in [
        pmc(x86(X86Reg::Rbx), x86(X86Reg::Rax), x86(X86Reg::Rdx)),
        pmc(x86(X86Reg::Rcx), x86(X86Reg::Rbx), x86(X86Reg::Rdx)),
        pmc(x86(X86Reg::Rcx), x86(X86Reg::Rax), x86(X86Reg::Rbx)),
        pmc(
            VReg::Virtual(VirtualId(0)),
            x86(X86Reg::Rax),
            x86(X86Reg::Rdx),
        ),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!x86_read_pmc_shape_valid(&smir_op(malformed.clone())));
        assert!(!x86_gate(malformed), "malformed RDPMC was admitted");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact_pmc());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_read_pmc_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&hinted));
}

#[test]
fn pmc_gate_rejects_cross_host_execution() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact_pmc());
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ),
        "RDPMC has no AArch64-host guest-PMU lowering"
    );
}

#[test]
fn pmc_reads_survive_o2_and_remain_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact_pmc());
    builder.push_op(0x1002, exact_pmc());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86ReadPmc(..)))
            .count(),
        2
    );
    assert!(is_native_clobber_safe(&function));
}
