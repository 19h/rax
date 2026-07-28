//! Fail-closed native admission for AMD XOP packed rotate/shift operations.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign, X86XopPackedBitKind};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, OpId, SrcOperand, VReg, VecElementType, VecWidth, VirtualId,
    X86Reg,
};
use crate::smir::ir::{FunctionBuilder, Terminator};
use crate::smir::lower::runtime::{
    GuestRegs, is_native_clobber_safe, is_native_clobber_safe_excluding,
    uses_x86_native_vectors_excluding, uses_x86_xmm_state_excluding,
    x86_jit_mem_xop_source_sequence_len,
};
use crate::smir::lower::x86_64::{
    x86_check_alignment_ac_shape_valid, x86_require_xop_shape_valid, x86_xop_packed_bit_shape_valid,
};

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn xop(
    dst: VReg,
    src: VReg,
    count: SrcOperand,
    elem: VecElementType,
    kind: X86XopPackedBitKind,
) -> OpKind {
    OpKind::X86XopPackedBit {
        dst,
        src,
        count,
        elem,
        kind,
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

fn memory_function(memory_is_source: bool) -> crate::smir::ir::SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx)));
    let mut function = function_with(vec![
        OpKind::X86RequireXop,
        OpKind::X86CheckAlignmentAc {
            addr: addr.clone(),
            access_size: 16,
            alignment: 16,
            stack_segment: false,
        },
        OpKind::VLoad {
            dst: temporary,
            addr,
            width: VecWidth::V128,
        },
        if memory_is_source {
            xop(
                xmm(2),
                temporary,
                SrcOperand::Reg(xmm(4)),
                VecElementType::I16,
                X86XopPackedBitKind::LogicalShift,
            )
        } else {
            xop(
                xmm(2),
                xmm(4),
                SrcOperand::Reg(temporary),
                VecElementType::I16,
                X86XopPackedBitKind::LogicalShift,
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
    x86_jit_mem_xop_source_sequence_len(&function.blocks[0], 2, allow_mem, &definitions, &uses)
}

#[test]
fn exact_guard_and_alignment_check_are_x86_only_and_fail_closed_on_hints() {
    let guard = SmirOp::new(OpId(0), 0x1000, OpKind::X86RequireXop);
    assert!(guard.kind.is_jit_safe());
    assert!(guard.is_jit_safe());
    assert!(x86_require_xop_shape_valid(&guard));
    assert!(x86_gate(OpKind::X86RequireXop));
    assert!(!aarch64_gate(vec![OpKind::X86RequireXop], false));
    assert!(!x86_aarch64_gate(vec![OpKind::X86RequireXop]));

    let alignment = OpKind::X86CheckAlignmentAc {
        addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
        access_size: 16,
        alignment: 16,
        stack_segment: true,
    };
    let alignment_op = SmirOp::new(OpId(1), 0x1000, alignment.clone());
    assert!(x86_check_alignment_ac_shape_valid(&alignment_op));
    assert!(x86_gate(alignment.clone()));
    assert!(!aarch64_gate(vec![alignment.clone()], false));
    assert!(!x86_aarch64_gate(vec![alignment]));
    let wide_alignment = OpKind::X86CheckAlignmentAc {
        addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
        access_size: 32,
        alignment: 16,
        stack_segment: false,
    };
    assert!(x86_check_alignment_ac_shape_valid(&SmirOp::new(
        OpId(2),
        0x1000,
        wide_alignment.clone()
    )));
    assert!(x86_gate(wide_alignment));

    for kind in [
        OpKind::X86RequireXop,
        OpKind::X86CheckAlignmentAc {
            addr: Address::Absolute(0x2000),
            access_size: 16,
            alignment: 16,
            stack_segment: false,
        },
    ] {
        let mut function = function_with(vec![kind]);
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
        assert!(!is_native_clobber_safe(&function));
    }

    for malformed in [
        OpKind::X86CheckAlignmentAc {
            addr: Address::Absolute(0x2000),
            access_size: 16,
            alignment: 8,
            stack_segment: false,
        },
        OpKind::X86CheckAlignmentAc {
            addr: Address::Direct(VReg::Virtual(VirtualId(0))),
            access_size: 16,
            alignment: 16,
            stack_segment: false,
        },
        OpKind::X86CheckAlignmentAc {
            addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
            access_size: 8,
            alignment: 16,
            stack_segment: false,
        },
    ] {
        assert!(!x86_gate(malformed));
    }
}

#[test]
fn every_register_kind_element_and_count_shape_is_state_backed_not_host_xop() {
    let excluded = std::collections::HashMap::new();
    for kind in [
        X86XopPackedBitKind::Rotate,
        X86XopPackedBitKind::LogicalShift,
        X86XopPackedBitKind::ArithmeticShift,
    ] {
        for elem in [
            VecElementType::I8,
            VecElementType::I16,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for count in [
                SrcOperand::Reg(xmm(3)),
                SrcOperand::Imm(0),
                SrcOperand::Imm(255),
            ] {
                let operation = xop(xmm(1), xmm(2), count, elem, kind);
                let op = SmirOp::new(OpId(0), 0x1000, operation.clone());
                assert!(
                    !op.kind.is_jit_safe(),
                    "custom state-backed gate is mandatory"
                );
                assert!(!op.is_jit_safe(), "custom state-backed gate is mandatory");
                assert!(x86_xop_packed_bit_shape_valid(&op), "{op:?}");
                assert!(x86_gate(operation.clone()), "{op:?}");
                assert!(!aarch64_gate(vec![operation.clone()], false), "{op:?}");
                assert!(!x86_aarch64_gate(vec![operation.clone()]), "{op:?}");

                let function = function_with(vec![operation]);
                assert!(uses_x86_xmm_state_excluding(&function, &excluded));
                assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
            }
        }
    }
}

#[test]
fn register_gate_rejects_every_unencodable_operand_width_and_hint() {
    for (name, kind) in [
        (
            "high destination",
            xop(
                xmm(16),
                xmm(2),
                SrcOperand::Imm(1),
                VecElementType::I8,
                X86XopPackedBitKind::Rotate,
            ),
        ),
        (
            "high source",
            xop(
                xmm(1),
                xmm(16),
                SrcOperand::Imm(1),
                VecElementType::I8,
                X86XopPackedBitKind::Rotate,
            ),
        ),
        (
            "virtual destination",
            xop(
                VReg::Virtual(VirtualId(0)),
                xmm(2),
                SrcOperand::Imm(1),
                VecElementType::I8,
                X86XopPackedBitKind::Rotate,
            ),
        ),
        (
            "virtual count",
            xop(
                xmm(1),
                xmm(2),
                SrcOperand::Reg(VReg::Virtual(VirtualId(0))),
                VecElementType::I8,
                X86XopPackedBitKind::Rotate,
            ),
        ),
        (
            "negative immediate",
            xop(
                xmm(1),
                xmm(2),
                SrcOperand::Imm(-1),
                VecElementType::I8,
                X86XopPackedBitKind::Rotate,
            ),
        ),
        (
            "oversized immediate",
            xop(
                xmm(1),
                xmm(2),
                SrcOperand::Imm(256),
                VecElementType::I8,
                X86XopPackedBitKind::Rotate,
            ),
        ),
        (
            "floating element",
            xop(
                xmm(1),
                xmm(2),
                SrcOperand::Imm(1),
                VecElementType::F32,
                X86XopPackedBitKind::Rotate,
            ),
        ),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(!x86_xop_packed_bit_shape_valid(&op), "{name}");
        assert!(!x86_gate(kind), "{name}");
    }

    let mut hinted = function_with(vec![xop(
        xmm(1),
        xmm(2),
        SrcOperand::Imm(1),
        VecElementType::I8,
        X86XopPackedBitKind::Rotate,
    )]);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe(&hinted));
}

#[test]
fn memory_source_and_count_pairs_are_exact_o2_stable_and_memory_gated() {
    let excluded = std::collections::HashMap::new();
    for memory_is_source in [false, true] {
        let mut function = memory_function(memory_is_source);
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
                    kind: OpKind::X86XopPackedBit { .. },
                    ..
                }
            ]
        ));
        assert_eq!(memory_sequence(&function, true), Some(2));
        assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
    }
}

#[test]
fn memory_pair_classifier_rejects_all_semantic_and_ssa_mutations() {
    let exact = memory_function(true);
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

    let mut wrong_guard_width = exact.clone();
    if let OpKind::X86CheckAlignmentAc { access_size, .. } =
        &mut wrong_guard_width.blocks[0].ops[1].kind
    {
        *access_size = 32;
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

    let mut unsafe_address = exact.clone();
    if let OpKind::VLoad { addr, .. } = &mut unsafe_address.blocks[0].ops[2].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(99)));
    }
    malformed.push(unsafe_address);

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[3].guest_pc += 1;
    malformed.push(wrong_pc);

    let mut reused_temporary = exact.clone();
    reused_temporary.blocks[0].ops.push(SmirOp::new(
        OpId(99),
        0x1000,
        xop(
            xmm(5),
            VReg::Virtual(VirtualId(7)),
            SrcOperand::Imm(1),
            VecElementType::I8,
            X86XopPackedBitKind::Rotate,
        ),
    ));
    malformed.push(reused_temporary);

    let mut both_roles = exact.clone();
    if let OpKind::X86XopPackedBit { count, .. } = &mut both_roles.blocks[0].ops[3].kind {
        *count = SrcOperand::Reg(VReg::Virtual(VirtualId(7)));
    }
    malformed.push(both_roles);

    let mut neither_role = exact;
    if let OpKind::X86XopPackedBit { src, .. } = &mut neither_role.blocks[0].ops[3].kind {
        *src = xmm(3);
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

#[test]
fn xop_state_layout_and_side_effect_metadata_are_append_only_and_exact() {
    assert_eq!(GuestRegs::default().cpuid_xop, 0);
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpuid_xop),
        std::mem::offset_of!(GuestRegs, cpuid_tbm) + std::mem::size_of::<u64>()
    );
    assert!(OpKind::X86RequireXop.has_side_effects());
    assert!(
        OpKind::X86CheckAlignmentAc {
            addr: Address::Absolute(0x2000),
            access_size: 16,
            alignment: 16,
            stack_segment: false,
        }
        .has_side_effects()
    );
    assert!(
        !xop(
            xmm(1),
            xmm(2),
            SrcOperand::Imm(1),
            VecElementType::I8,
            X86XopPackedBitKind::Rotate,
        )
        .has_side_effects()
    );
}
