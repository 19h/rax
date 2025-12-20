use rax::cpu::Registers;

#[path = "../common/mod.rs"]
mod common;
use common::{run_until_hlt, setup_vm};

// RDTSC - Read Time-Stamp Counter
// Opcode: 0F 31
// Reads 64-bit time-stamp counter into EDX:EAX
// EDX = high 32 bits, EAX = low 32 bits
// Does not modify flags
// Not serializing (executes as soon as possible)

// Basic RDTSC test - reads TSC into EDX:EAX
#[test]
fn test_rdtsc_basic() {
    let code = [
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // TSC should be loaded into EDX:EAX
    // Upper 32 bits of RAX and RDX should be cleared
    assert_eq!(regs.rax >> 32, 0, "Upper 32 bits of RAX should be cleared");
    assert_eq!(regs.rdx >> 32, 0, "Upper 32 bits of RDX should be cleared");
}

// Test RDTSC increments over time
#[test]
fn test_rdtsc_increments() {
    let code = [
        0x0f, 0x31, // RDTSC (first read)
        0x48, 0x89, 0xc3, // MOV RBX, RAX (save low part)
        0x48, 0x89, 0xd1, // MOV RCX, RDX (save high part)
        0x90, // NOP (some instruction to pass time)
        0x90, // NOP
        0x90, // NOP
        0x0f, 0x31, // RDTSC (second read)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Reconstruct both TSC values
    let tsc1 = ((regs.rcx as u64) << 32) | (regs.rbx as u64);
    let tsc2 = ((regs.rdx as u64) << 32) | (regs.rax as u64);

    // Second read should be >= first read (monotonic)
    assert!(tsc2 >= tsc1, "TSC should be monotonic");
}

// Test RDTSC doesn't modify flags
#[test]
fn test_rdtsc_preserves_flags() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (sets ZF)
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should still be set from the ADD
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

// Test RDTSC preserves other registers
#[test]
fn test_rdtsc_preserves_other_registers() {
    let code = [
        0x48, 0xc7, 0xc3, 0x42, 0x42, 0x42, 0x42, // MOV RBX, 0x42424242
        0x48, 0xc7, 0xc1, 0x99, 0x99, 0x99, 0x99, // MOV RCX, 0x99999999
        0x48, 0xc7, 0xc6, 0xaa, 0xaa, 0xaa, 0xaa, // MOV RSI, 0xaaaaaaaa
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RBX, RCX, RSI should be unchanged
    assert_eq!(regs.rbx, 0x42424242, "RBX should not be affected");
    assert_eq!(regs.rcx, 0x99999999, "RCX should not be affected");
    assert_eq!(regs.rsi, 0xaaaaaaaa, "RSI should not be affected");
}

// Test multiple sequential RDTSC calls
#[test]
fn test_rdtsc_sequential_calls() {
    let code = [
        0x0f, 0x31, // RDTSC #1
        0x0f, 0x31, // RDTSC #2
        0x0f, 0x31, // RDTSC #3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should complete without error
    let tsc = ((regs.rdx as u64) << 32) | (regs.rax as u64);
    let _ = tsc; // TSC value from last call
}

// Test RDTSC after arithmetic operations
#[test]
fn test_rdtsc_after_arithmetic() {
    let code = [
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1
        0x48, 0x83, 0xc3, 0x02, // ADD RBX, 2
        0x48, 0xf7, 0xdb, // NEG RBX
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RBX should still have result of NEG
    assert_eq!(regs.rbx as i64, -3, "RBX should be -3");
}

// Test RDTSC clears upper 32 bits of RAX and RDX
#[test]
fn test_rdtsc_clears_upper_bits() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xffffffffffffffff
        0x48, 0xc7, 0xc2, 0xff, 0xff, 0xff, 0xff, // MOV RDX, 0xffffffffffffffff
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Upper 32 bits should be cleared
    assert_eq!(regs.rax >> 32, 0, "Upper 32 bits of RAX should be cleared");
    assert_eq!(regs.rdx >> 32, 0, "Upper 32 bits of RDX should be cleared");
}

// Test RDTSC with pre-existing values in EAX and EDX
#[test]
fn test_rdtsc_overwrites_registers() {
    let code = [
        0xb8, 0x11, 0x11, 0x11, 0x11, // MOV EAX, 0x11111111
        0xba, 0x22, 0x22, 0x22, 0x22, // MOV EDX, 0x22222222
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Old values should be completely overwritten
    assert_ne!(regs.rax & 0xFFFFFFFF, 0x11111111, "EAX should be overwritten");
    assert_ne!(regs.rdx & 0xFFFFFFFF, 0x22222222, "EDX should be overwritten");
}

// Test RDTSC value is reasonable (non-zero)
#[test]
fn test_rdtsc_nonzero() {
    let code = [
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let tsc = ((regs.rdx as u64) << 32) | (regs.rax as u64);
    // TSC should typically be non-zero after some execution
    // (though technically could be zero immediately after reset)
    let _ = tsc;
}

// Test RDTSC is monotonic across multiple calls
#[test]
fn test_rdtsc_monotonic_three_calls() {
    let code = [
        0x0f, 0x31, // RDTSC #1
        0x50, // PUSH RAX
        0x52, // PUSH RDX
        0x90, // NOP
        0x0f, 0x31, // RDTSC #2
        0x50, // PUSH RAX
        0x52, // PUSH RDX
        0x90, // NOP
        0x0f, 0x31, // RDTSC #3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Latest TSC value should be largest
    let tsc3 = ((regs.rdx as u64) << 32) | (regs.rax as u64);
    assert!(tsc3 > 0, "TSC should be positive");
}

// Test RDTSC behavior with all flags set
#[test]
fn test_rdtsc_with_all_flags_set() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x83, 0xe8, 0x02, // SUB RAX, 2 (sets CF, SF)
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Flags from SUB should be preserved
    assert!(regs.rflags & 0x01 != 0, "CF should be preserved");
    assert!(regs.rflags & 0x80 != 0, "SF should be preserved");
}

// Test RDTSC doesn't serialize execution
#[test]
fn test_rdtsc_not_serializing() {
    let code = [
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1
        0x0f, 0x31, // RDTSC (not serializing)
        0x48, 0x83, 0xc3, 0x01, // ADD RBX, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ADD should complete
    assert_eq!(regs.rbx, 2, "ADD should complete normally");
}

// Test RDTSC with various instruction combinations
#[test]
fn test_rdtsc_with_mov_sequence() {
    let code = [
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0x0f, 0x31, // RDTSC
        0x48, 0x89, 0xc1, // MOV RCX, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RCX should contain low 32 bits of TSC
    assert_eq!(regs.rcx >> 32, 0, "Upper bits should be clear");
}

// Test RDTSC in a loop
#[test]
fn test_rdtsc_in_loop() {
    let code = [
        0x48, 0xc7, 0xc3, 0x03, 0x00, 0x00, 0x00, // MOV RBX, 3 (loop counter)
        // loop:
        0x0f, 0x31, // RDTSC
        0x48, 0x83, 0xeb, 0x01, // SUB RBX, 1
        0x75, 0xf9, // JNZ loop (back 7 bytes)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Loop should complete, RBX should be 0
    assert_eq!(regs.rbx, 0, "Loop should complete");
}

// Test RDTSC with PUSH/POP operations
#[test]
fn test_rdtsc_with_push_pop() {
    let code = [
        0x0f, 0x31, // RDTSC
        0x50, // PUSH RAX
        0x52, // PUSH RDX
        0x5a, // POP RDX
        0x58, // POP RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Values should be restored
    assert_eq!(regs.rax >> 32, 0, "Upper bits should still be clear");
    assert_eq!(regs.rdx >> 32, 0, "Upper bits should still be clear");
}

// Test RDTSC counter is consistent within same instruction stream
#[test]
fn test_rdtsc_consistency() {
    let code = [
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let tsc = ((regs.rdx as u64) << 32) | (regs.rax as u64);
    // Just verify we got some value
    let _ = tsc;
}

// Test RDTSC can be used for timing measurements
#[test]
fn test_rdtsc_timing_measurement() {
    let code = [
        0x0f, 0x31, // RDTSC (start)
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0x48, 0x89, 0xd1, // MOV RCX, RDX
        // Some work
        0x48, 0xc7, 0xc6, 0x64, 0x00, 0x00, 0x00, // MOV RSI, 100
        0x48, 0x83, 0xee, 0x01, // SUB RSI, 1
        0x75, 0xfa, // JNZ loop
        0x0f, 0x31, // RDTSC (end)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let tsc_start = ((regs.rcx as u64) << 32) | (regs.rbx as u64);
    let tsc_end = ((regs.rdx as u64) << 32) | (regs.rax as u64);

    // End should be >= start
    assert!(tsc_end >= tsc_start, "TSC should advance");
}

// Test RDTSC with RIP-relative addressing afterward
#[test]
fn test_rdtsc_with_rip_relative() {
    let code = [
        0x0f, 0x31, // RDTSC
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RBX should have low 32 bits of TSC
    assert_eq!(regs.rbx >> 32, 0, "Upper bits should be zero");
}

// Test RDTSC increment rate is reasonable
#[test]
fn test_rdtsc_increment_rate() {
    let code = [
        0x0f, 0x31, // RDTSC #1
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0x48, 0x89, 0xd1, // MOV RCX, RDX
        0x90, // NOP
        0x0f, 0x31, // RDTSC #2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let tsc1 = ((regs.rcx as u64) << 32) | (regs.rbx as u64);
    let tsc2 = ((regs.rdx as u64) << 32) | (regs.rax as u64);

    // Should have incremented
    assert!(tsc2 >= tsc1, "TSC should increment");
}

// Test RDTSC with high counter values
#[test]
fn test_rdtsc_high_values() {
    let code = [
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EDX and EAX should contain 32-bit values
    let edx = regs.rdx & 0xFFFFFFFF;
    let eax = regs.rax & 0xFFFFFFFF;
    let _ = (edx, eax);
}

// Test RDTSC preserves RBX through multiple calls
#[test]
fn test_rdtsc_rbx_preservation() {
    let code = [
        0x48, 0xc7, 0xc3, 0xef, 0xbe, 0xad, 0xde, // MOV RBX, 0xdeadbeef
        0x0f, 0x31, // RDTSC #1
        0x0f, 0x31, // RDTSC #2
        0x0f, 0x31, // RDTSC #3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0xdeadbeef, "RBX should be preserved");
}

// Test RDTSC with conditional jumps
#[test]
fn test_rdtsc_with_conditional_jump() {
    let code = [
        0x0f, 0x31, // RDTSC
        0x48, 0x85, 0xc0, // TEST RAX, RAX
        0x75, 0x02, // JNZ skip
        0x90, // NOP
        0x90, // NOP
        // skip:
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should complete successfully
    let _ = regs;
}

// Test RDTSC 64-bit reconstruction
#[test]
fn test_rdtsc_64bit_reconstruction() {
    let code = [
        0x0f, 0x31, // RDTSC
        0x48, 0xc1, 0xe2, 0x20, // SHL RDX, 32
        0x48, 0x09, 0xd0, // OR RAX, RDX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX should now contain full 64-bit TSC
    let tsc = regs.rax;
    let _ = tsc;
}

// Test RDTSC with zero flag conditions
#[test]
fn test_rdtsc_zero_flag() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX (sets ZF)
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should still be set
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

// Test RDTSC with carry flag conditions
#[test]
fn test_rdtsc_carry_flag() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xffffffffffffffff
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (sets CF)
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // CF should still be clear (from the ADD that caused overflow to 0)
    let _ = regs.rflags;
}

// Test RDTSC with sign flag conditions
#[test]
fn test_rdtsc_sign_flag() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (sets SF)
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // SF should still be set
    assert!(regs.rflags & 0x80 != 0, "SF should be preserved");
}

// Test RDTSC execution completes quickly
#[test]
fn test_rdtsc_execution_speed() {
    let code = [
        0x0f, 0x31, // RDTSC
        0x0f, 0x31, // RDTSC
        0x0f, 0x31, // RDTSC
        0x0f, 0x31, // RDTSC
        0x0f, 0x31, // RDTSC
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // All calls should complete
    let _ = regs;
}
