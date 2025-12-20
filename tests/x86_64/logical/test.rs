use crate::common::{run_until_hlt, setup_vm};
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
// Operation: TEMP := DEST AND SRC (result is not stored)
//
// Flags: OF and CF are CLEARED.
//        SF, ZF, PF are set according to result.
//        AF is undefined.
//
// CRITICAL: TEST performs AND but does NOT store the result.
// Used for testing bits without modifying the operand.

// ============================================================================
// TEST AL, imm8
// ============================================================================

#[test]
fn test_test_al_imm8_basic() {
    let code = [0xa8, 0x0F, 0xf4]; // TEST AL, 0x0F
    let mut regs = Registers::default();
    regs.rax = 0xAB;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // AL should remain unchanged
    assert_eq!(regs.rax & 0xFF, 0xAB, "AL unchanged by TEST");
    // Flags set according to (0xAB AND 0x0F) = 0x0B
    assert!(!zf_set(regs.rflags), "ZF clear");
    assert!(!cf_set(regs.rflags), "CF clear");
    assert!(!of_set(regs.rflags), "OF clear");
}

#[test]
fn test_test_al_imm8_zero_result() {
    let code = [0xa8, 0x00, 0xf4]; // TEST AL, 0
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0xFF, "AL unchanged");
    assert!(zf_set(regs.rflags), "ZF set (zero result)");
}

#[test]
fn test_test_al_imm8_bit_test() {
    let code = [0xa8, 0x10, 0xf4]; // TEST AL, 0x10 (test bit 4)
    let mut regs = Registers::default();
    regs.rax = 0x1F; // bit 4 is set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x1F, "AL unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear (bit 4 is set)");
}

#[test]
fn test_test_al_imm8_bit_clear() {
    let code = [0xa8, 0x10, 0xf4]; // TEST AL, 0x10 (test bit 4)
    let mut regs = Registers::default();
    regs.rax = 0x0F; // bit 4 is clear
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x0F, "AL unchanged");
    assert!(zf_set(regs.rflags), "ZF set (bit 4 is clear)");
}

#[test]
fn test_test_al_imm8_sign_flag() {
    let code = [0xa8, 0x80, 0xf4]; // TEST AL, 0x80
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0xFF, "AL unchanged");
    assert!(sf_set(regs.rflags), "SF set (result has high bit)");
}

#[test]
fn test_test_al_imm8_parity() {
    let code = [0xa8, 0x03, 0xf4]; // TEST AL, 0x03
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Result = 0x03 (two 1-bits = even parity)
    assert!(pf_set(regs.rflags), "PF set (even parity)");
}

// ============================================================================
// TEST AX/EAX/RAX, imm
// ============================================================================

#[test]
fn test_test_ax_imm16() {
    let code = [0x66, 0xa9, 0x0F, 0x00, 0xf4]; // TEST AX, 0x000F
    let mut regs = Registers::default();
    regs.rax = 0x1234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0x1234, "AX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_eax_imm32() {
    let code = [0xa9, 0xFF, 0x00, 0x00, 0x00, 0xf4]; // TEST EAX, 0x000000FF
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x12345678, "EAX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rax_imm32() {
    let code = [0x48, 0xa9, 0xFF, 0xFF, 0x00, 0x00, 0xf4]; // TEST RAX, 0x0000FFFF
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEF0, "RAX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

// ============================================================================
// TEST r/m8, imm8
// ============================================================================

#[test]
fn test_test_rm8_imm8_bl() {
    let code = [0xf6, 0xc3, 0x0F, 0xf4]; // TEST BL, 0x0F
    let mut regs = Registers::default();
    regs.rbx = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFF, 0xFF, "BL unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm8_imm8_cl() {
    let code = [0xf6, 0xc1, 0xAA, 0xf4]; // TEST CL, 0xAA
    let mut regs = Registers::default();
    regs.rcx = 0x55;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFF, 0x55, "CL unchanged");
    assert!(zf_set(regs.rflags), "ZF set (no common bits)");
}

#[test]
fn test_test_rm8_imm8_dh() {
    let code = [0xf6, 0xc6, 0x80, 0xf4]; // TEST DH, 0x80
    let mut regs = Registers::default();
    regs.rdx = 0xFF00;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdx, 0xFF00, "DH unchanged");
    assert!(sf_set(regs.rflags), "SF set");
}

// ============================================================================
// TEST r/m16, imm16
// ============================================================================

#[test]
fn test_test_rm16_imm16_bx() {
    let code = [0x66, 0xf7, 0xc3, 0xF0, 0x0F, 0xf4]; // TEST BX, 0x0FF0
    let mut regs = Registers::default();
    regs.rbx = 0xFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFF, 0xFFFF, "BX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm16_imm16_si() {
    let code = [0x66, 0xf7, 0xc6, 0x00, 0xFF, 0xf4]; // TEST SI, 0xFF00
    let mut regs = Registers::default();
    regs.rsi = 0x00FF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi & 0xFFFF, 0x00FF, "SI unchanged");
    assert!(zf_set(regs.rflags), "ZF set (no common bits)");
}

// ============================================================================
// TEST r/m32, imm32
// ============================================================================

#[test]
fn test_test_rm32_imm32_ebx() {
    let code = [0xf7, 0xc3, 0x00, 0xFF, 0x00, 0x00, 0xf4]; // TEST EBX, 0x0000FF00
    let mut regs = Registers::default();
    regs.rbx = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x12345678, "EBX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm32_imm32_esi() {
    let code = [0xf7, 0xc6, 0xFF, 0x00, 0x00, 0x00, 0xf4]; // TEST ESI, 0x000000FF
    let mut regs = Registers::default();
    regs.rsi = 0xABCDEF00;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi, 0xABCDEF00, "ESI unchanged");
    assert!(zf_set(regs.rflags), "ZF set");
}

// ============================================================================
// TEST r/m64, imm32
// ============================================================================

#[test]
fn test_test_rm64_imm32_rbx() {
    let code = [0x48, 0xf7, 0xc3, 0xFF, 0xFF, 0xFF, 0x00, 0xf4]; // TEST RBX, 0x00FFFFFF
    let mut regs = Registers::default();
    regs.rbx = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x123456789ABCDEF0, "RBX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

// ============================================================================
// TEST r/m, r
// ============================================================================

#[test]
fn test_test_rm8_r8_al_bl() {
    let code = [0x84, 0xd8, 0xf4]; // TEST AL, BL
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    regs.rbx = 0x0F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0xFF, "AL unchanged");
    assert_eq!(regs.rbx & 0xFF, 0x0F, "BL unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm8_r8_no_bits() {
    let code = [0x84, 0xd8, 0xf4]; // TEST AL, BL
    let mut regs = Registers::default();
    regs.rax = 0xAA;
    regs.rbx = 0x55;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0xAA, "AL unchanged");
    assert_eq!(regs.rbx & 0xFF, 0x55, "BL unchanged");
    assert!(zf_set(regs.rflags), "ZF set (no common bits)");
}

#[test]
fn test_test_rm16_r16_ax_bx() {
    let code = [0x66, 0x85, 0xd8, 0xf4]; // TEST AX, BX
    let mut regs = Registers::default();
    regs.rax = 0xFFFF;
    regs.rbx = 0x00FF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0xFFFF, "AX unchanged");
    assert_eq!(regs.rbx & 0xFFFF, 0x00FF, "BX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm32_r32_eax_ebx() {
    let code = [0x85, 0xd8, 0xf4]; // TEST EAX, EBX
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    regs.rbx = 0xF0F0F0F0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x12345678, "EAX unchanged");
    assert_eq!(regs.rbx, 0xF0F0F0F0, "EBX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_rm64_r64_rax_rbx() {
    let code = [0x48, 0x85, 0xd8, 0xf4]; // TEST RAX, RBX
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFFFFFFFFFF;
    regs.rbx = 0x00000000FFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0xFFFFFFFFFFFFFFFF, "RAX unchanged");
    assert_eq!(regs.rbx, 0x00000000FFFFFFFF, "RBX unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

// ============================================================================
// Different register combinations
// ============================================================================

#[test]
fn test_test_cl_dl() {
    let code = [0x84, 0xd1, 0xf4]; // TEST CL, DL
    let mut regs = Registers::default();
    regs.rcx = 0xFF;
    regs.rdx = 0x3C;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFF, 0xFF, "CL unchanged");
    assert_eq!(regs.rdx & 0xFF, 0x3C, "DL unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_ecx_edx() {
    let code = [0x85, 0xd1, 0xf4]; // TEST ECX, EDX
    let mut regs = Registers::default();
    regs.rcx = 0xF0F0F0F0;
    regs.rdx = 0x0F0F0F0F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0xF0F0F0F0, "ECX unchanged");
    assert_eq!(regs.rdx, 0x0F0F0F0F, "EDX unchanged");
    assert!(zf_set(regs.rflags), "ZF set (complementary)");
}

#[test]
fn test_test_rsi_rdi() {
    let code = [0x48, 0x85, 0xfe, 0xf4]; // TEST RSI, RDI
    let mut regs = Registers::default();
    regs.rsi = 0xAAAAAAAAAAAAAAAA;
    regs.rdi = 0xAAAAAAAAAAAAAAAA;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi, 0xAAAAAAAAAAAAAAAA, "RSI unchanged");
    assert_eq!(regs.rdi, 0xAAAAAAAAAAAAAAAA, "RDI unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

// ============================================================================
// Extended registers (R8-R15)
// ============================================================================

#[test]
fn test_test_r8b_imm8() {
    let code = [0x41, 0xf6, 0xc0, 0x0F, 0xf4]; // TEST R8B, 0x0F
    let mut regs = Registers::default();
    regs.r8 = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8 & 0xFF, 0xFF, "R8B unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_r9w_imm16() {
    let code = [0x66, 0x41, 0xf7, 0xc1, 0xF0, 0x0F, 0xf4]; // TEST R9W, 0x0FF0
    let mut regs = Registers::default();
    regs.r9 = 0xFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r9 & 0xFFFF, 0xFFFF, "R9W unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_r10d_imm32() {
    let code = [0x41, 0xf7, 0xc2, 0xFF, 0x00, 0x00, 0x00, 0xf4]; // TEST R10D, 0x000000FF
    let mut regs = Registers::default();
    regs.r10 = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r10, 0x12345678, "R10D unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_r11_imm32() {
    let code = [0x49, 0xf7, 0xc3, 0xFF, 0xFF, 0x00, 0x00, 0xf4]; // TEST R11, 0x0000FFFF
    let mut regs = Registers::default();
    regs.r11 = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r11, 0x123456789ABCDEF0, "R11 unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_r12d_r13d() {
    let code = [0x45, 0x85, 0xec, 0xf4]; // TEST R12D, R13D
    let mut regs = Registers::default();
    regs.r12 = 0xFFFFFFFF;
    regs.r13 = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r12, 0xFFFFFFFF, "R12D unchanged");
    assert_eq!(regs.r13, 0x12345678, "R13D unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_r14_r15() {
    let code = [0x4d, 0x85, 0xfe, 0xf4]; // TEST R14, R15
    let mut regs = Registers::default();
    regs.r14 = 0xFFFFFFFF00000000;
    regs.r15 = 0x00000000FFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r14, 0xFFFFFFFF00000000, "R14 unchanged");
    assert_eq!(regs.r15, 0x00000000FFFFFFFF, "R15 unchanged");
    assert!(zf_set(regs.rflags), "ZF set (no common bits)");
}

// ============================================================================
// Memory operands
// ============================================================================

#[test]
fn test_test_byte_ptr_imm8() {
    let code = [
        0xf6, 0x05, 0xf9, 0x0f, 0x00, 0x00, 0x0F, // TEST BYTE PTR [rip+0x0FF9], 0x0F
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u8(&mem, 0xFF);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    let result = read_mem_u8(&mem);

    assert_eq!(result, 0xFF, "Memory unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

#[test]
fn test_test_dword_ptr_imm32() {
    let code = [
        0xf7, 0x05, 0xf6, 0x0f, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, // TEST DWORD PTR [rip+0x0FF6], 0x000000FF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0x12345678);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    let result = read_mem_u32(&mem);

    assert_eq!(result, 0x12345678, "Memory unchanged");
    assert!(!zf_set(regs.rflags), "ZF clear");
}

// ============================================================================
// Flag behavior tests
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
// Practical use cases
// ============================================================================

#[test]
fn test_test_check_multiple_bits() {
    let code = [0xa8, 0x18, 0xf4]; // TEST AL, 0x18 (bits 3 and 4)
    let mut regs = Registers::default();
    regs.rax = 0x1F; // bits 3 and 4 are set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!zf_set(regs.rflags), "ZF clear (at least one bit is set)");
}

#[test]
fn test_test_register_zero() {
    let code = [0x85, 0xc0, 0xf4]; // TEST EAX, EAX
    let mut regs = Registers::default();
    regs.rax = 0x00000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(zf_set(regs.rflags), "ZF set (register is zero)");
    assert!(!sf_set(regs.rflags), "SF clear");
}

#[test]
fn test_test_register_negative() {
    let code = [0x48, 0x85, 0xc0, 0xf4]; // TEST RAX, RAX
    let mut regs = Registers::default();
    regs.rax = 0x8000000000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!zf_set(regs.rflags), "ZF clear");
    assert!(sf_set(regs.rflags), "SF set (high bit set)");
}
