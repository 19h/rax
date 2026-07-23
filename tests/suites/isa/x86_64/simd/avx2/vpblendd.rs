use crate::common::*;
use rax::vm::vcpu::Registers;
use vm_memory::{Bytes, GuestAddress};

// VPBLENDD - Blend Packed Dwords Using Immediate Mask (AVX2)
//
// Conditionally blends dwords from two source operands into destination
// based on an immediate 8-bit control mask. Each bit in the mask controls
// one dword element: 0 = select from first source, 1 = select from second source.
//
// For YMM registers (256-bit), 8 bits control 8 dwords.
//
// Opcodes (AVX2 - 256-bit YMM):
// VEX.256.66.0F3A.W0 02 /r ib       VPBLENDD ymm1, ymm2, ymm3/m256, imm8

const ALIGNED_ADDR: u64 = 0x3000;

fn blend_dwords(first: [u64; 4], second: [u64; 4], imm: u8) -> [u64; 4] {
    let mut result = [0_u64; 4];
    for lane in 0..8 {
        let word = lane / 2;
        let shift = (lane % 2) * 32;
        let source = if (imm >> lane) & 1 == 0 {
            first[word]
        } else {
            second[word]
        };
        result[word] |= ((source >> shift) & 0xFFFF_FFFF) << shift;
    }
    result
}

fn ymm(regs: &Registers, index: usize) -> [u64; 4] {
    [
        regs.xmm[index][0],
        regs.xmm[index][1],
        regs.ymm_high[index][0],
        regs.ymm_high[index][1],
    ]
}

fn set_ymm(regs: &mut Registers, index: usize, value: [u64; 4]) {
    regs.xmm[index] = [value[0], value[1]];
    regs.ymm_high[index] = [value[2], value[3]];
}

#[test]
fn vpblendd_commits_exact_bits_for_widths_aliases_and_unaligned_memory() {
    let first = [
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x8000_0000_7FFF_FFFF,
        0x7FC0_1234_FF80_0001,
    ];
    let second = [
        0xAAAA_AAAA_5555_5555,
        0x0000_0000_FFFF_FFFF,
        0x1357_9BDF_2468_ACE0,
        0xFFFF_FFFE_0000_0001,
    ];

    // Destination aliases source 1. Reading all old lanes before the write is
    // observable for an alternating immediate mask.
    let mut initial = Registers::default();
    set_ymm(&mut initial, 0, first);
    set_ymm(&mut initial, 2, second);
    initial.zmm_high[0] = [u64::MAX; 4];
    let code = [0xC4, 0xE3, 0x7D, 0x02, 0xC2, 0xA5, 0xF4];
    let (mut vcpu, _) = setup_vm(&code, Some(initial));
    run_until_hlt(&mut vcpu).unwrap();
    let actual = vcpu.get_regs().unwrap();
    assert_eq!(ymm(&actual, 0), blend_dwords(first, second, 0xA5));
    assert_eq!(actual.zmm_high[0], [0; 4]);

    // A VEX write clears ZMM[511:256] even when its low 256-bit result is
    // bit-identical to the old destination and therefore invisible to a
    // value-difference-based write detector.
    let mut initial = Registers::default();
    set_ymm(&mut initial, 0, first);
    initial.zmm_high[0] = [u64::MAX; 4];
    let code = [0xC4, 0xE3, 0x7D, 0x02, 0xC0, 0x00, 0xF4];
    let (mut vcpu, _) = setup_vm(&code, Some(initial));
    run_until_hlt(&mut vcpu).unwrap();
    let actual = vcpu.get_regs().unwrap();
    assert_eq!(ymm(&actual, 0), first);
    assert_eq!(actual.zmm_high[0], [0; 4]);

    // VEX.128 consumes only imm8[3:0] and clears the upper 128 bits.
    let mut initial = Registers::default();
    set_ymm(&mut initial, 1, first);
    set_ymm(&mut initial, 2, second);
    set_ymm(&mut initial, 0, [u64::MAX; 4]);
    initial.zmm_high[0] = [u64::MAX; 4];
    let code = [0xC4, 0xE3, 0x71, 0x02, 0xC2, 0xFA, 0xF4];
    let (mut vcpu, _) = setup_vm(&code, Some(initial));
    run_until_hlt(&mut vcpu).unwrap();
    let actual = ymm(&vcpu.get_regs().unwrap(), 0);
    let expected = blend_dwords(first, second, 0x0A);
    assert_eq!(&actual[..2], &expected[..2]);
    assert_eq!(&actual[2..], &[0, 0]);
    assert_eq!(vcpu.get_regs().unwrap().zmm_high[0], [0; 4]);

    // The VEX memory form is explicitly unaligned-capable.
    let address = ALIGNED_ADDR + 1;
    let mut initial = Registers::default();
    initial.rax = address;
    set_ymm(&mut initial, 1, first);
    let code = [0xC4, 0xE3, 0x75, 0x02, 0x00, 0x5A, 0xF4];
    let (mut vcpu, memory) = setup_vm(&code, Some(initial));
    memory
        .write_slice(
            &second
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect::<Vec<_>>(),
            GuestAddress(address),
        )
        .unwrap();
    run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(
        ymm(&vcpu.get_regs().unwrap(), 0),
        blend_dwords(first, second, 0x5A)
    );
}

#[test]
fn vpblendd_w1_raises_ud_without_fetching_operands_or_committing_state() {
    let mut initial = Registers::default();
    initial.rax = 0x0123_4567_89AB_CDEF;
    initial.rflags = 0x2 | 0x8D5;
    set_ymm(&mut initial, 0, [1, 2, 3, 4]);
    set_ymm(&mut initial, 1, [5, 6, 7, 8]);

    // No ModR/M or immediate follows the opcode. W=1 is reserved and must be
    // recognized at the opcode frontier on both the cold and cached paths.
    let code = [0xC4, 0xE3, 0xF5, 0x02];
    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));
    for path in ["cold decode", "decode-cache hit"] {
        let before = vcpu.get_regs().unwrap();
        let error = match vcpu.step() {
            Err(error) => error,
            Ok(exit) => panic!("{path}: expected #UD, got {exit:?}"),
        };
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{path}: expected #UD delivery failure, got {error}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, before.rip, "{path}: fault RIP");
        assert_eq!(after.rax, before.rax, "{path}: RAX");
        assert_eq!(after.rflags, before.rflags, "{path}: RFLAGS");
        assert_eq!(after.xmm, before.xmm, "{path}: XMM state");
        assert_eq!(after.ymm_high, before.ymm_high, "{path}: YMM upper state");
        assert_eq!(after.zmm_high, before.zmm_high, "{path}: ZMM upper state");
    }
}

#[test]
fn vpblendd_ymm_memory_fault_does_not_partially_commit_destination() {
    const MEMORY_END: u64 = 16 * 1024 * 1024;
    let mut initial = Registers {
        rax: MEMORY_END - 16,
        ..Registers::default()
    };
    set_ymm(&mut initial, 0, [1, 2, 3, 4]);
    set_ymm(&mut initial, 1, [5, 6, 7, 8]);
    initial.zmm_high[0] = [9, 10, 11, 12];

    // The first 16 bytes of the 32-byte source are mapped; the second 16 bytes
    // are outside the fixture. No destination lane may commit before the full
    // source operand has been read successfully.
    let code = [0xC4, 0xE3, 0x75, 0x02, 0x00, 0xFF];
    let (mut vcpu, memory) = setup_vm_no_idt(&code, Some(initial));
    memory
        .write_slice(&[0xAA; 16], GuestAddress(MEMORY_END - 16))
        .unwrap();
    let before = vcpu.get_regs().unwrap();
    let error = vcpu
        .step()
        .expect_err("32-byte source must cross memory end");
    assert!(
        error.to_string().contains("failed to read at 0x1000000"),
        "unexpected boundary fault: {error}"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rip, before.rip, "fault RIP");
    assert_eq!(after.xmm, before.xmm, "XMM state");
    assert_eq!(after.ymm_high, before.ymm_high, "YMM upper state");
    assert_eq!(after.zmm_high, before.zmm_high, "ZMM upper state");
}

// ============================================================================
// VPBLENDD Tests - Blend 8 Dwords Using Immediate Mask (256-bit)
// ============================================================================

#[test]
fn test_vpblendd_ymm0_ymm1_ymm2_0x00() {
    // VPBLENDD YMM0, YMM1, YMM2, 0x00 - select all from YMM1
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x00, // VPBLENDD YMM0, YMM1, YMM2, 0x00
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm0_ymm1_ymm2_0xFF() {
    // VPBLENDD YMM0, YMM1, YMM2, 0xFF - select all from YMM2
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0xFF, // VPBLENDD YMM0, YMM1, YMM2, 0xFF
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm0_ymm1_ymm2_0xAA() {
    // VPBLENDD YMM0, YMM1, YMM2, 0xAA - alternating selection (10101010)
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0xAA, // VPBLENDD YMM0, YMM1, YMM2, 0xAA
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm0_ymm1_ymm2_0x55() {
    // VPBLENDD YMM0, YMM1, YMM2, 0x55 - alternating selection (01010101)
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x55, // VPBLENDD YMM0, YMM1, YMM2, 0x55
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm0_ymm1_ymm2_0x0F() {
    // VPBLENDD YMM0, YMM1, YMM2, 0x0F - lower 4 from YMM2, upper 4 from YMM1
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x0F, // VPBLENDD YMM0, YMM1, YMM2, 0x0F
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm0_ymm1_ymm2_0xF0() {
    // VPBLENDD YMM0, YMM1, YMM2, 0xF0 - lower 4 from YMM1, upper 4 from YMM2
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0xF0, // VPBLENDD YMM0, YMM1, YMM2, 0xF0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm3_ymm4_ymm5_0x3C() {
    let code = [
        0xc4, 0xe3, 0x5d, 0x02, 0xdd, 0x3C, // VPBLENDD YMM3, YMM4, YMM5, 0x3C
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm6_ymm7_ymm0_0xC3() {
    let code = [
        0xc4, 0xe3, 0x45, 0x02, 0xf0, 0xC3, // VPBLENDD YMM6, YMM7, YMM0, 0xC3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm9_ymm10_ymm11_0x81() {
    let code = [
        0xc4, 0x43, 0x2d, 0x02, 0xcb, 0x81, // VPBLENDD YMM9, YMM10, YMM11, 0x81
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm12_ymm13_ymm14_0x42() {
    let code = [
        0xc4, 0x43, 0x15, 0x02, 0xe6, 0x42, // VPBLENDD YMM12, YMM13, YMM14, 0x42
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm15_ymm0_ymm1_0x24() {
    let code = [
        0xc4, 0x63, 0x7d, 0x02, 0xf9, 0x24, // VPBLENDD YMM15, YMM0, YMM1, 0x24
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// VPBLENDD Tests - Blend with Memory Operand
// ============================================================================

#[test]
fn test_vpblendd_ymm0_ymm1_mem_0xAA() {
    // VPBLENDD YMM0, YMM1, [memory], 0xAA
    let code = [0x48, 0xb8];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xc4, 0xe3, 0x75, 0x02, 0x00, 0xAA, // VPBLENDD YMM0, YMM1, [RAX], 0xAA
        0xf4, // HLT
    ]);

    let (mut vcpu, mem) = setup_vm(&full_code, None);
    let data = vec![
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
        0x1F, 0x20,
    ];
    mem.write_slice(&data, GuestAddress(ALIGNED_ADDR)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm3_ymm4_mem_0x55() {
    let code = [0x48, 0xb8];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xc4, 0xe3, 0x5d, 0x02, 0x18, 0x55, // VPBLENDD YMM3, YMM4, [RAX], 0x55
        0xf4, // HLT
    ]);

    let (mut vcpu, mem) = setup_vm(&full_code, None);
    let data = vec![
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF,
    ];
    mem.write_slice(&data, GuestAddress(ALIGNED_ADDR)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm6_ymm7_mem_0x0F() {
    let code = [0x48, 0xb8];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xc4, 0xe3, 0x45, 0x02, 0x30, 0x0F, // VPBLENDD YMM6, YMM7, [RAX], 0x0F
        0xf4, // HLT
    ]);

    let (mut vcpu, mem) = setup_vm(&full_code, None);
    let data = vec![
        0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
        0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
        0xAA, 0xAA,
    ];
    mem.write_slice(&data, GuestAddress(ALIGNED_ADDR)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm9_ymm10_mem_0xF0() {
    let code = [0x48, 0xb8];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xc4, 0x63, 0x2d, 0x02, 0x08, 0xF0, // VPBLENDD YMM9, YMM10, [RAX], 0xF0
        0xf4, // HLT
    ]);

    let (mut vcpu, mem) = setup_vm(&full_code, None);
    let data = vec![
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    mem.write_slice(&data, GuestAddress(ALIGNED_ADDR)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_ymm12_ymm13_mem_0x3C() {
    let code = [0x48, 0xb8];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xc4, 0x63, 0x15, 0x02, 0x20, 0x3C, // VPBLENDD YMM12, YMM13, [RAX], 0x3C
        0xf4, // HLT
    ]);

    let (mut vcpu, mem) = setup_vm(&full_code, None);
    let data = vec![
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99,
    ];
    mem.write_slice(&data, GuestAddress(ALIGNED_ADDR)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// Comprehensive tests
// ============================================================================

#[test]
fn test_vpblendd_all_masks() {
    // Test various mask patterns
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x00, // VPBLENDD YMM0, YMM1, YMM2, 0x00
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x01, // VPBLENDD YMM3, YMM1, YMM2, 0x01
        0xc4, 0xe3, 0x75, 0x02, 0xe2, 0x03, // VPBLENDD YMM4, YMM1, YMM2, 0x03
        0xc4, 0xe3, 0x75, 0x02, 0xea, 0x07, // VPBLENDD YMM5, YMM1, YMM2, 0x07
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_extended_regs() {
    let code = [
        0xc4, 0x43, 0x3d, 0x02, 0xc1, 0xAA, // VPBLENDD YMM8, YMM8, YMM9, 0xAA
        0xc4, 0x43, 0x15, 0x02, 0xd5, 0x55, // VPBLENDD YMM10, YMM13, YMM13, 0x55
        0xc4, 0x43, 0x05, 0x02, 0xff, 0xF0, // VPBLENDD YMM15, YMM15, YMM15, 0xF0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_single_bit_masks() {
    // Test each bit position individually
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x01, // VPBLENDD YMM0, YMM1, YMM2, 0x01 (bit 0)
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x02, // VPBLENDD YMM3, YMM1, YMM2, 0x02 (bit 1)
        0xc4, 0xe3, 0x75, 0x02, 0xe2, 0x04, // VPBLENDD YMM4, YMM1, YMM2, 0x04 (bit 2)
        0xc4, 0xe3, 0x75, 0x02, 0xea, 0x08, // VPBLENDD YMM5, YMM1, YMM2, 0x08 (bit 3)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_upper_bit_masks() {
    // Test upper 4 bits
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x10, // VPBLENDD YMM0, YMM1, YMM2, 0x10 (bit 4)
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x20, // VPBLENDD YMM3, YMM1, YMM2, 0x20 (bit 5)
        0xc4, 0xe3, 0x75, 0x02, 0xe2, 0x40, // VPBLENDD YMM4, YMM1, YMM2, 0x40 (bit 6)
        0xc4, 0xe3, 0x75, 0x02, 0xea, 0x80, // VPBLENDD YMM5, YMM1, YMM2, 0x80 (bit 7)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_chain() {
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0xAA, // VPBLENDD YMM0, YMM1, YMM2, 0xAA
        0xc4, 0xe3, 0x7d, 0x02, 0xdb, 0x55, // VPBLENDD YMM3, YMM0, YMM3, 0x55
        0xc4, 0xe3, 0x65, 0x02, 0xe0, 0x0F, // VPBLENDD YMM4, YMM3, YMM0, 0x0F
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_mem_various_offsets() {
    let code = [0x48, 0xb8];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xc4, 0xe3, 0x75, 0x02, 0x00, 0xAA, // VPBLENDD YMM0, YMM1, [RAX], 0xAA
        0xc4, 0xe3, 0x75, 0x02, 0x50, 0x20, 0x55, // VPBLENDD YMM2, YMM1, [RAX+32], 0x55
        0xc4, 0xe3, 0x75, 0x02, 0x60, 0x40, 0xF0, // VPBLENDD YMM4, YMM1, [RAX+64], 0xF0
        0xf4, // HLT
    ]);

    let (mut vcpu, mem) = setup_vm(&full_code, None);
    let data = vec![
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    ];
    mem.write_slice(&data, GuestAddress(ALIGNED_ADDR)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_same_src_dst() {
    // Destination same as first source
    let code = [
        0xc4, 0xe3, 0x7d, 0x02, 0xc1, 0xAA, // VPBLENDD YMM0, YMM0, YMM1, 0xAA
        0xc4, 0xe3, 0x75, 0x02, 0xd2, 0x55, // VPBLENDD YMM2, YMM1, YMM2, 0x55
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_sequential_patterns() {
    // Sequential mask patterns 0x00 through 0xFF
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x00, // VPBLENDD YMM0, YMM1, YMM2, 0x00
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x11, // VPBLENDD YMM3, YMM1, YMM2, 0x11
        0xc4, 0xe3, 0x75, 0x02, 0xe2, 0x22, // VPBLENDD YMM4, YMM1, YMM2, 0x22
        0xc4, 0xe3, 0x75, 0x02, 0xea, 0x33, // VPBLENDD YMM5, YMM1, YMM2, 0x33
        0xc4, 0xe3, 0x75, 0x02, 0xf2, 0x44, // VPBLENDD YMM6, YMM1, YMM2, 0x44
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_complement_masks() {
    // Test complementary masks
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0xAA, // VPBLENDD YMM0, YMM1, YMM2, 0xAA
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x55, // VPBLENDD YMM3, YMM1, YMM2, 0x55
        0xc4, 0xe3, 0x75, 0x02, 0xe2, 0x0F, // VPBLENDD YMM4, YMM1, YMM2, 0x0F
        0xc4, 0xe3, 0x75, 0x02, 0xea, 0xF0, // VPBLENDD YMM5, YMM1, YMM2, 0xF0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_three_way_pattern() {
    // Blend patterns that create specific dword layouts
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x18, // VPBLENDD YMM0, YMM1, YMM2, 0x18 (00011000)
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x81, // VPBLENDD YMM3, YMM1, YMM2, 0x81 (10000001)
        0xc4, 0xe3, 0x75, 0x02, 0xe2, 0x42, // VPBLENDD YMM4, YMM1, YMM2, 0x42 (01000010)
        0xc4, 0xe3, 0x75, 0x02, 0xea, 0x24, // VPBLENDD YMM5, YMM1, YMM2, 0x24 (00100100)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_low_lane_only() {
    // Blend only lower 128-bit lane
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2,
        0x0F, // VPBLENDD YMM0, YMM1, YMM2, 0x0F (lower 4 dwords)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_high_lane_only() {
    // Blend only upper 128-bit lane
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2,
        0xF0, // VPBLENDD YMM0, YMM1, YMM2, 0xF0 (upper 4 dwords)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_cross_lane_pattern() {
    // Pattern that blends across lanes
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0x99, // VPBLENDD YMM0, YMM1, YMM2, 0x99 (10011001)
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x66, // VPBLENDD YMM3, YMM1, YMM2, 0x66 (01100110)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_corner_case_masks() {
    // Test specific corner cases
    let code = [
        0xc4, 0xe3, 0x75, 0x02, 0xc2, 0xC0, // VPBLENDD YMM0, YMM1, YMM2, 0xC0 (11000000)
        0xc4, 0xe3, 0x75, 0x02, 0xda, 0x03, // VPBLENDD YMM3, YMM1, YMM2, 0x03 (00000011)
        0xc4, 0xe3, 0x75, 0x02, 0xe2, 0xE1, // VPBLENDD YMM4, YMM1, YMM2, 0xE1 (11100001)
        0xc4, 0xe3, 0x75, 0x02, 0xea, 0x1E, // VPBLENDD YMM5, YMM1, YMM2, 0x1E (00011110)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_mem_with_sib() {
    let code = [0x48, 0xb8];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x48, 0x31, 0xdb, // XOR RBX, RBX (RBX = 0)
        0xc4, 0xe3, 0x75, 0x02, 0x04, 0x18, 0xAA, // VPBLENDD YMM0, YMM1, [RAX + RBX], 0xAA
        0xf4, // HLT
    ]);

    let (mut vcpu, mem) = setup_vm(&full_code, None);
    let data = vec![
        0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A,
        0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A,
        0x5A, 0x5A,
    ];
    mem.write_slice(&data, GuestAddress(ALIGNED_ADDR)).unwrap();
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_vpblendd_all_reg_combos() {
    // Test different register combinations
    let code = [
        0xc4, 0xe3, 0x7d, 0x02, 0xc2, 0xAA, // VPBLENDD YMM0, YMM0, YMM2, 0xAA
        0xc4, 0xe3, 0x6d, 0x02, 0xdb, 0x55, // VPBLENDD YMM3, YMM2, YMM3, 0x55
        0xc4, 0xe3, 0x5d, 0x02, 0xe5, 0xF0, // VPBLENDD YMM4, YMM4, YMM5, 0xF0
        0xc4, 0xe3, 0x4d, 0x02, 0xf7, 0x0F, // VPBLENDD YMM6, YMM6, YMM7, 0x0F
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}
