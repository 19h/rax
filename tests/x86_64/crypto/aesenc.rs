use crate::common::{run_until_hlt, setup_vm};
use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// AESENC - Perform One Round of an AES Encryption Flow
//
// Performs a single round of AES encryption using ShiftRows, SubBytes, MixColumns,
// and XOR with the round key. This instruction is used for all but the last encryption round.
// For the last round, use AESENCLAST (which omits MixColumns).
//
// Operation:
//   STATE := SRC1
//   RoundKey := SRC2
//   STATE := ShiftRows(STATE)
//   STATE := SubBytes(STATE)
//   STATE := MixColumns(STATE)
//   DEST[127:0] := STATE XOR RoundKey
//
// Opcodes:
// 66 0F 38 DC /r              AESENC xmm1, xmm2/m128           - Perform one round of AES encryption

const ALIGNED_ADDR: u64 = 0x3000; // 16-byte aligned address for testing

// ============================================================================
// AESENC Tests - Register to Register Forms
// ============================================================================

#[test]
fn test_aesenc_xmm0_xmm1() {
    // AESENC XMM0, XMM1
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xc1, // AESENC XMM0, XMM1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm1_xmm2() {
    // AESENC XMM1, XMM2
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xca, // AESENC XMM1, XMM2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm2_xmm3() {
    // AESENC XMM2, XMM3
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xd3, // AESENC XMM2, XMM3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm3_xmm4() {
    // AESENC XMM3, XMM4
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xdc, // AESENC XMM3, XMM4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm4_xmm5() {
    // AESENC XMM4, XMM5
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xe5, // AESENC XMM4, XMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm5_xmm6() {
    // AESENC XMM5, XMM6
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xee, // AESENC XMM5, XMM6
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm6_xmm7() {
    // AESENC XMM6, XMM7
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xf7, // AESENC XMM6, XMM7
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm7_xmm0() {
    // AESENC XMM7, XMM0
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xf8, // AESENC XMM7, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// AESENC Tests - Extended Registers (XMM8-XMM15)
// ============================================================================

#[test]
fn test_aesenc_xmm8_xmm9() {
    // AESENC XMM8, XMM9 (requires REX.R and REX.B)
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xc1, // AESENC XMM8, XMM9
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm9_xmm10() {
    // AESENC XMM9, XMM10
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xca, // AESENC XMM9, XMM10
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm10_xmm11() {
    // AESENC XMM10, XMM11
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xd3, // AESENC XMM10, XMM11
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm11_xmm12() {
    // AESENC XMM11, XMM12
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xdc, // AESENC XMM11, XMM12
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm12_xmm13() {
    // AESENC XMM12, XMM13
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xe5, // AESENC XMM12, XMM13
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm13_xmm14() {
    // AESENC XMM13, XMM14
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xee, // AESENC XMM13, XMM14
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm14_xmm15() {
    // AESENC XMM14, XMM15
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xf7, // AESENC XMM14, XMM15
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm15_xmm8() {
    // AESENC XMM15, XMM8
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xf8, // AESENC XMM15, XMM8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// AESENC Tests - Memory Operand Forms
// ============================================================================

#[test]
fn test_aesenc_xmm0_mem() {
    // AESENC XMM0, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x04, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM0, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm1_mem() {
    // AESENC XMM1, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x0c, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM1, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm2_mem() {
    // AESENC XMM2, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x14, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM2, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm3_mem() {
    // AESENC XMM3, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x1c, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM3, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm4_mem() {
    // AESENC XMM4, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x24, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM4, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm5_mem() {
    // AESENC XMM5, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x2c, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM5, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm6_mem() {
    // AESENC XMM6, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x34, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM6, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm7_mem() {
    // AESENC XMM7, [ALIGNED_ADDR]
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0x3c, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM7, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm8_mem() {
    // AESENC XMM8, [ALIGNED_ADDR]
    let code = [
        0x66, 0x44, 0x0f, 0x38, 0xdc, 0x04, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM8, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm15_mem() {
    // AESENC XMM15, [ALIGNED_ADDR]
    let code = [
        0x66, 0x44, 0x0f, 0x38, 0xdc, 0x3c, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM15, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// AESENC Tests - Mixed Register Combinations
// ============================================================================

#[test]
fn test_aesenc_xmm0_xmm7() {
    // AESENC XMM0, XMM7
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xc7, // AESENC XMM0, XMM7
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm7_xmm7() {
    // AESENC XMM7, XMM7 (same register)
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xff, // AESENC XMM7, XMM7
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm0_xmm15() {
    // AESENC XMM0, XMM15 (low to high extended)
    let code = [
        0x66, 0x41, 0x0f, 0x38, 0xdc, 0xc7, // AESENC XMM0, XMM15
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm15_xmm0() {
    // AESENC XMM15, XMM0 (high extended to low)
    let code = [
        0x66, 0x44, 0x0f, 0x38, 0xdc, 0xf8, // AESENC XMM15, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm8_xmm0() {
    // AESENC XMM8, XMM0
    let code = [
        0x66, 0x44, 0x0f, 0x38, 0xdc, 0xc0, // AESENC XMM8, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm0_xmm8() {
    // AESENC XMM0, XMM8
    let code = [
        0x66, 0x41, 0x0f, 0x38, 0xdc, 0xc0, // AESENC XMM0, XMM8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// AESENC Tests - Sequential Operations
// ============================================================================

#[test]
fn test_aesenc_sequential_rounds() {
    // Multiple AESENC operations simulating AES encryption rounds
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xc1, // AESENC XMM0, XMM1 (round 1)
        0x66, 0x0f, 0x38, 0xdc, 0xc2, // AESENC XMM0, XMM2 (round 2)
        0x66, 0x0f, 0x38, 0xdc, 0xc3, // AESENC XMM0, XMM3 (round 3)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_alternating_registers() {
    // Alternating between different XMM registers
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xc1, // AESENC XMM0, XMM1
        0x66, 0x0f, 0x38, 0xdc, 0xca, // AESENC XMM1, XMM2
        0x66, 0x0f, 0x38, 0xdc, 0xd3, // AESENC XMM2, XMM3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_parallel_operations() {
    // Operating on different register pairs
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xc1, // AESENC XMM0, XMM1
        0x66, 0x0f, 0x38, 0xdc, 0xd3, // AESENC XMM2, XMM3
        0x66, 0x0f, 0x38, 0xdc, 0xe5, // AESENC XMM4, XMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// AESENC Tests - All 16 XMM Registers as Destination
// ============================================================================

#[test]
fn test_aesenc_all_destinations() {
    // Test each XMM register as destination
    let code = [
        0x66, 0x0f, 0x38, 0xdc, 0xc0, // AESENC XMM0, XMM0
        0x66, 0x0f, 0x38, 0xdc, 0xc9, // AESENC XMM1, XMM1
        0x66, 0x0f, 0x38, 0xdc, 0xd2, // AESENC XMM2, XMM2
        0x66, 0x0f, 0x38, 0xdc, 0xdb, // AESENC XMM3, XMM3
        0x66, 0x0f, 0x38, 0xdc, 0xe4, // AESENC XMM4, XMM4
        0x66, 0x0f, 0x38, 0xdc, 0xed, // AESENC XMM5, XMM5
        0x66, 0x0f, 0x38, 0xdc, 0xf6, // AESENC XMM6, XMM6
        0x66, 0x0f, 0x38, 0xdc, 0xff, // AESENC XMM7, XMM7
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_all_extended_destinations() {
    // Test each extended XMM register as destination
    let code = [
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xc0, // AESENC XMM8, XMM8
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xc9, // AESENC XMM9, XMM9
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xd2, // AESENC XMM10, XMM10
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xdb, // AESENC XMM11, XMM11
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xe4, // AESENC XMM12, XMM12
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xed, // AESENC XMM13, XMM13
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xf6, // AESENC XMM14, XMM14
        0x66, 0x45, 0x0f, 0x38, 0xdc, 0xff, // AESENC XMM15, XMM15
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm9_mem() {
    // AESENC XMM9, [ALIGNED_ADDR]
    let code = [
        0x66, 0x44, 0x0f, 0x38, 0xdc, 0x0c, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM9, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_aesenc_xmm10_mem() {
    // AESENC XMM10, [ALIGNED_ADDR]
    let code = [
        0x66, 0x44, 0x0f, 0x38, 0xdc, 0x14, 0x25, 0x00, 0x30, 0x00, 0x00, // AESENC XMM10, [0x3000]
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
