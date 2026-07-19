//! Fail-closed native-admission coverage for RDPKRU/WRPKRU.

use super::*;

fn pkru(write: bool) -> OpKind {
    OpKind::X86Pkru {
        eax: x86(X86Reg::Rax),
        ecx: x86(X86Reg::Rcx),
        edx: x86(X86Reg::Rdx),
        pkru: x86(X86Reg::Pkru),
        write,
    }
}

#[test]
fn x86_pkru_gate_accepts_only_the_exact_fixed_implicit_shape() {
    for op in [pkru(false), pkru(true)] {
        assert!(op.is_jit_safe(), "class whitelist: {op:?}");
        assert!(
            crate::smir::lower::x86_64::x86_pkru_shape_valid(&op),
            "shape validator: {op:?}"
        );
        assert!(x86_gate(op));
    }

    for malformed in [
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rbx),
            ecx: x86(X86Reg::Rcx),
            edx: x86(X86Reg::Rdx),
            pkru: x86(X86Reg::Pkru),
            write: false,
        },
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rax),
            ecx: x86(X86Reg::R8),
            edx: x86(X86Reg::Rdx),
            pkru: x86(X86Reg::Pkru),
            write: false,
        },
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rax),
            ecx: x86(X86Reg::Rcx),
            edx: x86(X86Reg::R9),
            pkru: x86(X86Reg::Pkru),
            write: true,
        },
        OpKind::X86Pkru {
            eax: x86(X86Reg::Rax),
            ecx: x86(X86Reg::Rcx),
            edx: x86(X86Reg::Rdx),
            pkru: x86(X86Reg::GsBase),
            write: true,
        },
    ] {
        assert!(malformed.is_jit_safe(), "class whitelist is shape-agnostic");
        assert!(!crate::smir::lower::x86_64::x86_pkru_shape_valid(
            &malformed
        ));
        assert!(!x86_gate(malformed));
    }
}

#[test]
fn aarch64_cross_host_gate_rejects_guest_pkru_state_operations() {
    for op in [pkru(false), pkru(true)] {
        assert!(op.is_jit_safe());
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op.clone());
        builder.set_terminator(Terminator::Return { values: vec![] });
        assert!(
            !is_x86_aarch64_native_clobber_safe_excluding(
                &builder.finish(),
                &std::collections::HashMap::new(),
            ),
            "PKRU has no AArch64 guest-state ABI or native lowering"
        );
        assert!(!x86_aarch64_scalar_shape_valid(&op));
    }
}
