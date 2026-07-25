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
    pub(crate) needs_fma: bool,
    pub(crate) needs_fma4: bool,
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

impl X86NativeReplayFeatureRequirements {
    /// Test replay-family CPUID requirements that are independent of the
    /// shared AVX-512 vector-state trampoline requirements.
    #[cfg(target_arch = "x86_64")]
    pub(crate) fn x86_host_supported(self) -> bool {
        (!self.needs_sse3 || std::is_x86_feature_detected!("sse3"))
            && (!self.needs_avx || std::is_x86_feature_detected!("avx"))
            && (!self.needs_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!self.needs_fma || std::is_x86_feature_detected!("fma"))
            && (!self.needs_fma4 || x86_host_has_fma4())
            && (!self.needs_gfni || std::is_x86_feature_detected!("gfni"))
            && (!self.needs_avx512vp2intersect
                || std::is_x86_feature_detected!("avx512vp2intersect"))
            && (!self.needs_pclmulqdq || std::is_x86_feature_detected!("pclmulqdq"))
            && (!self.needs_vpclmulqdq || std::is_x86_feature_detected!("vpclmulqdq"))
    }
}

/// Accumulate the host features required by exact x86 native-replay spans in
/// O(N) time and O(P) temporary space per block for N operations and P guest
/// instruction addresses.
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
            let immediate_blend_avx2 = span.instruction.vex_register_immediate_blend_needs_avx2();
            let variable_blend_avx2 = span.instruction.vex_register_variable_blend_needs_avx2();
            let variable_permute_avx2 = span.instruction.vex_register_variable_permute_needs_avx2();
            let alignr_avx2 = span.instruction.vex_register_alignr_needs_avx2();
            let cross_lane_128_avx2 = span.instruction.vex_register_cross_lane_128_needs_avx2();
            let scalar_insert_avx = span.instruction.is_vex_register_scalar_insert();
            let vex_vpclmulqdq_ymm = span.instruction.vex_register_vpclmulqdq_uses_ymm();
            let fp_horizontal_addsub_avx = span
                .instruction
                .legacy_vex_register_fp_horizontal_addsub_needs_avx();
            let widening_dword_multiply_avx2 = span
                .instruction
                .vex_register_widening_dword_multiply_needs_avx2();
            requirements.any = true;
            requirements.needs_sse3 |= fp_horizontal_addsub_avx == Some(false);
            all_spans_support_avx_ymm16 &= is_fma4
                || immediate_blend_avx2.is_some()
                || variable_blend_avx2.is_some()
                || variable_permute_avx2.is_some()
                || alignr_avx2.is_some()
                || cross_lane_128_avx2.is_some()
                || scalar_insert_avx
                || vex_vpclmulqdq_ymm.is_some();
            requirements.needs_avx |= span.instruction.is_vex_register_packed_string_compare()
                || span.instruction.is_vex_register_fma3()
                || is_fma4
                || immediate_blend_avx2.is_some()
                || variable_blend_avx2.is_some()
                || variable_permute_avx2.is_some()
                || alignr_avx2.is_some()
                || cross_lane_128_avx2.is_some()
                || scalar_insert_avx
                || vex_vpclmulqdq_ymm.is_some()
                || span.instruction.is_vex_register_fp_logic()
                || fp_horizontal_addsub_avx == Some(true)
                || widening_dword_multiply_avx2.is_some()
                || span
                    .instruction
                    .legacy_vex_register_fp_arithmetic_needs_avx()
                    == Some(true)
                || span.instruction.legacy_vex_register_fp_compare_needs_avx() == Some(true)
                || span.instruction.legacy_vex_register_fp_shuffle_needs_avx() == Some(true)
                || span
                    .instruction
                    .legacy_vex_register_high_low_move_needs_avx()
                    == Some(true)
                || span.instruction.legacy_vex_register_scalar_move_needs_avx() == Some(true)
                || span.instruction.legacy_vex_register_fp_sqrt_needs_avx() == Some(true);
            requirements.needs_avx2 |= widening_dword_multiply_avx2 == Some(true)
                || immediate_blend_avx2 == Some(true)
                || variable_blend_avx2 == Some(true)
                || variable_permute_avx2 == Some(true)
                || alignr_avx2 == Some(true)
                || cross_lane_128_avx2 == Some(true);
            requirements.needs_fma |= span.instruction.is_vex_register_fma3();
            requirements.needs_fma4 |= is_fma4;
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
            requirements.needs_gfni |= span.instruction.evex_register_gfni_needs_vl().is_some();
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
    }
    requirements.all_spans_support_avx_ymm16 = requirements.any && all_spans_support_avx_ymm16;
    if requirements.all_spans_support_avx_ymm16 {
        // These replay families address only YMM0-YMM15 and no opmask state.
        // Their dedicated state bridge requires AVX but no AVX-512 instruction.
        requirements.needs_avx512bw = false;
    }
    requirements
}

#[cfg(test)]
mod tests {
    use super::cpuid_enumerates_fma4;

    #[test]
    fn fma4_cpuid_bit_requires_the_extended_feature_leaf() {
        assert!(!cpuid_enumerates_fma4(0x8000_0000, 1 << 16));
        assert!(!cpuid_enumerates_fma4(0x8000_0001, 0));
        assert!(cpuid_enumerates_fma4(0x8000_0001, 1 << 16));
        assert!(cpuid_enumerates_fma4(u32::MAX, u32::MAX));
    }
}
