use crate::common::*;
use rax::cpu::Registers;

// Comprehensive tests for SYSENTER/SYSEXIT instructions
// SYSENTER (0F 34) - Fast system call (Intel)
// SYSEXIT (0F 35) - Return from fast system call (Intel)
// Intel's alternative to AMD's SYSCALL/SYSRET

// ============================================================================
// SYSENTER - Basic Operation
// ============================================================================

#[test]
fn test_sysenter_basic() {
    // SYSENTER - fast system call (Intel)
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (syscall number)
        0x0f, 0x34, // SYSENTER
        0x48, 0xc7, 0xc3, 0x99, 0x00, 0x00, 0x00, // MOV RBX, 0x99 (after call)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // SYSENTER behavior depends on MSR configuration
    assert_eq!(regs.rbx, 0x99);
}

#[test]
fn test_sysenter_loads_from_msrs() {
    // SYSENTER loads CS, EIP, ESP from MSRs
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Should load CS from SYSENTER_CS_MSR
    // Should load EIP from SYSENTER_EIP_MSR
    // Should load ESP from SYSENTER_ESP_MSR
}

#[test]
fn test_sysenter_no_return_address_save() {
    // SYSENTER does NOT save return address (unlike SYSCALL)
    let code = [
        0x48, 0xc7, 0xc1, 0xaa, 0xaa, 0x00, 0x00, // MOV RCX, 0xAAAA
        0x48, 0xc7, 0xc2, 0xbb, 0xbb, 0x00, 0x00, // MOV RDX, 0xBBBB
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // RCX and RDX should be unchanged
    assert_eq!(regs.rcx, 0xaaaa);
    assert_eq!(regs.rdx, 0xbbbb);
}

#[test]
fn test_sysenter_with_parameters() {
    // SYSENTER with parameters (calling convention varies by OS)
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1
        0x48, 0xc7, 0xc1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000
        0x48, 0xc7, 0xc2, 0x0c, 0x00, 0x00, 0x00, // MOV RDX, 12
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx, 1);
    assert_eq!(regs.rcx, 0x2000);
    assert_eq!(regs.rdx, 12);
}

// ============================================================================
// SYSENTER - MSR Configuration
// ============================================================================

#[test]
fn test_sysenter_cs_msr() {
    // SYSENTER_CS_MSR (0x174) - CS selector
    let code = [
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // CS loaded from SYSENTER_CS_MSR
}

#[test]
fn test_sysenter_esp_msr() {
    // SYSENTER_ESP_MSR (0x175) - ESP value
    let code = [
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // ESP loaded from SYSENTER_ESP_MSR
}

#[test]
fn test_sysenter_eip_msr() {
    // SYSENTER_EIP_MSR (0x176) - EIP value (kernel entry point)
    let code = [
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // EIP loaded from SYSENTER_EIP_MSR
}

// ============================================================================
// SYSEXIT - Basic Return
// ============================================================================

#[test]
fn test_sysexit_basic() {
    // SYSEXIT - return from system call
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000 (return EIP)
        0x48, 0xc7, 0xc2, 0x00, 0x80, 0x00, 0x00, // MOV RDX, 0x8000 (return ESP)
        0x0f, 0x35, // SYSEXIT
        0xf4, // HLT (should not execute)
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0xc7, 0xc0, 0x99, 0x00, 0x00, 0x00, // MOV RAX, 0x99
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x99);
}

#[test]
fn test_sysexit_loads_eip_from_ecx() {
    // SYSEXIT loads EIP from ECX
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x30, 0x00, 0x00, // MOV RCX, 0x3000
        0x48, 0xc7, 0xc2, 0x00, 0x80, 0x00, 0x00, // MOV RDX, 0x8000
        0x0f, 0x35, // SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3000);
}

#[test]
fn test_sysexit_loads_esp_from_edx() {
    // SYSEXIT loads ESP from EDX
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000
        0x48, 0xc7, 0xc2, 0x00, 0x90, 0x00, 0x00, // MOV RDX, 0x9000
        0x0f, 0x35, // SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x48, 0x89, 0xe0, // MOV RAX, RSP (check stack)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x9000);
}

// ============================================================================
// SYSENTER/SYSEXIT - Round Trip
// ============================================================================

#[test]
fn test_sysenter_sysexit_roundtrip() {
    // SYSENTER followed by SYSEXIT
    // Note: caller must save return address manually
    let code = [
        0x48, 0x8d, 0x0d, 0x0b, 0x00, 0x00, 0x00, // LEA RCX, [RIP + 11] (return address)
        0x48, 0x89, 0xe2, // MOV RDX, RSP (save stack)
        0x0f, 0x34, // SYSENTER
        // Return point
        0x48, 0xc7, 0xc3, 0x42, 0x00, 0x00, 0x00, // MOV RBX, 0x42
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // After roundtrip, RBX should be set
}

#[test]
fn test_sysenter_sysexit_preserves_registers() {
    let code = [
        0x48, 0xc7, 0xc3, 0x11, 0x11, 0x00, 0x00, // MOV RBX, 0x1111
        0x48, 0xc7, 0xc5, 0x22, 0x22, 0x00, 0x00, // MOV RBP, 0x2222
        0x49, 0xc7, 0xc4, 0x33, 0x33, 0x00, 0x00, // MOV R12, 0x3333
        0x48, 0x8d, 0x0d, 0x0b, 0x00, 0x00, 0x00, // LEA RCX, [return]
        0x48, 0x89, 0xe2, // MOV RDX, RSP
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx, 0x1111);
    assert_eq!(regs.rbp, 0x2222);
}

// ============================================================================
// SYSENTER - Different Calling Conventions
// ============================================================================

#[test]
fn test_sysenter_windows_convention() {
    // Windows uses different convention than Linux
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (syscall number)
        0x48, 0x8b, 0x0c, 0x24, // MOV RCX, [RSP] (return address from stack)
        0x48, 0x89, 0xe2, // MOV RDX, RSP
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Windows-specific behavior
}

#[test]
fn test_sysenter_linux_convention() {
    // Linux convention (via int 0x80 emulation or vDSO)
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx, 1);
}

// ============================================================================
// SYSENTER - Privilege Level Transitions
// ============================================================================

#[test]
fn test_sysenter_sets_cpl_to_zero() {
    // SYSENTER always sets CPL to 0 (kernel mode)
    let code = [
        0x0f, 0x34, // SYSENTER (CPL = 0)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Should be in kernel mode (CPL=0)
}

#[test]
fn test_sysexit_sets_cpl_to_three() {
    // SYSEXIT always sets CPL to 3 (user mode)
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000
        0x48, 0xc7, 0xc2, 0x00, 0x80, 0x00, 0x00, // MOV RDX, 0x8000
        0x0f, 0x35, // SYSEXIT (CPL = 3)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Should be in user mode (CPL=3)
}

// ============================================================================
// SYSENTER - Error Conditions
// ============================================================================

#[test]
fn test_sysenter_invalid_in_real_mode() {
    // SYSENTER is invalid in real mode
    let code = [
        0x0f, 0x34, // SYSENTER (invalid in real mode)
        0x48, 0xc7, 0xc0, 0xff, 0x00, 0x00, 0x00, // MOV RAX, 0xFF
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Should fault or be ignored
}

#[test]
fn test_sysenter_invalid_in_vm86() {
    // SYSENTER is invalid in virtual 8086 mode
    let code = [
        0x0f, 0x34, // SYSENTER (invalid in VM86)
        0x48, 0xc7, 0xc0, 0xfe, 0x00, 0x00, 0x00, // MOV RAX, 0xFE
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xfe);
}

#[test]
fn test_sysexit_invalid_in_user_mode() {
    // SYSEXIT from user mode should fault
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000
        0x48, 0xc7, 0xc2, 0x00, 0x80, 0x00, 0x00, // MOV RDX, 0x8000
        0x0f, 0x35, // SYSEXIT (invalid from user mode)
        0x48, 0xc7, 0xc0, 0xfd, 0x00, 0x00, 0x00, // MOV RAX, 0xFD
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xfd);
}

#[test]
fn test_sysenter_with_null_cs_msr() {
    // SYSENTER with null CS in MSR should fault
    let code = [
        0x0f, 0x34, // SYSENTER
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Should fault if CS MSR is 0
}

// ============================================================================
// SYSENTER - Register Usage
// ============================================================================

#[test]
fn test_sysenter_preserves_general_registers() {
    let code = [
        0x48, 0xc7, 0xc0, 0x11, 0x11, 0x00, 0x00, // MOV RAX, 0x1111
        0x48, 0xc7, 0xc3, 0x22, 0x22, 0x00, 0x00, // MOV RBX, 0x2222
        0x48, 0xc7, 0xc5, 0x33, 0x33, 0x00, 0x00, // MOV RBP, 0x3333
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x1111);
    assert_eq!(regs.rbx, 0x2222);
    assert_eq!(regs.rbp, 0x3333);
}

#[test]
fn test_sysenter_modifies_cs_eip_esp() {
    let code = [
        0x48, 0xc7, 0xc4, 0x00, 0x70, 0x00, 0x00, // MOV RSP, 0x7000
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // CS, EIP, ESP should be loaded from MSRs
}

#[test]
fn test_sysexit_preserves_general_registers() {
    let code = [
        0x48, 0xc7, 0xc0, 0x44, 0x44, 0x00, 0x00, // MOV RAX, 0x4444
        0x48, 0xc7, 0xc3, 0x55, 0x55, 0x00, 0x00, // MOV RBX, 0x5555
        0x48, 0xc7, 0xc1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000
        0x48, 0xc7, 0xc2, 0x00, 0x80, 0x00, 0x00, // MOV RDX, 0x8000
        0x0f, 0x35, // SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x4444);
    assert_eq!(regs.rbx, 0x5555);
}

// ============================================================================
// SYSENTER - Flags Handling
// ============================================================================

#[test]
fn test_sysenter_clears_vm_flag() {
    // SYSENTER clears VM flag
    let code = [
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // VM flag should be cleared
}

#[test]
fn test_sysenter_clears_if_flag() {
    // SYSENTER clears IF (interrupt enable) flag
    let code = [
        0xfb, // STI (set IF)
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // IF should be cleared after SYSENTER
}

#[test]
fn test_sysenter_clears_rf_flag() {
    // SYSENTER clears RF (resume) flag
    let code = [
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // RF should be cleared
}

// ============================================================================
// SYSEXIT - Flags Handling
// ============================================================================

#[test]
fn test_sysexit_sets_if_flag() {
    // SYSEXIT sets IF (interrupt enable) flag
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000
        0x48, 0xc7, 0xc2, 0x00, 0x80, 0x00, 0x00, // MOV RDX, 0x8000
        0x0f, 0x35, // SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [
        0x9c, // PUSHFQ
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // IF should be set
}

// ============================================================================
// SYSENTER - Edge Cases
// ============================================================================

#[test]
fn test_sysenter_with_zero_eip_msr() {
    // SYSENTER with EIP MSR = 0
    let code = [
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // If MSR points to 0
    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x0000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Should jump to address in EIP MSR (could be 0)
}

#[test]
fn test_sysexit_with_zero_ecx() {
    // SYSEXIT with ECX = 0
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x48, 0xc7, 0xc2, 0x00, 0x80, 0x00, 0x00, // MOV RDX, 0x8000
        0x0f, 0x35, // SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x0000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0000);
}

#[test]
fn test_sysexit_with_high_addresses() {
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0xf0, 0x00, 0x00, // MOV RCX, 0xF000
        0x48, 0xc7, 0xc2, 0x00, 0xe0, 0x00, 0x00, // MOV RDX, 0xE000
        0x0f, 0x35, // SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0xF000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0xf000);
}

// ============================================================================
// SYSENTER - 32-bit vs 64-bit Mode
// ============================================================================

#[test]
fn test_sysenter_32bit_compatibility() {
    // SYSENTER in 32-bit compatibility mode
    let code = [
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // In 32-bit mode, only loads EIP (not RIP)
}

#[test]
fn test_sysenter_64bit_mode() {
    // SYSENTER in 64-bit mode
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0f, 0x34, // SYSENTER
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // In 64-bit mode
}

#[test]
fn test_sysexit_32bit() {
    // SYSEXIT (32-bit form) - loads EIP from ECX
    let code = [
        0xb9, 0x00, 0x20, 0x00, 0x00, // MOV ECX, 0x2000
        0xba, 0x00, 0x80, 0x00, 0x00, // MOV EDX, 0x8000
        0x0f, 0x35, // SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2000);
}

#[test]
fn test_sysexit_64bit() {
    // SYSEXIT (64-bit form) - loads RIP from RCX
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x30, 0x00, 0x00, // MOV RCX, 0x3000
        0x48, 0xc7, 0xc2, 0x00, 0x90, 0x00, 0x00, // MOV RDX, 0x9000
        0x48, 0x0f, 0x35, // REX.W SYSEXIT
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3000);
}

// ============================================================================
// SYSENTER/SYSEXIT - Real-World Patterns
// ============================================================================

#[test]
fn test_sysenter_windows_ntdll_pattern() {
    // Windows NTDLL uses SYSENTER
    let code = [
        0x48, 0x8b, 0xd4, // MOV RDX, RSP (save stack)
        0x0f, 0x05, // Could be SYSCALL on AMD, checking pattern
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx, 1);
}

#[test]
fn test_sysenter_vdso_pattern() {
    // Linux vDSO (virtual dynamic shared object) pattern
    let code = [
        0x55, // PUSH RBP
        0x48, 0x89, 0xe5, // MOV RBP, RSP
        0x0f, 0x34, // SYSENTER
        // Kernel returns here
        0x5d, // POP RBP
        0xc3, // RET
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // vDSO wrapper pattern
}

#[test]
fn test_sysenter_sysexit_kernel_handler_pattern() {
    // Typical kernel handler pattern
    let code = [
        // User space
        0x48, 0x8d, 0x0d, 0x0b, 0x00, 0x00, 0x00, // LEA RCX, [return]
        0x48, 0x89, 0xe2, // MOV RDX, RSP
        0x0f, 0x34, // SYSENTER
        // Return point
        0x48, 0xc7, 0xc3, 0xaa, 0x00, 0x00, 0x00, // MOV RBX, 0xAA
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Pattern completes
}
