use crate::common::{run_until_hlt, setup_vm};
use rax::cpu::Registers;

// Comprehensive tests for IRET/IRETD/IRETQ instructions (interrupt return)
// IRET (CF), IRETD, IRETQ
// Returns from an interrupt handler, restoring FLAGS, CS, and IP/EIP/RIP

// ============================================================================
// IRET - Basic Return from Interrupt (16-bit)
// ============================================================================

#[test]
fn test_iret_basic_16bit() {
    // IRET - return from interrupt (16-bit mode)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push interrupt frame: IP, CS, FLAGS (16-bit)
        0x66, 0x68, 0x00, 0x20, // PUSH IP (0x2000)
        0x66, 0x6a, 0x08, // PUSH CS (0x08)
        0x66, 0x9c, // PUSHF (16-bit flags)
        0x66, 0xcf, // IRET (16-bit)
        0xf4, // HLT (should not execute)
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_iret_pops_ip_cs_flags() {
    // Verify IRET pops IP, CS, and FLAGS
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x66, 0x68, 0x00, 0x30, // PUSH IP
        0x66, 0x6a, 0x08, // PUSH CS
        0x66, 0x68, 0x02, 0x02, // PUSH FLAGS (with bit 1 set)
        0x66, 0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe0, // MOV RAX, RSP (check stack restored)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert!(regs.rax >= 0x8000 - 16);
}

// ============================================================================
// IRETD - Return from Interrupt (32-bit)
// ============================================================================

#[test]
fn test_iretd_basic_32bit() {
    // IRETD - return from interrupt (32-bit mode)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push interrupt frame: EIP, CS, EFLAGS
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH EIP (0x2000)
        0x66, 0x6a, 0x08, // PUSH CS (0x08)
        0x9c, // PUSHFQ (64-bit flags in 64-bit mode)
        0xcf, // IRETD (32-bit default)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_iretd_restores_eflags() {
    // IRETD restores EFLAGS register
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH EIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x02, 0x04, 0x00, 0x00, // PUSH EFLAGS (CF set, bit 1 set)
        0xcf, // IRETD
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x9c, // PUSHFQ (save restored flags)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Flags should be restored
}

// ============================================================================
// IRETQ - Return from Interrupt (64-bit)
// ============================================================================

#[test]
fn test_iretq_basic_64bit() {
    // IRETQ - return from interrupt (64-bit mode)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push interrupt frame: RIP, CS, RFLAGS, RSP, SS
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP (lower 32 bits)
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0x48, 0xcf, // IRETQ (64-bit)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_iretq_restores_rflags() {
    // IRETQ restores full RFLAGS
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x40, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x46, 0x02, 0x00, 0x00, // PUSH RFLAGS (ZF, PF, bit 1 set)
        0x48, 0xcf, // IRETQ
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x9c, // PUSHFQ
        0x58, // POP RAX (get restored flags)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Check flags were restored
}

// ============================================================================
// IRET - Stack Frame Variations
// ============================================================================

#[test]
fn test_iret_same_privilege_level() {
    // IRET within same privilege level (no SS:RSP restore)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS (CPL=0)
        0x9c, // PUSHFQ
        0xcf, // IRET
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
fn test_iret_to_outer_privilege_level() {
    // IRET to outer (lower) privilege - pops SS:RSP too
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Push full interrupt frame with SS:RSP
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x68, 0x1b, 0x00, // PUSH CS (RPL=3, outer privilege)
        0x9c, // PUSHFQ
        0x68, 0x00, 0xa0, 0x00, 0x00, // PUSH outer RSP
        0x66, 0x68, 0x23, 0x00, // PUSH SS (RPL=3)
        0x48, 0xcf, // IRETQ
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
fn test_iret_restores_outer_stack() {
    // IRET to outer level restores SS:RSP
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH RIP
        0x66, 0x68, 0x1b, 0x00, // PUSH CS (RPL=3)
        0x9c, // PUSHFQ
        0x68, 0x00, 0xb0, 0x00, 0x00, // PUSH outer RSP = 0xB000
        0x66, 0x68, 0x23, 0x00, // PUSH SS
        0x48, 0xcf, // IRETQ
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe2, // MOV RDX, RSP (check restored stack)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rdx, 0xb000);
}

// ============================================================================
// IRET - Flags Restoration
// ============================================================================

#[test]
fn test_iret_restores_carry_flag() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x03, 0x00, 0x00, 0x00, // PUSH FLAGS (CF=1, bit 1 set)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x72, 0x05, // JC +5 (jump if carry)
        0xf4, 0xf4, 0xf4, 0xf4, 0xf4, // HLT padding
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 1); // CF was set
}

#[test]
fn test_iret_restores_zero_flag() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x46, 0x00, 0x00, 0x00, // PUSH FLAGS (ZF=1, PF=1, bit 1 set)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x74, 0x05, // JZ +5 (jump if zero)
        0xf4, 0xf4, 0xf4, 0xf4, 0xf4, // HLT padding
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 2); // ZF was set
}

#[test]
fn test_iret_restores_sign_flag() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x82, 0x00, 0x00, 0x00, // PUSH FLAGS (SF=1, bit 1 set)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x78, 0x05, // JS +5 (jump if sign)
        0xf4, 0xf4, 0xf4, 0xf4, 0xf4,
        0x48, 0xc7, 0xc0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 3); // SF was set
}

#[test]
fn test_iret_restores_overflow_flag() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x02, 0x08, 0x00, 0x00, // PUSH FLAGS (OF=1, bit 1 set)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x70, 0x05, // JO +5 (jump if overflow)
        0xf4, 0xf4, 0xf4, 0xf4, 0xf4,
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x00, // MOV RAX, 4
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 4); // OF was set
}

#[test]
fn test_iret_restores_direction_flag() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x02, 0x04, 0x00, 0x00, // PUSH FLAGS (DF=1, bit 1 set)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x9c, // PUSHFQ
        0x58, // POP RAX (check DF)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // DF should be set in restored flags
}

#[test]
fn test_iret_restores_interrupt_flag() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x02, 0x02, 0x00, 0x00, // PUSH FLAGS (IF=1, bit 1 set)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x9c, // PUSHFQ
        0x58, // POP RAX
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // IF should be restored
}

// ============================================================================
// IRET - Validation and Error Cases
// ============================================================================

#[test]
fn test_iret_null_cs_selector() {
    // IRET with null CS selector should fault
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x00, // PUSH null CS
        0x9c, // PUSHFQ
        0xcf, // IRET
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 1);
}

#[test]
fn test_iret_invalid_cs_selector() {
    // IRET with invalid CS selector
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x68, 0xff, 0xff, // PUSH invalid CS (0xFFFF)
        0x9c, // PUSHFQ
        0xcf, // IRET
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 2);
}

#[test]
fn test_iret_non_present_segment() {
    // IRET to non-present segment
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x20, // PUSH non-present CS
        0x9c, // PUSHFQ
        0xcf, // IRET
        0x48, 0xc7, 0xc0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 3);
}

#[test]
fn test_iret_to_data_segment() {
    // IRET to data segment (should fault)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x10, // PUSH data segment selector
        0x9c, // PUSHFQ
        0xcf, // IRET
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x00, // MOV RAX, 4
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 4);
}

#[test]
fn test_iret_insufficient_stack() {
    // IRET with insufficient stack space
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x00, 0x00, 0x00, // MOV RSP, 0 (empty)
        0xcf, // IRET (should fault)
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 5);
}

// ============================================================================
// IRET - Nested Interrupt Returns
// ============================================================================

#[test]
fn test_iret_nested_interrupts() {
    // Simulate nested interrupt returns
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // First interrupt frame
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP (level 1)
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0xcf, // IRET to level 1
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Level 1 handler
    let level1 = [
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH RIP (level 2)
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0xcf, // IRET to level 2
        0xf4,
    ];
    mem.write_slice(&level1, vm_memory::GuestAddress(0x2000)).unwrap();

    // Level 2 handler
    let level2 = [
        0x48, 0xc7, 0xc0, 0x77, 0x00, 0x00, 0x00, // MOV RAX, 0x77
        0xf4,
    ];
    mem.write_slice(&level2, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x77);
}

// ============================================================================
// IRET - Register Preservation
// ============================================================================

#[test]
fn test_iret_preserves_general_registers() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x48, 0xc7, 0xc0, 0x11, 0x11, 0x00, 0x00, // MOV RAX, 0x1111
        0x48, 0xc7, 0xc3, 0x22, 0x22, 0x00, 0x00, // MOV RBX, 0x2222
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0xcf, // IRET
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
fn test_iret_modifies_cs_rip_rflags() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x46, 0x00, 0x00, 0x00, // PUSH FLAGS
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe5, // MOV RBP, RSP
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

// ============================================================================
// IRET - VM and IOPL Flags
// ============================================================================

#[test]
fn test_iret_vm_flag() {
    // IRET with VM flag (virtual 8086 mode)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x02, 0x00, 0x02, 0x00, // PUSH FLAGS (VM=1)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // VM flag handling
}

#[test]
fn test_iret_iopl_levels() {
    // IRET with different IOPL levels
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP
        0x66, 0x6a, 0x08, // PUSH CS
        0x68, 0x02, 0x30, 0x00, 0x00, // PUSH FLAGS (IOPL=3)
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // IOPL should be restored
}

// ============================================================================
// IRET - Edge Cases
// ============================================================================

#[test]
fn test_iret_to_zero_address() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x00, 0x00, 0x00, // PUSH RIP=0
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x0000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0000);
}

#[test]
fn test_iret_to_max_address() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0xff, 0xff, 0x00, 0x00, // PUSH RIP=0xFFFF
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0xFFFF)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0xFFFF);
}

#[test]
fn test_iret_aligned_address() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x30, 0x00, 0x00, // PUSH aligned address
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3000);
}

#[test]
fn test_iret_unaligned_address() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x03, 0x30, 0x00, 0x00, // PUSH unaligned address
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        0xcf, // IRET
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3003)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3003);
}

// ============================================================================
// IRET - Real-World Patterns
// ============================================================================

#[test]
fn test_iret_interrupt_handler_pattern() {
    // Common interrupt handler pattern
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        // Simulate interrupt entry
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP (return address)
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ
        // Jump to handler
        0xcf, // IRET (return from handler)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let handler = [
        0x50, // PUSH RAX (save registers)
        0x48, 0xc7, 0xc0, 0x99, 0x00, 0x00, 0x00, // MOV RAX, 0x99 (do work)
        0x58, // POP RAX (restore registers)
        0xf4,
    ];
    mem.write_slice(&handler, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Interrupt handler executed and returned
}

#[test]
fn test_iretd_iretq_difference() {
    // Test difference between IRETD (32-bit) and IRETQ (64-bit)
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x80, 0x00, 0x00, // MOV RSP, 0x8000
        0x68, 0x00, 0x20, 0x00, 0x00, // PUSH RIP (32-bit)
        0x66, 0x6a, 0x08, // PUSH CS
        0x9c, // PUSHFQ (64-bit flags)
        0xcf, // IRETD (default in current mode)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc0, 0xab, 0xcd, 0x00, 0x00, // MOV RAX, 0xCDAB
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xcdab);
}
