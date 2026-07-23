use crate::common::*;
use rax::vm::vcpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// VEXTRACTF128 - Extract 128-bit Floating-Point Value
// VINSERTF128 - Insert 128-bit Floating-Point Value
// VEXTRACTI128 - Extract 128-bit Integer Value
// VINSERTI128 - Insert 128-bit Integer Value
//
// VEXTRACTF128 extracts a 128-bit floating-point value from a 256-bit source
// and stores it to a 128-bit destination (XMM register or memory).
// VINSERTF128 inserts a 128-bit floating-point value into a 256-bit destination.
// The imm8 parameter specifies which 128-bit lane to extract/insert.
//
// Opcodes:
// VEX.256.66 0F 3A 19 /r ib    VEXTRACTF128 xmm1/m128, ymm2, imm8   - Extract 128-bit float
// VEX.256.66 0F 3A 18 /r ib    VINSERTF128 ymm1, ymm2, xmm3/m128, imm8 - Insert 128-bit float

const ALIGNED_ADDR: u64 = 0x3000; // 32-byte aligned address for testing

// ============================================================================
// VEXTRACTF128 Tests - Extract 128-bit Float (YMM -> XMM/Memory)
// ============================================================================

#[test]
fn test_vextractf128_ymm0_xmm1_lane0() {
    // VEXTRACTF128 XMM1, YMM0, 0 (extract lower 128 bits)
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xc1, 0x00, // VEXTRACTF128 XMM1, YMM0, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm0_xmm1_lane1() {
    // VEXTRACTF128 XMM1, YMM0, 1 (extract upper 128 bits)
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xc1, 0x01, // VEXTRACTF128 XMM1, YMM0, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm1_xmm2_lane0() {
    // VEXTRACTF128 XMM2, YMM1, 0
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xca, 0x00, // VEXTRACTF128 XMM2, YMM1, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm1_xmm2_lane1() {
    // VEXTRACTF128 XMM2, YMM1, 1
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xca, 0x01, // VEXTRACTF128 XMM2, YMM1, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm2_xmm3_lane0() {
    // VEXTRACTF128 XMM3, YMM2, 0
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xd3, 0x00, // VEXTRACTF128 XMM3, YMM2, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm3_xmm4_lane1() {
    // VEXTRACTF128 XMM4, YMM3, 1
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xdc, 0x01, // VEXTRACTF128 XMM4, YMM3, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm4_xmm5_lane0() {
    // VEXTRACTF128 XMM5, YMM4, 0
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xe5, 0x00, // VEXTRACTF128 XMM5, YMM4, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm5_xmm6_lane1() {
    // VEXTRACTF128 XMM6, YMM5, 1
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xee, 0x01, // VEXTRACTF128 XMM6, YMM5, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm6_xmm7_lane0() {
    // VEXTRACTF128 XMM7, YMM6, 0
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xf7, 0x00, // VEXTRACTF128 XMM7, YMM6, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm7_xmm0_lane1() {
    // VEXTRACTF128 XMM0, YMM7, 1
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xf8, 0x01, // VEXTRACTF128 XMM0, YMM7, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm8_xmm9_lane0() {
    // VEXTRACTF128 XMM9, YMM8, 0
    let code = [
        0xc4, 0xc3, 0x7d, 0x19, 0xc1, 0x00, // VEXTRACTF128 XMM9, YMM8, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm12_xmm13_lane1() {
    // VEXTRACTF128 XMM13, YMM12, 1
    let code = [
        0xc4, 0xc3, 0x7d, 0x19, 0xe5, 0x01, // VEXTRACTF128 XMM13, YMM12, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VEXTRACTF128 Memory Tests
// ============================================================================

#[test]
fn test_vextractf128_ymm0_mem_lane0() {
    // VEXTRACTF128 [mem128], YMM0, 0
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0x05, 0x00, 0x40, 0x00, 0x00,
        0x00, // VEXTRACTF128 [rip + 0x4000], YMM0, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm1_mem_lane1() {
    // VEXTRACTF128 [mem128], YMM1, 1
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0x0d, 0x00, 0x40, 0x00, 0x00,
        0x01, // VEXTRACTF128 [rip + 0x4000], YMM1, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm8_mem_lane0() {
    // VEXTRACTF128 [mem128], YMM8, 0
    let code = [
        0xc4, 0xc3, 0x7d, 0x19, 0x05, 0x00, 0x40, 0x00, 0x00,
        0x00, // VEXTRACTF128 [rip + 0x4000], YMM8, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_ymm15_mem_lane1() {
    // VEXTRACTF128 [mem128], YMM15, 1
    let code = [
        0xc4, 0xc3, 0x7d, 0x19, 0x3d, 0x00, 0x40, 0x00, 0x00,
        0x01, // VEXTRACTF128 [rip + 0x4000], YMM15, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VINSERTF128 Tests - Insert 128-bit Float (YMM, XMM -> YMM)
// ============================================================================

#[test]
fn test_vinsertf128_ymm0_ymm1_xmm2_lane0() {
    // VINSERTF128 YMM0, YMM1, XMM2, 0 (insert into lower 128 bits)
    let code = [
        0xc4, 0xe3, 0x75, 0x18, 0xc2, 0x00, // VINSERTF128 YMM0, YMM1, XMM2, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm0_ymm1_xmm2_lane1() {
    // VINSERTF128 YMM0, YMM1, XMM2, 1 (insert into upper 128 bits)
    let code = [
        0xc4, 0xe3, 0x75, 0x18, 0xc2, 0x01, // VINSERTF128 YMM0, YMM1, XMM2, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm1_ymm2_xmm3_lane0() {
    // VINSERTF128 YMM1, YMM2, XMM3, 0
    let code = [
        0xc4, 0xe3, 0x6d, 0x18, 0xcb, 0x00, // VINSERTF128 YMM1, YMM2, XMM3, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm2_ymm3_xmm4_lane1() {
    // VINSERTF128 YMM2, YMM3, XMM4, 1
    let code = [
        0xc4, 0xe3, 0x65, 0x18, 0xd4, 0x01, // VINSERTF128 YMM2, YMM3, XMM4, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm3_ymm4_xmm5_lane0() {
    // VINSERTF128 YMM3, YMM4, XMM5, 0
    let code = [
        0xc4, 0xe3, 0x5d, 0x18, 0xdd, 0x00, // VINSERTF128 YMM3, YMM4, XMM5, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm4_ymm5_xmm6_lane1() {
    // VINSERTF128 YMM4, YMM5, XMM6, 1
    let code = [
        0xc4, 0xe3, 0x55, 0x18, 0xe6, 0x01, // VINSERTF128 YMM4, YMM5, XMM6, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm5_ymm6_xmm7_lane0() {
    // VINSERTF128 YMM5, YMM6, XMM7, 0
    let code = [
        0xc4, 0xe3, 0x4d, 0x18, 0xef, 0x00, // VINSERTF128 YMM5, YMM6, XMM7, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm6_ymm7_xmm0_lane1() {
    // VINSERTF128 YMM6, YMM7, XMM0, 1
    let code = [
        0xc4, 0xe3, 0x45, 0x18, 0xf8, 0x01, // VINSERTF128 YMM6, YMM7, XMM0, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm7_ymm0_xmm1_lane0() {
    // VINSERTF128 YMM7, YMM0, XMM1, 0
    let code = [
        0xc4, 0xe3, 0x7d, 0x18, 0xf9, 0x00, // VINSERTF128 YMM7, YMM0, XMM1, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm8_ymm9_xmm10_lane0() {
    // VINSERTF128 YMM8, YMM9, XMM10, 0
    let code = [
        0xc4, 0xc3, 0x35, 0x18, 0xc2, 0x00, // VINSERTF128 YMM8, YMM9, XMM10, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm12_ymm13_xmm14_lane1() {
    // VINSERTF128 YMM12, YMM13, XMM14, 1
    let code = [
        0xc4, 0xc3, 0x15, 0x18, 0xe6, 0x01, // VINSERTF128 YMM12, YMM13, XMM14, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VINSERTF128 Memory Tests
// ============================================================================

#[test]
fn test_vinsertf128_ymm0_ymm1_mem_lane0() {
    // VINSERTF128 YMM0, YMM1, [mem128], 0
    let code = [
        0xc4, 0xe3, 0x75, 0x18, 0x05, 0x00, 0x40, 0x00, 0x00,
        0x00, // VINSERTF128 YMM0, YMM1, [rip + 0x4000], 0
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Initialize memory with test data
    let test_data: [u8; 16] = [
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x80,
        0x40,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm2_ymm3_mem_lane1() {
    // VINSERTF128 YMM2, YMM3, [mem128], 1
    let code = [
        0xc4, 0xe3, 0x65, 0x18, 0x15, 0x00, 0x40, 0x00, 0x00,
        0x01, // VINSERTF128 YMM2, YMM3, [rip + 0x4000], 1
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 16] = [
        0x00, 0x00, 0xa0, 0x40, 0x00, 0x00, 0xc0, 0x40, 0x00, 0x00, 0xe0, 0x40, 0x00, 0x00, 0x00,
        0x41,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_ymm8_ymm9_mem_lane0() {
    // VINSERTF128 YMM8, YMM9, [mem128], 0
    let code = [
        0xc4, 0xc3, 0x35, 0x18, 0x05, 0x00, 0x40, 0x00, 0x00,
        0x00, // VINSERTF128 YMM8, YMM9, [rip + 0x4000], 0
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

#[test]
fn test_vinsertf128_ymm14_ymm15_mem_lane1() {
    // VINSERTF128 YMM14, YMM15, [mem128], 1
    let code = [
        0xc4, 0xc3, 0x05, 0x18, 0x35, 0x00, 0x40, 0x00, 0x00,
        0x01, // VINSERTF128 YMM14, YMM15, [rip + 0x4000], 1
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let test_data: [u8; 16] = [
        0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb,
        0xbb,
    ];
    mem.write_slice(&test_data, GuestAddress(ALIGNED_ADDR))
        .unwrap();

    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// Combined Extract-Insert Tests
// ============================================================================

#[test]
fn test_vextractf128_then_insert() {
    // Extract from YMM0 and insert back (simulated sequence)
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xc1, 0x00, // VEXTRACTF128 XMM1, YMM0, 0
        0xc4, 0xe3, 0x75, 0x18, 0xc1, 0x00, // VINSERTF128 YMM0, YMM1, XMM1, 0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vextractf128_lane_swap() {
    // Extract upper lane and insert into lower lane
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xc1, 0x01, // VEXTRACTF128 XMM1, YMM0, 1 (extract upper)
        0xc4, 0xe3, 0x75, 0x18, 0xc1, 0x00, // VINSERTF128 YMM0, YMM1, XMM1, 0 (insert lower)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vinsertf128_sequence() {
    // Insert XMM data into both lanes of YMM
    let code = [
        0xc4, 0xe3, 0x75, 0x18, 0xc1, 0x00, // VINSERTF128 YMM0, YMM1, XMM1, 0 (lower)
        0xc4, 0xe3, 0x75, 0x18, 0xc2, 0x01, // VINSERTF128 YMM0, YMM1, XMM2, 1 (upper)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// Known-answer VALUE tests : VEXTRACTF128 / VINSERTF128 move a whole 128-bit lane.
// ============================================================================

use rax::isa::x86_64::X86_64Vcpu;

fn kei_set(vcpu: &mut X86_64Vcpu, idx: usize, lo: u128, hi: u128) {
    let mut regs = vcpu.get_regs().unwrap();
    regs.xmm[idx][0] = lo as u64;
    regs.xmm[idx][1] = (lo >> 64) as u64;
    regs.ymm_high[idx][0] = hi as u64;
    regs.ymm_high[idx][1] = (hi >> 64) as u64;
    vcpu.set_regs(&regs).unwrap();
}
fn kei_lo(vcpu: &X86_64Vcpu, idx: usize) -> u128 {
    let r = vcpu.get_regs().unwrap();
    (r.xmm[idx][0] as u128) | ((r.xmm[idx][1] as u128) << 64)
}
fn kei_hi(vcpu: &X86_64Vcpu, idx: usize) -> u128 {
    let r = vcpu.get_regs().unwrap();
    (r.ymm_high[idx][0] as u128) | ((r.ymm_high[idx][1] as u128) << 64)
}

const EI_LO: u128 = 0x1122_3344_5566_7788_99AA_BBCC_DDEE_FF00;
const EI_HI: u128 = 0xFEDC_BA98_7654_3210_0123_4567_89AB_CDEF;

#[test]
fn test_vextractf128_low_value() {
    // VEXTRACTF128 XMM1, YMM0, 0 ; XMM1 gets the LOW lane, upper 128 of YMM1 zeroed.
    let code = [0xc4, 0xe3, 0x7d, 0x19, 0xc1, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    kei_set(&mut vcpu, 0, EI_LO, EI_HI);
    kei_set(&mut vcpu, 1, 0xDEAD_BEEF, 0xCAFE_BABE); // pre-existing garbage
    run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(kei_lo(&vcpu, 1), EI_LO);
    assert_eq!(
        kei_hi(&vcpu, 1),
        0,
        "128-bit write must zero upper 128 bits"
    );
}

#[test]
fn test_vextractf128_high_value() {
    // VEXTRACTF128 XMM1, YMM0, 1 ; XMM1 gets the HIGH lane.
    let code = [0xc4, 0xe3, 0x7d, 0x19, 0xc1, 0x01, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    kei_set(&mut vcpu, 0, EI_LO, EI_HI);
    kei_set(&mut vcpu, 1, 0xDEAD_BEEF, 0xCAFE_BABE);
    run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(kei_lo(&vcpu, 1), EI_HI);
    assert_eq!(
        kei_hi(&vcpu, 1),
        0,
        "128-bit write must zero upper 128 bits"
    );
}

#[test]
fn test_vinsertf128_low_value() {
    // VINSERTF128 YMM0, YMM1, XMM2, 0 ; low lane <- XMM2, high lane <- YMM1 high.
    let code = [0xc4, 0xe3, 0x75, 0x18, 0xc2, 0x00, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    kei_set(&mut vcpu, 1, EI_LO, EI_HI);
    let new_lane: u128 = 0xAAAA_BBBB_CCCC_DDDD_EEEE_FFFF_0000_1111;
    kei_set(&mut vcpu, 2, new_lane, 0); // XMM2 low 128 only
    run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(kei_lo(&vcpu, 0), new_lane);
    assert_eq!(kei_hi(&vcpu, 0), EI_HI);
}

#[test]
fn test_vinsertf128_high_value() {
    // VINSERTF128 YMM0, YMM1, XMM2, 1 ; high lane <- XMM2, low lane <- YMM1 low.
    let code = [0xc4, 0xe3, 0x75, 0x18, 0xc2, 0x01, 0xf4];
    let (mut vcpu, _) = setup_vm(&code, None);
    kei_set(&mut vcpu, 1, EI_LO, EI_HI);
    let new_lane: u128 = 0xAAAA_BBBB_CCCC_DDDD_EEEE_FFFF_0000_1111;
    kei_set(&mut vcpu, 2, new_lane, 0);
    run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(kei_lo(&vcpu, 0), EI_LO);
    assert_eq!(kei_hi(&vcpu, 0), new_lane);
}

#[test]
fn test_vextract_then_vinsert_roundtrip() {
    // Extract high lane to XMM1, then insert XMM1 into low lane of YMM2 (from YMM3).
    let code = [
        0xc4, 0xe3, 0x7d, 0x19, 0xc1, 0x01, // VEXTRACTF128 XMM1, YMM0, 1
        0xc4, 0xe3, 0x65, 0x18, 0xd1, 0x00, // VINSERTF128 YMM2, YMM3, XMM1, 0
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    kei_set(&mut vcpu, 0, EI_LO, EI_HI);
    kei_set(
        &mut vcpu,
        3,
        0x1111_1111_1111_1111_2222_2222_2222_2222,
        0x3333_3333_3333_3333_4444_4444_4444_4444,
    );
    run_until_hlt(&mut vcpu).unwrap();
    // XMM1 == high lane of YMM0
    assert_eq!(kei_lo(&vcpu, 1), EI_HI);
    // YMM2 low lane == EI_HI, high lane == YMM3 high lane.
    assert_eq!(kei_lo(&vcpu, 2), EI_HI);
    assert_eq!(kei_hi(&vcpu, 2), 0x3333_3333_3333_3333_4444_4444_4444_4444);
}

fn assert_vex_chunk_ud(code: &[u8], name: &str) {
    let mut initial = Registers::default();
    initial.rax = 0x0123_4567_89AB_CDEF;
    initial.rflags = 0x2 | 0x08D5;
    initial.xmm[0] = [1, 2];
    initial.ymm_high[0] = [3, 4];
    initial.zmm_high[0] = [5, 6, 7, 8];
    initial.xmm[1] = [9, 10];
    initial.ymm_high[1] = [11, 12];
    initial.zmm_high[1] = [13, 14, 15, 16];

    let (mut vcpu, _) = setup_vm_no_idt(code, Some(initial));
    for path in ["cold decode", "decode-cache hit"] {
        let before = vcpu.get_regs().unwrap();
        let error = match vcpu.step() {
            Err(error) => error,
            Ok(exit) => panic!("{name} {path}: expected #UD, got {exit:?}"),
        };
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name} {path}: expected #UD delivery failure, got {error}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, before.rip, "{name} {path}: RIP");
        assert_eq!(after.rax, before.rax, "{name} {path}: RAX");
        assert_eq!(after.rflags, before.rflags, "{name} {path}: RFLAGS");
        assert_eq!(after.xmm, before.xmm, "{name} {path}: XMM state");
        assert_eq!(
            after.ymm_high, before.ymm_high,
            "{name} {path}: YMM upper state"
        );
        assert_eq!(
            after.zmm_high, before.zmm_high,
            "{name} {path}: ZMM upper state"
        );
    }
}

#[test]
fn vex_chunk_reserved_fields_raise_ud_before_operand_fetch_or_commit() {
    for (opcode, mnemonic) in [(0x18, "VINSERTF128"), (0x38, "VINSERTI128")] {
        for (third, field) in [
            (0xF5, "W=1"),
            (0x71, "L=0"),
            (0x74, "pp=none"),
            (0x76, "pp=F3"),
            (0x77, "pp=F2"),
        ] {
            assert_vex_chunk_ud(&[0xC4, 0xE3, third, opcode], &format!("{mnemonic} {field}"));
        }
    }

    for (opcode, mnemonic) in [(0x19, "VEXTRACTF128"), (0x39, "VEXTRACTI128")] {
        for (third, field) in [
            (0xFD, "W=1"),
            (0x79, "L=0"),
            (0x75, "vvvv!=1111b"),
            (0x7C, "pp=none"),
            (0x7E, "pp=F3"),
            (0x7F, "pp=F2"),
        ] {
            assert_vex_chunk_ud(&[0xC4, 0xE3, third, opcode], &format!("{mnemonic} {field}"));
        }
    }
}

#[test]
fn vex_chunk_register_writes_clear_every_bit_above_the_architectural_result() {
    for (opcode, mnemonic) in [(0x18, "VINSERTF128"), (0x38, "VINSERTI128")] {
        let code = [0xC4, 0xE3, 0x75, opcode, 0xC2, 0x01, 0xF4];
        let mut initial = Registers::default();
        initial.xmm[1] = [0x1111, 0x2222];
        initial.ymm_high[1] = [0x3333, 0x4444];
        initial.xmm[2] = [0xAAAA, 0xBBBB];
        // Keep the low 256-bit result bit-identical so the generic
        // value-difference detector cannot perform the architectural zeroing.
        initial.xmm[0] = [0x1111, 0x2222];
        initial.ymm_high[0] = [0xAAAA, 0xBBBB];
        initial.zmm_high[0] = [u64::MAX; 4];
        let (mut vcpu, _) = setup_vm(&code, Some(initial));
        run_until_hlt(&mut vcpu).unwrap();
        let actual = vcpu.get_regs().unwrap();
        assert_eq!(actual.xmm[0], [0x1111, 0x2222], "{mnemonic}: low lane");
        assert_eq!(
            actual.ymm_high[0],
            [0xAAAA, 0xBBBB],
            "{mnemonic}: inserted lane"
        );
        assert_eq!(actual.zmm_high[0], [0; 4], "{mnemonic}: ZMM[511:256]");
    }

    for (opcode, mnemonic) in [(0x19, "VEXTRACTF128"), (0x39, "VEXTRACTI128")] {
        let code = [0xC4, 0xE3, 0x7D, opcode, 0xC8, 0x01, 0xF4];
        let mut initial = Registers::default();
        initial.xmm[1] = [0x1111, 0x2222];
        initial.ymm_high[1] = [0x3333, 0x4444];
        // The extracted XMM value and zero YMM-high half are already present;
        // only the mandatory ZMM-high clearing is observable.
        initial.xmm[0] = [0x3333, 0x4444];
        initial.ymm_high[0] = [0; 2];
        initial.zmm_high[0] = [u64::MAX; 4];
        let (mut vcpu, _) = setup_vm(&code, Some(initial));
        run_until_hlt(&mut vcpu).unwrap();
        let actual = vcpu.get_regs().unwrap();
        assert_eq!(actual.xmm[0], [0x3333, 0x4444], "{mnemonic}: result");
        assert_eq!(actual.ymm_high[0], [0; 2], "{mnemonic}: YMM[255:128]");
        assert_eq!(actual.zmm_high[0], [0; 4], "{mnemonic}: ZMM[511:256]");
    }
}

#[test]
fn vex_chunk_vex_r_and_b_extensions_select_the_high_register_bank() {
    for (opcode, mnemonic) in [(0x18, "VINSERTF128"), (0x38, "VINSERTI128")] {
        // Independently assembled as `{mnemonic} ymm12, ymm13, xmm14, 1`.
        let code = [0xC4, 0x43, 0x15, opcode, 0xE6, 0x01, 0xF4];
        let mut initial = Registers::default();
        initial.xmm[4] = [0x4444; 2];
        initial.ymm_high[4] = [0x4444; 2];
        initial.xmm[6] = [0x6666; 2];
        initial.xmm[12] = [0xCCCC; 2];
        initial.ymm_high[12] = [0xCCCC; 2];
        initial.xmm[13] = [0x1313, 0x2323];
        initial.ymm_high[13] = [0x3333, 0x4343];
        initial.xmm[14] = [0x1414, 0x2424];
        let (mut vcpu, _) = setup_vm(&code, Some(initial));
        run_until_hlt(&mut vcpu).unwrap();
        let actual = vcpu.get_regs().unwrap();
        assert_eq!(actual.xmm[12], [0x1313, 0x2323], "{mnemonic}: YMM12 low");
        assert_eq!(
            actual.ymm_high[12],
            [0x1414, 0x2424],
            "{mnemonic}: YMM12 high"
        );
        assert_eq!(actual.xmm[4], [0x4444; 2], "{mnemonic}: unextended dst");
        assert_eq!(actual.xmm[6], [0x6666; 2], "{mnemonic}: unextended src2");
    }

    for (opcode, mnemonic) in [(0x19, "VEXTRACTF128"), (0x39, "VEXTRACTI128")] {
        // Independently assembled as `{mnemonic} xmm14, ymm13, 1`.
        let code = [0xC4, 0x43, 0x7D, opcode, 0xEE, 0x01, 0xF4];
        let mut initial = Registers::default();
        initial.ymm_high[5] = [0x5555; 2];
        initial.xmm[6] = [0x6666; 2];
        initial.xmm[13] = [0x1313, 0x2323];
        initial.ymm_high[13] = [0x3333, 0x4343];
        initial.xmm[14] = [0xEEEE; 2];
        initial.ymm_high[14] = [u64::MAX; 2];
        initial.zmm_high[14] = [u64::MAX; 4];
        let (mut vcpu, _) = setup_vm(&code, Some(initial));
        run_until_hlt(&mut vcpu).unwrap();
        let actual = vcpu.get_regs().unwrap();
        assert_eq!(actual.xmm[14], [0x3333, 0x4343], "{mnemonic}: XMM14");
        assert_eq!(actual.ymm_high[14], [0; 2], "{mnemonic}: YMM14 high");
        assert_eq!(actual.zmm_high[14], [0; 4], "{mnemonic}: ZMM14 high");
        assert_eq!(
            actual.ymm_high[5], [0x5555; 2],
            "{mnemonic}: unextended src"
        );
        assert_eq!(actual.xmm[6], [0x6666; 2], "{mnemonic}: unextended dst");
    }
}

#[test]
fn vex_chunk_memory_extract_fault_does_not_partially_commit_the_store() {
    const MEMORY_END: u64 = 16 * 1024 * 1024;
    const ADDRESS: u64 = MEMORY_END - 8;
    const BEFORE: [u8; 8] = [0xA5; 8];

    for (opcode, mnemonic) in [(0x19, "VEXTRACTF128"), (0x39, "VEXTRACTI128")] {
        let code = [0xC4, 0xE3, 0x7D, opcode, 0x00, 0x01];
        let mut initial = Registers {
            rax: ADDRESS,
            ..Registers::default()
        };
        initial.xmm[0] = [0x1111, 0x2222];
        initial.ymm_high[0] = [0x3333, 0x4444];
        initial.zmm_high[0] = [0x5555, 0x6666, 0x7777, 0x8888];
        let (mut vcpu, memory) = setup_vm_no_idt(&code, Some(initial));
        memory.write_slice(&BEFORE, GuestAddress(ADDRESS)).unwrap();
        let regs_before = vcpu.get_regs().unwrap();

        let error = vcpu
            .step()
            .expect_err("128-bit store crossing memory end must fault");
        assert!(
            error.to_string().contains("write"),
            "{mnemonic}: unexpected store fault: {error}"
        );
        let regs_after = vcpu.get_regs().unwrap();
        assert_eq!(regs_after.rip, regs_before.rip, "{mnemonic}: fault RIP");
        assert_eq!(regs_after.xmm, regs_before.xmm, "{mnemonic}: XMM state");
        assert_eq!(
            regs_after.ymm_high, regs_before.ymm_high,
            "{mnemonic}: YMM upper state"
        );
        assert_eq!(
            regs_after.zmm_high, regs_before.zmm_high,
            "{mnemonic}: ZMM upper state"
        );
        let mut after = [0_u8; 8];
        memory
            .read_slice(&mut after, GuestAddress(ADDRESS))
            .unwrap();
        assert_eq!(after, BEFORE, "{mnemonic}: partial low-qword store");
    }
}
