// Module path for tests run via x86_64.rs
use crate::common::*;
use rax::vm::vcpu::Registers;

// BSWAP - Byte Swap
// Reverses the byte order of a 32-bit or 64-bit register.
// For 32-bit: bytes 0,1,2,3 become 3,2,1,0
// For 64-bit: bytes 0-7 become 7-0
// 16-bit results are architecturally undefined. RAX's deterministic profile
// preserves the complete destination register.
//
// Opcodes:
// 0F C8+rd    BSWAP r32    - Reverse byte order of r32
// REX.W 0F C8+rd BSWAP r64 - Reverse byte order of r64

#[test]
fn test_bswap_eax_basic() {
    // BSWAP EAX - basic 32-bit swap
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x78563412,
        "EAX bytes should be reversed"
    );
}

#[test]
fn test_bswap_ebx() {
    // BSWAP EBX
    let code = [
        0x0f, 0xcb, // BSWAP EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xAABBCCDD;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rbx & 0xFFFFFFFF,
        0xDDCCBBAA,
        "EBX bytes should be reversed"
    );
}

#[test]
fn test_bswap_ecx() {
    // BSWAP ECX
    let code = [
        0x0f, 0xc9, // BSWAP ECX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rcx = 0x01020304;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rcx & 0xFFFFFFFF,
        0x04030201,
        "ECX bytes should be reversed"
    );
}

#[test]
fn test_bswap_edx() {
    // BSWAP EDX
    let code = [
        0x0f, 0xca, // BSWAP EDX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rdx = 0xDEADBEEF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rdx & 0xFFFFFFFF,
        0xEFBEADDE,
        "EDX bytes should be reversed"
    );
}

#[test]
fn test_bswap_esi() {
    // BSWAP ESI
    let code = [
        0x0f, 0xce, // BSWAP ESI
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rsi = 0x11223344;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rsi & 0xFFFFFFFF,
        0x44332211,
        "ESI bytes should be reversed"
    );
}

#[test]
fn test_bswap_edi() {
    // BSWAP EDI
    let code = [
        0x0f, 0xcf, // BSWAP EDI
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rdi = 0xAABBCCDD;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rdi & 0xFFFFFFFF,
        0xDDCCBBAA,
        "EDI bytes should be reversed"
    );
}

#[test]
fn test_bswap_rax_64bit() {
    // BSWAP RAX (64-bit)
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x0123456789ABCDEF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0xEFCDAB8967452301, "RAX bytes should be reversed");
}

#[test]
fn test_bswap_rbx_64bit() {
    // BSWAP RBX (64-bit)
    let code = [
        0x48, 0x0f, 0xcb, // BSWAP RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xFEDCBA9876543210;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x1032547698BADCFE, "RBX bytes should be reversed");
}

#[test]
fn test_bswap_rcx_64bit() {
    // BSWAP RCX (64-bit)
    let code = [
        0x48, 0x0f, 0xc9, // BSWAP RCX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rcx = 0x0011223344556677;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0x7766554433221100, "RCX bytes should be reversed");
}

#[test]
fn test_bswap_rdx_64bit() {
    // BSWAP RDX (64-bit)
    let code = [
        0x48, 0x0f, 0xca, // BSWAP RDX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rdx = 0xA0B0C0D0E0F0A0B0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdx, 0xB0A0F0E0D0C0B0A0, "RDX bytes should be reversed");
}

#[test]
fn test_bswap_all_zeros_32bit() {
    // BSWAP with all zeros (32-bit)
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x00000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x00000000, "EAX should remain zero");
}

#[test]
fn test_bswap_all_zeros_64bit() {
    // BSWAP with all zeros (64-bit)
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x0000000000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x0000000000000000, "RAX should remain zero");
}

#[test]
fn test_bswap_all_ones_32bit() {
    // BSWAP with all ones (32-bit)
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0xFFFFFFFF,
        "EAX should remain all ones"
    );
}

#[test]
fn test_bswap_all_ones_64bit() {
    // BSWAP with all ones (64-bit)
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFFFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0xFFFFFFFFFFFFFFFF, "RAX should remain all ones");
}

#[test]
fn test_bswap_alternating_bytes_32bit() {
    // BSWAP with alternating byte pattern (32-bit)
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xAA55AA55;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x55AA55AA,
        "EAX bytes should be reversed"
    );
}

#[test]
fn test_bswap_sequential_bytes_32bit() {
    // BSWAP with sequential byte values (32-bit)
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x00010203;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x03020100,
        "EAX bytes should be reversed"
    );
}

#[test]
fn test_bswap_with_r8d() {
    // BSWAP R8D (extended register 32-bit)
    let code = [
        0x41, 0x0f, 0xc8, // BSWAP R8D
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r8 = 0x11223344;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.r8 & 0xFFFFFFFF,
        0x44332211,
        "R8D bytes should be reversed"
    );
}

#[test]
fn test_bswap_with_r9d() {
    // BSWAP R9D
    let code = [
        0x41, 0x0f, 0xc9, // BSWAP R9D
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r9 = 0xABCDEF01;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.r9 & 0xFFFFFFFF,
        0x01EFCDAB,
        "R9D bytes should be reversed"
    );
}

#[test]
fn test_bswap_with_r15d() {
    // BSWAP R15D
    let code = [
        0x41, 0x0f, 0xcf, // BSWAP R15D
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r15 = 0xDEADBEEF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.r15 & 0xFFFFFFFF,
        0xEFBEADDE,
        "R15D bytes should be reversed"
    );
}

#[test]
fn test_bswap_r8_64bit() {
    // BSWAP R8 (extended register 64-bit)
    let code = [
        0x49, 0x0f, 0xc8, // BSWAP R8
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r8 = 0x0011223344556677;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8, 0x7766554433221100, "R8 bytes should be reversed");
}

#[test]
fn test_bswap_r15_64bit() {
    // BSWAP R15 (64-bit)
    let code = [
        0x49, 0x0f, 0xcf, // BSWAP R15
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r15 = 0x0011223344556677;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r15, 0x7766554433221100, "R15 bytes should be reversed");
}

#[test]
fn test_bswap_idempotent() {
    // Double BSWAP returns to original
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0x0f, 0xc8, // BSWAP EAX again
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x12345678,
        "EAX should return to original after double BSWAP"
    );
}

#[test]
fn test_bswap_endianness_conversion() {
    // BSWAP for endianness conversion
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x80000000; // Big-endian representation
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x00000080,
        "Endianness should be converted"
    );
}

#[test]
fn test_bswap_preserves_other_registers() {
    // BSWAP should not affect other registers
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    regs.rbx = 0xAABBCCDD;
    regs.rcx = 0x11111111;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFFFFFF, 0xAABBCCDD, "EBX should be unchanged");
    assert_eq!(regs.rcx & 0xFFFFFFFF, 0x11111111, "ECX should be unchanged");
}

#[test]
fn test_bswap_single_byte_values() {
    // Test with only one byte set
    let test_cases = vec![
        (0x000000FF, 0xFF000000),
        (0x0000FF00, 0x00FF0000),
        (0x00FF0000, 0x0000FF00),
        (0xFF000000, 0x000000FF),
    ];

    for (input, expected) in test_cases {
        let code = [
            0x0f, 0xc8, // BSWAP EAX
            0xf4,
        ];
        let mut regs = Registers::default();
        regs.rax = input;
        let (mut vcpu, _) = setup_vm(&code, Some(regs));
        let regs = run_until_hlt(&mut vcpu).unwrap();

        assert_eq!(
            regs.rax & 0xFFFFFFFF,
            expected,
            "BSWAP(0x{:08X}) should be 0x{:08X}",
            input,
            expected
        );
    }
}

#[test]
fn test_bswap_network_byte_order() {
    // Network byte order (big-endian) to host byte order (little-endian)
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x00000100; // Network byte order for 256
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x00010000,
        "Should convert network to host byte order"
    );
}

#[test]
fn test_bswap_64bit_full_reversal() {
    // Test full 64-bit byte reversal
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    // This value when BSWAP'd: 0x0102030405060708 -> 0x0807060504030201
    regs.rax = 0x0102030405060708;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax, 0x0807060504030201,
        "64-bit value should be fully byte-reversed"
    );
}

#[test]
fn test_bswap_64bit_asymmetric() {
    // Asymmetric 64-bit value
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x0102030405060708;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x0807060504030201, "RAX bytes should be reversed");
}

#[test]
fn test_bswap_high_low_word_swap() {
    // BSWAP effectively swaps high and low words with byte reversal
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12340000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x00003412,
        "High word moves to low with byte swap"
    );
}

#[test]
fn test_bswap_ascii_to_reversed() {
    // BSWAP with ASCII-like values
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x41424344; // "ABCD" in ASCII
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x44434241,
        "ASCII bytes should be reversed"
    );
}

#[test]
fn test_bswap_powers_of_256() {
    // Test with powers of 256
    let test_cases = vec![
        (0x00000001, 0x01000000), // 256^0
        (0x00000100, 0x00010000), // 256^1
        (0x00010000, 0x00000100), // 256^2
        (0x01000000, 0x00000001), // 256^3
    ];

    for (input, expected) in test_cases {
        let code = [
            0x0f, 0xc8, // BSWAP EAX
            0xf4,
        ];
        let mut regs = Registers::default();
        regs.rax = input;
        let (mut vcpu, _) = setup_vm(&code, Some(regs));
        let regs = run_until_hlt(&mut vcpu).unwrap();

        assert_eq!(
            regs.rax & 0xFFFFFFFF,
            expected,
            "BSWAP(0x{:08X}) should be 0x{:08X}",
            input,
            expected
        );
    }
}

#[test]
fn test_bswap_sequential_registers_32bit() {
    // BSWAP multiple registers in sequence (32-bit)
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0x0f, 0xcb, // BSWAP EBX
        0x0f, 0xc9, // BSWAP ECX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x11111111;
    regs.rbx = 0x22222222;
    regs.rcx = 0x33333333;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x11111111, "EAX should be swapped");
    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x22222222, "EBX should be swapped");
    assert_eq!(regs.rcx & 0xFFFFFFFF, 0x33333333, "ECX should be swapped");
}

#[test]
fn test_bswap_64bit_upper_lower_independence() {
    // Upper and lower 32 bits swap independently
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678_9ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax, 0xF0DEBC9A_78563412,
        "Both halves should be byte-swapped and position-swapped"
    );
}

#[test]
fn test_bswap_pattern_mirror() {
    // Test pattern that mirrors around byte 2
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12341234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x34123412,
        "Mirrored pattern should swap correctly"
    );
}

#[test]
fn test_bswap_64bit_pattern_mirror() {
    // Test pattern that mirrors around byte 4
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x1234567812345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax, 0x7856341278563412,
        "64-bit mirrored pattern should swap correctly"
    );
}

#[test]
fn test_bswap_max_32bit_value() {
    // Test with maximum 32-bit signed value
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x7FFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0xFFFFFF7F,
        "Maximum 32-bit signed value swapped"
    );
}

#[test]
fn test_bswap_min_32bit_value() {
    // Test with minimum 32-bit signed value
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x80000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0x00000080,
        "Minimum 32-bit signed value swapped"
    );
}

#[test]
fn test_bswap_64bit_max_signed_value() {
    // Test with maximum 64-bit signed value
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x7FFFFFFFFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax, 0xFFFFFFFFFFFFFF7F,
        "Maximum 64-bit signed value swapped"
    );
}

#[test]
fn test_bswap_64bit_min_signed_value() {
    // Test with minimum 64-bit signed value
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x8000000000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax, 0x0000000000000080,
        "Minimum 64-bit signed value swapped"
    );
}

#[test]
fn test_bswap_rsi_64bit() {
    // BSWAP RSI (64-bit)
    let code = [
        0x48, 0x0f, 0xce, // BSWAP RSI
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rsi = 0x0123456789ABCDEF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi, 0xEFCDAB8967452301, "RSI bytes should be reversed");
}

#[test]
fn test_bswap_rdi_64bit() {
    // BSWAP RDI (64-bit)
    let code = [
        0x48, 0x0f, 0xcf, // BSWAP RDI
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rdi = 0xFEDCBA9876543210;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdi, 0x1032547698BADCFE, "RDI bytes should be reversed");
}

#[test]
fn test_bswap_multiple_sequential_32bit() {
    // Multiple BSWAP operations on different registers in sequence
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0x0f, 0xcb, // BSWAP EBX
        0x0f, 0xc9, // BSWAP ECX
        0x0f, 0xca, // BSWAP EDX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xAABBCCDD;
    regs.rbx = 0x11223344;
    regs.rcx = 0xFF00FF00;
    regs.rdx = 0x00FF00FF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0xDDCCBBAA, "EAX should be swapped");
    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x44332211, "EBX should be swapped");
    assert_eq!(regs.rcx & 0xFFFFFFFF, 0x00FF00FF, "ECX should be swapped");
    assert_eq!(regs.rdx & 0xFFFFFFFF, 0xFF00FF00, "EDX should be swapped");
}

#[test]
fn test_bswap_multiple_sequential_64bit() {
    // Multiple BSWAP operations on 64-bit registers in sequence
    let code = [
        0x48, 0x0f, 0xc8, // BSWAP RAX
        0x48, 0x0f, 0xcb, // BSWAP RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x0123456789ABCDEF;
    regs.rbx = 0xFEDCBA9876543210;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0xEFCDAB8967452301, "RAX should be swapped");
    assert_eq!(regs.rbx, 0x1032547698BADCFE, "RBX should be swapped");
}

#[test]
fn test_bswap_preserves_upper_32bits() {
    // BSWAP on 32-bit register doesn't affect upper 32 bits
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xDEADBEEF12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Lower 32 bits should be swapped, upper 32 bits cleared (due to 32-bit op)
    assert_eq!(
        regs.rax, 0x00000000_78563412,
        "32-bit BSWAP should clear upper bits"
    );
}

#[test]
fn test_bswap_with_does_not_modify_flags() {
    // BSWAP does not modify any flags
    let code = [
        0x0f, 0xc8, // BSWAP EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    regs.rflags = 0x2; // Only reserved bit 1 set
    let initial_flags = regs.rflags;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rflags, initial_flags, "Flags should not be modified");
}

// ============================================================================
// Strengthened BSWAP tests (appended): exact byte-reversed results for 32-bit
// (with upper-RAX clearing) and 64-bit operands, plus extended-register form.
// ============================================================================

#[test]
fn test_strict_bswap_r32_exact_and_clears_upper() {
    // BSWAP EAX: 0x11223344 -> 0x44332211; upper 32 bits of RAX cleared.
    let code = [0x0f, 0xc8, 0xf4]; // BSWAP EAX
    let mut regs = Registers::default();
    regs.rax = 0xFFFF_FFFF_1122_3344;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(
        regs.rax, 0x0000_0000_4433_2211,
        "BSWAP EAX reverses 4 bytes, clears upper"
    );
}

#[test]
fn test_strict_bswap_r64_exact() {
    // BSWAP RAX: 0x0102030405060708 -> 0x0807060504030201.
    let code = [0x48, 0x0f, 0xc8, 0xf4]; // BSWAP RAX
    let mut regs = Registers::default();
    regs.rax = 0x0102_0304_0506_0708;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(
        regs.rax, 0x0807_0605_0403_0201,
        "BSWAP RAX reverses 8 bytes"
    );
}

#[test]
fn test_strict_bswap_r64_extended_reg() {
    // BSWAP R8: REX.WB 0F C8.
    let code = [0x49, 0x0f, 0xc8, 0xf4]; // BSWAP R8
    let mut regs = Registers::default();
    regs.r8 = 0xDEAD_BEEF_CAFE_BABE;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.r8, 0xBEBA_FECA_EFBE_ADDE, "BSWAP R8 reverses 8 bytes");
}

#[test]
fn test_strict_bswap_r32_round_trip() {
    // Two BSWAP EBX in a row return the original 32-bit value.
    let code = [0x0f, 0xcb, 0x0f, 0xcb, 0xf4]; // BSWAP EBX; BSWAP EBX
    let mut regs = Registers::default();
    regs.rbx = 0x0000_0000_ABCD_1234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(
        regs.rbx, 0x0000_0000_ABCD_1234,
        "double BSWAP is identity (32-bit)"
    );
}

fn undefined_bswap_initial_registers() -> Registers {
    Registers {
        rax: 0xA000_0000_0000_1001,
        rbx: 0xA001_0000_0000_1002,
        rcx: 0xA002_0000_0000_1003,
        rdx: 0xA003_0000_0000_1004,
        rsi: 0xA004_0000_0000_1005,
        rdi: 0xA005_0000_0000_1006,
        rsp: 0x0000_0000_0000_8000,
        rbp: 0xA007_0000_0000_1008,
        r8: 0xA008_0000_0000_1009,
        r9: 0xA009_0000_0000_100A,
        r10: 0xA00A_0000_0000_100B,
        r11: 0xA00B_0000_0000_100C,
        r12: 0xA00C_0000_0000_100D,
        r13: 0xA00D_0000_0000_100E,
        r14: 0xA00E_0000_0000_100F,
        r15: 0xA00F_0000_0000_1010,
        r16: 0xA010_0000_0000_1011,
        r17: 0xA011_0000_0000_1012,
        r18: 0xA012_0000_0000_1013,
        r19: 0xA013_0000_0000_1014,
        r20: 0xA014_0000_0000_1015,
        r21: 0xA015_0000_0000_1016,
        r22: 0xA016_0000_0000_1017,
        r23: 0xA017_0000_0000_1018,
        r24: 0xA018_0000_0000_1019,
        r25: 0xA019_0000_0000_101A,
        r26: 0xA01A_0000_0000_101B,
        r27: 0xA01B_0000_0000_101C,
        r28: 0xA01C_0000_0000_101D,
        r29: 0xA01D_0000_0000_101E,
        r30: 0xA01E_0000_0000_101F,
        r31: 0xA01F_0000_0000_1020,
        rflags: 0x0CD7,
        ..Registers::default()
    }
}

#[test]
fn test_undefined_bswap_r16_preserves_all_32_gprs_flags_and_cache_paths() {
    let mut code = Vec::new();
    let mut lengths = Vec::new();
    for leader in [
        &[0x66, 0x0F][..],
        &[0x66, 0x41, 0x0F][..],
        &[0x66, 0xD5, 0x90][..],
        &[0x66, 0xD5, 0x91][..],
    ] {
        for opcode in 0xC8_u8..=0xCF {
            code.extend_from_slice(leader);
            code.push(opcode);
            lengths.push(leader.len() + 1);
        }
    }
    code.push(0xF4);
    assert_eq!(lengths.len(), 32);

    let (mut vcpu, _) = setup_vm(&code, Some(undefined_bswap_initial_registers()));
    vcpu.set_apx_enabled(true);
    for path in ["cold decode", "decode-cache hit"] {
        let mut before = vcpu.get_regs().unwrap();
        before.rip = CODE_ADDR;
        vcpu.set_regs(&before).unwrap();
        let before_sregs = vcpu.get_sregs().unwrap();
        let mut expected_rip = CODE_ADDR;

        for (index, length) in lengths.iter().enumerate() {
            assert!(
                vcpu.step()
                    .unwrap_or_else(|error| panic!("BSWAP r16 {index} ({path}): {error}"))
                    .is_none(),
                "BSWAP r16 {index} ({path})"
            );
            expected_rip += *length as u64;
            assert_eq!(
                vcpu.get_regs().unwrap().rip,
                expected_rip,
                "{index} ({path})"
            );
        }

        let mut expected = before;
        expected.rip = expected_rip;
        let actual =
            serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();
        let expected = serde_json::to_value((expected, before_sregs)).unwrap();
        assert_eq!(actual, expected, "all 32 BSWAP r16 registers ({path})");
    }
}

#[test]
fn test_bswap_operand_size_prefix_order_and_rex2_w_precedence() {
    for (instruction, apx_enabled, expected) in [
        (&[0x66, 0x48, 0x0F, 0xC8][..], false, 0x0807_0605_0403_0201),
        (&[0x48, 0x66, 0x0F, 0xC8][..], false, 0x0102_0304_0506_0708),
        (&[0x66, 0xD5, 0x98, 0xC8][..], true, 0x0102_0304_0506_0708),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let mut regs = Registers::default();
        regs.rax = 0x0102_0304_0506_0708;
        regs.r16 = 0x0807_0605_0403_0201;
        let (mut vcpu, _) = setup_vm(&code, Some(regs));
        vcpu.set_apx_enabled(apx_enabled);
        for path in ["cold decode", "decode-cache hit"] {
            let mut before = vcpu.get_regs().unwrap();
            before.rip = CODE_ADDR;
            before.rax = 0x0102_0304_0506_0708;
            before.r16 = 0x0807_0605_0403_0201;
            before.rflags = 0x0CD7;
            vcpu.set_regs(&before).unwrap();

            assert!(
                vcpu.step()
                    .unwrap_or_else(|error| panic!("{instruction:02X?} ({path}): {error}"))
                    .is_none()
            );
            let final_regs = vcpu.get_regs().unwrap();
            let (actual, other) = if apx_enabled {
                (final_regs.r16, final_regs.rax)
            } else {
                (final_regs.rax, final_regs.r16)
            };
            assert_eq!(actual, expected, "{instruction:02X?} ({path})");
            assert_eq!(
                other,
                if apx_enabled {
                    0x0102_0304_0506_0708
                } else {
                    0x0807_0605_0403_0201
                },
                "non-destination {instruction:02X?} ({path})"
            );
            assert_eq!(
                final_regs.rflags, before.rflags,
                "{instruction:02X?} ({path})"
            );
            assert_eq!(
                final_regs.rip,
                CODE_ADDR + instruction.len() as u64,
                "{instruction:02X?} ({path})"
            );
        }
    }
}

#[test]
fn test_lock_bswap_r16_faults_without_commit_on_both_decode_paths() {
    for (instruction, apx_enabled) in [
        (&[0xF0, 0x66, 0x0F, 0xC8][..], false),
        (&[0xF0, 0x66, 0xD5, 0x90, 0xC8][..], true),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(instruction, Some(undefined_bswap_initial_registers()));
        vcpu.set_apx_enabled(apx_enabled);
        for path in ["cold decode", "decode-cache hit"] {
            let before =
                serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap()))
                    .unwrap();
            let error = vcpu.step().expect_err("LOCK BSWAP r16 must inject #UD");
            assert!(
                error.to_string().contains("IDT entry 6 not present"),
                "{instruction:02X?} ({path}): {error}"
            );
            let after = serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap()))
                .unwrap();
            assert_eq!(after, before, "{instruction:02X?} ({path})");
        }
    }
}

#[test]
fn test_rex2_bswap_r16_requires_apx_without_commit_on_both_decode_paths() {
    let instruction = [0x66, 0xD5, 0x90, 0xC8];
    let (mut vcpu, _) = setup_vm_no_idt(&instruction, Some(undefined_bswap_initial_registers()));
    vcpu.set_apx_enabled(false);

    for path in ["cold decode", "decode-cache hit"] {
        let before =
            serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();
        let error = vcpu.step().expect_err("REX2 BSWAP r16 must require APX");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{instruction:02X?} ({path}): {error}"
        );
        let after =
            serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();
        assert_eq!(after, before, "{instruction:02X?} ({path})");
    }
}
