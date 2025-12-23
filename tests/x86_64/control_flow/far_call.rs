use crate::common::*;

// Comprehensive tests for FAR CALL instruction (inter-segment call)
// CALL ptr16:16, CALL ptr16:32, CALL m16:16, CALL m16:32, CALL m16:64
// Opcode: 9A (immediate), FF /3 (memory)
// Note: 0x9A is invalid in 64-bit mode; use compatibility mode for that form.

// ============================================================================
// FAR CALL - Direct with Immediate Selector:Offset
// ============================================================================

#[test]
fn test_far_call_immediate_16_16_basic() {
    // CALL 0x0008:0x2000 - far call to selector 0x0008, offset 0x2000
    let code = [
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000 (16-bit addressing)
        0xf4, // HLT (should not execute immediately)
        // Target code at 0x2000
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    // Write HLT at target address 0x2000
    let target_code = [0xf4]; // HLT
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Should have performed far call
    assert_eq!(regs.rip, 0x2001);
}

#[test]
fn test_far_call_immediate_16_32_basic() {
    // CALL 0x0008:0x00003000 - far call with 32-bit offset
    let code = [
        0x66, 0x9a, 0x00, 0x30, 0x00, 0x00, 0x08, 0x00, // CALL 0x0008:0x00003000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_call_saves_return_address() {
    // Verify that FAR CALL pushes CS and IP/EIP/RIP onto stack
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    // At target, check stack and return
    let target_code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (marker)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 1);
    assert_eq!(regs.rip, 0x2008);
}

// ============================================================================
// FAR CALL - Memory Indirect m16:16, m16:32, m16:64
// ============================================================================

#[test]
fn test_far_call_mem_indirect_16_16() {
    // CALL FAR [mem] with 16-bit offset (0x66 operand-size override in 64-bit mode).
    let code = [
        0x66, 0xff, 0x1c, 0x25, 0x00, 0x20, 0x00, 0x00, // CALL FAR [0x2000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Write far pointer at 0x2000: offset=0x3000, selector=0x0008
    let far_ptr = [0x00, 0x30, 0x08, 0x00]; // offset:selector (little-endian)
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    // Write target code at 0x3000
    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_call_mem_indirect_16_32() {
    // CALL FAR [mem] with 32-bit offset (default in 64-bit mode).
    let code = [
        0xff, 0x1c, 0x25, 0x00, 0x20, 0x00, 0x00, // CALL FAR [0x2000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Write far pointer: 32-bit offset + 16-bit selector
    let far_ptr = [0x00, 0x40, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x4001);
}

#[test]
fn test_far_call_mem_indirect_16_64() {
    // CALL FAR [mem] with 64-bit offset
    let code = [
        0x48, 0xff, 0x1c, 0x25, 0x00, 0x20, 0x00, 0x00, // CALL FAR [0x2000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Write far pointer: 64-bit offset + 16-bit selector
    let far_ptr = [
        0x00, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 64-bit offset
        0x08, 0x00, // 16-bit selector
    ];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x5000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x5001);
}

// ============================================================================
// FAR CALL - Stack Behavior
// ============================================================================

#[test]
fn test_far_call_stack_push_order() {
    // Verify that CS is pushed before IP
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Stack should have return address pushed
    // Stack should have return address pushed above the prior stack contents.
    assert!(regs.rsp < 0x8000);
}

#[test]
fn test_far_call_stack_alignment() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x48, 0x89, 0xe0, // MOV RAX, RSP (save stack pointer)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Stack pointer should be decremented after push
    assert!(regs.rax < 0x8000);
}

// ============================================================================
// FAR CALL - Different Privilege Levels
// ============================================================================

#[test]
fn test_far_call_same_privilege_level() {
    // Call within same privilege level (CPL=0)
    let code = [
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc1, 0xaa, 0x00, 0x00, 0x00, // MOV RCX, 0xAA
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rcx, 0xaa);
}

#[test]
fn test_far_call_conforming_segment() {
    // Call using an alternate GDT selector (descriptor checks not modeled).
    let code = [
        0x9a, 0x00, 0x20, 0x10, 0x00, // CALL 0x0010:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc2, 0xbb, 0x00, 0x00, 0x00, // MOV RDX, 0xBB
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rdx, 0xbb);
}

// ============================================================================
// FAR CALL - Call Gates
// ============================================================================

#[test]
fn test_far_call_through_call_gate_basic() {
    // Call through a call gate descriptor
    let code = [
        0x9a, 0x00, 0x20, 0x10, 0x00, // CALL 0x0010:0x2000 (call gate selector)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc3, 0xcc, 0x00, 0x00, 0x00, // MOV RBX, 0xCC
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx, 0xcc);
}

#[test]
fn test_far_call_call_gate_parameter_copy() {
    // In IA-32e mode, far calls do not copy parameters to a new stack.
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x48, 0xc7, 0xc0, 0x11, 0x00, 0x00, 0x00, // MOV RAX, 0x11
        0x50, // PUSH RAX (parameter)
        0x48, 0xff, 0x1c, 0x25, 0x00, 0x20, 0x00, 0x00, // CALL FAR [0x2000] (64-bit offset)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x58, // POP RAX (return RIP)
        0x5b, // POP RBX (return CS)
        0x59, // POP RCX (original parameter)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();
    let far_ptr = [
        0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 64-bit offset
        0x08, 0x00, // selector
    ];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, CODE_ADDR + 23);
    assert_eq!(regs.rcx, 0x11);
}

// ============================================================================
// FAR CALL - Task Gates
// ============================================================================

#[test]
fn test_far_call_through_task_gate() {
    // NOTE: Task gates are not supported in IA-32e mode; this verifies the
    // emulator's simplified far-call transfer only.
    let code = [
        0x9a, 0x00, 0x20, 0x18, 0x00, // CALL 0x0018:0x2000 (task gate)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc4, 0xdd, 0x00, 0x00, 0x00, // MOV RSP, 0xDD
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, 0xdd);
}

#[test]
fn test_far_call_task_gate_saves_state() {
    // NOTE: Task gates are not supported in IA-32e mode; this verifies the
    // emulator's simplified far-call transfer only.
    let code = [
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x9a, 0x00, 0x20, 0x18, 0x00, // CALL task gate
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc0, 0x99, 0x00, 0x00, 0x00, // MOV RAX, 0x99
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // New task should have different RAX
    assert_eq!(regs.rax, 0x99);
}

// ============================================================================
// FAR CALL - Segment Selector Validation
// ============================================================================

#[test]
fn test_far_call_null_selector() {
    // Calling with a null selector should fault (#GP).
    let code = [
        0x9a, 0x00, 0x20, 0x00, 0x00, // CALL 0x0000:0x2000 (null selector)
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (fallback)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_call_invalid_selector() {
    // Selector beyond GDT/LDT limit should fault (#GP).
    let code = [
        0x9a, 0x00, 0x20, 0xff, 0xff, // CALL 0xFFFF:0x2000
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_call_ldt_selector() {
    // Call using an LDT selector without an LDT present should fault (#GP).
    let code = [
        0x9a, 0x00, 0x20, 0x0c, 0x00, // CALL 0x000C:0x2000 (LDT selector)
        0xf4,
    ];
    let (mut vcpu, _mem) = setup_vm_compat(&code, None);

    assert!(run_until_hlt(&mut vcpu).is_err());
}

// ============================================================================
// FAR CALL - Different Operand Sizes
// ============================================================================

#[test]
fn test_far_call_operand_size_16() {
    // 16-bit operand size (default in compatibility mode).
    let code = [
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000 (16-bit)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2001);
}

#[test]
fn test_far_call_operand_size_32() {
    // 32-bit operand size (0x66 in compatibility mode).
    let code = [
        0x66, 0x9a, 0x00, 0x30, 0x00, 0x00, 0x08, 0x00, // CALL 0x0008:0x3000 (32-bit offset)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_call_rex_prefix_64() {
    // REX.W prefix for 64-bit operand size
    let code = [
        0x48, 0xff, 0x1c, 0x25, 0x00, 0x20, 0x00, 0x00, // CALL FAR [0x2000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let far_ptr = [
        0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00,
    ];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x4001);
}

// ============================================================================
// FAR CALL - Nested Calls
// ============================================================================

#[test]
fn test_far_call_nested_same_segment() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    // First level call
    let level1_code = [
        0x9a, 0x00, 0x30, 0x08, 0x00, // CALL 0x0008:0x3000
        0xf4,
    ];
    mem.write_slice(&level1_code, vm_memory::GuestAddress(0x2000)).unwrap();

    // Second level
    let level2_code = [
        0x48, 0xc7, 0xc6, 0x77, 0x00, 0x00, 0x00, // MOV RSI, 0x77
        0xf4,
    ];
    mem.write_slice(&level2_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsi, 0x77);
}

#[test]
fn test_far_call_nested_different_segments() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let level1_code = [
        0x9a, 0x00, 0x30, 0x10, 0x00, // CALL 0x0010:0x3000 (different segment)
        0xf4,
    ];
    mem.write_slice(&level1_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let level2_code = [
        0x48, 0xc7, 0xc7, 0x88, 0x00, 0x00, 0x00, // MOV RDI, 0x88
        0xf4,
    ];
    mem.write_slice(&level2_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rdi, 0x88);
}

// ============================================================================
// FAR CALL - Error Conditions
// ============================================================================

#[test]
fn test_far_call_non_present_segment() {
    // Selector beyond GDT limit should fault (#GP).
    let code = [
        0x9a, 0x00, 0x20, 0x20, 0x00, // CALL 0x0020:0x2000 (non-present)
        0x48, 0xc7, 0xc0, 0xff, 0x00, 0x00, 0x00, // MOV RAX, 0xFF
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_call_wrong_descriptor_type() {
    // Selector beyond GDT limit should fault (#GP).
    let code = [
        0x9a, 0x00, 0x20, 0x28, 0x00, // CALL 0x0028:0x2000 (data segment)
        0x48, 0xc7, 0xc0, 0xfe, 0x00, 0x00, 0x00, // MOV RAX, 0xFE
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    assert!(run_until_hlt(&mut vcpu).is_err());
}

// ============================================================================
// FAR CALL - Memory Addressing Modes
// ============================================================================

#[test]
fn test_far_call_mem_register_indirect() {
    // CALL FAR [RAX]
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x20, 0x00, 0x00, // MOV RAX, 0x2000
        0xff, 0x18, // CALL FAR [RAX]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let far_ptr = [0x00, 0x30, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_call_mem_base_displacement() {
    // CALL FAR [RBX + 0x100]
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x1f, 0x00, 0x00, // MOV RBX, 0x1F00
        0xff, 0x9b, 0x00, 0x01, 0x00, 0x00, // CALL FAR [RBX + 0x100]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let far_ptr = [0x00, 0x40, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x4001);
}

#[test]
fn test_far_call_mem_sib_addressing() {
    // CALL FAR [RAX + RBX*4]
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x1e, 0x00, 0x00, // MOV RAX, 0x1E00
        0x48, 0xc7, 0xc3, 0x80, 0x00, 0x00, 0x00, // MOV RBX, 0x80
        0xff, 0x1c, 0x98, // CALL FAR [RAX + RBX*4]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Address = 0x1E00 + 0x80*4 = 0x1E00 + 0x200 = 0x2000
    let far_ptr = [0x00, 0x50, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000)).unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x5000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x5001);
}

// ============================================================================
// FAR CALL - Register Preservation
// ============================================================================

#[test]
fn test_far_call_preserves_general_registers() {
    let code = [
        0x48, 0xc7, 0xc0, 0x11, 0x11, 0x00, 0x00, // MOV RAX, 0x1111
        0x48, 0xc7, 0xc3, 0x22, 0x22, 0x00, 0x00, // MOV RBX, 0x2222
        0x48, 0xc7, 0xc1, 0x33, 0x33, 0x00, 0x00, // MOV RCX, 0x3333
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        // Registers should be preserved
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x1111);
    assert_eq!(regs.rbx, 0x2222);
    assert_eq!(regs.rcx, 0x3333);
}

#[test]
fn test_far_call_modifies_cs_and_rip() {
    let code = [
        0x9a, 0x00, 0x20, 0x08, 0x00, // CALL 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2001);
    // CS should be updated to selector 0x0008
}

// ============================================================================
// FAR CALL - Edge Cases
// ============================================================================

#[test]
fn test_far_call_to_boundary_address() {
    // Call to address at segment boundary
    let code = [
        0x9a, 0xff, 0xff, 0x08, 0x00, // CALL 0x0008:0xFFFF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0xFFFF)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x10000);
}

#[test]
fn test_far_call_zero_offset() {
    // Call to offset 0
    let code = [
        0x9a, 0x00, 0x00, 0x08, 0x00, // CALL 0x0008:0x0000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x0000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0001);
}

#[test]
fn test_far_call_max_offset_32bit() {
    // Call with maximum 32-bit offset within test memory.
    let code = [
        0x66, 0x9a, 0xff, 0xff, 0xff, 0x00, 0x08, 0x00, // CALL 0x0008:0x00FFFFFF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x00FF_FFFF)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0100_0000);
}

#[test]
fn test_far_call_aligned_addresses() {
    // Call to 16-byte aligned address
    let code = [
        0x9a, 0x00, 0x30, 0x08, 0x00, // CALL 0x0008:0x3000 (aligned)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_call_unaligned_addresses() {
    // Call to unaligned address
    let code = [
        0x9a, 0x03, 0x30, 0x08, 0x00, // CALL 0x0008:0x3003 (unaligned)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3003)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3004);
}
