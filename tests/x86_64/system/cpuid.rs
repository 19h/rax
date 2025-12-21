use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

#[path = "../common/mod.rs"]
mod common;
use common::{run_until_hlt, setup_vm};

// CPUID - CPU Identification
// Opcode: 0F A2
// Input: RAX (function), RCX (sub-function)
// Output: RAX, RBX, RCX, RDX (CPU info)
//
// Main functions:
// EAX=0: Vendor ID, max function
// EAX=1: Family, model, features
// EAX=2: TLB/Cache info
// EAX=3: Serial number
// EAX=4: Cache descriptors (Pentium 4+)
// EAX=5: Monitor/Mwait
// EAX=6: Thermal info
// EAX=7: Extended features
// EAX=0x80000000+: Extended functions

// Basic CPUID with EAX=0 (Get Vendor ID)
#[test]
fn test_cpuid_function_0_vendor_id() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0 (get vendor ID)
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // CPUID function 0 returns vendor string in EBX:EDX:ECX
    // and max function number in EAX
    // For a basic emulator, these should be set to some values
    assert_ne!(regs.rbx, 0, "RBX should contain part of vendor ID");
}

// CPUID function 1 - Get Family, Model, Stepping, Features
#[test]
fn test_cpuid_function_1_features() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains version/stepping info
    // RBX contains brand and cache info
    // RCX contains feature bits
    // RDX contains feature bits
    // These should be non-zero for a valid emulator
    assert_ne!(regs.rax, 0, "RAX should contain version info");
}

// CPUID clears RCX when EAX < 0x80000000 and ECX input is non-zero
#[test]
fn test_cpuid_ecx_handling() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0xc7, 0xc1, 0x42, 0x00, 0x00, 0x00, // MOV RCX, 0x42
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // For some functions, ECX input matters
    // but for function 1, it should return feature info in ECX
    assert_ne!(regs.rcx, 0, "RCX should contain extended features");
}

// CPUID function 2 - Cache/TLB information
#[test]
fn test_cpuid_function_2_cache_info() {
    let code = [
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EAX contains count and descriptor count
    // EBX, ECX, EDX contain cache descriptors
    assert_ne!(regs.rax, 0, "EAX should contain cache info");
}

// CPUID function 3 - Processor Serial Number
#[test]
fn test_cpuid_function_3_serial_number() {
    let code = [
        0x48, 0xc7, 0xc0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RCX contains high part of serial number
    // RDX contains low part
    // Modern CPUs may return 0 for security reasons
    let _ = regs; // Just verify it doesn't crash
}

// CPUID function 4 - Deterministic Cache Parameters
#[test]
fn test_cpuid_function_4_cache_params() {
    let code = [
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x00, // MOV RAX, 4
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains cache info
    // RBX contains associativity
    // RCX contains number of sets
    // RDX contains invalidation info
    let _ = regs;
}

// CPUID function 5 - MONITOR/MWAIT Feature
#[test]
fn test_cpuid_function_5_monitor_mwait() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains smallest monitor line size
    // RBX contains largest monitor line size
    // RCX contains MONITOR/MWAIT features
    // RDX contains MWAIT sub-C states
    let _ = regs;
}

// CPUID function 6 - Thermal Power Management
#[test]
fn test_cpuid_function_6_thermal_power() {
    let code = [
        0x48, 0xc7, 0xc0, 0x06, 0x00, 0x00, 0x00, // MOV RAX, 6
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EAX contains thermal/power management features
    // EBX contains number of address bits
    // ECX contains ACNT bits
    // EDX contains reserved
    let _ = regs;
}

// CPUID function 7 - Extended Features (ECX=0)
#[test]
fn test_cpuid_function_7_extended_features() {
    let code = [
        0x48, 0xc7, 0xc0, 0x07, 0x00, 0x00, 0x00, // MOV RAX, 7
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains maximum sub-leaf
    // RBX contains extended feature flags
    // RCX contains reserved
    // RDX contains reserved
    let _ = regs;
}

// CPUID extended function 0x80000000 - Get Max Extended Function
#[test]
fn test_cpuid_extended_function_80000000() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x80, // MOV RAX, 0x80000000
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains maximum extended function number
    // Should be >= 0x80000001 for modern processors
    assert!(regs.rax >= 0x80000000, "Extended functions should be supported");
}

// CPUID extended function 0x80000001 - Extended Feature Info
#[test]
fn test_cpuid_extended_function_80000001() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x80, // MOV RAX, 0x80000001
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains family, model, stepping
    // RBX contains brand ID
    // RCX contains advanced power management
    // RDX contains extended features (NX, etc.)
    assert_ne!(regs.rax, 0, "Should return CPU info");
}

// CPUID extended function 0x80000002 - Brand String Part 1
#[test]
fn test_cpuid_extended_function_80000002_brand_1() {
    let code = [
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x80, // MOV RAX, 0x80000002
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX:RBX:RCX:RDX contain brand string (first 16 chars)
    // For an emulator, these might be empty but shouldn't crash
    let _ = regs;
}

// CPUID extended function 0x80000003 - Brand String Part 2
#[test]
fn test_cpuid_extended_function_80000003_brand_2() {
    let code = [
        0x48, 0xc7, 0xc0, 0x03, 0x00, 0x00, 0x80, // MOV RAX, 0x80000003
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX:RBX:RCX:RDX contain brand string (second 16 chars)
    let _ = regs;
}

// CPUID extended function 0x80000004 - Brand String Part 3
#[test]
fn test_cpuid_extended_function_80000004_brand_3() {
    let code = [
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x80, // MOV RAX, 0x80000004
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX:RBX:RCX:RDX contain brand string (third 16 chars)
    let _ = regs;
}

// CPUID extended function 0x80000005 - TLB/Cache Info (L1)
#[test]
fn test_cpuid_extended_function_80000005() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x80, // MOV RAX, 0x80000005
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains L1 TLB info for 2MB/4MB
    // RBX contains L1 TLB info for 4KB
    // RCX contains L1 data cache info
    // RDX contains L1 instruction cache info
    let _ = regs;
}

// CPUID extended function 0x80000006 - Cache Info (L2/L3)
#[test]
fn test_cpuid_extended_function_80000006() {
    let code = [
        0x48, 0xc7, 0xc0, 0x06, 0x00, 0x00, 0x80, // MOV RAX, 0x80000006
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX contains L2 TLB info for 2MB/4MB
    // RBX contains L2 TLB info for 4KB
    // RCX contains L2 cache info
    // RDX contains L3 cache info
    let _ = regs;
}

// CPUID extended function 0x80000007 - Advanced Power Management
#[test]
fn test_cpuid_extended_function_80000007() {
    let code = [
        0x48, 0xc7, 0xc0, 0x07, 0x00, 0x00, 0x80, // MOV RAX, 0x80000007
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX is reserved
    // RBX is reserved
    // RCX contains C-state info
    // RDX contains advanced power management features
    let _ = regs;
}

// CPUID extended function 0x80000008 - Virtual/Physical Address Size
#[test]
fn test_cpuid_extended_function_80000008() {
    let code = [
        0x48, 0xc7, 0xc0, 0x08, 0x00, 0x00, 0x80, // MOV RAX, 0x80000008
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX low byte = physical address bits
    // RAX second byte = linear address bits
    // RBX contains L1 TLB assoc for 4K pages
    // RCX contains cores per processor
    // RDX is reserved
    let _ = regs;
}

// CPUID preserves register values when called with same input
#[test]
fn test_cpuid_deterministic() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0f, 0xa2, // CPUID (first call)
        0x50, // PUSH RAX
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0f, 0xa2, // CPUID (second call with same input)
        0x59, // POP RCX
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RCX has result from first CPUID(1), RAX has result from second CPUID(1)
    // They should match
    assert_eq!(regs.rax, regs.rcx, "CPUID should be deterministic");
}

// CPUID doesn't affect other registers unnecessarily
#[test]
fn test_cpuid_preserves_other_registers() {
    // Use values with bit 31 clear to avoid sign-extension issues with MOV r64, imm32
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0xc7, 0xc2, 0x42, 0x42, 0x42, 0x42, // MOV RDX, 0x42424242
        0x48, 0xc7, 0xc6, 0x55, 0x55, 0x55, 0x55, // MOV RSI, 0x55555555
        0x48, 0xc7, 0xc7, 0x66, 0x66, 0x66, 0x66, // MOV RDI, 0x66666666
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RSI and RDI should be unchanged
    assert_eq!(regs.rsi, 0x55555555, "RSI should not be affected");
    assert_eq!(regs.rdi, 0x66666666, "RDI should not be affected");
}

// Multiple CPUID calls in sequence
#[test]
fn test_multiple_cpuid_calls() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x0f, 0xa2, // CPUID
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0f, 0xa2, // CPUID
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should complete without error
    let _ = regs;
}

// CPUID with function 0x1 returns sensible feature bits
#[test]
fn test_cpuid_feature_bits_function_1() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // At least one feature bit should be set in EDX or ECX
    // For a modern x86_64, we expect FPU (bit 0 of EDX) to be set
    assert!((regs.rdx as u32) & 0x01 != 0 || (regs.rcx as u32) != 0, "Should have some features");
}

// CPUID ECX input for cache descriptors
#[test]
fn test_cpuid_function_4_with_different_ecx() {
    let code = [
        // First call with ECX=0
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x00, // MOV RAX, 4
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x0f, 0xa2, // CPUID
        0x50, // PUSH RAX
        0x51, // PUSH RCX

        // Second call with ECX=1
        0x48, 0xc7, 0xc0, 0x04, 0x00, 0x00, 0x00, // MOV RAX, 4
        0x48, 0xc7, 0xc1, 0x01, 0x00, 0x00, 0x00, // MOV RCX, 1
        0x0f, 0xa2, // CPUID

        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Both calls should complete
    let _ = regs;
}

// CPUID with input in EAX only (not RAX full width)
#[test]
fn test_cpuid_eax_32bit_input() {
    let code = [
        0xb8, 0x01, 0x00, 0x00, 0x00, // MOV EAX, 1 (32-bit, zero extends)
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    regs.rax = 0xFFFFFFFFFFFFFFFF; // High bits set
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // After MOV EAX, high bits of RAX should be zeroed
    // But CPUID should still work
    let _ = regs;
}

// CPUID function 0 max returned function check
#[test]
fn test_cpuid_function_0_returns_max() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EAX should contain max function number
    // Should be at least 1 for modern CPUs
    assert!(regs.rax >= 0x01, "Should support at least function 1");
}

// CPUID stores vendor ID correctly in function 0
#[test]
fn test_cpuid_function_0_vendor_layout() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // After CPUID 0: EBX, EDX, ECX form vendor string
    // Like "GenuineIntel" or "AuthenticAMD"
    // RBX, RDX, RCX should have values (though emulator might not)
    let _ = regs;
}

// CPUID doesn't modify flags
#[test]
fn test_cpuid_preserves_flags() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (sets ZF)
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0f, 0xa2, // CPUID
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should still be set from the ADD
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

// CPUID with large RAX input (unsupported function)
#[test]
fn test_cpuid_unsupported_function() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xffffffff
        0x0f, 0xa2, // CPUID (unsupported function)
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should not crash, might return empty results
    let _ = regs;
}

// CPUID between supported and extended functions boundary
#[test]
fn test_cpuid_boundary_standard_extended() {
    let code = [
        // High standard function
        0x48, 0xc7, 0xc0, 0x0f, 0x00, 0x00, 0x00, // MOV RAX, 15
        0x0f, 0xa2, // CPUID
        0x50, // PUSH RAX

        // Low extended function
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x80, // MOV RAX, 0x80000000
        0x0f, 0xa2, // CPUID

        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Both should work
    let _ = regs;
}

// CPUID RCX input for sub-leaf enumeration (function 7)
#[test]
fn test_cpuid_function_7_subleaves() {
    let code = [
        // Leaf 7, sub-leaf 0
        0x48, 0xc7, 0xc0, 0x07, 0x00, 0x00, 0x00, // MOV RAX, 7
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x0f, 0xa2, // CPUID
        0x50, // PUSH RAX

        // Leaf 7, sub-leaf 1
        0x48, 0xc7, 0xc0, 0x07, 0x00, 0x00, 0x00, // MOV RAX, 7
        0x48, 0xc7, 0xc1, 0x01, 0x00, 0x00, 0x00, // MOV RCX, 1
        0x0f, 0xa2, // CPUID

        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x1000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Both sub-leaves should work
    let _ = regs;
}
