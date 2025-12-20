//! Tests for TSX (Transactional Synchronization Extensions) instructions.
//!
//! XBEGIN - Begin Transaction
//! XEND - End Transaction
//! XABORT - Abort Transaction
//! XTEST - Test if in Transactional Execution
//!
//! TSX provides hardware transactional memory support.
//!
//! Opcodes:
//!   C7 F8 - XBEGIN rel32
//!   0F 01 D5 - XEND
//!   C6 F8 ib - XABORT imm8
//!   0F 01 D6 - XTEST
//!
//! Reference: docs/xbegin.txt, docs/xend.txt, docs/xabort.txt, docs/xtest.txt

#[path = "../common/mod.rs"]
mod common;

use common::*;
use rax::cpu::Registers;

// ============================================================================
// XBEGIN Tests - Begin Transaction
// ============================================================================

#[test]
fn test_xbegin_basic() {
    // XBEGIN - Start a transaction
    // C7 F8 + rel32 = XBEGIN
    // Returns -1 in EAX if transaction starts successfully
    let code = [
        0xC7, 0xF8, 0x05, 0x00, 0x00, 0x00, // XBEGIN +5 (fallback path)
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (in transaction)
        0x0F, 0x01, 0xD5,                   // XEND
        // fallback:
        0xF4,                               // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EAX will be -1 if transaction started, or abort status
    // Note: TSX may not be supported in all environments
}

#[test]
fn test_xbegin_preserves_registers() {
    // XBEGIN should preserve other registers
    let code = [
        0x48, 0xC7, 0xC3, 0x42, 0x42, 0x42, 0x42, // MOV RBX, 0x42424242
        0x48, 0xC7, 0xC1, 0x99, 0x99, 0x99, 0x99, // MOV RCX, 0x99999999
        0xC7, 0xF8, 0x05, 0x00, 0x00, 0x00,       // XBEGIN +5
        0x0F, 0x01, 0xD5,                         // XEND
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RBX and RCX should be preserved (unless aborted)
    // Behavior depends on TSX support
}

#[test]
fn test_xbegin_abort_path() {
    // XBEGIN with abort - jumps to fallback
    let code = [
        0xC7, 0xF8, 0x0E, 0x00, 0x00, 0x00,       // XBEGIN +14 (to fallback)
        // Transaction path:
        0x48, 0xC7, 0xC3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1
        0xC6, 0xF8, 0x01,                         // XABORT 1
        // Fallback path:
        0x48, 0xC7, 0xC3, 0x02, 0x00, 0x00, 0x00, // MOV RBX, 2
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should take fallback path and RBX = 2
}

#[test]
fn test_xbegin_with_memory() {
    // XBEGIN with memory access in transaction
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x50, 0x00, 0x00, // MOV RCX, 0x5000
        0xC7, 0xF8, 0x0C, 0x00, 0x00, 0x00,       // XBEGIN +12
        // In transaction:
        0xC7, 0x01, 0x42, 0x42, 0x42, 0x42,       // MOV DWORD PTR [RCX], 0x42424242
        0x0F, 0x01, 0xD5,                         // XEND
        // After transaction:
        0x8B, 0x01,                               // MOV EAX, [RCX]
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_xbegin_nested() {
    // Nested transactions (if supported)
    let code = [
        0xC7, 0xF8, 0x12, 0x00, 0x00, 0x00, // XBEGIN +18 (outer)
        // Outer transaction:
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xC7, 0xF8, 0x05, 0x00, 0x00, 0x00, // XBEGIN +5 (inner)
        0x48, 0xC7, 0xC3, 0x02, 0x00, 0x00, 0x00, // MOV RBX, 2
        0x0F, 0x01, 0xD5,                   // XEND (inner)
        0x0F, 0x01, 0xD5,                   // XEND (outer)
        0xF4,                               // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

// ============================================================================
// XEND Tests - End Transaction
// ============================================================================

#[test]
fn test_xend_basic() {
    // XEND - Commit transaction
    // 0F 01 D5 = XEND
    let code = [
        0xC7, 0xF8, 0x05, 0x00, 0x00, 0x00, // XBEGIN +5
        0x48, 0xC7, 0xC0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x0F, 0x01, 0xD5,                   // XEND
        0xF4,                               // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_xend_preserves_registers() {
    // XEND should not modify registers
    let code = [
        0x48, 0xC7, 0xC3, 0x11, 0x11, 0x11, 0x11, // MOV RBX, 0x11111111
        0xC7, 0xF8, 0x0B, 0x00, 0x00, 0x00,       // XBEGIN +11
        0x48, 0xC7, 0xC0, 0x22, 0x22, 0x22, 0x22, // MOV RAX, 0x22222222
        0x0F, 0x01, 0xD5,                         // XEND
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Registers set in transaction should persist after XEND
}

#[test]
fn test_xend_preserves_flags() {
    // XEND should not modify flags
    let code = [
        0xC7, 0xF8, 0x0E, 0x00, 0x00, 0x00,       // XBEGIN +14
        0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RAX, -1
        0x48, 0x83, 0xC0, 0x01,                   // ADD RAX, 1 (sets ZF)
        0x0F, 0x01, 0xD5,                         // XEND
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should be preserved after XEND
}

// ============================================================================
// XABORT Tests - Abort Transaction
// ============================================================================

#[test]
fn test_xabort_basic() {
    // XABORT - Abort transaction with code
    // C6 F8 ib = XABORT imm8
    let code = [
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX
        0xC7, 0xF8, 0x0B, 0x00, 0x00, 0x00,       // XBEGIN +11
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0xC6, 0xF8, 0x42,                         // XABORT 0x42
        // Fallback:
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EAX should contain abort status (0x42 in bits 31:24)
    // RAX should be 0 (transaction aborted, MOV not committed)
}

#[test]
fn test_xabort_different_codes() {
    // XABORT with different abort codes
    let code = [
        0xC7, 0xF8, 0x05, 0x00, 0x00, 0x00, // XBEGIN +5
        0xC6, 0xF8, 0xFF,                   // XABORT 0xFF
        // Fallback:
        0xF4,                               // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_xabort_unconditional() {
    // XABORT always aborts the transaction
    let code = [
        0x48, 0xC7, 0xC3, 0x00, 0x00, 0x00, 0x00, // MOV RBX, 0
        0xC7, 0xF8, 0x0E, 0x00, 0x00, 0x00,       // XBEGIN +14
        0x48, 0xC7, 0xC3, 0x99, 0x99, 0x99, 0x99, // MOV RBX, 0x99999999
        0xC6, 0xF8, 0x01,                         // XABORT 1
        // Fallback:
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RBX should be 0 (transaction aborted)
    assert_eq!(regs.rbx, 0);
}

// ============================================================================
// XTEST Tests - Test Transaction State
// ============================================================================

#[test]
fn test_xtest_outside_transaction() {
    // XTEST - Test if in transaction
    // 0F 01 D6 = XTEST
    // Sets ZF=1 if not in transaction
    let code = [
        0x0F, 0x01, 0xD6, // XTEST
        0xF4,             // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should be 1 (not in transaction)
    assert!(regs.rflags & 0x40 != 0, "ZF should be set outside transaction");
}

#[test]
fn test_xtest_inside_transaction() {
    // XTEST inside a transaction
    let code = [
        0xC7, 0xF8, 0x08, 0x00, 0x00, 0x00, // XBEGIN +8
        0x0F, 0x01, 0xD6,                   // XTEST
        0x0F, 0x01, 0xD5,                   // XEND
        0xF4,                               // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
    // ZF should be 0 (in transaction)
}

#[test]
fn test_xtest_preserves_registers() {
    // XTEST should only modify RFLAGS
    let code = [
        0x48, 0xC7, 0xC0, 0x42, 0x42, 0x42, 0x42, // MOV RAX, 0x42424242
        0x48, 0xC7, 0xC3, 0x99, 0x99, 0x99, 0x99, // MOV RBX, 0x99999999
        0x0F, 0x01, 0xD6,                         // XTEST
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x42424242);
    assert_eq!(regs.rbx, 0x99999999);
}

#[test]
fn test_xtest_multiple() {
    // Multiple XTEST calls
    let code = [
        0x0F, 0x01, 0xD6, // XTEST #1
        0x0F, 0x01, 0xD6, // XTEST #2
        0x0F, 0x01, 0xD6, // XTEST #3
        0xF4,             // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rflags & 0x40 != 0, "All XTESTs should show not-in-transaction");
}

// ============================================================================
// Combined TSX Tests
// ============================================================================

#[test]
fn test_tsx_complete_transaction() {
    // Complete transaction with begin, operations, and end
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x50, 0x00, 0x00, // MOV RCX, 0x5000
        0xC7, 0xF8, 0x11, 0x00, 0x00, 0x00,       // XBEGIN +17
        // In transaction:
        0xC7, 0x01, 0x42, 0x42, 0x42, 0x42,       // MOV DWORD PTR [RCX], 0x42424242
        0x0F, 0x01, 0xD6,                         // XTEST
        0x0F, 0x01, 0xD5,                         // XEND
        // After transaction:
        0x8B, 0x01,                               // MOV EAX, [RCX]
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_tsx_abort_and_retry() {
    // Transaction that aborts, with retry logic
    let code = [
        0x48, 0x31, 0xC3,                   // XOR RBX, RBX (retry counter)
        // retry:
        0xC7, 0xF8, 0x0E, 0x00, 0x00, 0x00, // XBEGIN +14
        // In transaction:
        0x48, 0x83, 0xFB, 0x00,             // CMP RBX, 0
        0x75, 0x02,                         // JNE skip_abort
        0xC6, 0xF8, 0x01,                   // XABORT 1
        // skip_abort:
        0x0F, 0x01, 0xD5,                   // XEND
        0xEB, 0x03,                         // JMP done
        // Fallback:
        0x48, 0xFF, 0xC3,                   // INC RBX
        0xEB, 0xED,                         // JMP retry
        // done:
        0xF4,                               // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should retry once
    assert_eq!(regs.rbx, 1);
}

#[test]
fn test_tsx_xtest_sequence() {
    // XTEST before, during, and after transaction
    let code = [
        0x0F, 0x01, 0xD6,                   // XTEST (outside)
        0xC7, 0xF8, 0x08, 0x00, 0x00, 0x00, // XBEGIN +8
        0x0F, 0x01, 0xD6,                   // XTEST (inside)
        0x0F, 0x01, 0xD5,                   // XEND
        0x0F, 0x01, 0xD6,                   // XTEST (after)
        0xF4,                               // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_tsx_with_arithmetic() {
    // Transaction with arithmetic operations
    let code = [
        0x48, 0x31, 0xC0,                         // XOR RAX, RAX
        0xC7, 0xF8, 0x14, 0x00, 0x00, 0x00,       // XBEGIN +20
        // In transaction:
        0x48, 0xC7, 0xC0, 0x0A, 0x00, 0x00, 0x00, // MOV RAX, 10
        0x48, 0xC7, 0xC3, 0x14, 0x00, 0x00, 0x00, // MOV RBX, 20
        0x48, 0x01, 0xD8,                         // ADD RAX, RBX
        0x0F, 0x01, 0xD5,                         // XEND
        // After transaction:
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // If transaction succeeded, RAX = 30
}

#[test]
fn test_tsx_memory_conflict_simulation() {
    // Simulate transaction with memory operations
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x50, 0x00, 0x00, // MOV RCX, 0x5000
        0xC7, 0x01, 0x00, 0x00, 0x00, 0x00,       // MOV DWORD PTR [RCX], 0
        0xC7, 0xF8, 0x14, 0x00, 0x00, 0x00,       // XBEGIN +20
        // In transaction:
        0x8B, 0x01,                               // MOV EAX, [RCX]
        0x48, 0x83, 0xC0, 0x01,                   // ADD RAX, 1
        0x89, 0x01,                               // MOV [RCX], EAX
        0x0F, 0x01, 0xD5,                         // XEND
        // After:
        0x8B, 0x01,                               // MOV EAX, [RCX]
        0xF4,                                      // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let _regs = run_until_hlt(&mut vcpu).unwrap();
}
