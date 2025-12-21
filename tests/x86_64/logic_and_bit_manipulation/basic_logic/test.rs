use crate::common::*;
use rax::backend::emulator::x86_64::flags;
use rax::cpu::Registers;

// TEST — Logical Compare
//
// Opcodes:
// - A8 ib           TEST AL, imm8
// - A9 iw/id        TEST AX/EAX/RAX, imm16/32
// - F6 /0 ib        TEST r/m8, imm8
// - F7 /0 iw/id     TEST r/m16/32/64, imm16/32
// - 84 /r           TEST r/m8, r8
// - 85 /r           TEST r/m16/32/64, r16/32/64
//
// Operation: temp = SRC1 AND SRC2; set flags; discard temp
//
// Flags: OF and CF are CLEARED.
//        SF, ZF, PF are set according to result.
//        AF is undefined.
//
// CRITICAL: TEST performs AND but discards the result - operands unchanged.
// Commonly used to test if bits are set or if register is zero.

// ============================================================================
// TEST with immediate
// ============================================================================

#[test]
fn test_test_al_imm8_basic() {
    let code = [
        0xa8, 0x0F, // TEST AL, 0x0F
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xAB;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0xAB & 0x0F = 0x0B (non-zero)
    assert_eq!(regs.rax & 0xFF, 0xAB, "AL unchanged by TEST");
    assert!(!zf_set(regs.rflags), "ZF clear (result non-zero)");
    assert!(!cf_set(regs.rflags), "CF cleared");
    assert!(!of_set(regs.rflags), "OF cleared");
}

#[test]
fn test_test_al_imm8_zero_result() {
    let code = [0xa8, 0x0F, 0xf4]; // TEST AL, 0x0F
    let mut regs = Registers::default();
    regs.rax = 0xF0; // No bits in common with 0x0F
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0xF0 & 0x0F = 0x00 (zero)
    assert_eq!(regs.rax & 0xFF, 0xF0, "AL unchanged");
    assert!(zf_set(regs.rflags), "ZF set (result is zero)");
    assert!(!cf_set(regs.rflags), "CF cleared");
}

#[test]
fn test_test_al_imm8_sign_flag() {
    let code = [0xa8, 0xFF, 0xf4]; // TEST AL, 0xFF
    let mut regs = Registers::default();
    regs.rax = 0x80; // High bit set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0x80 & 0xFF = 0x80 (high bit set)
    assert!(sf_set(regs.rflags), "SF set (high bit = 1)");
}

#[test]
fn test_test_eax_imm32_basic() {
    let code = [
        0xa9, 0x00, 0xFF, 0x00, 0x00, // TEST EAX, 0x0000FF00
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0x12345678 & 0x0000FF00 = 0x00005600 (non-zero)
    assert_eq!(regs.rax, 0x12345678, "EAX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rax_imm32_basic() {
    let code = [
        0x48, 0xa9, 0xFF, 0xFF, 0x00, 0x00, // TEST RAX, 0x0000FFFF
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABC0000; // Low 16 bits are zero
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0x123456789ABC0000 & 0x0000FFFF = 0x0000 (zero)
    assert_eq!(regs.rax, 0x123456789ABC0000, "RAX unchanged");
    assert!(zf_set(regs.rflags), "ZF set (no bits in common)");
}

// ============================================================================
// TEST r/m with immediate
// ============================================================================

#[test]
fn test_test_rm8_imm8_basic() {
    let code = [
        0xf6, 0xc3, 0x0F, // TEST BL, 0x0F (F6 /0 ib)
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0xFF & 0x0F = 0x0F (non-zero)
    assert_eq!(regs.rbx & 0xFF, 0xFF, "BL unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm32_imm32_basic() {
    let code = [
        0xf7, 0xc3, 0x00, 0x00, 0xFF, 0x00, // TEST EBX, 0x00FF0000
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0x12345678 & 0x00FF0000 = 0x00340000 (non-zero)
    assert_eq!(regs.rbx, 0x12345678, "EBX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm64_imm32_basic() {
    let code = [
        0x48, 0xf7, 0xc3, 0xFF, 0xFF, 0xFF, 0xFF, // TEST RBX, 0xFFFFFFFF
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xFFFFFFFF00000000; // High 32 bits set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // High bits set but testing low 32 bits only
    assert_eq!(regs.rbx, 0xFFFFFFFF00000000, "RBX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear (sign-extended imm32 tests all bits)");
}

// ============================================================================
// TEST r/m, r
// ============================================================================

#[test]
fn test_test_rm8_r8_basic() {
    let code = [
        0x84, 0xd8, // TEST AL, BL
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xAA; // 10101010
    regs.rbx = 0x55; // 01010101
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0xAA & 0x55 = 0x00 (no common bits)
    assert_eq!(regs.rax & 0xFF, 0xAA, "AL unchanged");
    assert_eq!(regs.rbx & 0xFF, 0x55, "BL unchanged");
    assert!(zf_set(regs.rflags), "ZF set (zero result)");
}

#[test]
fn test_test_rm32_r32_basic() {
    let code = [
        0x85, 0xd8, // TEST EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFF00FF00;
    regs.rbx = 0x00FF00FF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 0xFF00FF00 & 0x00FF00FF = 0x00000000
    assert_eq!(regs.rax, 0xFF00FF00, "EAX unchanged");
    assert_eq!(regs.rbx, 0x00FF00FF, "EBX unchanged");
    assert!(zf_set(regs.rflags), "ZF set");
}

#[test]
fn test_test_rm64_r64_basic() {
    let code = [
        0x48, 0x85, 0xd8, // TEST RAX, RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFF00000000;
    regs.rbx = 0x00000000FFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // No common bits
    assert_eq!(regs.rax, 0xFFFFFFFF00000000, "RAX unchanged");
    assert_eq!(regs.rbx, 0x00000000FFFFFFFF, "RBX unchanged");
    assert!(zf_set(regs.rflags), "ZF set");
}

// ============================================================================
// TEST reg, reg (common idiom to check if reg is zero)
// ============================================================================

#[test]
fn test_test_eax_eax_zero() {
    // TEST EAX, EAX is common idiom to test if EAX is zero
    let code = [
        0x85, 0xc0, // TEST EAX, EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "EAX unchanged");
    assert!(zf_set(regs.rflags), "ZF set (EAX is zero)");
    assert!(!sf_set(regs.rflags), "SF clear");
    assert!(!cf_set(regs.rflags), "CF clear");
    assert!(!of_set(regs.rflags), "OF clear");
}

#[test]
fn test_test_eax_eax_non_zero() {
    let code = [0x85, 0xc0, 0xf4]; // TEST EAX, EAX
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x12345678, "EAX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear (EAX is non-zero)");
}

#[test]
fn test_test_rax_rax_negative() {
    let code = [0x48, 0x85, 0xc0, 0xf4]; // TEST RAX, RAX
    let mut regs = Registers::default();
    regs.rax = 0x8000000000000000; // High bit set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!zf_set(regs.rflags), "ZF clear (non-zero)");
    assert!(sf_set(regs.rflags), "SF set (high bit = 1)");
}

// ============================================================================
// Bit testing use cases
// ============================================================================

#[test]
fn test_test_check_bit_set() {
    // Check if bit 4 is set
    let code = [0xa8, 0x10, 0xf4]; // TEST AL, 0x10
    let mut regs = Registers::default();
    regs.rax = 0x1F; // bit 4 is set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!zf_set(regs.rflags), "ZF clear means bit 4 is set");
}

#[test]
fn test_test_check_bit_clear() {
    // Check if bit 4 is clear
    let code = [0xa8, 0x10, 0xf4]; // TEST AL, 0x10
    let mut regs = Registers::default();
    regs.rax = 0x0F; // bit 4 is clear
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(zf_set(regs.rflags), "ZF set means bit 4 is clear");
}

#[test]
fn test_test_check_multiple_bits() {
    // Check if ANY of bits 0, 2, 4 are set
    let code = [0xa8, 0x15, 0xf4]; // TEST AL, 0x15 (00010101)
    let mut regs = Registers::default();
    regs.rax = 0x04; // Only bit 2 is set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!zf_set(regs.rflags), "ZF clear (at least one bit matches)");
}

#[test]
fn test_test_check_all_bits_clear() {
    // Check if bits 0, 2, 4 are all clear
    let code = [0xa8, 0x15, 0xf4]; // TEST AL, 0x15
    let mut regs = Registers::default();
    regs.rax = 0xEA; // bits 1, 3, 5, 6, 7 set (not 0, 2, 4)
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(zf_set(regs.rflags), "ZF set (all tested bits are clear)");
}

// ============================================================================
// Parity flag tests
// ============================================================================

#[test]
fn test_test_parity_even() {
    let code = [0xa8, 0x03, 0xf4]; // TEST AL, 0x03
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Result = 0x03 (two 1-bits = even parity)
    assert!(pf_set(regs.rflags), "PF set (even parity)");
}

#[test]
fn test_test_parity_odd() {
    let code = [0xa8, 0x07, 0xf4]; // TEST AL, 0x07
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Result = 0x07 (three 1-bits = odd parity)
    assert!(!pf_set(regs.rflags), "PF clear (odd parity)");
}

// ============================================================================
// OF and CF always cleared
// ============================================================================

#[test]
fn test_test_clears_of_cf() {
    let code = [0xa8, 0xFF, 0xf4]; // TEST AL, 0xFF
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    regs.rflags = 0x2 | flags::bits::OF | flags::bits::CF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!of_set(regs.rflags), "OF cleared by TEST");
    assert!(!cf_set(regs.rflags), "CF cleared by TEST");
}

// ============================================================================
// Different registers
// ============================================================================

#[test]
fn test_test_different_registers() {
    // TEST CL, DL
    let code = [0x84, 0xd1, 0xf4];
    let mut regs = Registers::default();
    regs.rcx = 0x0F;
    regs.rdx = 0xF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFF, 0x0F, "CL unchanged");
    assert_eq!(regs.rdx & 0xFF, 0xF0, "DL unchanged");
    assert!(zf_set(regs.rflags), "ZF set (no common bits)");
}

// ============================================================================
// Extended registers (R8-R15)
// ============================================================================

#[test]
fn test_test_r8b_basic() {
    let code = [
        0x41, 0xf6, 0xc0, 0x0F, // TEST R8B, 0x0F
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r8 = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8 & 0xFF, 0xFF, "R8B unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_r10d_basic() {
    let code = [
        0x41, 0xf7, 0xc2, 0xFF, 0xFF, 0x00, 0x00, // TEST R10D, 0x0000FFFF
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r10 = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r10, 0x12345678, "R10D unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_r11_r12() {
    let code = [
        0x4d, 0x85, 0xe3, // TEST R11, R12
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r11 = 0xAAAAAAAAAAAAAAAA;
    regs.r12 = 0x5555555555555555;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r11, 0xAAAAAAAAAAAAAAAA, "R11 unchanged");
    assert_eq!(regs.r12, 0x5555555555555555, "R12 unchanged");
    assert!(zf_set(regs.rflags), "ZF set (no common bits)");
}

// ============================================================================
// Memory operands
// ============================================================================

#[test]
fn test_test_byte_ptr_mem() {
    let code = [
        0xf6, 0x05, 0xf9, 0x0f, 0x00, 0x00, 0x0F, // TEST BYTE PTR [rip+0x0FF9], 0x0F
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u8(&mem, 0xFF);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    let result = read_mem_u8(&mem);

    assert_eq!(result, 0xFF, "Memory unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear (0xFF & 0x0F = 0x0F)");
}

#[test]
fn test_test_dword_ptr_mem() {
    let code = [
        0xf7, 0x05, 0xf6, 0x0f, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, // TEST DWORD PTR [rip+0x0FF6], 0x000000FF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0x12345600); // Low byte is zero

    let regs = run_until_hlt(&mut vcpu).unwrap();
    let result = read_mem_u32(&mem);

    assert_eq!(result, 0x12345600, "Memory unchanged");
    assert!(zf_set(regs.rflags), "ZF set (low byte is zero)");
}

// ============================================================================
// Practical use cases
// ============================================================================

#[test]
fn test_test_validate_alignment() {
    // Check if address is 16-byte aligned (low 4 bits should be zero)
    let code = [
        0x48, 0xa9, 0x0F, 0x00, 0x00, 0x00, // TEST RAX, 0x0000000F
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDEF0; // Aligned (low 4 bits = 0)
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(zf_set(regs.rflags), "ZF set means aligned");
}

#[test]
fn test_test_validate_not_aligned() {
    let code = [
        0x48, 0xa9, 0x0F, 0x00, 0x00, 0x00, // TEST RAX, 0x0000000F
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDEF7; // Not aligned
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!zf_set(regs.rflags), "ZF clear means not aligned");
}

#[test]
fn test_test_check_sign_bit() {
    // Check if high bit is set (number is negative in signed interpretation)
    let code = [
        0x48, 0x85, 0xc0, // TEST RAX, RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x8000000000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(sf_set(regs.rflags), "SF set means high bit is set (negative)");
}
