use crate::common::*;
use rax::backend::emulator::x86_64::flags;
use rax::cpu::{Registers, VCpu};

// INC/DEC — Increment/Decrement by 1
//
// INC:
// - FE /0       INC r/m8      Increment r/m8 by 1
// - FF /0       INC r/m16/32  Increment r/m16/32 by 1
// - REX.W+FF /0 INC r/m64     Increment r/m64 by 1
//
// DEC:
// - FE /1       DEC r/m8      Decrement r/m8 by 1
// - FF /1       DEC r/m16/32  Decrement r/m16/32 by 1
// - REX.W+FF /1 DEC r/m64     Decrement r/m64 by 1
//
// Flags: INC/DEC modify OF, SF, ZF, AF, PF (NOT CF)
//        CF is NOT affected by INC/DEC
//
// This is a critical difference from ADD/SUB which modify CF

// ============================================================================
// 8-bit INC (opcode FE /0)
// ============================================================================

#[test]
fn test_inc_al_basic() {
    let code = [0xfe, 0xc0, 0xf4]; // INC AL
    let mut regs = Registers::default();
    regs.rax = 10;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 11, "INC AL: 10 + 1 = 11");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_inc_al_zero_result() {
    // INC AL when AL = 0xFF -> 0x00 (wraps)
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0, "INC AL: 0xFF + 1 = 0x00 (wraps)");
    assert!(zf_set(regs.rflags), "ZF should be set (result = 0)");
}

#[test]
fn test_inc_al_overflow() {
    // INC AL when AL = 0x7F -> 0x80 (signed overflow)
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x7F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x80, "INC AL: 0x7F + 1 = 0x80");
    assert!(of_set(regs.rflags), "OF should be set (signed overflow)");
    assert!(sf_set(regs.rflags), "SF should be set (result negative)");
}

#[test]
fn test_inc_al_preserves_cf() {
    // INC should NOT affect CF flag
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    regs.rflags = 0x2 | flags::bits::CF; // CF=1 initially
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0);
    assert!(cf_set(regs.rflags), "CF should be preserved (still set)");
}

#[test]
fn test_inc_al_memory() {
    // INC BYTE PTR [RBX]
    let code = [0xfe, 0x03, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u8(&mem, 42);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u8(&mem), 43, "INC [RBX]: 42 + 1 = 43");
}

// ============================================================================
// 8-bit DEC (opcode FE /1)
// ============================================================================

#[test]
fn test_dec_al_basic() {
    let code = [0xfe, 0xc8, 0xf4]; // DEC AL
    let mut regs = Registers::default();
    regs.rax = 10;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 9, "DEC AL: 10 - 1 = 9");
    assert!(!zf_set(regs.rflags), "ZF should be clear");
}

#[test]
fn test_dec_al_zero() {
    // DEC AL when AL = 0x00 -> 0xFF
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x00;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0xFF, "DEC AL: 0x00 - 1 = 0xFF");
    assert!(sf_set(regs.rflags), "SF should be set (result negative)");
}

#[test]
fn test_dec_al_underflow() {
    // DEC AL when AL = 0x80 -> 0x7F (signed overflow to positive)
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x80; // -128
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x7F, "DEC AL: 0x80 - 1 = 0x7F");
    assert!(of_set(regs.rflags), "OF should be set (signed underflow)");
    assert!(!sf_set(regs.rflags), "SF should be clear (result positive)");
}

#[test]
fn test_dec_al_preserves_cf() {
    // DEC should NOT affect CF flag
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x80;
    regs.rflags = 0x2; // CF=0
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(!cf_set(regs.rflags), "CF should be preserved (still clear)");
}

#[test]
fn test_dec_al_memory() {
    // DEC BYTE PTR [RBX]
    let code = [0xfe, 0x0b, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u8(&mem, 42);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u8(&mem), 41, "DEC [RBX]: 42 - 1 = 41");
}

// ============================================================================
// 16-bit INC (opcode FF /0 with 0x66 prefix)
// ============================================================================

#[test]
fn test_inc_ax_basic() {
    let code = [0x66, 0xff, 0xc0, 0xf4]; // INC AX
    let mut regs = Registers::default();
    regs.rax = 0x1234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0x1235, "INC AX: 0x1234 + 1 = 0x1235");
}

#[test]
fn test_inc_ax_overflow() {
    // INC AX when AX = 0xFFFF -> 0x0000
    let code = [0x66, 0xff, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0, "INC AX: 0xFFFF + 1 = 0");
    assert!(zf_set(regs.rflags), "ZF should be set");
}

#[test]
fn test_inc_ax_memory() {
    // INC WORD PTR [RBX]
    let code = [0x66, 0xff, 0x03, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u16(&mem, 0x1234);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u16(&mem), 0x1235);
}

// ============================================================================
// 16-bit DEC (opcode FF /1 with 0x66 prefix)
// ============================================================================

#[test]
fn test_dec_ax_basic() {
    let code = [0x66, 0xff, 0xc8, 0xf4]; // DEC AX
    let mut regs = Registers::default();
    regs.rax = 0x1234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0x1233, "DEC AX: 0x1234 - 1 = 0x1233");
}

#[test]
fn test_dec_ax_underflow() {
    // DEC AX when AX = 0x0000 -> 0xFFFF
    let code = [0x66, 0xff, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x0000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0xFFFF, "DEC AX: 0x0000 - 1 = 0xFFFF");
    assert!(sf_set(regs.rflags), "SF should be set");
}

#[test]
fn test_dec_ax_memory() {
    // DEC WORD PTR [RBX]
    let code = [0x66, 0xff, 0x0b, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u16(&mem, 0x1234);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u16(&mem), 0x1233);
}

// ============================================================================
// 32-bit INC (opcode FF /0)
// ============================================================================

#[test]
fn test_inc_eax_basic() {
    let code = [0xff, 0xc0, 0xf4]; // INC EAX
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x12345679, "INC EAX: 0x12345678 + 1");
}

#[test]
fn test_inc_eax_overflow() {
    // INC EAX when EAX = 0xFFFFFFFF -> 0x00000000
    let code = [0xff, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "INC EAX: 0xFFFFFFFF + 1 = 0");
    assert!(zf_set(regs.rflags), "ZF should be set");
}

#[test]
fn test_inc_eax_signed_overflow() {
    // INC EAX when EAX = 0x7FFFFFFF -> 0x80000000 (signed overflow)
    let code = [0xff, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x7FFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x80000000);
    assert!(of_set(regs.rflags), "OF should be set (signed overflow)");
}

#[test]
fn test_inc_eax_memory() {
    // INC DWORD PTR [RBX]
    let code = [0xff, 0x03, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u32(&mem, 0x12345678);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0x12345679);
}

// ============================================================================
// 32-bit DEC (opcode FF /1)
// ============================================================================

#[test]
fn test_dec_eax_basic() {
    let code = [0xff, 0xc8, 0xf4]; // DEC EAX
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x12345677, "DEC EAX: 0x12345678 - 1");
}

#[test]
fn test_dec_eax_underflow() {
    // DEC EAX when EAX = 0x00000000 -> 0xFFFFFFFF
    let code = [0xff, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x00000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0xFFFFFFFF, "DEC EAX: 0x00000000 - 1 = 0xFFFFFFFF");
    assert!(sf_set(regs.rflags), "SF should be set");
}

#[test]
fn test_dec_eax_signed_underflow() {
    // DEC EAX when EAX = 0x80000000 -> 0x7FFFFFFF (signed underflow)
    let code = [0xff, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x80000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x7FFFFFFF);
    assert!(of_set(regs.rflags), "OF should be set (signed underflow)");
    assert!(!sf_set(regs.rflags), "SF should be clear");
}

#[test]
fn test_dec_eax_memory() {
    // DEC DWORD PTR [RBX]
    let code = [0xff, 0x0b, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u32(&mem, 0x12345678);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u32(&mem), 0x12345677);
}

// ============================================================================
// 64-bit INC (opcode REX.W FF /0)
// ============================================================================

#[test]
fn test_inc_rax_basic() {
    let code = [0x48, 0xff, 0xc0, 0xf4]; // INC RAX
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEF1, "INC RAX");
}

#[test]
fn test_inc_rax_overflow() {
    // INC RAX when RAX = 0xFFFFFFFFFFFFFFFF -> 0x0000000000000000
    let code = [0x48, 0xff, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFFFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "INC RAX: max + 1 = 0");
    assert!(zf_set(regs.rflags), "ZF should be set");
}

#[test]
fn test_inc_rax_signed_overflow() {
    // INC RAX when RAX = 0x7FFFFFFFFFFFFFFF -> 0x8000000000000000
    let code = [0x48, 0xff, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x7FFFFFFFFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x8000000000000000);
    assert!(of_set(regs.rflags), "OF should be set (signed overflow)");
    assert!(sf_set(regs.rflags), "SF should be set");
}

#[test]
fn test_inc_rax_memory() {
    // INC QWORD PTR [RBX]
    let code = [0x48, 0xff, 0x03, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u64(&mem, 0xFEDCBA9876543210);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 0xFEDCBA9876543211);
}

// ============================================================================
// 64-bit DEC (opcode REX.W FF /1)
// ============================================================================

#[test]
fn test_dec_rax_basic() {
    let code = [0x48, 0xff, 0xc8, 0xf4]; // DEC RAX
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEEF, "DEC RAX");
}

#[test]
fn test_dec_rax_underflow() {
    // DEC RAX when RAX = 0x0000000000000000 -> 0xFFFFFFFFFFFFFFFF
    let code = [0x48, 0xff, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x0000000000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0xFFFFFFFFFFFFFFFF, "DEC RAX: 0 - 1 = max");
    assert!(sf_set(regs.rflags), "SF should be set");
}

#[test]
fn test_dec_rax_signed_underflow() {
    // DEC RAX when RAX = 0x8000000000000000 -> 0x7FFFFFFFFFFFFFFF
    let code = [0x48, 0xff, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x8000000000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x7FFFFFFFFFFFFFFF);
    assert!(of_set(regs.rflags), "OF should be set (signed underflow)");
    assert!(!sf_set(regs.rflags), "SF should be clear");
}

#[test]
fn test_dec_rax_memory() {
    // DEC QWORD PTR [RBX]
    let code = [0x48, 0xff, 0x0b, 0xf4];
    let mut regs = Registers::default();
    regs.rbx = DATA_ADDR;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));

    write_mem_u64(&mem, 0xFEDCBA9876543210);

    let _ = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(read_mem_u64(&mem), 0xFEDCBA987654320F);
}

// ============================================================================
// Different Registers
// ============================================================================

#[test]
fn test_inc_ecx() {
    let code = [0xff, 0xc1, 0xf4]; // INC ECX
    let mut regs = Registers::default();
    regs.rcx = 99;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 100, "INC ECX: 99 + 1 = 100");
}

#[test]
fn test_dec_ecx() {
    let code = [0xff, 0xc9, 0xf4]; // DEC ECX
    let mut regs = Registers::default();
    regs.rcx = 100;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 99, "DEC ECX: 100 - 1 = 99");
}

#[test]
fn test_inc_cl() {
    let code = [0xfe, 0xc1, 0xf4]; // INC CL
    let mut regs = Registers::default();
    regs.rcx = 255;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFF, 0, "INC CL: 255 + 1 = 0 (wraps)");
}

#[test]
fn test_dec_cl() {
    let code = [0xfe, 0xc9, 0xf4]; // DEC CL
    let mut regs = Registers::default();
    regs.rcx = 0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFF, 0xFF, "DEC CL: 0 - 1 = 0xFF");
}

// ============================================================================
// Flag Tests
// ============================================================================

#[test]
fn test_inc_parity_flag() {
    // INC should set parity based on result
    // 0x02 + 1 = 0x03 (binary 00000011, 2 bits = even parity)
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x02;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x03);
    assert!(pf_set(regs.rflags), "PF should be set (even parity)");
}

#[test]
fn test_inc_auxiliary_flag() {
    // AF should be set when carry from bit 3 to bit 4
    // 0x0F + 1 = 0x10 (carry from bit 3)
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x0F;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x10);
    assert!(af_set(regs.rflags), "AF should be set (carry from bit 3)");
}

#[test]
fn test_inc_no_auxiliary_flag() {
    // 0x0E + 1 = 0x0F (no carry from bit 3)
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x0E;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x0F);
    assert!(!af_set(regs.rflags), "AF should be clear");
}

#[test]
fn test_dec_parity_flag() {
    // DEC should set parity based on result
    // 0x04 - 1 = 0x03 (binary 00000011, 2 bits = even parity)
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x04;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x03);
    assert!(pf_set(regs.rflags), "PF should be set (even parity)");
}

#[test]
fn test_dec_auxiliary_flag() {
    // AF should be set when borrow from bit 4 to bit 3
    // 0x10 - 1 = 0x0F (borrow from bit 4)
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x10;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x0F);
    assert!(af_set(regs.rflags), "AF should be set (borrow from bit 4)");
}

#[test]
fn test_inc_cf_independence() {
    // Verify CF is completely independent
    // Test 1: CF=0 initially
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    regs.rflags = 0x2; // CF=0
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert!(!cf_set(regs.rflags), "CF should remain clear");

    // Test 2: CF=1 initially
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xFF;
    regs.rflags = 0x2 | flags::bits::CF; // CF=1
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert!(cf_set(regs.rflags), "CF should remain set");
}

#[test]
fn test_dec_cf_independence() {
    // Verify CF is completely independent for DEC
    // Test 1: CF=0 initially
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x00;
    regs.rflags = 0x2; // CF=0
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert!(!cf_set(regs.rflags), "CF should remain clear");

    // Test 2: CF=1 initially
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0x00;
    regs.rflags = 0x2 | flags::bits::CF; // CF=1
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert!(cf_set(regs.rflags), "CF should remain set");
}

// ============================================================================
// Extended Registers (R8-R15)
// ============================================================================

#[test]
fn test_inc_r8d_extended() {
    let code = [0x41, 0xff, 0xc0, 0xf4]; // INC R8D
    let mut regs = Registers::default();
    regs.r8 = 100;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8, 101, "INC R8D: 100 + 1 = 101");
}

#[test]
fn test_dec_r8d_extended() {
    let code = [0x41, 0xff, 0xc8, 0xf4]; // DEC R8D
    let mut regs = Registers::default();
    regs.r8 = 100;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8, 99, "DEC R8D: 100 - 1 = 99");
}

#[test]
fn test_inc_r15_extended() {
    let code = [0x49, 0xff, 0xc7, 0xf4]; // INC R15
    let mut regs = Registers::default();
    regs.r15 = 0xFFFFFFFFFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r15, 0, "INC R15: max + 1 = 0");
    assert!(zf_set(regs.rflags), "ZF should be set");
}

#[test]
fn test_dec_r15_extended() {
    let code = [0x49, 0xff, 0xcf, 0xf4]; // DEC R15
    let mut regs = Registers::default();
    regs.r15 = 0x0000000000000000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r15, 0xFFFFFFFFFFFFFFFF, "DEC R15: 0 - 1 = max");
    assert!(sf_set(regs.rflags), "SF should be set");
}

#[test]
fn test_inc_r8l_byte() {
    let code = [0x41, 0xfe, 0xc0, 0xf4]; // INC R8L
    let mut regs = Registers::default();
    regs.r8 = 0xFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8 & 0xFF, 0, "INC R8L: 0xFF + 1 = 0x00");
}

#[test]
fn test_dec_r8l_byte() {
    let code = [0x41, 0xfe, 0xc8, 0xf4]; // DEC R8L
    let mut regs = Registers::default();
    regs.r8 = 0x00;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8 & 0xFF, 0xFF, "DEC R8L: 0x00 - 1 = 0xFF");
}

// ============================================================================
// Loop Counter Use Case
// ============================================================================

#[test]
fn test_inc_as_loop_counter() {
    // Simulating loop: increment counter 3 times
    let code = [
        0xff, 0xc0, // INC EAX
        0xff, 0xc0, // INC EAX
        0xff, 0xc0, // INC EAX
        0xf4,       // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 3, "Three INC operations: 0 + 1 + 1 + 1 = 3");
}

#[test]
fn test_dec_as_loop_counter() {
    // Simulating loop: decrement counter 3 times
    let code = [
        0xff, 0xc8, // DEC EAX
        0xff, 0xc8, // DEC EAX
        0xff, 0xc8, // DEC EAX
        0xf4,       // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 5;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 2, "Three DEC operations: 5 - 1 - 1 - 1 = 2");
}

#[test]
fn test_inc_preserves_high_bytes() {
    // Verify INC AL doesn't affect high bytes
    let code = [0xfe, 0xc0, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xDEADBEEF_12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x79);
    assert_eq!(regs.rax & !0xFF, 0xDEADBEEF_12345600, "High bytes should be preserved");
}

#[test]
fn test_dec_preserves_high_bytes() {
    // Verify DEC AL doesn't affect high bytes
    let code = [0xfe, 0xc8, 0xf4];
    let mut regs = Registers::default();
    regs.rax = 0xDEADBEEF_12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x77);
    assert_eq!(regs.rax & !0xFF, 0xDEADBEEF_12345600, "High bytes should be preserved");
}
