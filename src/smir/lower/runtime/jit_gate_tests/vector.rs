//! jit_gate_tests::vector tests

use super::*;
use crate::smir::ir::ops::X86SatFpFormat;
use crate::smir::lower::X86_GUEST_VECTOR_SCRATCH_OFFSET;
use crate::smir::lower::runtime::*;

#[test]
fn x86_sha512_feature_requirement_is_exact_to_the_three_native_ops() {
    assert!(x86_sha512_feature_required(&OpKind::X86Sha512Msg1 {
        dst: x86(X86Reg::Ymm(1)),
        src: x86(X86Reg::Xmm(2)),
    }));
    assert!(x86_sha512_feature_required(&OpKind::X86Sha512Msg2 {
        dst: x86(X86Reg::Ymm(1)),
        src: x86(X86Reg::Ymm(2)),
    }));
    assert!(x86_sha512_feature_required(&OpKind::X86Sha512Rounds2 {
        dst: x86(X86Reg::Ymm(1)),
        state: x86(X86Reg::Ymm(2)),
        wk: x86(X86Reg::Xmm(3)),
    }));
    assert!(!x86_sha512_feature_required(&OpKind::Nop));
}
#[test]
fn x86_sm3_feature_requirement_is_exact_to_the_three_native_ops() {
    assert!(x86_sm3_feature_required(&OpKind::X86Sm3Msg1 {
        dst: x86(X86Reg::Xmm(1)),
        src1: x86(X86Reg::Xmm(2)),
        src2: x86(X86Reg::Xmm(3)),
    }));
    assert!(x86_sm3_feature_required(&OpKind::X86Sm3Msg2 {
        dst: x86(X86Reg::Xmm(1)),
        src1: x86(X86Reg::Xmm(2)),
        src2: x86(X86Reg::Xmm(3)),
    }));
    assert!(x86_sm3_feature_required(&OpKind::X86Sm3Rounds2 {
        dst: x86(X86Reg::Xmm(1)),
        state: x86(X86Reg::Xmm(2)),
        words: x86(X86Reg::Xmm(3)),
        imm: 0x3E,
    }));
    assert!(!x86_sm3_feature_required(&OpKind::Nop));
}
#[test]
fn x86_sm4_feature_requirement_is_exact_to_the_native_op() {
    assert!(x86_sm4_feature_required(&OpKind::X86Sm4 {
        dst: x86(X86Reg::Ymm(1)),
        src1: x86(X86Reg::Ymm(2)),
        src2: x86(X86Reg::Ymm(3)),
        width: VecWidth::V256,
        key_schedule: false,
    }));
    assert!(!x86_sm4_feature_required(&OpKind::Nop));
}
#[test]
fn x86_vector_guest_state_layout_matches_trampoline_offsets() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, zmm),
        X86_GUEST_ZMM_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, k),
        X86_GUEST_K_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, vector_active),
        X86_GUEST_VECTOR_ACTIVE_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, mxcsr),
        X86_GUEST_MXCSR_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, host_mxcsr),
        X86_HOST_MXCSR_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, tsc_aux),
        X86_GUEST_TSC_AUX_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, pkru),
        X86_GUEST_PKRU_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, xcr0),
        X86_GUEST_XCR0_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, xgetbv1),
        X86_GUEST_XGETBV1_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cr4),
        X86_GUEST_CR4_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cr0),
        X86_GUEST_CR0_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpl),
        X86_GUEST_CPL_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, apx_enabled),
        X86_GUEST_APX_ENABLED_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, vec_load_fn),
        X86_GUEST_VEC_LOAD_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, vec_store_fn),
        X86_GUEST_VEC_STORE_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, pair_load_fn),
        X86_GUEST_PAIR_LOAD_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, pair_store_fn),
        X86_GUEST_PAIR_STORE_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, mm),
        X86_GUEST_MM_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, mmx_active),
        X86_GUEST_MMX_ACTIVE_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, x87_tag_word),
        X86_GUEST_X87_TAG_WORD_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpuid_fn),
        X86_GUEST_CPUID_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpuid_xeon_phi_avx512),
        X86_GUEST_CPUID_XEON_PHI_AVX512_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpuid_vp2intersect),
        X86_GUEST_CPUID_VP2INTERSECT_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpuid_sse4a),
        X86_GUEST_CPUID_SSE4A_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpuid_tbm),
        X86_GUEST_CPUID_TBM_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cpuid_xop),
        X86_GUEST_CPUID_XOP_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, kernel_gs_base),
        X86_GUEST_KERNEL_GS_BASE_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, tsc_fn),
        X86_GUEST_TSC_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, ac_flag),
        X86_GUEST_AC_FLAG_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cr2),
        X86_GUEST_CR2_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cr3),
        X86_GUEST_CR3_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, cr8),
        X86_GUEST_CR8_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, dr0),
        X86_GUEST_DR0_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, dr1),
        X86_GUEST_DR1_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, dr2),
        X86_GUEST_DR2_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, dr3),
        X86_GUEST_DR3_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, dr6),
        X86_GUEST_DR6_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, dr7),
        X86_GUEST_DR7_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, vector_scratch),
        X86_GUEST_VECTOR_SCRATCH_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, vector_scratch),
        std::mem::offset_of!(GuestRegs, mxcsr_state_active) + 8
    );
    assert_eq!(std::mem::align_of::<GuestRegs>(), 64);

    let mut regs = GuestRegs::default();
    let low = [0x0101_0101_0101_0101; 8];
    let high = [0x3131_3131_3131_3131; 8];
    regs.set_zmm(0, low);
    regs.set_zmm(31, high);
    assert_eq!(regs.get_zmm(0), low);
    assert_eq!(regs.get_zmm(31), high);
    assert_eq!(regs.mxcsr, 0x1F80);
    assert_eq!(regs.x87_tag_word, 0xFFFF);
    assert_eq!(regs.cpuid_fn, 0);
    assert_eq!(regs.cpuid_xeon_phi_avx512, 0);
    assert_eq!(regs.cpuid_vp2intersect, 0);
    assert_eq!(regs.cpuid_sse4a, 0);
    assert_eq!(regs.cpuid_tbm, 0);
    assert_eq!(regs.cpuid_xop, 0);
    assert_eq!(regs.kernel_gs_base, 0);
    assert_eq!(regs.tsc_fn, 0);
    assert_eq!(regs.vector_scratch, [0; 8]);
}
#[test]
fn recip28_native_gate_validates_packed_scalar_masks_and_encodings() {
    let packed = OpKind::X86Recip28 {
        dst: x86(X86Reg::Zmm(17)),
        merge: None,
        src: x86(X86Reg::Zmm(19)),
        mask: Some(x86(X86Reg::K(2))),
        elem: VecElementType::F32,
        width: VecWidth::V512,
        lanes: 16,
        scalar: false,
        mask_zeroing: true,
        suppress_exceptions: true,
    };
    let packed_op = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0xA000,
        packed.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0xCA,
            width: VecWidth::V512,
            w: false,
        },
    );
    assert!(is_x86_native_vector_op(&packed));
    assert!(x86_native_vector_smir_op(&packed_op));

    let scalar = OpKind::X86Recip28 {
        dst: x86(X86Reg::Xmm(17)),
        merge: Some(x86(X86Reg::Xmm(18))),
        src: x86(X86Reg::Xmm(19)),
        mask: Some(x86(X86Reg::K(2))),
        elem: VecElementType::F64,
        width: VecWidth::V128,
        lanes: 1,
        scalar: true,
        mask_zeroing: true,
        suppress_exceptions: true,
    };
    let scalar_op = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(1),
        0xA006,
        scalar.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0xCB,
            width: VecWidth::V128,
            w: true,
        },
    );
    assert!(is_x86_native_vector_op(&scalar));
    assert!(x86_native_vector_smir_op(&scalar_op));

    let mut wrong_hint = scalar_op;
    wrong_hint.x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0xCA,
        width: VecWidth::V128,
        w: true,
    });
    assert!(!x86_native_vector_smir_op(&wrong_hint));

    let mut virtual_merge = scalar.clone();
    let OpKind::X86Recip28 { merge, .. } = &mut virtual_merge else {
        unreachable!()
    };
    *merge = Some(VReg::virt(7));
    assert!(!is_x86_native_vector_op(&virtual_merge));

    let mut missing_merge = scalar;
    let OpKind::X86Recip28 { merge, .. } = &mut missing_merge else {
        unreachable!()
    };
    *merge = None;
    assert!(!is_x86_native_vector_op(&missing_merge));

    let mut short = packed;
    let OpKind::X86Recip28 {
        dst,
        src,
        width,
        lanes,
        ..
    } = &mut short
    else {
        unreachable!()
    };
    *dst = x86(X86Reg::Ymm(17));
    *src = x86(X86Reg::Ymm(19));
    *width = VecWidth::V256;
    *lanes = 8;
    assert!(!is_x86_native_vector_op(&short));
}
#[test]
fn rsqrt28_native_gate_validates_packed_scalar_masks_and_encodings() {
    let packed = OpKind::X86Rsqrt28 {
        dst: x86(X86Reg::Zmm(17)),
        merge: None,
        src: x86(X86Reg::Zmm(19)),
        mask: Some(x86(X86Reg::K(2))),
        elem: VecElementType::F32,
        width: VecWidth::V512,
        lanes: 16,
        scalar: false,
        mask_zeroing: true,
        suppress_exceptions: true,
    };
    let packed_op = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0xA000,
        packed.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0xCC,
            width: VecWidth::V512,
            w: false,
        },
    );
    assert!(is_x86_native_vector_op(&packed));
    assert!(x86_native_vector_smir_op(&packed_op));

    let scalar = OpKind::X86Rsqrt28 {
        dst: x86(X86Reg::Xmm(17)),
        merge: Some(x86(X86Reg::Xmm(18))),
        src: x86(X86Reg::Xmm(19)),
        mask: Some(x86(X86Reg::K(2))),
        elem: VecElementType::F64,
        width: VecWidth::V128,
        lanes: 1,
        scalar: true,
        mask_zeroing: true,
        suppress_exceptions: true,
    };
    let scalar_op = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(1),
        0xA006,
        scalar.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0xCD,
            width: VecWidth::V128,
            w: true,
        },
    );
    assert!(is_x86_native_vector_op(&scalar));
    assert!(x86_native_vector_smir_op(&scalar_op));

    let mut wrong_hint = scalar_op;
    wrong_hint.x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0xCC,
        width: VecWidth::V128,
        w: true,
    });
    assert!(!x86_native_vector_smir_op(&wrong_hint));

    let mut virtual_merge = scalar.clone();
    let OpKind::X86Rsqrt28 { merge, .. } = &mut virtual_merge else {
        unreachable!()
    };
    *merge = Some(VReg::virt(7));
    assert!(!is_x86_native_vector_op(&virtual_merge));

    let mut missing_merge = scalar;
    let OpKind::X86Rsqrt28 { merge, .. } = &mut missing_merge else {
        unreachable!()
    };
    *merge = None;
    assert!(!is_x86_native_vector_op(&missing_merge));

    let mut short = packed;
    let OpKind::X86Rsqrt28 {
        dst,
        src,
        width,
        lanes,
        ..
    } = &mut short
    else {
        unreachable!()
    };
    *dst = x86(X86Reg::Ymm(17));
    *src = x86(X86Reg::Ymm(19));
    *width = VecWidth::V256;
    *lanes = 8;
    assert!(!is_x86_native_vector_op(&short));
}
#[test]
fn packed_fp32_fp64_integer_conversion_native_gate_validates_shapes_and_encodings() {
    let int_to_fp = OpKind::X86PackedIntToFp {
        dst: x86(X86Reg::Ymm(17)),
        src: x86(X86Reg::Zmm(18)),
        mask: Some(x86(X86Reg::K(3))),
        int_elem: VecElementType::I64,
        fp_elem: VecElementType::F32,
        signed: true,
        lanes: 8,
        src_width: VecWidth::V512,
        dst_width: VecWidth::V256,
        mask_zeroing: true,
        zero_upper: true,
        round: crate::smir::ir::types::FpRoundMode::RoundDown,
        suppress_exceptions: true,
    };
    let int_to_fp_hint = X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::None,
        opcode: 0x5B,
        width: VecWidth::V512,
        w: true,
    };
    let canonical_int_to_fp = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        int_to_fp.clone(),
        int_to_fp_hint,
    );
    assert!(is_x86_native_vector_op(&int_to_fp));
    assert!(x86_native_vector_smir_op(&canonical_int_to_fp));

    let fp_to_int = OpKind::X86PackedFpToInt {
        dst: x86(X86Reg::Zmm(17)),
        src: x86(X86Reg::Zmm(18)),
        mask: Some(x86(X86Reg::K(3))),
        fp_elem: VecElementType::F64,
        int_elem: VecElementType::I64,
        signed: false,
        truncate: true,
        lanes: 8,
        src_width: VecWidth::V512,
        dst_width: VecWidth::V512,
        mask_zeroing: true,
        zero_upper: true,
        round: crate::smir::ir::types::FpRoundMode::RoundTowardZero,
        suppress_exceptions: true,
    };
    let fp_to_int_hint = X86OpHint::EvexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0x78,
        width: VecWidth::V512,
        w: true,
    };
    let canonical_fp_to_int = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        fp_to_int.clone(),
        fp_to_int_hint,
    );
    assert!(is_x86_native_vector_op(&fp_to_int));
    assert!(x86_native_vector_smir_op(&canonical_fp_to_int));

    let legacy = OpKind::X86PackedFpToInt {
        dst: x86(X86Reg::Xmm(1)),
        src: x86(X86Reg::Xmm(2)),
        mask: None,
        fp_elem: VecElementType::F32,
        int_elem: VecElementType::I32,
        signed: true,
        truncate: false,
        lanes: 4,
        src_width: VecWidth::V128,
        dst_width: VecWidth::V128,
        mask_zeroing: false,
        zero_upper: false,
        round: crate::smir::ir::types::FpRoundMode::Dynamic,
        suppress_exceptions: false,
    };
    let canonical_legacy =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, legacy.clone());
    assert!(is_x86_native_vector_op(&legacy));
    assert!(x86_native_vector_smir_op(&canonical_legacy));

    let wrong_hint = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        int_to_fp.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::Repne,
            opcode: 0x7A,
            width: VecWidth::V512,
            w: true,
        },
    );
    assert!(!x86_native_vector_smir_op(&wrong_hint));

    let mut virtual_src = int_to_fp.clone();
    let OpKind::X86PackedIntToFp { src, .. } = &mut virtual_src else {
        unreachable!()
    };
    *src = VReg::Virtual(VirtualId(0));
    let mut wrong_width = int_to_fp.clone();
    let OpKind::X86PackedIntToFp { dst_width, .. } = &mut wrong_width else {
        unreachable!()
    };
    *dst_width = VecWidth::V512;
    let mut unsupported_round = int_to_fp;
    let OpKind::X86PackedIntToFp { round, .. } = &mut unsupported_round else {
        unreachable!()
    };
    *round = crate::smir::ir::types::FpRoundMode::RoundNearestTiesAway;
    let mut malformed_truncate = fp_to_int.clone();
    let OpKind::X86PackedFpToInt { round, .. } = &mut malformed_truncate else {
        unreachable!()
    };
    *round = crate::smir::ir::types::FpRoundMode::RoundUp;
    let mut malformed_legacy = legacy.clone();
    let OpKind::X86PackedFpToInt { zero_upper, .. } = &mut malformed_legacy else {
        unreachable!()
    };
    *zero_upper = true;
    for malformed in [
        virtual_src,
        wrong_width,
        unsupported_round,
        malformed_truncate,
    ] {
        assert!(!is_x86_native_vector_op(&malformed), "{malformed:?}");
    }
    let malformed_legacy_op = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        malformed_legacy,
    );
    assert!(!x86_native_vector_smir_op(&malformed_legacy_op));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, fp_to_int);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(fp_to_int_hint);
    assert!(is_native_clobber_safe(&function));
}
#[test]
fn clobber_gate_admits_only_architectural_native_vector_operands() {
    let zmm1 = x86(X86Reg::Zmm(1));
    let zmm2 = x86(X86Reg::Zmm(2));
    let zmm3 = x86(X86Reg::Zmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let k4 = x86(X86Reg::K(4));
    let k5 = x86(X86Reg::K(5));
    let native_ops = [
        OpKind::VPopcnt {
            dst: zmm1,
            src: zmm2,
            mask: Some(k4),
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::VShuffleBitQM {
            dst: k5,
            src: zmm3,
            indices: zmm2,
            mask: Some(k4),
            width: VecWidth::V512,
        },
        OpKind::VConflict {
            dst: zmm1,
            src: zmm2,
            mask: Some(k4),
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::VLeadingZeros {
            dst: zmm1,
            src: zmm2,
            mask: Some(k4),
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: zmm2,
            table2: None,
            indices: zmm3,
            mask: Some(k4),
            elem: VecElementType::I8,
            width: VecWidth::V512,
            overwrite_table: false,
            zeroing: true,
        },
        OpKind::VCompress {
            dst: zmm1,
            src: zmm2,
            mask: Some(k4),
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::VExpand {
            dst: zmm1,
            src: zmm2,
            mask: Some(k4),
            elem: VecElementType::F64,
            width: VecWidth::V512,
            zeroing: false,
        },
        OpKind::X86NarrowInt {
            dst: ymm1,
            src: zmm2,
            mask: Some(k4),
            src_elem: VecElementType::I16,
            dst_elem: VecElementType::I8,
            width: VecWidth::V512,
            mode: crate::smir::ir::types::X86NarrowMode::Truncate,
            zeroing: true,
        },
        OpKind::X86Aes {
            dst: zmm1,
            src1: zmm2,
            src2: Some(zmm3),
            width: VecWidth::V512,
            op: X86AesOp::Enc,
            imm: 0,
        },
        OpKind::X86Aes {
            dst: xmm1,
            src1: xmm2,
            src2: None,
            width: VecWidth::V128,
            op: X86AesOp::KeygenAssist,
            imm: 0x5A,
        },
        OpKind::X86Sha512Msg1 {
            dst: ymm1,
            src: xmm2,
        },
        OpKind::X86Sha512Msg2 {
            dst: ymm1,
            src: ymm2,
        },
        OpKind::X86Sha512Rounds2 {
            dst: ymm1,
            state: ymm2,
            wk: xmm3,
        },
        OpKind::X86Sm3Msg1 {
            dst: xmm1,
            src1: xmm2,
            src2: xmm3,
        },
        OpKind::X86Sm3Msg2 {
            dst: xmm1,
            src1: xmm2,
            src2: xmm3,
        },
        OpKind::X86Sm3Rounds2 {
            dst: xmm1,
            state: xmm2,
            words: xmm3,
            imm: 0x3E,
        },
        OpKind::X86Sm4 {
            dst: ymm1,
            src1: ymm2,
            src2: ymm3,
            width: VecWidth::V256,
            key_schedule: false,
        },
        OpKind::X86PackedShiftImm {
            dst: zmm1,
            src: zmm2,
            width: VecWidth::V512,
            elem: VecElementType::I64,
            shift: ShiftOp::Asr,
            amount: 9,
            byte_lane: false,
        },
        OpKind::X86PackedShift {
            dst: zmm1,
            src: zmm2,
            count: xmm3,
            width: VecWidth::V512,
            elem: VecElementType::I64,
            shift: ShiftOp::Lsl,
        },
        OpKind::VCompress {
            dst: zmm1,
            src: zmm2,
            mask: Some(k4),
            elem: VecElementType::I8,
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::VExpand {
            dst: zmm1,
            src: zmm2,
            mask: Some(k4),
            elem: VecElementType::I16,
            width: VecWidth::V512,
            zeroing: false,
        },
        OpKind::VDotProduct {
            dst: zmm1,
            acc: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V512,
            src1_unsigned: true,
            saturate: false,
            zeroing: true,
        },
        OpKind::VMultiplyAdd52 {
            dst: zmm1,
            acc: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            width: VecWidth::V512,
            high: false,
            zeroing: true,
        },
        OpKind::VDotProductBF16 {
            dst: zmm1,
            acc: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::VCvtFP32ToBF16 {
            dst: ymm1,
            src1: zmm2,
            src2: None,
            mask: Some(k4),
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::VFP16Arith {
            dst: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            op: crate::smir::ir::types::Avx10FP16Op::Add,
            round: crate::smir::ir::types::FpRoundMode::Dynamic,
            width: VecWidth::V512,
            lanes: 32,
            zeroing: true,
        },
        OpKind::VFP16Arith {
            dst: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            op: crate::smir::ir::types::Avx10FP16Op::Min,
            round: crate::smir::ir::types::FpRoundMode::Dynamic,
            width: VecWidth::V512,
            lanes: 32,
            zeroing: true,
        },
        OpKind::VFP16Arith {
            dst: zmm1,
            src1: zmm2,
            src2: zmm3,
            mask: Some(k4),
            op: crate::smir::ir::types::Avx10FP16Op::Max,
            round: crate::smir::ir::types::FpRoundMode::Dynamic,
            width: VecWidth::V512,
            lanes: 32,
            zeroing: false,
        },
        OpKind::X86PackedShiftVariable {
            dst: zmm1,
            src: zmm2,
            count: zmm3,
            mask: Some(k4),
            width: VecWidth::V512,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsl,
            zeroing: true,
        },
        OpKind::X86PackedRotate {
            dst: zmm1,
            src: zmm2,
            count: None,
            mask: Some(k4),
            amount: 7,
            width: VecWidth::V512,
            elem: VecElementType::I32,
            left: true,
            zeroing: true,
        },
        OpKind::X86PackedRotate {
            dst: zmm1,
            src: zmm2,
            count: Some(zmm3),
            mask: Some(k4),
            amount: 0,
            width: VecWidth::V512,
            elem: VecElementType::I64,
            left: false,
            zeroing: false,
        },
        OpKind::X86TernaryLogic {
            dst: zmm1,
            src1: zmm1,
            src2: zmm2,
            src3: zmm3,
            mask: Some(k4),
            imm: 0x96,
            width: VecWidth::V512,
            elem: VecElementType::I32,
            zeroing: true,
        },
        OpKind::X86PackedFunnelShift {
            dst: zmm1,
            src: zmm1,
            fill: zmm2,
            count: Some(zmm3),
            mask: Some(k4),
            amount: 0,
            width: VecWidth::V512,
            elem: VecElementType::I32,
            left: true,
            zeroing: true,
        },
        OpKind::X86MultiShiftQB {
            dst: zmm1,
            control: zmm2,
            source: zmm3,
            mask: Some(k4),
            width: VecWidth::V512,
            zeroing: true,
        },
    ];
    for native in &native_ops {
        assert!(is_x86_native_vector_op(native), "{native:?}");
        assert!(x86_gate(native.clone()), "{native:?}");
    }

    let embedded_rounding = OpKind::VFP16Arith {
        dst: zmm1,
        src1: zmm2,
        src2: zmm3,
        mask: None,
        op: crate::smir::ir::types::Avx10FP16Op::Add,
        round: crate::smir::ir::types::FpRoundMode::RoundNearest,
        width: VecWidth::V512,
        lanes: 32,
        zeroing: false,
    };
    assert!(!is_x86_native_vector_op(&embedded_rounding));
    assert!(!x86_gate(embedded_rounding));

    let partial_lanes = OpKind::VFP16Arith {
        dst: x86(X86Reg::Xmm(1)),
        src1: x86(X86Reg::Xmm(2)),
        src2: x86(X86Reg::Xmm(3)),
        mask: None,
        op: crate::smir::ir::types::Avx10FP16Op::Div,
        round: crate::smir::ir::types::FpRoundMode::Dynamic,
        width: VecWidth::V128,
        lanes: 1,
        zeroing: false,
    };
    assert!(!is_x86_native_vector_op(&partial_lanes));
    assert!(!x86_gate(partial_lanes));

    // The byte encoder knows the AVX10.2 MAP5 form, but runtime admission
    // remains fail-closed until an AVX10.2 host feature probe and MXCSR replay
    // contract are available.
    let scalar_saturating_conversion = OpKind::X86ScalarFpToIntSat {
        dst: x86(X86Reg::Rax),
        src: xmm2,
        elem: VecElementType::F64,
        int_width: OpWidth::W64,
        signed: true,
        suppress_exceptions: true,
    };
    assert!(!is_x86_native_vector_op(&scalar_saturating_conversion));
    assert!(!scalar_saturating_conversion.is_jit_safe());
    assert!(!x86_gate(scalar_saturating_conversion));

    let saturating_conversion = OpKind::VCvtFpToIntSat {
        dst: zmm1,
        src: zmm2,
        mask: Some(k4),
        fp_elem: X86SatFpFormat::F32,
        int_elem: VecElementType::I8,
        width: VecWidth::V512,
        signed: true,
        truncate: true,
        round: crate::smir::ir::types::FpRoundMode::RoundTowardZero,
        zeroing: true,
        suppress_exceptions: false,
    };
    assert!(!is_x86_native_vector_op(&saturating_conversion));
    assert!(!x86_gate(saturating_conversion));

    let rounded_saturating_conversion = OpKind::VCvtFpToIntSat {
        dst: zmm1,
        src: zmm2,
        mask: Some(k4),
        fp_elem: X86SatFpFormat::F32,
        int_elem: VecElementType::I8,
        width: VecWidth::V512,
        signed: false,
        truncate: false,
        round: crate::smir::ir::types::FpRoundMode::RoundUp,
        zeroing: true,
        suppress_exceptions: true,
    };
    assert!(!is_x86_native_vector_op(&rounded_saturating_conversion));
    assert!(!x86_gate(rounded_saturating_conversion));

    let widening_saturating_conversion = OpKind::VCvtFpToIntSat {
        dst: zmm1,
        src: x86(X86Reg::Ymm(2)),
        mask: None,
        fp_elem: X86SatFpFormat::F32,
        int_elem: VecElementType::I64,
        width: VecWidth::V512,
        signed: true,
        truncate: true,
        round: crate::smir::ir::types::FpRoundMode::RoundTowardZero,
        zeroing: false,
        suppress_exceptions: false,
    };
    assert!(!is_x86_native_vector_op(&widening_saturating_conversion));
    assert!(!x86_gate(widening_saturating_conversion));

    let bf16_saturating_conversion = OpKind::VCvtFpToIntSat {
        dst: zmm1,
        src: zmm2,
        mask: None,
        fp_elem: X86SatFpFormat::BF16,
        int_elem: VecElementType::I8,
        width: VecWidth::V512,
        signed: true,
        truncate: false,
        round: crate::smir::ir::types::FpRoundMode::RoundNearest,
        zeroing: false,
        suppress_exceptions: false,
    };
    assert!(!is_x86_native_vector_op(&bf16_saturating_conversion));
    assert!(!x86_gate(bf16_saturating_conversion));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, native_ops[0].clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();
    assert!(uses_x86_native_vectors_excluding(
        &func,
        &std::collections::HashMap::new()
    ));

    for (move_kind, hint) in [
        (
            OpKind::VMov {
                dst: xmm1,
                src: xmm2,
                width: VecWidth::V128,
            },
            X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x28,
            },
        ),
        (
            OpKind::VMov {
                dst: ymm1,
                src: ymm2,
                width: VecWidth::V256,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x28,
                width: VecWidth::V256,
                w: false,
            },
        ),
        (
            OpKind::VMov {
                dst: zmm1,
                src: zmm2,
                width: VecWidth::V512,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x28,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(is_x86_native_vector_op(&move_kind));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, move_kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(x86_native_vector_smir_op(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
    }

    let unhinted_move = OpKind::VMov {
        dst: xmm1,
        src: xmm2,
        width: VecWidth::V128,
    };
    assert!(is_x86_native_vector_op(&unhinted_move));
    assert!(!x86_gate(unhinted_move));

    let virtual_source = OpKind::X86PackedShiftVariable {
        dst: zmm1,
        src: VReg::Virtual(VirtualId(7)),
        count: zmm2,
        mask: None,
        width: VecWidth::V512,
        elem: VecElementType::I32,
        shift: ShiftOp::Lsl,
        zeroing: false,
    };
    assert!(!is_x86_native_vector_op(&virtual_source));
    assert!(!x86_gate(virtual_source));

    for malformed_move in [
        OpKind::VMov {
            dst: xmm1,
            src: ymm2,
            width: VecWidth::V128,
        },
        OpKind::VMov {
            dst: ymm1,
            src: ymm2,
            width: VecWidth::V128,
        },
        OpKind::VMov {
            dst: zmm1,
            src: VReg::Virtual(VirtualId(9)),
            width: VecWidth::V512,
        },
        OpKind::VMov {
            dst: x86(X86Reg::Xmm(32)),
            src: xmm2,
            width: VecWidth::V128,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed_move));
        assert!(!x86_gate(malformed_move));
    }

    let move_kind = OpKind::VMov {
        dst: xmm1,
        src: xmm2,
        width: VecWidth::V128,
    };
    let mut malformed_hint = FunctionBuilder::new(FunctionId(0), 0x1000);
    malformed_hint.push_op(0x1000, move_kind);
    malformed_hint.set_terminator(Terminator::Return { values: vec![] });
    let mut malformed_hint = malformed_hint.finish();
    malformed_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F38,
        pp: X86SsePrefix::OpSize,
        opcode: 0x40,
        width: VecWidth::V128,
        w: false,
    });
    assert!(!x86_native_vector_smir_op(&malformed_hint.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&malformed_hint));

    let low_move = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VMov {
            dst: xmm1,
            src: xmm2,
            width: VecWidth::V128,
        },
        X86OpHint::SseMov {
            prefix: X86SsePrefix::None,
            opcode: 0x28,
        },
    );
    assert!(!x86_vector_move_needs_vl(&low_move));
    let evex_move = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        low_move.kind.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x28,
            width: VecWidth::V128,
            w: false,
        },
    );
    assert!(x86_native_vector_smir_op(&evex_move));
    assert!(x86_vector_move_needs_vl(&evex_move));
    let high_move = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VMov {
            dst: x86(X86Reg::Ymm(16)),
            src: x86(X86Reg::Ymm(31)),
            width: VecWidth::V256,
        },
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x28,
            width: VecWidth::V256,
            w: false,
        },
    );
    assert!(x86_native_vector_smir_op(&high_move));
    assert!(x86_vector_move_needs_vl(&high_move));

    for (logic_kind, hint, feature_requirements) in [
        (
            OpKind::VAnd {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                width: VecWidth::V128,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x54,
            },
            (false, false, false, false),
        ),
        (
            OpKind::VAndNot {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                width: VecWidth::V128,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x55,
                width: VecWidth::V128,
                w: false,
            },
            (true, false, false, false),
        ),
        (
            OpKind::VOr {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                width: VecWidth::V256,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xEB,
                width: VecWidth::V256,
                w: false,
            },
            (true, true, false, false),
        ),
        (
            OpKind::VXor {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                width: VecWidth::V512,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x57,
                width: VecWidth::V512,
                w: true,
            },
            (false, false, true, false),
        ),
        (
            OpKind::VAnd {
                dst: x86(X86Reg::Ymm(16)),
                src1: x86(X86Reg::Ymm(17)),
                src2: x86(X86Reg::Ymm(18)),
                width: VecWidth::V256,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xDB,
                width: VecWidth::V256,
                w: false,
            },
            (false, false, false, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            logic_kind.clone(),
            hint,
        );
        assert!(is_x86_native_vector_op(&logic_kind), "{logic_kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_logic_feature_requirements(&smir_op),
            feature_requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, logic_kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let unhinted_logic = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VXor {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            width: VecWidth::V128,
        },
    );
    assert!(is_x86_native_vector_op(&unhinted_logic.kind));
    assert!(!x86_native_vector_smir_op(&unhinted_logic));

    for malformed_logic_kind in [
        OpKind::VOr {
            dst: xmm1,
            src1: xmm2,
            src2: VReg::Virtual(VirtualId(10)),
            width: VecWidth::V128,
        },
        OpKind::VXor {
            dst: xmm1,
            src1: xmm2,
            src2: ymm3,
            width: VecWidth::V128,
        },
        OpKind::VAndNot {
            dst: x86(X86Reg::Xmm(32)),
            src1: xmm2,
            src2: xmm3,
            width: VecWidth::V128,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed_logic_kind));
    }

    for malformed_logic in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VAnd {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                width: VecWidth::V128,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x54,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VAnd {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                width: VecWidth::V128,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0x57,
                width: VecWidth::V128,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VXor {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                width: VecWidth::V512,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x57,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed_logic));
    }

    for (arithmetic_kind, hint, feature_requirements) in [
        (
            OpKind::VAdd {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xFC,
            },
            (false, false, false),
        ),
        (
            OpKind::VAdd {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                elem: VecElementType::I32,
                lanes: 4,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xFE,
                width: VecWidth::V128,
                w: false,
            },
            (true, false, false),
        ),
        (
            OpKind::VSub {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I16,
                lanes: 16,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF9,
                width: VecWidth::V256,
                w: false,
            },
            (true, true, false),
        ),
        (
            OpKind::VAdd {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I64,
                lanes: 8,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD4,
                width: VecWidth::V512,
                w: true,
            },
            (false, false, false),
        ),
        (
            OpKind::VSub {
                dst: x86(X86Reg::Ymm(16)),
                src1: x86(X86Reg::Ymm(17)),
                src2: x86(X86Reg::Ymm(18)),
                elem: VecElementType::I32,
                lanes: 8,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xFA,
                width: VecWidth::V256,
                w: false,
            },
            (false, false, true),
        ),
        (
            OpKind::VAddSubSat {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                subtract: false,
                signed: true,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xEC,
            },
            (false, false, false),
        ),
        (
            OpKind::VAddSubSat {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I16,
                lanes: 16,
                subtract: true,
                signed: false,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD9,
                width: VecWidth::V256,
                w: true,
            },
            (true, true, false),
        ),
        (
            OpKind::VAddSubSat {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I8,
                lanes: 64,
                subtract: false,
                signed: false,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xDC,
                width: VecWidth::V512,
                w: true,
            },
            (false, false, false),
        ),
        (
            OpKind::VAddSubSat {
                dst: x86(X86Reg::Ymm(16)),
                src1: x86(X86Reg::Ymm(17)),
                src2: x86(X86Reg::Ymm(18)),
                elem: VecElementType::I16,
                lanes: 16,
                subtract: true,
                signed: true,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xE9,
                width: VecWidth::V256,
                w: true,
            },
            (false, false, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            arithmetic_kind.clone(),
            hint,
        );
        assert!(
            is_x86_native_vector_op(&arithmetic_kind),
            "{arithmetic_kind:?}"
        );
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_arithmetic_feature_requirements(&smir_op),
            feature_requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, arithmetic_kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let unhinted_arithmetic = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VSub {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I64,
            lanes: 2,
        },
    );
    assert!(is_x86_native_vector_op(&unhinted_arithmetic.kind));
    assert!(!x86_native_vector_smir_op(&unhinted_arithmetic));

    let unhinted_saturating = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VAddSubSat {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I8,
            lanes: 16,
            subtract: false,
            signed: true,
        },
    );
    assert!(is_x86_native_vector_op(&unhinted_saturating.kind));
    assert!(!x86_native_vector_smir_op(&unhinted_saturating));

    for malformed_arithmetic_kind in [
        OpKind::VAdd {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VSub {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I16,
            lanes: 7,
        },
        OpKind::VAdd {
            dst: xmm1,
            src1: xmm2,
            src2: ymm3,
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VAddSubSat {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I32,
            lanes: 4,
            subtract: false,
            signed: true,
        },
        OpKind::VAddSubSat {
            dst: ymm1,
            src1: ymm2,
            src2: ymm3,
            elem: VecElementType::I8,
            lanes: 31,
            subtract: true,
            signed: false,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed_arithmetic_kind));
    }

    for malformed_arithmetic in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VAdd {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                elem: VecElementType::I8,
                lanes: 16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xFC,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VSub {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I16,
                lanes: 16,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xF8,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VAdd {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I64,
                lanes: 8,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD4,
                width: VecWidth::V512,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VAddSubSat {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                subtract: false,
                signed: false,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xEC,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VAddSubSat {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I16,
                lanes: 32,
                subtract: true,
                signed: true,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::Rep,
                opcode: 0xE9,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed_arithmetic));
    }

    for (multiply_kind, hint, feature_requirements) in [
        (
            OpKind::VMul {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I16,
                lanes: 8,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xD5,
            },
            (false, false, false, false, false),
        ),
        (
            OpKind::VMul {
                dst: xmm1,
                src1: xmm1,
                src2: xmm2,
                elem: VecElementType::I32,
                lanes: 4,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x40,
            },
            (true, false, false, false, false),
        ),
        (
            OpKind::VMul {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                elem: VecElementType::I16,
                lanes: 8,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD5,
                width: VecWidth::V128,
                w: true,
            },
            (false, true, false, false, false),
        ),
        (
            OpKind::VMul {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I32,
                lanes: 8,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V256,
                w: true,
            },
            (false, true, true, false, false),
        ),
        (
            OpKind::VMul {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I32,
                lanes: 16,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V512,
                w: false,
            },
            (false, false, false, false, false),
        ),
        (
            OpKind::VMul {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I64,
                lanes: 8,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V512,
                w: true,
            },
            (false, false, false, true, false),
        ),
        (
            OpKind::VMul {
                dst: x86(X86Reg::Ymm(16)),
                src1: x86(X86Reg::Ymm(17)),
                src2: x86(X86Reg::Ymm(18)),
                elem: VecElementType::I64,
                lanes: 4,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V256,
                w: true,
            },
            (false, false, false, true, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            multiply_kind.clone(),
            hint,
        );
        assert!(is_x86_native_vector_op(&multiply_kind), "{multiply_kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_multiply_feature_requirements(&smir_op),
            feature_requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, multiply_kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let unhinted_multiply = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VMul {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I16,
            lanes: 8,
        },
    );
    assert!(is_x86_native_vector_op(&unhinted_multiply.kind));
    assert!(!x86_native_vector_smir_op(&unhinted_multiply));

    for malformed_multiply_kind in [
        OpKind::VMul {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::I8,
            lanes: 16,
        },
        OpKind::VMul {
            dst: xmm1,
            src1: xmm1,
            src2: xmm2,
            elem: VecElementType::F32,
            lanes: 4,
        },
        OpKind::VMul {
            dst: ymm1,
            src1: ymm2,
            src2: ymm3,
            elem: VecElementType::I32,
            lanes: 7,
        },
        OpKind::VMul {
            dst: xmm1,
            src1: xmm2,
            src2: ymm3,
            elem: VecElementType::I32,
            lanes: 4,
        },
        OpKind::VMul {
            dst: xmm1,
            src1: xmm1,
            src2: VReg::Virtual(VirtualId(11)),
            elem: VecElementType::I16,
            lanes: 8,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed_multiply_kind));
    }

    for malformed_multiply in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMul {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                elem: VecElementType::I16,
                lanes: 8,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xD5,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMul {
                dst: xmm1,
                src1: xmm2,
                src2: xmm3,
                elem: VecElementType::I16,
                lanes: 8,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V128,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMul {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I32,
                lanes: 8,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMul {
                dst: ymm1,
                src1: ymm2,
                src2: ymm3,
                elem: VecElementType::I64,
                lanes: 4,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V256,
                w: true,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMul {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I32,
                lanes: 16,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V512,
                w: true,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VMul {
                dst: zmm1,
                src1: zmm2,
                src2: zmm3,
                elem: VecElementType::I64,
                lanes: 8,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed_multiply));
    }

    for (abs_kind, hint, feature_requirements) in [
        (
            OpKind::VUnary {
                dst: xmm1,
                src: xmm2,
                elem: VecElementType::I8,
                lanes: 16,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x1C,
            },
            (true, false, false, false),
        ),
        (
            OpKind::VUnary {
                dst: ymm1,
                src: ymm2,
                elem: VecElementType::I16,
                lanes: 16,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x1D,
                width: VecWidth::V256,
                w: true,
            },
            (false, true, true, false),
        ),
        (
            OpKind::VUnary {
                dst: x86(X86Reg::Zmm(16)),
                src: x86(X86Reg::Zmm(17)),
                elem: VecElementType::I32,
                lanes: 16,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x1E,
                width: VecWidth::V512,
                w: false,
            },
            (false, false, false, false),
        ),
        (
            OpKind::VUnary {
                dst: x86(X86Reg::Ymm(16)),
                src: x86(X86Reg::Ymm(17)),
                elem: VecElementType::I64,
                lanes: 4,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x1F,
                width: VecWidth::V256,
                w: true,
            },
            (false, false, false, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            abs_kind.clone(),
            hint,
        );
        assert!(is_x86_native_vector_op(&abs_kind), "{abs_kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_abs_feature_requirements(&smir_op),
            feature_requirements,
            "{smir_op:?}"
        );

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, abs_kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    let unhinted_abs = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        OpKind::VUnary {
            dst: xmm1,
            src: xmm2,
            elem: VecElementType::I32,
            lanes: 4,
            op: VecUnaryOp::Abs,
        },
    );
    assert!(is_x86_native_vector_op(&unhinted_abs.kind));
    assert!(!x86_native_vector_smir_op(&unhinted_abs));

    for malformed_abs_kind in [
        OpKind::VUnary {
            dst: xmm1,
            src: xmm2,
            elem: VecElementType::I32,
            lanes: 4,
            op: VecUnaryOp::Neg,
        },
        OpKind::VUnary {
            dst: xmm1,
            src: xmm2,
            elem: VecElementType::F32,
            lanes: 4,
            op: VecUnaryOp::Abs,
        },
        OpKind::VUnary {
            dst: ymm1,
            src: xmm2,
            elem: VecElementType::I16,
            lanes: 16,
            op: VecUnaryOp::Abs,
        },
        OpKind::VUnary {
            dst: xmm1,
            src: VReg::Virtual(VirtualId(12)),
            elem: VecElementType::I8,
            lanes: 16,
            op: VecUnaryOp::Abs,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed_abs_kind));
    }

    for malformed_abs in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VUnary {
                dst: xmm1,
                src: xmm2,
                elem: VecElementType::I64,
                lanes: 2,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x1F,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VUnary {
                dst: ymm1,
                src: ymm2,
                elem: VecElementType::I16,
                lanes: 16,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x1D,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VUnary {
                dst: zmm1,
                src: zmm2,
                elem: VecElementType::I32,
                lanes: 16,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x1E,
                width: VecWidth::V512,
                w: true,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            OpKind::VUnary {
                dst: zmm1,
                src: zmm2,
                elem: VecElementType::I64,
                lanes: 8,
                op: VecUnaryOp::Abs,
            },
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x1F,
                width: VecWidth::V512,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed_abs));
    }

    for invalid_vplzcnt in [
        OpKind::VLeadingZeros {
            dst: zmm1,
            src: VReg::Virtual(VirtualId(8)),
            mask: None,
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: false,
        },
        OpKind::VLeadingZeros {
            dst: zmm1,
            src: zmm2,
            mask: None,
            elem: VecElementType::I16,
            width: VecWidth::V512,
            zeroing: false,
        },
        OpKind::VLeadingZeros {
            dst: zmm1,
            src: zmm2,
            mask: Some(x86(X86Reg::K(0))),
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: false,
        },
        OpKind::VLeadingZeros {
            dst: zmm1,
            src: zmm2,
            mask: None,
            elem: VecElementType::I32,
            width: VecWidth::V512,
            zeroing: true,
        },
        OpKind::VLeadingZeros {
            dst: zmm1,
            src: zmm2,
            mask: None,
            elem: VecElementType::I32,
            width: VecWidth::V256,
            zeroing: false,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_vplzcnt));
        assert!(!x86_gate(invalid_vplzcnt));
    }

    for invalid_permute in [
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: VReg::Virtual(VirtualId(9)),
            table2: None,
            indices: zmm3,
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V512,
            overwrite_table: false,
            zeroing: false,
        },
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: zmm2,
            table2: None,
            indices: zmm3,
            mask: None,
            elem: VecElementType::I32,
            width: VecWidth::V512,
            overwrite_table: false,
            zeroing: false,
        },
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: zmm2,
            table2: None,
            indices: zmm3,
            mask: Some(x86(X86Reg::K(0))),
            elem: VecElementType::I8,
            width: VecWidth::V512,
            overwrite_table: false,
            zeroing: false,
        },
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: zmm2,
            table2: None,
            indices: zmm3,
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V512,
            overwrite_table: false,
            zeroing: true,
        },
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: zmm2,
            table2: None,
            indices: zmm3,
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V256,
            overwrite_table: false,
            zeroing: false,
        },
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: zmm2,
            table2: None,
            indices: zmm3,
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V512,
            overwrite_table: true,
            zeroing: false,
        },
        OpKind::X86PermuteBytesWords {
            dst: zmm1,
            table1: zmm2,
            table2: Some(zmm3),
            indices: zmm2,
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V512,
            overwrite_table: false,
            zeroing: false,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_permute));
        assert!(!x86_gate(invalid_permute));
    }

    for invalid_narrow in [
        OpKind::X86NarrowInt {
            dst: ymm1,
            src: VReg::Virtual(VirtualId(10)),
            mask: None,
            src_elem: VecElementType::I16,
            dst_elem: VecElementType::I8,
            width: VecWidth::V512,
            mode: crate::smir::ir::types::X86NarrowMode::Truncate,
            zeroing: false,
        },
        OpKind::X86NarrowInt {
            dst: zmm1,
            src: zmm2,
            mask: None,
            src_elem: VecElementType::I16,
            dst_elem: VecElementType::I8,
            width: VecWidth::V512,
            mode: crate::smir::ir::types::X86NarrowMode::Truncate,
            zeroing: false,
        },
        OpKind::X86NarrowInt {
            dst: ymm1,
            src: zmm2,
            mask: Some(x86(X86Reg::K(0))),
            src_elem: VecElementType::I16,
            dst_elem: VecElementType::I8,
            width: VecWidth::V512,
            mode: crate::smir::ir::types::X86NarrowMode::Truncate,
            zeroing: false,
        },
        OpKind::X86NarrowInt {
            dst: ymm1,
            src: zmm2,
            mask: None,
            src_elem: VecElementType::I16,
            dst_elem: VecElementType::I8,
            width: VecWidth::V512,
            mode: crate::smir::ir::types::X86NarrowMode::Truncate,
            zeroing: true,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_narrow));
        assert!(!x86_gate(invalid_narrow));
    }

    for invalid_aes in [
        OpKind::X86Aes {
            dst: zmm1,
            src1: zmm2,
            src2: None,
            width: VecWidth::V512,
            op: X86AesOp::Enc,
            imm: 0,
        },
        OpKind::X86Aes {
            dst: zmm1,
            src1: zmm2,
            src2: Some(zmm3),
            width: VecWidth::V512,
            op: X86AesOp::DecLast,
            imm: 1,
        },
        OpKind::X86Aes {
            dst: xmm1,
            src1: xmm2,
            src2: Some(xmm1),
            width: VecWidth::V128,
            op: X86AesOp::KeygenAssist,
            imm: 0,
        },
        OpKind::X86Aes {
            dst: x86(X86Reg::Xmm(16)),
            src1: xmm2,
            src2: None,
            width: VecWidth::V128,
            op: X86AesOp::InvMixColumns,
            imm: 0,
        },
        OpKind::X86Aes {
            dst: xmm1,
            src1: xmm2,
            src2: None,
            width: VecWidth::V128,
            op: X86AesOp::InvMixColumns,
            imm: 1,
        },
        OpKind::X86Aes {
            dst: xmm1,
            src1: xmm2,
            src2: Some(xmm1),
            width: VecWidth::V256,
            op: X86AesOp::EncLast,
            imm: 0,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_aes));
        assert!(!x86_gate(invalid_aes));
    }

    for invalid_sha512 in [
        OpKind::X86Sha512Msg1 {
            dst: ymm1,
            src: VReg::Virtual(VirtualId(11)),
        },
        OpKind::X86Sha512Msg1 {
            dst: xmm1,
            src: xmm2,
        },
        OpKind::X86Sha512Msg2 {
            dst: ymm1,
            src: xmm2,
        },
        OpKind::X86Sha512Rounds2 {
            dst: ymm1,
            state: xmm2,
            wk: xmm3,
        },
        OpKind::X86Sha512Rounds2 {
            dst: ymm1,
            state: ymm2,
            wk: ymm3,
        },
        OpKind::X86Sha512Msg2 {
            dst: x86(X86Reg::Ymm(16)),
            src: ymm2,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_sha512));
        assert!(!x86_gate(invalid_sha512));
    }

    for invalid_sm3 in [
        OpKind::X86Sm3Msg1 {
            dst: VReg::Virtual(VirtualId(12)),
            src1: xmm2,
            src2: xmm3,
        },
        OpKind::X86Sm3Msg2 {
            dst: xmm1,
            src1: ymm2,
            src2: xmm3,
        },
        OpKind::X86Sm3Rounds2 {
            dst: xmm1,
            state: xmm2,
            words: x86(X86Reg::Xmm(16)),
            imm: 0xFF,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_sm3));
        assert!(!x86_gate(invalid_sm3));
    }

    for invalid_sm4 in [
        OpKind::X86Sm4 {
            dst: xmm1,
            src1: ymm2,
            src2: xmm3,
            width: VecWidth::V128,
            key_schedule: false,
        },
        OpKind::X86Sm4 {
            dst: ymm1,
            src1: ymm2,
            src2: ymm3,
            width: VecWidth::V512,
            key_schedule: true,
        },
        OpKind::X86Sm4 {
            dst: x86(X86Reg::Xmm(16)),
            src1: xmm2,
            src2: xmm3,
            width: VecWidth::V128,
            key_schedule: true,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_sm4));
        assert!(!x86_gate(invalid_sm4));
    }

    for invalid_shift_imm in [
        OpKind::X86PackedShiftImm {
            dst: xmm1,
            src: xmm2,
            width: VecWidth::V64,
            elem: VecElementType::I16,
            shift: ShiftOp::Lsr,
            amount: 1,
            byte_lane: false,
        },
        OpKind::X86PackedShiftImm {
            dst: xmm1,
            src: xmm2,
            width: VecWidth::V128,
            elem: VecElementType::F32,
            shift: ShiftOp::Lsl,
            amount: 1,
            byte_lane: false,
        },
        OpKind::X86PackedShiftImm {
            dst: xmm1,
            src: xmm2,
            width: VecWidth::V128,
            elem: VecElementType::I16,
            shift: ShiftOp::Asr,
            amount: 1,
            byte_lane: true,
        },
        OpKind::X86PackedShiftImm {
            dst: xmm1,
            src: VReg::Virtual(VirtualId(13)),
            width: VecWidth::V128,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsr,
            amount: 1,
            byte_lane: false,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_shift_imm));
        assert!(!x86_gate(invalid_shift_imm));
    }

    for invalid_shared_count_shift in [
        OpKind::X86PackedShift {
            dst: xmm1,
            src: xmm2,
            count: xmm3,
            width: VecWidth::V64,
            elem: VecElementType::I16,
            shift: ShiftOp::Lsr,
        },
        OpKind::X86PackedShift {
            dst: ymm1,
            src: xmm2,
            count: xmm3,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsl,
        },
        OpKind::X86PackedShift {
            dst: xmm1,
            src: xmm2,
            count: ymm3,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            shift: ShiftOp::Asr,
        },
        OpKind::X86PackedShift {
            dst: xmm1,
            src: xmm2,
            count: VReg::Virtual(VirtualId(14)),
            width: VecWidth::V128,
            elem: VecElementType::I64,
            shift: ShiftOp::Lsr,
        },
        OpKind::X86PackedShift {
            dst: xmm1,
            src: xmm2,
            count: xmm3,
            width: VecWidth::V128,
            elem: VecElementType::F32,
            shift: ShiftOp::Lsl,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_shared_count_shift));
        assert!(!x86_gate(invalid_shared_count_shift));
    }

    for invalid_rotate in [
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: None,
            mask: None,
            amount: 1,
            width: VecWidth::V64,
            elem: VecElementType::I32,
            left: false,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: ymm1,
            src: xmm2,
            count: None,
            mask: None,
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: false,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: None,
            mask: None,
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I16,
            left: false,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: VReg::Virtual(VirtualId(15)),
            count: None,
            mask: None,
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I64,
            left: true,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: Some(ymm3),
            mask: None,
            amount: 0,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: true,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: Some(VReg::Virtual(VirtualId(16))),
            mask: None,
            amount: 0,
            width: VecWidth::V128,
            elem: VecElementType::I64,
            left: true,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: Some(xmm3),
            mask: None,
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: true,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: None,
            mask: None,
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: true,
            zeroing: true,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: None,
            mask: Some(x86(X86Reg::K(0))),
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: true,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: None,
            mask: Some(x86(X86Reg::K(8))),
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: true,
            zeroing: false,
        },
        OpKind::X86PackedRotate {
            dst: xmm1,
            src: xmm2,
            count: None,
            mask: Some(xmm3),
            amount: 1,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: true,
            zeroing: false,
        },
    ] {
        assert!(!is_x86_native_vector_op(&invalid_rotate));
        assert!(!x86_gate(invalid_rotate));
    }

    let invalid_bf16_output_width = OpKind::VCvtFP32ToBF16 {
        dst: zmm1,
        src1: zmm2,
        src2: None,
        mask: Some(k4),
        width: VecWidth::V512,
        zeroing: true,
    };
    assert!(!is_x86_native_vector_op(&invalid_bf16_output_width));
    assert!(!x86_gate(invalid_bf16_output_width));

    let invalid_bf16_mask_class = OpKind::VCvtFP32ToBF16 {
        dst: ymm1,
        src1: zmm2,
        src2: None,
        mask: Some(zmm3),
        width: VecWidth::V512,
        zeroing: false,
    };
    assert!(!is_x86_native_vector_op(&invalid_bf16_mask_class));
    assert!(!x86_gate(invalid_bf16_mask_class));

    let invalid_alias = OpKind::VDotProduct {
        dst: zmm1,
        acc: zmm2,
        src1: zmm2,
        src2: zmm3,
        mask: Some(k4),
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V512,
        src1_unsigned: true,
        saturate: false,
        zeroing: true,
    };
    assert!(!is_x86_native_vector_op(&invalid_alias));
    assert!(!x86_gate(invalid_alias));

    let invalid_signedness = OpKind::VDotProduct {
        dst: zmm1,
        acc: zmm1,
        src1: zmm2,
        src2: zmm3,
        mask: None,
        src_elem: VecElementType::I16,
        acc_elem: VecElementType::I32,
        width: VecWidth::V512,
        src1_unsigned: true,
        saturate: false,
        zeroing: false,
    };
    assert!(!is_x86_native_vector_op(&invalid_signedness));
    assert!(!x86_gate(invalid_signedness));

    let invalid_zeroing = OpKind::VDotProduct {
        dst: zmm1,
        acc: zmm1,
        src1: zmm2,
        src2: zmm3,
        mask: None,
        src_elem: VecElementType::I8,
        acc_elem: VecElementType::I32,
        width: VecWidth::V512,
        src1_unsigned: true,
        saturate: false,
        zeroing: true,
    };
    assert!(!is_x86_native_vector_op(&invalid_zeroing));
    assert!(!x86_gate(invalid_zeroing));

    let invalid_ifma_alias = OpKind::VMultiplyAdd52 {
        dst: zmm1,
        acc: zmm2,
        src1: zmm2,
        src2: zmm3,
        mask: Some(k4),
        width: VecWidth::V512,
        high: false,
        zeroing: true,
    };
    assert!(!is_x86_native_vector_op(&invalid_ifma_alias));
    assert!(!x86_gate(invalid_ifma_alias));
}
#[test]
fn x86_packed_integer_minmax_gate_validates_shapes_encodings_aliases_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let xmm3 = x86(X86Reg::Xmm(3));
    let ymm1 = x86(X86Reg::Ymm(1));
    let ymm2 = x86(X86Reg::Ymm(2));
    let ymm3 = x86(X86Reg::Ymm(3));
    let minmax = |dst, src1, src2, elem, lanes, op, signed| OpKind::VLane {
        dst,
        src1,
        src2,
        elem,
        lanes,
        op,
        signed,
        set_ovf: false,
    };

    for (kind, hint, requirements) in [
        (
            minmax(
                xmm1,
                xmm1,
                xmm2,
                VecElementType::I8,
                16,
                VLaneOp::Min,
                false,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xDA,
            },
            (false, false, false, false),
        ),
        (
            minmax(xmm1, xmm1, xmm2, VecElementType::I32, 4, VLaneOp::Max, true),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x3D,
            },
            (true, false, false, false),
        ),
        (
            minmax(
                xmm1,
                xmm2,
                xmm1,
                VecElementType::I16,
                8,
                VLaneOp::Min,
                false,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x3A,
                width: VecWidth::V128,
                w: true,
            },
            (false, true, false, false),
        ),
        (
            minmax(
                ymm1,
                ymm1,
                ymm3,
                VecElementType::I16,
                16,
                VLaneOp::Max,
                true,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xEE,
                width: VecWidth::V256,
                w: false,
            },
            (false, true, true, false),
        ),
        (
            minmax(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                x86(X86Reg::Xmm(18)),
                VecElementType::I32,
                4,
                VLaneOp::Min,
                true,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x39,
                width: VecWidth::V128,
                w: false,
            },
            (false, false, false, true),
        ),
        (
            minmax(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecElementType::I8,
                64,
                VLaneOp::Min,
                true,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x38,
                width: VecWidth::V512,
                w: true,
            },
            (false, false, false, false),
        ),
        (
            minmax(
                x86(X86Reg::Zmm(19)),
                x86(X86Reg::Zmm(20)),
                x86(X86Reg::Zmm(21)),
                VecElementType::I64,
                8,
                VLaneOp::Max,
                false,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x3F,
                width: VecWidth::V512,
                w: true,
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
        assert!(x86_vector_integer_minmax_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_vector_integer_minmax_feature_requirements(&smir_op),
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

    let valid = minmax(
        xmm1,
        xmm1,
        xmm2,
        VecElementType::I8,
        16,
        VLaneOp::Min,
        false,
    );
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
            op: VLaneOp::Min,
            signed: false,
            set_ovf: true,
        },
        minmax(xmm1, xmm1, xmm2, VecElementType::F32, 4, VLaneOp::Min, true),
        minmax(
            xmm1,
            xmm1,
            xmm2,
            VecElementType::I8,
            15,
            VLaneOp::Min,
            false,
        ),
        minmax(
            xmm1,
            xmm1,
            ymm3,
            VecElementType::I8,
            16,
            VLaneOp::Min,
            false,
        ),
        minmax(
            VReg::Virtual(VirtualId(65)),
            xmm1,
            xmm2,
            VecElementType::I8,
            16,
            VLaneOp::Min,
            false,
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
            minmax(
                xmm1,
                xmm2,
                xmm3,
                VecElementType::I8,
                16,
                VLaneOp::Min,
                false,
            ),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xDA,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid.clone(),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xDA,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            valid,
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xDE,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minmax(
                ymm1,
                ymm2,
                ymm3,
                VecElementType::I16,
                16,
                VLaneOp::Max,
                true,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0xEE,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minmax(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecElementType::I16,
                16,
                VLaneOp::Max,
                true,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xEE,
                width: VecWidth::V256,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minmax(
                xmm1,
                xmm2,
                xmm3,
                VecElementType::I64,
                2,
                VLaneOp::Max,
                false,
            ),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x3F,
                width: VecWidth::V128,
                w: true,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minmax(
                x86(X86Reg::Xmm(16)),
                x86(X86Reg::Xmm(17)),
                x86(X86Reg::Xmm(18)),
                VecElementType::I32,
                4,
                VLaneOp::Min,
                true,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x39,
                width: VecWidth::V128,
                w: true,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minmax(
                x86(X86Reg::Zmm(16)),
                x86(X86Reg::Zmm(17)),
                x86(X86Reg::Zmm(18)),
                VecElementType::I64,
                8,
                VLaneOp::Max,
                false,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x3F,
                width: VecWidth::V512,
                w: false,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minmax(
                x86(X86Reg::Ymm(16)),
                x86(X86Reg::Ymm(17)),
                x86(X86Reg::Ymm(18)),
                VecElementType::I8,
                32,
                VLaneOp::Min,
                true,
            ),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x38,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_phminposuw_gate_validates_registers_encodings_wig_and_features() {
    let xmm1 = x86(X86Reg::Xmm(1));
    let xmm2 = x86(X86Reg::Xmm(2));
    let minpos = |dst, src| OpKind::X86Phminposuw { dst, src };

    for (kind, hint, requirements) in [
        (
            minpos(xmm1, xmm2),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x41,
            },
            (true, false),
        ),
        (
            minpos(xmm1, xmm1),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x41,
                width: VecWidth::V128,
                w: true,
            },
            (false, true),
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_phminposuw_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");
        assert_eq!(
            x86_phminposuw_feature_requirements(&smir_op),
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
        minpos(xmm1, xmm2),
    );
    assert!(is_x86_native_vector_op(&unhinted.kind));
    assert!(!x86_native_vector_smir_op(&unhinted));

    for malformed_kind in [
        minpos(x86(X86Reg::Xmm(16)), xmm2),
        minpos(xmm1, x86(X86Reg::Ymm(2))),
        minpos(VReg::Virtual(VirtualId(62)), xmm2),
    ] {
        assert!(!x86_phminposuw_shape_valid(&malformed_kind));
        assert!(!is_x86_native_vector_op(&malformed_kind));
    }

    for (hint, label) in [
        (
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x41,
            },
            "legacy mandatory prefix",
        ),
        (
            X86OpHint::VexOp {
                map: X86VecMap::Map0F3A,
                pp: X86SsePrefix::OpSize,
                opcode: 0x41,
                width: VecWidth::V128,
                w: false,
            },
            "VEX map",
        ),
        (
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x40,
                width: VecWidth::V128,
                w: false,
            },
            "VEX opcode",
        ),
        (
            X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x41,
                width: VecWidth::V256,
                w: false,
            },
            "VEX.L",
        ),
        (
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: 0x41,
                width: VecWidth::V128,
                w: false,
            },
            "nonexistent EVEX form",
        ),
    ] {
        let malformed = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            minpos(xmm1, xmm2),
            hint,
        );
        assert!(!x86_native_vector_smir_op(&malformed), "{label}");
    }
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_vector_trampoline_round_trips_all_zmm_and_opmask_registers() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::Nop);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower vector-state no-op region");
    let code = lowerer
        .finalize()
        .expect("finalize vector-state no-op region");
    let exec = ExecMem::new(&code).expect("map vector-state no-op region");

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    for register in 0..32 {
        for lane in 0..8 {
            regs.zmm[register][lane] =
                0x5a00_0000_0000_0000 | ((register as u64) << 16) | lane as u64;
        }
    }
    for register in 0..8 {
        regs.k[register] = 0xa500_0000_0000_0000 | register as u64;
    }
    let expected_zmm = regs.zmm;
    let expected_k = regs.k;

    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.zmm, expected_zmm);
    assert_eq!(regs.k, expected_k);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_vector_trampoline_k16_mode_preserves_upper_opmask_state_without_bw() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") {
        return;
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, OpKind::Nop);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower narrow vector-state no-op region");
    let code = lowerer
        .finalize()
        .expect("finalize narrow vector-state no-op region");
    let exec = ExecMem::new(&code).expect("map narrow vector-state no-op region");

    let mut regs = GuestRegs {
        vector_active: X86_VECTOR_STATE_K16,
        ..GuestRegs::default()
    };
    for register in 0..32 {
        regs.zmm[register] = [0x5A00_0000_0000_0000 | register as u64; 8];
    }
    for register in 0..8 {
        regs.k[register] = 0xA5A5_5A5A_C3C3_0000 | (0x1001 * register as u64 + 0x55AA);
    }
    let expected_zmm = regs.zmm;
    let expected_k = regs.k;

    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.zmm, expected_zmm);
    assert_eq!(
        regs.k, expected_k,
        "KMOVW stores must not overwrite architectural K[63:16]"
    );
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_vector_trampoline_round_trips_guest_mxcsr_and_restores_host() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    fn read_mxcsr() -> u32 {
        let mut value = 0u32;
        unsafe {
            core::arch::asm!(
                "stmxcsr [{ptr}]",
                ptr = in(reg) &mut value,
                options(nostack, preserves_flags)
            );
        }
        value
    }

    // stmxcsr [rdi]; ldmxcsr [rsi]; ret
    let exec =
        ExecMem::new(&[0x0F, 0xAE, 0x1F, 0x0F, 0xAE, 0x16, 0xC3]).expect("map raw MXCSR block");
    let host_before = read_mxcsr();
    let mut observed = 0u32;
    let replacement = 0x5F80u32;
    let mut regs = GuestRegs {
        vector_active: 1,
        mxcsr: 0x3F80,
        ..GuestRegs::default()
    };
    regs.gpr[7] = (&mut observed as *mut u32) as u64;
    regs.gpr[6] = (&replacement as *const u32) as u64;

    exec.run(0, &mut regs);

    assert_eq!(observed, 0x3F80, "block did not observe guest MXCSR");
    assert_eq!(
        regs.mxcsr, replacement,
        "guest MXCSR write was not captured"
    );
    assert_eq!(
        regs.host_mxcsr, host_before,
        "host MXCSR save slot mismatch"
    );
    assert_eq!(
        read_mxcsr(),
        host_before,
        "guest MXCSR leaked into host Rust"
    );
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_vector_trampoline_executes_masked_rotate_and_round_trips_state() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86PackedRotate {
            dst: x86(X86Reg::Zmm(17)),
            src: x86(X86Reg::Zmm(18)),
            count: None,
            mask: Some(x86(X86Reg::K(4))),
            amount: 7,
            width: VecWidth::V512,
            elem: VecElementType::I32,
            left: true,
            zeroing: true,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower masked VPROLD region");
    assert!(lowered.relocations.is_empty());
    let code = lowerer.finalize().expect("finalize masked VPROLD region");
    let exec = ExecMem::new(&code).expect("map masked VPROLD region");

    let source = [
        0x0123_4567_89ab_cdef,
        0x1111_2222_3333_4444,
        0x8000_0001_7fff_ffff,
        0xdead_beef_cafe_babe,
        0x0102_0304_0506_0708,
        0xf0e0_d0c0_b0a0_9080,
        0x1357_9bdf_2468_ace0,
        0xffff_ffff_0000_0001,
    ];
    let mask = 0x5555u64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        let input = (source[lane / 2] >> ((lane % 2) * 32)) as u32;
        let output = if ((mask >> lane) & 1) != 0 {
            input.rotate_left(7)
        } else {
            0
        };
        expected[lane / 2] |= (output as u64) << ((lane % 2) * 32);
    }

    let mut regs = GuestRegs::default();
    regs.vector_active = 1;
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, source);
    regs.k[4] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), source, "source ZMM must survive");
    assert_eq!(regs.k[4], mask, "source opmask must survive");
}
#[test]
fn x86_vector_memory_gate_requires_memory_mode_and_exact_architectural_shapes() {
    let gate = |op: OpKind, allow_mem: bool| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let function = builder.finish();
        is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), allow_mem)
    };

    let addresses = [
        Address::Direct(x86(X86Reg::Rsp)),
        Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rbp)),
            index: x86(X86Reg::R16),
            scale: 8,
            disp: -64,
            disp_size: DispSize::Disp8,
        },
        Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::R31)),
            index: None,
            scale: 1,
            disp: i64::MIN,
        },
    ];
    let shapes = [
        (x86(X86Reg::Xmm(0)), VecWidth::V128),
        (x86(X86Reg::Ymm(16)), VecWidth::V256),
        (x86(X86Reg::Zmm(31)), VecWidth::V512),
    ];
    for (index, (vector, width)) in shapes.into_iter().enumerate() {
        let addr = addresses[index].clone();
        for op in [
            OpKind::VLoad {
                dst: vector,
                addr: addr.clone(),
                width,
            },
            OpKind::VStore {
                src: vector,
                addr: addr.clone(),
                width,
            },
        ] {
            assert!(x86_jit_vector_mem_shape_valid(&op));
            assert!(!gate(op.clone(), false));
            assert!(gate(op, true));
        }
    }

    for malformed in [
        OpKind::VLoad {
            dst: x86(X86Reg::Xmm(1)),
            addr: Address::Direct(x86(X86Reg::Rax)),
            width: VecWidth::V256,
        },
        OpKind::VStore {
            src: VReg::Virtual(VirtualId(4)),
            addr: Address::Direct(x86(X86Reg::Rax)),
            width: VecWidth::V128,
        },
        OpKind::VLoad {
            dst: x86(X86Reg::Zmm(32)),
            addr: Address::Direct(x86(X86Reg::Rax)),
            width: VecWidth::V512,
        },
        OpKind::VStore {
            src: x86(X86Reg::Ymm(2)),
            addr: Address::GpRel { offset: 0 },
            width: VecWidth::V256,
        },
    ] {
        assert!(!x86_jit_vector_mem_shape_valid(&malformed));
        assert!(!gate(malformed, true));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::VLoad {
            dst: x86(X86Reg::Ymm(17)),
            addr: Address::Absolute(0x2000),
            width: VecWidth::V256,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(uses_x86_native_vectors_excluding(
        &builder.finish(),
        &std::collections::HashMap::new(),
    ));
}
