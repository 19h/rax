//! Feature requirements contributed by exact x86 native-replay spans.

/// Host features accumulated from byte-validated replay spans in executable
/// blocks. The surrounding vector trampoline separately accumulates features
/// required by directly lowered SMIR operations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct X86NativeReplayFeatureRequirements {
    pub(crate) any: bool,
    /// Every admitted replay span can use the AVX YMM0-YMM15 state bridge
    /// instead of the AVX-512 ZMM/K bridge. The caller separately excludes
    /// directly lowered vector operations.
    pub(crate) all_spans_support_avx_ymm16: bool,
    pub(crate) needs_sse3: bool,
    pub(crate) needs_avx: bool,
    pub(crate) needs_avx2: bool,
    pub(crate) needs_avx_vnni: bool,
    pub(crate) needs_avx_ifma: bool,
    pub(crate) needs_avx_ne_convert: bool,
    pub(crate) needs_avx_vnni_int8: bool,
    pub(crate) needs_avx_vnni_int16: bool,
    pub(crate) needs_f16c: bool,
    pub(crate) needs_vex_fp16_narrow: bool,
    pub(crate) needs_vex_unaligned_packed_fp_move: bool,
    pub(crate) needs_fma: bool,
    pub(crate) needs_fma4: bool,
    pub(crate) needs_xop: bool,
    pub(crate) needs_sm3: bool,
    pub(crate) needs_sm4: bool,
    pub(crate) needs_avx512bw: bool,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
    pub(crate) needs_avx512fp16: bool,
    pub(crate) needs_avx512cd: bool,
    pub(crate) needs_avx512vbmi: bool,
    pub(crate) needs_avx512vbmi2: bool,
    pub(crate) needs_gfni: bool,
    pub(crate) needs_avx512vp2intersect: bool,
    pub(crate) needs_aes: bool,
    pub(crate) needs_vaes: bool,
    pub(crate) needs_pclmulqdq: bool,
    pub(crate) needs_vpclmulqdq: bool,
}

#[path = "vector_replay_feature_probes.rs"]
mod feature_probes;

pub(crate) use feature_probes::*;
#[cfg(target_arch = "x86_64")]
fn x86_host_supports_vex_unaligned_packed_fp_move() -> bool {
    // Rosetta currently enumerates AVX but raises #UD for valid register-only
    // VEX VMOVUPS/VMOVUPD encodings. Keep this replay family at the SMIR
    // interpreter frontier in translated x86-64 processes.
    #[cfg(target_os = "macos")]
    if super::super::running_under_rosetta() {
        return false;
    }
    true
}

#[cfg(target_arch = "x86_64")]
fn x86_host_supports_vex_fp16_narrow() -> bool {
    // Intel specifies that VCVTPS2PH retains FP16 denormal results regardless
    // of MXCSR.FTZ. Rosetta currently flushes those results when FTZ is set,
    // so translated x86-64 processes must remain at the SMIR interpreter
    // frontier for this replay family.
    #[cfg(target_os = "macos")]
    if super::super::running_under_rosetta() {
        return false;
    }
    true
}

impl X86NativeReplayFeatureRequirements {
    /// Test replay-family CPUID requirements that are independent of the
    /// shared AVX-512 vector-state trampoline requirements.
    #[cfg(target_arch = "x86_64")]
    pub(crate) fn x86_host_supported(self) -> bool {
        (!self.needs_sse3 || std::is_x86_feature_detected!("sse3"))
            && (!self.needs_avx || std::is_x86_feature_detected!("avx"))
            && (!self.needs_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!self.needs_avx_vnni || x86_host_has_avx_vnni())
            && (!self.needs_avx_ifma || x86_host_has_avx_ifma())
            && (!self.needs_avx_ne_convert || x86_host_has_avx_ne_convert())
            && (!self.needs_avx_vnni_int8 || x86_host_has_avx_vnni_int8())
            && (!self.needs_avx_vnni_int16 || x86_host_has_avx_vnni_int16())
            && (!self.needs_f16c || std::is_x86_feature_detected!("f16c"))
            && (!self.needs_vex_fp16_narrow || x86_host_supports_vex_fp16_narrow())
            && (!self.needs_vex_unaligned_packed_fp_move
                || x86_host_supports_vex_unaligned_packed_fp_move())
            && (!self.needs_fma || std::is_x86_feature_detected!("fma"))
            && (!self.needs_fma4 || x86_host_has_fma4())
            && (!self.needs_xop || x86_host_has_xop())
            && (!self.needs_sm3 || std::is_x86_feature_detected!("sm3"))
            && (!self.needs_sm4 || std::is_x86_feature_detected!("sm4"))
            && (!self.needs_avx512vbmi || std::is_x86_feature_detected!("avx512vbmi"))
            && (!self.needs_avx512vbmi2 || std::is_x86_feature_detected!("avx512vbmi2"))
            && (!self.needs_gfni || std::is_x86_feature_detected!("gfni"))
            && (!self.needs_avx512vp2intersect
                || std::is_x86_feature_detected!("avx512vp2intersect"))
            && (!self.needs_aes || std::is_x86_feature_detected!("aes"))
            && (!self.needs_vaes || std::is_x86_feature_detected!("vaes"))
            && (!self.needs_pclmulqdq || std::is_x86_feature_detected!("pclmulqdq"))
            && (!self.needs_vpclmulqdq || std::is_x86_feature_detected!("vpclmulqdq"))
    }
}

/// Accumulate the host features required by exact x86 native-replay spans and
/// helper-backed x86 memory-source sequences in O(N) time and O(P + V)
/// temporary space per block for N operations, P guest instruction addresses,
/// and V virtual registers.
pub(crate) fn x86_native_replay_feature_requirements(
    func: &crate::smir::ir::SmirFunction,
    excluded: &std::collections::HashMap<crate::smir::ir::types::BlockId, u64>,
) -> X86NativeReplayFeatureRequirements {
    let mut requirements = X86NativeReplayFeatureRequirements::default();
    let mut all_spans_support_avx_ymm16 = true;
    for block in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
    {
        for span in crate::smir::ir::x86_native_replay_spans(block, &func.x86_instruction_bytes)
            .into_values()
        {
            let is_fma4 = span.instruction.is_vex_register_fma4();
            let is_vpermil2 = span.instruction.is_vex_register_vpermil2();
            let vex_fp_dot_product_ymm = span.instruction.vex_register_fp_dot_product_uses_ymm();
            let vex_integer_dot = span.instruction.vex_register_integer_dot_fields().is_some();
            let vex_ifma52 = span.instruction.vex_register_ifma52_fields().is_some();
            let vex_ne_convert = span.instruction.vex_register_ne_convert_fields().is_some();
            let vex_integer_dot_ext_int16 =
                span.instruction.vex_register_integer_dot_ext_is_int16();
            let immediate_blend_avx2 = span.instruction.vex_register_immediate_blend_needs_avx2();
            let immediate_permute_avx2 =
                span.instruction.vex_register_immediate_permute_needs_avx2();
            let chunk_extract_avx2 = span.instruction.vex_register_chunk_extract_needs_avx2();
            let scalar_extract_avx = span.instruction.is_vex_register_scalar_extract();
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
            let widening_dword_multiply_avx2 = span
                .instruction
                .vex_register_widening_dword_multiply_needs_avx2();
            let vex_packed_extend_avx2 = span.instruction.vex_register_packed_extend_needs_avx2();
            let vex_aligned_packed_fp_move =
                span.instruction.is_vex_register_aligned_packed_fp_move();
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
            requirements.any = true;
            requirements.needs_sse3 |= fp_horizontal_addsub_avx == Some(false);
            requirements.needs_vex_unaligned_packed_fp_move |= vex_unaligned_packed_fp_move;
            all_spans_support_avx_ymm16 &= is_fma4
                || is_vpermil2
                || vex_fp_dot_product_ymm.is_some()
                || vex_integer_dot
                || vex_ifma52
                || vex_ne_convert
                || vex_integer_dot_ext_int16.is_some()
                || fp_estimate_avx.is_some()
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
            requirements.needs_avx |= vex_packed_string
                || span.instruction.is_vex_register_fma3()
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
                || span.instruction.is_vex_register_fp_logic()
                || fp_horizontal_addsub_avx == Some(true)
                || widening_dword_multiply_avx2.is_some()
                || vex_packed_extend_avx2.is_some()
                || span
                    .instruction
                    .legacy_vex_register_fp_arithmetic_needs_avx()
                    == Some(true)
                || span.instruction.legacy_vex_register_fp_compare_needs_avx() == Some(true)
                || fp_estimate_avx == Some(true)
                || span.instruction.legacy_vex_register_fp_shuffle_needs_avx() == Some(true)
                || span
                    .instruction
                    .legacy_vex_register_high_low_move_needs_avx()
                    == Some(true)
                || span.instruction.legacy_vex_register_scalar_move_needs_avx() == Some(true)
                || span.instruction.legacy_vex_register_fp_sqrt_needs_avx() == Some(true)
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
            requirements.needs_fma |= span.instruction.is_vex_register_fma3();
            requirements.needs_f16c |= vex_fp16_widen || vex_fp16_narrow;
            requirements.needs_vex_fp16_narrow |= vex_fp16_narrow;
            requirements.needs_fma4 |= is_fma4;
            requirements.needs_xop |= is_vpermil2;
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
            requirements.needs_gfni |=
                span.instruction.evex_register_gfni_needs_vl().is_some() || vex_gfni_ymm.is_some();
            requirements.needs_avx512vp2intersect |= span
                .instruction
                .evex_register_vp2intersect_needs_vl()
                .is_some();
            requirements.needs_pclmulqdq |= vex_vpclmulqdq_ymm == Some(false);
            requirements.needs_vpclmulqdq |= span
                .instruction
                .evex_register_vpclmulqdq_needs_vl()
                .is_some()
                || vex_vpclmulqdq_ymm == Some(true);
        }

        let mut virtual_definitions = std::collections::HashMap::new();
        let mut virtual_uses = std::collections::HashMap::new();
        for op in &block.ops {
            for register in op.kind.dests() {
                if matches!(register, crate::smir::ir::types::VReg::Virtual(_)) {
                    *virtual_definitions.entry(register).or_insert(0usize) += 1;
                }
            }
            for register in op.kind.source_vregs() {
                if matches!(register, crate::smir::ir::types::VReg::Virtual(_)) {
                    *virtual_uses.entry(register).or_insert(0usize) += 1;
                }
            }
        }
        let mut index = 0usize;
        while index < block.ops.len() {
            if let Some(sequence) = super::x86_jit_evex_bf16_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // the final directly lowered operation contributes BF16.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_fp_interleave_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // VUNPCKL/HPS/PD require AVX-512F. The full-width native
                // vector/opmask bridge additionally requires AVX-512BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_fp_shuffle_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // VSHUFPS/PD require AVX-512F. The full-width native
                // vector/opmask bridge additionally requires AVX-512BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_chunk_shuffle_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // All four chunk shuffles require AVX-512F. The full-width
                // native vector/opmask bridge additionally requires BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_dbpsadbw_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge and VDBPSADBW itself
                // require AVX-512BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_gfni_multiply_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // VGF2P8MULB requires GFNI and AVX-512F/VL. The full-width
                // vector/opmask state bridge additionally requires BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_gfni = true;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_bw_shuffle_madd_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // VPSHUFB, VPMADDUBSW, and VPMADDWD require AVX-512BW.
                // The full-width vector/opmask bridge has the same minimum.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_integer_arithmetic_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW.
                // Byte/word operations also require BW architecturally;
                // dword/quadword operations require only AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512dq |= sequence.encoding.needs_avx512dq;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_integer_pack_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // Every EVEX saturating-pack form requires AVX-512BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_integer_interleave_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width native vector-state bridge uses AVX-512BW.
                // Byte/word interleaves also require BW architecturally;
                // doubleword/quadword forms themselves require AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_packed_integer_mask_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW.
                // Byte/word compare/test forms also require BW
                // architecturally; dword/quadword forms require only F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_integer_minmax_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW.
                // Byte/word operations also require BW architecturally;
                // dword/quadword operations require only AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_logic_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width native vector-state bridge uses AVX-512BW
                // even when the logical instruction itself requires only F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512dq |= sequence.encoding.needs_avx512dq;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_masked_logic_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width native vector-state bridge uses AVX-512BW
                // even when the logical instruction itself requires only F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512dq |= sequence.encoding.needs_avx512dq;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_multishift_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // VPMULTISHIFTQB additionally requires AVX-512VBMI.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512vbmi = true;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_full_permute_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW.
                // VPERMB additionally requires AVX-512VBMI; every other
                // covered operation requires AVX-512F or AVX-512BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512vbmi |= sequence.encoding.needs_avx512vbmi;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_two_table_permute_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW.
                // Byte permutations additionally require AVX-512VBMI;
                // word forms require BW, and D/Q/PS/PD forms require F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512vbmi |= sequence.encoding.needs_avx512vbmi;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_variable_permute_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width native vector-state bridge uses AVX-512BW;
                // VPERMILPS/PD themselves require AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_broadcast_logic_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width native vector-state bridge uses AVX-512BW
                // even when the logical instruction itself requires only F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512dq |= sequence.encoding.needs_avx512dq;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_broadcast_interleave_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width native vector-state bridge uses AVX-512BW
                // even though VPUNPCK*DQ/QDQ itself requires AVX-512F only.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) =
                super::x86_jit_evex_packed_fp16_arithmetic_memory_sequence(
                    block,
                    index,
                    true,
                    &func.x86_instruction_bytes,
                    &virtual_definitions,
                    &virtual_uses,
                )
            {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // the arithmetic operation itself requires AVX-512-FP16.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512fp16 = true;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_packed_fp_arithmetic_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // packed binary32/binary64 arithmetic itself requires F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_packed_fp_compare_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // packed comparison itself requires F or FP16.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512fp16 |= sequence.encoding.needs_avx512fp16;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_gfni_affine_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // affine GFNI additionally requires GFNI and F/VL.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_gfni = true;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_fixup_imm_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // VFIXUPIMM itself requires AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_range_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // every VRANGE operation additionally requires AVX-512DQ.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512dq = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_scale_f_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW.
                // Binary16 VSCALEF additionally requires AVX-512FP16;
                // binary32/binary64 use the baseline AVX-512F gate.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512fp16 |=
                    sequence.encoding.elem == crate::smir::ir::types::VecElementType::F16;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_packed_funnel_shift_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // every packed funnel-shift operation requires VBMI2.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512vbmi2 = true;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_packed_rotate_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // packed doubleword/quadword rotates themselves require F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_packed_variable_shift_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // doubleword/quadword shifts themselves require AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_ternary_logic_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // VPTERNLOGD/Q themselves require AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_shared_count_shift_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // doubleword/quadword shifts themselves require AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_alignr_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge and VPALIGNR itself
                // require AVX-512BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_vector_align_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // VALIGND/Q itself requires AVX-512F.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_mask_blend_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                // The full-width vector/opmask bridge requires AVX-512BW;
                // dword/qword/float blends require F, while byte/word blends
                // already require BW.
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_scalar_fma3_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx512bw = true;
                requirements.needs_avx512fp16 |=
                    sequence.encoding.elem == crate::smir::ir::types::VecElementType::F16;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_evex_packed_fma3_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx512bw = true;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_avx512fp16 |=
                    sequence.encoding.elem == crate::smir::ir::types::VecElementType::F16;
                all_spans_support_avx_ymm16 = false;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_fma4_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_fma4 = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_vpermil2_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_xop = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_sm3_sm4_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_sm3 |= sequence.encoding.kind.needs_sm3();
                requirements.needs_sm4 |= sequence.encoding.kind.needs_sm4();
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_packed_string_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_masked_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                // The fused implementation emits AVX VMOVDQU only; integer
                // guest forms therefore do not require host AVX2.
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vpclmulqdq_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx512bw |= !sequence.encoding.supports_avx_ymm16;
                requirements.needs_avx512vl |= sequence.encoding.needs_avx512vl;
                requirements.needs_pclmulqdq |= sequence.encoding.needs_pclmulqdq;
                requirements.needs_vpclmulqdq |= sequence.encoding.needs_vpclmulqdq;
                all_spans_support_avx_ymm16 &= sequence.encoding.supports_avx_ymm16;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_gfni_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_gfni = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_duplicate_move_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_estimate_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_fp_flag_compare_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_sqrt_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_packed_convert_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_f16c |= sequence.encoding.needs_f16c();
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_ne_convert_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx_ne_convert = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_fp16_narrow_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_f16c = true;
                requirements.needs_vex_fp16_narrow = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_round_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_scalar_convert_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_extract_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.needs_avx2();
                index += sequence.consumed();
            } else if let Some(consumed) = super::x86_jit_vex_scalar_move_memory_sequence_len(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += consumed;
            } else if let Some(sequence) = super::x86_jit_vex_fp_compare_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_fp_dot_product_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_mpsadbw_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.width == crate::smir::ir::types::VecWidth::V256;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_scalar_insert_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_alignr_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.width == crate::smir::ir::types::VecWidth::V256;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_fp_shuffle_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_immediate_blend_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.encoding.needs_avx2;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_immediate_permute_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.encoding.needs_avx2;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_cross_lane_128_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.encoding.needs_avx2;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_variable_blend_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.encoding.needs_avx2;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_variable_permute_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.encoding.needs_avx2;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_lane_shuffle_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.width == crate::smir::ir::types::VecWidth::V256;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_aes_memory_sequence(
                block,
                index,
                true,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx512bw |= !sequence.supports_avx_ymm16;
                requirements.needs_avx512vl |= sequence.needs_avx512vl;
                requirements.needs_aes |= sequence.needs_aes;
                requirements.needs_vaes |= sequence.needs_vaes;
                all_spans_support_avx_ymm16 &= sequence.supports_avx_ymm16;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_movntdqa_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                // The helper performs the memory transfer and ignores the
                // cache-placement hint; only the AVX YMM16 state bridge is
                // executed on the host, including for the guest AVX2 form.
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_phminposuw_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_packed_abs_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.width == crate::smir::ir::types::VecWidth::V256;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_broadcast_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.needs_avx2;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_packed_extend_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.width == crate::smir::ir::types::VecWidth::V256;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_ptest_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                index += sequence.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_integer_dot_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx_vnni = true;
                index += sequence.binary.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_ifma52_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx_ifma = true;
                index += sequence.binary.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_integer_dot_ext_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx_vnni_int8 |= !sequence.int16;
                requirements.needs_avx_vnni_int16 |= sequence.int16;
                index += sequence.binary.consumed;
            } else if let Some(sequence) = super::x86_jit_vex_binary_memory_sequence(
                block,
                index,
                true,
                &func.x86_instruction_bytes,
                &virtual_definitions,
                &virtual_uses,
            ) {
                requirements.any = true;
                requirements.needs_avx = true;
                requirements.needs_avx2 |= sequence.needs_avx2;
                requirements.needs_fma |= sequence.needs_fma;
                index += sequence.consumed;
            } else {
                index += 1;
            }
        }
    }
    requirements.all_spans_support_avx_ymm16 = requirements.any && all_spans_support_avx_ymm16;
    if requirements.all_spans_support_avx_ymm16 {
        // These replay families address only YMM0-YMM15 and no opmask state.
        // Their dedicated state bridge itself requires AVX even when every
        // replayed source instruction is legacy SSE, but no AVX-512 feature.
        requirements.needs_avx = true;
        requirements.needs_avx512bw = false;
    }
    requirements
}

#[cfg(test)]
#[path = "vector_replay_features_tests.rs"]
mod tests;
