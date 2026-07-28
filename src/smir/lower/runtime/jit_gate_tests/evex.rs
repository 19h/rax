//! jit_gate_tests::evex tests

use super::*;
use crate::smir::lower::runtime::*;

#[test]
fn x86_aes_feature_requirements_distinguish_vex_evex_vl_and_aes_ni() {
    let aes = |dst, src1, src2, width, op| OpKind::X86Aes {
        dst,
        src1,
        src2,
        width,
        op,
        imm: 0,
    };
    assert_eq!(
        x86_aes_feature_requirements(&aes(
            x86(X86Reg::Xmm(1)),
            x86(X86Reg::Xmm(2)),
            Some(x86(X86Reg::Xmm(3))),
            VecWidth::V128,
            X86AesOp::Enc,
        )),
        (true, false, false)
    );
    assert_eq!(
        x86_aes_feature_requirements(&aes(
            x86(X86Reg::Ymm(1)),
            x86(X86Reg::Ymm(2)),
            Some(x86(X86Reg::Ymm(3))),
            VecWidth::V256,
            X86AesOp::Dec,
        )),
        (false, true, false)
    );
    assert_eq!(
        x86_aes_feature_requirements(&aes(
            x86(X86Reg::Xmm(16)),
            x86(X86Reg::Xmm(17)),
            Some(x86(X86Reg::Xmm(18))),
            VecWidth::V128,
            X86AesOp::EncLast,
        )),
        (false, true, true)
    );
    assert_eq!(
        x86_aes_feature_requirements(&aes(
            x86(X86Reg::Zmm(16)),
            x86(X86Reg::Zmm(17)),
            Some(x86(X86Reg::Zmm(18))),
            VecWidth::V512,
            X86AesOp::Dec,
        )),
        (false, true, false)
    );
    assert_eq!(
        x86_aes_feature_requirements(&aes(
            x86(X86Reg::Xmm(9)),
            x86(X86Reg::Xmm(8)),
            None,
            VecWidth::V128,
            X86AesOp::InvMixColumns,
        )),
        (true, false, false)
    );
    assert_eq!(
        x86_aes_feature_requirements(&OpKind::Nop),
        (false, false, false)
    );
}
#[test]
fn x86_packed_shift_imm_requirements_select_vex_or_evex_exactly() {
    let op = |dst, src, width, elem, shift| OpKind::X86PackedShiftImm {
        dst,
        src,
        width,
        elem,
        shift,
        amount: 3,
        byte_lane: false,
    };
    assert_eq!(
        x86_packed_shift_imm_feature_requirements(&op(
            x86(X86Reg::Xmm(1)),
            x86(X86Reg::Xmm(2)),
            VecWidth::V128,
            VecElementType::I32,
            ShiftOp::Lsr
        )),
        (true, false, false)
    );
    assert_eq!(
        x86_packed_shift_imm_feature_requirements(&op(
            x86(X86Reg::Ymm(1)),
            x86(X86Reg::Ymm(2)),
            VecWidth::V256,
            VecElementType::I32,
            ShiftOp::Lsl
        )),
        (false, true, false)
    );
    assert_eq!(
        x86_packed_shift_imm_feature_requirements(&op(
            x86(X86Reg::Xmm(16)),
            x86(X86Reg::Xmm(17)),
            VecWidth::V128,
            VecElementType::I32,
            ShiftOp::Asr
        )),
        (false, false, true)
    );
    assert_eq!(
        x86_packed_shift_imm_feature_requirements(&op(
            x86(X86Reg::Zmm(1)),
            x86(X86Reg::Zmm(2)),
            VecWidth::V512,
            VecElementType::I64,
            ShiftOp::Asr
        )),
        (false, false, false)
    );
}
#[test]
fn x86_packed_shared_count_requirements_select_vex_or_evex_exactly() {
    let op = |dst, src, count, width, elem, shift| OpKind::X86PackedShift {
        dst,
        src,
        count,
        width,
        elem,
        shift,
    };
    assert_eq!(
        x86_packed_shift_feature_requirements(&op(
            x86(X86Reg::Xmm(1)),
            x86(X86Reg::Xmm(2)),
            x86(X86Reg::Xmm(3)),
            VecWidth::V128,
            VecElementType::I32,
            ShiftOp::Lsr
        )),
        (true, false, false)
    );
    assert_eq!(
        x86_packed_shift_feature_requirements(&op(
            x86(X86Reg::Ymm(1)),
            x86(X86Reg::Ymm(2)),
            x86(X86Reg::Xmm(3)),
            VecWidth::V256,
            VecElementType::I16,
            ShiftOp::Lsl
        )),
        (false, true, false)
    );
    assert_eq!(
        x86_packed_shift_feature_requirements(&op(
            x86(X86Reg::Xmm(1)),
            x86(X86Reg::Xmm(2)),
            x86(X86Reg::Xmm(18)),
            VecWidth::V128,
            VecElementType::I32,
            ShiftOp::Asr
        )),
        (false, false, true)
    );
    assert_eq!(
        x86_packed_shift_feature_requirements(&op(
            x86(X86Reg::Xmm(1)),
            x86(X86Reg::Xmm(2)),
            x86(X86Reg::Xmm(3)),
            VecWidth::V128,
            VecElementType::I64,
            ShiftOp::Asr
        )),
        (false, false, true)
    );
    assert_eq!(
        x86_packed_shift_feature_requirements(&op(
            x86(X86Reg::Zmm(17)),
            x86(X86Reg::Zmm(18)),
            x86(X86Reg::Xmm(19)),
            VecWidth::V512,
            VecElementType::I64,
            ShiftOp::Lsl
        )),
        (false, false, false)
    );
}
#[test]
fn fixup_imm_native_gate_validates_shapes_full_immediate_and_encodings() {
    let packed = OpKind::X86FixupImm {
        dst: x86(X86Reg::Zmm(17)),
        src1: x86(X86Reg::Zmm(18)),
        src2: x86(X86Reg::Zmm(19)),
        mask: Some(x86(X86Reg::K(2))),
        elem: VecElementType::F32,
        width: VecWidth::V512,
        lanes: 16,
        imm: 0xFF,
        scalar: false,
        mask_zeroing: true,
        suppress_exceptions: true,
    };
    let packed_op = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0xA000,
        packed.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F3A,
            pp: X86SsePrefix::OpSize,
            opcode: 0x54,
            width: VecWidth::V512,
            w: false,
        },
    );
    assert!(is_x86_native_vector_op(&packed));
    assert!(x86_native_vector_smir_op(&packed_op));

    let scalar = OpKind::X86FixupImm {
        dst: x86(X86Reg::Xmm(17)),
        src1: x86(X86Reg::Xmm(18)),
        src2: x86(X86Reg::Xmm(19)),
        mask: Some(x86(X86Reg::K(2))),
        elem: VecElementType::F64,
        width: VecWidth::V128,
        lanes: 1,
        imm: 0xC3,
        scalar: true,
        mask_zeroing: true,
        suppress_exceptions: true,
    };
    let scalar_op = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(1),
        0xA007,
        scalar.clone(),
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F3A,
            pp: X86SsePrefix::OpSize,
            opcode: 0x55,
            width: VecWidth::V128,
            w: true,
        },
    );
    assert!(is_x86_native_vector_op(&scalar));
    assert!(x86_native_vector_smir_op(&scalar_op));

    let mut wrong_hint = scalar_op;
    wrong_hint.x86_hint = Some(X86OpHint::EvexOp {
        map: X86VecMap::Map0F3A,
        pp: X86SsePrefix::OpSize,
        opcode: 0x54,
        width: VecWidth::V128,
        w: true,
    });
    assert!(!x86_native_vector_smir_op(&wrong_hint));

    let mut virtual_source = scalar.clone();
    let OpKind::X86FixupImm { src2, .. } = &mut virtual_source else {
        unreachable!()
    };
    *src2 = VReg::virt(7);
    assert!(!is_x86_native_vector_op(&virtual_source));

    let mut short_sae = packed;
    let OpKind::X86FixupImm {
        dst,
        src1,
        src2,
        width,
        lanes,
        ..
    } = &mut short_sae
    else {
        unreachable!()
    };
    *dst = x86(X86Reg::Ymm(17));
    *src1 = x86(X86Reg::Ymm(18));
    *src2 = x86(X86Reg::Ymm(19));
    *width = VecWidth::V256;
    *lanes = 8;
    assert!(!is_x86_native_vector_op(&short_sae));
}
#[test]
fn packed_int_to_fp16_native_gate_requires_canonical_architectural_evex_shape() {
    let kind = OpKind::X86PackedIntToFp16 {
        dst: x86(X86Reg::Xmm(17)),
        src: x86(X86Reg::Zmm(18)),
        mask: Some(x86(X86Reg::K(3))),
        int_elem: VecElementType::I64,
        signed: true,
        lanes: 8,
        src_width: VecWidth::V512,
        dst_width: VecWidth::V128,
        mask_zeroing: true,
        zero_upper: true,
        round: crate::smir::ir::types::FpRoundMode::RoundDown,
        suppress_exceptions: true,
    };
    let hint = X86OpHint::EvexOp {
        map: X86VecMap::Map5,
        pp: X86SsePrefix::None,
        opcode: 0x5B,
        width: VecWidth::V512,
        w: true,
    };
    let canonical = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        kind.clone(),
        hint,
    );
    assert!(is_x86_native_vector_op(&kind));
    assert!(x86_native_vector_smir_op(&canonical));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(hint);
    assert!(is_native_clobber_safe(&function));

    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, kind.clone());
    assert!(!x86_native_vector_smir_op(&unhinted));
    let wrong_hint = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        kind,
        X86OpHint::EvexOp {
            map: X86VecMap::Map5,
            pp: X86SsePrefix::Repne,
            opcode: 0x7A,
            width: VecWidth::V512,
            w: true,
        },
    );
    assert!(!x86_native_vector_smir_op(&wrong_hint));

    for malformed in [
        OpKind::X86PackedIntToFp16 {
            dst: x86(X86Reg::Ymm(17)),
            src: x86(X86Reg::Zmm(18)),
            mask: Some(x86(X86Reg::K(3))),
            int_elem: VecElementType::I64,
            signed: true,
            lanes: 8,
            src_width: VecWidth::V512,
            dst_width: VecWidth::V128,
            mask_zeroing: true,
            zero_upper: true,
            round: crate::smir::ir::types::FpRoundMode::RoundDown,
            suppress_exceptions: true,
        },
        OpKind::X86PackedIntToFp16 {
            dst: x86(X86Reg::Xmm(1)),
            src: VReg::Virtual(VirtualId(0)),
            mask: None,
            int_elem: VecElementType::I32,
            signed: false,
            lanes: 4,
            src_width: VecWidth::V128,
            dst_width: VecWidth::V64,
            mask_zeroing: false,
            zero_upper: true,
            round: crate::smir::ir::types::FpRoundMode::Dynamic,
            suppress_exceptions: false,
        },
        OpKind::X86PackedIntToFp16 {
            dst: x86(X86Reg::Xmm(1)),
            src: x86(X86Reg::Ymm(2)),
            mask: None,
            int_elem: VecElementType::I32,
            signed: true,
            lanes: 8,
            src_width: VecWidth::V256,
            dst_width: VecWidth::V128,
            mask_zeroing: false,
            zero_upper: true,
            round: crate::smir::ir::types::FpRoundMode::RoundUp,
            suppress_exceptions: true,
        },
        OpKind::X86PackedIntToFp16 {
            dst: x86(X86Reg::Ymm(1)),
            src: x86(X86Reg::Zmm(2)),
            mask: None,
            int_elem: VecElementType::I32,
            signed: true,
            lanes: 16,
            src_width: VecWidth::V512,
            dst_width: VecWidth::V256,
            mask_zeroing: false,
            zero_upper: true,
            round: crate::smir::ir::types::FpRoundMode::RoundNearestTiesAway,
            suppress_exceptions: true,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn packed_fp16_to_int_native_gate_requires_canonical_architectural_evex_shape() {
    let kind = OpKind::X86PackedFp16ToInt {
        dst: x86(X86Reg::Zmm(17)),
        src: x86(X86Reg::Xmm(18)),
        mask: Some(x86(X86Reg::K(3))),
        int_elem: VecElementType::I64,
        signed: true,
        truncate: false,
        lanes: 8,
        src_width: VecWidth::V128,
        dst_width: VecWidth::V512,
        mask_zeroing: true,
        zero_upper: true,
        round: crate::smir::ir::types::FpRoundMode::RoundDown,
        suppress_exceptions: true,
    };
    let hint = X86OpHint::EvexOp {
        map: X86VecMap::Map5,
        pp: X86SsePrefix::OpSize,
        opcode: 0x7B,
        width: VecWidth::V512,
        w: false,
    };
    let canonical = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        kind.clone(),
        hint,
    );
    assert!(is_x86_native_vector_op(&kind));
    assert!(x86_native_vector_smir_op(&canonical));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(hint);
    assert!(is_native_clobber_safe(&function));

    let unhinted =
        crate::smir::ir::ops::SmirOp::new(crate::smir::ir::types::OpId(0), 0x1000, kind.clone());
    assert!(!x86_native_vector_smir_op(&unhinted));
    let wrong_hint = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        kind,
        X86OpHint::EvexOp {
            map: X86VecMap::Map5,
            pp: X86SsePrefix::OpSize,
            opcode: 0x79,
            width: VecWidth::V512,
            w: false,
        },
    );
    assert!(!x86_native_vector_smir_op(&wrong_hint));

    for malformed in [
        OpKind::X86PackedFp16ToInt {
            dst: x86(X86Reg::Ymm(17)),
            src: x86(X86Reg::Xmm(18)),
            mask: Some(x86(X86Reg::K(3))),
            int_elem: VecElementType::I64,
            signed: true,
            truncate: false,
            lanes: 8,
            src_width: VecWidth::V128,
            dst_width: VecWidth::V512,
            mask_zeroing: true,
            zero_upper: true,
            round: crate::smir::ir::types::FpRoundMode::RoundDown,
            suppress_exceptions: true,
        },
        OpKind::X86PackedFp16ToInt {
            dst: x86(X86Reg::Xmm(1)),
            src: x86(X86Reg::Xmm(2)),
            mask: None,
            int_elem: VecElementType::I64,
            signed: true,
            truncate: false,
            lanes: 2,
            src_width: VecWidth::V128,
            dst_width: VecWidth::V128,
            mask_zeroing: false,
            zero_upper: true,
            round: crate::smir::ir::types::FpRoundMode::Dynamic,
            suppress_exceptions: false,
        },
        OpKind::X86PackedFp16ToInt {
            dst: x86(X86Reg::Zmm(1)),
            src: x86(X86Reg::Ymm(2)),
            mask: None,
            int_elem: VecElementType::I32,
            signed: false,
            truncate: true,
            lanes: 16,
            src_width: VecWidth::V256,
            dst_width: VecWidth::V512,
            mask_zeroing: false,
            zero_upper: true,
            round: crate::smir::ir::types::FpRoundMode::RoundUp,
            suppress_exceptions: true,
        },
    ] {
        assert!(!is_x86_native_vector_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_evex_fp_replay_is_admitted_only_with_exact_provenance() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VADDPS: [u8; 6] = [0x62, 0xF1, 0x6C, 0xC9, 0x58, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VADDPS, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);

    assert!(!is_native_clobber_safe(&function));
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VADDPS).unwrap());
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut wrong_block = function.clone();
    let instruction = wrong_block
        .x86_instruction_bytes
        .remove(&(BlockId(0), PC))
        .unwrap();
    wrong_block
        .x86_instruction_bytes
        .insert((BlockId(1), PC), instruction);
    assert!(!is_native_clobber_safe(&wrong_block));

    let mut excluded = std::collections::HashMap::new();
    excluded.insert(function.entry, PC);
    assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
    assert!(x86_native_vector_features_supported_excluding(
        &function, &excluded
    ));

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}
#[test]
fn x86_evex_broadcast_replay_requires_dq_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VBROADCASTF32X2: [u8; 6] = [0x62, 0xA2, 0x7D, 0xC9, 0x19, 0xCA];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, &VBROADCASTF32X2, &mut context)
        .unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);

    assert!(!is_native_clobber_safe(&function));
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VBROADCASTF32X2).unwrap(),
    );
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512dq")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VBROADCASTF32X2;
    bytes[5] = 0x08;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_narrow_broadcast_replay_uses_bw_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VPBROADCASTB: [u8; 6] = [0x62, 0xA2, 0x7D, 0xC9, 0x78, 0xCA];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPBROADCASTB, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);

    assert!(!is_native_clobber_safe(&function));
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPBROADCASTB).unwrap(),
    );
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPBROADCASTB;
    bytes[5] = 0x08;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_logic_replay_requires_dq_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VANDPS: [u8; 6] = [0x62, 0xF1, 0x6C, 0xC9, 0x54, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VANDPS, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VANDPS).unwrap());

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512dq")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VANDPS;
    bytes[5] = 0x08;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_integer_arithmetic_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VPADDSB: [u8; 6] = [0x62, 0xA1, 0x6D, 0xC1, 0xEC, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPADDSB, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPADDSB).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPADDSB;
    bytes[5] = 0x08;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_shared_count_shift_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VPSRAQ: [u8; 6] = [0x62, 0xA1, 0xED, 0xC1, 0xE2, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPSRAQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPSRAQ).unwrap());

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPSRAQ;
    bytes[5] = 0x08;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_immediate_count_shift_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VPSRAQ: [u8; 7] = [0x62, 0xB1, 0xF5, 0xC1, 0x72, 0xE2, 0x05];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPSRAQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPSRAQ).unwrap());

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPSRAQ;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_packed_fma_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VFMADD231PD: [u8; 6] = [0x62, 0xA2, 0xED, 0xC1, 0xB8, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VFMADD231PD, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VFMADD231PD).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VFMADD231PD;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_scalar_fma_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vfmadd231sd xmm17{k1}{z}, xmm18, xmm19
    const VFMADD231SD: [u8; 6] = [0x62, 0xA2, 0xED, 0x81, 0xB9, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VFMADD231SD, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VFMADD231SD).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VFMADD231SD;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_fp16_fma_replay_requires_fp16_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for (bytes, needs_vl) in [
        (&[0x62, 0xF6, 0x6D, 0x08, 0x98, 0xCB][..], true),
        (&[0x62, 0xA6, 0x6D, 0xC1, 0xB8, 0xCB][..], false),
        (&[0x62, 0xA6, 0x6D, 0x81, 0xBF, 0xCB][..], false),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && std::is_x86_feature_detected!("avx512fp16")
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl")),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );

        let mut memory_metadata = function;
        let mut memory_bytes = [0u8; 6];
        memory_bytes.copy_from_slice(bytes);
        memory_bytes[5] &= 0x3f;
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            X86InstructionBytes::new(&memory_bytes).unwrap(),
        );
        assert!(!is_native_clobber_safe(&memory_metadata), "{bytes:02X?}");
    }
}
#[test]
fn x86_evex_reduce_replay_requires_exact_dq_fp16_and_vl_features() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for (bytes, needs_dq, needs_fp16, needs_vl) in [
        (
            &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x53][..],
            true,
            false,
            true,
        ),
        (
            &[0x62, 0xF3, 0xFD, 0x48, 0x56, 0xCB, 0xA7][..],
            true,
            false,
            false,
        ),
        (
            &[0x62, 0xF3, 0x6D, 0x08, 0x57, 0xCB, 0x4D][..],
            true,
            false,
            false,
        ),
        (
            &[0x62, 0xF3, 0x7C, 0x08, 0x56, 0xCB, 0xB9][..],
            false,
            true,
            true,
        ),
        (
            &[0x62, 0xA3, 0x7C, 0x9A, 0x56, 0xCB, 0xB9][..],
            false,
            true,
            false,
        ),
        (
            &[0x62, 0xF3, 0x6C, 0x08, 0x57, 0xCB, 0x10][..],
            false,
            true,
            false,
        ),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && (!needs_dq || std::is_x86_feature_detected!("avx512dq"))
                && (!needs_fp16 || std::is_x86_feature_detected!("avx512fp16"))
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl")),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn x86_evex_range_replay_requires_exact_dq_and_vl_features() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for (bytes, needs_vl) in [
        (&[0x62, 0xF3, 0x6D, 0x08, 0x50, 0xCB, 0x05][..], true),
        (&[0x62, 0xF3, 0xED, 0x48, 0x50, 0xCB, 0x0D][..], false),
        (&[0x62, 0xF3, 0x6D, 0x08, 0x51, 0xCB, 0x05][..], false),
        (&[0x62, 0xA3, 0xED, 0x92, 0x51, 0xCB, 0x0D][..], false),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && std::is_x86_feature_detected!("avx512dq")
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl")),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn x86_evex_approx14_requires_f_and_vl_only_for_short_packed_forms() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for (bytes, needs_vl) in [
        (&[0x62, 0xF2, 0x7D, 0x08, 0x4C, 0xCB][..], true),
        (&[0x62, 0xF2, 0xFD, 0x28, 0x4C, 0xCB][..], true),
        (&[0x62, 0xA2, 0x7D, 0xCA, 0x4C, 0xCB][..], false),
        (&[0x62, 0xF2, 0x6D, 0x08, 0x4D, 0xCB][..], false),
        (&[0x62, 0xA2, 0xED, 0x82, 0x4D, 0xCB][..], false),
        (&[0x62, 0xF2, 0x7D, 0x08, 0x4E, 0xCB][..], true),
        (&[0x62, 0xF2, 0xFD, 0x28, 0x4E, 0xCB][..], true),
        (&[0x62, 0xA2, 0x7D, 0xCA, 0x4E, 0xCB][..], false),
        (&[0x62, 0xF2, 0x6D, 0x08, 0x4F, 0xCB][..], false),
        (&[0x62, 0xA2, 0xED, 0x82, 0x4F, 0xCB][..], false),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        assert!(
            x86_native_vector_uses_k16_opmasks_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f")
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl")),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn x86_evex_exp2_recip28_and_rsqrt28_require_exact_er_without_bw_or_vl() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for bytes in [
        &[0x62, 0xF2, 0x7D, 0x48, 0xC8, 0xCB][..],
        &[0x62, 0xF2, 0xFD, 0x48, 0xC8, 0xCB][..],
        &[0x62, 0xA2, 0x7D, 0x99, 0xC8, 0xCB][..],
        &[0x62, 0xF2, 0x7D, 0x48, 0xCA, 0xCB][..],
        &[0x62, 0xF2, 0xFD, 0x48, 0xCA, 0xCB][..],
        &[0x62, 0xA2, 0x7D, 0x99, 0xCA, 0xCB][..],
        &[0x62, 0xF2, 0x6D, 0x08, 0xCB, 0xCB][..],
        &[0x62, 0xA2, 0xED, 0x92, 0xCB, 0xCB][..],
        &[0x62, 0xF2, 0x7D, 0x48, 0xCC, 0xCB][..],
        &[0x62, 0xF2, 0xFD, 0x48, 0xCC, 0xCB][..],
        &[0x62, 0xA2, 0x7D, 0x99, 0xCC, 0xCB][..],
        &[0x62, 0xF2, 0x6D, 0x08, 0xCD, 0xCB][..],
        &[0x62, 0xA2, 0xED, 0x92, 0xCD, 0xCB][..],
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        assert!(
            x86_native_vector_uses_k16_opmasks_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );

        let recip28 = lifter
            .lift_insn(
                PC + bytes.len() as u64,
                &[0x62, 0xF2, 0x7D, 0x48, 0xCA, 0xCB],
                &mut context,
            )
            .unwrap();
        let mut mixed_er_function = function.clone();
        mixed_er_function.blocks[0].ops.extend(recip28.ops);
        let rsqrt28 = lifter
            .lift_insn(
                PC + bytes.len() as u64,
                &[0x62, 0xF2, 0x7D, 0x48, 0xCC, 0xCB],
                &mut context,
            )
            .unwrap();
        mixed_er_function.blocks[0].ops.extend(rsqrt28.ops);
        assert!(
            x86_native_vector_uses_k16_opmasks_excluding(
                &mixed_er_function,
                &std::collections::HashMap::new()
            ),
            "mixed VEXP2/VRCP28/VRSQRT28 region must retain low-16-bit opmask marshalling"
        );

        let scale_f = lifter
            .lift_insn(
                PC + bytes.len() as u64,
                &[0x62, 0xF2, 0x6D, 0x48, 0x2C, 0xCB],
                &mut context,
            )
            .unwrap();
        let mut mixed_function = function.clone();
        mixed_function.blocks[0].ops.extend(scale_f.ops);
        assert!(
            !x86_native_vector_uses_k16_opmasks_excluding(
                &mixed_function,
                &std::collections::HashMap::new()
            ),
            "mixed AVX512ER/VSCALEF region must retain full-width opmask marshalling"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f") && x86_host_has_avx512er(),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn x86_evex_scale_f_replay_requires_exact_fp16_and_vl_features() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for (bytes, needs_fp16, needs_vl) in [
        (&[0x62, 0xF2, 0x6D, 0x08, 0x2C, 0xCB][..], false, true),
        (&[0x62, 0xF2, 0x6D, 0x48, 0x2C, 0xCB][..], false, false),
        (&[0x62, 0xF2, 0x6D, 0x08, 0x2D, 0xCB][..], false, false),
        (&[0x62, 0xF6, 0x6D, 0x08, 0x2C, 0xCB][..], true, true),
        (&[0x62, 0xA6, 0x6D, 0x92, 0x2C, 0xCB][..], true, false),
        (&[0x62, 0xF6, 0x6D, 0x08, 0x2D, 0xCB][..], true, false),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        assert!(
            !x86_native_vector_uses_k16_opmasks_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && (!needs_fp16 || std::is_x86_feature_detected!("avx512fp16"))
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl")),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn x86_evex_fp16_approx_requires_fp16_bw_and_exact_vl_features() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for (bytes, needs_vl) in [
        (&[0x62, 0xF6, 0x7D, 0x08, 0x4C, 0xCB][..], true),
        (&[0x62, 0xF6, 0x7D, 0x28, 0x4E, 0xCB][..], true),
        (&[0x62, 0xA6, 0x7D, 0xCA, 0x4C, 0xCB][..], false),
        (&[0x62, 0xA6, 0x7D, 0xCA, 0x4E, 0xCB][..], false),
        (&[0x62, 0xF6, 0x6D, 0x08, 0x4D, 0xCB][..], false),
        (&[0x62, 0xA6, 0x6D, 0x82, 0x4F, 0xCB][..], false),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        assert!(
            !x86_native_vector_uses_k16_opmasks_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "FP16 approximate regions require full-width opmask marshalling: {bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && std::is_x86_feature_detected!("avx512fp16")
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl")),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn x86_evex_fp16_complex_requires_exact_fp16_and_vl_features() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    for (bytes, needs_vl) in [
        (&[0x62, 0xF6, 0x6E, 0x08, 0xD6, 0xCB][..], true),
        (&[0x62, 0xA6, 0x6E, 0x12, 0x56, 0xCB][..], false),
        (&[0x62, 0xA6, 0x6F, 0xB2, 0xD7, 0xCB][..], false),
    ] {
        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
        let result = lifter.lift_insn(PC, bytes, &mut context).unwrap();
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        function
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());

        assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
            "{bytes:02X?}"
        );
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && std::is_x86_feature_detected!("avx512fp16")
                && (!needs_vl || std::is_x86_feature_detected!("avx512vl")),
            "{bytes:02X?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(
            !x86_native_vector_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            "{bytes:02X?}"
        );
    }
}
#[test]
fn x86_evex_integer_minmax_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VPMAXSQ: [u8; 6] = [0x62, 0xA2, 0xED, 0xC1, 0x3D, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPMAXSQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPMAXSQ).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPMAXSQ;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_integer_multiply_replay_requires_dq_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    const VPMULLQ: [u8; 6] = [0x62, 0xA2, 0xED, 0xC1, 0x40, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPMULLQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPMULLQ).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512dq")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPMULLQ;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_integer_interleave_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vpunpckhqdq zmm17{k1}{z}, zmm18, zmm19
    const VPUNPCKHQDQ: [u8; 6] = [0x62, 0xA1, 0xED, 0xC1, 0x6D, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPUNPCKHQDQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPUNPCKHQDQ).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPUNPCKHQDQ;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_integer_pack_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vpackusdw zmm17{k1}{z}, zmm18, zmm19
    const VPACKUSDW: [u8; 6] = [0x62, 0xA2, 0x6D, 0xC1, 0x2B, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPACKUSDW, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPACKUSDW).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPACKUSDW;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_packed_abs_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vpabsq zmm17{k1}{z}, zmm18
    const VPABSQ: [u8; 6] = [0x62, 0xA2, 0xFD, 0xC9, 0x1F, 0xCA];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPABSQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPABSQ).unwrap());

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPABSQ;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_packed_average_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vpavgw zmm17{k1}{z}, zmm18, zmm19
    const VPAVGW: [u8; 6] = [0x62, 0xA1, 0x6D, 0xC1, 0xE3, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPAVGW, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPAVGW).unwrap());

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPAVGW;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_packed_test_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vptestnmq k2{k1}, zmm18, zmm19
    const VPTESTNMQ: [u8; 6] = [0x62, 0xB2, 0xEE, 0x41, 0x27, 0xD3];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPTESTNMQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPTESTNMQ).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPTESTNMQ;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_packed_compare_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vpcmpq k2{k1}, zmm18, zmm19, equal
    const VPCMPQ: [u8; 7] = [0x62, 0xB3, 0xED, 0x41, 0x1F, 0xD3, 0x00];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPCMPQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPCMPQ).unwrap());

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VPCMPQ;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[test]
fn x86_evex_fp_shuffle_replay_uses_base_vector_gate_and_rejects_memory_metadata() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0x1000;
    // vunpcklpd zmm17{k1}{z}, zmm18, zmm19
    const VUNPCKLPD: [u8; 6] = [0x62, 0xA1, 0xED, 0xC1, 0x14, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VUNPCKLPD, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VUNPCKLPD).unwrap(),
    );

    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_vector_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let mut memory_metadata = function;
    let mut bytes = VUNPCKLPD;
    bytes[5] &= 0x3f;
    memory_metadata
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
    assert!(!is_native_clobber_safe(&memory_metadata));
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_fp_replay_crosses_virtual_temp_barrier_and_executes_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    const PC: u64 = 0x1000;
    const VADDPS: [u8; 6] = [0x62, 0xF1, 0x6C, 0xC9, 0x58, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VADDPS, &mut context).unwrap();
    assert!(
        result.ops.iter().any(|op| op
            .kind
            .dests()
            .iter()
            .any(|dst| matches!(dst, VReg::Virtual(_)))),
        "test instruction must exercise the virtual-vector-temp barrier"
    );

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    let semantic_only = function.clone();
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VADDPS).unwrap());
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    assert!(!is_native_clobber_safe(&semantic_only));
    assert!(is_native_clobber_safe(&function));
    assert!(uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    let host_supports_replay =
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw");
    assert_eq!(
        x86_native_vector_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        host_supports_replay
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower replay region");
    let code = lowerer.finalize().expect("finalize replay region");
    assert!(code.windows(VADDPS.len()).any(|window| window == VADDPS));
    if !host_supports_replay {
        return;
    }

    let exec = ExecMem::new(&code).expect("map replay region");
    let mut source1 = [0u64; 8];
    let mut source2 = [0u64; 8];
    let mut expected = [0u64; 8];
    let mask = 0xA55Au64;
    for lane in 0..16 {
        let lhs = lane as f32 + 0.25;
        let rhs = (32 - lane) as f32 + 0.5;
        let shift = (lane % 2) * 32;
        source1[lane / 2] |= (lhs.to_bits() as u64) << shift;
        source2[lane / 2] |= (rhs.to_bits() as u64) << shift;
        if mask >> lane & 1 != 0 {
            expected[lane / 2] |= ((lhs + rhs).to_bits() as u64) << shift;
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(1, [u64::MAX; 8]);
    regs.set_zmm(2, source1);
    regs.set_zmm(3, source2);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(1), expected);
    assert_eq!(regs.get_zmm(2), source1);
    assert_eq!(regs.get_zmm(3), source2);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_logic_replay_executes_masked_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512dq")
    {
        return;
    }

    const PC: u64 = 0x1000;
    // vandps zmm17{k1}{z}, zmm18, zmm19
    const VANDPS: [u8; 6] = [0x62, 0xA1, 0x6C, 0xC1, 0x54, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VANDPS, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VANDPS).unwrap());

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VANDPS replay");
    let code = lowerer.finalize().expect("finalize VANDPS replay");
    assert!(code.windows(VANDPS.len()).any(|window| window == VANDPS));
    let exec = ExecMem::new(&code).expect("map VANDPS replay");

    let source1 = [
        0xFFFF_0000_F0F0_0F0F,
        0x0123_4567_89AB_CDEF,
        0xAAAA_AAAA_5555_5555,
        0x8000_0001_7FFF_FFFE,
        0xDEAD_BEEF_CAFE_BABE,
        0x1357_9BDF_2468_ACE0,
        0xFFFF_FFFF_0000_0000,
        0x0102_0304_0506_0708,
    ];
    let source2 = [
        0x0FF0_F00F_FFFF_FFFF,
        0xFEDC_BA98_7654_3210,
        0x3333_3333_CCCC_CCCC,
        0xFFFF_FFFF_0000_0001,
        0xFFFF_0000_FFFF_0000,
        0xFFFF_FFFF_FFFF_FFFF,
        0x1234_5678_9ABC_DEF0,
        0xF0E0_D0C0_B0A0_9080,
    ];
    let mask = 0xA55Au64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        if mask >> lane & 1 != 0 {
            let shift = (lane % 2) * 32;
            let lhs = (source1[lane / 2] >> shift) as u32;
            let rhs = (source2[lane / 2] >> shift) as u32;
            expected[lane / 2] |= ((lhs & rhs) as u64) << shift;
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, source1);
    regs.set_zmm(19, source2);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), source1);
    assert_eq!(regs.get_zmm(19), source2);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_integer_arithmetic_replay_executes_saturating_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpaddsb zmm17{k1}{z}, zmm18, zmm19
    const VPADDSB: [u8; 6] = [0x62, 0xA1, 0x6D, 0xC1, 0xEC, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPADDSB, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPADDSB).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPADDSB replay");
    let code = lowerer.finalize().expect("finalize VPADDSB replay");
    assert!(code.windows(VPADDSB.len()).any(|window| window == VPADDSB));
    let exec = ExecMem::new(&code).expect("map VPADDSB replay");

    let mask = 0xA55A_F00F_9696_6996u64;
    let mut source1 = [0u64; 8];
    let mut source2 = [0u64; 8];
    let mut expected = [0u64; 8];
    for lane in 0..64 {
        let lhs = (lane as i16 * 17 - 200) as i8;
        let rhs = (300 - lane as i16 * 13) as i8;
        let shift = (lane % 8) * 8;
        source1[lane / 8] |= (lhs as u8 as u64) << shift;
        source2[lane / 8] |= (rhs as u8 as u64) << shift;
        if mask >> lane & 1 != 0 {
            expected[lane / 8] |= (lhs.saturating_add(rhs) as u8 as u64) << shift;
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, source1);
    regs.set_zmm(19, source2);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), source1);
    assert_eq!(regs.get_zmm(19), source2);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_shared_count_shift_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpsraq zmm17{k1}{z}, zmm18, xmm19
    const VPSRAQ: [u8; 6] = [0x62, 0xA1, 0xED, 0xC1, 0xE2, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPSRAQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPSRAQ).unwrap());

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPSRAQ replay");
    let code = lowerer.finalize().expect("finalize VPSRAQ replay");
    assert!(code.windows(VPSRAQ.len()).any(|window| window == VPSRAQ));
    let exec = ExecMem::new(&code).expect("map VPSRAQ replay");

    let source = [
        0x7FFF_FFFF_FFFF_FFE0,
        0x8000_0000_0000_0020,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0,
        u64::MAX,
        0x4000_0000_0000_0000,
        0xC000_0000_0000_0000,
    ];
    let count = [5, 0xDEAD_BEEF_CAFE_BABE, u64::MAX, 7, 11, 13, 17, 19];
    let mask = 0xA5u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if mask >> lane & 1 != 0 {
            expected[lane] = ((source[lane] as i64) >> 5) as u64;
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, source);
    regs.set_zmm(19, count);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), source);
    assert_eq!(regs.get_zmm(19), count);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_immediate_count_shift_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpsraq zmm17{k1}{z}, zmm18, 5
    const VPSRAQ: [u8; 7] = [0x62, 0xB1, 0xF5, 0xC1, 0x72, 0xE2, 0x05];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPSRAQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPSRAQ).unwrap());

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower immediate VPSRAQ replay");
    let code = lowerer
        .finalize()
        .expect("finalize immediate VPSRAQ replay");
    assert!(code.windows(VPSRAQ.len()).any(|window| window == VPSRAQ));
    let exec = ExecMem::new(&code).expect("map immediate VPSRAQ replay");

    let source = [
        0x7FFF_FFFF_FFFF_FFE0,
        0x8000_0000_0000_0020,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0,
        u64::MAX,
        0x4000_0000_0000_0000,
        0xC000_0000_0000_0000,
    ];
    let mask = 0xA5u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if mask >> lane & 1 != 0 {
            expected[lane] = ((source[lane] as i64) >> 5) as u64;
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, source);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), source);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_packed_fma_replay_executes_masked_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vfmadd231pd zmm17{k1}{z}, zmm18, zmm19
    const VFMADD231PD: [u8; 6] = [0x62, 0xA2, 0xED, 0xC1, 0xB8, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VFMADD231PD, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VFMADD231PD).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower packed VFMADD231PD replay");
    let code = lowerer
        .finalize()
        .expect("finalize packed VFMADD231PD replay");
    assert!(
        code.windows(VFMADD231PD.len())
            .any(|window| window == VFMADD231PD)
    );
    let exec = ExecMem::new(&code).expect("map packed VFMADD231PD replay");

    let destination = [1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let lhs = [2.0f64, -3.0, 4.0, -5.0, 6.0, -7.0, 8.0, -9.0];
    let rhs = [10.0f64, 11.0, -12.0, -13.0, 14.0, 15.0, -16.0, -17.0];
    let mask = 0xA5u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if mask >> lane & 1 != 0 {
            expected[lane] = (lhs[lane] * rhs[lane] + destination[lane]).to_bits();
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, destination.map(f64::to_bits));
    regs.set_zmm(18, lhs.map(f64::to_bits));
    regs.set_zmm(19, rhs.map(f64::to_bits));
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs.map(f64::to_bits));
    assert_eq!(regs.get_zmm(19), rhs.map(f64::to_bits));
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_scalar_fma_replay_executes_masked_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vfmadd231sd xmm17{k1}{z}, xmm18, xmm19
    const VFMADD231SD: [u8; 6] = [0x62, 0xA2, 0xED, 0x81, 0xB9, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VFMADD231SD, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VFMADD231SD).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower scalar VFMADD231SD replay");
    let code = lowerer
        .finalize()
        .expect("finalize scalar VFMADD231SD replay");
    assert!(
        code.windows(VFMADD231SD.len())
            .any(|window| window == VFMADD231SD)
    );
    let exec = ExecMem::new(&code).expect("map scalar VFMADD231SD replay");

    let destination = [1.0f64.to_bits(), 0x1122_3344_5566_7788, 3, 4, 5, 6, 7, 8];
    let lhs = [
        2.0f64.to_bits(),
        0x8877_6655_4433_2211,
        13,
        14,
        15,
        16,
        17,
        18,
    ];
    let rhs = [
        3.0f64.to_bits(),
        0x0123_4567_89AB_CDEF,
        23,
        24,
        25,
        26,
        27,
        28,
    ];
    let expected = [7.0f64.to_bits(), destination[1], 0, 0, 0, 0, 0, 0];

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, destination);
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = 1;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], 1);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_integer_minmax_replay_executes_signed_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpmaxsq zmm17{k1}{z}, zmm18, zmm19
    const VPMAXSQ: [u8; 6] = [0x62, 0xA2, 0xED, 0xC1, 0x3D, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPMAXSQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPMAXSQ).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPMAXSQ replay");
    let code = lowerer.finalize().expect("finalize VPMAXSQ replay");
    assert!(code.windows(VPMAXSQ.len()).any(|window| window == VPMAXSQ));
    let exec = ExecMem::new(&code).expect("map VPMAXSQ replay");

    let lhs = [
        i64::MIN as u64,
        i64::MAX as u64,
        (-5i64) as u64,
        7,
        0,
        (-1i64) as u64,
        42,
        (-100i64) as u64,
    ];
    let rhs = [
        0,
        (-1i64) as u64,
        (-6i64) as u64,
        8,
        (-1i64) as u64,
        1,
        41,
        (-99i64) as u64,
    ];
    let mask = 0xA5u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if mask >> lane & 1 != 0 {
            expected[lane] = std::cmp::max(lhs[lane] as i64, rhs[lane] as i64) as u64;
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_integer_multiply_replay_executes_dq_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512dq")
    {
        return;
    }

    const PC: u64 = 0x1000;
    // vpmullq zmm17{k1}{z}, zmm18, zmm19
    const VPMULLQ: [u8; 6] = [0x62, 0xA2, 0xED, 0xC1, 0x40, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPMULLQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPMULLQ).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPMULLQ replay");
    let code = lowerer.finalize().expect("finalize VPMULLQ replay");
    assert!(code.windows(VPMULLQ.len()).any(|window| window == VPMULLQ));
    let exec = ExecMem::new(&code).expect("map VPMULLQ replay");

    let lhs = [
        u64::MAX,
        0x8000_0000_0000_0000,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0,
        1,
        0xFFFF_0000_FFFF_0000,
        0x0000_FFFF_0000_FFFF,
    ];
    let rhs = [3, 2, 7, 11, u64::MAX, u64::MAX, 0x10001, 0x1_0000_0001];
    let mask = 0xA5u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if mask >> lane & 1 != 0 {
            expected[lane] = lhs[lane].wrapping_mul(rhs[lane]);
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_integer_interleave_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpunpckhqdq zmm17{k1}{z}, zmm18, zmm19
    const VPUNPCKHQDQ: [u8; 6] = [0x62, 0xA1, 0xED, 0xC1, 0x6D, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPUNPCKHQDQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPUNPCKHQDQ).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPUNPCKHQDQ replay");
    let code = lowerer.finalize().expect("finalize VPUNPCKHQDQ replay");
    assert!(
        code.windows(VPUNPCKHQDQ.len())
            .any(|window| window == VPUNPCKHQDQ)
    );
    let exec = ExecMem::new(&code).expect("map VPUNPCKHQDQ replay");

    let lhs = [10, 11, 12, 13, 14, 15, 16, 17];
    let rhs = [20, 21, 22, 23, 24, 25, 26, 27];
    let interleaved = [11, 21, 13, 23, 15, 25, 17, 27];
    let mask = 0xA5u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if mask >> lane & 1 != 0 {
            expected[lane] = interleaved[lane];
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_integer_pack_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpackusdw zmm17{k1}{z}, zmm18, zmm19
    const VPACKUSDW: [u8; 6] = [0x62, 0xA2, 0x6D, 0xC1, 0x2B, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPACKUSDW, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPACKUSDW).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPACKUSDW replay");
    let code = lowerer.finalize().expect("finalize VPACKUSDW replay");
    assert!(
        code.windows(VPACKUSDW.len())
            .any(|window| window == VPACKUSDW)
    );
    let exec = ExecMem::new(&code).expect("map VPACKUSDW replay");

    let lhs_elements = [
        -1,
        0,
        1,
        65_535,
        65_536,
        i32::MAX,
        i32::MIN,
        42,
        32_767,
        32_768,
        65_534,
        65_535,
        -32_768,
        -2,
        70_000,
        12_345,
    ];
    let rhs_elements = [
        65_535,
        65_536,
        -1,
        2,
        100_000,
        -100_000,
        255,
        256,
        i32::MAX,
        0,
        -214,
        4_096,
        7,
        65_534,
        65_537,
        i32::MIN,
    ];
    let pack_i32 = |elements: &[i32; 16]| {
        std::array::from_fn(|word| {
            elements[word * 2] as u32 as u64 | ((elements[word * 2 + 1] as u32 as u64) << 32)
        })
    };
    let lhs = pack_i32(&lhs_elements);
    let rhs = pack_i32(&rhs_elements);

    let mut packed = [0u16; 32];
    for lane in 0..4 {
        for element in 0..4 {
            let saturate = |value: i32| value.clamp(0, u16::MAX as i32) as u16;
            packed[lane * 8 + element] = saturate(lhs_elements[lane * 4 + element]);
            packed[lane * 8 + 4 + element] = saturate(rhs_elements[lane * 4 + element]);
        }
    }
    let mask = 0xF0F0_0F0F_A55A_C33Cu64;
    for (lane, value) in packed.iter_mut().enumerate() {
        if mask >> lane & 1 == 0 {
            *value = 0;
        }
    }
    let expected: [u64; 8] = std::array::from_fn(|word| {
        (0..4).fold(0u64, |bits, element| {
            bits | ((packed[word * 4 + element] as u64) << (element * 16))
        })
    });

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_packed_abs_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpabsq zmm17{k1}{z}, zmm18
    const VPABSQ: [u8; 6] = [0x62, 0xA2, 0xFD, 0xC9, 0x1F, 0xCA];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPABSQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPABSQ).unwrap());

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPABSQ replay");
    let code = lowerer.finalize().expect("finalize VPABSQ replay");
    assert!(code.windows(VPABSQ.len()).any(|window| window == VPABSQ));
    let exec = ExecMem::new(&code).expect("map VPABSQ replay");

    let source = [
        0,
        1,
        (-1i64) as u64,
        i64::MAX as u64,
        i64::MIN as u64,
        (-123_456_789i64) as u64,
        42,
        (-7i64) as u64,
    ];
    let mask = 0xF0F0_0F0F_5A5A_A5BDu64;
    let expected = std::array::from_fn(|lane| {
        if mask >> lane & 1 != 0 {
            (source[lane] as i64).wrapping_abs() as u64
        } else {
            0
        }
    });

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, source);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), source);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_packed_average_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpavgw zmm17{k1}{z}, zmm18, zmm19
    const VPAVGW: [u8; 6] = [0x62, 0xA1, 0x6D, 0xC1, 0xE3, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPAVGW, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPAVGW).unwrap());

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPAVGW replay");
    let code = lowerer.finalize().expect("finalize VPAVGW replay");
    assert!(code.windows(VPAVGW.len()).any(|window| window == VPAVGW));
    let exec = ExecMem::new(&code).expect("map VPAVGW replay");

    let lhs_elements: [u16; 32] = std::array::from_fn(|lane| match lane % 8 {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => u16::MAX,
        4 => u16::MAX - 1,
        5 => 32_767,
        6 => 32_768,
        _ => (lane * 257) as u16,
    });
    let rhs_elements: [u16; 32] = std::array::from_fn(|lane| match lane % 8 {
        0 | 1 => 0,
        2 => 1,
        3 | 4 => u16::MAX,
        5 => 32_768,
        6 => 32_767,
        _ => u16::MAX - (lane * 131) as u16,
    });
    let pack = |elements: &[u16; 32]| {
        std::array::from_fn(|word| {
            (0..4).fold(0u64, |bits, element| {
                bits | ((elements[word * 4 + element] as u64) << (element * 16))
            })
        })
    };
    let lhs = pack(&lhs_elements);
    let rhs = pack(&rhs_elements);
    let mask = 0xF0F0_0F0F_A55A_C33Cu64;
    let expected_elements: [u16; 32] = std::array::from_fn(|lane| {
        if mask >> lane & 1 != 0 {
            ((lhs_elements[lane] as u32 + rhs_elements[lane] as u32 + 1) >> 1) as u16
        } else {
            0
        }
    });
    let expected = pack(&expected_elements);

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_packed_test_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vptestnmq k2{k1}, zmm18, zmm19
    const VPTESTNMQ: [u8; 6] = [0x62, 0xB2, 0xEE, 0x41, 0x27, 0xD3];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPTESTNMQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VPTESTNMQ).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPTESTNMQ replay");
    let code = lowerer.finalize().expect("finalize VPTESTNMQ replay");
    assert!(
        code.windows(VPTESTNMQ.len())
            .any(|window| window == VPTESTNMQ)
    );
    let exec = ExecMem::new(&code).expect("map VPTESTNMQ replay");

    let lhs = [0, 1, 2, 3, 4, 5, 6, 7];
    let rhs = [u64::MAX, 1, 4, 0, 4, 8, 2, 7];
    let input_mask = 0xA5u64;
    let expected_mask = 0x25u64;

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = input_mask;
    regs.k[2] = u64::MAX;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], input_mask);
    assert_eq!(regs.k[2], expected_mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_packed_compare_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vpcmpq k2{k1}, zmm18, zmm19, equal
    const VPCMPQ: [u8; 7] = [0x62, 0xB3, 0xED, 0x41, 0x1F, 0xD3, 0x00];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VPCMPQ, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(&VPCMPQ).unwrap());

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VPCMPQ replay");
    let code = lowerer.finalize().expect("finalize VPCMPQ replay");
    assert!(code.windows(VPCMPQ.len()).any(|window| window == VPCMPQ));
    let exec = ExecMem::new(&code).expect("map VPCMPQ replay");

    let lhs = [0, 1, 2, 3, 4, 5, 6, 7];
    let rhs = [0, 9, 2, 8, 4, 10, 6, 11];
    let input_mask = 0xA5u64;
    let expected_mask = 0x05u64;

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = input_mask;
    regs.k[2] = u64::MAX;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], input_mask);
    assert_eq!(regs.k[2], expected_mask);
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_evex_fp_shuffle_replay_executes_high_register_form_exactly() {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const PC: u64 = 0x1000;
    // vunpcklpd zmm17{k1}{z}, zmm18, zmm19
    const VUNPCKLPD: [u8; 6] = [0x62, 0xA1, 0xED, 0xC1, 0x14, 0xCB];
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter.lift_insn(PC, &VUNPCKLPD, &mut context).unwrap();
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&VUNPCKLPD).unwrap(),
    );

    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower VUNPCKLPD replay");
    let code = lowerer.finalize().expect("finalize VUNPCKLPD replay");
    assert!(
        code.windows(VUNPCKLPD.len())
            .any(|window| window == VUNPCKLPD)
    );
    let exec = ExecMem::new(&code).expect("map VUNPCKLPD replay");

    let lhs = [10, 11, 12, 13, 14, 15, 16, 17];
    let rhs = [20, 21, 22, 23, 24, 25, 26, 27];
    let interleaved = [10, 20, 12, 22, 14, 24, 16, 26];
    let mask = 0xA5u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if mask >> lane & 1 != 0 {
            expected[lane] = interleaved[lane];
        }
    }

    let mut regs = GuestRegs {
        vector_active: 1,
        ..GuestRegs::default()
    };
    regs.set_zmm(17, [u64::MAX; 8]);
    regs.set_zmm(18, lhs);
    regs.set_zmm(19, rhs);
    regs.k[1] = mask;
    exec.run(lowered.entry_offset, &mut regs);

    assert_eq!(regs.get_zmm(17), expected);
    assert_eq!(regs.get_zmm(18), lhs);
    assert_eq!(regs.get_zmm(19), rhs);
    assert_eq!(regs.k[1], mask);
}
