//! Architectural CPUID probes used by native replay feature admission.

pub(crate) fn cpuid_enumerates_fma4(max_extended_leaf: u32, extended_features_ecx: u32) -> bool {
    max_extended_leaf >= 0x8000_0001 && extended_features_ecx & (1 << 16) != 0
}

pub(crate) fn cpuid_enumerates_xop(max_extended_leaf: u32, extended_features_ecx: u32) -> bool {
    max_extended_leaf >= 0x8000_0001 && extended_features_ecx & (1 << 11) != 0
}

pub(crate) fn cpuid_enumerates_leaf7_subleaf0_edx_feature(
    max_basic_leaf: u32,
    subleaf0_edx: u32,
    feature_mask: u32,
) -> bool {
    max_basic_leaf >= 7 && subleaf0_edx & feature_mask != 0
}

pub(crate) fn cpuid_enumerates_leaf7_subleaf1_edx_feature(
    max_basic_leaf: u32,
    max_structured_subleaf: u32,
    subleaf1_edx: u32,
    feature_mask: u32,
) -> bool {
    max_basic_leaf >= 7 && max_structured_subleaf >= 1 && subleaf1_edx & feature_mask != 0
}

pub(crate) fn cpuid_enumerates_leaf7_subleaf1_eax_feature(
    max_basic_leaf: u32,
    max_structured_subleaf: u32,
    subleaf1_eax: u32,
    feature_mask: u32,
) -> bool {
    max_basic_leaf >= 7 && max_structured_subleaf >= 1 && subleaf1_eax & feature_mask != 0
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

/// Intel SDM Volume 2 defines AVX512_4FMAPS at CPUID.07H.00H:EDX[3].
/// Stable Rust does not expose this Xeon Phi feature through
/// `is_x86_feature_detected!`, so query the architectural bit directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx5124fmaps() -> bool {
    // SAFETY: CPUID is architecturally available in x86-64 mode. The maximum
    // basic leaf is checked before querying CPUID.07H.00H.
    unsafe {
        let max_basic_leaf = std::arch::x86_64::__cpuid(0).eax;
        let subleaf0_edx = if max_basic_leaf >= 7 {
            std::arch::x86_64::__cpuid_count(7, 0).edx
        } else {
            0
        };
        cpuid_enumerates_leaf7_subleaf0_edx_feature(max_basic_leaf, subleaf0_edx, 1 << 3)
    }
}

/// Intel SDM Volume 1 defines AVX_VNNI at CPUID.07H.01H:EAX[4]. Stable Rust
/// does not expose an `is_x86_feature_detected!` probe for this feature, so
/// query the structured extended-feature leaf directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx_vnni() -> bool {
    x86_host_has_leaf7_subleaf1_eax_feature(1 << 4)
}

/// Intel SDM Volume 1 defines AVX_IFMA at CPUID.07H.01H:EAX[23]. Stable Rust
/// exposes the target feature for code generation, but use the architectural
/// bit directly so replay admission follows the same explicit leaf-bound
/// validation as the other VEX-only feature classes.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx_ifma() -> bool {
    x86_host_has_leaf7_subleaf1_eax_feature(1 << 23)
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_leaf7_subleaf1_eax_feature(feature_mask: u32) -> bool {
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
        let subleaf1_eax = if max_basic_leaf >= 7 && max_structured_subleaf >= 1 {
            std::arch::x86_64::__cpuid_count(7, 1).eax
        } else {
            0
        };
        cpuid_enumerates_leaf7_subleaf1_eax_feature(
            max_basic_leaf,
            max_structured_subleaf,
            subleaf1_eax,
            feature_mask,
        )
    }
}

/// Intel SDM Volume 1 defines AVX_VNNI_INT8 at CPUID.07H.01H:EDX[4].
/// Stable Rust does not expose an `is_x86_feature_detected!` probe for this
/// feature, so query the structured extended-feature leaf directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx_vnni_int8() -> bool {
    x86_host_has_leaf7_subleaf1_edx_feature(1 << 4)
}

/// Intel SDM Volume 1 defines AVX_NE_CONVERT at
/// CPUID.07H.01H:EDX[5]. Stable Rust does not expose an
/// `is_x86_feature_detected!` probe for this feature, so query the structured
/// extended-feature leaf directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx_ne_convert() -> bool {
    x86_host_has_leaf7_subleaf1_edx_feature(1 << 5)
}

/// Intel SDM Volume 1 defines AVX_VNNI_INT16 at CPUID.07H.01H:EDX[10].
/// Stable Rust does not expose an `is_x86_feature_detected!` probe for this
/// feature, so query the structured extended-feature leaf directly.
#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_avx_vnni_int16() -> bool {
    x86_host_has_leaf7_subleaf1_edx_feature(1 << 10)
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_host_has_leaf7_subleaf1_edx_feature(feature_mask: u32) -> bool {
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
