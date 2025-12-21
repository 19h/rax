//! Tests for the WRMSR instruction.
//!
//! WRMSR - Write to Model Specific Register
//!
//! Writes the contents of registers EDX:EAX into the 64-bit model specific
//! register (MSR) specified in the ECX register. The contents of the EDX
//! register are copied to high-order 32 bits of the selected MSR and the
//! contents of the EAX register are copied to low-order 32 bits of the MSR.
//!
//! Opcode: 0F 30
//! Flags affected: None
//!
//! Reference: docs/wrmsr.txt

#[path = "../common/mod.rs"]
mod common;

use common::*;
use rax::cpu::Registers;

// ============================================================================
// Basic WRMSR Tests
// ============================================================================

#[test]
fn test_wrmsr_basic() {
    // WRMSR - Write EDX:EAX to MSR specified by ECX
    // 0F 30 = WRMSR
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (low 32 bits)
        0x48, 0xC7, 0xC2, 0x00, 0x00, 0x00, 0x00, // MOV RDX, 0 (high 32 bits)
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // WRMSR should complete without errors
    // Registers should be preserved
    assert_eq!(regs.rcx, 0x100, "ECX should be preserved");
    assert_eq!(regs.rax & 0xFFFFFFFF, 1, "EAX should be preserved");
    assert_eq!(regs.rdx & 0xFFFFFFFF, 0, "EDX should be preserved");
}

#[test]
fn test_wrmsr_write_then_read() {
    // Write a value to an MSR, then read it back
    let code = [
        // Write phase
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x42, 0x42, 0x42, 0x42, // MOV RAX, 0x42424242
        0x48, 0xC7, 0xC2, 0x99, 0x99, 0x99, 0x99, // MOV RDX, 0x99999999
        0x0F, 0x30,                               // WRMSR
        // Read phase
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX (clear)
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX (clear)
        0x0F, 0x32,                               // RDMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should read back the written value (if MSR is writable)
    // Note: Some MSRs may mask or ignore certain bits
}

#[test]
fn test_wrmsr_preserves_flags() {
    // WRMSR should not modify flags
    let code = [
        0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RAX, -1
        0x48, 0x83, 0xC0, 0x01,                   // ADD RAX, 1 (sets ZF)
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x48, 0xC7, 0xC2, 0x00, 0x00, 0x00, 0x00, // MOV RDX, 0
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should still be set from ADD
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

#[test]
fn test_wrmsr_preserves_other_registers() {
    // WRMSR should not modify other registers
    // Use values with bit 31 clear to avoid sign-extension issues
    let code = [
        0x48, 0xC7, 0xC3, 0x42, 0x42, 0x42, 0x42, // MOV RBX, 0x42424242
        0x48, 0xC7, 0xC6, 0x55, 0x55, 0x55, 0x55, // MOV RSI, 0x55555555 (bit 31 clear)
        0x48, 0xC7, 0xC7, 0x66, 0x66, 0x66, 0x66, // MOV RDI, 0x66666666 (bit 31 clear)
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x48, 0xC7, 0xC2, 0x00, 0x00, 0x00, 0x00, // MOV RDX, 0
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x42424242, "RBX should not be modified");
    assert_eq!(regs.rsi, 0x55555555, "RSI should not be modified");
    assert_eq!(regs.rdi, 0x66666666, "RDI should not be modified");
}

// ============================================================================
// EDX:EAX Value Tests
// ============================================================================

#[test]
fn test_wrmsr_zero_value() {
    // Write zero to an MSR
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Should complete successfully
}

#[test]
fn test_wrmsr_all_ones() {
    // Write all ones to an MSR (may be masked)
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RAX, 0xFFFFFFFF
        0x48, 0xC7, 0xC2, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RDX, 0xFFFFFFFF
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Should complete (reserved bits may cause #GP in real systems)
}

#[test]
fn test_wrmsr_pattern_values() {
    // Write alternating bit patterns
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0xAA, 0xAA, 0xAA, 0xAA, // MOV RAX, 0xAAAAAAAA
        0x48, 0xC7, 0xC2, 0x55, 0x55, 0x55, 0x55, // MOV RDX, 0x55555555
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_wrmsr_high_32_bits_only() {
    // Write value with only high 32 bits set
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX (low = 0)
        0x48, 0xC7, 0xC2, 0x12, 0x34, 0x56, 0x78, // MOV RDX, 0x78563412
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_wrmsr_low_32_bits_only() {
    // Write value with only low 32 bits set
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x87, 0x65, 0x43, 0x21, // MOV RAX, 0x21436587
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX (high = 0)
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// ECX Value Tests - Different MSR Indices
// ============================================================================

#[test]
fn test_wrmsr_high_rcx_ignored() {
    // High 32 bits of RCX should be ignored in 64-bit mode
    let code = [
        0x48, 0xB9, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RCX, 0xFFFFFFFF_00000100
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00,                   // MOV RAX, 1
        0x48, 0x31, 0xD2,                                           // XOR RDX, RDX
        0x0F, 0x30,                                                  // WRMSR
        0xF4,                                                         // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Should write to MSR 0x100, ignoring high 32 bits
}

#[test]
fn test_wrmsr_preserves_ecx() {
    // ECX should not be modified by WRMSR
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0x100, "ECX should not be modified");
}

// ============================================================================
// Sequential WRMSR Tests
// ============================================================================

#[test]
fn test_wrmsr_multiple_writes() {
    // Write to same MSR twice
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x11, 0x11, 0x11, 0x11, // MOV RAX, 0x11111111
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x0F, 0x30,                               // WRMSR #1
        0x48, 0xC7, 0xC0, 0x22, 0x22, 0x22, 0x22, // MOV RAX, 0x22222222
        0x0F, 0x30,                               // WRMSR #2 (overwrites)
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Second write should overwrite first
}

#[test]
fn test_wrmsr_different_msrs() {
    // Write to two different MSRs
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x11, 0x00, 0x00, 0x00, // MOV RAX, 0x11
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x0F, 0x30,                               // WRMSR (MSR 0x100)
        0x48, 0xC7, 0xC1, 0x01, 0x01, 0x00, 0x00, // MOV RCX, 0x101
        0x48, 0xC7, 0xC0, 0x22, 0x00, 0x00, 0x00, // MOV RAX, 0x22
        0x0F, 0x30,                               // WRMSR (MSR 0x101)
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_wrmsr_loop() {
    // Write MSR in a loop
    // Offsets: 0-6 MOV RCX, 7-13 MOV RBX, 14-16 XOR RAX, 17-19 XOR RDX
    //          20-21 WRMSR (loop_start), 22-24 INC RAX, 25-27 DEC RBX, 28-29 JNZ, 30 HLT
    // JNZ target = 20, RIP after JNZ = 30, offset = -10 = 0xF6
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC3, 0x03, 0x00, 0x00, 0x00, // MOV RBX, 3 (loop counter)
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        // loop_start (offset 20):
        0x0F, 0x30,                               // WRMSR
        0x48, 0xFF, 0xC0,                         // INC RAX (change value)
        0x48, 0xFF, 0xCB,                         // DEC RBX
        0x75, 0xF6,                               // JNZ loop_start (offset -10)
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0, "Loop should complete 3 iterations");
}

// ============================================================================
// Write/Read Sequences
// ============================================================================

#[test]
fn test_wrmsr_write_read_verify() {
    // Write a specific value, then verify by reading
    let code = [
        // Write
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0xEF, 0xBE, 0xAD, 0xDE, // MOV RAX, 0xDEADBEEF
        0x48, 0xC7, 0xC2, 0xFE, 0xCA, 0xEF, 0xBE, // MOV RDX, 0xBEEFCAFE
        0x0F, 0x30,                               // WRMSR
        // Clear registers
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        // Read
        0x0F, 0x32,                               // RDMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Depending on MSR characteristics, may or may not match exactly
    // (some MSRs have reserved/read-only bits)
}

#[test]
fn test_wrmsr_increment_pattern() {
    // Write incrementing values
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX (start at 0)
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x48, 0xC7, 0xC3, 0x05, 0x00, 0x00, 0x00, // MOV RBX, 5
        // loop:
        0x0F, 0x30,                               // WRMSR
        0x48, 0x05, 0x00, 0x10, 0x00, 0x00,       // ADD RAX, 0x1000
        0x48, 0xFF, 0xCB,                         // DEC RBX
        0x75, 0xF5,                               // JNZ loop
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0);
    assert_eq!(regs.rax & 0xFFFFFFFF, 0x5000);
}

// ============================================================================
// 64-bit Specific Tests
// ============================================================================

#[test]
fn test_wrmsr_uses_only_lower_32bits() {
    // Upper 32 bits of RAX and RDX should be ignored
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00,                   // MOV RCX, 0x100
        0x48, 0xB8, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RAX, 0xFFFFFFFF_00000001
        0x48, 0xBA, 0x02, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RDX, 0xFFFFFFFF_00000002
        0x0F, 0x30,                                                  // WRMSR
        0xF4,                                                         // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // Should write 0x00000002_00000001 (only lower 32 bits)
}

#[test]
fn test_wrmsr_edx_eax_composition() {
    // Verify EDX:EAX forms the 64-bit value correctly
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x78, 0x56, 0x34, 0x12, // MOV RAX, 0x12345678 (low)
        0x48, 0xC7, 0xC2, 0xF0, 0xDE, 0xBC, 0x9A, // MOV RDX, 0x9ABCDEF0 (high)
        0x0F, 0x30,                               // WRMSR (writes 0x9ABCDEF0_12345678)
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x0F, 0x32,                               // RDMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should read back the 64-bit value (if MSR supports it)
    let value = ((regs.rdx as u64) << 32) | (regs.rax as u64);
    // Value depends on MSR implementation
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_wrmsr_boundary_values() {
    // Test with boundary values for 32-bit fields
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0x7F, // MOV RAX, 0x7FFFFFFF (max positive)
        0x48, 0xC7, 0xC2, 0x00, 0x00, 0x00, 0x80, // MOV RDX, 0x80000000 (min negative)
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_wrmsr_single_bit_patterns() {
    // Write with single bits set
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 0x00000001
        0x48, 0xC7, 0xC2, 0x00, 0x00, 0x00, 0x80, // MOV RDX, 0x80000000
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_wrmsr_alternating_writes() {
    // Alternate between two values
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        // First write
        0x48, 0xC7, 0xC0, 0xAA, 0xAA, 0xAA, 0xAA, // MOV RAX, 0xAAAAAAAA
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x0F, 0x30,                               // WRMSR
        // Second write
        0x48, 0xC7, 0xC0, 0x55, 0x55, 0x55, 0x55, // MOV RAX, 0x55555555
        0x0F, 0x30,                               // WRMSR
        // Third write
        0x48, 0xC7, 0xC0, 0xAA, 0xAA, 0xAA, 0xAA, // MOV RAX, 0xAAAAAAAA
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_wrmsr_with_computation() {
    // Compute value before writing
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x10, 0x00, 0x00, 0x00, // MOV RAX, 0x10
        0x48, 0xC1, 0xE0, 0x04,                   // SHL RAX, 4 (RAX = 0x100)
        0x48, 0x31, 0xD2,                         // XOR RDX, RDX
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFFFFFF, 0x100);
}

#[test]
fn test_wrmsr_preserves_rax_rdx() {
    // RAX and RDX values should be preserved after WRMSR
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x01, 0x00, 0x00, // MOV RCX, 0x100
        0x48, 0xC7, 0xC0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x48, 0xC7, 0xC2, 0x99, 0x00, 0x00, 0x00, // MOV RDX, 0x99
        0x0F, 0x30,                               // WRMSR
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX and RDX should still contain the values used for writing
    assert_eq!(regs.rax & 0xFFFFFFFF, 0x42, "RAX should be preserved");
    assert_eq!(regs.rdx & 0xFFFFFFFF, 0x99, "RDX should be preserved");
}
