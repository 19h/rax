//! Native-replay host feature requirement state and host probes.

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
    pub(crate) needs_ssse3: bool,
    pub(crate) needs_sse41: bool,
    pub(crate) needs_sse42: bool,
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
    pub(crate) needs_avx512bf16: bool,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
    pub(crate) needs_avx512er: bool,
    pub(crate) needs_avx512fp16: bool,
    pub(crate) needs_avx512cd: bool,
    pub(crate) needs_avx512bitalg: bool,
    pub(crate) needs_avx512vpopcntdq: bool,
    pub(crate) needs_avx512vbmi: bool,
    pub(crate) needs_avx512vbmi2: bool,
    pub(crate) needs_gfni: bool,
    pub(crate) needs_avx512vp2intersect: bool,
    pub(crate) needs_avx5124vnniw: bool,
    pub(crate) needs_avx5124fmaps: bool,
    /// At least one exact replay span observes no opmask bit above K[15].
    /// This permits the AVX512F KMOVW helper bridge when every other vector
    /// operation satisfies the same bound.
    pub(crate) has_k16_opmask_span: bool,
    pub(crate) needs_aes: bool,
    pub(crate) needs_sha: bool,
    pub(crate) needs_vaes: bool,
    pub(crate) needs_pclmulqdq: bool,
    pub(crate) needs_vpclmulqdq: bool,
}

#[cfg(target_arch = "x86_64")]
fn x86_host_supports_vex_unaligned_packed_fp_move() -> bool {
    // Rosetta currently enumerates AVX but raises #UD for valid register-only
    // VEX VMOVUPS/VMOVUPD encodings. Keep this replay family at the SMIR
    // interpreter frontier in translated x86-64 processes.
    #[cfg(target_os = "macos")]
    if super::super::super::running_under_rosetta() {
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
    if super::super::super::running_under_rosetta() {
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
            && (!self.needs_ssse3 || std::is_x86_feature_detected!("ssse3"))
            && (!self.needs_sse41 || std::is_x86_feature_detected!("sse4.1"))
            && (!self.needs_sse42 || std::is_x86_feature_detected!("sse4.2"))
            && (!self.needs_avx || std::is_x86_feature_detected!("avx"))
            && (!self.needs_avx2 || std::is_x86_feature_detected!("avx2"))
            && (!self.needs_avx_vnni || super::x86_host_has_avx_vnni())
            && (!self.needs_avx_ifma || super::x86_host_has_avx_ifma())
            && (!self.needs_avx_ne_convert || super::x86_host_has_avx_ne_convert())
            && (!self.needs_avx_vnni_int8 || super::x86_host_has_avx_vnni_int8())
            && (!self.needs_avx_vnni_int16 || super::x86_host_has_avx_vnni_int16())
            && (!self.needs_f16c || std::is_x86_feature_detected!("f16c"))
            && (!self.needs_vex_fp16_narrow || x86_host_supports_vex_fp16_narrow())
            && (!self.needs_vex_unaligned_packed_fp_move
                || x86_host_supports_vex_unaligned_packed_fp_move())
            && (!self.needs_fma || std::is_x86_feature_detected!("fma"))
            && (!self.needs_fma4 || super::x86_host_has_fma4())
            && (!self.needs_xop || super::x86_host_has_xop())
            && (!self.needs_sm3 || std::is_x86_feature_detected!("sm3"))
            && (!self.needs_sm4 || std::is_x86_feature_detected!("sm4"))
            && (!self.needs_avx512bitalg || std::is_x86_feature_detected!("avx512bitalg"))
            && (!self.needs_avx512vpopcntdq || std::is_x86_feature_detected!("avx512vpopcntdq"))
            && (!self.needs_avx512vbmi || std::is_x86_feature_detected!("avx512vbmi"))
            && (!self.needs_avx512vbmi2 || std::is_x86_feature_detected!("avx512vbmi2"))
            && (!self.needs_gfni || std::is_x86_feature_detected!("gfni"))
            && (!self.needs_avx512vp2intersect
                || std::is_x86_feature_detected!("avx512vp2intersect"))
            && (!self.needs_avx5124vnniw || super::x86_host_has_avx5124vnniw())
            && (!self.needs_avx5124fmaps || super::x86_host_has_avx5124fmaps())
            && (!self.needs_aes || std::is_x86_feature_detected!("aes"))
            && (!self.needs_sha || std::is_x86_feature_detected!("sha"))
            && (!self.needs_vaes || std::is_x86_feature_detected!("vaes"))
            && (!self.needs_pclmulqdq || std::is_x86_feature_detected!("pclmulqdq"))
            && (!self.needs_vpclmulqdq || std::is_x86_feature_detected!("vpclmulqdq"))
    }
}
