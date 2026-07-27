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
    pub(crate) needs_avx_vnni_int8: bool,
    pub(crate) needs_avx_vnni_int16: bool,
    pub(crate) needs_f16c: bool,
    pub(crate) needs_vex_fp16_narrow: bool,
    pub(crate) needs_vex_unaligned_packed_fp_move: bool,
    pub(crate) needs_fma: bool,
    pub(crate) needs_fma4: bool,
    pub(crate) needs_xop: bool,
    pub(crate) needs_avx512bw: bool,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
    pub(crate) needs_avx512fp16: bool,
    pub(crate) needs_avx512cd: bool,
    pub(crate) needs_gfni: bool,
    pub(crate) needs_avx512vp2intersect: bool,
    pub(crate) needs_pclmulqdq: bool,
    pub(crate) needs_vpclmulqdq: bool,
}

fn cpuid_enumerates_fma4(max_extended_leaf: u32, extended_features_ecx: u32) -> bool {
    max_extended_leaf >= 0x8000_0001 && extended_features_ecx & (1 << 16) != 0
}

fn cpuid_enumerates_xop(max_extended_leaf: u32, extended_features_ecx: u32) -> bool {
    max_extended_leaf >= 0x8000_0001 && extended_features_ecx & (1 << 11) != 0
}

fn cpuid_enumerates_leaf7_subleaf1_edx_feature(
    max_basic_leaf: u32,
    max_structured_subleaf: u32,
    subleaf1_edx: u32,
    feature_mask: u32,
) -> bool {
    max_basic_leaf >= 7 && max_structured_subleaf >= 1 && subleaf1_edx & feature_mask != 0
}

/// AMD APM Volume 3 defines FMA4 at CPUID Fn8000_0001_ECX[16]. Stable Rust
/// exposes `fma4` for code generation but not through
/// `is_x86_feature_detected!`, so query the architectural bit directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_fma4() -> bool {
    // SAFETY: CPUID is architecturally available in x86-64 mode. The maximum
    // extended leaf is checked before querying Fn8000_0001.
    unsafe {
        let max_extended_leaf = std::arch::x86_64::__cpuid(0x8000_0000).eax;
        let extended_features_ecx = if max_extended_leaf >= 0x8000_0001 {
            std::arch::x86_64::__cpuid(0x8000_0001).ecx
        } else {
            0
        };
        cpuid_enumerates_fma4(max_extended_leaf, extended_features_ecx)
    }
}

/// AMD APM Volume 3 defines XOP at CPUID Fn8000_0001_ECX[11]. Stable Rust
/// does not expose an `is_x86_feature_detected!("xop")` probe, so query the
/// architectural bit directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_xop() -> bool {
    // SAFETY: CPUID is architecturally available in x86-64 mode. The maximum
    // extended leaf is checked before querying Fn8000_0001.
    unsafe {
        let max_extended_leaf = std::arch::x86_64::__cpuid(0x8000_0000).eax;
        let extended_features_ecx = if max_extended_leaf >= 0x8000_0001 {
            std::arch::x86_64::__cpuid(0x8000_0001).ecx
        } else {
            0
        };
        cpuid_enumerates_xop(max_extended_leaf, extended_features_ecx)
    }
}

/// Intel SDM Volume 1 defines AVX_VNNI_INT8 at CPUID.07H.01H:EDX[4].
/// Stable Rust does not expose an `is_x86_feature_detected!` probe for this
/// feature, so query the structured extended-feature leaf directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx_vnni_int8() -> bool {
    x86_host_has_leaf7_subleaf1_edx_feature(1 << 4)
}

/// Intel SDM Volume 1 defines AVX_VNNI_INT16 at CPUID.07H.01H:EDX[10].
/// Stable Rust does not expose an `is_x86_feature_detected!` probe for this
/// feature, so query the structured extended-feature leaf directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx_vnni_int16() -> bool {
    x86_host_has_leaf7_subleaf1_edx_feature(1 << 10)
}

#[cfg(target_arch = "x86_64")]
fn x86_host_has_leaf7_subleaf1_edx_feature(feature_mask: u32) -> bool {
    // SAFETY: CPUID is architecturally available in x86-64 mode. The maximum
    // basic leaf and CPUID.07H.00H:EAX maximum subleaf are checked before
    // querying CPUID.07H.01H.
    unsafe {
        let max_basic_leaf = std::arch::x86_64::__cpuid(0).eax;
        let max_structured_subleaf = if max_basic_leaf >= 7 {
            std::arch::x86_64::__cpuid_count(7, 0).eax
        } else {
            0
        };
        let subleaf1_edx = if max_basic_leaf >= 7 && max_structured_subleaf >= 1 {
            std::arch::x86_64::__cpuid_count(7, 1).edx
        } else {
            0
        };
        cpuid_enumerates_leaf7_subleaf1_edx_feature(
            max_basic_leaf,
            max_structured_subleaf,
            subleaf1_edx,
            feature_mask,
        )
    }
}

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
            && (!self.needs_avx_vnni_int8 || x86_host_has_avx_vnni_int8())
            && (!self.needs_avx_vnni_int16 || x86_host_has_avx_vnni_int16())
            && (!self.needs_f16c || std::is_x86_feature_detected!("f16c"))
            && (!self.needs_vex_fp16_narrow || x86_host_supports_vex_fp16_narrow())
            && (!self.needs_vex_unaligned_packed_fp_move
                || x86_host_supports_vex_unaligned_packed_fp_move())
            && (!self.needs_fma || std::is_x86_feature_detected!("fma"))
            && (!self.needs_fma4 || x86_host_has_fma4())
            && (!self.needs_xop || x86_host_has_xop())
            && (!self.needs_gfni || std::is_x86_feature_detected!("gfni"))
            && (!self.needs_avx512vp2intersect
                || std::is_x86_feature_detected!("avx512vp2intersect"))
            && (!self.needs_pclmulqdq || std::is_x86_feature_detected!("pclmulqdq"))
            && (!self.needs_vpclmulqdq || std::is_x86_feature_detected!("vpclmulqdq"))
    }
}

/// Accumulate the host features required by exact x86 native-replay spans and
/// helper-backed VEX packed-logic memory-source sequences in O(N) time and
/// O(P + V) temporary space per block for N operations, P guest instruction
/// addresses, and V virtual registers.
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
            requirements.any = true;
            requirements.needs_sse3 |= fp_horizontal_addsub_avx == Some(false);
            requirements.needs_vex_unaligned_packed_fp_move |= vex_unaligned_packed_fp_move;
            all_spans_support_avx_ymm16 &= is_fma4
                || is_vpermil2
                || vex_fp_dot_product_ymm.is_some()
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
                || vex_zero;
            requirements.needs_avx |= span.instruction.is_vex_register_packed_string_compare()
                || span.instruction.is_vex_register_fma3()
                || is_fma4
                || is_vpermil2
                || vex_fp_dot_product_ymm.is_some()
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
            if let Some(sequence) = super::x86_jit_vex_binary_memory_sequence(
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
mod tests {
    use super::{
        cpuid_enumerates_fma4, cpuid_enumerates_leaf7_subleaf1_edx_feature, cpuid_enumerates_xop,
    };

    #[test]
    fn fma4_cpuid_bit_requires_the_extended_feature_leaf() {
        assert!(!cpuid_enumerates_fma4(0x8000_0000, 1 << 16));
        assert!(!cpuid_enumerates_fma4(0x8000_0001, 0));
        assert!(cpuid_enumerates_fma4(0x8000_0001, 1 << 16));
        assert!(cpuid_enumerates_fma4(u32::MAX, u32::MAX));
    }

    #[test]
    fn xop_cpuid_bit_requires_the_extended_feature_leaf() {
        assert!(!cpuid_enumerates_xop(0x8000_0000, 1 << 11));
        assert!(!cpuid_enumerates_xop(0x8000_0001, 0));
        assert!(cpuid_enumerates_xop(0x8000_0001, 1 << 11));
        assert!(cpuid_enumerates_xop(u32::MAX, u32::MAX));
    }

    #[test]
    fn avx_vnni_int8_cpuid_bit_requires_basic_leaf_7_and_subleaf_1() {
        let bit = 1 << 4;
        assert!(!cpuid_enumerates_leaf7_subleaf1_edx_feature(6, 1, bit, bit));
        assert!(!cpuid_enumerates_leaf7_subleaf1_edx_feature(7, 0, bit, bit));
        assert!(!cpuid_enumerates_leaf7_subleaf1_edx_feature(7, 1, 0, bit));
        assert!(cpuid_enumerates_leaf7_subleaf1_edx_feature(7, 1, bit, bit));
        assert!(cpuid_enumerates_leaf7_subleaf1_edx_feature(
            u32::MAX,
            u32::MAX,
            u32::MAX,
            bit
        ));
    }

    #[test]
    fn avx_vnni_int16_cpuid_bit_requires_basic_leaf_7_and_subleaf_1() {
        let bit = 1 << 10;
        assert!(!cpuid_enumerates_leaf7_subleaf1_edx_feature(6, 1, bit, bit));
        assert!(!cpuid_enumerates_leaf7_subleaf1_edx_feature(7, 0, bit, bit));
        assert!(!cpuid_enumerates_leaf7_subleaf1_edx_feature(7, 1, 0, bit));
        assert!(cpuid_enumerates_leaf7_subleaf1_edx_feature(7, 1, bit, bit));
        assert!(!cpuid_enumerates_leaf7_subleaf1_edx_feature(
            7,
            1,
            1 << 4,
            bit
        ));
    }
}
