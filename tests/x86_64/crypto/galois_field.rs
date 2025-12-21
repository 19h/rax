use crate::common::TestCase;

// Galois Field (GF2P8) Instructions for AES-GCM

// GF2P8AFFINEINVQB - Galois Field Affine Inverse with Quadratic Basis

#[test]
fn test_gf2p8affineinvqb_xmm_xmm_imm8() {
    TestCase::from("66 0f 3a cf c1 00").check();
}

#[test]
fn test_gf2p8affineinvqb_xmm_m128_imm8() {
    TestCase::from("66 0f 3a cf 00 00").check();
}

#[test]
fn test_gf2p8affineinvqb_xmm1_xmm2_00() {
    TestCase::from("66 0f 3a cf c1 00").check();
}

#[test]
fn test_gf2p8affineinvqb_xmm1_xmm2_ff() {
    TestCase::from("66 0f 3a cf c1 ff").check();
}

#[test]
fn test_gf2p8affineinvqb_xmm1_mem_42() {
    TestCase::from("66 0f 3a cf 00 42").check();
}

#[test]
fn test_vgf2p8affineinvqb_xmm_xmm_xmm_imm8() {
    TestCase::from("c4 e3 f9 cf c1 00").check();
}

#[test]
fn test_vgf2p8affineinvqb_ymm_ymm_ymm_imm8() {
    TestCase::from("c4 e3 fd cf c1 00").check();
}

#[test]
fn test_vgf2p8affineinvqb_xmm_xmm_m128_imm8() {
    TestCase::from("c4 e3 f9 cf 00 00").check();
}

// GF2P8AFFINEQB - Galois Field Affine with Quadratic Basis

#[test]
fn test_gf2p8affineqb_xmm_xmm_imm8() {
    TestCase::from("66 0f 3a ce c1 00").check();
}

#[test]
fn test_gf2p8affineqb_xmm_m128_imm8() {
    TestCase::from("66 0f 3a ce 00 00").check();
}

#[test]
fn test_gf2p8affineqb_xmm1_xmm2_00() {
    TestCase::from("66 0f 3a ce c1 00").check();
}

#[test]
fn test_gf2p8affineqb_xmm1_xmm2_ff() {
    TestCase::from("66 0f 3a ce c1 ff").check();
}

#[test]
fn test_gf2p8affineqb_xmm1_mem_42() {
    TestCase::from("66 0f 3a ce 00 42").check();
}

#[test]
fn test_vgf2p8affineqb_xmm_xmm_xmm_imm8() {
    TestCase::from("c4 e3 f9 ce c1 00").check();
}

#[test]
fn test_vgf2p8affineqb_ymm_ymm_ymm_imm8() {
    TestCase::from("c4 e3 fd ce c1 00").check();
}

#[test]
fn test_vgf2p8affineqb_xmm_xmm_m128_imm8() {
    TestCase::from("c4 e3 f9 ce 00 00").check();
}

// GF2P8MULB - Galois Field Multiply Bytes

#[test]
fn test_gf2p8mulb_xmm_xmm() {
    TestCase::from("66 0f 38 cf c1").check();
}

#[test]
fn test_gf2p8mulb_xmm_m128() {
    TestCase::from("66 0f 38 cf 00").check();
}

#[test]
fn test_gf2p8mulb_xmm1_xmm2() {
    TestCase::from("66 0f 38 cf c1").check();
}

#[test]
fn test_gf2p8mulb_xmm3_xmm4() {
    TestCase::from("66 0f 38 cf dc").check();
}

#[test]
fn test_gf2p8mulb_xmm1_mem() {
    TestCase::from("66 0f 38 cf 00").check();
}

#[test]
fn test_vgf2p8mulb_xmm_xmm_xmm() {
    TestCase::from("c4 e2 71 cf c1").check();
}

#[test]
fn test_vgf2p8mulb_ymm_ymm_ymm() {
    TestCase::from("c4 e2 75 cf c1").check();
}

#[test]
fn test_vgf2p8mulb_xmm_xmm_m128() {
    TestCase::from("c4 e2 71 cf 00").check();
}

// LOADIWKEY - Load internal wrapping key

#[test]
fn test_loadiwkey_xmm1_xmm2() {
    TestCase::from("f3 0f 38 dc c8").check();
}

#[test]
fn test_loadiwkey_xmm3_xmm4() {
    TestCase::from("f3 0f 38 dc dc").check();
}

#[test]
fn test_loadiwkey_xmm5_xmm6() {
    TestCase::from("f3 0f 38 dc ee").check();
}

// ENCODEKEY128 - Encode 128-bit key

#[test]
fn test_encodekey128_r32_r32() {
    TestCase::from("f3 0f 38 fa c1").check();
}

#[test]
fn test_encodekey128_eax_ecx() {
    TestCase::from("f3 0f 38 fa c1").check();
}

#[test]
fn test_encodekey128_edx_ebx() {
    TestCase::from("f3 0f 38 fa d3").check();
}

// ENCODEKEY256 - Encode 256-bit key

#[test]
fn test_encodekey256_r32_r32() {
    TestCase::from("f3 0f 38 fb c1").check();
}

#[test]
fn test_encodekey256_eax_ecx() {
    TestCase::from("f3 0f 38 fb c1").check();
}

#[test]
fn test_encodekey256_edx_ebx() {
    TestCase::from("f3 0f 38 fb d3").check();
}
