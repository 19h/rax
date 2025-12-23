use crate::common::{run_until_hlt, setup_vm};
use crate::common::*;
use rax::backend::emulator::x86_64::flags;
use rax::cpu::Registers;

// SHR — Shift Right (Logical)
//
// Opcodes:
// - D0 /5       SHR r/m8, 1
// - D2 /5       SHR r/m8, CL
// - C0 /5 ib    SHR r/m8, imm8
// - D1 /5       SHR r/m16/32/64, 1
// - D3 /5       SHR r/m16/32/64, CL
// - C1 /5 ib    SHR r/m16/32/64, imm8
//
// Flags:
// - CF: Last bit shifted out
// - OF: Only for 1-bit shifts (original MSB)
// - SF, ZF, PF: Set according to result
// - Count is 0: No flags affected
// - Count is masked to 5 bits (0x1F) for 8/16/32-bit, 6 bits (0x3F) for 64-bit

// ============================================================================
// 8-bit SHR tests
// ============================================================================

#[test]
fn test_shr_al_1_basic() {
    let code = [0xd0, 0xe8, 0xf4]; // SHR AL, 1
    let mut regs = Registers::default();
    regs.rax = 0x42; // 0100_0010
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x21, "AL: 0x42 >> 1 = 0x21");
    assert!(!cf_set(regs.rflags), "CF clear (LSB was 0)");
    assert!(!of_set(regs.rflags), "OF clear (original MSB was 0)");
}

#[test]
fn test_shr_al_1_with_carry() {
    let code = [0xd0, 0xe8, 0xf4]; // SHR AL, 1
    let mut regs = Registers::default();
    regs.rax = 0x43; // 0100_0011
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x21, "AL: 0x43 >> 1 = 0x21");
    assert!(cf_set(regs.rflags), "CF set (LSB was 1)");
}

#[test]
fn test_shr_al_1_overflow() {
    let code = [0xd0, 0xe8, 0xf4]; // SHR AL, 1
    let mut regs = Registers::default();
    regs.rax = 0x80; // 1000_0000
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x40, "AL: 0x80 >> 1 = 0x40");
    assert!(!cf_set(regs.rflags), "CF clear");
    assert!(of_set(regs.rflags), "OF set (original MSB was 1)");
}

#[test]
fn test_shr_bl_cl() {
    let code = [0xd2, 0xeb, 0xf4]; // SHR BL, CL
    let mut regs = Registers::default();
    regs.rbx = 0x80;
    regs.rcx = 0x07;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFF, 0x01, "BL: 0x80 >> 7 = 0x01");
    assert!(!cf_set(regs.rflags), "CF: last bit shifted out was 0");
}

#[test]
fn test_shr_cl_imm8() {
    let code = [0xc0, 0xe9, 0x03, 0xf4]; // SHR CL, 3
    let mut regs = Registers::default();
    regs.rcx = 0x88; // 1000_1000
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFF, 0x11, "CL: 0x88 >> 3 = 0x11");
    assert!(!cf_set(regs.rflags), "CF: last bit shifted out was 0");
}

#[test]
fn test_shr_al_to_zero() {
    let code = [0xc0, 0xe8, 0x08, 0xf4]; // SHR AL, 8
    let mut regs = Registers::default();
    regs.rax = 0x42;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x00, "AL: all bits shifted out");
    assert!(zf_set(regs.rflags), "ZF set (result is zero)");
    assert!(!sf_set(regs.rflags), "SF clear");
}

#[test]
fn test_shr_count_masked_8bit() {
    let code = [0xd2, 0xe8, 0xf4]; // SHR AL, CL
    let mut regs = Registers::default();
    regs.rax = 0x88;
    regs.rcx = 0x23; // 35 & 0x1F = 3
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x11, "AL: 0x88 >> 3 = 0x11 (count masked)");
}

#[test]
fn test_shr_count_zero_preserves_flags() {
    let code = [0xc0, 0xe8, 0x00, 0xf4]; // SHR AL, 0
    let mut regs = Registers::default();
    regs.rax = 0x42;
    regs.rflags = 0x2 | flags::bits::CF | flags::bits::ZF | flags::bits::OF;
    let initial_flags = regs.rflags;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x42, "AL unchanged");
    assert_eq!(regs.rflags, initial_flags, "Flags unchanged when count is 0");
}

#[test]
fn test_shr_dh_1() {
    let code = [0xd0, 0xee, 0xf4]; // SHR DH, 1
    let mut regs = Registers::default();
    regs.rdx = 0x4200; // DH = 0x42
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!((regs.rdx >> 8) & 0xFF, 0x21, "DH: 0x42 >> 1 = 0x21");
}

#[test]
fn test_shr_al_carry_propagation() {
    let code = [0xc0, 0xe8, 0x04, 0xf4]; // SHR AL, 4
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x0F, "AL: 0xFF >> 4 = 0x0F");
    assert!(cf_set(regs.rflags), "CF set (last bit shifted out was 1)");
}

// ============================================================================
// 16-bit SHR tests
// ============================================================================

#[test]
fn test_shr_ax_1() {
    let code = [0x66, 0xd1, 0xe8, 0xf4]; // SHR AX, 1
    let mut regs = Registers::default();
    regs.rax = 0x8642;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0x4321, "AX: 0x8642 >> 1 = 0x4321");
    assert!(!cf_set(regs.rflags), "CF clear");
    assert!(of_set(regs.rflags), "OF set (original MSB was 1)");
}

#[test]
fn test_shr_ax_cl() {
    let code = [0x66, 0xd3, 0xe8, 0xf4]; // SHR AX, CL
    let mut regs = Registers::default();
    regs.rax = 0x8000;
    regs.rcx = 0x0F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0x0001, "AX: 0x8000 >> 15 = 0x0001");
    assert!(!cf_set(regs.rflags), "CF: last bit shifted out was 0");
}

#[test]
fn test_shr_bx_imm8() {
    let code = [0x66, 0xc1, 0xeb, 0x04, 0xf4]; // SHR BX, 4
    let mut regs = Registers::default();
    regs.rbx = 0x1234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFF, 0x0123, "BX: 0x1234 >> 4 = 0x0123");
    assert!(!cf_set(regs.rflags), "CF clear");
}

#[test]
fn test_shr_cx_to_zero() {
    let code = [0x66, 0xc1, 0xe9, 0x10, 0xf4]; // SHR CX, 16
    let mut regs = Registers::default();
    regs.rcx = 0x1234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFFFF, 0x0000, "CX: all bits shifted out");
    assert!(zf_set(regs.rflags), "ZF set");
}

#[test]
fn test_shr_si_1_with_carry() {
    let code = [0x66, 0xd1, 0xee, 0xf4]; // SHR SI, 1
    let mut regs = Registers::default();
    regs.rsi = 0x0003;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi & 0xFFFF, 0x0001, "SI: 0x0003 >> 1 = 0x0001");
    assert!(cf_set(regs.rflags), "CF set (LSB was 1)");
}

// ============================================================================
// 32-bit SHR tests
// ============================================================================

#[test]
fn test_shr_eax_1() {
    let code = [0xd1, 0xe8, 0xf4]; // SHR EAX, 1
    let mut regs = Registers::default();
    regs.rax = 0x2468ACF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x12345678, "EAX: 0x2468ACF0 >> 1 = 0x12345678");
    assert!(!cf_set(regs.rflags), "CF clear");
}

#[test]
fn test_shr_ebx_cl() {
    let code = [0xd3, 0xeb, 0xf4]; // SHR EBX, CL
    let mut regs = Registers::default();
    regs.rbx = 0x80000000;
    regs.rcx = 0x1F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x00000001, "EBX: 0x80000000 >> 31 = 0x00000001");
    assert!(!cf_set(regs.rflags), "CF: last bit shifted out was 0");
}

#[test]
fn test_shr_ecx_imm8() {
    let code = [0xc1, 0xe9, 0x08, 0xf4]; // SHR ECX, 8
    let mut regs = Registers::default();
    regs.rcx = 0x12345600;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0x00123456, "ECX: 0x12345600 >> 8 = 0x00123456");
}

#[test]
fn test_shr_esi_with_carry() {
    let code = [0xc1, 0xee, 0x10, 0xf4]; // SHR ESI, 16
    let mut regs = Registers::default();
    regs.rsi = 0x56780000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi, 0x00005678, "ESI: 0x56780000 >> 16 = 0x00005678");
}

#[test]
fn test_shr_edi_to_zero() {
    let code = [0xc1, 0xef, 0x20, 0xf4]; // SHR EDI, 32
    let mut regs = Registers::default();
    regs.rdi = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdi, 0x00000000, "EDI: all bits shifted out");
    assert!(zf_set(regs.rflags), "ZF set");
}

// ============================================================================
// 64-bit SHR tests
// ============================================================================

#[test]
fn test_shr_rax_1() {
    let code = [0x48, 0xd1, 0xe8, 0xf4]; // SHR RAX, 1
    let mut regs = Registers::default();
    regs.rax = 0x2468ACF13579BDE0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEF0, "RAX: >> 1");
    assert!(!cf_set(regs.rflags), "CF clear");
}

#[test]
fn test_shr_rbx_cl() {
    let code = [0x48, 0xd3, 0xeb, 0xf4]; // SHR RBX, CL
    let mut regs = Registers::default();
    regs.rbx = 0x8000000000000000;
    regs.rcx = 0x3F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x0000000000000001, "RBX: 0x8000...0 >> 63 = 0x01");
    assert!(!cf_set(regs.rflags), "CF: last bit shifted out was 0");
}

#[test]
fn test_shr_rcx_imm8() {
    let code = [0x48, 0xc1, 0xe9, 0x10, 0xf4]; // SHR RCX, 16
    let mut regs = Registers::default();
    regs.rcx = 0x123456789ABC0000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0x0000123456789ABC, "RCX: >> 16");
}

#[test]
fn test_shr_rsi_with_carry() {
    let code = [0x48, 0xc1, 0xee, 0x20, 0xf4]; // SHR RSI, 32
    let mut regs = Registers::default();
    regs.rsi = 0x9ABCDEF000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi, 0x000000009ABCDEF0, "RSI: >> 32");
}

#[test]
fn test_shr_rdi_to_zero() {
    let code = [0x48, 0xc1, 0xef, 0x40, 0xf4]; // SHR RDI, 64
    let mut regs = Registers::default();
    regs.rdi = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdi, 0x0000000000000000, "RDI: all bits shifted out");
    assert!(zf_set(regs.rflags), "ZF set");
}

#[test]
fn test_shr_count_masked_64bit() {
    let code = [0x48, 0xd3, 0xe8, 0xf4]; // SHR RAX, CL
    let mut regs = Registers::default();
    regs.rax = 0x0000000000000008;
    regs.rcx = 0x43; // 67 & 0x3F = 3
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x0000000000000001, "RAX: 0x08 >> 3 = 0x01 (count masked)");
}

// ============================================================================
// Extended registers (R8-R15)
// ============================================================================

#[test]
fn test_shr_r8b_1() {
    let code = [0x41, 0xd0, 0xe8, 0xf4]; // SHR R8B, 1
    let mut regs = Registers::default();
    regs.r8 = 0x42;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8 & 0xFF, 0x21, "R8B: 0x42 >> 1 = 0x21");
}

#[test]
fn test_shr_r9w_cl() {
    let code = [0x66, 0x41, 0xd3, 0xe9, 0xf4]; // SHR R9W, CL
    let mut regs = Registers::default();
    regs.r9 = 0x8000;
    regs.rcx = 0x0F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r9 & 0xFFFF, 0x0001, "R9W: 0x8000 >> 15 = 0x0001");
}

#[test]
fn test_shr_r10d_imm8() {
    let code = [0x41, 0xc1, 0xea, 0x08, 0xf4]; // SHR R10D, 8
    let mut regs = Registers::default();
    regs.r10 = 0x12345600;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r10, 0x00123456, "R10D: >> 8");
}

#[test]
fn test_shr_r11_1() {
    let code = [0x49, 0xd1, 0xeb, 0xf4]; // SHR R11, 1
    let mut regs = Registers::default();
    regs.r11 = 0x2468ACF13579BDE0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r11, 0x123456789ABCDEF0, "R11: >> 1");
}

#[test]
fn test_shr_r12_cl() {
    let code = [0x49, 0xd3, 0xec, 0xf4]; // SHR R12, CL
    let mut regs = Registers::default();
    regs.r12 = 0x0000000100000000;
    regs.rcx = 0x20;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r12, 0x0000000000000001, "R12: >> 32");
}

#[test]
fn test_shr_r15_imm8() {
    let code = [0x49, 0xc1, 0xef, 0x10, 0xf4]; // SHR R15, 16
    let mut regs = Registers::default();
    regs.r15 = 0x123456789ABC0000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r15, 0x0000123456789ABC, "R15: >> 16");
}

// ============================================================================
// Memory operands
// ============================================================================

#[test]
fn test_shr_byte_ptr_1() {
    let code = [
        0xd0, 0x2d, 0xfa, 0x0f, 0x00, 0x00, // SHR BYTE PTR [rip+0x0FFA], 1
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u8(&mem, 0x42);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    let result = read_mem_u8(&mem);

    assert_eq!(result, 0x21, "Memory: 0x42 >> 1 = 0x21");
}

#[test]
fn test_shr_dword_ptr_imm8() {
    let code = [
        0xc1, 0x2d, 0xf9, 0x0f, 0x00, 0x00, 0x08, // SHR DWORD PTR [rip+0x0FF9], 8
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0x12345600);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    let result = read_mem_u32(&mem);

    assert_eq!(result, 0x00123456, "Memory: >> 8");
}

#[test]
fn test_shr_qword_ptr_imm8() {
    let code = [
        0x48, 0xc1, 0x2d, 0xf8, 0x0f, 0x00, 0x00, 0x10, // SHR QWORD PTR [rip+0x0FF8], 16
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0x123456789ABC0000);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    let result = read_mem_u64(&mem);

    assert_eq!(result, 0x0000123456789ABC, "Memory: >> 16");
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_shr_no_sign_extension() {
    let code = [0xc0, 0xe8, 0x04, 0xf4]; // SHR AL, 4
    let mut regs = Registers::default();
    regs.rax = 0xFF; // All bits set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Logical shift: zeros fill from left
    assert_eq!(regs.rax & 0xFF, 0x0F, "AL: 0xFF >> 4 = 0x0F (no sign extension)");
}

#[test]
fn test_shr_multiple_operations() {
    let code = [
        0xd0, 0xe8, // SHR AL, 1
        0xd0, 0xe8, // SHR AL, 1
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x84; // 1000_0100
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x21, "AL: 0x84 >> 2 = 0x21");
}
