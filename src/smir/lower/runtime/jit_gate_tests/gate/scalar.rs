//! gate::scalar tests

use super::*;
use crate::smir::lower::runtime::*;
use crate::smir::lower::runtime::jit_gate_tests::*;

    #[test]
    fn x86_mulhrs_gate_validates_rounding_shape_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let mulhrs = |dst, src1, src2, lanes| OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem: VecElementType::I16,
            lanes,
            signed1: true,
            signed2: true,
            shift_left: 0,
            round: true,
            sat_bits: 0,
            out_shift: 15,
        };

        for (kind, hint, requirements) in [
            (
                mulhrs(xmm1, xmm1, xmm2, 8),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                },
                (true, false, false, false),
            ),
            (
                mulhrs(xmm1, xmm2, xmm3, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                mulhrs(ymm1, ymm2, ymm3, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                mulhrs(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    8,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, false, true),
            ),
            (
                mulhrs(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    32,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
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
            assert!(x86_vector_integer_mul_shift_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_mul_shift_feature_requirements(&smir_op),
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

        let valid = mulhrs(xmm1, xmm1, xmm2, 8);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        let configured = |src_elem, signed1, signed2, shift_left, round, sat_bits, out_shift| {
            OpKind::VMulShiftSat {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                src_elem,
                lanes: 8,
                signed1,
                signed2,
                shift_left,
                round,
                sat_bits,
                out_shift,
            }
        };
        for malformed_kind in [
            configured(VecElementType::I32, true, true, 0, true, 0, 15),
            configured(VecElementType::I16, false, true, 0, true, 0, 15),
            configured(VecElementType::I16, true, false, 0, true, 0, 15),
            configured(VecElementType::I16, true, true, 1, true, 0, 15),
            configured(VecElementType::I16, true, true, 0, false, 0, 15),
            configured(VecElementType::I16, true, true, 0, true, 16, 15),
            configured(VecElementType::I16, true, true, 0, true, 0, 14),
            mulhrs(xmm1, xmm1, xmm2, 7),
            mulhrs(xmm1, xmm1, ymm2, 8),
            mulhrs(VReg::Virtual(VirtualId(61)), xmm1, xmm2, 8),
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
                mulhrs(xmm1, xmm2, xmm3, 8),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x0B,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x0A,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(ymm1, ymm2, ymm3, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(ymm1, ymm2, ymm3, 16),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    16,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mulhrs(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    32,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0B,
                    width: VecWidth::V512,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }
    #[test]
    fn x86_mulhw_mulhuw_gate_validates_signedness_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let mul_high = |dst, src1, src2, lanes, signed| OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem: VecElementType::I16,
            lanes,
            signed1: signed,
            signed2: signed,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        };

        for (kind, hint, requirements) in [
            (
                mul_high(xmm1, xmm1, xmm2, 8, true),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                },
                (false, false, false, false),
            ),
            (
                mul_high(xmm1, xmm2, xmm3, 8, false),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false, false),
            ),
            (
                mul_high(ymm1, ymm2, ymm3, 16, true),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                    width: VecWidth::V256,
                    w: false,
                },
                (false, true, true, false),
            ),
            (
                mul_high(
                    x86(X86Reg::Xmm(16)),
                    x86(X86Reg::Xmm(17)),
                    x86(X86Reg::Xmm(18)),
                    8,
                    true,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, false, false, true),
            ),
            (
                mul_high(
                    x86(X86Reg::Zmm(16)),
                    x86(X86Reg::Zmm(17)),
                    x86(X86Reg::Zmm(18)),
                    32,
                    false,
                ),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE4,
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
            assert!(x86_vector_integer_mul_shift_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_mul_shift_feature_requirements(&smir_op),
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

        let valid = mul_high(xmm1, xmm1, xmm2, 8, true);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed in [
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mul_high(ymm1, ymm2, ymm3, 16, false),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE4,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                mul_high(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    16,
                    true,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0xE5,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }

        let mixed_signedness = OpKind::VMulShiftSat {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            src_elem: VecElementType::I16,
            lanes: 8,
            signed1: true,
            signed2: false,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        };
        assert!(!is_x86_native_vector_op(&mixed_signedness));
    }
    #[test]
    fn x86_psign_gate_validates_signed_control_shape_encodings_aliases_and_features() {
        let xmm1 = x86(X86Reg::Xmm(1));
        let xmm2 = x86(X86Reg::Xmm(2));
        let xmm3 = x86(X86Reg::Xmm(3));
        let ymm1 = x86(X86Reg::Ymm(1));
        let ymm2 = x86(X86Reg::Ymm(2));
        let ymm3 = x86(X86Reg::Ymm(3));
        let sign = |dst, src1, src2, elem, lanes| OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op: VLaneOp::Sign,
            signed: true,
            set_ovf: false,
        };

        for (kind, hint, requirements) in [
            (
                sign(xmm1, xmm1, xmm2, VecElementType::I8, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x08,
                },
                (true, false, false),
            ),
            (
                sign(xmm1, xmm2, xmm1, VecElementType::I16, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V128,
                    w: true,
                },
                (false, true, false),
            ),
            (
                sign(ymm1, ymm1, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0A,
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
            assert!(x86_vector_integer_sign_shape_valid(&kind), "{kind:?}");
            assert!(is_x86_native_vector_op(&kind), "{kind:?}");
            assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
            assert_eq!(
                x86_vector_integer_sign_feature_requirements(&smir_op),
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

        let valid = sign(xmm1, xmm1, xmm2, VecElementType::I8, 16);
        let unhinted = crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
        );
        assert!(is_x86_native_vector_op(&unhinted.kind));
        assert!(!x86_native_vector_smir_op(&unhinted));

        for malformed_kind in [
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::Sign,
                signed: false,
                set_ovf: false,
            },
            OpKind::VLane {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VLaneOp::Sign,
                signed: true,
                set_ovf: true,
            },
            sign(xmm1, xmm1, xmm2, VecElementType::I64, 2),
            sign(xmm1, xmm1, xmm2, VecElementType::I8, 15),
            sign(xmm1, xmm1, ymm3, VecElementType::I8, 16),
            sign(
                xmm1,
                VReg::Virtual(VirtualId(64)),
                xmm2,
                VecElementType::I8,
                16,
            ),
            sign(
                x86(X86Reg::Zmm(1)),
                x86(X86Reg::Zmm(2)),
                x86(X86Reg::Zmm(3)),
                VecElementType::I32,
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
                sign(xmm1, xmm2, xmm3, VecElementType::I8, 16),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x08,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid.clone(),
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0x08,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                valid,
                X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: 0x09,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0A,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::None,
                    opcode: 0x0A,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(ymm1, ymm2, ymm3, VecElementType::I32, 8),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x0A,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(
                    x86(X86Reg::Ymm(16)),
                    x86(X86Reg::Ymm(17)),
                    x86(X86Reg::Ymm(18)),
                    VecElementType::I16,
                    16,
                ),
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V256,
                    w: false,
                },
            ),
            crate::smir::ir::ops::SmirOp::with_hint(
                crate::smir::ir::types::OpId(0),
                0x1000,
                sign(xmm1, xmm2, xmm3, VecElementType::I16, 8),
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x09,
                    width: VecWidth::V128,
                    w: false,
                },
            ),
        ] {
            assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
        }
    }
    #[test]
    fn clobber_gate_accepts_valid_mulx_and_rejects_malformed_shapes() {
        let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
        b.push_op(
            0x1000,
            OpKind::MulU {
                dst_lo: x86(X86Reg::Rbx),
                dst_hi: Some(x86(X86Reg::Rcx)),
                src1: x86(X86Reg::Rdx),
                src2: SrcOperand::Reg(x86(X86Reg::Rax)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        b.set_terminator(Terminator::Return { values: vec![] });

        let mut func = b.finish();
        let op = &mut func.blocks[0].ops[0];
        assert!(op.kind.is_jit_safe(), "generic MulU stays whitelisted");
        op.x86_hint = Some(X86OpHint::Mulx);

        assert!(
            is_native_clobber_safe(&func),
            "well-formed MULX must enter its non-destructive BMI2 lowering"
        );

        let mut excluded = std::collections::HashMap::new();
        excluded.insert(func.entry, 0x1000);
        assert!(
            x86_native_scalar_features_supported_excluding(&func, &excluded),
            "an excluded MULX block has no host BMI2 requirement"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &func,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("bmi2")
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_scalar_features_supported_excluding(
            &func,
            &std::collections::HashMap::new()
        ));

        for (name, mutate) in [
            ("missing high destination", 0u8),
            ("wrong implicit source", 1),
            ("immediate source", 2),
            ("unsupported width", 3),
            ("flag-writing form", 4),
        ] {
            let mut malformed = func.clone();
            let OpKind::MulU {
                dst_hi,
                src1,
                src2,
                width,
                flags,
                ..
            } = &mut malformed.blocks[0].ops[0].kind
            else {
                unreachable!()
            };
            match mutate {
                0 => *dst_hi = None,
                1 => *src1 = x86(X86Reg::Rax),
                2 => *src2 = SrcOperand::Imm(7),
                3 => *width = OpWidth::W16,
                4 => *flags = FlagUpdate::All,
                _ => unreachable!(),
            }
            assert!(!is_native_clobber_safe(&malformed), "{name}");
        }
    }
    #[test]
    fn scalar_count_gate_tracks_features_and_rejects_malformed_shapes() {
        let valid = [
            OpKind::Popcnt {
                dst: x86(X86Reg::R8),
                src: x86(X86Reg::Rax),
                width: OpWidth::W16,
            },
            OpKind::Ctz {
                dst: x86(X86Reg::R9),
                src: x86(X86Reg::Rbx),
                width: OpWidth::W32,
            },
            OpKind::Clz {
                dst: x86(X86Reg::R15),
                src: x86(X86Reg::R14),
                width: OpWidth::W64,
            },
        ];
        for (op, expected) in valid.iter().cloned().zip([
            (false, false, false, true, false),
            (false, true, false, false, false),
            (false, false, true, false, false),
        ]) {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op);
            builder.set_terminator(Terminator::Return { values: vec![] });
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &builder.finish(),
                    &std::collections::HashMap::new()
                ),
                expected,
                "each count operation must request exactly its own host extension"
            );
        }
        for op in &valid {
            assert!(op.is_jit_safe(), "count op must be on the scalar whitelist");
            assert!(
                x86_gate(op.clone()),
                "well-formed count op must pass the clobber gate"
            );
        }

        let x86_valid = [
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R8),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W16,
                    kind: X86CountKind::Popcnt,
                    flags: FlagUpdate::All,
                },
                (false, false, false, true, false),
            ),
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R9),
                    src: x86(X86Reg::Rbx),
                    width: OpWidth::W32,
                    kind: X86CountKind::Tzcnt,
                    flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
                },
                (false, true, false, false, false),
            ),
            (
                OpKind::X86Count {
                    dst: x86(X86Reg::R15),
                    src: x86(X86Reg::R14),
                    width: OpWidth::W64,
                    kind: X86CountKind::Lzcnt,
                    flags: FlagUpdate::None,
                },
                (false, false, true, false, false),
            ),
        ];
        for (op, expected) in &x86_valid {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, op.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            assert_eq!(
                x86_native_scalar_feature_requirements_excluding(
                    &builder.finish(),
                    &std::collections::HashMap::new()
                ),
                *expected
            );
            assert!(op.is_jit_safe());
            assert!(x86_gate(op.clone()));
        }

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (index, op) in valid.into_iter().enumerate() {
            builder.push_op(0x1000 + index as u64, op);
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut excluded = std::collections::HashMap::new();
        excluded.insert(func.entry, 0x1000);
        assert!(
            x86_native_scalar_features_supported_excluding(&func, &excluded),
            "an excluded count block has no host feature requirement"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &func,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("popcnt")
                && std::is_x86_feature_detected!("bmi1")
                && std::is_x86_feature_detected!("lzcnt")
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_scalar_features_supported_excluding(
            &func,
            &std::collections::HashMap::new()
        ));

        for (name, op) in [
            (
                "byte width",
                OpKind::Popcnt {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W8,
                },
            ),
            (
                "guest stack source",
                OpKind::Ctz {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rsp),
                    width: OpWidth::W64,
                },
            ),
            (
                "guest frame destination",
                OpKind::Clz {
                    dst: x86(X86Reg::Rbp),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W64,
                },
            ),
            (
                "extended guest register",
                OpKind::Popcnt {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rax),
                    width: OpWidth::W32,
                },
            ),
            (
                "virtual source",
                OpKind::Ctz {
                    dst: x86(X86Reg::Rax),
                    src: VReg::Virtual(VirtualId(0)),
                    width: OpWidth::W64,
                },
            ),
            (
                "foreign architecture source",
                OpKind::Clz {
                    dst: x86(X86Reg::Rax),
                    src: arm_x(0),
                    width: OpWidth::W64,
                },
            ),
            (
                "TZCNT undefined flag request",
                OpKind::X86Count {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                    kind: X86CountKind::Tzcnt,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "LZCNT overflow flag request",
                OpKind::X86Count {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rcx),
                    width: OpWidth::W64,
                    kind: X86CountKind::Lzcnt,
                    flags: FlagUpdate::Specific(FlagSet::OF),
                },
            ),
        ] {
            assert!(!x86_gate(op), "malformed {name} count must deopt");
        }
    }
    #[test]
    fn carry_rotate_gate_admits_only_defined_immediate_one_forms() {
        let flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
        for (name, op) in [
            (
                "RCL byte",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W8,
                    flags,
                },
            ),
            (
                "RCR word",
                OpKind::Rcr {
                    dst: x86(X86Reg::Rcx),
                    src: x86(X86Reg::Rcx),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W16,
                    flags,
                },
            ),
            (
                "RCL dword NDD",
                OpKind::Rcl {
                    dst: x86(X86Reg::R8),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W32,
                    flags,
                },
            ),
            (
                "RCR qword NDD",
                OpKind::Rcr {
                    dst: x86(X86Reg::R15),
                    src: x86(X86Reg::R14),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags,
                },
            ),
            (
                "RCR qword APX EGPR destination",
                OpKind::Rcr {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} must be class-whitelisted");
            assert!(x86_gate(op), "{name} must enter native lowering");
        }

        for (name, op) in [
            (
                "multi-bit undefined OF",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(2),
                    width: OpWidth::W64,
                    flags,
                },
            ),
            (
                "variable count",
                OpKind::Rcr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                    width: OpWidth::W64,
                    flags,
                },
            ),
            (
                "suppressed flags",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "overbroad flags",
                OpKind::Rcr {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "wide operand",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W128,
                    flags,
                },
            ),
            (
                "virtual source",
                OpKind::Rcl {
                    dst: x86(X86Reg::Rax),
                    src: VReg::Virtual(VirtualId(0)),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W32,
                    flags,
                },
            ),
            (
                "foreign destination",
                OpKind::Rcr {
                    dst: arm_x(0),
                    src: x86(X86Reg::Rax),
                    amount: SrcOperand::Imm(1),
                    width: OpWidth::W16,
                    flags,
                },
            ),
        ] {
            assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
            assert!(!x86_gate(op), "malformed {name} must deopt");
        }
    }
    #[test]
    fn x86_unsigned_division_gate_accepts_exact_sources_and_fails_closed() {
        let rax = x86(X86Reg::Rax);
        let rdx = x86(X86Reg::Rdx);
        let div = |source, width, flags| OpKind::DivU {
            quot: rax,
            rem: (width != OpWidth::W8).then_some(rdx),
            src1: rax,
            src2: SrcOperand::Reg(source),
            width,
            flags,
        };

        for (name, source, width, flags) in [
            (
                "byte legacy",
                x86(X86Reg::Rbx),
                OpWidth::W8,
                FlagUpdate::All,
            ),
            (
                "word stack",
                x86(X86Reg::Rsp),
                OpWidth::W16,
                FlagUpdate::All,
            ),
            (
                "dword frame",
                x86(X86Reg::Rbp),
                OpWidth::W32,
                FlagUpdate::All,
            ),
            ("qword NF", x86(X86Reg::R15), OpWidth::W64, FlagUpdate::None),
            (
                "qword EGPR",
                x86(X86Reg::R16),
                OpWidth::W64,
                FlagUpdate::None,
            ),
        ] {
            assert!(x86_gate(div(source, width, flags)), "{name}");
        }

        for (name, malformed) in [
            (
                "wrong quotient",
                OpKind::DivU {
                    quot: x86(X86Reg::R8),
                    rem: Some(rdx),
                    src1: rax,
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "wrong remainder",
                OpKind::DivU {
                    quot: rax,
                    rem: Some(x86(X86Reg::R8)),
                    src1: rax,
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "byte RDX output",
                OpKind::DivU {
                    quot: rax,
                    rem: Some(rdx),
                    src1: rax,
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "wrong low dividend",
                OpKind::DivU {
                    quot: rax,
                    rem: Some(rdx),
                    src1: x86(X86Reg::R8),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "partial flags",
                div(
                    x86(X86Reg::Rbx),
                    OpWidth::W64,
                    FlagUpdate::Specific(FlagSet::CF),
                ),
            ),
        ] {
            assert!(!x86_gate(malformed), "{name}");
        }

        let temporary = VReg::Virtual(VirtualId(31));
        let mut memory = FunctionBuilder::new(FunctionId(0), 0x1000);
        memory.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(x86(X86Reg::Rbx)),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        memory.push_op(0x1000, div(temporary, OpWidth::W64, FlagUpdate::All));
        memory.set_terminator(Terminator::Return { values: vec![] });
        let memory = memory.finish();
        let memory_definitions = std::collections::HashMap::from([(temporary, 1)]);
        let memory_uses = std::collections::HashMap::from([(temporary, 1)]);
        assert_eq!(
            x86_jit_mem_unsigned_div_source_sequence_len(
                &memory.blocks[0],
                0,
                true,
                &memory_definitions,
                &memory_uses,
            ),
            Some(2),
        );
        assert_eq!(
            x86_jit_mem_unsigned_div_source_sequence_len(
                &memory.blocks[0],
                0,
                false,
                &memory_definitions,
                &memory_uses,
            ),
            None,
        );
        assert!(is_native_clobber_safe_excluding(
            &memory,
            &std::collections::HashMap::new(),
            true,
        ));
        assert!(!is_native_clobber_safe_excluding(
            &memory,
            &std::collections::HashMap::new(),
            false,
        ));

        let high_byte = VReg::Virtual(VirtualId(32));
        let mut high = FunctionBuilder::new(FunctionId(0), 0x1000);
        high.push_op(
            0x1000,
            OpKind::Shr {
                dst: high_byte,
                src: x86(X86Reg::Rcx),
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        high.push_op(0x1000, div(high_byte, OpWidth::W8, FlagUpdate::All));
        high.set_terminator(Terminator::Return { values: vec![] });
        let high = high.finish();
        let high_definitions = std::collections::HashMap::from([(high_byte, 1)]);
        let high_uses = std::collections::HashMap::from([(high_byte, 1)]);
        assert_eq!(
            x86_jit_high_byte_unsigned_div_source_sequence_len(
                &high.blocks[0],
                0,
                &high_definitions,
                &high_uses,
            ),
            Some(2),
        );
        assert!(is_native_clobber_safe(&high));
    }
    #[test]
    fn x86_signed_division_gate_accepts_exact_sources_and_fails_closed() {
        let rax = x86(X86Reg::Rax);
        let rdx = x86(X86Reg::Rdx);
        let div = |source, width, flags| OpKind::DivS {
            quot: rax,
            rem: (width != OpWidth::W8).then_some(rdx),
            src1: rax,
            src2: SrcOperand::Reg(source),
            width,
            flags,
        };

        for (name, source, width, flags) in [
            (
                "byte legacy",
                x86(X86Reg::Rbx),
                OpWidth::W8,
                FlagUpdate::All,
            ),
            (
                "word stack",
                x86(X86Reg::Rsp),
                OpWidth::W16,
                FlagUpdate::All,
            ),
            (
                "dword frame",
                x86(X86Reg::Rbp),
                OpWidth::W32,
                FlagUpdate::All,
            ),
            ("qword NF", x86(X86Reg::R15), OpWidth::W64, FlagUpdate::None),
            (
                "qword EGPR",
                x86(X86Reg::R16),
                OpWidth::W64,
                FlagUpdate::None,
            ),
        ] {
            assert!(x86_gate(div(source, width, flags)), "{name}");
        }

        for (name, malformed) in [
            (
                "wrong quotient",
                OpKind::DivS {
                    quot: x86(X86Reg::R8),
                    rem: Some(rdx),
                    src1: rax,
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "wrong remainder",
                OpKind::DivS {
                    quot: rax,
                    rem: Some(x86(X86Reg::R8)),
                    src1: rax,
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "byte RDX output",
                OpKind::DivS {
                    quot: rax,
                    rem: Some(rdx),
                    src1: rax,
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W8,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "wrong low dividend",
                OpKind::DivS {
                    quot: rax,
                    rem: Some(rdx),
                    src1: x86(X86Reg::R8),
                    src2: SrcOperand::Reg(x86(X86Reg::Rbx)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "partial flags",
                div(
                    x86(X86Reg::Rbx),
                    OpWidth::W64,
                    FlagUpdate::Specific(FlagSet::OF),
                ),
            ),
        ] {
            assert!(!x86_gate(malformed), "{name}");
        }

        let temporary = VReg::Virtual(VirtualId(33));
        let mut memory = FunctionBuilder::new(FunctionId(0), 0x1000);
        memory.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(x86(X86Reg::Rbx)),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        memory.push_op(0x1000, div(temporary, OpWidth::W64, FlagUpdate::All));
        memory.set_terminator(Terminator::Return { values: vec![] });
        let memory = memory.finish();
        let memory_definitions = std::collections::HashMap::from([(temporary, 1)]);
        let memory_uses = std::collections::HashMap::from([(temporary, 1)]);
        assert_eq!(
            x86_jit_mem_signed_div_source_sequence_len(
                &memory.blocks[0],
                0,
                true,
                &memory_definitions,
                &memory_uses,
            ),
            Some(2),
        );
        assert_eq!(
            x86_jit_mem_signed_div_source_sequence_len(
                &memory.blocks[0],
                0,
                false,
                &memory_definitions,
                &memory_uses,
            ),
            None,
        );
        assert!(is_native_clobber_safe_excluding(
            &memory,
            &std::collections::HashMap::new(),
            true,
        ));

        let high_byte = VReg::Virtual(VirtualId(34));
        let mut high = FunctionBuilder::new(FunctionId(0), 0x1000);
        high.push_op(
            0x1000,
            OpKind::Shr {
                dst: high_byte,
                src: x86(X86Reg::Rcx),
                amount: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        high.push_op(0x1000, div(high_byte, OpWidth::W8, FlagUpdate::All));
        high.set_terminator(Terminator::Return { values: vec![] });
        let high = high.finish();
        let high_definitions = std::collections::HashMap::from([(high_byte, 1)]);
        let high_uses = std::collections::HashMap::from([(high_byte, 1)]);
        assert_eq!(
            x86_jit_high_byte_signed_div_source_sequence_len(
                &high.blocks[0],
                0,
                &high_definitions,
                &high_uses,
            ),
            Some(2),
        );
        assert!(is_native_clobber_safe(&high));
    }
    #[test]
    fn clobber_gate_admits_flag_preserving_shifts_and_direct_cl_aliases() {
        let rax = x86(X86Reg::Rax);
        let rcx = x86(X86Reg::Rcx);
        for (name, op) in [
            (
                "shl",
                OpKind::Shl {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "shr",
                OpKind::Shr {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "sar",
                OpKind::Sar {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "rol",
                OpKind::Rol {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "ror",
                OpKind::Ror {
                    dst: rax,
                    src: rax,
                    amount: SrcOperand::Imm(4),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "CL-alias shl",
                OpKind::Shl {
                    dst: rcx,
                    src: rax,
                    amount: SrcOperand::Reg(rcx),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "NF CL-alias shl",
                OpKind::Shl {
                    dst: rcx,
                    src: rax,
                    amount: SrcOperand::Reg(rcx),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            assert!(x86_gate(op), "{name} must remain native-eligible");
        }
    }
    #[test]
    fn clobber_gate_admits_flag_preserving_binary_alu_including_ndd_aliases() {
        let rax = x86(X86Reg::Rax);
        let r8 = x86(X86Reg::R8);
        for (name, op) in [
            (
                "add",
                OpKind::Add {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "sub",
                OpKind::Sub {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "and",
                OpKind::And {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "or",
                OpKind::Or {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "xor",
                OpKind::Xor {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            assert!(
                x86_gate(op),
                "NF APX NDD {name} must remain native-eligible"
            );
        }
    }
    #[test]
    fn clobber_gate_allows_flag_updating_x86_alu() {
        assert!(x86_gate(OpKind::Add {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rax),
            src2: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        }));
    }
    #[test]
    fn clobber_gate_admits_apx_ndd_binary_alu_aliasing_second_source() {
        let rax = x86(X86Reg::Rax);
        let r8 = x86(X86Reg::R8);
        for (name, op) in [
            (
                "add",
                OpKind::Add {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "or",
                OpKind::Or {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "and",
                OpKind::And {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "sub",
                OpKind::Sub {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
            (
                "xor",
                OpKind::Xor {
                    dst: r8,
                    src1: rax,
                    src2: SrcOperand::Reg(r8),
                    width: OpWidth::W64,
                    flags: FlagUpdate::All,
                },
            ),
        ] {
            assert!(x86_gate(op), "alias-safe APX NDD {name} must JIT");
        }
    }
    #[test]
    fn clobber_gate_admits_only_exact_architectural_apx_ndd_double_shift_shapes() {
        let rax = x86(X86Reg::Rax);
        let rcx = x86(X86Reg::Rcx);
        let rbx = x86(X86Reg::Rbx);
        for op in [
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: rcx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Reg(rcx),
                width: OpWidth::W64,
                left: false,
                flags: FlagUpdate::None,
            },
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::R16),
                base: rax,
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(x86_gate(op), "valid APX NDD double shift must JIT");
        }

        for op in [
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: VReg::Virtual(VirtualId(21)),
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Reg(x86(X86Reg::Rdx)),
                width: OpWidth::W64,
                left: false,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W8,
                left: true,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: rbx,
                base: rax,
                fill: rbx,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ] {
            assert!(!x86_gate(op), "malformed APX NDD double shift must deopt");
        }
    }
