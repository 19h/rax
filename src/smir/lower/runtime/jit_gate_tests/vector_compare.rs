//! Fail-closed x86-64 admission for VPCOM register and helper-backed memory forms.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, OpId, VReg, VecCmpCond, VecElementType, VecWidth, VirtualId,
    X86Reg,
};
use crate::smir::ir::{FunctionBuilder, Terminator};
use crate::smir::lower::runtime::{
    is_native_clobber_safe, is_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    uses_x86_xmm_state_excluding, x86_jit_mem_vpcom_sequence_len,
};
use crate::smir::lower::x86_64::x86_state_vcmp_shape_valid;

const CONDITIONS: &[VecCmpCond] = &[
    VecCmpCond::Eq,
    VecCmpCond::Ne,
    VecCmpCond::Lt,
    VecCmpCond::Le,
    VecCmpCond::Gt,
    VecCmpCond::Ge,
    VecCmpCond::Ltu,
    VecCmpCond::Leu,
    VecCmpCond::Gtu,
    VecCmpCond::Geu,
    VecCmpCond::False,
    VecCmpCond::True,
];
const SHAPES: &[(VecElementType, u8)] = &[
    (VecElementType::I8, 16),
    (VecElementType::I16, 8),
    (VecElementType::I32, 4),
    (VecElementType::I64, 2),
];

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn compare(
    dst: VReg,
    src1: VReg,
    src2: VReg,
    elem: VecElementType,
    lanes: u8,
    cond: VecCmpCond,
) -> OpKind {
    OpKind::VCmp {
        dst,
        src1,
        src2,
        cond,
        elem,
        lanes,
    }
}

fn function_with(ops: Vec<OpKind>) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for kind in ops {
        builder.push_op(0x1000, kind);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    for op in &mut function.blocks[0].ops {
        if matches!(op.kind, OpKind::VCmp { .. }) {
            op.x86_hint = Some(X86OpHint::XopVpcom);
        }
    }
    function
}

fn memory_function(
    elem: VecElementType,
    lanes: u8,
    cond: VecCmpCond,
) -> crate::smir::ir::SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx)));
    let mut function = function_with(vec![
        OpKind::X86RequireXop,
        OpKind::X86CheckAlignmentAc {
            addr: addr.clone(),
            access_size: 16,
            alignment: 16,
            stack_segment: false,
            natural_alignment: false,
        },
        OpKind::VLoad {
            dst: temporary,
            addr,
            width: VecWidth::V128,
        },
        compare(xmm(1), xmm(2), temporary, elem, lanes, cond),
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
    x86_jit_mem_vpcom_sequence_len(&function.blocks[0], 2, allow_mem, &definitions, &uses)
}

#[test]
fn register_admission_is_target_specific_exact_and_class_whitelist_remains_closed() {
    let excluded = std::collections::HashMap::new();
    for &(elem, lanes) in SHAPES {
        for &cond in CONDITIONS {
            for (dst, src1, src2) in [(1, 2, 3), (2, 2, 3), (3, 2, 3), (2, 2, 2), (15, 13, 14)] {
                let kind = compare(xmm(dst), xmm(src1), xmm(src2), elem, lanes, cond);
                let op = SmirOp::with_hint(OpId(0), 0x1000, kind.clone(), X86OpHint::XopVpcom);
                assert!(!kind.is_jit_safe(), "{kind:?}");
                assert!(!op.is_jit_safe(), "{op:?}");
                assert!(x86_state_vcmp_shape_valid(&op), "{op:?}");
                let function = function_with(vec![kind]);
                assert!(is_native_clobber_safe(&function));
                assert!(uses_x86_xmm_state_excluding(&function, &excluded));
                assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
                assert!(!aarch64_gate(
                    function.blocks[0]
                        .ops
                        .iter()
                        .map(|op| op.kind.clone())
                        .collect(),
                    false,
                ));
                assert!(!x86_aarch64_gate(
                    function.blocks[0]
                        .ops
                        .iter()
                        .map(|op| op.kind.clone())
                        .collect(),
                ));
            }
        }
    }

    for kind in [
        compare(
            xmm(1),
            xmm(2),
            xmm(3),
            VecElementType::F32,
            4,
            VecCmpCond::Eq,
        ),
        compare(
            xmm(1),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            15,
            VecCmpCond::Eq,
        ),
        compare(
            xmm(16),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            16,
            VecCmpCond::Eq,
        ),
        compare(
            VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            16,
            VecCmpCond::Eq,
        ),
        compare(
            VReg::Virtual(VirtualId(0)),
            xmm(2),
            xmm(3),
            VecElementType::I8,
            16,
            VecCmpCond::Eq,
        ),
    ] {
        let op = SmirOp::with_hint(OpId(0), 0x1000, kind.clone(), X86OpHint::XopVpcom);
        assert!(!x86_state_vcmp_shape_valid(&op), "{op:?}");
        assert!(!is_native_clobber_safe(&function_with(vec![kind.clone()])));
        assert!(!x86_gate(kind));
    }

    let mut hinted = function_with(vec![compare(
        xmm(1),
        xmm(2),
        xmm(3),
        VecElementType::I8,
        16,
        VecCmpCond::Ne,
    )]);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe(&hinted));

    let mut unhinted = function_with(vec![compare(
        xmm(1),
        xmm(2),
        xmm(3),
        VecElementType::I8,
        16,
        VecCmpCond::Ne,
    )]);
    unhinted.blocks[0].ops[0].x86_hint = None;
    assert!(!x86_state_vcmp_shape_valid(&unhinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&unhinted));
}

#[test]
fn memory_pair_is_shape_optimizer_and_memory_gate_exact() {
    let excluded = std::collections::HashMap::new();
    for &(elem, lanes) in SHAPES {
        for &cond in CONDITIONS {
            let mut function = memory_function(elem, lanes, cond);
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
                        kind: OpKind::VCmp { .. },
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
fn memory_pair_classifier_rejects_every_guard_hint_shape_pc_and_ssa_mutation() {
    let exact = memory_function(VecElementType::I32, 4, VecCmpCond::Ne);
    let excluded = std::collections::HashMap::new();
    let mut malformed = Vec::new();

    let mut wrong_width = exact.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[2].kind {
        *width = VecWidth::V256;
    }
    malformed.push(wrong_width);

    let mut wrong_hint = exact.clone();
    wrong_hint.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(wrong_hint);

    let mut wrong_consumer_hint = exact.clone();
    wrong_consumer_hint.blocks[0].ops[3].x86_hint = Some(X86OpHint::RexByteReg);
    malformed.push(wrong_consumer_hint);

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[3].guest_pc += 1;
    malformed.push(wrong_pc);

    let mut wrong_guard_width = exact.clone();
    if let OpKind::X86CheckAlignmentAc { access_size, .. } =
        &mut wrong_guard_width.blocks[0].ops[1].kind
    {
        *access_size = 8;
    }
    malformed.push(wrong_guard_width);

    let mut wrong_guard_alignment = exact.clone();
    if let OpKind::X86CheckAlignmentAc { alignment, .. } =
        &mut wrong_guard_alignment.blocks[0].ops[1].kind
    {
        *alignment = 8;
    }
    malformed.push(wrong_guard_alignment);

    let mut wrong_guard_address = exact.clone();
    if let OpKind::X86CheckAlignmentAc { addr, .. } = &mut wrong_guard_address.blocks[0].ops[1].kind
    {
        *addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rcx)));
    }
    malformed.push(wrong_guard_address);

    let mut missing_feature_guard = exact.clone();
    missing_feature_guard.blocks[0].ops.remove(0);
    malformed.push(missing_feature_guard);

    let mut wrong_source_role = exact.clone();
    if let OpKind::VCmp { src2, .. } = &mut wrong_source_role.blocks[0].ops[3].kind {
        *src2 = xmm(3);
    }
    malformed.push(wrong_source_role);

    let mut wrong_destination = exact.clone();
    if let OpKind::VCmp { dst, .. } = &mut wrong_destination.blocks[0].ops[3].kind {
        *dst = xmm(16);
    }
    malformed.push(wrong_destination);

    let mut wrong_source = exact.clone();
    if let OpKind::VCmp { src1, .. } = &mut wrong_source.blocks[0].ops[3].kind {
        *src1 = xmm(16);
    }
    malformed.push(wrong_source);

    let mut wrong_shape = exact.clone();
    if let OpKind::VCmp { lanes, .. } = &mut wrong_shape.blocks[0].ops[3].kind {
        *lanes = 3;
    }
    malformed.push(wrong_shape);

    let mut duplicate_use = exact.clone();
    let temporary = match duplicate_use.blocks[0].ops[2].kind {
        OpKind::VLoad { dst, .. } => dst,
        _ => unreachable!(),
    };
    duplicate_use.blocks[0].ops.push(SmirOp::new(
        OpId(99),
        0x1000,
        OpKind::VMov {
            dst: xmm(4),
            src: temporary,
            width: VecWidth::V128,
        },
    ));
    malformed.push(duplicate_use);

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
