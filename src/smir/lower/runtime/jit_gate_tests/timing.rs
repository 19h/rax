//! Fail-closed native admission for x86 RDTSC/RDTSCP.

use super::*;
use crate::smir::ir::ops::X86ReadTscOp;
use crate::smir::lower::x86_64::x86_read_tsc_shape_valid;

fn timestamp(dst_lo: VReg, dst_hi: VReg, dst_aux: Option<VReg>) -> OpKind {
    OpKind::X86ReadTsc(X86ReadTscOp {
        dst_lo,
        dst_hi,
        dst_aux,
    })
}

fn exact_timestamp(aux: bool) -> OpKind {
    timestamp(
        x86(X86Reg::Rax),
        x86(X86Reg::Rdx),
        aux.then(|| x86(X86Reg::Rcx)),
    )
}

#[test]
fn timestamp_helper_layout_is_exact_and_appended() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, tsc_fn),
        X86_GUEST_TSC_FN_OFFSET as usize
    );
    assert_eq!(X86_GUEST_TSC_FN_OFFSET, X86_GUEST_KERNEL_GS_BASE_OFFSET + 8);
    assert_eq!(GuestRegs::default().tsc_fn, 0);
}

#[test]
fn x86_timestamp_gate_admits_only_rdtsc_and_rdtscp_fixed_shapes() {
    for aux in [false, true] {
        let exact = exact_timestamp(aux);
        assert!(exact.is_jit_safe());
        assert!(x86_read_tsc_shape_valid(&exact));
        assert!(x86_gate(exact));
    }

    for malformed in [
        timestamp(x86(X86Reg::Rbx), x86(X86Reg::Rdx), None),
        timestamp(x86(X86Reg::Rax), x86(X86Reg::Rcx), None),
        timestamp(x86(X86Reg::Rax), x86(X86Reg::Rdx), Some(x86(X86Reg::Rbx))),
        timestamp(
            x86(X86Reg::Rax),
            x86(X86Reg::Rdx),
            Some(VReg::Virtual(VirtualId(0))),
        ),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!x86_read_tsc_shape_valid(&malformed));
        assert!(!x86_gate(malformed), "malformed timestamp read admitted");
    }
}

#[test]
fn timestamp_gate_rejects_cross_host_execution() {
    for aux in [false, true] {
        let op = exact_timestamp(aux);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        assert!(
            !is_x86_aarch64_native_clobber_safe_excluding(
                &builder.finish(),
                &std::collections::HashMap::new(),
            ),
            "timestamp reads have no AArch64-host guest-clock lowering"
        );
    }
}

#[test]
fn timestamp_reads_survive_o2_and_remain_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact_timestamp(false));
    builder.push_op(0x1002, exact_timestamp(true));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    let aux: Vec<_> = function
        .entry_block()
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::X86ReadTsc(read) => Some(read.dst_aux),
            _ => None,
        })
        .collect();
    assert_eq!(aux, vec![None, Some(x86(X86Reg::Rcx))]);
    assert!(is_native_clobber_safe(&function));
}
