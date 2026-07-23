//! Fail-closed native admission for AMD SSE4A state-backed operations.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86Sse4aBitfieldKind};
use crate::smir::ir::types::{ArchReg, FunctionId, OpId, VReg, VirtualId, X86Reg};
use crate::smir::ir::{FunctionBuilder, Terminator};
use crate::smir::lower::runtime::GuestRegs;
use crate::smir::lower::x86_64::{x86_require_sse4a_shape_valid, x86_sse4a_bitfield_shape_valid};

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn bitfield(
    dst: VReg,
    source: VReg,
    kind: X86Sse4aBitfieldKind,
    length: Option<u8>,
    index: Option<u8>,
) -> OpKind {
    OpKind::X86Sse4aBitfield {
        dst,
        source,
        kind,
        length,
        index,
    }
}

fn function_with(kind: OpKind) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

#[test]
fn sse4a_gates_admit_exact_x86_shapes_and_reject_both_aarch64_paths() {
    let guard = SmirOp::new(OpId(0), 0x1000, OpKind::X86RequireSse4a);
    assert!(guard.kind.is_jit_safe());
    assert!(guard.is_jit_safe());
    assert!(x86_require_sse4a_shape_valid(&guard));
    assert!(x86_gate(OpKind::X86RequireSse4a));
    assert!(!aarch64_gate(vec![OpKind::X86RequireSse4a], false));
    assert!(!x86_aarch64_gate(vec![OpKind::X86RequireSse4a]));

    for kind in [
        bitfield(
            xmm(1),
            xmm(1),
            X86Sse4aBitfieldKind::Extract,
            Some(8),
            Some(4),
        ),
        bitfield(xmm(1), xmm(2), X86Sse4aBitfieldKind::Extract, None, None),
        bitfield(
            xmm(1),
            xmm(2),
            X86Sse4aBitfieldKind::Insert,
            Some(8),
            Some(4),
        ),
        bitfield(xmm(1), xmm(2), X86Sse4aBitfieldKind::Insert, None, None),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(op.kind.is_jit_safe(), "{op:?}");
        assert!(op.is_jit_safe(), "{op:?}");
        assert!(x86_sse4a_bitfield_shape_valid(&op), "{op:?}");
        assert!(x86_gate(kind.clone()), "{op:?}");
        assert!(!aarch64_gate(vec![kind.clone()], false), "{op:?}");
        assert!(!x86_aarch64_gate(vec![kind]), "{op:?}");
    }
}

#[test]
fn sse4a_gate_rejects_malformed_operands_controls_and_hints() {
    for (name, kind) in [
        (
            "virtual destination",
            bitfield(
                VReg::Virtual(VirtualId(0)),
                xmm(1),
                X86Sse4aBitfieldKind::Insert,
                None,
                None,
            ),
        ),
        (
            "extended XMM",
            bitfield(xmm(16), xmm(1), X86Sse4aBitfieldKind::Insert, None, None),
        ),
        (
            "unpaired controls",
            bitfield(xmm(1), xmm(1), X86Sse4aBitfieldKind::Extract, Some(8), None),
        ),
        (
            "out-of-range length",
            bitfield(
                xmm(1),
                xmm(1),
                X86Sse4aBitfieldKind::Extract,
                Some(64),
                Some(0),
            ),
        ),
        (
            "immediate EXTRQ source mismatch",
            bitfield(
                xmm(1),
                xmm(2),
                X86Sse4aBitfieldKind::Extract,
                Some(8),
                Some(4),
            ),
        ),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(!x86_sse4a_bitfield_shape_valid(&op), "{name}");
        assert!(!x86_gate(kind), "{name}");
    }

    for kind in [
        OpKind::X86RequireSse4a,
        bitfield(xmm(1), xmm(2), X86Sse4aBitfieldKind::Insert, None, None),
    ] {
        let mut function = function_with(kind);
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
        assert!(!is_native_clobber_safe(&function));
    }
}

#[test]
fn sse4a_state_detection_layout_and_o2_retention_are_exact() {
    assert_eq!(GuestRegs::default().xmm_state_active, 0);
    assert_eq!(
        std::mem::offset_of!(GuestRegs, xmm_state_active),
        std::mem::offset_of!(GuestRegs, umwait_control) + std::mem::size_of::<u64>()
    );
    let field_end = std::mem::offset_of!(GuestRegs, xmm_state_active) + std::mem::size_of::<u64>();
    assert!(field_end <= std::mem::size_of::<GuestRegs>());
    assert!(
        std::mem::size_of::<GuestRegs>() - field_end < std::mem::align_of::<GuestRegs>(),
        "only trailing repr(C) alignment padding may follow xmm_state_active"
    );

    let mut function = function_with(bitfield(
        xmm(1),
        xmm(2),
        X86Sse4aBitfieldKind::Insert,
        None,
        None,
    ));
    let excluded = std::collections::HashMap::new();
    assert!(uses_x86_xmm_state_excluding(&function, &excluded));

    let mut excluded_entry = std::collections::HashMap::new();
    excluded_entry.insert(function.entry, 0x1000);
    assert!(!uses_x86_xmm_state_excluding(&function, &excluded_entry));

    function.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(1), 0x1000, OpKind::X86RequireSse4a));
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert!(matches!(
        function.entry_block().unwrap().ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86RequireSse4a,
                ..
            },
            SmirOp {
                kind: OpKind::X86Sse4aBitfield { .. },
                ..
            }
        ]
    ));
    assert!(is_native_clobber_safe(&function));
}

#[test]
fn sse4a_side_effect_metadata_distinguishes_fault_guard_from_data_transform() {
    assert!(OpKind::X86RequireSse4a.has_side_effects());
    assert!(
        !bitfield(
            xmm(1),
            xmm(1),
            X86Sse4aBitfieldKind::Extract,
            Some(8),
            Some(4),
        )
        .has_side_effects()
    );
}
