//! Fail-closed native admission for AMD TBM guards and scalar operations.

use super::*;
use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{OpKind, SmirOp, X86TbmKind};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, MemWidth, OpId, OpWidth, SignExtend, VReg, VirtualId, X86Reg,
};
use crate::smir::ir::{FunctionBuilder, Terminator};
use crate::smir::lower::runtime::{
    is_native_clobber_safe, is_x86_aarch64_native_clobber_safe_excluding,
    x86_native_scalar_feature_requirements_excluding,
    x86_native_scalar_features_supported_excluding,
};
use crate::smir::lower::x86_64::x86_require_tbm_shape_valid;

const DEFINED: FlagSet = FlagSet::CF
    .union(FlagSet::ZF)
    .union(FlagSet::SF)
    .union(FlagSet::OF);

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn tbm(dst: VReg, src: VReg, width: OpWidth, flags: FlagUpdate) -> OpKind {
    OpKind::X86Tbm {
        dst,
        src,
        width,
        kind: X86TbmKind::Blcfill,
        flags,
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

fn memory_function(consumer: OpKind) -> crate::smir::ir::SmirFunction {
    function_with(vec![
        OpKind::X86RequireTbm,
        OpKind::Load {
            dst: VReg::Virtual(VirtualId(0)),
            addr: Address::Direct(x86(X86Reg::Rbx)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
        consumer,
    ])
}

#[test]
fn exact_tbm_guard_is_admitted_only_by_x86_guest_paths() {
    let exact = SmirOp::new(OpId(0), 0x1000, OpKind::X86RequireTbm);
    assert!(exact.kind.is_jit_safe());
    assert!(exact.is_jit_safe());
    assert!(x86_require_tbm_shape_valid(&exact));
    assert!(x86_gate(OpKind::X86RequireTbm));
    assert!(!aarch64_gate(vec![OpKind::X86RequireTbm], false));
    assert!(x86_aarch64_gate(vec![OpKind::X86RequireTbm]));
    assert!(x86_aarch64_scalar_shape_valid(&OpKind::X86RequireTbm));

    let mut hinted = function_with(vec![OpKind::X86RequireTbm]);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_require_tbm_shape_valid(&hinted.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&hinted));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
    ));
}

#[test]
fn every_tbm_kind_and_width_is_admitted_without_a_host_xop_or_bmi_requirement() {
    for tbm_kind in [
        X86TbmKind::Blcfill,
        X86TbmKind::Blci,
        X86TbmKind::Blcic,
        X86TbmKind::Blcmsk,
        X86TbmKind::Blcs,
        X86TbmKind::Blsfill,
        X86TbmKind::Blsic,
        X86TbmKind::T1mskc,
        X86TbmKind::Tzmsk,
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            let mut op_kind = tbm(
                x86(X86Reg::Rax),
                x86(X86Reg::Rcx),
                width,
                FlagUpdate::Specific(DEFINED),
            );
            let OpKind::X86Tbm {
                kind: actual_kind, ..
            } = &mut op_kind
            else {
                unreachable!()
            };
            *actual_kind = tbm_kind;
            let op = SmirOp::new(OpId(0), 0x1000, op_kind.clone());
            assert!(!op.kind.is_jit_safe(), "custom scalar gate is mandatory");
            assert!(x86_bmi_shape_valid(&op.kind));
            assert!(x86_gate(op_kind.clone()), "{tbm_kind:?}, {width:?}");
            assert!(!aarch64_gate(vec![op_kind.clone()], false));
            assert!(x86_aarch64_gate(vec![op_kind.clone()]));

            let function = function_with(vec![op_kind]);
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                ),
                (false, false, false, false, false),
                "synthetic TBM must not require host XOP/BMI"
            );
            assert!(x86_native_scalar_features_supported_excluding(
                &function,
                &std::collections::HashMap::new(),
            ));
        }
    }

    let stack_tbm = tbm(
        x86(X86Reg::Rsp),
        x86(X86Reg::Rbp),
        OpWidth::W64,
        FlagUpdate::Specific(DEFINED),
    );
    assert!(!x86_bmi_shape_valid(&stack_tbm));
    assert!(x86_gate(stack_tbm.clone()));
    assert!(x86_aarch64_gate(vec![stack_tbm]));
}

#[test]
fn tbm_gate_rejects_invalid_width_flags_operands_and_hints() {
    for (name, kind) in [
        (
            "W16",
            tbm(
                x86(X86Reg::Rax),
                x86(X86Reg::Rcx),
                OpWidth::W16,
                FlagUpdate::Specific(DEFINED),
            ),
        ),
        (
            "partial flags",
            tbm(
                x86(X86Reg::Rax),
                x86(X86Reg::Rcx),
                OpWidth::W64,
                FlagUpdate::Specific(FlagSet::CF),
            ),
        ),
        (
            "vector destination",
            tbm(
                x86(X86Reg::Xmm(0)),
                x86(X86Reg::Rcx),
                OpWidth::W64,
                FlagUpdate::Specific(DEFINED),
            ),
        ),
        (
            "immediate source",
            tbm(
                x86(X86Reg::Rax),
                VReg::Imm(1),
                OpWidth::W64,
                FlagUpdate::Specific(DEFINED),
            ),
        ),
    ] {
        assert!(!x86_gate(kind.clone()), "{name}");
        assert!(!x86_aarch64_gate(vec![kind]), "{name}");
    }

    let virtual_kind = tbm(
        VReg::Virtual(VirtualId(0)),
        VReg::Virtual(VirtualId(1)),
        OpWidth::W64,
        FlagUpdate::Specific(DEFINED),
    );
    assert!(!x86_gate(virtual_kind.clone()));
    assert!(!x86_aarch64_gate(vec![virtual_kind]));

    let mut hinted = function_with(vec![tbm(
        x86(X86Reg::Rax),
        x86(X86Reg::Rcx),
        OpWidth::W64,
        FlagUpdate::Specific(DEFINED),
    )]);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe(&hinted));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
    ));
}

#[test]
fn tbm_guard_and_semantic_op_survive_o2_in_order() {
    let mut function = function_with(vec![
        OpKind::X86RequireTbm,
        tbm(
            x86(X86Reg::Rax),
            x86(X86Reg::Rcx),
            OpWidth::W64,
            FlagUpdate::Specific(DEFINED),
        ),
    ]);
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert!(matches!(
        function.entry_block().unwrap().ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86RequireTbm,
                ..
            },
            SmirOp {
                kind: OpKind::X86Tbm { .. },
                ..
            }
        ]
    ));
    assert!(is_native_clobber_safe(&function));
    assert!(is_x86_aarch64_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
}

#[test]
fn tbm_immediate_bextr_guard_survives_o2_and_is_admitted_exactly() {
    let bextr = OpKind::Bextr {
        dst: x86(X86Reg::Rax),
        src: x86(X86Reg::Rcx),
        control: VReg::Imm(0x0804),
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
    };
    assert!(x86_gate(bextr.clone()));
    assert!(x86_aarch64_gate(vec![bextr.clone()]));
    let stack_bextr = OpKind::Bextr {
        dst: x86(X86Reg::Rsp),
        src: x86(X86Reg::Rbp),
        control: VReg::Imm(0x0804),
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
    };
    assert!(x86_gate(stack_bextr.clone()));
    assert!(x86_aarch64_gate(vec![stack_bextr]));

    let mut function = function_with(vec![OpKind::X86RequireTbm, bextr]);
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert!(matches!(
        function.entry_block().unwrap().ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86RequireTbm,
                ..
            },
            SmirOp {
                kind: OpKind::Bextr {
                    control: VReg::Imm(0x0804),
                    ..
                },
                ..
            }
        ]
    ));
    assert_eq!(
        x86_native_scalar_feature_requirements_excluding(
            &function,
            &std::collections::HashMap::new(),
        ),
        (false, true, false, false, false),
        "x86 lowering emits BMI1 BEXTR; AArch64 synthesizes the same semantics"
    );
    assert!(is_native_clobber_safe(&function));
    assert!(is_x86_aarch64_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
}

#[test]
fn tbm_memory_source_pairs_are_exact_o2_stable_and_memory_gated() {
    let temporary = VReg::Virtual(VirtualId(0));
    for consumer in [
        OpKind::X86Tbm {
            dst: x86(X86Reg::Rsp),
            src: temporary,
            width: OpWidth::W64,
            kind: X86TbmKind::Blcfill,
            flags: FlagUpdate::Specific(DEFINED),
        },
        OpKind::Bextr {
            dst: x86(X86Reg::Rbp),
            src: temporary,
            control: VReg::Imm(0x0804),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
        },
    ] {
        let mut function = memory_function(consumer);
        crate::smir::optimize::optimize_function(
            &mut function,
            crate::smir::optimize::OptLevel::O2,
        );
        assert!(matches!(
            function.entry_block().unwrap().ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86RequireTbm,
                    ..
                },
                SmirOp {
                    kind: OpKind::Load { .. },
                    ..
                },
                SmirOp {
                    kind: OpKind::X86Tbm { .. } | OpKind::Bextr { .. },
                    ..
                }
            ]
        ));
        assert!(is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            false,
        ));
        assert!(!is_x86_aarch64_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
        ));
    }
}

#[test]
fn tbm_memory_source_classifier_rejects_every_semantic_mutation() {
    let temporary = VReg::Virtual(VirtualId(0));
    let exact = OpKind::X86Tbm {
        dst: x86(X86Reg::Rax),
        src: temporary,
        width: OpWidth::W64,
        kind: X86TbmKind::Blcfill,
        flags: FlagUpdate::Specific(DEFINED),
    };
    let mut malformed = Vec::new();

    let mut signed_load = memory_function(exact.clone());
    let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *sign = SignExtend::Sign;
    malformed.push(("signed load", signed_load));

    let mut width_mismatch = memory_function(exact.clone());
    let OpKind::Load { width, .. } = &mut width_mismatch.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *width = MemWidth::B4;
    malformed.push(("width mismatch", width_mismatch));

    let mut repeated_use = memory_function(exact.clone());
    repeated_use.blocks[0].ops.push(SmirOp::new(
        OpId(99),
        0x1000,
        OpKind::Mov {
            dst: x86(X86Reg::Rdx),
            src: crate::smir::ir::types::SrcOperand::Reg(temporary),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("repeated temporary use", repeated_use));

    let mut wrong_pc = memory_function(exact.clone());
    wrong_pc.blocks[0].ops[2].guest_pc = 0x1001;
    malformed.push(("mismatched guest PC", wrong_pc));

    let mut hinted = memory_function(exact.clone());
    hinted.blocks[0].ops[2].x86_hint = Some(X86OpHint::RexByteReg);
    malformed.push(("encoding hint", hinted));

    let mut egpr = memory_function(exact.clone());
    let OpKind::X86Tbm { dst, .. } = &mut egpr.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *dst = x86(X86Reg::R16);
    malformed.push(("XOP-inexpressible EGPR", egpr));

    let invalid_control = memory_function(OpKind::Bextr {
        dst: x86(X86Reg::Rax),
        src: temporary,
        control: VReg::Imm(-1),
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
    });
    malformed.push(("non-imm32 BEXTR control", invalid_control));

    for (name, function) in malformed {
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}"
        );
    }
}
