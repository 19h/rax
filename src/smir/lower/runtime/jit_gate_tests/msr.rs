//! Fail-closed native admission and ABI layout for x86 RDMSR/WRMSR.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86MsrOp};
use crate::smir::ir::types::OpId;
use crate::smir::lower::x86_64::x86_msr_shape_valid;
use crate::smir::lower::{
    X86_GUEST_CONTROL_WRITE_FN_OFFSET, X86_GUEST_CSTAR_OFFSET, X86_GUEST_FMASK_OFFSET,
    X86_GUEST_LSTAR_OFFSET, X86_GUEST_MSR_FN_OFFSET, X86_GUEST_STAR_OFFSET,
    X86_GUEST_SYSENTER_CS_OFFSET, X86_GUEST_SYSENTER_EIP_OFFSET, X86_GUEST_SYSENTER_ESP_OFFSET,
    X86_GUEST_TSC_ADJUST_OFFSET,
};

fn msr(eax: VReg, ecx: VReg, edx: VReg, write: bool, next_pc: u64) -> OpKind {
    OpKind::X86Msr(X86MsrOp {
        eax,
        ecx,
        edx,
        write,
        next_pc,
    })
}

fn exact_msr(write: bool, next_pc: u64) -> OpKind {
    msr(
        x86(X86Reg::Rax),
        x86(X86Reg::Rcx),
        x86(X86Reg::Rdx),
        write,
        next_pc,
    )
}

fn smir_op(kind: OpKind) -> SmirOp {
    SmirOp::new(OpId(0), 0x1000, kind)
}

#[test]
fn msr_helper_and_state_layout_is_exact_append_only_and_zero_initialized() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, msr_fn),
        X86_GUEST_MSR_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, tsc_adjust),
        X86_GUEST_TSC_ADJUST_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, star),
        X86_GUEST_STAR_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, lstar),
        X86_GUEST_LSTAR_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cstar),
        X86_GUEST_CSTAR_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, fmask),
        X86_GUEST_FMASK_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, sysenter_cs),
        X86_GUEST_SYSENTER_CS_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, sysenter_esp),
        X86_GUEST_SYSENTER_ESP_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, sysenter_eip),
        X86_GUEST_SYSENTER_EIP_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_MSR_FN_OFFSET,
        X86_GUEST_CONTROL_WRITE_FN_OFFSET + 8
    );
    assert_eq!(X86_GUEST_TSC_ADJUST_OFFSET, X86_GUEST_MSR_FN_OFFSET + 8);
    assert_eq!(X86_GUEST_STAR_OFFSET, X86_GUEST_TSC_ADJUST_OFFSET + 8);
    assert_eq!(X86_GUEST_LSTAR_OFFSET, X86_GUEST_STAR_OFFSET + 8);
    assert_eq!(X86_GUEST_CSTAR_OFFSET, X86_GUEST_LSTAR_OFFSET + 8);
    assert_eq!(X86_GUEST_FMASK_OFFSET, X86_GUEST_CSTAR_OFFSET + 8);
    assert_eq!(X86_GUEST_SYSENTER_CS_OFFSET, X86_GUEST_FMASK_OFFSET + 8);
    assert_eq!(
        X86_GUEST_SYSENTER_ESP_OFFSET,
        X86_GUEST_SYSENTER_CS_OFFSET + 8
    );
    assert_eq!(
        X86_GUEST_SYSENTER_EIP_OFFSET,
        X86_GUEST_SYSENTER_ESP_OFFSET + 8
    );

    let defaults = GuestRegs::default();
    assert_eq!(
        [
            defaults.msr_fn,
            defaults.tsc_adjust,
            defaults.star,
            defaults.lstar,
            defaults.cstar,
            defaults.fmask,
            defaults.sysenter_cs,
            defaults.sysenter_esp,
            defaults.sysenter_eip,
        ],
        [0; 9]
    );
}

#[test]
fn x86_msr_gate_admits_only_fixed_lifter_registers_and_frontiers() {
    for write in [false, true] {
        for length in [2, 3, 15] {
            let kind = exact_msr(write, 0x1000 + length);
            let op = smir_op(kind.clone());
            assert!(kind.is_jit_safe(), "write={write} len={length}");
            assert!(x86_msr_shape_valid(&op), "write={write} len={length}");
            assert!(x86_gate(kind), "write={write} len={length}");
        }
    }

    for malformed in [
        msr(
            VReg::virt(0),
            x86(X86Reg::Rcx),
            x86(X86Reg::Rdx),
            false,
            0x1002,
        ),
        msr(
            x86(X86Reg::Rax),
            VReg::virt(1),
            x86(X86Reg::Rdx),
            false,
            0x1002,
        ),
        msr(x86(X86Reg::Rax), x86(X86Reg::Rcx), arm_x(0), true, 0x1002),
        msr(
            x86(X86Reg::Rbx),
            x86(X86Reg::Rcx),
            x86(X86Reg::Rdx),
            false,
            0x1002,
        ),
        msr(
            x86(X86Reg::Rax),
            x86(X86Reg::Rbx),
            x86(X86Reg::Rdx),
            true,
            0x1002,
        ),
        msr(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            x86(X86Reg::Rbx),
            true,
            0x1002,
        ),
        exact_msr(false, 0x1001),
        exact_msr(false, 0x1010),
        exact_msr(true, 0x0FFF),
    ] {
        assert!(!x86_msr_shape_valid(&smir_op(malformed.clone())));
        assert!(!x86_gate(malformed));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact_msr(false, 0x1002));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_msr_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&hinted));
}

#[test]
fn x86_msr_gate_rejects_every_aarch64_host_path() {
    for write in [false, true] {
        let op = exact_msr(write, 0x1002);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op.clone());
        builder.set_terminator(Terminator::Return { values: vec![] });
        assert!(!is_x86_aarch64_native_clobber_safe_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ));
        assert!(!x86_aarch64_scalar_shape_valid(&op));
        assert!(!aarch64_gate(vec![op], false));
    }
}

#[test]
fn x86_msr_survives_o2_in_order_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact_msr(true, 0x1002));
    builder.push_op(0x1002, exact_msr(false, 0x1004));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    let accesses: Vec<_> = function
        .entry_block()
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::X86Msr(msr) => Some((msr.write, msr.next_pc)),
            _ => None,
        })
        .collect();
    assert_eq!(accesses, vec![(true, 0x1002), (false, 0x1004)]);
    assert!(is_native_clobber_safe(&function));
}
