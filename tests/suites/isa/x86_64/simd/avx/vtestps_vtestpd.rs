use crate::common::*;
use rax::vm::vcpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// VTESTPS - Packed Bit Test for Single-Precision Floating-Point Values
// VTESTPD - Packed Bit Test for Double-Precision Floating-Point Values
//
// VTESTPS/VTESTPD perform a bitwise AND and ANDN operation between two operands,
// set ZF if the result of AND is all zeros, and set CF if the result of ANDN is all zeros.
//
// The instructions compute:
// - TEMP1 = SRC1 AND SRC2
// - TEMP2 = (NOT SRC1) AND SRC2
// - ZF = (TEMP1 == 0)  // All bits are zero after AND
// - CF = (TEMP2 == 0)  // All bits are zero after ANDN
//
// This is commonly used for:
// - Testing if any bits are set (ZF=0 means at least one bit matched)
// - Testing if all bits are set in masked region (CF=1 means all masked bits are set)
//
// Opcodes:
// VEX.128.66.0F38.W0 0E /r   VTESTPS xmm1, xmm2/m128   - Test 128-bit packed singles
// VEX.256.66.0F38.W0 0E /r   VTESTPS ymm1, ymm2/m256   - Test 256-bit packed singles
// VEX.128.66.0F38.W0 0F /r   VTESTPD xmm1, xmm2/m128   - Test 128-bit packed doubles
// VEX.256.66.0F38.W0 0F /r   VTESTPD ymm1, ymm2/m256   - Test 256-bit packed doubles

const ALIGNED_ADDR: u64 = 0x3000; // 32-byte aligned address for testing

// ============================================================================
// VTESTPS Tests - 128-bit XMM registers
// ============================================================================

#[test]
fn test_vtestps_xmm0_xmm1() {
    // VTESTPS XMM0, XMM1
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xc1, // VTESTPS XMM0, XMM1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm1_xmm2() {
    // VTESTPS XMM1, XMM2
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xca, // VTESTPS XMM1, XMM2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm2_xmm3() {
    // VTESTPS XMM2, XMM3
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xd3, // VTESTPS XMM2, XMM3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm3_xmm4() {
    // VTESTPS XMM3, XMM4
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xdc, // VTESTPS XMM3, XMM4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm4_xmm5() {
    // VTESTPS XMM4, XMM5
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xe5, // VTESTPS XMM4, XMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm5_xmm6() {
    // VTESTPS XMM5, XMM6
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xee, // VTESTPS XMM5, XMM6
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm6_xmm7() {
    // VTESTPS XMM6, XMM7
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xf7, // VTESTPS XMM6, XMM7
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm7_xmm0() {
    // VTESTPS XMM7, XMM0
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xf8, // VTESTPS XMM7, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPS Tests - Extended XMM registers (XMM8-XMM15)
// ============================================================================

#[test]
fn test_vtestps_xmm8_xmm9() {
    // VTESTPS XMM8, XMM9
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xc1, // VTESTPS XMM8, XMM9
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm9_xmm10() {
    // VTESTPS XMM9, XMM10
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xca, // VTESTPS XMM9, XMM10
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm10_xmm11() {
    // VTESTPS XMM10, XMM11
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xd3, // VTESTPS XMM10, XMM11
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm11_xmm12() {
    // VTESTPS XMM11, XMM12
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xdc, // VTESTPS XMM11, XMM12
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm12_xmm13() {
    // VTESTPS XMM12, XMM13
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xe5, // VTESTPS XMM12, XMM13
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm13_xmm14() {
    // VTESTPS XMM13, XMM14
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xee, // VTESTPS XMM13, XMM14
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm14_xmm15() {
    // VTESTPS XMM14, XMM15
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xf7, // VTESTPS XMM14, XMM15
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm15_xmm8() {
    // VTESTPS XMM15, XMM8
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xf8, // VTESTPS XMM15, XMM8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPS Tests - Cross-domain XMM registers
// ============================================================================

#[test]
fn test_vtestps_xmm0_xmm8() {
    // VTESTPS XMM0, XMM8
    let code = [
        0xc4, 0xc2, 0x79, 0x0e, 0xc0, // VTESTPS XMM0, XMM8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm8_xmm0() {
    // VTESTPS XMM8, XMM0
    let code = [
        0xc4, 0x42, 0x79, 0x0e, 0xc0, // VTESTPS XMM8, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm7_xmm15() {
    // VTESTPS XMM7, XMM15
    let code = [
        0xc4, 0xc2, 0x79, 0x0e, 0xff, // VTESTPS XMM7, XMM15
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPS Tests - Memory operands (128-bit)
// ============================================================================

#[test]
fn test_vtestps_xmm0_mem() {
    // VTESTPS XMM0, [mem]
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPS XMM0, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 16] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm1_mem() {
    // VTESTPS XMM1, [mem]
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0x0d, 0x00, 0x40, 0x00, 0x00, // VTESTPS XMM1, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 16] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_xmm8_mem() {
    // VTESTPS XMM8, [mem]
    let code = [
        0xc4, 0x62, 0x79, 0x0e, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPS XMM8, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 16] = [
        0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
        0xaa,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPS Tests - 256-bit YMM registers
// ============================================================================

#[test]
fn test_vtestps_ymm0_ymm1() {
    // VTESTPS YMM0, YMM1
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xc1, // VTESTPS YMM0, YMM1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm1_ymm2() {
    // VTESTPS YMM1, YMM2
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xca, // VTESTPS YMM1, YMM2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm2_ymm3() {
    // VTESTPS YMM2, YMM3
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xd3, // VTESTPS YMM2, YMM3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm3_ymm4() {
    // VTESTPS YMM3, YMM4
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xdc, // VTESTPS YMM3, YMM4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm4_ymm5() {
    // VTESTPS YMM4, YMM5
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xe5, // VTESTPS YMM4, YMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm5_ymm6() {
    // VTESTPS YMM5, YMM6
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xee, // VTESTPS YMM5, YMM6
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm6_ymm7() {
    // VTESTPS YMM6, YMM7
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xf7, // VTESTPS YMM6, YMM7
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm7_ymm0() {
    // VTESTPS YMM7, YMM0
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0xf8, // VTESTPS YMM7, YMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm8_ymm9() {
    // VTESTPS YMM8, YMM9
    let code = [
        0xc4, 0x42, 0x7d, 0x0e, 0xc1, // VTESTPS YMM8, YMM9
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm15_ymm14() {
    // VTESTPS YMM15, YMM14
    let code = [
        0xc4, 0x42, 0x7d, 0x0e, 0xfe, // VTESTPS YMM15, YMM14
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPS Tests - Memory operands (256-bit)
// ============================================================================

#[test]
fn test_vtestps_ymm0_mem() {
    // VTESTPS YMM0, [mem]
    let code = [
        0xc4, 0xe2, 0x7d, 0x0e, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPS YMM0, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 32] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_ymm8_mem() {
    // VTESTPS YMM8, [mem]
    let code = [
        0xc4, 0x62, 0x7d, 0x0e, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPS YMM8, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 32] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPD Tests - 128-bit XMM registers
// ============================================================================

#[test]
fn test_vtestpd_xmm0_xmm1() {
    // VTESTPD XMM0, XMM1
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xc1, // VTESTPD XMM0, XMM1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm1_xmm2() {
    // VTESTPD XMM1, XMM2
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xca, // VTESTPD XMM1, XMM2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm2_xmm3() {
    // VTESTPD XMM2, XMM3
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xd3, // VTESTPD XMM2, XMM3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm3_xmm4() {
    // VTESTPD XMM3, XMM4
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xdc, // VTESTPD XMM3, XMM4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm4_xmm5() {
    // VTESTPD XMM4, XMM5
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xe5, // VTESTPD XMM4, XMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm5_xmm6() {
    // VTESTPD XMM5, XMM6
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xee, // VTESTPD XMM5, XMM6
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm6_xmm7() {
    // VTESTPD XMM6, XMM7
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xf7, // VTESTPD XMM6, XMM7
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm7_xmm0() {
    // VTESTPD XMM7, XMM0
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xf8, // VTESTPD XMM7, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPD Tests - Extended XMM registers
// ============================================================================

#[test]
fn test_vtestpd_xmm8_xmm9() {
    // VTESTPD XMM8, XMM9
    let code = [
        0xc4, 0x42, 0x79, 0x0f, 0xc1, // VTESTPD XMM8, XMM9
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm9_xmm10() {
    // VTESTPD XMM9, XMM10
    let code = [
        0xc4, 0x42, 0x79, 0x0f, 0xca, // VTESTPD XMM9, XMM10
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm10_xmm11() {
    // VTESTPD XMM10, XMM11
    let code = [
        0xc4, 0x42, 0x79, 0x0f, 0xd3, // VTESTPD XMM10, XMM11
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm15_xmm14() {
    // VTESTPD XMM15, XMM14
    let code = [
        0xc4, 0x42, 0x79, 0x0f, 0xfe, // VTESTPD XMM15, XMM14
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPD Tests - Memory operands (128-bit)
// ============================================================================

#[test]
fn test_vtestpd_xmm0_mem() {
    // VTESTPD XMM0, [mem]
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPD XMM0, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 16] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_xmm8_mem() {
    // VTESTPD XMM8, [mem]
    let code = [
        0xc4, 0x62, 0x79, 0x0f, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPD XMM8, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 16] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPD Tests - 256-bit YMM registers
// ============================================================================

#[test]
fn test_vtestpd_ymm0_ymm1() {
    // VTESTPD YMM0, YMM1
    let code = [
        0xc4, 0xe2, 0x7d, 0x0f, 0xc1, // VTESTPD YMM0, YMM1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_ymm1_ymm2() {
    // VTESTPD YMM1, YMM2
    let code = [
        0xc4, 0xe2, 0x7d, 0x0f, 0xca, // VTESTPD YMM1, YMM2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_ymm2_ymm3() {
    // VTESTPD YMM2, YMM3
    let code = [
        0xc4, 0xe2, 0x7d, 0x0f, 0xd3, // VTESTPD YMM2, YMM3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_ymm3_ymm4() {
    // VTESTPD YMM3, YMM4
    let code = [
        0xc4, 0xe2, 0x7d, 0x0f, 0xdc, // VTESTPD YMM3, YMM4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_ymm4_ymm5() {
    // VTESTPD YMM4, YMM5
    let code = [
        0xc4, 0xe2, 0x7d, 0x0f, 0xe5, // VTESTPD YMM4, YMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_ymm8_ymm9() {
    // VTESTPD YMM8, YMM9
    let code = [
        0xc4, 0x42, 0x7d, 0x0f, 0xc1, // VTESTPD YMM8, YMM9
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_ymm15_ymm8() {
    // VTESTPD YMM15, YMM8
    let code = [
        0xc4, 0x42, 0x7d, 0x0f, 0xf8, // VTESTPD YMM15, YMM8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VTESTPD Tests - Memory operands (256-bit)
// ============================================================================

#[test]
fn test_vtestpd_ymm0_mem() {
    // VTESTPD YMM0, [mem]
    let code = [
        0xc4, 0xe2, 0x7d, 0x0f, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPD YMM0, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 32] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_ymm8_mem() {
    // VTESTPD YMM8, [mem]
    let code = [
        0xc4, 0x62, 0x7d, 0x0f, 0x05, 0x00, 0x40, 0x00, 0x00, // VTESTPD YMM8, [rip + 0x4000]
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 32] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// Combined tests with comparison operations
// ============================================================================

#[test]
fn test_vtestps_after_vcmpps() {
    // VCMPPS followed by VTESTPS
    let code = [
        0xc5, 0xf0, 0xc2, 0xc2, 0x00, // VCMPPS XMM0, XMM1, XMM2, 0 (EQ)
        0xc4, 0xe2, 0x79, 0x0e, 0xc0, // VTESTPS XMM0, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_after_vcmppd() {
    // VCMPPD followed by VTESTPD
    let code = [
        0xc5, 0xf1, 0xc2, 0xc2, 0x00, // VCMPPD XMM0, XMM1, XMM2, 0 (EQ)
        0xc4, 0xe2, 0x79, 0x0f, 0xc0, // VTESTPD XMM0, XMM0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestps_multiple_tests() {
    // Multiple VTESTPS operations
    let code = [
        0xc4, 0xe2, 0x79, 0x0e, 0xc1, // VTESTPS XMM0, XMM1
        0xc4, 0xe2, 0x79, 0x0e, 0xd3, // VTESTPS XMM2, XMM3
        0xc4, 0xe2, 0x79, 0x0e, 0xe5, // VTESTPS XMM4, XMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vtestpd_multiple_tests() {
    // Multiple VTESTPD operations
    let code = [
        0xc4, 0xe2, 0x79, 0x0f, 0xc1, // VTESTPD XMM0, XMM1
        0xc4, 0xe2, 0x79, 0x0f, 0xd3, // VTESTPD XMM2, XMM3
        0xc4, 0xe2, 0x79, 0x0f, 0xe5, // VTESTPD XMM4, XMM5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

fn assert_vtestpd_register_flags(wide: bool, first: [u64; 4], second: [u64; 4]) {
    let code = [
        0xC4,
        0xE2,
        if wide { 0x7D } else { 0x79 },
        0x0F,
        0xC1, // VTESTPD XMM/YMM0, XMM/YMM1
        0xF4,
    ];
    let mut initial = Registers::default();
    initial.xmm[0] = [first[0], first[1]];
    initial.ymm_high[0] = [first[2], first[3]];
    initial.xmm[1] = [second[0], second[1]];
    initial.ymm_high[1] = [second[2], second[3]];
    initial.rflags = 0x2 | 0x8D5 | (1 << 10);

    let (mut vcpu, _) = setup_vm(&code, Some(initial.clone()));
    let actual = run_until_hlt(&mut vcpu).unwrap();
    let lane_count = if wide { 4 } else { 2 };
    let mut intersection = 0u64;
    let mut outside = 0u64;
    for lane in 0..lane_count {
        let first_sign = first[lane] & (1 << 63);
        let second_sign = second[lane] & (1 << 63);
        intersection |= first_sign & second_sign;
        outside |= second_sign & !first_sign;
    }
    let expected_rflags =
        (initial.rflags & !0x8D5) | u64::from(outside == 0) | (u64::from(intersection == 0) << 6);
    assert_eq!(actual.rflags, expected_rflags, "wide={wide}");
    assert_eq!(actual.xmm[0], initial.xmm[0], "wide={wide}: XMM0");
    assert_eq!(
        actual.ymm_high[0], initial.ymm_high[0],
        "wide={wide}: YMM0 high"
    );
    assert_eq!(actual.xmm[1], initial.xmm[1], "wide={wide}: XMM1");
    assert_eq!(
        actual.ymm_high[1], initial.ymm_high[1],
        "wide={wide}: YMM1 high"
    );
}

#[test]
fn vtestpd_tests_each_64_bit_sign_lane_and_all_defined_flag_outcomes() {
    const SIGN: u64 = 1 << 63;
    for wide in [false, true] {
        let lane_count = if wide { 4 } else { 2 };
        for lane in 0..lane_count {
            let mut first = [0u64; 4];
            let mut second = [0u64; 4];
            assert_vtestpd_register_flags(wide, first, second);

            second[lane] = SIGN;
            assert_vtestpd_register_flags(wide, first, second);

            first[lane] = SIGN;
            assert_vtestpd_register_flags(wide, first, second);

            second[if lane == 0 { 1 } else { 0 }] = SIGN;
            assert_vtestpd_register_flags(wide, first, second);
        }
    }
}

fn assert_reserved_vtest_ud_noncommitting(opcode: u8, p1: u8, name: &str) {
    let code = [0xC4, 0xE2, p1, opcode, 0xC1, 0xF4];
    let mut initial = Registers::default();
    initial.rax = 0x0123_4567_89AB_CDEF;
    initial.rflags = 0x2 | 0x8D5 | (1 << 10);
    initial.xmm[0] = [1, 2];
    initial.ymm_high[0] = [3, 4];
    initial.xmm[1] = [5, 6];
    initial.ymm_high[1] = [7, 8];

    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));
    for path in ["cold decode", "decode-cache hit"] {
        let before = vcpu.get_regs().unwrap();
        let error = vcpu
            .step()
            .expect_err("reserved VTEST encoding must raise #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name} ({path}): expected #UD delivery failure, got {error}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, before.rip, "{name} ({path}): RIP");
        assert_eq!(after.rax, before.rax, "{name} ({path}): RAX");
        assert_eq!(after.rflags, before.rflags, "{name} ({path}): RFLAGS");
        assert_eq!(after.xmm, before.xmm, "{name} ({path}): XMM state");
        assert_eq!(
            after.ymm_high, before.ymm_high,
            "{name} ({path}): YMM upper state"
        );
    }
}

#[test]
fn vtestps_vtestpd_reserved_w_and_vvvv_raise_ud_without_committing() {
    for (opcode, mnemonic) in [(0x0E, "VTESTPS"), (0x0F, "VTESTPD")] {
        for (p1, field) in [
            (0xF9, "W=1,L=0"),
            (0xFD, "W=1,L=1"),
            (0x71, "vvvv!=1111,L=0"),
            (0x75, "vvvv!=1111,L=1"),
        ] {
            assert_reserved_vtest_ud_noncommitting(opcode, p1, &format!("{mnemonic} {field}"));
        }
    }
}
