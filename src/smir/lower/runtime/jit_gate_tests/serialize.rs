//! Native-admission coverage for instruction serialization.

use super::*;

fn serialize() -> OpKind {
    OpKind::Fence {
        kind: FenceKind::InstructionSerialize,
    }
}

#[test]
fn serialize_is_admitted_only_through_exact_target_lowerings() {
    let op = serialize();
    assert!(op.is_jit_safe(), "serialization fence is side-effect safe");
    assert!(
        x86_gate(op.clone()),
        "x86-64 CPUID barrier must be admitted"
    );
    assert!(
        aarch64_gate(vec![op.clone()], false),
        "AArch64 DSB+ISB barrier must be admitted"
    );
    assert!(
        x86_aarch64_gate(vec![op]),
        "x86 guest SERIALIZE must be admitted on an AArch64 host"
    );
}
