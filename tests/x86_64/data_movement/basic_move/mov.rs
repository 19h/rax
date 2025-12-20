#[path = "../../common/mod.rs"]
mod common;

use common::*;
use rax::cpu::Registers;

// MOV - Move Data
// Copies the source operand to the destination operand.
// The source operand can be an immediate, register, or memory location.
// The destination can be a register or memory location.
// Both operands cannot be memory locations simultaneously.
// Flags are not affected.
//
// Opcodes (representative selection):
// 88 /r       MOV r/m8, r8        - Move r8 to r/m8
// 89 /r       MOV r/m16, r16      - Move r16 to r/m16
// 89 /r       MOV r/m32, r32      - Move r32 to r/m32
// REX.W 89 /r MOV r/m64, r64      - Move r64 to r/m64
// 8A /r       MOV r8, r/m8        - Move r/m8 to r8
// 8B /r       MOV r16, r/m16      - Move r/m16 to r16
// 8B /r       MOV r32, r/m32      - Move r/m32 to r32
// REX.W 8B /r MOV r64, r/m64      - Move r/m64 to r64
// B0+rb ib    MOV r8, imm8        - Move imm8 to r8
// B8+rw iw    MOV r16, imm16      - Move imm16 to r16
// B8+rd id    MOV r32, imm32      - Move imm32 to r32
// REX.W B8+rd io MOV r64, imm64   - Move imm64 to r64
// C6 /0 ib    MOV r/m8, imm8      - Move imm8 to r/m8
// C7 /0 iw    MOV r/m16, imm16    - Move imm16 to r/m16
// C7 /0 id    MOV r/m32, imm32    - Move imm32 to r/m32
// REX.W C7 /0 id MOV r/m64, imm32 - Move sign-extended imm32 to r/m64

#[test]
fn test_mov_al_bl() {
    // MOV AL, BL - register to register (8-bit)
    let code = [
        0x88, 0xd8, // MOV AL, BL
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x42;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x42, "AL should contain 0x42");
}

#[test]
fn test_mov_ax_bx() {
    // MOV AX, BX - register to register (16-bit)
    let code = [
        0x66, 0x89, 0xd8, // MOV AX, BX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x1234;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0x1234, "AX should contain 0x1234");
}

#[test]
fn test_mov_eax_ebx() {
    // MOV EAX, EBX - register to register (32-bit)
    let code = [
        0x89, 0xd8, // MOV EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x12345678, "EAX should contain 0x12345678");
}

#[test]
fn test_mov_rax_rbx() {
    // MOV RAX, RBX - register to register (64-bit)
    let code = [
        0x48, 0x89, 0xd8, // MOV RAX, RBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEF0, "RAX should contain full 64-bit value");
}

#[test]
fn test_mov_al_imm8() {
    // MOV AL, imm8
    let code = [
        0xb0, 0x42, // MOV AL, 0x42
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0x42, "AL should contain 0x42");
}

#[test]
fn test_mov_ax_imm16() {
    // MOV AX, imm16
    let code = [
        0x66, 0xb8, 0x34, 0x12, // MOV AX, 0x1234
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0x1234, "AX should contain 0x1234");
}

#[test]
fn test_mov_eax_imm32() {
    // MOV EAX, imm32
    let code = [
        0xb8, 0x78, 0x56, 0x34, 0x12, // MOV EAX, 0x12345678
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x12345678, "EAX should contain 0x12345678");
}

#[test]
fn test_mov_rax_imm64() {
    // MOV RAX, imm64
    let code = [
        0x48, 0xb8, 0xf0, 0xde, 0xbc, 0x9a, 0x78, 0x56, 0x34, 0x12, // MOV RAX, 0x123456789ABCDEF0
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEF0, "RAX should contain full 64-bit immediate");
}

#[test]
fn test_mov_mem8_reg() {
    // MOV [mem], AL
    let code = [
        0x88, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [DATA_ADDR], AL
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xAB;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let result = read_mem_u8(&mem);
    assert_eq!(result, 0xAB, "Memory should contain 0xAB");
}

#[test]
fn test_mov_mem16_reg() {
    // MOV [mem], AX
    let code = [
        0x66, 0x89, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [DATA_ADDR], AX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x1234;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let result = read_mem_u16(&mem);
    assert_eq!(result, 0x1234, "Memory should contain 0x1234");
}

#[test]
fn test_mov_mem32_reg() {
    // MOV [mem], EAX
    let code = [
        0x89, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [DATA_ADDR], EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let result = read_mem_u32(&mem);
    assert_eq!(result, 0x12345678, "Memory should contain 0x12345678");
}

#[test]
fn test_mov_mem64_reg() {
    // MOV [mem], RAX
    let code = [
        0x48, 0x89, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [DATA_ADDR], RAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDEF0;
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let result = read_mem_u64(&mem);
    assert_eq!(result, 0x123456789ABCDEF0, "Memory should contain full 64-bit value");
}

#[test]
fn test_mov_reg_mem8() {
    // MOV AL, [mem]
    let code = [
        0x8a, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV AL, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u8(&mem, 0xCD);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFF, 0xCD, "AL should contain value from memory");
}

#[test]
fn test_mov_reg_mem16() {
    // MOV AX, [mem]
    let code = [
        0x66, 0x8b, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV AX, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u16(&mem, 0xABCD);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, 0xABCD, "AX should contain value from memory");
}

#[test]
fn test_mov_reg_mem32() {
    // MOV EAX, [mem]
    let code = [
        0x8b, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV EAX, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u32(&mem, 0xDEADBEEF);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0xDEADBEEF, "EAX should contain value from memory");
}

#[test]
fn test_mov_reg_mem64() {
    // MOV RAX, [mem]
    let code = [
        0x48, 0x8b, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV RAX, [DATA_ADDR]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    write_mem_u64(&mem, 0xFEDCBA9876543210);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0xFEDCBA9876543210, "RAX should contain value from memory");
}

#[test]
fn test_mov_with_extended_registers() {
    // MOV R8D, R9D
    let code = [
        0x45, 0x89, 0xc8, // MOV R8D, R9D
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.r9 = 0x11223344;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8 & 0xFFFFFFFF, 0x11223344, "R8D should contain value from R9D");
}

#[test]
fn test_mov_r15_imm64() {
    // MOV R15, imm64
    let code = [
        0x49, 0xbf, 0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, // MOV R15, 0x8899AABBCCDDEEFF
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r15, 0x8899AABBCCDDEEFF, "R15 should contain immediate");
}

#[test]
fn test_mov_preserves_other_registers() {
    // MOV should only affect destination
    let code = [
        0x89, 0xd8, // MOV EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x12345678;
    regs.rcx = 0xAABBCCDD;
    regs.rdx = 0x11111111;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x12345678, "EBX should be unchanged");
    assert_eq!(regs.rcx & 0xFFFFFFFF, 0xAABBCCDD, "ECX should be unchanged");
    assert_eq!(regs.rdx & 0xFFFFFFFF, 0x11111111, "EDX should be unchanged");
}

#[test]
fn test_mov_does_not_affect_flags() {
    // MOV does not modify flags
    let code = [
        0x89, 0xd8, // MOV EAX, EBX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x12345678;
    regs.rflags = 0x2 | (1 << 6) | (1 << 11); // Set ZF and OF
    let initial_flags = regs.rflags;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rflags, initial_flags, "Flags should be unchanged");
}

#[test]
fn test_mov_zero_extension_32bit() {
    // 32-bit MOV zero-extends to 64-bit
    let code = [
        0xb8, 0xff, 0xff, 0xff, 0xff, // MOV EAX, 0xFFFFFFFF
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFFFFFFFFFF; // Set all bits
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x00000000FFFFFFFF, "Upper 32 bits should be zero");
}

#[test]
fn test_mov_no_zero_extension_8bit() {
    // 8-bit MOV does not zero-extend upper bits
    let code = [
        0xb0, 0xff, // MOV AL, 0xFF
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDE00;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEFF, "Only AL should be modified");
}

#[test]
fn test_mov_no_zero_extension_16bit() {
    // 16-bit MOV does not zero-extend upper bits
    let code = [
        0x66, 0xb8, 0xff, 0xff, // MOV AX, 0xFFFF
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABC0000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCFFFF, "Only AX should be modified");
}

#[test]
fn test_mov_self() {
    // MOV register to itself (NOP-like)
    let code = [
        0x89, 0xc0, // MOV EAX, EAX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x12345678, "EAX should be unchanged");
}

#[test]
fn test_mov_chain() {
    // Chain of MOV instructions
    let code = [
        0x89, 0xd8, // MOV EAX, EBX
        0x89, 0xc1, // MOV ECX, EAX
        0x89, 0xca, // MOV EDX, ECX
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xAABBCCDD;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0xAABBCCDD, "EAX should have value from EBX");
    assert_eq!(regs.rcx & 0xFFFFFFFF, 0xAABBCCDD, "ECX should have value from EAX");
    assert_eq!(regs.rdx & 0xFFFFFFFF, 0xAABBCCDD, "EDX should have value from ECX");
}

#[test]
fn test_mov_all_zeros() {
    // MOV with all zeros
    let code = [
        0xb8, 0x00, 0x00, 0x00, 0x00, // MOV EAX, 0
        0xf4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFFFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "RAX should be zero");
}

#[test]
fn test_mov_all_ones() {
    // MOV with all ones (32-bit)
    let code = [
        0xb8, 0xff, 0xff, 0xff, 0xff, // MOV EAX, 0xFFFFFFFF
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0xFFFFFFFF, "EAX should be all ones");
}

#[test]
fn test_mov_mem_imm8() {
    // MOV [mem], imm8
    let code = [
        0xc6, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, 0x42, // MOV byte [DATA_ADDR], 0x42
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let result = read_mem_u8(&mem);
    assert_eq!(result, 0x42, "Memory should contain 0x42");
}

#[test]
fn test_mov_mem_imm32() {
    // MOV [mem], imm32
    let code = [
        0xc7, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12, // MOV dword [DATA_ADDR], 0x12345678
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let result = read_mem_u32(&mem);
    assert_eq!(result, 0x12345678, "Memory should contain 0x12345678");
}

#[test]
fn test_mov_sequential_registers() {
    // MOV to sequential registers
    let code = [
        0xb8, 0x11, 0x11, 0x11, 0x11, // MOV EAX, 0x11111111
        0xbb, 0x22, 0x22, 0x22, 0x22, // MOV EBX, 0x22222222
        0xb9, 0x33, 0x33, 0x33, 0x33, // MOV ECX, 0x33333333
        0xba, 0x44, 0x44, 0x44, 0x44, // MOV EDX, 0x44444444
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x11111111, "EAX should be set");
    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x22222222, "EBX should be set");
    assert_eq!(regs.rcx & 0xFFFFFFFF, 0x33333333, "ECX should be set");
    assert_eq!(regs.rdx & 0xFFFFFFFF, 0x44444444, "EDX should be set");
}
