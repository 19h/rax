//! Unit tests for architectural CPUID feature probes.

use super::{
    cpuid_enumerates_fma4, cpuid_enumerates_leaf7_subleaf1_eax_feature,
    cpuid_enumerates_leaf7_subleaf1_edx_feature, cpuid_enumerates_xop,
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
fn avx_vnni_cpuid_bit_requires_basic_leaf_7_and_subleaf_1() {
    let bit = 1 << 4;
    assert!(!cpuid_enumerates_leaf7_subleaf1_eax_feature(6, 1, bit, bit));
    assert!(!cpuid_enumerates_leaf7_subleaf1_eax_feature(7, 0, bit, bit));
    assert!(!cpuid_enumerates_leaf7_subleaf1_eax_feature(7, 1, 0, bit));
    assert!(cpuid_enumerates_leaf7_subleaf1_eax_feature(7, 1, bit, bit));
    assert!(cpuid_enumerates_leaf7_subleaf1_eax_feature(
        u32::MAX,
        u32::MAX,
        u32::MAX,
        bit
    ));
}

#[test]
fn avx_ifma_cpuid_bit_requires_basic_leaf_7_and_subleaf_1() {
    let bit = 1 << 23;
    assert!(!cpuid_enumerates_leaf7_subleaf1_eax_feature(6, 1, bit, bit));
    assert!(!cpuid_enumerates_leaf7_subleaf1_eax_feature(7, 0, bit, bit));
    assert!(!cpuid_enumerates_leaf7_subleaf1_eax_feature(7, 1, 0, bit));
    assert!(cpuid_enumerates_leaf7_subleaf1_eax_feature(7, 1, bit, bit));
    assert!(cpuid_enumerates_leaf7_subleaf1_eax_feature(
        u32::MAX,
        u32::MAX,
        u32::MAX,
        bit
    ));
}

#[test]
fn avx_ne_convert_cpuid_bit_requires_basic_leaf_7_and_subleaf_1() {
    let bit = 1 << 5;
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
