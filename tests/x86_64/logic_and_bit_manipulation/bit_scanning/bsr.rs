#[path = "../../common/mod.rs"]
mod common;

use common::*;
use rax::cpu::Registers;

// BSR - Bit Scan Reverse
// Searches the source operand (second operand) for the most significant set bit (1 bit).
// If a most significant 1 bit is found, its bit index is stored in the destination operand.
// The source operand can be a register or a memory location; the destination operand is a register.
// The bit index is an unsigned offset from bit 0 of the source operand.
// If the source operand is 0, the ZF flag is set, and the destination operand is undefined.
// Otherwise, the ZF flag is cleared.
//
// Opcodes:
// 0F BD /r    BSR r16, r/m16    - Bit scan reverse on r/m16
// 0F BD /r    BSR r32, r/m32    - Bit scan reverse on r/m32
// REX.W 0F BD /r BSR r64, r/m64 - Bit scan reverse on r/m64

#[test]
fn test_bsr_ax_bx_bit_0() {
    // BSR AX, BX - find most significant bit (only bit 0)
    let code = [
        0x66, 0x0f, 0xbd, 0xc3, // BSR AX, BX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0b0000_0000_0000_0001; // bit 0 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0, "AX should contain 0 (bit 0 is MSB)");
    assert!(!zf_set(regs.rflags), "ZF should be clear (source is non-zero)");
}

#[test]
fn test_bsr_ax_bx_bit_15() {
    // BSR AX, BX - find most significant bit (bit 15)
    let code = [
        0x66, 0x0f, 0xbd, 0xc3, // BSR AX, BX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x8000; // bit 15 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 15, "AX should contain 15 (bit 15 is MSB)");
    assert!(!zf_set(regs.rflags), "ZF should be clear (source is non-zero)");
}

#[test]
fn test_bsr_eax_ebx_bit_0() {
    // BSR EAX, EBX - find most significant bit (bit 0, 32-bit)
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0b0000_0001;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0, "EAX should contain 0 (bit 0 is MSB)");
    assert!(!zf_set(regs.rflags), "ZF should be clear (source is non-zero)");
}

#[test]
fn test_bsr_eax_ebx_bit_31() {
    // BSR EAX, EBX - find most significant bit (bit 31)
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x80000000; // bit 31 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 31, "EAX should contain 31 (bit 31 is MSB)");
    assert!(!zf_set(regs.rflags), "ZF should be clear (source is non-zero)");
}

#[test]
fn test_bsr_rax_rbx_bit_0() {
    // BSR RAX, RBX - find most significant bit (bit 0, 64-bit)
    let code = [
        0x48, 0x0f, 0xbd, 0xc3, // BSR RAX, RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0b0000_0001;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "RAX should contain 0 (bit 0 is MSB)");
    assert!(!zf_set(regs.rflags), "ZF should be clear (source is non-zero)");
}

#[test]
fn test_bsr_rax_rbx_bit_63() {
    // BSR RAX, RBX - find most significant bit (bit 63)
    let code = [
        0x48, 0x0f, 0xbd, 0xc3, // BSR RAX, RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x8000_0000_0000_0000; // bit 63 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 63, "RAX should contain 63 (bit 63 is MSB)");
    assert!(!zf_set(regs.rflags), "ZF should be clear (source is non-zero)");
}

#[test]
fn test_bsr_zero_source() {
    // BSR with zero source sets ZF
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0; // zero source
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(zf_set(regs.rflags), "ZF should be set (source is zero)");
}

#[test]
fn test_bsr_multiple_bits_finds_highest() {
    // BSR should find the highest set bit when multiple are set
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0b1010_1000; // bits 3, 5, 7 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 7, "EAX should contain 7 (highest bit set)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_all_bits_set() {
    // BSR with all bits set should find highest bit
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 31, "EAX should contain 31 (bit 31 is highest)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_alternating_bits() {
    // BSR with alternating pattern 1010...1010
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xAAAAAAAA; // 1010...1010 (bits 1,3,5,7,... set)
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 31, "EAX should contain 31 (highest bit set)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_alternating_bits_inverted() {
    // BSR with alternating pattern 0101...0101
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x55555555; // 0101...0101 (bits 0,2,4,6,... set)
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 30, "EAX should contain 30 (highest bit set)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_single_bit_positions() {
    // Test each individual bit position
    for bit_pos in 0..32 {
        let code = [
            0x0f, 0xbd, 0xc3, // BSR EAX, EBX
            0xf4,
        ];
        let mut regs = Registers::default();
        regs.rbx = 1u64 << bit_pos;
        let (mut vcpu, _) = setup_vm(&code, Some(regs));
        let regs = run_until_hlt(&mut vcpu).unwrap();

        assert_eq!(regs.rax & 0xFFFFFFFF, bit_pos as u64, "EAX should contain {} for bit {}", bit_pos, bit_pos);
        assert!(!zf_set(regs.rflags), "ZF should be clear for bit {}", bit_pos);
    }
}

#[test]
fn test_bsr_with_extended_registers() {
    // BSR R8D, R9D - test with extended registers
    let code = [
        0x45, 0x0f, 0xbd, 0xc1, // BSR R8D, R9D
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r9 = 0b0000_1111; // bits 0-3 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8 & 0xFFFFFFFF, 3, "R8D should contain 3");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_r15() {
    // BSR with R15
    let code = [
        0x4d, 0x0f, 0xbd, 0xff, // BSR R15, R15
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r15 = 0x1_0000_0000; // bit 32 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r15, 32, "R15 should contain 32");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_mem16() {
    // BSR AX, [mem]
    let code = [
        0x66, 0x0f, 0xbd, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // BSR AX, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u16(&mem, 0x0100); // bit 8 set
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 8, "AX should contain 8");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_mem32() {
    // BSR EAX, [mem]
    let code = [
        0x0f, 0xbd, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // BSR EAX, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0x00010000); // bit 16 set
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 16, "EAX should contain 16");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_mem64() {
    // BSR RAX, [mem]
    let code = [
        0x48, 0x0f, 0xbd, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // BSR RAX, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0x100_0000_0000); // bit 40 set
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 40, "RAX should contain 40");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_mem_zero() {
    // BSR with zero in memory
    let code = [
        0x0f, 0xbd, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // BSR EAX, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0); // zero
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(zf_set(regs.rflags), "ZF should be set (memory is zero)");
}

#[test]
fn test_bsr_high_bits_64() {
    // BSR with high bits in 64-bit operand
    let code = [
        0x48, 0x0f, 0xbd, 0xc3, // BSR RAX, RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x0800_0000_0000_0000; // bit 59 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 59, "RAX should contain 59");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_mixed_high_low() {
    // BSR with both high and low bits, should find high
    let code = [
        0x48, 0x0f, 0xbd, 0xc3, // BSR RAX, RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x8000_0000_0000_0100; // bits 8 and 63 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 63, "RAX should contain 63 (higher bit)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_sparse_pattern() {
    // BSR with sparse bit pattern
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x80001000; // bits 12 and 31 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 31, "EAX should contain 31 (higher bit)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_consecutive_bits() {
    // BSR with consecutive bits set
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x00FF0000; // bits 16-23 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 23, "EAX should contain 23 (highest of consecutive bits)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_preserves_source() {
    // BSR should not modify source register
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x12345678, "EBX should be unchanged");
}

#[test]
fn test_bsr_dest_equals_source() {
    // BSR where destination equals source
    let code = [
        0x0f, 0xbd, 0xc0, // BSR EAX, EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0b0000_1111; // bits 0-3 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 3, "EAX should contain 3");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_power_of_two() {
    // BSR with powers of two
    for i in 0..32 {
        let code = [
            0x0f, 0xbd, 0xc3, // BSR EAX, EBX
            0xf4,
        ];
        let mut regs = Registers::default();
        regs.rbx = 1u64 << i;
        let (mut vcpu, _) = setup_vm(&code, Some(regs));
        let regs = run_until_hlt(&mut vcpu).unwrap();

        assert_eq!(regs.rax & 0xFFFFFFFF, i as u64, "EAX should contain {} for 2^{}", i, i);
    }
}

#[test]
fn test_bsr_leading_zeros() {
    // BSR effectively finds highest set bit (inverse of leading zeros)
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x00000FFF; // bits 0-11 set, 20 leading zeros
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 11, "EAX should contain 11 (highest bit set)");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_sign_bit() {
    // BSR with sign bit set (treated as unsigned)
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x80000000; // sign bit set (bit 31)
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 31, "EAX should contain 31");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_vs_bsf_comparison() {
    // Compare BSR and BSF results on same value
    // BSR should find highest, BSF should find lowest
    let code_bsr = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x80000001; // bits 0 and 31 set
    let (mut vcpu, _) = setup_vm(&code_bsr, Some(regs));
    let regs_bsr = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs_bsr.rax & 0xFFFFFFFF, 31, "BSR should find bit 31");

    // Compare with BSF which would find bit 0
    let code_bsf = [
        0x0f, 0xbc, 0xc3, // BSF EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x80000001;
    let (mut vcpu, _) = setup_vm(&code_bsf, Some(regs));
    let regs_bsf = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs_bsf.rax & 0xFFFFFFFF, 0, "BSF should find bit 0");
}

#[test]
fn test_bsr_low_byte() {
    // BSR with only low byte set
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x000000FF; // bits 0-7 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 7, "EAX should contain 7");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_bsr_high_byte() {
    // BSR with only high byte set (32-bit)
    let code = [
        0x0f, 0xbd, 0xc3, // BSR EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xFF000000; // bits 24-31 set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 31, "EAX should contain 31");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}
