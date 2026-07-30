//! Fail-closed x86-64 admission for VBitSelect and its helper-backed load pair.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, OpId, VReg, VecWidth, VirtualId, X86Reg,
};
use crate::smir::ir::{FunctionBuilder, Terminator};
use crate::smir::lower::runtime::{
    is_native_clobber_safe, is_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    uses_x86_xmm_state_excluding, x86_jit_mem_vbit_select_sequence_len,
};
use crate::smir::lower::x86_64::x86_vbit_select_shape_valid;

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V64 | VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        VecWidth::V512 => X86Reg::Zmm(index),
    }))
}

fn select(dst: VReg, mask: VReg, src_true: VReg, src_false: VReg, width: VecWidth) -> OpKind {
    OpKind::VBitSelect {
        dst,
        mask,
        src_true,
        src_false,
        width,
    }
}

fn function_with(ops: Vec<OpKind>) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for kind in ops {
        builder.push_op(0x1000, kind);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn memory_function(width: VecWidth, memory_is_mask: bool) -> crate::smir::ir::SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx)));
    let mut function = function_with(vec![
        OpKind::X86RequireXop,
        OpKind::X86CheckAlignmentAc {
            addr: addr.clone(),
            access_size: width.bytes() as u8,
            alignment: 16,
            stack_segment: false,
            natural_alignment: false,
        },
        OpKind::VLoad {
            dst: temporary,
            addr,
            width,
        },
        if memory_is_mask {
            select(
                vector(1, width),
                temporary,
                vector(2, width),
                vector(3, width),
                width,
            )
        } else {
            select(
                vector(1, width),
                vector(3, width),
                vector(2, width),
                temporary,
                width,
            )
        },
    ]);
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    function
}

fn virtual_counts(
    function: &crate::smir::ir::SmirFunction,
) -> (
    std::collections::HashMap<VReg, usize>,
    std::collections::HashMap<VReg, usize>,
) {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &function.blocks[0].ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn memory_sequence(function: &crate::smir::ir::SmirFunction, allow_mem: bool) -> Option<usize> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_mem_vbit_select_sequence_len(&function.blocks[0], 2, allow_mem, &definitions, &uses)
}

#[test]
fn register_admission_is_target_specific_exact_and_class_whitelist_remains_closed() {
    let excluded = std::collections::HashMap::new();
    for width in [VecWidth::V128, VecWidth::V256] {
        for (dst, mask, src_true, src_false) in [
            (1, 2, 3, 4),
            (2, 2, 3, 4),
            (3, 2, 3, 4),
            (4, 2, 3, 4),
            (2, 2, 2, 2),
            (15, 12, 13, 14),
        ] {
            let kind = select(
                vector(dst, width),
                vector(mask, width),
                vector(src_true, width),
                vector(src_false, width),
                width,
            );
            let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
            assert!(!kind.is_jit_safe(), "{kind:?}");
            assert!(!op.is_jit_safe(), "{op:?}");
            assert!(x86_vbit_select_shape_valid(&op), "{op:?}");
            let function = function_with(vec![kind]);
            assert!(is_native_clobber_safe(&function));
            assert!(uses_x86_xmm_state_excluding(&function, &excluded));
            assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
        }
    }

    for kind in [
        select(
            vector(1, VecWidth::V64),
            vector(2, VecWidth::V64),
            vector(3, VecWidth::V64),
            vector(4, VecWidth::V64),
            VecWidth::V64,
        ),
        select(
            vector(1, VecWidth::V512),
            vector(2, VecWidth::V512),
            vector(3, VecWidth::V512),
            vector(4, VecWidth::V512),
            VecWidth::V512,
        ),
        select(xmm(16), xmm(2), xmm(3), xmm(4), VecWidth::V128),
        select(xmm(1), xmm(2), xmm(3), xmm(4), VecWidth::V256),
        select(
            VReg::Virtual(VirtualId(0)),
            xmm(2),
            xmm(3),
            xmm(4),
            VecWidth::V128,
        ),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(!x86_vbit_select_shape_valid(&op), "{op:?}");
        assert!(!x86_gate(kind));
    }

    let mut hinted = function_with(vec![select(xmm(1), xmm(2), xmm(3), xmm(4), VecWidth::V128)]);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe(&hinted));
}

#[test]
fn memory_pair_is_width_role_optimizer_and_memory_gate_exact() {
    let excluded = std::collections::HashMap::new();
    for width in [VecWidth::V128, VecWidth::V256] {
        for memory_is_mask in [false, true] {
            let mut function = memory_function(width, memory_is_mask);
            assert_eq!(memory_sequence(&function, true), Some(2));
            assert_eq!(memory_sequence(&function, false), None);
            assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
            assert!(!is_native_clobber_safe_excluding(
                &function, &excluded, false
            ));
            assert!(uses_x86_xmm_state_excluding(&function, &excluded));
            assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
            assert!(!aarch64_gate(
                function.blocks[0]
                    .ops
                    .iter()
                    .map(|op| op.kind.clone())
                    .collect(),
                true,
            ));
            assert!(!x86_aarch64_gate(
                function.blocks[0]
                    .ops
                    .iter()
                    .map(|op| op.kind.clone())
                    .collect(),
            ));

            crate::smir::optimize::optimize_function(
                &mut function,
                crate::smir::optimize::OptLevel::O2,
            );
            assert!(matches!(
                function.entry_block().unwrap().ops.as_slice(),
                [
                    SmirOp {
                        kind: OpKind::X86RequireXop,
                        ..
                    },
                    SmirOp {
                        kind: OpKind::X86CheckAlignmentAc { .. },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VLoad { .. },
                        ..
                    },
                    SmirOp {
                        kind: OpKind::VBitSelect { .. },
                        ..
                    }
                ]
            ));
            assert_eq!(memory_sequence(&function, true), Some(2));
            assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
        }
    }
}

#[test]
fn memory_pair_classifier_rejects_every_width_hint_role_pc_and_ssa_mutation() {
    let exact = memory_function(VecWidth::V256, true);
    let excluded = std::collections::HashMap::new();
    let mut malformed = Vec::new();

    let mut wrong_width = exact.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[2].kind {
        *width = VecWidth::V128;
    }
    malformed.push(wrong_width);

    let mut wrong_hint = exact.clone();
    wrong_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(wrong_hint);

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[3].guest_pc += 1;
    malformed.push(wrong_pc);

    let mut wrong_guard_width = exact.clone();
    if let OpKind::X86CheckAlignmentAc { access_size, .. } =
        &mut wrong_guard_width.blocks[0].ops[1].kind
    {
        *access_size = 16;
    }
    malformed.push(wrong_guard_width);

    let mut wrong_guard_address = exact.clone();
    if let OpKind::X86CheckAlignmentAc { addr, .. } = &mut wrong_guard_address.blocks[0].ops[1].kind
    {
        *addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rcx)));
    }
    malformed.push(wrong_guard_address);

    let mut missing_feature_guard = exact.clone();
    missing_feature_guard.blocks[0].ops.remove(0);
    malformed.push(missing_feature_guard);

    let mut temporary_true = exact.clone();
    let temporary = match temporary_true.blocks[0].ops[2].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    if let OpKind::VBitSelect { src_true, mask, .. } = &mut temporary_true.blocks[0].ops[3].kind {
        *src_true = temporary;
        *mask = vector(3, VecWidth::V256);
    }
    malformed.push(temporary_true);

    let mut both_roles = exact.clone();
    if let OpKind::VBitSelect {
        mask, src_false, ..
    } = &mut both_roles.blocks[0].ops[3].kind
    {
        *mask = temporary;
        *src_false = temporary;
    }
    malformed.push(both_roles);

    let mut neither_role = exact.clone();
    if let OpKind::VBitSelect { mask, .. } = &mut neither_role.blocks[0].ops[3].kind {
        *mask = vector(3, VecWidth::V256);
    }
    malformed.push(neither_role);

    for function in malformed {
        assert!(
            memory_sequence(&function, true).is_none(),
            "{:#?}",
            function.blocks[0].ops
        );
        assert!(!is_native_clobber_safe_excluding(
            &function, &excluded, true
        ));
    }
}
