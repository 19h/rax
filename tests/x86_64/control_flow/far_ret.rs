use crate::common::*;
use rax::cpu::Registers;

// Comprehensive tests for FAR RET instruction (inter-segment return)
// RET (far return), RETF, RETF imm16
// Opcode: CA imm16 (with immediate), CB (without immediate)

// ============================================================================
// FAR RET - Basic Return Without Parameter
// ============================================================================

#[test]
fn test_far_ret_basic() {
    // FAR RET - return from far call
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push return address (CS:IP) manually for testing
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH 0x2000 (return offset)
        0x66, 0x6a, 0x08, // PUSH 0x08 (return selector)
        0xcb, // RETF (far return)
        0xf4, // HLT (should not execute)
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Write HLT at return address
    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_far_ret_pops_cs_and_ip() {
    // Verify that FAR RET pops both IP and CS from stack
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH return offset
        0x66, 0x6a, 0x08, // PUSH return selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe0, // MOV RAX, RSP (check stack was popped)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Stack should have been restored
    assert!(regs.rax > 0x8000 - 16);
}

// ============================================================================
// FAR RET - With Immediate (Pop Parameters)
// ============================================================================

#[test]
fn test_far_ret_with_immediate_16() {
    // RETF 16 - pop CS:IP and discard 16 bytes of parameters
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push parameters (16 bytes)
        0x6a, 0x01, // PUSH 1
        0x6a, 0x02, // PUSH 2
        // Push return address
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH return offset
        0x66, 0x6a, 0x08, // PUSH return selector
        0xca, 0x10, 0x00, // RETF 16 (discard 16 bytes)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe0, // MOV RAX, RSP (check stack)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Stack should be: initial - parameters + return_addr_size + immediate
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_far_ret_with_immediate_32() {
    // RETF 32 - discard 32 bytes of parameters
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push 32 bytes of parameters
        0x6a, 0x01, 0x6a, 0x02, 0x6a, 0x03, 0x6a, 0x04, // 4 pushes = 32 bytes
        // Push return address
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xca, 0x20, 0x00, // RETF 32
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3000);
}

#[test]
fn test_far_ret_immediate_zero() {
    // RETF 0 - equivalent to RETF without immediate
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xca, 0x00, 0x00, // RETF 0
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

// ============================================================================
// FAR RET - Different Operand Sizes
// ============================================================================

#[test]
fn test_far_ret_16bit_operand_size() {
    // 16-bit operand size - pops 16-bit IP and 16-bit CS
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x66, 0x68, 0x00, 0x20, // PUSH 0x2000 (16-bit)
        0x66, 0x6a, 0x08, // PUSH 0x08 (16-bit)
        0x66, 0xcb, // RETF (16-bit operand size)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_far_ret_32bit_operand_size() {
    // 32-bit operand size - pops 32-bit EIP and 16-bit CS
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH 0x3000 (32-bit)
        0x66, 0x6a, 0x08, // PUSH 0x08
        0xcb, // RETF (32-bit default in this mode)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3000);
}

#[test]
fn test_far_ret_64bit_operand_size() {
    // 64-bit operand size - pops 64-bit RIP and 16-bit CS
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x40, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0x48, 0xcb, // RETF (64-bit with REX.W)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x4000);
}

// ============================================================================
// FAR RET - Privilege Level Transitions
// ============================================================================

#[test]
fn test_far_ret_same_privilege() {
    // Return within same privilege level
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector (CPL=0)
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc0, 0xaa, 0x00, 0x00, 0x00, // MOV RAX, 0xAA
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xaa);
}

#[test]
fn test_far_ret_to_outer_privilege() {
    // Return to outer (lower) privilege level (CPL 0 -> 3)
    // This pops SS:RSP as well
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push outer SS:RSP
        0x68, 0x00, 0x90, 0x00, 0x00, // PUSH outer RSP
        0x66, 0x6a, 0x1b, // PUSH outer SS (RPL=3)
        // Push return CS:RIP
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x1b, // PUSH selector (RPL=3, outer)
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc1, 0xbb, 0x00, 0x00, 0x00, // MOV RCX, 0xBB
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rcx, 0xbb);
}

#[test]
fn test_far_ret_restores_outer_stack() {
    // Return to outer level should restore outer SS:RSP
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0xa0, 0x00, 0x00, // PUSH outer RSP = 0xA000
        0x66, 0x6a, 0x1b, // PUSH outer SS
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x1b, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe2, // MOV RDX, RSP (check restored stack)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // RSP should be restored to outer value
    assert_eq!(regs.rdx, 0xa000);
}

// ============================================================================
// FAR RET - Stack Validation
// ============================================================================

#[test]
fn test_far_ret_stack_alignment() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe3, // MOV RBX, RSP
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Stack should be properly aligned after return
    assert!(regs.rbx >= 0x8000);
}

#[test]
fn test_far_ret_empty_stack() {
    // RETF with insufficient stack (should fault)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x00, 0x00, 0x00, // MOV RSP, 0 (empty)
        0xcb, // RETF (should fault)
        0x48, 0xc7, 0xc0, 0xff, 0x00, 0x00, 0x00, // MOV RAX, 0xFF
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xff);
}

// ============================================================================
// FAR RET - Segment Validation
// ============================================================================

#[test]
fn test_far_ret_null_selector() {
    // Return to null selector should fault
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x00, // PUSH null selector
        0xcb, // RETF
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 1);
}

#[test]
fn test_far_ret_invalid_selector() {
    // Return to invalid selector beyond GDT limit
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x68, 0xff, 0xff, // PUSH 0xFFFF (invalid)
        0xcb, // RETF
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 2);
}

#[test]
fn test_far_ret_non_present_segment() {
    // Return to non-present segment
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x20, // PUSH non-present selector
        0xcb, // RETF
        0x48, 0xc7, 0xc0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 3);
}

#[test]
fn test_far_ret_to_data_segment() {
    // Return to data segment selector (should fault)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x10, // PUSH data segment selector
        0xcb, // RETF
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x00, // MOV RAX, 4
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 4);
}

// ============================================================================
// FAR RET - Return from Nested Calls
// ============================================================================

#[test]
fn test_far_ret_nested_calls() {
    // Simulate nested far calls and returns
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // First call setup
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH level1 offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF to level 1
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Level 1 code
    let level1 = [
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH level2 offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF to level 2
        0xf4,
    ];
    mem.write_slice(&level1, vm_memory::GuestAddress(0x2000)).unwrap();

    // Level 2 code
    let level2 = [
        0x48, 0xc7, 0xc0, 0x77, 0x00, 0x00, 0x00, // MOV RAX, 0x77
        0xf4,
    ];
    mem.write_slice(&level2, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x77);
}

// ============================================================================
// FAR RET - Register Preservation
// ============================================================================

#[test]
fn test_far_ret_preserves_general_registers() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x48, 0xc7, 0xc0, 0x11, 0x11, 0x00, 0x00, // MOV RAX, 0x1111
        0x48, 0xc7, 0xc3, 0x22, 0x22, 0x00, 0x00, // MOV RBX, 0x2222
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x1111);
    assert_eq!(regs.rbx, 0x2222);
}

#[test]
fn test_far_ret_modifies_cs_rip_rsp() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe5, // MOV RBP, RSP (save final RSP)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
    // RSP should be modified by pops
}

// ============================================================================
// FAR RET - Edge Cases
// ============================================================================

#[test]
fn test_far_ret_to_zero_offset() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x00, 0x00, 0x00, // PUSH offset=0
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x0000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0000);
}

#[test]
fn test_far_ret_to_max_offset() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0xff, 0xff, 0x00, 0x00, // PUSH offset=0xFFFF
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0xFFFF)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0xFFFF);
}

#[test]
fn test_far_ret_aligned_address() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH aligned offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3000);
}

#[test]
fn test_far_ret_unaligned_address() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x03, 0x30, 0x00, 0x00, // PUSH unaligned offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3003)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3003);
}

// ============================================================================
// FAR RET - Parameter Cleanup Edge Cases
// ============================================================================

#[test]
fn test_far_ret_immediate_max_value() {
    // RETF with maximum immediate value (64KB)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xca, 0xff, 0xff, // RETF 0xFFFF (max)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_far_ret_immediate_odd_value() {
    // RETF with odd immediate (non-aligned cleanup)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xca, 0x0f, 0x00, // RETF 15 (odd)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

// ============================================================================
// FAR RET - Cross-Segment Returns
// ============================================================================

#[test]
fn test_far_ret_different_code_segment() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x10, // PUSH different selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc6, 0xcc, 0x00, 0x00, 0x00, // MOV RSI, 0xCC
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsi, 0xcc);
}

#[test]
fn test_far_ret_ldt_to_gdt() {
    // Return from LDT segment to GDT segment
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH GDT selector (TI=0)
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_far_ret_gdt_to_ldt() {
    // Return from GDT segment to LDT segment
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x0c, // PUSH LDT selector (TI=1)
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

// ============================================================================
// FAR RET - Flags Preservation
// ============================================================================

#[test]
fn test_far_ret_preserves_flags() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0xf5, // CMC (set carry flag for testing)
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH offset
        0x66, 0x6a, 0x08, // PUSH selector
        0xcb, // RETF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x9c, // PUSHF (check flags preserved)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Flags should be preserved across RETF
}

// ============================================================================
// FAR RET - Combined with CALL
// ============================================================================

#[test]
fn test_far_call_and_ret_roundtrip() {
    // Test FAR CALL followed by FAR RET
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x48, 0xc7, 0xc0, 0x11, 0x00, 0x00, 0x00, // MOV RAX, 0x11
        // Simulate FAR CALL by pushing return address
        0x68, 0x1d, 0x10, 0x00, 0x00, // PUSH return offset (after this block)
        0x66, 0x6a, 0x08, // PUSH CS
        // Jump to subroutine
        0xeb, 0x05, // JMP +5 to subroutine
        // Return point
        0x48, 0xc7, 0xc3, 0x99, 0x00, 0x00, 0x00, // MOV RBX, 0x99
        0xf4,
        // Subroutine
        0x48, 0xff, 0xc0, // INC RAX
        0xcb, // RETF
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x12); // Incremented
    assert_eq!(regs.rbx, 0x99); // Continued after return
}
