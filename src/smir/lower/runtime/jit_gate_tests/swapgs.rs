//! Fail-closed native admission for x86 SWAPGS.

use super::*;
use crate::smir::lower::x86_64::x86_swapgs_shape_valid;

fn swapgs(gs_base: VReg, kernel_gs_base: VReg) -> OpKind {
    OpKind::X86SwapGs {
        gs_base,
        kernel_gs_base,
    }
}

fn exact_swapgs() -> OpKind {
    swapgs(x86(X86Reg::GsBase), x86(X86Reg::KernelGsBase))
}

#[test]
fn x86_swapgs_guest_state_layout_is_exact_and_appended() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, kernel_gs_base),
        X86_GUEST_KERNEL_GS_BASE_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_KERNEL_GS_BASE_OFFSET,
        X86_GUEST_CPUID_SSE4A_OFFSET + 8
    );
    assert_eq!(GuestRegs::default().kernel_gs_base, 0);
}

#[test]
fn x86_swapgs_gate_admits_only_the_exact_state_pair() {
    let exact = exact_swapgs();
    assert!(exact.is_jit_safe());
    assert!(x86_swapgs_shape_valid(&exact));
    assert!(x86_gate(exact));

    for malformed in [
        swapgs(x86(X86Reg::KernelGsBase), x86(X86Reg::GsBase)),
        swapgs(x86(X86Reg::GsBase), x86(X86Reg::FsBase)),
        swapgs(VReg::Virtual(VirtualId(0)), x86(X86Reg::KernelGsBase)),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!x86_swapgs_shape_valid(&malformed));
        assert!(!x86_gate(malformed), "malformed SWAPGS admitted");
    }
}

#[test]
fn x86_swapgs_gate_rejects_cross_host_execution() {
    let exact = exact_swapgs();
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ),
        "SWAPGS has no AArch64-host lowering"
    );
    assert!(!x86_aarch64_scalar_shape_valid(&exact));
}

#[test]
fn x86_swapgs_survives_o2_and_remains_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, exact_swapgs());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(function.entry_block().unwrap().ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::X86SwapGs {
                gs_base: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                kernel_gs_base: VReg::Arch(ArchReg::X86(X86Reg::KernelGsBase)),
            }
        )
    }));
    assert!(is_native_clobber_safe(&function));
}
