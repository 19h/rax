//! Fail-closed native admission for deterministic x86 CPUID.

use super::*;

fn cpuid(
    dst_eax: X86Reg,
    dst_ebx: X86Reg,
    dst_ecx: X86Reg,
    dst_edx: X86Reg,
    leaf: X86Reg,
    subleaf: X86Reg,
) -> OpKind {
    OpKind::X86Cpuid {
        dst_eax: x86(dst_eax),
        dst_ebx: x86(dst_ebx),
        dst_ecx: x86(dst_ecx),
        dst_edx: x86(dst_edx),
        leaf: x86(leaf),
        subleaf: x86(subleaf),
    }
}

#[test]
fn x86_cpuid_gate_admits_only_the_architectural_implicit_register_shape() {
    let valid = cpuid(
        X86Reg::Rax,
        X86Reg::Rbx,
        X86Reg::Rcx,
        X86Reg::Rdx,
        X86Reg::Rax,
        X86Reg::Rcx,
    );
    assert!(valid.is_jit_safe(), "CPUID must be class-whitelisted");
    assert!(x86_gate(valid.clone()), "exact x86 CPUID must be admitted");

    for malformed in [
        cpuid(
            X86Reg::R8,
            X86Reg::Rbx,
            X86Reg::Rcx,
            X86Reg::Rdx,
            X86Reg::Rax,
            X86Reg::Rcx,
        ),
        cpuid(
            X86Reg::Rax,
            X86Reg::R9,
            X86Reg::Rcx,
            X86Reg::Rdx,
            X86Reg::Rax,
            X86Reg::Rcx,
        ),
        cpuid(
            X86Reg::Rax,
            X86Reg::Rbx,
            X86Reg::R10,
            X86Reg::Rdx,
            X86Reg::Rax,
            X86Reg::Rcx,
        ),
        cpuid(
            X86Reg::Rax,
            X86Reg::Rbx,
            X86Reg::Rcx,
            X86Reg::R11,
            X86Reg::Rax,
            X86Reg::Rcx,
        ),
        cpuid(
            X86Reg::Rax,
            X86Reg::Rbx,
            X86Reg::Rcx,
            X86Reg::Rdx,
            X86Reg::R12,
            X86Reg::Rcx,
        ),
        cpuid(
            X86Reg::Rax,
            X86Reg::Rbx,
            X86Reg::Rcx,
            X86Reg::Rdx,
            X86Reg::Rax,
            X86Reg::R13,
        ),
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!x86_gate(malformed), "malformed CPUID must deoptimize");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, valid);
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ),
        "x86 CPUID has no AArch64-host lowering and must remain a frontier there"
    );
}

#[test]
fn x86_cpuid_implicit_inputs_survive_o2_copy_propagation_and_remain_admitted() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: x86(X86Reg::Rax),
            src: SrcOperand::Reg(x86(X86Reg::R8)),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1003,
        OpKind::Mov {
            dst: x86(X86Reg::Rcx),
            src: SrcOperand::Reg(x86(X86Reg::R9)),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1006,
        cpuid(
            X86Reg::Rax,
            X86Reg::Rbx,
            X86Reg::Rcx,
            X86Reg::Rdx,
            X86Reg::Rax,
            X86Reg::Rcx,
        ),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(function.entry_block().unwrap().ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::X86Cpuid {
                leaf: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                subleaf: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                ..
            }
        )
    }));
    assert!(
        is_native_clobber_safe(&function),
        "O2 must not turn an exact CPUID region into an interpreter fallback"
    );
}
