//! Tests for System Management Instructions.
//!
//! Instructions covered:
//! - PCONFIG - Platform Configuration
//! - WBNOINVD - Write Back and Do Not Invalidate Cache
//! - INVPCID - Invalidate Process-Context Identifier
//!
//! References: docs/pconfig.txt, docs/wbnoinvd.txt, docs/invpcid.txt

use crate::common::*;
use rax::vm::vcpu::Registers;

// ============================================================================
// PCONFIG Tests - Platform Configuration
// ============================================================================

#[test]
fn test_pconfig_feature_and_targets_are_not_enumerated() {
    let code = [
        0xB8, 0x07, 0x00, 0x00, 0x00, // MOV EAX, 7
        0x31, 0xC9, // XOR ECX, ECX
        0x0F, 0xA2, // CPUID
        0xF4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rdx & (1 << 18), 0, "CPUID must not advertise PCONFIG");

    let code = [
        0xB8, 0x1B, 0x00, 0x00, 0x00, // MOV EAX, 1Bh
        0x31, 0xC9, // XOR ECX, ECX
        0x0F, 0xA2, // CPUID
        0xF4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!([regs.rax, regs.rbx, regs.rcx, regs.rdx], [0; 4]);
}

fn assert_disabled_pconfig_ud(name: &str, bytes: &[u8], apx: bool, cpl: u16, leaf: u64) {
    let initial = Registers {
        rax: leaf,
        // Non-canonical and unmapped: a feature-disabled execution must not
        // attempt the MKTME_KEY_PROGRAM structure read implied by leaf 0.
        rbx: 0x0000_8000_0000_0000,
        rcx: 0x1111_2222_3333_4444,
        rdx: 0xAAAA_BBBB_CCCC_DDDD,
        rsp: STACK_ADDR,
        rbp: 0x5555_6666_7777_8888,
        r16: 0x1616_1616_1616_1616,
        r31: 0x3131_3131_3131_3131,
        rflags: 0x0CD7,
        xmm: [[0x1111_2222_3333_4444, 0xAAAA_BBBB_CCCC_DDDD]; 16],
        ..Registers::default()
    };
    let (mut vcpu, _) = if apx {
        setup_apx_vm_no_idt(bytes, Some(initial))
    } else {
        setup_vm_no_idt(bytes, Some(initial))
    };
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = cpl;
    sregs.cr2 = 0x2222_0000;
    sregs.cr3 = 0x3333_0000;
    sregs.cr4 |= 0x4444;
    sregs.cr8 = 0x8;
    sregs.fs.base = 0x0000_1111_2222_3333;
    sregs.gs.base = 0x0000_4444_5555_6666;
    vcpu.set_sregs(&sregs).unwrap();
    let before =
        serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();

    let error = vcpu.step().expect_err("disabled PCONFIG must inject #UD");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "{name}: expected #UD before CPL, leaf, memory, or state checks, got {error}"
    );
    let after =
        serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();
    assert_eq!(after, before, "{name}");
}

#[test]
fn test_pconfig_feature_fault_precedes_leaf_memory_or_state_commit() {
    assert_disabled_pconfig_ud("leaf 0", &[0x0F, 0x01, 0xC5], false, 0, 0);
    assert_disabled_pconfig_ud("undefined leaf", &[0x0F, 0x01, 0xC5], false, 0, u64::MAX);
}

#[test]
fn test_pconfig_disabled_feature_fault_is_independent_of_cpl() {
    for cpl in [0, 3] {
        assert_disabled_pconfig_ud("CPL profile", &[0x0F, 0x01, 0xC5], false, cpl, 0);
    }
}

#[test]
fn test_pconfig_invalid_legacy_prefixes_are_precise_ud() {
    for prefix in [0x66, 0xF2, 0xF3, 0xF0] {
        assert_disabled_pconfig_ud(
            "invalid legacy prefix",
            &[prefix, 0x0F, 0x01, 0xC5],
            false,
            3,
            0,
        );
    }
}

#[test]
fn test_pconfig_ignored_legacy_prefixes_reach_the_disabled_feature_fault() {
    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67, 0x40, 0x48, 0x4F] {
        assert_disabled_pconfig_ud(
            "ignored legacy prefix",
            &[prefix, 0x0F, 0x01, 0xC5],
            false,
            0,
            0,
        );
    }
}

#[test]
fn test_pconfig_vector_prefix_forms_are_precise_ud() {
    for (name, bytes) in [
        ("two-byte VEX", &[0xC5, 0xF8, 0x01, 0xC5][..]),
        ("three-byte VEX", &[0xC4, 0xE1, 0x78, 0x01, 0xC5][..]),
        ("EVEX", &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC5][..]),
    ] {
        assert_disabled_pconfig_ud(name, bytes, false, 3, 0);
    }
}

#[test]
fn test_pconfig_rex2_reaches_the_disabled_feature_fault_with_apx_enabled() {
    assert_disabled_pconfig_ud("REX2", &[0xD5, 0x80, 0x01, 0xC5], true, 3, 0);
}

// ============================================================================
// WBNOINVD Tests - Write Back No Invalidate
// ============================================================================

#[test]
fn test_wbnoinvd_basic() {
    // WBNOINVD - Write back and do not invalidate cache
    // Opcode: F3 0F 09
    let code = [
        0xF3, 0x0F, 0x09, // WBNOINVD
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_wbnoinvd_no_operands() {
    // WBNOINVD takes no operands
    let code = [0xF3, 0x0F, 0x09, 0xF4];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_wbnoinvd_preserves_registers() {
    // WBNOINVD should not modify any registers
    let code = [
        0x48, 0xC7, 0xC0, 0x11, 0x11, 0x11, 0x11, // MOV RAX, 0x11111111
        0x48, 0xC7, 0xC3, 0x22, 0x22, 0x22, 0x22, // MOV RBX, 0x22222222
        0x48, 0xC7, 0xC1, 0x33, 0x33, 0x33, 0x33, // MOV RCX, 0x33333333
        0xF3, 0x0F, 0x09, // WBNOINVD
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x11111111, "RAX should not be modified");
    assert_eq!(regs.rbx, 0x22222222, "RBX should not be modified");
    assert_eq!(regs.rcx, 0x33333333, "RCX should not be modified");
}

#[test]
fn test_wbnoinvd_multiple() {
    // Multiple WBNOINVD operations
    let code = [
        0xF3, 0x0F, 0x09, // WBNOINVD
        0xF3, 0x0F, 0x09, // WBNOINVD
        0xF3, 0x0F, 0x09, // WBNOINVD
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_wbnoinvd_preserves_flags() {
    // WBNOINVD should not modify flags
    let code = [
        0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RAX, -1
        0x48, 0x83, 0xC0, 0x01, // ADD RAX, 1 (sets ZF)
        0xF3, 0x0F, 0x09, // WBNOINVD
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_ne!(regs.rflags & 0x40, 0, "ZF should still be set");
}

// ============================================================================
// INVPCID Tests - Invalidate Process-Context Identifier
// ============================================================================

#[test]
fn test_invpcid_individual_address() {
    // INVPCID - Invalidate TLB entries for PCID
    // Opcode: 66 0F 38 82
    // Type 0: Individual-address invalidation
    let code = [
        0x48, 0x31, 0xC0, // XOR RAX, RAX (type 0)
        0x48, 0xC7, 0xC3, 0x00, 0x10, 0x00, 0x00, // MOV RBX, 0x1000 (descriptor)
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invpcid_single_context() {
    // Type 1: Single-context invalidation
    let code = [
        0x48, 0xC7, 0xC1, 0x01, 0x00, 0x00, 0x00, // MOV RCX, 1 (type 1)
        0x48, 0xC7, 0xC2, 0x00, 0x20, 0x00, 0x00, // MOV RDX, 0x2000 (descriptor)
        0x66, 0x0F, 0x38, 0x82, 0x0A, // INVPCID rcx, [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invpcid_all_contexts() {
    // Type 2: All-contexts invalidation
    let code = [
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2 (type 2)
        0x48, 0xC7, 0xC3, 0x00, 0x30, 0x00, 0x00, // MOV RBX, 0x3000
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invpcid_all_contexts_including_globals() {
    // Type 3: All-contexts including globals
    let code = [
        0x48, 0xC7, 0xC1, 0x03, 0x00, 0x00, 0x00, // MOV RCX, 3 (type 3)
        0x48, 0xC7, 0xC2, 0x00, 0x40, 0x00, 0x00, // MOV RDX, 0x4000
        0x66, 0x0F, 0x38, 0x82, 0x0A, // INVPCID rcx, [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invpcid_different_types() {
    // Test all 4 INVPCID types
    let code = [
        0x48, 0xC7, 0xC3, 0x00, 0x10, 0x00, 0x00, // MOV RBX, 0x1000
        // Type 0
        0x48, 0x31, 0xC0, // XOR RAX, RAX
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        // Type 1
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        // Type 2
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        // Type 3
        0x48, 0xC7, 0xC0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invpcid_with_displacement() {
    // INVPCID with memory displacement
    let code = [
        0x48, 0x31, 0xC0, // XOR RAX, RAX
        0x48, 0xC7, 0xC1, 0x00, 0x10, 0x00, 0x00, // MOV RCX, 0x1000
        0x66, 0x0F, 0x38, 0x82, 0x81, 0x00, 0x04, 0x00, 0x00, // INVPCID rax, [rcx+0x400]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invpcid_multiple_invalidations() {
    // Multiple INVPCID calls
    let code = [
        0x48, 0xC7, 0xC3, 0x00, 0x10, 0x00, 0x00, // MOV RBX, 0x1000
        0x48, 0x31, 0xC0, // XOR RAX, RAX
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// Combined System Management Tests
// ============================================================================

#[test]
fn test_cache_invalidation_sequence() {
    // Sequence of cache and TLB operations
    let code = [
        0xF3, 0x0F, 0x09, // WBNOINVD (writeback caches)
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x48, 0xC7, 0xC3, 0x00, 0x10, 0x00, 0x00, // MOV RBX, 0x1000
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx] (invalidate TLB)
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_tlb_shootdown_sequence() {
    // TLB shootdown-like sequence
    let code = [
        // Invalidate specific address
        0x48, 0x31, 0xC0, // XOR RAX, RAX (type 0)
        0x48, 0xC7, 0xC3, 0x00, 0x10, 0x00, 0x00, // MOV RBX, 0x1000
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        // Invalidate single context
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x66, 0x0F, 0x38, 0x82, 0x03, // INVPCID rax, [rbx]
        // Writeback caches
        0xF3, 0x0F, 0x09, // WBNOINVD
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}
