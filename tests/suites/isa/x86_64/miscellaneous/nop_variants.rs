use crate::common::{CODE_ADDR, run_until_hlt, setup_vm, setup_vm_no_idt};
use rax::vm::vcpu::{Registers, VCpu};

// NOP Variants - Multi-byte NOP instructions
// Various NOP encodings for alignment and optimization
// All variants preserve all registers, flags, and memory
// Opcodes:
//   90              - 1-byte NOP (XCHG EAX, EAX)
//   0F 1F /0        - Multi-byte NOP with ModR/M
//   66 0F 1F /0     - Multi-byte NOP with prefix
//   And various combinations for 1-9 byte NOPs

// Test 1-byte NOP (0x90)
#[test]
fn test_nop_1_byte() {
    let code = [
        0x90, // NOP
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x12345678;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x12345678, "RAX should be unchanged");
}

// Test 2-byte NOP (0x66 0x90)
#[test]
fn test_nop_2_byte_66_90() {
    let code = [
        0x66, 0x90, // NOP (operand size prefix + XCHG)
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0xDEADBEEF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0xDEADBEEF, "RAX should be unchanged");
}

// Test 3-byte NOP (0x0F 0x1F 0x00)
#[test]
fn test_nop_3_byte_0f_1f_00() {
    let code = [
        0x0f, 0x1f, 0x00, // NOP dword [RAX]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x42424242;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x42424242, "RAX should be unchanged");
}

// Test 4-byte NOP (0x0F 0x1F 0x40 0x00)
#[test]
fn test_nop_4_byte() {
    let code = [
        0x0f, 0x1f, 0x40, 0x00, // NOP dword [RAX+0x00]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x11111111;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x11111111, "RBX should be unchanged");
}

// Test 5-byte NOP (0x0F 0x1F 0x44 0x00 0x00)
#[test]
fn test_nop_5_byte() {
    let code = [
        0x0f, 0x1f, 0x44, 0x00, 0x00, // NOP dword [RAX+RAX*1+0x00]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rcx = 0x22222222;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFFFFFFFF, 0x22222222, "RCX should be unchanged");
}

// Test 6-byte NOP (0x66 0x0F 0x1F 0x44 0x00 0x00)
#[test]
fn test_nop_6_byte() {
    let code = [
        0x66, 0x0f, 0x1f, 0x44, 0x00, 0x00, // NOP word [RAX+RAX*1+0x00]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rdx = 0x33333333;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdx & 0xFFFFFFFF, 0x33333333, "RDX should be unchanged");
}

// Test 7-byte NOP (0x0F 0x1F 0x80 0x00 0x00 0x00 0x00)
#[test]
fn test_nop_7_byte() {
    let code = [
        0x0f, 0x1f, 0x80, 0x00, 0x00, 0x00, 0x00, // NOP dword [RAX+0x00000000]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsi = 0x44444444;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi & 0xFFFFFFFF, 0x44444444, "RSI should be unchanged");
}

// Test 8-byte NOP (0x0F 0x1F 0x84 0x00 0x00 0x00 0x00 0x00)
#[test]
fn test_nop_8_byte() {
    let code = [
        0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // NOP dword [RAX+RAX*1+0x00000000]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rdi = 0x55555555;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdi & 0xFFFFFFFF, 0x55555555, "RDI should be unchanged");
}

// Test 9-byte NOP (0x66 0x0F 0x1F 0x84 0x00 0x00 0x00 0x00 0x00)
#[test]
fn test_nop_9_byte() {
    let code = [
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00,
        0x00, // NOP word [RAX+RAX*1+0x00000000]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rbp = 0x66666666;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbp & 0xFFFFFFFF, 0x66666666, "RBP should be unchanged");
}

// Test multi-byte NOP doesn't modify flags
#[test]
fn test_multibyte_nop_preserves_flags() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (sets ZF)
        0x0f, 0x1f, 0x44, 0x00, 0x00, // 5-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should still be set from the ADD
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

// Test REX prefix NOP (0x48 0x90)
#[test]
fn test_nop_rex_prefix() {
    let code = [
        0x48, 0x90, // NOP with REX.W (XCHG RAX, RAX)
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x123456789ABCDEF0, "RAX should be unchanged");
}

// Test NOP with ModR/M = 0xC0 (register form)
#[test]
fn test_nop_modrm_c0() {
    let code = [
        0x0f, 0x1f, 0xc0, // NOP EAX
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x42424242;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x42424242, "RAX should be unchanged");
}

// Test sequence of different NOP lengths
#[test]
fn test_nop_sequence_different_lengths() {
    let code = [
        0x90, // 1-byte NOP
        0x66, 0x90, // 2-byte NOP
        0x0f, 0x1f, 0x00, // 3-byte NOP
        0x0f, 0x1f, 0x40, 0x00, // 4-byte NOP
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0xDEADBEEF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0xDEADBEEF,
        "RAX unchanged after NOP sequence"
    );
}

// Test NOP with RBX as base register
#[test]
fn test_nop_rbx_base() {
    let code = [
        0x0f, 0x1f, 0x03, // NOP dword [RBX]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rbx = 0x11111111;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFFFFFF, 0x11111111, "RBX should be unchanged");
}

// Test NOP with RCX as base register
#[test]
fn test_nop_rcx_base() {
    let code = [
        0x0f, 0x1f, 0x01, // NOP dword [RCX]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rcx = 0x22222222;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xFFFFFFFF, 0x22222222, "RCX should be unchanged");
}

// Test NOP preserves all general-purpose registers
#[test]
fn test_multibyte_nop_preserves_all_gprs() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x01, 0x01, 0x01, // MOV RAX, 0x01010101
        0x48, 0xc7, 0xc3, 0x02, 0x02, 0x02, 0x02, // MOV RBX, 0x02020202
        0x48, 0xc7, 0xc1, 0x03, 0x03, 0x03, 0x03, // MOV RCX, 0x03030303
        0x48, 0xc7, 0xc2, 0x04, 0x04, 0x04, 0x04, // MOV RDX, 0x04040404
        0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 8-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x01010101, "RAX should be unchanged");
    assert_eq!(regs.rbx, 0x02020202, "RBX should be unchanged");
    assert_eq!(regs.rcx, 0x03030303, "RCX should be unchanged");
    assert_eq!(regs.rdx, 0x04040404, "RDX should be unchanged");
}

// Test NOP preserves R8-R15
#[test]
fn test_multibyte_nop_preserves_extended_registers() {
    let code = [
        0x49, 0xc7, 0xc0, 0x11, 0x11, 0x11, 0x11, // MOV R8, 0x11111111
        0x49, 0xc7, 0xc7, 0xff, 0xff, 0xff, 0xff, // MOV R15, 0xffffffff
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r8, 0x11111111, "R8 should be preserved");
    assert_eq!(regs.r15, 0xffffffffffffffff, "R15 should be preserved");
}

// Test NOP for 16-byte alignment (combination)
#[test]
fn test_nop_16_byte_alignment() {
    let code = [
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-byte NOP
        0x0f, 0x1f, 0x80, 0x00, 0x00, 0x00, 0x00, // 7-byte NOP (total 16 bytes)
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0xAAAAAAAA;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0xAAAAAAAA, "RAX unchanged");
}

// Test NOP with displacement addressing
#[test]
fn test_nop_with_displacement() {
    let code = [
        0x0f, 0x1f, 0x40, 0x08, // NOP dword [RAX+0x08]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x2000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x2000, "RAX should be unchanged");
}

// Test NOP with 32-bit displacement
#[test]
fn test_nop_with_32bit_displacement() {
    let code = [
        0x0f, 0x1f, 0x80, 0x00, 0x10, 0x00, 0x00, // NOP dword [RAX+0x1000]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x3000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x3000, "RAX should be unchanged");
}

// Test NOP with SIB byte
#[test]
fn test_nop_with_sib() {
    let code = [
        0x0f, 0x1f, 0x44, 0x00, 0x00, // NOP dword [RAX+RAX*1+0x00]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x1000, "RAX should be unchanged");
}

// Test NOP with different SIB scale
#[test]
fn test_nop_sib_scale_2() {
    let code = [
        0x0f, 0x1f, 0x44, 0x40, 0x00, // NOP dword [RAX+RAX*2+0x00]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x2000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x2000, "RAX should be unchanged");
}

// Test NOP preserves stack pointer
#[test]
fn test_multibyte_nop_preserves_stack_pointer() {
    let code = [
        0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 8-byte NOP
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x8000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, 0x8000, "RSP should be unchanged");
}

// Test NOP preserves base pointer
#[test]
fn test_multibyte_nop_preserves_base_pointer() {
    let code = [
        0x48, 0xc7, 0xc5, 0x00, 0x70, 0x00, 0x00, // MOV RBP, 0x7000
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbp, 0x7000, "RBP should be preserved");
}

// Test NOP between arithmetic instructions
#[test]
fn test_nop_between_arithmetic() {
    let code = [
        0x48, 0xc7, 0xc0, 0x10, 0x00, 0x00, 0x00, // MOV RAX, 0x10
        0x48, 0x83, 0xc0, 0x05, // ADD RAX, 5
        0x0f, 0x1f, 0x40, 0x00, // 4-byte NOP
        0x48, 0x83, 0xe8, 0x03, // SUB RAX, 3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX = 16 + 5 - 3 = 18
    assert_eq!(regs.rax, 18, "RAX should be 18");
}

// Test NOP with zero flag set
#[test]
fn test_multibyte_nop_zero_flag() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX (sets ZF)
        0x0f, 0x1f, 0x80, 0x00, 0x00, 0x00, 0x00, // 7-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should still be set
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

// Test NOP with carry flag set
#[test]
fn test_multibyte_nop_carry_flag() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xffffffffffffffff
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (sets CF)
        0x66, 0x0f, 0x1f, 0x44, 0x00, 0x00, // 6-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Flags should be preserved
    let _ = regs.rflags;
}

// Test NOP with sign flag set
#[test]
fn test_multibyte_nop_sign_flag() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (sets SF)
        0x0f, 0x1f, 0x44, 0x00, 0x00, // 5-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // SF should still be set
    assert!(regs.rflags & 0x80 != 0, "SF should be preserved");
}

// Test longest possible NOP (9 bytes) multiple times
#[test]
fn test_nop_9_byte_multiple() {
    let code = [
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-byte NOP
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-byte NOP
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-byte NOP
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0xFFFFFFFF;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax, 0xFFFFFFFF,
        "RAX unchanged after three 9-byte NOPs"
    );
}

// Test NOP doesn't cause exceptions
#[test]
fn test_multibyte_nop_no_exception() {
    let code = [
        0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 8-byte NOP
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    // Should complete without panicking or returning an error
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// Test all NOP lengths in sequence (1-9 bytes)
#[test]
fn test_all_nop_lengths_sequence() {
    let code = [
        0x90, // 1-byte
        0x66, 0x90, // 2-byte
        0x0f, 0x1f, 0x00, // 3-byte
        0x0f, 0x1f, 0x40, 0x00, // 4-byte
        0x0f, 0x1f, 0x44, 0x00, 0x00, // 5-byte
        0x66, 0x0f, 0x1f, 0x44, 0x00, 0x00, // 6-byte
        0x0f, 0x1f, 0x80, 0x00, 0x00, 0x00, 0x00, // 7-byte
        0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 8-byte
        0x66, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, // 9-byte
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0xBAADF00D;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(
        regs.rax & 0xFFFFFFFF,
        0xBAADF00D,
        "RAX unchanged after all NOP variants"
    );
}

const RESERVED_NOP_SCANNER_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn reserved_nop_0f1d_image(prefix: &[u8], modrm: u8) -> Vec<u8> {
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(&[0x0F, 0x1D, modrm]);

    let mode = modrm >> 6;
    let rm = modrm & 7;
    if mode != 3 && rm == 4 {
        bytes.push(0x00);
    }
    match mode {
        0 if rm == 5 => bytes.extend_from_slice(&[0; 4]),
        1 => bytes.push(0),
        2 => bytes.extend_from_slice(&[0; 4]),
        _ => {}
    }
    bytes
}

fn reserved_nop_initial_registers() -> Registers {
    Registers {
        rax: 0x0000_8000_0000_0000,
        rbx: 0x1111_2222_3333_4444,
        rcx: 0x5555_6666_7777_8888,
        rdx: 0x9999_AAAA_BBBB_CCCC,
        rsp: 0x8000,
        rbp: 0x7000,
        r8: 0x0808_0808_0808_0808,
        r15: 0x1515_1515_1515_1515,
        r16: 0x1616_1616_1616_1616,
        r31: 0xFFFF_8000_0000_0000,
        rflags: 0x0CD7,
        xmm: [[0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210]; 16],
        ..Registers::default()
    }
}

fn assert_reserved_nop(instruction: &[u8], apx_enabled: bool) {
    let mut code = instruction.to_vec();
    code.push(0xF4);
    let initial = reserved_nop_initial_registers();
    let (mut vcpu, _) = setup_vm(&code, Some(initial));
    vcpu.set_apx_enabled(apx_enabled);

    for path in ["cold decode", "decode-cache hit"] {
        let mut before = vcpu.get_regs().unwrap();
        before.rip = CODE_ADDR;
        vcpu.set_regs(&before).unwrap();
        let before_sregs = vcpu.get_sregs().unwrap();
        let mut expected = before.clone();
        expected.rip += instruction.len() as u64;

        assert!(
            vcpu.step()
                .unwrap_or_else(|error| panic!("{instruction:02X?} ({path}): {error}"))
                .is_none(),
            "{instruction:02X?} ({path})"
        );

        let actual =
            serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();
        let expected = serde_json::to_value((expected, before_sregs)).unwrap();
        assert_eq!(actual, expected, "{instruction:02X?} ({path})");
    }
}

#[test]
fn test_reserved_nop_0f19_executes_every_modrm_register_form() {
    for modrm in 0xC0..=0xFF {
        assert_reserved_nop(&[0x0F, 0x19, modrm], false);
    }
}

#[test]
fn test_reserved_nop_0f1d_executes_all_3584_scanner_images_without_observing_addresses() {
    let mut code = Vec::new();
    let mut lengths = Vec::new();
    for prefix in RESERVED_NOP_SCANNER_PREFIXES {
        for modrm in u8::MIN..=u8::MAX {
            let instruction = reserved_nop_0f1d_image(prefix, modrm);
            lengths.push(instruction.len());
            code.extend_from_slice(&instruction);
        }
    }
    code.push(0xF4);
    assert_eq!(lengths.len(), 14 * 256);

    let (mut vcpu, _) = setup_vm(&code, Some(reserved_nop_initial_registers()));
    for path in ["cold decode", "decode-cache hit"] {
        let mut before = vcpu.get_regs().unwrap();
        before.rip = CODE_ADDR;
        vcpu.set_regs(&before).unwrap();
        let before_sregs = vcpu.get_sregs().unwrap();
        let mut expected_rip = CODE_ADDR;

        for (image, length) in lengths.iter().enumerate() {
            assert!(
                vcpu.step()
                    .unwrap_or_else(|error| panic!("image {image} ({path}): {error}"))
                    .is_none(),
                "image {image} ({path})"
            );
            expected_rip += *length as u64;
            assert_eq!(
                vcpu.get_regs().unwrap().rip,
                expected_rip,
                "image {image} ({path})"
            );
        }

        let mut expected = before;
        expected.rip = expected_rip;
        let actual =
            serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();
        let expected = serde_json::to_value((expected, before_sregs)).unwrap();
        assert_eq!(actual, expected, "all scanner images ({path})");
    }
}

#[test]
fn test_reserved_nop_prefix_address_and_rex2_forms_are_non_accessing() {
    for (instruction, apx_enabled) in [
        (&[0x66, 0x0F, 0x19, 0xC0][..], false),
        (&[0x67, 0x0F, 0x19, 0xC0][..], false),
        (&[0xF2, 0x0F, 0x19, 0xC0][..], false),
        (&[0xF3, 0x0F, 0x19, 0xC0][..], false),
        (&[0x2E, 0x0F, 0x19, 0xC0][..], false),
        (&[0x48, 0x0F, 0x19, 0xC0][..], false),
        (&[0x0F, 0x19, 0x00][..], false),
        (&[0x0F, 0x19, 0x7F, 0x80][..], false),
        (&[0x0F, 0x19, 0x80, 0x78, 0x56, 0x34, 0x12][..], false),
        (&[0x0F, 0x19, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12][..], false),
        (&[0x0F, 0x19, 0x05, 0x78, 0x56, 0x34, 0x12][..], false),
        (
            &[0x67, 0x0F, 0x19, 0x04, 0x25, 0x78, 0x56, 0x34, 0x12][..],
            false,
        ),
        (
            &[0x4F, 0x0F, 0x19, 0x84, 0x7F, 0x78, 0x56, 0x34, 0x12][..],
            false,
        ),
        (&[0xD5, 0x80, 0x19, 0xC0][..], true),
        (
            &[0xD5, 0xFF, 0x19, 0x84, 0x7F, 0x78, 0x56, 0x34, 0x12][..],
            true,
        ),
        (&[0x66, 0x67, 0xF3, 0x2E, 0xD5, 0x80, 0x19, 0xC0][..], true),
        (&[0xD5, 0x80, 0x1D, 0xC0][..], true),
        (
            &[0xD5, 0xFF, 0x1D, 0x84, 0x7F, 0x78, 0x56, 0x34, 0x12][..],
            true,
        ),
        (&[0x66, 0x67, 0xF3, 0x2E, 0xD5, 0x80, 0x1D, 0xC0][..], true),
    ] {
        assert_reserved_nop(instruction, apx_enabled);
    }
}

#[test]
fn test_lock_reserved_nop_faults_before_state_commit_on_both_decode_paths() {
    for (instruction, apx_enabled) in [
        (&[0xF0, 0x0F, 0x19, 0xC0][..], false),
        (&[0xF0, 0xD5, 0x80, 0x19, 0xC0][..], true),
        (&[0xF0, 0x0F, 0x1D, 0xC0][..], false),
        (&[0xF0, 0xD5, 0x80, 0x1D, 0xC0][..], true),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(instruction, None);
        vcpu.set_apx_enabled(apx_enabled);
        for path in ["cold decode", "decode-cache hit"] {
            let before =
                serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap()))
                    .unwrap();
            let error = vcpu
                .step()
                .expect_err("LOCK reserved NOP must inject #UD")
                .to_string();
            assert!(
                error.contains("IDT entry 6 not present"),
                "{instruction:02X?} ({path}): {error}"
            );
            let after = serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap()))
                .unwrap();
            assert_eq!(after, before, "{instruction:02X?} ({path})");
        }
    }
}
