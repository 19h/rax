//! Feature accumulation for exact register-only native-replay spans.

use super::X86NativeReplayFeatureRequirements;
use crate::smir::ir::SmirBlock;
use crate::smir::ir::X86InstructionBytes;
use crate::smir::ir::types::{BlockId, GuestAddr};

pub(super) fn accumulate_x86_native_replay_span_requirements(
    block: &SmirBlock,
    instruction_bytes: &std::collections::HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    requirements: &mut X86NativeReplayFeatureRequirements,
    all_spans_support_avx_ymm16: &mut bool,
) {
    for span in crate::smir::ir::x86_native_replay_spans(block, instruction_bytes).into_values() {
        // Scalar high-byte replay needs no vector-state marshalling. CRC32's
        // SSE4.2 requirement remains enforced by the scalar host-feature gate,
        // including state-backed ESP/EBP forms.
        if span.instruction.is_legacy_high_byte_register_replay() {
            continue;
        }
        let legacy_widening_dword_multiply = span
            .instruction
            .legacy_register_widening_dword_multiply_replay();
        let legacy_scalar_extract = span.instruction.legacy_register_scalar_extract_replay();
        let legacy_scalar_insert = span.instruction.legacy_register_scalar_insert_replay();
        let legacy_lane_shuffle = span.instruction.legacy_register_lane_shuffle_replay();
        let legacy_alignr = span.instruction.legacy_register_alignr_replay().is_some();
        let legacy_gfni = span.instruction.legacy_register_gfni_replay().is_some();
        let legacy_mov_mask_stack = span.instruction.legacy_mov_mask_stack_destination_replay();
        let legacy_movd_q_stack = span.instruction.legacy_movd_q_stack_replay();
        // The MMX form has an independent architectural state bridge. It must
        // not make an MMX-only region require AVX vector-state marshalling.
        if legacy_widening_dword_multiply.is_some_and(|replay| replay.mmx)
            || legacy_scalar_extract.is_some_and(|replay| replay.kind.touches_mmx())
            || legacy_scalar_insert.is_some_and(|replay| replay.kind.touches_mmx())
            || legacy_mov_mask_stack.is_some_and(|replay| replay.touches_mmx())
            || legacy_movd_q_stack.is_some_and(|replay| replay.touches_mmx())
        {
            continue;
        }
        let legacy_aes = span.instruction.legacy_register_aes_replay().is_some();
        let legacy_blend = span.instruction.legacy_register_blend_replay().is_some();
        let legacy_scalar_xmm_movq = span.instruction.legacy_scalar_xmm_movq_replay().is_some();
        let legacy_packed_fp_convert = span
            .instruction
            .legacy_register_packed_fp_convert_replay()
            .is_some();
        let legacy_scalar_fp_convert = span
            .instruction
            .legacy_register_scalar_fp_convert_replay()
            .is_some();
        let legacy_round = span.instruction.legacy_register_round_replay().is_some();
        let legacy_dot_product = span
            .instruction
            .legacy_register_dot_product_replay()
            .is_some();
        let legacy_insertps = span.instruction.legacy_register_insertps_replay().is_some();
        let legacy_pclmulqdq = span
            .instruction
            .legacy_register_pclmulqdq_replay()
            .is_some();
        let legacy_ptest = span.instruction.legacy_register_ptest_replay().is_some();
        let legacy_packed_extend = span
            .instruction
            .legacy_register_packed_extend_replay()
            .is_some();
        let legacy_packed_shift = span
            .instruction
            .legacy_register_packed_shift_replay()
            .is_some();
        let legacy_fp_flag_compare = span
            .instruction
            .legacy_register_fp_flag_compare_replay()
            .is_some();
        let legacy_sha = span.instruction.legacy_register_sha_replay().is_some();
        let is_fma3 = span.instruction.is_vex_register_fma3();
        let is_fma4 = span.instruction.is_vex_register_fma4();
        let is_vpermil2 = span.instruction.is_vex_register_vpermil2();
        let vex_fp_dot_product_ymm = span.instruction.vex_register_fp_dot_product_uses_ymm();
        let vex_integer_dot = span.instruction.vex_register_integer_dot_fields().is_some();
        let vex_ifma52 = span.instruction.vex_register_ifma52_fields().is_some();
        let vex_ne_convert = span.instruction.vex_register_ne_convert_fields().is_some();
        let vex_integer_dot_ext_int16 = span.instruction.vex_register_integer_dot_ext_is_int16();
        let immediate_blend_avx2 = span.instruction.vex_register_immediate_blend_needs_avx2();
        let immediate_permute_avx2 = span.instruction.vex_register_immediate_permute_needs_avx2();
        let chunk_extract_avx2 = span.instruction.vex_register_chunk_extract_needs_avx2();
        let scalar_extract_avx = span.instruction.is_vex_register_scalar_extract();
        let legacy_mov_mask_stack = legacy_mov_mask_stack.is_some();
        let legacy_movd_q_stack = legacy_movd_q_stack.is_some();
        let mov_mask_stack_avx2 = span.instruction.vex_mov_mask_stack_destination_needs_avx2();
        let vex_ptest = span.instruction.is_vex_register_ptest();
        let variable_blend_avx2 = span.instruction.vex_register_variable_blend_needs_avx2();
        let variable_permute_avx2 = span.instruction.vex_register_variable_permute_needs_avx2();
        let alignr_avx2 = span.instruction.vex_register_alignr_needs_avx2();
        let cross_lane_128_avx2 = span.instruction.vex_register_cross_lane_128_needs_avx2();
        let scalar_insert_avx = span.instruction.is_vex_register_scalar_insert();
        let vex_gfni_ymm = span.instruction.vex_register_gfni_uses_ymm();
        let vex_vpclmulqdq_ymm = span.instruction.vex_register_vpclmulqdq_uses_ymm();
        let fp_horizontal_addsub_avx = span
            .instruction
            .legacy_vex_register_fp_horizontal_addsub_needs_avx();
        let fp_estimate_avx = span.instruction.legacy_vex_register_fp_estimate_needs_avx();
        let fp_arithmetic_avx = span
            .instruction
            .legacy_vex_register_fp_arithmetic_needs_avx();
        let fp_compare_avx = span.instruction.legacy_vex_register_fp_compare_needs_avx();
        let fp_shuffle_avx = span.instruction.legacy_vex_register_fp_shuffle_needs_avx();
        let high_low_move_avx = span
            .instruction
            .legacy_vex_register_high_low_move_needs_avx();
        let scalar_move_avx = span.instruction.legacy_vex_register_scalar_move_needs_avx();
        let fp_sqrt_avx = span.instruction.legacy_vex_register_fp_sqrt_needs_avx();
        let widening_dword_multiply_avx2 = span
            .instruction
            .vex_register_widening_dword_multiply_needs_avx2();
        let vex_packed_extend_avx2 = span.instruction.vex_register_packed_extend_needs_avx2();
        let vex_aligned_packed_fp_move = span.instruction.is_vex_register_aligned_packed_fp_move();
        let vex_unaligned_packed_fp_move =
            span.instruction.is_vex_register_unaligned_packed_fp_move();
        let vex_packed_integer_move = span.instruction.is_vex_register_packed_integer_move();
        let vex_scalar_vmovq = span.instruction.is_vex_register_scalar_vmovq();
        let vex_register_broadcast = span
            .instruction
            .vex_register_broadcast_element_bits()
            .is_some();
        let vex_lane_shuffle_avx2 = span.instruction.vex_register_lane_shuffle_needs_avx2();
        let vex_fp32_fp64_convert = span.instruction.is_vex_register_fp32_fp64_convert();
        let vex_fp16_widen = span.instruction.is_vex_register_fp16_widen();
        let vex_fp16_narrow = span.instruction.is_vex_register_fp16_narrow();
        let vex_fp_flag_compare = span.instruction.is_vex_register_fp_flag_compare();
        let vex_round = span.instruction.vex_round_destination_index().is_some();
        let vex_scalar_fp_convert = span
            .instruction
            .vex_scalar_fp_convert_destination_index()
            .is_some();
        let vex_scalar_fp_to_int = span
            .instruction
            .vex_scalar_fp_to_int_destination_index()
            .is_some();
        let vex_scalar_int_to_fp = span
            .instruction
            .vex_scalar_int_to_fp_destination_index()
            .is_some();
        let vex_zero = span.instruction.vex_zeroes_all_register_bits().is_some();
        let vex_packed_string = span.instruction.is_vex_register_packed_string_compare();
        let vex_fp_logic = span.instruction.is_vex_register_fp_logic();
        let vex_new_ymm16_upper_clear_destination = span
            .instruction
            .vex_avx_ymm16_upper_clear_destination_index();
        requirements.any = true;
        requirements.needs_sse3 |= fp_horizontal_addsub_avx == Some(false)
            || legacy_lane_shuffle.is_some_and(|replay| replay.kind.requires_sse3());
        requirements.needs_ssse3 |= legacy_alignr;
        requirements.needs_sse41 |= legacy_blend
            || legacy_packed_extend
            || legacy_round
            || legacy_dot_product
            || legacy_insertps
            || legacy_ptest
            || legacy_scalar_extract.is_some_and(|replay| replay.kind.requires_sse41())
            || legacy_scalar_insert.is_some_and(|replay| replay.kind.requires_sse41())
            || legacy_widening_dword_multiply.is_some_and(|replay| replay.signed);
        requirements.needs_vex_unaligned_packed_fp_move |= vex_unaligned_packed_fp_move;
        *all_spans_support_avx_ymm16 &= legacy_aes
            || legacy_blend
            || legacy_scalar_xmm_movq
            || legacy_packed_fp_convert
            || legacy_scalar_fp_convert
            || legacy_scalar_extract.is_some()
            || legacy_scalar_insert.is_some()
            || legacy_lane_shuffle.is_some()
            || legacy_alignr
            || legacy_gfni
            || legacy_round
            || legacy_dot_product
            || legacy_insertps
            || legacy_pclmulqdq
            || legacy_ptest
            || legacy_packed_extend
            || legacy_packed_shift
            || legacy_widening_dword_multiply.is_some()
            || legacy_fp_flag_compare
            || legacy_sha
            || is_fma4
            || is_vpermil2
            || vex_fp_dot_product_ymm.is_some()
            || vex_integer_dot
            || vex_ifma52
            || vex_ne_convert
            || vex_integer_dot_ext_int16.is_some()
            || fp_horizontal_addsub_avx == Some(false)
            || fp_estimate_avx.is_some()
            || fp_arithmetic_avx == Some(false)
            || fp_shuffle_avx == Some(false)
            || high_low_move_avx == Some(false)
            || scalar_move_avx == Some(false)
            || fp_sqrt_avx == Some(false)
            || vex_new_ymm16_upper_clear_destination.is_some()
            || immediate_blend_avx2.is_some()
            || immediate_permute_avx2.is_some()
            || chunk_extract_avx2.is_some()
            || scalar_extract_avx
            || legacy_mov_mask_stack
            || legacy_movd_q_stack
            || mov_mask_stack_avx2.is_some()
            || vex_ptest
            || variable_blend_avx2.is_some()
            || variable_permute_avx2.is_some()
            || alignr_avx2.is_some()
            || cross_lane_128_avx2.is_some()
            || scalar_insert_avx
            || vex_gfni_ymm.is_some()
            || vex_vpclmulqdq_ymm.is_some()
            || vex_packed_extend_avx2.is_some()
            || vex_aligned_packed_fp_move
            || vex_unaligned_packed_fp_move
            || vex_packed_integer_move
            || vex_scalar_vmovq
            || vex_register_broadcast
            || vex_lane_shuffle_avx2.is_some()
            || vex_fp32_fp64_convert
            || vex_fp16_widen
            || vex_fp16_narrow
            || vex_fp_flag_compare
            || vex_round
            || vex_scalar_fp_convert
            || vex_scalar_fp_to_int
            || vex_scalar_int_to_fp
            || vex_zero
            || vex_packed_string;
        requirements.needs_avx |= legacy_aes
            || legacy_blend
            || legacy_scalar_xmm_movq
            || legacy_packed_fp_convert
            || legacy_scalar_fp_convert
            || legacy_scalar_extract.is_some()
            || legacy_scalar_insert.is_some()
            || legacy_lane_shuffle.is_some()
            || legacy_alignr
            || legacy_gfni
            || legacy_round
            || legacy_dot_product
            || legacy_insertps
            || legacy_pclmulqdq
            || legacy_ptest
            || legacy_packed_extend
            || legacy_packed_shift
            || legacy_widening_dword_multiply.is_some()
            || legacy_fp_flag_compare
            || legacy_sha
            || vex_packed_string
            || is_fma3
            || is_fma4
            || is_vpermil2
            || vex_fp_dot_product_ymm.is_some()
            || vex_integer_dot
            || vex_ifma52
            || vex_ne_convert
            || vex_integer_dot_ext_int16.is_some()
            || immediate_blend_avx2.is_some()
            || immediate_permute_avx2.is_some()
            || chunk_extract_avx2.is_some()
            || scalar_extract_avx
            || mov_mask_stack_avx2.is_some()
            || vex_ptest
            || variable_blend_avx2.is_some()
            || variable_permute_avx2.is_some()
            || alignr_avx2.is_some()
            || cross_lane_128_avx2.is_some()
            || scalar_insert_avx
            || vex_gfni_ymm.is_some()
            || vex_vpclmulqdq_ymm.is_some()
            || vex_fp_logic
            || fp_horizontal_addsub_avx == Some(true)
            || widening_dword_multiply_avx2.is_some()
            || vex_packed_extend_avx2.is_some()
            || fp_arithmetic_avx == Some(true)
            || fp_compare_avx == Some(true)
            || fp_estimate_avx == Some(true)
            || fp_shuffle_avx == Some(true)
            || high_low_move_avx == Some(true)
            || scalar_move_avx == Some(true)
            || fp_sqrt_avx == Some(true)
            || vex_aligned_packed_fp_move
            || vex_unaligned_packed_fp_move
            || vex_packed_integer_move
            || vex_scalar_vmovq
            || vex_register_broadcast
            || vex_lane_shuffle_avx2.is_some()
            || vex_fp32_fp64_convert
            || vex_fp16_widen
            || vex_fp16_narrow
            || vex_fp_flag_compare
            || vex_round
            || vex_scalar_fp_convert
            || vex_scalar_fp_to_int
            || vex_scalar_int_to_fp
            || vex_zero;
        requirements.needs_avx_vnni |= vex_integer_dot;
        requirements.needs_avx_ifma |= vex_ifma52;
        requirements.needs_avx_ne_convert |= vex_ne_convert;
        requirements.needs_avx2 |= widening_dword_multiply_avx2 == Some(true)
            || immediate_blend_avx2 == Some(true)
            || immediate_permute_avx2 == Some(true)
            || chunk_extract_avx2 == Some(true)
            || mov_mask_stack_avx2 == Some(true)
            || variable_blend_avx2 == Some(true)
            || variable_permute_avx2 == Some(true)
            || alignr_avx2 == Some(true)
            || cross_lane_128_avx2 == Some(true)
            || vex_packed_extend_avx2 == Some(true)
            || vex_register_broadcast
            || vex_lane_shuffle_avx2 == Some(true);
        requirements.needs_avx_vnni_int8 |= vex_integer_dot_ext_int16 == Some(false);
        requirements.needs_avx_vnni_int16 |= vex_integer_dot_ext_int16 == Some(true);
        requirements.needs_fma |= is_fma3;
        requirements.needs_f16c |= vex_fp16_widen || vex_fp16_narrow;
        requirements.needs_vex_fp16_narrow |= vex_fp16_narrow;
        requirements.needs_fma4 |= is_fma4;
        requirements.needs_xop |= is_vpermil2;
        requirements.needs_aes |= legacy_aes;
        requirements.needs_sha |= legacy_sha;
        // Assume the full-width K0-K7 helper boundary while accumulating
        // replay families. A set containing only AVX-YMM16-safe spans is
        // relaxed after the scan.
        requirements.needs_avx512bw = true;
        requirements.needs_avx512vl |= span.needs_avx512vl;
        requirements.needs_avx512dq |= span.needs_avx512dq;
        requirements.needs_avx512fp16 |= span.needs_avx512fp16;
        requirements.needs_avx512cd |= span
            .instruction
            .evex_register_mask_broadcast_needs_vl()
            .is_some();
        requirements.needs_avx512vbmi2 |= span
            .instruction
            .evex_register_packed_funnel_shift_needs_vl()
            .is_some();
        requirements.needs_gfni |= legacy_gfni
            || span.instruction.evex_register_gfni_needs_vl().is_some()
            || vex_gfni_ymm.is_some();
        requirements.needs_avx512vp2intersect |= span
            .instruction
            .evex_register_vp2intersect_needs_vl()
            .is_some();
        requirements.needs_pclmulqdq |= legacy_pclmulqdq || vex_vpclmulqdq_ymm == Some(false);
        requirements.needs_vpclmulqdq |= span
            .instruction
            .evex_register_vpclmulqdq_needs_vl()
            .is_some()
            || vex_vpclmulqdq_ymm == Some(true);
    }
}
