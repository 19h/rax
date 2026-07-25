//! Feature requirements contributed by exact x86 native-replay spans.

/// Host features accumulated from byte-validated replay spans in executable
/// blocks. The surrounding vector trampoline separately accumulates features
/// required by directly lowered SMIR operations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct X86NativeReplayFeatureRequirements {
    pub(crate) any: bool,
    pub(crate) needs_avx: bool,
    pub(crate) needs_fma: bool,
    pub(crate) needs_avx512bw: bool,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
    pub(crate) needs_avx512fp16: bool,
    pub(crate) needs_avx512cd: bool,
    pub(crate) needs_gfni: bool,
    pub(crate) needs_avx512vp2intersect: bool,
    pub(crate) needs_vpclmulqdq: bool,
}

impl X86NativeReplayFeatureRequirements {
    /// Test replay-family CPUID requirements that are independent of the
    /// shared AVX-512 vector-state trampoline requirements.
    #[cfg(target_arch = "x86_64")]
    pub(crate) fn x86_host_supported(self) -> bool {
        (!self.needs_avx || std::is_x86_feature_detected!("avx"))
            && (!self.needs_fma || std::is_x86_feature_detected!("fma"))
            && (!self.needs_gfni || std::is_x86_feature_detected!("gfni"))
            && (!self.needs_avx512vp2intersect
                || std::is_x86_feature_detected!("avx512vp2intersect"))
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
    for block in func
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
    {
        for span in crate::smir::ir::x86_native_replay_spans(block, &func.x86_instruction_bytes)
            .into_values()
        {
            requirements.any = true;
            requirements.needs_avx |= span.instruction.is_vex_register_packed_string_compare()
                || span.instruction.is_vex_register_fma3()
                || span.instruction.is_vex_register_fp_logic()
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
            requirements.needs_fma |= span.instruction.is_vex_register_fma3();
            // Replay spans use the full-width K0-K7 helper boundary. KMOVQ is
            // an AVX-512BW instruction, independently of the replayed opcode's
            // own CPUID feature set.
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
            requirements.needs_vpclmulqdq |= span
                .instruction
                .evex_register_vpclmulqdq_needs_vl()
                .is_some();
        }
    }
    requirements
}
