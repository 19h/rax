//! Single-pass aggregation of all exact x86 native source-replay classifiers.

use std::collections::HashMap;

use super::{X86InstructionBytes, X86NativeReplaySpan, x86_native_replay_spans_where};
use crate::smir::ir::SmirBlock;
use crate::smir::ir::types::{BlockId, GuestAddr};

/// Identify every validated native x86 source-replay group in one O(N)-time,
/// O(P + V)-space block pass for N operations, P source instruction addresses,
/// and V virtual registers. Classifiers are intentionally disjoint and ordered
/// explicitly so adding a replay family does not add another scan of the SMIR
/// operation stream.
pub fn x86_native_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        if instruction.legacy_register_aes_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_blend_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_scalar_xmm_movq_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction
            .legacy_register_packed_fp_convert_replay()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_register_scalar_fp_convert_replay()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_register_scalar_extract_replay()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction.legacy_register_scalar_insert_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_lane_shuffle_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_alignr_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_gfni_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_round_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_dot_product_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_insertps_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_pclmulqdq_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_ptest_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_packed_extend_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.legacy_register_packed_shift_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction
            .legacy_register_widening_dword_multiply_replay()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_register_fp_flag_compare_replay()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction.legacy_register_sha_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction
            .legacy_mov_mask_stack_destination_replay()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction.legacy_movd_q_stack_replay().is_some() {
            return Some((false, false, false));
        }
        if instruction.is_legacy_high_byte_register_replay() {
            return Some((false, false, false));
        }
        if let Some(needs_vl) = instruction.evex_register_fp_arithmetic_needs_vl() {
            return Some((needs_vl, false, false));
        }
        if instruction
            .legacy_vex_register_fp_arithmetic_needs_avx()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_vex_register_fp_estimate_needs_avx()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_vex_register_fp_compare_needs_avx()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction.is_vex_register_fp_flag_compare() {
            return Some((false, false, false));
        }
        if instruction.vex_round_destination_index().is_some() {
            return Some((false, false, false));
        }
        if instruction
            .vex_scalar_fp_convert_destination_index()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .vex_scalar_fp_to_int_destination_index()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .vex_scalar_int_to_fp_destination_index()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_vex_register_fp_shuffle_needs_avx()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_vex_register_fp_horizontal_addsub_needs_avx()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_vex_register_high_low_move_needs_avx()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .vex_register_widening_dword_multiply_needs_avx2()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction
            .legacy_vex_register_scalar_move_needs_avx()
            .is_some()
        {
            return Some((false, false, false));
        }
        if instruction.is_vex_register_aligned_packed_fp_move() {
            return Some((false, false, false));
        }
        if instruction.is_vex_register_unaligned_packed_fp_move() {
            return Some((false, false, false));
        }
        if instruction.is_vex_register_packed_integer_move() {
            return Some((false, false, false));
        }
        if instruction.is_vex_register_scalar_vmovq() {
            return Some((false, false, false));
        }
        if instruction.vex_register_broadcast_element_bits().is_some() {
            return Some((false, false, false));
        }
        if instruction.vex_register_lane_shuffle_needs_avx2().is_some() {
            return Some((false, false, false));
        }
        if instruction.is_vex_register_fp16_widen() {
            return Some((false, false, false));
        }
        if instruction.is_vex_register_fp16_narrow() {
            return Some((false, false, false));
        }
        if let Some(requirements) = instruction.evex_register_logic_requirements() {
            return Some((requirements.0, requirements.1, false));
        }
        instruction
            .evex_register_integer_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
            .or_else(|| {
                instruction
                    .evex_register_shared_count_shift_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_immediate_count_shift_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_funnel_shift_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_rotate_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_fp16_embedded_control_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_fp16_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp16_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp16_arithmetic_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_minmax_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_multiply_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_interleave_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_pack_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_abs_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_average_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_test_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_compare_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_mask_blend_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_vector_to_mask_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_mask_to_vector_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_mask_broadcast_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_lane_shuffle_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_vector_align_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_bw_shuffle_madd_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_bw_immediate_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_chunk_shuffle_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_chunk_insert_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_chunk_extract_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| instruction.evex_register_fp_class_requirements())
            .or_else(|| {
                instruction
                    .evex_register_fp_compare_requirements()
                    .or_else(|| instruction.evex_register_fp16_flag_compare_requirements())
                    .or_else(|| instruction.evex_register_fp32_fp64_flag_compare_requirements())
                    .map(|(needs_vl, needs_fp16)| (needs_vl, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .evex_register_fp16_widen_requirements()
                    .map(|(needs_vl, needs_fp16)| (needs_vl, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .evex_register_fp16_narrow_requirements()
                    .map(|(needs_vl, needs_fp16)| (needs_vl, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_fp32_fp64_convert()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_fp32_fp64_convert_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_fp_sqrt_requirements()
                    .map(|(needs_vl, needs_fp16)| (needs_vl, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .legacy_vex_register_fp_sqrt_needs_avx()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_move_requires_fp16()
                    .map(|needs_fp16| (false, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_integer_move_requires_fp16()
                    .map(|needs_fp16| (false, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp_to_int_requires_fp16()
                    .map(|needs_fp16| (false, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp_convert_requires_fp16()
                    .map(|needs_fp16| (false, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_int_to_fp_requires_fp16()
                    .map(|needs_fp16| (false, false, needs_fp16))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_lane_transfer_requires_dq()
                    .map(|needs_dq| (false, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_high_low_move_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_gfni_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_gfni_uses_ymm()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_vpclmulqdq_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_vpclmulqdq_uses_ymm()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_vp2intersect_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_fp_shuffle_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_avx512f_permute_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_move_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_packed_extend_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_zeroes_all_register_bits()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_extend_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_broadcast_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_narrow_broadcast_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_gpr_broadcast_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_packed_string_compare()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_fma3()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_fma4()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_vpermil2()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_fp_dot_product_uses_ymm()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_integer_dot_fields()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_ifma52_fields()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_ne_convert_fields()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_integer_dot_ext_is_int16()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_immediate_blend_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_immediate_permute_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_chunk_extract_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_scalar_extract()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_mov_mask_stack_destination_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_ptest()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_variable_blend_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_variable_permute_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_alignr_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .vex_register_cross_lane_128_needs_avx2()
                    .map(|_| (false, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_scalar_insert()
                    .then_some((false, false, false))
            })
            .or_else(|| {
                instruction
                    .is_vex_register_fp_logic()
                    .then_some((false, false, false))
            })
    })
}

/// Identify valid register-only AVX VEX signed scalar floating-point-to-
/// integer replay groups in O(N) time and O(P) space for N operations and P
/// unique guest PCs. The rounding and truncating binary32/binary64 forms are
/// admitted; memory forms use their precise helper-backed path. Scalar
/// `VEX.L=1` sources are emitted only as deterministic `VEX.L=0`.
pub fn x86_vex_scalar_fp_to_int_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .vex_scalar_fp_to_int_destination_index()
            .map(|_| (false, false, false))
    })
}

/// Identify valid register-only AVX VEX signed integer-to-scalar-FP replay
/// groups in O(N) time and O(P) space for N operations and P unique guest PCs.
/// The binary32/binary64 destination forms are admitted; memory forms use their
/// precise helper-backed path. Scalar `VEX.L=1` sources are emitted only as
/// deterministic `VEX.L=0`.
pub fn x86_vex_scalar_int_to_fp_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .vex_scalar_int_to_fp_destination_index()
            .map(|_| (false, false, false))
    })
}

/// Identify valid register-only legacy SSE and AVX VEX reciprocal-estimate
/// replay groups in O(N) time and O(P) space for N operations and P unique
/// guest PCs. Memory forms remain at the precise SMIR interpreter boundary.
pub fn x86_legacy_vex_fp_estimate_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_native_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .legacy_vex_register_fp_estimate_needs_avx()
            .map(|_| (false, false, false))
    })
}
