//! gate::misc tests

use super::*;
use crate::smir::lower::runtime::jit_gate_tests::*;
use crate::smir::lower::runtime::*;

#[test]
fn x86_fixed_integer_compare_gate_validates_exact_shapes_encodings_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));

    for (kind, hint, requirements) in [
        (
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I8,
                lanes: 16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x74,
            },
            (false, false, false, false),
        ),
        (
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I64,
                lanes: 2,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x29,
            },
            (true, false, false, false),
        ),
        (
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                cond: VecCmpCond::Gt,
                elem: VecElementType::I64,
                lanes: 2,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x37,
            },
            (false, true, false, false),
        ),
        (
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                cond: VecCmpCond::Gt,
                elem: VecElementType::I32,
                lanes: 4,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x66,
                width: VecWidth::V128,
                w: true,
            },
            (false, false, true, false),
        ),
        (
            OpKind::VCmp {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I16,
                lanes: 16,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x75,
                width: VecWidth::V256,
                w: false,
            },
            (false, false, true, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_compare_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let unhinted = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VCmp {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            cond: VecCmpCond::Eq,
            elem: VecElementType::I32,
            lanes: 4,
        },
    );
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed_kind in [
        OpKind::VCmp {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            cond: VecCmpCond::Ne,
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VCmp {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            cond: VecCmpCond::Eq,
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VCmp {
            dst: x86(X86Reg::Zmm(1)),
            src1: x86(X86Reg::Zmm(2)),
            src2: x86(X86Reg::Zmm(3)),
            cond: VecCmpCond::Gt,
            elem: VecElementType::I64,
            lanes: 8,
        },
        OpKind::VCmp {
            dst: xmm1,
            src1: VReg::Virtual(VirtualId(30)),
            src2: xmm2,
            cond: VecCmpCond::Eq,
            elem: VecElementType::I8,
            lanes: 16,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed_kind));
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I8,
                lanes: 16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x74,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VCmp {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I16,
                lanes: 8,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x74,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VCmp {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I64,
                lanes: 4,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x29,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VCmp {
                dst: x86(X86Reg::Xmm(16)),
                src1: x86(X86Reg::Xmm(17)),
                src2: x86(X86Reg::Xmm(18)),
                cond: VecCmpCond::Gt,
                elem: VecElementType::I32,
                lanes: 4,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x66,
                width: VecWidth::V128,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VCmp {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                cond: VecCmpCond::Eq,
                elem: VecElementType::I16,
                lanes: 16,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x75,
                width: VecWidth::V256,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed));
    }
}
#[test]
fn x86_integer_interleave_gate_validates_blocks_encodings_registers_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let zmm16 = x86(X86Reg::Zmm(16));
    let zmm17 = x86(X86Reg::Zmm(17));
    let zmm18 = x86(X86Reg::Zmm(18));

    for (kind, hint, requirements) in [
        (
            OpKind::VInterleave {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 16,
                high: false,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x60,
            },
            (false, false, false),
        ),
        (
            OpKind::VInterleave {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                elem: VecElementType::I64,
                lanes: 2,
                block_lanes: 2,
                high: true,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6D,
                width: VecWidth::V128,
                w: true,
            },
            (true, false, false),
        ),
        (
            OpKind::VInterleave {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I16,
                lanes: 16,
                block_lanes: 8,
                high: true,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x69,
                width: VecWidth::V256,
                w: false,
            },
            (true, true, false),
        ),
        (
            OpKind::VInterleave {
                dst: zmm16,
                src1: zmm17,
                src2: zmm18,
                elem: VecElementType::I32,
                lanes: 16,
                block_lanes: 4,
                high: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x62,
                width: VecWidth::V512,
                w: false,
            },
            (false, false, false),
        ),
        (
            OpKind::VInterleave {
                dst: x86(X86Reg::Xmm(16)),
                src1: x86(X86Reg::Xmm(17)),
                src2: x86(X86Reg::Xmm(18)),
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 16,
                high: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x60,
                width: VecWidth::V128,
                w: true,
            },
            (false, false, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_interleave_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    for malformed_kind in [
        OpKind::VInterleave {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I8,
            lanes: 16,
            block_lanes: 8,
            high: false,
        },
        OpKind::VInterleave {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::F32,
            lanes: 4,
            block_lanes: 4,
            high: false,
        },
        OpKind::VInterleave {
            dst: xmm1,
            src1: VReg::Virtual(VirtualId(40)),
            src2: xmm2,
            elem: VecElementType::I32,
            lanes: 4,
            block_lanes: 4,
            high: false,
        },
        OpKind::VInterleave {
            dst: xmm1,
            src1: xmm2,
            src2: ymm3,
            elem: VecElementType::I32,
            lanes: 4,
            block_lanes: 4,
            high: false,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed_kind));
    }

    let unhinted = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VInterleave {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I8,
            lanes: 16,
            block_lanes: 16,
            high: false,
        },
    );
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VInterleave {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 16,
                high: false,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x60,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VInterleave {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I16,
                lanes: 16,
                block_lanes: 8,
                high: true,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x69,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VInterleave {
                dst: x86(X86Reg::Ymm(16)),
                src1: x86(X86Reg::Ymm(17)),
                src2: x86(X86Reg::Ymm(18)),
                elem: VecElementType::I32,
                lanes: 8,
                block_lanes: 4,
                high: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x62,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VInterleave {
                dst: zmm16,
                src1: zmm17,
                src2: zmm18,
                elem: VecElementType::I32,
                lanes: 16,
                block_lanes: 4,
                high: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x62,
                width: VecWidth::V512,
                w: true,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed));
    }
}
#[test]
fn x86_saturating_pack_gate_validates_shapes_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let pack = |dst, src1, src2, src_elem, to_unsigned, src_lanes, block_lanes| OpKind::VPackSat {
        dst,
        src1,
        src2,
        src_elem,
        to_unsigned,
        src_lanes,
        block_lanes,
    };

    for (kind, hint, requirements) in [
        (
            pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x63,
            },
            (false, false, false, false),
        ),
        (
            pack(xmm1, xmm2, xmm1, VecElementType::I32, true, 4, 4),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x2B,
            },
            (true, false, false, false),
        ),
        (
            pack(xmm1, xmm3, xmm2, VecElementType::I32, true, 4, 4),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x2B,
                width: VecWidth::V128,
                w: true,
            },
            (false, true, false, false),
        ),
        (
            pack(ymm1, ymm3, ymm2, VecElementType::I32, false, 8, 4),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6B,
                width: VecWidth::V256,
                w: false,
            },
            (false, true, true, false),
        ),
        (
            pack(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(18)),
                x86(X86Reg::Zmm(17)),
                VecElementType::I16,
                true,
                32,
                8,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x67,
                width: VecWidth::V512,
                w: true,
            },
            (false, false, false, false),
        ),
        (
            pack(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(18)),
                x86(X86Reg::Xmm(17)),
                VecElementType::I32,
                true,
                4,
                4,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x2B,
                width: VecWidth::V128,
                w: false,
            },
            (false, false, false, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_pack_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    for malformed_kind in [
        pack(xmm1, xmm2, xmm1, VecElementType::I8, false, 16, 16),
        pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 7, 8),
        pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 4),
        pack(
            xmm1,
            VReg::Virtual(VirtualId(50)),
            xmm1,
            VecElementType::I16,
            false,
            8,
            8,
        ),
        pack(xmm1, ymm2, xmm1, VecElementType::I32, false, 4, 4),
    ] {
        assert!(!is_x86_native_vector_op(&malformed_kind));
    }

    let unhinted = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 8),
    );
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            pack(xmm1, xmm2, xmm3, VecElementType::I16, false, 8, 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x63,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            pack(xmm1, xmm2, xmm1, VecElementType::I16, false, 8, 8),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x67,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            pack(ymm1, ymm3, ymm2, VecElementType::I32, true, 8, 4),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x2B,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            pack(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(18)),
                x86(X86Reg::Ymm(17)),
                VecElementType::I16,
                true,
                16,
                8,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x67,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            pack(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(18)),
                x86(X86Reg::Zmm(17)),
                VecElementType::I32,
                false,
                16,
                4,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6B,
                width: VecWidth::V512,
                w: true,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            pack(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(18)),
                x86(X86Reg::Ymm(17)),
                VecElementType::I16,
                false,
                16,
                8,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x63,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_byte_shuffle_gate_validates_blocks_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let shuffle = |dst, src, control, lanes, block_lanes| OpKind::VByteShuffle {
        dst,
        src,
        control,
        lanes,
        block_lanes,
    };

    for (kind, hint, requirements) in [
        (
            shuffle(xmm1, xmm1, xmm2, 16, 16),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x00,
            },
            (true, false, false, false),
        ),
        (
            shuffle(xmm1, xmm2, xmm3, 16, 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V128,
                w: true,
            },
            (false, true, false, false),
        ),
        (
            shuffle(ymm1, ymm2, ymm3, 32, 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V256,
                w: false,
            },
            (false, true, true, false),
        ),
        (
            shuffle(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                64,
                16,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V512,
                w: true,
            },
            (false, false, false, false),
        ),
        (
            shuffle(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                x86(X86Reg::Xmm(18)),
                16,
                16,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V128,
                w: false,
            },
            (false, false, false, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_byte_shuffle_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    for malformed_kind in [
        shuffle(xmm1, xmm1, xmm2, 16, 8),
        shuffle(xmm1, xmm1, xmm2, 15, 16),
        shuffle(xmm1, VReg::Virtual(VirtualId(60)), xmm2, 16, 16),
        shuffle(xmm1, xmm2, ymm3, 16, 16),
    ] {
        assert!(!is_x86_native_vector_op(&malformed_kind));
    }

    let unhinted = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        shuffle(xmm1, xmm1, xmm2, 16, 16),
    );
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            shuffle(xmm1, xmm2, xmm3, 16, 16),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x00,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            shuffle(xmm1, xmm1, xmm2, 16, 16),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x00,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            shuffle(ymm1, ymm2, ymm3, 32, 16),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            shuffle(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                32,
                16,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            shuffle(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                64,
                16,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x01,
                width: VecWidth::V512,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            shuffle(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                32,
                16,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x00,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_horizontal_integer_gate_validates_modes_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let horizontal =
        |dst, src1, src2, elem, lanes, block_lanes, subtract, saturating| OpKind::VHorizontalBin {
            dst,
            src1,
            src2,
            elem,
            lanes,
            block_lanes,
            subtract,
            saturating,
        };

    for (opcode, elem, subtract, saturating) in [
        (0x01, VecElementType::I16, false, false),
        (0x02, VecElementType::I32, false, false),
        (0x03, VecElementType::I16, false, true),
        (0x05, VecElementType::I16, true, false),
        (0x06, VecElementType::I32, true, false),
        (0x07, VecElementType::I16, true, true),
    ] {
        let block_lanes = (16 / elem.bytes()) as u8;
        for (kind, hint, requirements) in [
            (
                horizontal(
                    xmm1,
                    xmm1,
                    xmm2,
                    elem,
                    VecWidth::V128.lanes(elem) as u8,
                    block_lanes,
                    subtract,
                    saturating,
                ),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode,
                },
                (true, false, false),
            ),
            (
                horizontal(
                    ymm1,
                    ymm2,
                    ymm3,
                    elem,
                    VecWidth::V256.lanes(elem) as u8,
                    block_lanes,
                    subtract,
                    saturating,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode,
                    width: VecWidth::V256,
                    w: true,
                },
                (false, true, true),
            ),
        ] {
            let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                kind.clone(),
                hint,
            );
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_horizontal_feature_requirements(&smir_op),
                requirements,
                "{smir_op:?}"
            );

            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = Some(hint);
            assert!(is_native_clobber_safe(&function), "{smir_op:?}");
        }
    }

    let vex128 = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        horizontal(xmm1, xmm2, xmm3, VecElementType::I16, 8, 8, false, false),
        X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x01,
            width: VecWidth::V128,
            w: true,
        },
    );
    assert!(x86_native_vector_smir_op(&vex128));
    assert_eq!(
        x86_vector_integer_horizontal_feature_requirements(&vex128),
        (false, true, false)
    );

    for malformed_kind in [
        horizontal(xmm1, xmm1, xmm2, VecElementType::I16, 8, 4, false, false),
        horizontal(xmm1, xmm1, xmm2, VecElementType::I32, 4, 4, false, true),
        horizontal(
            x86(X86Reg::Zmm(1)),
            x86(X86Reg::Zmm(2)),
            x86(X86Reg::Zmm(3)),
            VecElementType::I16,
            32,
            8,
            false,
            false,
        ),
        horizontal(
            xmm1,
            VReg::Virtual(VirtualId(61)),
            xmm2,
            VecElementType::I16,
            8,
            8,
            false,
            false,
        ),
        horizontal(xmm1, xmm2, ymm3, VecElementType::I16, 8, 8, false, false),
    ] {
        assert!(!is_x86_native_vector_op(&malformed_kind));
    }

    let valid = horizontal(xmm1, xmm1, xmm2, VecElementType::I16, 8, 8, false, false);
    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, valid.clone());
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            horizontal(xmm1, xmm2, xmm3, VecElementType::I16, 8, 8, false, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x01,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x01,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid,
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x02,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            horizontal(ymm1, ymm2, ymm3, VecElementType::I32, 8, 4, true, false),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x06,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            horizontal(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecElementType::I16,
                16,
                8,
                false,
                true,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x03,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            horizontal(xmm1, xmm2, xmm3, VecElementType::I16, 8, 8, true, true),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x07,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_pavg_gate_validates_rounding_shape_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let average = |dst, src1, src2, elem, lanes| OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op: VLaneOp::AvgRnd,
        signed: false,
        set_ovf: false,
    };

    for (kind, hint, requirements) in [
        (
            average(xmm1, xmm1, xmm2, VecElementType::I8, 16),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE0,
            },
            (false, false, false),
        ),
        (
            average(xmm1, xmm2, xmm3, VecElementType::I16, 8),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE3,
                width: VecWidth::V128,
                w: true,
            },
            (true, false, false),
        ),
        (
            average(ymm1, ymm2, ymm3, VecElementType::I8, 32),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE0,
                width: VecWidth::V256,
                w: false,
            },
            (true, true, false),
        ),
        (
            average(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                x86(X86Reg::Xmm(18)),
                VecElementType::I16,
                8,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE3,
                width: VecWidth::V128,
                w: true,
            },
            (false, false, true),
        ),
        (
            average(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecElementType::I8,
                64,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE0,
                width: VecWidth::V512,
                w: false,
            },
            (false, false, false),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_vector_integer_average_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_average_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let valid = average(xmm1, xmm1, xmm2, VecElementType::I8, 16);
    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, valid.clone());
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed_kind in [
        OpKind::VLane {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I8,
            lanes: 16,
            op: VLaneOp::Avg,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I8,
            lanes: 16,
            op: VLaneOp::AvgRnd,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I8,
            lanes: 16,
            op: VLaneOp::AvgRnd,
            signed: false,
            set_ovf: true,
        },
        average(xmm1, xmm1, xmm2, VecElementType::I32, 4),
        average(xmm1, xmm1, xmm2, VecElementType::I8, 15),
        average(xmm1, xmm1, ymm3, VecElementType::I8, 16),
        average(
            xmm1,
            VReg::Virtual(VirtualId(61)),
            xmm2,
            VecElementType::I8,
            16,
        ),
    ] {
        assert!(
            !is_x86_native_vector_op(&malformed_kind),
            "{malformed_kind:?}"
        );
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            average(xmm1, xmm2, xmm3, VecElementType::I8, 16),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE0,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xE0,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid,
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE3,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            average(ymm1, ymm2, ymm3, VecElementType::I8, 32),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE0,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            average(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecElementType::I16,
                16,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE3,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            average(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecElementType::I8,
                64,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE3,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_psadbw_gate_validates_widths_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let sad = |dst, src1, src2, width| OpKind::VSadBytes {
        dst,
        src1,
        src2,
        width,
    };

    for (kind, hint, requirements) in [
        (
            sad(xmm1, xmm1, xmm2, VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF6,
            },
            (false, false, false),
        ),
        (
            sad(xmm1, xmm2, xmm3, VecWidth::V128),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF6,
                width: VecWidth::V128,
                w: true,
            },
            (true, false, false),
        ),
        (
            sad(ymm1, ymm2, ymm3, VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF6,
                width: VecWidth::V256,
                w: false,
            },
            (true, true, false),
        ),
        (
            sad(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                x86(X86Reg::Xmm(18)),
                VecWidth::V128,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF6,
                width: VecWidth::V128,
                w: true,
            },
            (false, false, true),
        ),
        (
            sad(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecWidth::V512,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF6,
                width: VecWidth::V512,
                w: false,
            },
            (false, false, false),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_vector_sad_bytes_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_sad_bytes_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let valid = sad(xmm1, xmm1, xmm2, VecWidth::V128);
    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, valid.clone());
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed_kind in [
        sad(xmm1, xmm1, ymm2, VecWidth::V128),
        sad(ymm1, ymm2, ymm3, VecWidth::V128),
        sad(VReg::Virtual(VirtualId(61)), xmm1, xmm2, VecWidth::V128),
    ] {
        assert!(
            !is_x86_native_vector_op(&malformed_kind),
            "{malformed_kind:?}"
        );
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            sad(xmm1, xmm2, xmm3, VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF6,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xF6,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid,
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xE0,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            sad(ymm1, ymm2, ymm3, VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF6,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            sad(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecWidth::V256,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF6,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            sad(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecWidth::V512,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF6,
                width: VecWidth::V256,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_mov_mask_gate_validates_shapes_encodings_wig_and_features() {
    let xmm = |index| x86(X86Reg::Xmm(index));
    let ymm = |index| x86(X86Reg::Ymm(index));
    let mov_mask = |dst, src, elem, lanes, dst_width| OpKind::X86MovMask {
        dst,
        src,
        elem,
        lanes,
        dst_width,
    };

    for (kind, hint, requirements) in [
        (
            mov_mask(
                x86(X86Reg::Rax),
                xmm(1),
                VecElementType::F32,
                4,
                OpWidth::W32,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x50,
            },
            (false, false),
        ),
        (
            mov_mask(
                x86(X86Reg::R8),
                xmm(9),
                VecElementType::F64,
                2,
                OpWidth::W64,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x50,
            },
            (false, false),
        ),
        (
            mov_mask(
                x86(X86Reg::R9),
                xmm(10),
                VecElementType::I8,
                16,
                OpWidth::W32,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xD7,
            },
            (false, false),
        ),
        (
            mov_mask(
                x86(X86Reg::R8),
                ymm(9),
                VecElementType::F32,
                8,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x50,
                width: VecWidth::V256,
                w: true,
            },
            (true, false),
        ),
        (
            mov_mask(
                x86(X86Reg::Rdx),
                ymm(1),
                VecElementType::F64,
                4,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x50,
                width: VecWidth::V256,
                w: false,
            },
            (true, false),
        ),
        (
            mov_mask(
                x86(X86Reg::Rax),
                xmm(1),
                VecElementType::I8,
                16,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD7,
                width: VecWidth::V128,
                w: true,
            },
            (true, false),
        ),
        (
            mov_mask(
                x86(X86Reg::R9),
                ymm(10),
                VecElementType::I8,
                32,
                OpWidth::W32,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD7,
                width: VecWidth::V256,
                w: true,
            },
            (true, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_mov_mask_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_mov_mask_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    for malformed in [
        mov_mask(
            x86(X86Reg::Rsp),
            xmm(1),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        mov_mask(
            x86(X86Reg::Rbp),
            xmm(1),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        mov_mask(
            VReg::Virtual(VirtualId(63)),
            xmm(1),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        mov_mask(
            x86(X86Reg::Rax),
            xmm(16),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        mov_mask(
            x86(X86Reg::Rax),
            ymm(1),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        mov_mask(
            x86(X86Reg::Rax),
            xmm(1),
            VecElementType::I16,
            8,
            OpWidth::W32,
        ),
        mov_mask(
            x86(X86Reg::Rax),
            xmm(1),
            VecElementType::I8,
            16,
            OpWidth::W16,
        ),
    ] {
        assert!(!x86_mov_mask_shape_valid(&malformed), "{malformed:?}");
        assert!(!is_x86_native_vector_op(&malformed), "{malformed:?}");
    }

    let base = mov_mask(
        x86(X86Reg::Rax),
        xmm(1),
        VecElementType::F32,
        4,
        OpWidth::W32,
    );
    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, base.clone());
    assert!(is_x86_native_vector_op(&base));
    assert!(!x86_native_vector_smir_op(&unhinted));

    let vex_wide_destination = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        mov_mask(
            x86(X86Reg::Rax),
            xmm(1),
            VecElementType::F32,
            4,
            OpWidth::W64,
        ),
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x50,
            width: VecWidth::V128,
            w: false,
        },
    );
    assert!(!x86_native_vector_smir_op(&vex_wide_destination));

    for hint in [
        X86OpHint::SseOp {
            prefix: X86SsePrefix::OpSize,
            opcode: 0x50,
        },
        X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode: 0xD7,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::None,
            opcode: 0x50,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x51,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x50,
            width: VecWidth::V256,
            w: false,
        },
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x50,
            width: VecWidth::V128,
            w: false,
        },
    ] {
        let malformed = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            base.clone(),
            hint,
        );
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_mpsadbw_gate_validates_widths_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let mpsad = |dst, src1, src2, width, imm| OpKind::VMpsadbw {
        dst,
        src1,
        src2,
        mask: None,
        width,
        imm,
        zeroing: false,
    };

    for (kind, hint, requirements) in [
        (
            mpsad(xmm1, xmm1, xmm2, VecWidth::V128, 0xE7),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x42,
            },
            (true, false, false),
        ),
        (
            mpsad(xmm1, xmm2, xmm3, VecWidth::V128, 0xFF),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V128,
                w: true,
            },
            (false, true, false),
        ),
        (
            mpsad(ymm1, ymm2, ymm3, VecWidth::V256, 0x3F),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V256,
                w: false,
            },
            (false, true, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_vector_mpsadbw_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_mpsadbw_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let valid = mpsad(xmm1, xmm1, xmm2, VecWidth::V128, 0xFF);
    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, valid.clone());
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed_kind in [
        mpsad(xmm1, xmm1, ymm2, VecWidth::V128, 0),
        mpsad(ymm1, ymm2, ymm3, VecWidth::V128, 0),
        mpsad(
            x86(X86Reg::Zmm(1)),
            x86(X86Reg::Zmm(2)),
            x86(X86Reg::Zmm(3)),
            VecWidth::V512,
            0,
        ),
        mpsad(VReg::Virtual(VirtualId(61)), xmm1, xmm2, VecWidth::V128, 0),
        OpKind::VMpsadbw {
            dst: xmm1,
            src1: xmm2,
            src2: xmm3,
            mask: Some(x86(X86Reg::K(3))),
            width: VecWidth::V128,
            imm: 0,
            zeroing: false,
        },
        OpKind::VMpsadbw {
            dst: xmm1,
            src1: xmm2,
            src2: xmm3,
            mask: None,
            width: VecWidth::V128,
            imm: 0,
            zeroing: true,
        },
    ] {
        assert!(
            !is_x86_native_vector_op(&malformed_kind),
            "{malformed_kind:?}"
        );
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            mpsad(xmm1, xmm2, xmm3, VecWidth::V128, 0),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x42,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x42,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid,
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF6,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            mpsad(ymm1, ymm2, ymm3, VecWidth::V256, 0),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            mpsad(ymm1, ymm2, ymm3, VecWidth::V256, 0),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::None,
                opcode: 0x42,
                width: VecWidth::V128,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            mpsad(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecWidth::V256,
                0,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            mpsad(xmm1, xmm2, xmm3, VecWidth::V128, 0),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0x42,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_maddubs_gate_validates_zero_accumulator_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let maddubs = |dst, src1, src2, width| OpKind::VDotProduct {
        dst,
        acc: VReg::Imm(0),
        src1,
        src2,
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I16,
        width,
        src1_unsigned: true,
        saturate: true,
        zeroing: false,
    };

    for (kind, hint, requirements) in [
        (
            maddubs(xmm1, xmm1, xmm2, VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x04,
            },
            (true, false, false, false),
        ),
        (
            maddubs(xmm1, xmm2, xmm3, VecWidth::V128),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V128,
                w: true,
            },
            (false, true, false, false),
        ),
        (
            maddubs(ymm1, ymm2, ymm3, VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V256,
                w: false,
            },
            (false, true, true, false),
        ),
        (
            maddubs(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                x86(X86Reg::Xmm(18)),
                VecWidth::V128,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V128,
                w: true,
            },
            (false, false, false, true),
        ),
        (
            maddubs(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecWidth::V512,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V512,
                w: false,
            },
            (false, false, false, false),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_vector_integer_maddubs_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_maddubs_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let valid = maddubs(xmm1, xmm1, xmm2, VecWidth::V128);
    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, valid.clone());
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    let altered =
        |acc, mask, src_elem, acc_elem, src1_unsigned, saturate, zeroing| OpKind::VDotProduct {
            dst: xmm1,
            acc,
            src1: xmm1,
            src2: xmm2,
            mask,
            src_elem,
            acc_elem,
            width: VecWidth::V128,
            src1_unsigned,
            saturate,
            zeroing,
        };
    for malformed_kind in [
        altered(
            VReg::Imm(1),
            None,
            VecElementType::I8,
            VecElementType::I16,
            true,
            true,
            false,
        ),
        altered(
            VReg::Imm(0),
            Some(x86(X86Reg::K(1))),
            VecElementType::I8,
            VecElementType::I16,
            true,
            true,
            false,
        ),
        altered(
            VReg::Imm(0),
            None,
            VecElementType::I16,
            VecElementType::I16,
            true,
            true,
            false,
        ),
        altered(
            VReg::Imm(0),
            None,
            VecElementType::I8,
            VecElementType::I32,
            true,
            true,
            false,
        ),
        altered(
            VReg::Imm(0),
            None,
            VecElementType::I8,
            VecElementType::I16,
            false,
            true,
            false,
        ),
        altered(
            VReg::Imm(0),
            None,
            VecElementType::I8,
            VecElementType::I16,
            true,
            false,
            false,
        ),
        altered(
            VReg::Imm(0),
            None,
            VecElementType::I8,
            VecElementType::I16,
            true,
            true,
            true,
        ),
        maddubs(xmm1, xmm2, ymm3, VecWidth::V128),
        maddubs(xmm1, VReg::Virtual(VirtualId(62)), xmm2, VecWidth::V128),
    ] {
        assert!(
            !is_x86_native_vector_op(&malformed_kind),
            "{malformed_kind:?}"
        );
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddubs(xmm1, xmm2, xmm3, VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x04,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x04,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid,
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x05,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddubs(ymm1, ymm2, ymm3, VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddubs(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecWidth::V256,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddubs(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecWidth::V512,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x05,
                width: VecWidth::V512,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddubs(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecWidth::V256,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x04,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_maddwd_gate_validates_wrapping_shape_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let maddwd = |dst, src1, src2, width| OpKind::VDotProduct {
        dst,
        acc: VReg::Imm(0),
        src1,
        src2,
        mask: None,
        src_elem: VecElementType::I16,
        acc_elem: VecElementType::I32,
        width,
        src1_unsigned: false,
        saturate: false,
        zeroing: false,
    };

    for (kind, hint, requirements) in [
        (
            maddwd(xmm1, xmm1, xmm2, VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF5,
            },
            (false, false, false),
        ),
        (
            maddwd(xmm1, xmm2, xmm3, VecWidth::V128),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V128,
                w: true,
            },
            (true, false, false),
        ),
        (
            maddwd(ymm1, ymm2, ymm3, VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V256,
                w: false,
            },
            (true, true, false),
        ),
        (
            maddwd(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                x86(X86Reg::Xmm(18)),
                VecWidth::V128,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V128,
                w: true,
            },
            (false, false, true),
        ),
        (
            maddwd(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecWidth::V512,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V512,
                w: false,
            },
            (false, false, false),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_vector_integer_maddwd_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_maddwd_feature_requirements(&smir_op),
            requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let valid = maddwd(xmm1, xmm1, xmm2, VecWidth::V128);
    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, valid.clone());
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    let altered = |acc, mask, src1_unsigned, saturate, zeroing| OpKind::VDotProduct {
        dst: xmm1,
        acc,
        src1: xmm1,
        src2: xmm2,
        mask,
        src_elem: VecElementType::I16,
        acc_elem: VecElementType::I32,
        width: VecWidth::V128,
        src1_unsigned,
        saturate,
        zeroing,
    };
    for malformed_kind in [
        altered(VReg::Imm(1), None, false, false, false),
        altered(VReg::Imm(0), Some(x86(X86Reg::K(1))), false, false, false),
        altered(VReg::Imm(0), None, true, false, false),
        altered(VReg::Imm(0), None, false, true, false),
        altered(VReg::Imm(0), None, false, false, true),
        maddwd(xmm1, xmm2, ymm3, VecWidth::V128),
        maddwd(xmm1, VReg::Virtual(VirtualId(63)), xmm2, VecWidth::V128),
    ] {
        assert!(
            !is_x86_native_vector_op(&malformed_kind),
            "{malformed_kind:?}"
        );
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddwd(xmm1, xmm2, xmm3, VecWidth::V128),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF5,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid,
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xF5,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddwd(ymm1, ymm2, ymm3, VecWidth::V256),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddwd(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecWidth::V256,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF5,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            maddwd(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecWidth::V512,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF4,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn andn_gate_accepts_only_register_bmi_and_apx_nf_shapes() {
    let defined = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    for (name, op) in [
        (
            "VEX flagful",
            OpKind::AndNot {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                width: OpWidth::W32,
                flags: FlagUpdate::Specific(defined),
            },
        ),
        (
            "APX NF aliased",
            OpKind::AndNot {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        (
            "state-backed flagful",
            OpKind::AndNot {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rbp),
                src2: SrcOperand::Reg(x86(X86Reg::R16)),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(defined),
            },
        ),
        (
            "state-backed NF",
            OpKind::AndNot {
                dst: x86(X86Reg::R31),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ),
        (
            "state-backed NF all operands alias",
            OpKind::AndNot {
                dst: x86(X86Reg::R16),
                src1: x86(X86Reg::R16),
                src2: SrcOperand::Reg(x86(X86Reg::R16)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
    ] {
        assert!(
            !op.is_jit_safe(),
            "{name} must remain scoped to the x86 exact-shape gate"
        );
        assert!(x86_gate(op.clone()), "{name} must pass the exact gate");
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(
            x86_native_scalar_feature_requirements_excluding(
                &builder.finish(),
                &std::collections::HashMap::new()
            ),
            (false, false, false, false, false),
            "generic lowering must not require host BMI1"
        );
    }

    for (name, op) in [
        (
            "word width",
            OpKind::AndNot {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                width: OpWidth::W16,
                flags: FlagUpdate::Specific(defined),
            },
        ),
        (
            "overbroad flags",
            OpKind::AndNot {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "immediate source",
            OpKind::AndNot {
                dst: x86(X86Reg::Rax),
                src1: x86(X86Reg::Rcx),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(defined),
            },
        ),
        (
            "state-backed word width",
            OpKind::AndNot {
                dst: x86(X86Reg::R16),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
        (
            "state-backed immediate source",
            OpKind::AndNot {
                dst: x86(X86Reg::R31),
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        (
            "state-backed virtual source",
            OpKind::AndNot {
                dst: x86(X86Reg::R31),
                src1: VReg::Virtual(VirtualId(0)),
                src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        (
            "state-backed overbroad flags",
            OpKind::AndNot {
                dst: x86(X86Reg::Rsp),
                src1: x86(X86Reg::Rbp),
                src2: SrcOperand::Reg(x86(X86Reg::R16)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "virtual source",
            OpKind::AndNot {
                dst: x86(X86Reg::Rax),
                src1: VReg::Virtual(VirtualId(0)),
                src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
    ] {
        assert!(
            !op.is_jit_safe(),
            "{name} must remain outside the shared architecture whitelist"
        );
        assert!(!x86_gate(op), "malformed {name} must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::AndNot {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed ANDN must fail closed"
    );
}
#[test]
fn x86_bls_gate_is_architecture_scoped_and_requires_exact_bmi1_shapes() {
    let defined = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    for op in [
        OpKind::X86Bls {
            dst: x86(X86Reg::R8),
            src: x86(X86Reg::R9),
            width: OpWidth::W32,
            kind: X86BlsKind::Blsr,
            flags: FlagUpdate::Specific(defined),
        },
        OpKind::X86Bls {
            dst: x86(X86Reg::R15),
            src: x86(X86Reg::R15),
            width: OpWidth::W64,
            kind: X86BlsKind::Blsi,
            flags: FlagUpdate::None,
        },
        OpKind::X86Bls {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            kind: X86BlsKind::Blsr,
            flags: FlagUpdate::Specific(defined),
        },
        OpKind::X86Bls {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W32,
            kind: X86BlsKind::Blsmsk,
            flags: FlagUpdate::None,
        },
        OpKind::X86Bls {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R16),
            width: OpWidth::W64,
            kind: X86BlsKind::Blsi,
            flags: FlagUpdate::None,
        },
    ] {
        assert!(
            !op.is_jit_safe(),
            "x86 BLS must remain outside the shared architecture whitelist"
        );
        assert!(x86_gate(op.clone()), "valid x86 BLS shape must JIT");
        assert!(
            !aarch64_gate(vec![op.clone()], false),
            "x86 BLS must not enter the AArch64 native gate"
        );
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(
            x86_native_scalar_feature_requirements_excluding(
                &builder.finish(),
                &std::collections::HashMap::new()
            ),
            (false, true, false, false, false),
            "native BLS encoding requires host BMI1"
        );
    }

    for malformed in [
        OpKind::X86Bls {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rbx),
            width: OpWidth::W16,
            kind: X86BlsKind::Blsmsk,
            flags: FlagUpdate::Specific(defined),
        },
        OpKind::X86Bls {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W16,
            kind: X86BlsKind::Blsr,
            flags: FlagUpdate::None,
        },
        OpKind::X86Bls {
            dst: x86(X86Reg::R31),
            src: VReg::Virtual(VirtualId(0)),
            width: OpWidth::W64,
            kind: X86BlsKind::Blsi,
            flags: FlagUpdate::None,
        },
        OpKind::X86Bls {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            kind: X86BlsKind::Blsr,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        },
    ] {
        assert!(!x86_gate(malformed), "malformed BLS shape must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Bls {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W64,
            kind: X86BlsKind::Blsr,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed BLS must fail closed"
    );
}
#[test]
fn x86_adx_gate_tracks_cpuid_shapes_architecture_and_suppressed_flag_liveness() {
    for (kind, output) in [
        (X86AdxKind::Adcx, FlagSet::CF),
        (X86AdxKind::Adox, FlagSet::OF),
    ] {
        let op = OpKind::X86Adx {
            dst: x86(X86Reg::R8),
            src1: x86(X86Reg::Rax),
            src2: x86(X86Reg::Rbx),
            width: OpWidth::W64,
            kind,
            flags: FlagUpdate::Specific(output),
        };
        assert!(!op.is_jit_safe(), "ADX remains scoped to the x86 gate");
        assert!(x86_gate(op.clone()), "valid ADX shape must JIT");
        assert!(!aarch64_gate(vec![op.clone()], false));

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();
        assert_eq!(
            x86_native_scalar_feature_requirements_excluding(
                &func,
                &std::collections::HashMap::new()
            ),
            (false, false, false, false, true)
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &func,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("adx")
        );

        let state_op = OpKind::X86Adx {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::Rsp),
            src2: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            kind,
            flags: FlagUpdate::Specific(output),
        };
        assert!(
            x86_gate(state_op.clone()),
            "valid state-backed ADX shape must JIT"
        );
        assert!(
            !aarch64_gate(vec![state_op], false),
            "state-backed x86 ADX must not enter the AArch64 native gate"
        );
    }

    let suppressed = OpKind::X86Adx {
        dst: x86(X86Reg::R8),
        src1: x86(X86Reg::Rax),
        src2: x86(X86Reg::Rbx),
        width: OpWidth::W32,
        kind: X86AdxKind::Adcx,
        flags: FlagUpdate::None,
    };
    assert!(
        !x86_gate(suppressed.clone()),
        "suppressed native CF output cannot escape a region"
    );
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, suppressed);
    builder.push_op(
        0x1001,
        OpKind::Xor {
            dst: x86(X86Reg::Rcx),
            src1: x86(X86Reg::Rcx),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(
        is_native_clobber_safe(&builder.finish()),
        "suppressed native CF output is safe when overwritten before observation"
    );

    let state_suppressed = OpKind::X86Adx {
        dst: x86(X86Reg::R31),
        src1: x86(X86Reg::Rsp),
        src2: x86(X86Reg::Rbp),
        width: OpWidth::W32,
        kind: X86AdxKind::Adcx,
        flags: FlagUpdate::None,
    };
    assert!(
        x86_gate(state_suppressed),
        "validated state-backed suppressed ADX preserves its native output flag exactly"
    );

    for malformed in [
        OpKind::X86Adx {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rcx),
            src2: x86(X86Reg::Rdx),
            width: OpWidth::W16,
            kind: X86AdxKind::Adcx,
            flags: FlagUpdate::Specific(FlagSet::CF),
        },
        OpKind::X86Adx {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::Rsp),
            src2: x86(X86Reg::Rbp),
            width: OpWidth::W16,
            kind: X86AdxKind::Adcx,
            flags: FlagUpdate::Specific(FlagSet::CF),
        },
        OpKind::X86Adx {
            dst: x86(X86Reg::R31),
            src1: VReg::Virtual(VirtualId(0)),
            src2: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            kind: X86AdxKind::Adcx,
            flags: FlagUpdate::Specific(FlagSet::CF),
        },
        OpKind::X86Adx {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rcx),
            src2: x86(X86Reg::Rdx),
            width: OpWidth::W64,
            kind: X86AdxKind::Adox,
            flags: FlagUpdate::Specific(FlagSet::CF),
        },
    ] {
        assert!(!x86_gate(malformed), "malformed ADX shape must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Adx {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::Rsp),
            src2: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            kind: X86AdxKind::Adox,
            flags: FlagUpdate::Specific(FlagSet::OF),
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed ADX must fail closed"
    );
}
#[test]
fn cwd_gate_accepts_only_implicit_architectural_registers_and_widths() {
    for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let op = OpKind::Cwd {
            dst: x86(X86Reg::Rdx),
            src: x86(X86Reg::Rax),
            width,
        };
        assert!(
            op.is_jit_safe(),
            "{width:?} must be on the scalar whitelist"
        );
        assert!(x86_gate(op), "{width:?} must pass the exact-shape gate");
    }

    for (name, op) in [
        (
            "byte width",
            OpKind::Cwd {
                dst: x86(X86Reg::Rdx),
                src: x86(X86Reg::Rax),
                width: OpWidth::W8,
            },
        ),
        (
            "wide width",
            OpKind::Cwd {
                dst: x86(X86Reg::Rdx),
                src: x86(X86Reg::Rax),
                width: OpWidth::W128,
            },
        ),
        (
            "wrong source",
            OpKind::Cwd {
                dst: x86(X86Reg::Rdx),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W64,
            },
        ),
        (
            "wrong destination",
            OpKind::Cwd {
                dst: x86(X86Reg::Rcx),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
            },
        ),
        (
            "virtual source",
            OpKind::Cwd {
                dst: x86(X86Reg::Rdx),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W32,
            },
        ),
        (
            "foreign destination",
            OpKind::Cwd {
                dst: arm_x(0),
                src: x86(X86Reg::Rax),
                width: OpWidth::W16,
            },
        ),
    ] {
        assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
        assert!(!x86_gate(op), "malformed {name} must deopt");
    }
}
#[test]
fn x86_bit_test_gate_accepts_exact_register_shapes_and_rejects_unsafe_ir() {
    for op in [
        OpKind::Bt {
            src: x86(X86Reg::R8),
            index: SrcOperand::Reg(x86(X86Reg::R9)),
            width: OpWidth::W16,
        },
        OpKind::Bts {
            dst: x86(X86Reg::R10),
            src: x86(X86Reg::R10),
            index: SrcOperand::Imm(31),
            width: OpWidth::W32,
        },
        OpKind::Btr {
            dst: x86(X86Reg::R14),
            src: x86(X86Reg::R14),
            index: SrcOperand::Imm64(63),
            width: OpWidth::W64,
        },
        OpKind::Btc {
            dst: x86(X86Reg::R15),
            src: x86(X86Reg::R15),
            index: SrcOperand::Reg(x86(X86Reg::Rax)),
            width: OpWidth::W64,
        },
        OpKind::Btr {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rsp),
            index: SrcOperand::Imm(63),
            width: OpWidth::W64,
        },
        OpKind::Bt {
            src: x86(X86Reg::R8),
            index: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W32,
        },
        OpKind::Btc {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R31),
            index: SrcOperand::Reg(x86(X86Reg::R16)),
            width: OpWidth::W64,
        },
    ] {
        assert!(op.is_jit_safe(), "register bit test must be whitelisted");
        assert!(x86_gate(op), "well-formed register bit test must JIT");
    }

    for (name, op) in [
        (
            "byte width",
            OpKind::Bt {
                src: x86(X86Reg::Rax),
                index: SrcOperand::Imm(0),
                width: OpWidth::W8,
            },
        ),
        (
            "non-destructive update",
            OpKind::Bts {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                index: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ),
        (
            "state-backed non-destructive update",
            OpKind::Btr {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                index: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ),
        (
            "virtual index",
            OpKind::Btc {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rax),
                index: SrcOperand::Reg(VReg::Virtual(VirtualId(0))),
                width: OpWidth::W64,
            },
        ),
    ] {
        assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
        assert!(!x86_gate(op), "malformed {name} bit test must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Bts {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R16),
            index: SrcOperand::Imm(7),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed bit test must fail closed"
    );
}
#[test]
fn x86_cldemote_gate_admits_only_the_ignorable_cache_hint() {
    for addr in [
        Address::Direct(x86(X86Reg::Rbx)),
        Address::BaseOffset {
            base: x86(X86Reg::Rsp),
            offset: -64,
            disp_size: DispSize::Disp8,
        },
        Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::Rbp)),
            index: Some(x86(X86Reg::R9)),
            scale: 4,
            disp: 32,
        },
    ] {
        let op = OpKind::X86CacheControl {
            addr,
            kind: X86CacheControlKind::Cldemote,
        };
        assert!(
            !op.is_jit_safe(),
            "cache hints remain outside the ALU whitelist"
        );
        assert!(x86_gate(op), "CLDEMOTE must be admitted without memory JIT");
    }

    let malformed = OpKind::X86CacheControl {
        addr: Address::Direct(VReg::Virtual(VirtualId(12))),
        kind: X86CacheControlKind::Cldemote,
    };
    assert!(!x86_gate(malformed));

    for kind in [
        X86CacheControlKind::Clflush,
        X86CacheControlKind::Clflushopt,
        X86CacheControlKind::Clwb,
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86CacheControl {
                addr: Address::Direct(x86(X86Reg::Rbx)),
                kind,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let function = builder.finish();
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true
        ));
    }
}
#[test]
fn x86_fence_gate_admits_exact_lfence_mfence_sfence_semantics() {
    for kind in [FenceKind::LoadLoad, FenceKind::Full, FenceKind::StoreStore] {
        let op = OpKind::Fence { kind };
        assert!(op.is_jit_safe(), "{kind:?} must be class-whitelisted");
        assert!(x86_gate(op), "{kind:?} must enter the native tier");
    }
    for kind in [
        FenceKind::LoadStore,
        FenceKind::StoreLoad,
        FenceKind::ISync,
        FenceKind::DSync,
    ] {
        let op = OpKind::Fence { kind };
        assert!(op.is_jit_safe(), "{kind:?} remains class-whitelisted");
        assert!(!x86_gate(op), "non-x86 fence {kind:?} must deopt");
    }
}
#[test]
fn clobber_gate_accepts_exact_bit_scan_shapes_and_rejects_malformed_ir() {
    let valid_flags = FlagUpdate::Specific(FlagSet::ZF);
    for op in [
        OpKind::Bsf {
            dst: x86(X86Reg::R8),
            src: x86(X86Reg::Rax),
            width: OpWidth::W16,
            flags: valid_flags,
        },
        OpKind::Bsr {
            dst: x86(X86Reg::R15),
            src: x86(X86Reg::R14),
            width: OpWidth::W64,
            flags: valid_flags,
        },
        OpKind::Bsf {
            dst: x86(X86Reg::R8),
            src: x86(X86Reg::Rax),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    ] {
        assert!(op.is_jit_safe(), "bit scan must be on the scalar whitelist");
        assert!(x86_gate(op), "well-formed bit scan must enter native JIT");
    }

    for (name, op) in [
        (
            "byte width",
            OpKind::Bsf {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W8,
                flags: valid_flags,
            },
        ),
        (
            "wrong flag contract",
            OpKind::Bsr {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "virtual source",
            OpKind::Bsf {
                dst: x86(X86Reg::Rax),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W64,
                flags: valid_flags,
            },
        ),
        (
            "foreign architecture source",
            OpKind::Bsr {
                dst: x86(X86Reg::Rax),
                src: arm_x(0),
                width: OpWidth::W32,
                flags: valid_flags,
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed {name} bit scan must deopt");
    }
}
#[test]
fn clobber_gate_admits_apx_ndd_imul_aliasing_second_source_with_or_without_nf() {
    let rax = x86(X86Reg::Rax);
    let rbx = x86(X86Reg::Rbx);
    for flags in [FlagUpdate::All, FlagUpdate::None] {
        assert!(
            x86_gate(OpKind::MulS {
                dst_lo: rbx,
                dst_hi: None,
                src1: rax,
                src2: SrcOperand::Reg(rbx),
                width: OpWidth::W64,
                flags,
            }),
            "alias-safe APX NDD IMUL {flags:?} must JIT"
        );
    }
}
#[test]
fn clobber_gate_admits_explicit_legacy_high_byte_movx_shapes() {
    for src in [X86Reg::Rsi, X86Reg::Rdi] {
        assert!(
            !x86_gate(OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(src),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            }),
            "W8 ZeroExtend from {src:?} can be legacy DH/BH and must deopt"
        );
        assert!(
            !x86_gate(OpKind::SignExtend {
                dst: x86(X86Reg::Rax),
                src: x86(src),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            }),
            "W8 SignExtend from {src:?} can be legacy DH/BH and must deopt"
        );
    }

    assert!(
        x86_gate(OpKind::ZeroExtend {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rdx),
            from_width: OpWidth::W8,
            to_width: OpWidth::W64,
        }),
        "unambiguous DL byte source stays native-eligible"
    );
    assert!(
        x86_gate(OpKind::ZeroExtend {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rsi),
            from_width: OpWidth::W16,
            to_width: OpWidth::W64,
        }),
        "word-sized RSI source is not a high-byte register ambiguity"
    );

    for op in [
        OpKind::ZeroExtend {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rdi),
            from_width: OpWidth::W8,
            to_width: OpWidth::W64,
        },
        OpKind::SignExtend {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rsi),
            from_width: OpWidth::W8,
            to_width: OpWidth::W64,
        },
    ] {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(0x1000, op);
        b.set_terminator(Terminator::Return { values: vec![] });
        let mut func = b.finish();
        func.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
        assert!(
            is_native_clobber_safe(&func),
            "REX-prefixed byte-register MOVX cannot be AH/CH/DH/BH and may JIT"
        );
    }

    for (src, op) in [
        (
            X86Reg::Rax,
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rax),
                from_width: OpWidth::W8,
                to_width: OpWidth::W32,
            },
        ),
        (
            X86Reg::Rcx,
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rdx),
                src: x86(X86Reg::Rcx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
        ),
        (
            X86Reg::Rdx,
            OpKind::SignExtend {
                dst: x86(X86Reg::Rsi),
                src: x86(X86Reg::Rdx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W32,
            },
        ),
        (
            X86Reg::Rbx,
            OpKind::SignExtend {
                dst: x86(X86Reg::Rdi),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
        ),
    ] {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(0x1000, op);
        b.set_terminator(Terminator::Return { values: vec![] });
        let mut func = b.finish();
        func.blocks[0].ops[0].x86_hint = Some(X86OpHint::LegacyHighByteReg);
        assert!(
            is_native_clobber_safe(&func),
            "explicit legacy high-byte parent {src:?} must JIT"
        );
    }

    for op in [
        OpKind::ZeroExtend {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rsi),
            from_width: OpWidth::W8,
            to_width: OpWidth::W32,
        },
        OpKind::SignExtend {
            dst: x86(X86Reg::R8),
            src: x86(X86Reg::Rbx),
            from_width: OpWidth::W8,
            to_width: OpWidth::W32,
        },
        OpKind::ZeroExtend {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rax),
            from_width: OpWidth::W16,
            to_width: OpWidth::W32,
        },
    ] {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(0x1000, op);
        b.set_terminator(Terminator::Return { values: vec![] });
        let mut func = b.finish();
        func.blocks[0].ops[0].x86_hint = Some(X86OpHint::LegacyHighByteReg);
        assert!(
            !is_native_clobber_safe(&func),
            "malformed legacy high-byte hint must deopt"
        );
    }
}
// Regression for issue #14: alias-safe ADC/SBB lowering removes the former
// deliberate deopt for APX NDD operations whose destination is source 2.
#[test]
fn clobber_gate_admits_adc_sbb_dst_aliasing_src2() {
    fn gate(op: OpKind) -> bool {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(0x1000, op);
        b.set_terminator(Terminator::Return { values: vec![] });
        is_native_clobber_safe(&b.finish())
    }

    for op in [
        OpKind::Adc {
            dst: x86(X86Reg::R8),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::R8)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        OpKind::Sbb {
            dst: x86(X86Reg::R8),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::R8)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    ] {
        assert!(
            gate(op),
            "alias-safe ADC/SBB with dst==src2 must remain native-eligible"
        );
    }

    // A non-aliased ADC (dst != src2) stays native-eligible.
    assert!(
        gate(OpKind::Adc {
            dst: x86(X86Reg::R8),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        }),
        "non-aliased ADC must stay native-eligible"
    );
}
