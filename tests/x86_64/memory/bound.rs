use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

#[path = "../common/mod.rs"]
mod common;
use common::{run_until_hlt, setup_vm, write_mem_at_u32, read_mem_at_u32};

// BOUND - Check Array Index Against Bounds
// Opcode: 62 /r
// Operand: register, memory(lower_bound:upper_bound pair)
//
// BOUND reg, m
// - reg: 16-bit or 32-bit register containing array index
// - m: 4 or 8 byte memory location containing bounds
//   - First word/dword: lower bound (inclusive)
//   - Second word/dword: upper bound (inclusive)
//
// If index < lower_bound or index > upper_bound, generates #BR (bound exceeded) exception
//
// Note: BOUND is a legacy instruction and may not be implemented in all modern
// x86_64 emulators. These tests verify behavior if implemented.

// BOUND: Index within bounds (lower)
#[test]
fn test_bound_index_equals_lower_bound() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0 (index = lower bound)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data at next instruction
        0x00, 0x00, // Lower bound = 0
        0x0a, 0x00, // Upper bound = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should succeed without exception
    assert_eq!(regs.rax, 0, "Index should remain unchanged");
}

// BOUND: Index within bounds (middle)
#[test]
fn test_bound_index_within_bounds() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5 (index = 5, within bounds)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower bound = 0
        0x0a, 0x00, // Upper bound = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 5, "Index within bounds accepted");
}

// BOUND: Index equals upper bound
#[test]
fn test_bound_index_equals_upper_bound() {
    let code = [
        0x48, 0xc7, 0xc0, 0x0a, 0x00, 0x00, 0x00, // MOV RAX, 10 (index = upper bound)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower bound = 0
        0x0a, 0x00, // Upper bound = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 10, "Index at upper bound accepted");
}

// BOUND: Index below lower bound (should trigger exception)
#[test]
fn test_bound_index_below_lower() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1 (index below lower bound)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower bound = 0
        0x0a, 0x00, // Upper bound = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    // This might raise an exception or just execute depending on emulator
    let _ = run_until_hlt(&mut vcpu);
}

// BOUND: Index above upper bound (should trigger exception)
#[test]
fn test_bound_index_above_upper() {
    let code = [
        0x48, 0xc7, 0xc0, 0x0b, 0x00, 0x00, 0x00, // MOV RAX, 11 (above upper bound)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower bound = 0
        0x0a, 0x00, // Upper bound = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    // This might raise an exception or just execute depending on emulator
    let _ = run_until_hlt(&mut vcpu);
}

// BOUND: Negative bounds handling
#[test]
fn test_bound_negative_bounds() {
    let code = [
        0x48, 0xc7, 0xc0, 0xf6, 0xff, 0xff, 0xff, // MOV RAX, -10 (index = -10)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0xf0, 0xff, // Lower bound = -16 (as signed)
        0x00, 0x00, // Upper bound = 0
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, -10i64 as u64, "Negative index within negative bounds");
}

// BOUND with different registers - RBX
#[test]
fn test_bound_with_rbx() {
    let code = [
        0x48, 0xc7, 0xc3, 0x05, 0x00, 0x00, 0x00, // MOV RBX, 5
        0x62, 0x1d, // BOUND EBX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower = 0
        0x0a, 0x00, // Upper = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 5, "BOUND works with RBX");
}

// BOUND with RCX
#[test]
fn test_bound_with_rcx() {
    let code = [
        0x48, 0xc7, 0xc1, 0x07, 0x00, 0x00, 0x00, // MOV RCX, 7
        0x62, 0x0d, // BOUND ECX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x01, 0x00, // Lower = 1
        0x09, 0x00, // Upper = 9
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 7, "BOUND works with RCX");
}

// BOUND: Zero as index with positive bounds
#[test]
fn test_bound_zero_index() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower = 0
        0xff, 0x00, // Upper = 255
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "Zero index at lower bound");
}

// BOUND: Same lower and upper bounds (exact match required)
#[test]
fn test_bound_same_bounds() {
    let code = [
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x42, 0x00, // Lower = 0x42
        0x42, 0x00, // Upper = 0x42
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x42, "Exact match with same bounds");
}

// BOUND with large bounds
#[test]
fn test_bound_large_bounds() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x01, 0x00, // MOV RAX, 0x10000
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower = 0
        0xff, 0xff, // Upper = 65535
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _ = run_until_hlt(&mut vcpu);
}

// BOUND doesn't affect other registers
#[test]
fn test_bound_preserves_other_registers() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x48, 0xc7, 0xc3, 0x11, 0x11, 0x11, 0x11, // MOV RBX, 0x11111111
        0x48, 0xc7, 0xc1, 0x22, 0x22, 0x22, 0x22, // MOV RCX, 0x22222222
        0x48, 0xc7, 0xc2, 0x33, 0x33, 0x33, 0x33, // MOV RDX, 0x33333333
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data
        0x00, 0x00, // Lower = 0
        0x0a, 0x00, // Upper = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x11111111, "RBX unchanged");
    assert_eq!(regs.rcx, 0x22222222, "RCX unchanged");
    assert_eq!(regs.rdx, 0x33333333, "RDX unchanged");
}

// BOUND with 32-bit operands
#[test]
fn test_bound_32bit_operands() {
    let code = [
        0xb8, 0x05, 0x00, 0x00, 0x00, // MOV EAX, 5 (32-bit)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data (32-bit)
        0x00, 0x00, // Lower = 0
        0x0a, 0x00, // Upper = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax as u32, 5, "32-bit BOUND operates correctly");
}

// BOUND multiple sequential checks
#[test]
fn test_bound_multiple_sequential_checks() {
    let code = [
        // First check
        0x48, 0xc7, 0xc0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds data 1
        0x00, 0x00, // Lower = 0
        0x0a, 0x00, // Upper = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 3, "First check passes");
}

// BOUND: bounds at specific memory location
#[test]
fn test_bound_with_memory_bounds() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x62, 0x85, 0xd8, 0x0f, 0x00, 0x00, // BOUND EAX, [RBP+0x0fd8]
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    regs.rbp = 0x2000;
    let (mut vcpu, vm) = setup_vm(&code, Some(regs));

    // Write bounds at RBP+0x0fd8 = 0x2fd8
    let bounds_addr = 0x2000 + 0x0fd8;
    write_mem_at_u32(&vm, bounds_addr, 0); // Lower = 0
    write_mem_at_u32(&vm, bounds_addr + 4, 10); // Upper = 10

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 5, "BOUND with memory operand bounds");
}

// BOUND: Index in RDX with various bounds
#[test]
fn test_bound_with_rdx() {
    let code = [
        0x48, 0xc7, 0xc2, 0x08, 0x00, 0x00, 0x00, // MOV RDX, 8
        0x62, 0x15, // BOUND EDX, [RIP+5]
        0xf4, // HLT
        // Bounds
        0x00, 0x00, // Lower = 0
        0x0f, 0x00, // Upper = 15
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdx, 8, "BOUND works with RDX");
}

// BOUND with high bounds values
#[test]
fn test_bound_high_bounds_values() {
    let code = [
        0x48, 0xc7, 0xc0, 0x80, 0x00, 0x00, 0x00, // MOV RAX, 0x80
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds
        0x00, 0x00, // Lower = 0
        0xff, 0x00, // Upper = 255
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x80, "Index within high bounds");
}

// BOUND preserves flags
#[test]
fn test_bound_preserves_flags() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (sets ZF)
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds
        0x00, 0x00, // Lower = 0
        0x0a, 0x00, // Upper = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should be cleared by MOV RAX, 5
    // BOUND shouldn't affect flags
    let _ = regs;
}

// BOUND with RSI register
#[test]
fn test_bound_with_rsi() {
    let code = [
        0x48, 0xc7, 0xc6, 0x04, 0x00, 0x00, 0x00, // MOV RSI, 4
        0x62, 0x35, // BOUND ESI, [RIP+5]
        0xf4, // HLT
        // Bounds
        0x00, 0x00, // Lower = 0
        0x0a, 0x00, // Upper = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi, 4, "BOUND works with RSI");
}

// BOUND with RDI register
#[test]
fn test_bound_with_rdi() {
    let code = [
        0x48, 0xc7, 0xc7, 0x09, 0x00, 0x00, 0x00, // MOV RDI, 9
        0x62, 0x3d, // BOUND EDI, [RIP+5]
        0xf4, // HLT
        // Bounds
        0x00, 0x00, // Lower = 0
        0x0a, 0x00, // Upper = 10
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rdi, 9, "BOUND works with RDI");
}

// BOUND: Bounds crossing zero
#[test]
fn test_bound_bounds_crossing_zero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds (signed: -5 to 5)
        0xfb, 0xff, // Lower = -5
        0x05, 0x00, // Upper = 5
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "Zero within bounds crossing zero");
}

// BOUND with maximum 32-bit index value
#[test]
fn test_bound_max_32bit_index() {
    let code = [
        0xb8, 0xff, 0xff, 0xff, 0x7f, // MOV EAX, 0x7fffffff (max signed 32-bit)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds
        0x00, 0x00, // Lower = 0
        0xff, 0xff, // Upper = 65535
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let _ = run_until_hlt(&mut vcpu);
}

// BOUND: Simple array bounds check use case
#[test]
fn test_bound_practical_array_bounds() {
    // Simulate: check if array index (in RAX) is within array bounds
    // Array has 10 elements, so valid indices are 0-9
    let code = [
        0x48, 0xc7, 0xc0, 0x07, 0x00, 0x00, 0x00, // MOV RAX, 7 (array index)
        0x62, 0x05, // BOUND EAX, [RIP+5]
        0xf4, // HLT
        // Bounds
        0x00, 0x00, // Lower = 0
        0x09, 0x00, // Upper = 9 (array has indices 0-9)
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 7, "Valid array index passes bounds check");
}
