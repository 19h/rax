use crate::common::{
    CODE_ADDR, VCpu, run_until_hlt, setup_apx_vm_no_idt, setup_vm, setup_vm_no_idt,
};
// MOV CR - Move to/from Control Registers
// Opcodes:
// 0F 20 /r - MOV r32/r64, CR0/CR2/CR3/CR4 (read control register)
// 0F 22 /r - MOV CR0/CR2/CR3/CR4, r32/r64 (write control register)
// REX.R + 0F 20/0 - MOV r64, CR8
// REX.R + 0F 22/0 - MOV CR8, r64
//
// Control Registers:
// CR0 - System control flags (PE, MP, EM, TS, ET, NE, WP, AM, NW, CD, PG)
// CR2 - Page fault linear address
// CR3 - Page directory base register (PDBR)
// CR4 - Extended control flags (VME, PVI, TSD, DE, PSE, PAE, MCE, PGE, etc.)
// CR8 - Task priority register (TPR) - 64-bit mode only

// Test MOV from CR0 to RAX
#[test]
fn test_mov_cr0_to_rax() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // setup_vm initializes CR0 = 0x00050033 (PE|MP|ET|NE|WP|AM, PG clear).
    assert_eq!(regs.rax, 0x00050033, "CR0 exact default");
    assert!(regs.rax & 1 != 0, "PE set");
    assert_eq!(regs.rax >> 31 & 1, 0, "PG clear (no paging)");
}

// Test MOV from CR0 to RBX
#[test]
fn test_mov_cr0_to_rbx() {
    let code = [
        0x0f, 0x20, 0xc3, // MOV RBX, CR0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rbx != 0, "CR0 loaded into RBX");
}

// Test MOV from CR0 to RCX
#[test]
fn test_mov_cr0_to_rcx() {
    let code = [
        0x0f, 0x20, 0xc1, // MOV RCX, CR0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rcx != 0, "CR0 loaded into RCX");
}

// Test MOV from CR0 to RDX
#[test]
fn test_mov_cr0_to_rdx() {
    let code = [
        0x0f, 0x20, 0xc2, // MOV RDX, CR0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rdx != 0, "CR0 loaded into RDX");
}

// Test MOV from CR2 to RAX
#[test]
fn test_mov_cr2_to_rax() {
    let code = [
        0x0f, 0x20, 0xd0, // MOV RAX, CR2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // CR2 contains page fault address, should be 0 if no page fault occurred
    let _ = regs.rax;
}

// Test MOV from CR2 to RBX
#[test]
fn test_mov_cr2_to_rbx() {
    let code = [
        0x0f, 0x20, 0xd3, // MOV RBX, CR2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs.rbx;
}

// Test MOV from CR3 to RAX
#[test]
fn test_mov_cr3_to_rax() {
    let code = [
        0x0f, 0x20, 0xd8, // MOV RAX, CR3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // CR3 should contain page directory base
    let _ = regs.rax;
}

// Test MOV from CR3 to RDX
#[test]
fn test_mov_cr3_to_rdx() {
    let code = [
        0x0f, 0x20, 0xda, // MOV RDX, CR3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs.rdx;
}

// Test MOV from CR4 to RAX
#[test]
fn test_mov_cr4_to_rax() {
    let code = [
        0x0f, 0x20, 0xe0, // MOV RAX, CR4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // setup_vm initializes CR4 = 0x20 (PAE, bit 5).
    assert_eq!(regs.rax, 0x20, "CR4 exact default (PAE)");
    assert!(regs.rax & (1 << 5) != 0, "PAE set");
}

// Test MOV from CR4 to RBX
#[test]
fn test_mov_cr4_to_rbx() {
    let code = [
        0x0f, 0x20, 0xe3, // MOV RBX, CR4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs.rbx;
}

// Test MOV from CR8 to RAX (requires REX.R prefix)
#[test]
fn test_mov_cr8_to_rax() {
    let code = [
        0x44, 0x0f, 0x20, 0xc0, // MOV RAX, CR8 (REX.R + 0F 20 /0)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // CR8 is task priority register, value depends on system state
    let _ = regs.rax;
}

// Test MOV from CR8 to RBX
#[test]
fn test_mov_cr8_to_rbx() {
    let code = [
        0x44, 0x0f, 0x20, 0xc3, // MOV RBX, CR8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs.rbx;
}

// Test MOV to CR0 from RAX (write back same value)
#[test]
fn test_mov_rax_to_cr0() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should complete successfully
    let _ = regs;
}

// Test MOV to CR0 from RBX
#[test]
fn test_mov_rbx_to_cr0() {
    let code = [
        0x0f, 0x20, 0xc3, // MOV RBX, CR0
        0x0f, 0x22, 0xc3, // MOV CR0, RBX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR2 from RAX
#[test]
fn test_mov_rax_to_cr2() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x10, 0x00, 0x00, // MOV RAX, 0x1000
        0x0f, 0x22, 0xd0, // MOV CR2, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR2 from RDX
#[test]
fn test_mov_rdx_to_cr2() {
    let code = [
        0x48, 0xc7, 0xc2, 0x00, 0x20, 0x00, 0x00, // MOV RDX, 0x2000
        0x0f, 0x22, 0xd2, // MOV CR2, RDX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR3 from RAX (write back same value)
#[test]
fn test_mov_rax_to_cr3() {
    let code = [
        0x0f, 0x20, 0xd8, // MOV RAX, CR3
        0x0f, 0x22, 0xd8, // MOV CR3, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR3 from RBX
#[test]
fn test_mov_rbx_to_cr3() {
    let code = [
        0x0f, 0x20, 0xdb, // MOV RBX, CR3
        0x0f, 0x22, 0xdb, // MOV CR3, RBX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR4 from RAX (write back same value)
#[test]
fn test_mov_rax_to_cr4() {
    let code = [
        0x0f, 0x20, 0xe0, // MOV RAX, CR4
        0x0f, 0x22, 0xe0, // MOV CR4, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR4 from RCX
#[test]
fn test_mov_rcx_to_cr4() {
    let code = [
        0x0f, 0x20, 0xe1, // MOV RCX, CR4
        0x0f, 0x22, 0xe1, // MOV CR4, RCX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR8 from RAX
#[test]
fn test_mov_rax_to_cr8() {
    let code = [
        0x44, 0x0f, 0x20, 0xc0, // MOV RAX, CR8
        0x44, 0x0f, 0x22, 0xc0, // MOV CR8, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test MOV to CR8 from RDX
#[test]
fn test_mov_rdx_to_cr8() {
    let code = [
        0x48, 0x31, 0xd2, // XOR RDX, RDX
        0x44, 0x0f, 0x22, 0xc2, // MOV CR8, RDX (set TPR to 0)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test round-trip CR0 read/write
#[test]
fn test_cr0_round_trip() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX should match RBX (original CR0 value)
    assert_eq!(regs.rax, regs.rbx, "CR0 value should be preserved");
}

// Test round-trip CR3 read/write
#[test]
fn test_cr3_round_trip() {
    let code = [
        0x0f, 0x20, 0xd8, // MOV RAX, CR3
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0x0f, 0x22, 0xd8, // MOV CR3, RAX
        0x0f, 0x20, 0xd8, // MOV RAX, CR3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RAX should match RBX (original CR3 value)
    assert_eq!(regs.rax, regs.rbx, "CR3 value should be preserved");
}

// Test round-trip CR4 read/write
#[test]
fn test_cr4_round_trip() {
    let code = [
        0x0f, 0x20, 0xe0, // MOV RAX, CR4
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0x0f, 0x22, 0xe0, // MOV CR4, RAX
        0x0f, 0x20, 0xe0, // MOV RAX, CR4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, regs.rbx, "CR4 value should be preserved");
}

// Test CR2 write/read
#[test]
fn test_cr2_write_read() {
    let code = [
        0x48, 0xc7, 0xc0, 0x34, 0x12, 0x00, 0x00, // MOV RAX, 0x1234
        0x0f, 0x22, 0xd0, // MOV CR2, RAX
        0x0f, 0x20, 0xd0, // MOV RAX, CR2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x1234, "CR2 should contain written value");
}

// Test CR8 write/read with value 0
#[test]
fn test_cr8_write_read_zero() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX
        0x44, 0x0f, 0x22, 0xc0, // MOV CR8, RAX
        0x44, 0x0f, 0x20, 0xc0, // MOV RAX, CR8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "CR8 should be 0");
}

// Test CR8 with different values
#[test]
fn test_cr8_different_values() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x44, 0x0f, 0x22, 0xc0, // MOV CR8, RAX
        0x44, 0x0f, 0x20, 0xc1, // MOV RCX, CR8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 1, "CR8 should contain 1");
}

// Test preserving other registers during CR0 operations
#[test]
fn test_cr0_preserves_other_registers() {
    let code = [
        0x48, 0xc7, 0xc6, 0x42, 0x42, 0x42, 0x42, // MOV RSI, 0x42424242
        0x48, 0xc7, 0xc7, 0x2a, 0x2a, 0x2a,
        0x2a, // MOV RDI, 0x2a2a2a2a (bit 31 clear to avoid sign-extension)
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsi, 0x42424242, "RSI should be preserved");
    assert_eq!(regs.rdi, 0x2a2a2a2a, "RDI should be preserved");
}

// Test multiple CR reads in sequence
#[test]
fn test_multiple_cr_reads() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x0f, 0x20, 0xd3, // MOV RBX, CR2
        0x0f, 0x20, 0xd9, // MOV RCX, CR3
        0x0f, 0x20, 0xe2, // MOV RDX, CR4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // All should complete successfully
    assert!(regs.rax != 0, "CR0 should be non-zero");
}

// Test CR operations with R8-R15
#[test]
fn test_cr0_to_r8() {
    let code = [
        0x49, 0x0f, 0x20, 0xc0, // MOV R8, CR0 (REX.W + REX.B extends rm field)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.r8 != 0, "CR0 loaded into R8");
}

// Test CR0 to R15
#[test]
fn test_cr0_to_r15() {
    let code = [
        0x49, 0x0f, 0x20, 0xc7, // MOV R15, CR0 (REX.B extends rm field)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.r15 != 0, "CR0 loaded into R15");
}

// Test CR3 to R8
#[test]
fn test_cr3_to_r8() {
    let code = [
        0x49, 0x0f, 0x20, 0xd8, // MOV R8, CR3 (REX.B extends rm field)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs.r8;
}

// Test CR4 to R9
#[test]
fn test_cr4_to_r9() {
    let code = [
        0x49, 0x0f, 0x20, 0xe1, // MOV R9, CR4 (REX.B extends rm field)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs.r9;
}

// Test writing from R8 to CR0
#[test]
fn test_r8_to_cr0() {
    let code = [
        0x49, 0x0f, 0x20, 0xc0, // MOV R8, CR0 (REX.B extends rm field)
        0x49, 0x0f, 0x22, 0xc0, // MOV CR0, R8 (REX.B extends rm field)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test CR flags are undefined (MOV CR affects flags)
#[test]
fn test_mov_cr_flags_undefined() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX (sets ZF)
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Flags are undefined after MOV CR
    let _ = regs.rflags;
}

// Test sequential CR writes
#[test]
fn test_sequential_cr_writes() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test CR2 with different addresses
#[test]
fn test_cr2_various_addresses() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x01, 0x00, // MOV RAX, 0x10000
        0x0f, 0x22, 0xd0, // MOV CR2, RAX
        0x0f, 0x20, 0xd1, // MOV RCX, CR2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0x10000, "CR2 should contain 0x10000");
}

// Test CR operations preserve stack
#[test]
fn test_cr_preserves_stack() {
    let code = [
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x50, // PUSH RAX
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0x58, // POP RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x42, "Stack value should be preserved");
}

// Test CR0 read preserves instruction pointer advancement
#[test]
fn test_cr0_read_advances_rip() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x48, 0xc7, 0xc3, 0x99, 0x00, 0x00, 0x00, // MOV RBX, 0x99
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x99, "RIP should advance correctly");
}

// Test CR8 priority levels (0-15)
#[test]
fn test_cr8_priority_level_15() {
    let code = [
        0x48, 0xc7, 0xc0, 0x0f, 0x00, 0x00, 0x00, // MOV RAX, 15
        0x44, 0x0f, 0x22, 0xc0, // MOV CR8, RAX
        0x44, 0x0f, 0x20, 0xc1, // MOV RCX, CR8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 15, "CR8 should contain priority 15");
}

// Test CR operations in loop
#[test]
fn test_cr_operations_in_loop() {
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0 (offset 0, 7 bytes)
        // loop: (offset 7)
        0x0f, 0x20, 0xc0, // MOV RAX, CR0 (offset 7, 3 bytes)
        0x48, 0x83, 0xc1, 0x01, // ADD RCX, 1 (offset 10, 4 bytes)
        0x48, 0x83, 0xf9, 0x03, // CMP RCX, 3 (offset 14, 4 bytes)
        0x75, 0xf3, // JNZ loop (offset 18, disp = 7 - 20 = -13 = 0xf3)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 3, "Loop should complete");
}

// Test CR0 multiple register destinations
#[test]
fn test_cr0_to_multiple_registers() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x0f, 0x20, 0xc3, // MOV RBX, CR0
        0x0f, 0x20, 0xc1, // MOV RCX, CR0
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // All should have same CR0 value
    assert_eq!(regs.rax, regs.rbx, "CR0 values should match");
    assert_eq!(regs.rax, regs.rcx, "CR0 values should match");
}

// ============================================================================
// Strengthened: CR0/CR4 specific bit read/write assertions.
// ============================================================================

// Set CR0.WP (bit 16) on top of the default, read it back exactly.
#[test]
fn test_cr0_set_wp_bit_roundtrip() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0  (= 0x00050033)
        0x48, 0x0d, 0x00, 0x00, 0x01, 0x00, // OR RAX, 0x10000 (WP, bit 16)
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0x0f, 0x20, 0xc3, // MOV RBX, CR0 (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // WP was already set in 0x00050033, OR is idempotent; value unchanged.
    assert_eq!(regs.rbx, 0x00050033, "CR0 retains WP after OR");
    assert!(regs.rbx & (1 << 16) != 0, "WP set");
}

// Clearing CR0.MP (bit 1) and reading it back. Avoids touching PG/PE so the
// no-paging instruction-fetch path stays valid.
#[test]
fn test_cr0_clear_mp_bit_roundtrip() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0 (= 0x00050033, MP set)
        0x48, 0x83, 0xe0, 0xfd, // AND RAX, ~2 (clear MP, bit 1)
        0x0f, 0x22, 0xc0, // MOV CR0, RAX
        0x0f, 0x20, 0xc1, // MOV RCX, CR0
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rcx, 0x00050031, "CR0 with MP cleared");
    assert_eq!(regs.rcx & 2, 0, "MP cleared");
    assert!(regs.rcx & 1 != 0, "PE preserved");
}

// Write a fresh CR4 value (PAE|PSE|OSFXSR|OSXSAVE) and read it back exactly.
#[test]
fn test_cr4_write_read_exact() {
    // 0x30 = PAE|PSE; 0x200 = OSFXSR (bit 9); 0x40000 = OSXSAVE (bit 18).
    let code = [
        0x48, 0xc7, 0xc0, 0x30, 0x02, 0x04, 0x00, // MOV RAX, 0x40230
        0x0f, 0x22, 0xe0, // MOV CR4, RAX
        0x0f, 0x20, 0xe3, // MOV RBX, CR4
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx, 0x40230, "CR4 stores written value exactly");
    assert!(regs.rbx & (1 << 5) != 0, "PAE");
    assert!(regs.rbx & (1 << 4) != 0, "PSE");
    assert!(regs.rbx & (1 << 9) != 0, "OSFXSR");
    assert!(regs.rbx & (1 << 18) != 0, "OSXSAVE");
}

// CLTS clears CR0.TS (bit 3). Set TS, then CLTS, verify it is cleared.
#[test]
fn test_clts_clears_cr0_ts() {
    let code = [
        0x0f, 0x20, 0xc0, // MOV RAX, CR0
        0x48, 0x83, 0xc8, 0x08, // OR RAX, 8 (TS, bit 3)
        0x0f, 0x22, 0xc0, // MOV CR0, RAX (TS now set)
        0x0f, 0x06, // CLTS
        0x0f, 0x20, 0xc3, // MOV RBX, CR0
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx & 8, 0, "CLTS cleared CR0.TS");
    // The rest of CR0 (PE etc.) is preserved.
    assert!(regs.rbx & 1 != 0, "PE preserved by CLTS");
}

#[test]
fn mov_from_cr_ignores_mod_bits_and_consumes_no_sib_or_displacement() {
    for modrm in [0x00, 0x40, 0x80, 0xC0] {
        let code = [
            0x0F, 0x20, modrm, // MOV RAX,CR0; ModR/M.mod is ignored
            0xBB, 0x78, 0x56, 0x34, 0x12, // MOV EBX,0x12345678
            0xF4,
        ];
        let (mut vcpu, _) = setup_vm(&code, None);
        let regs = run_until_hlt(&mut vcpu).unwrap();
        assert_eq!(regs.rax, 0x0005_0033, "ModR/M={modrm:#04x}");
        assert_eq!(regs.rbx, 0x1234_5678, "ModR/M={modrm:#04x}");
    }
}

#[test]
fn mov_from_cr_ignores_non_lock_prefixes_and_preserves_deterministic_flags() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // neutral REX and ignored REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        let (mut vcpu, _) = setup_vm(&[prefix, 0x0F, 0x20, 0xC3], None);
        let before = vcpu.get_regs().unwrap().rflags;
        assert!(vcpu.step().unwrap().is_none(), "prefix {prefix:#04x}");
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rbx, 0x0005_0033, "prefix {prefix:#04x}");
        assert_eq!(regs.rflags, before, "prefix {prefix:#04x}");
        assert_eq!(regs.rip, CODE_ADDR + 4, "prefix {prefix:#04x}");
    }
}

#[test]
fn mov_from_cr_decode_faults_precede_privilege_faults_without_committing() {
    for (name, code) in [
        ("reserved-cr1", &[0x0F, 0x20, 0xC8][..]),
        ("reserved-cr9", &[0x44, 0x0F, 0x20, 0xC8]),
        ("lock-valid-cr0", &[0xF0, 0x0F, 0x20, 0xC0]),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(code, None);
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cr0 |= 1;
        sregs.cs.selector = 3;
        vcpu.set_sregs(&sregs).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
        vcpu.set_regs(&regs).unwrap();

        let error = vcpu
            .step()
            .expect_err("decode-invalid MOV-from-CR must #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: wrong exception: {error}"
        );
        assert_eq!(vcpu.get_regs().unwrap().rax, regs.rax, "{name}");
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }

    // LLVM encodes MOV R16,CR0 as D5 90 20 C0. A valid REX2 form must
    // survive decode and reach the ordinary CPL check without committing R16.
    let (mut vcpu, _) = setup_apx_vm_no_idt(&[0xD5, 0x90, 0x20, 0xC0], None);
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 3;
    vcpu.set_sregs(&sregs).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.r16 = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.set_regs(&regs).unwrap();
    let error = vcpu
        .step()
        .expect_err("valid REX2 MOV-from-CR must reach the CPL check");
    assert!(error.to_string().contains("IDT entry 13 not present"));
    assert_eq!(vcpu.get_regs().unwrap().r16, regs.r16);
    assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR);

    // REX2.R4 selects the nonexistent CR16. That decode fault precedes CPL.
    let (mut vcpu, _) = setup_apx_vm_no_idt(&[0xD5, 0xC0, 0x20, 0xC0], None);
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 3;
    vcpu.set_sregs(&sregs).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.set_regs(&regs).unwrap();
    let error = vcpu
        .step()
        .expect_err("REX2 MOV-from-CR16 must #UD before the CPL check");
    assert!(error.to_string().contains("IDT entry 6 not present"));
    assert_eq!(vcpu.get_regs().unwrap().rax, regs.rax);
    assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR);
}

#[test]
fn mov_from_cr_privilege_check_handles_cpl3_vm86_and_real_mode_precisely() {
    for (name, selector, vm) in [
        ("protected-cpl3", 3, false),
        ("virtual-8086-cs-rpl0", 0, true),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x20, 0xD8], None);
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cr0 |= 1;
        sregs.cr3 = 0x1234_5000;
        sregs.cs.selector = selector;
        vcpu.set_sregs(&sregs).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
        if vm {
            regs.rflags |= 1 << 17;
        }
        vcpu.set_regs(&regs).unwrap();

        let error = vcpu.step().expect_err("MOV-from-CR privilege must #GP(0)");
        assert!(
            error.to_string().contains("IDT entry 13 not present"),
            "{name}: wrong exception: {error}"
        );
        assert_eq!(vcpu.get_regs().unwrap().rax, regs.rax, "{name}");
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }

    let (mut real_mode, _) = setup_vm_no_idt(&[0x0F, 0x20, 0xD8], None);
    let mut sregs = real_mode.get_sregs().unwrap();
    sregs.cr0 &= !1;
    sregs.cr3 = 0x1234_5000;
    sregs.cs.selector = 3;
    real_mode.set_sregs(&sregs).unwrap();
    assert!(real_mode.step().unwrap().is_none());
    assert_eq!(real_mode.get_regs().unwrap().rax, 0x1234_5000);
}

#[test]
fn mov_from_cr_outside_64_bit_mode_is_an_ignored_prefix_32_bit_write() {
    let (mut vcpu, _) = setup_vm_no_idt(&[0x66, 0x0F, 0x20, 0xD0], None);
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.l = false;
    sregs.cs.db = true;
    sregs.cr2 = 0xFFFF_AAAA_8765_4321;
    vcpu.set_sregs(&sregs).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = u64::MAX;
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.step().unwrap().is_none());
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rax, 0x8765_4321);
    assert_eq!(regs.rip, CODE_ADDR + 4);
}

#[test]
fn mov_to_cr_decode_faults_precede_privilege_faults_without_committing() {
    for (name, code) in [
        ("reserved-cr1", &[0x0F, 0x22, 0xC8][..]),
        ("reserved-cr9", &[0x44, 0x0F, 0x22, 0xC8]),
        ("lock-valid-cr0", &[0xF0, 0x0F, 0x22, 0xC0]),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(code, None);
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cr0 |= 1;
        sregs.cs.selector = 3;
        let before = sregs.clone();
        vcpu.set_sregs(&sregs).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
        vcpu.set_regs(&regs).unwrap();

        let error = vcpu.step().expect_err("decode-invalid MOV-to-CR must #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: wrong exception: {error}"
        );
        let after = vcpu.get_sregs().unwrap();
        assert_eq!(after.cr0, before.cr0, "{name}");
        assert_eq!(after.cr2, before.cr2, "{name}");
        assert_eq!(after.cr3, before.cr3, "{name}");
        assert_eq!(after.cr4, before.cr4, "{name}");
        assert_eq!(after.cr8, before.cr8, "{name}");
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }

    // LLVM encodes MOV CR0,R16 as D5 90 22 C0. A valid REX2 form must
    // survive decode and reach the ordinary CPL check without committing CR0.
    let mut initial = rax::vm::vcpu::Registers::default();
    initial.r16 = 0xA5A5_5A5A_DEAD_BEEF;
    let (mut vcpu, _) = setup_apx_vm_no_idt(&[0xD5, 0x90, 0x22, 0xC0], Some(initial));
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 3;
    let before = sregs.clone();
    vcpu.set_sregs(&sregs).unwrap();
    let error = vcpu
        .step()
        .expect_err("valid REX2 MOV-to-CR must reach the CPL check");
    assert!(error.to_string().contains("IDT entry 13 not present"));
    assert_eq!(vcpu.get_sregs().unwrap().cr0, before.cr0);
    assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR);

    // REX2.R4 selects the nonexistent CR16. That decode fault precedes CPL.
    let (mut vcpu, _) = setup_apx_vm_no_idt(&[0xD5, 0xC0, 0x22, 0xC0], None);
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 3;
    let before = sregs.clone();
    vcpu.set_sregs(&sregs).unwrap();
    let error = vcpu
        .step()
        .expect_err("REX2 MOV-to-CR16 must #UD before the CPL check");
    assert!(error.to_string().contains("IDT entry 6 not present"));
    assert_eq!(vcpu.get_sregs().unwrap().cr0, before.cr0);
    assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR);
}

#[test]
fn mov_to_cr_privilege_and_non_64_bit_source_width_are_precise() {
    for (name, selector, vm) in [
        ("protected-cpl3", 3, false),
        ("virtual-8086-cs-rpl0", 0, true),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x22, 0xD0], None);
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cr0 |= 1;
        sregs.cr2 = 0x1111_2222_3333_4444;
        sregs.cs.selector = selector;
        vcpu.set_sregs(&sregs).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
        if vm {
            regs.rflags |= 1 << 17;
        }
        vcpu.set_regs(&regs).unwrap();

        let error = vcpu.step().expect_err("MOV-to-CR privilege must #GP(0)");
        assert!(
            error.to_string().contains("IDT entry 13 not present"),
            "{name}: wrong exception: {error}"
        );
        assert_eq!(vcpu.get_sregs().unwrap().cr2, sregs.cr2, "{name}");
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }

    let (mut real_mode, _) = setup_vm_no_idt(&[0x66, 0x0F, 0x22, 0xD0], None);
    let mut sregs = real_mode.get_sregs().unwrap();
    sregs.cr0 &= !1;
    sregs.cs.l = false;
    sregs.cs.db = true;
    sregs.cs.selector = 3;
    real_mode.set_sregs(&sregs).unwrap();
    let mut regs = real_mode.get_regs().unwrap();
    regs.rax = 0xFFFF_AAAA_8765_4321;
    real_mode.set_regs(&regs).unwrap();

    assert!(real_mode.step().unwrap().is_none());
    assert_eq!(real_mode.get_sregs().unwrap().cr2, 0x8765_4321);
    assert_eq!(real_mode.get_regs().unwrap().rip, CODE_ADDR + 4);
}

#[test]
fn mov_to_cr0_faults_are_non_committing_and_reserved_low_bits_are_ignored() {
    for (name, value) in [
        ("pg-without-pe", 1 << 31),
        ("nw-without-cd", 1 << 29),
        ("reserved-high", 1 << 32),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x22, 0xC0], None);
        let before = vcpu.get_sregs().unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = value;
        let before_flags = regs.rflags;
        vcpu.set_regs(&regs).unwrap();

        let error = vcpu.step().expect_err("invalid CR0 value must #GP(0)");
        assert!(
            error.to_string().contains("IDT entry 13 not present"),
            "{name}: wrong exception: {error}"
        );
        assert_eq!(vcpu.get_sregs().unwrap().cr0, before.cr0, "{name}");
        assert_eq!(vcpu.get_sregs().unwrap().efer, before.efer, "{name}");
        assert_eq!(vcpu.get_regs().unwrap().rflags, before_flags, "{name}");
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }

    let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x22, 0xC0], None);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 1 | (1 << 6) | (1 << 15); // PE plus two reserved low fields
    let before_flags = regs.rflags;
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.get_sregs().unwrap().cr0, 1 | (1 << 4));
    assert_eq!(vcpu.get_regs().unwrap().rflags, before_flags);
}

#[test]
fn mov_to_cr3_validates_physical_width_pcide_and_normalizes_non_pcid_fields() {
    let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x22, 0xD8], None);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0x0000_1234_5678_9FFF;
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.get_sregs().unwrap().cr3, 0x0000_1234_5678_9018);

    for (name, value) in [
        ("no-flush-without-pcide", 1 << 63),
        ("above-maxphyaddr", 1 << 48),
    ] {
        let (mut fault, _) = setup_vm_no_idt(&[0x0F, 0x22, 0xD8], None);
        let before = fault.get_sregs().unwrap().cr3;
        let mut regs = fault.get_regs().unwrap();
        regs.rax = value;
        fault.set_regs(&regs).unwrap();
        let error = fault.step().expect_err("invalid CR3 value must #GP(0)");
        assert!(
            error.to_string().contains("IDT entry 13 not present"),
            "{name}: wrong exception: {error}"
        );
        assert_eq!(fault.get_sregs().unwrap().cr3, before, "{name}");
    }

    let (mut pcid, _) = setup_vm_no_idt(&[0x0F, 0x22, 0xD8], None);
    let mut sregs = pcid.get_sregs().unwrap();
    sregs.cr4 |= 1 << 17;
    sregs.efer |= 1 << 10;
    pcid.set_sregs(&sregs).unwrap();
    let mut regs = pcid.get_regs().unwrap();
    regs.rax = (1 << 63) | 0x0000_1234_5678_9ABC;
    pcid.set_regs(&regs).unwrap();
    assert!(pcid.step().unwrap().is_none());
    assert_eq!(pcid.get_sregs().unwrap().cr3, 0x0000_1234_5678_9ABC);
}

#[test]
fn mov_to_control_mode_transitions_validate_before_commit() {
    for (name, cr0, cr3, cr4, efer, cs_l, tr_type, value, modrm) in [
        (
            "pcide-with-nonzero-pcid",
            1,
            1,
            1 << 5,
            1 << 10,
            true,
            9,
            (1 << 5) | (1 << 17),
            0xE0,
        ),
        (
            "clear-pae-in-ia32e",
            1,
            0,
            1 << 5,
            1 << 10,
            true,
            9,
            0,
            0xE0,
        ),
        (
            "activate-ia32e-from-64-bit-cs",
            1,
            0,
            1 << 5,
            1 << 8,
            true,
            9,
            (1 << 31) | 1,
            0xC0,
        ),
        (
            "activate-ia32e-with-16-bit-tss",
            1,
            0,
            1 << 5,
            1 << 8,
            false,
            3,
            (1 << 31) | 1,
            0xC0,
        ),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x22, modrm], None);
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cr0 = cr0;
        sregs.cr3 = cr3;
        sregs.cr4 = cr4;
        sregs.efer = efer;
        sregs.cs.l = cs_l;
        sregs.tr.type_ = tr_type;
        let before = sregs.clone();
        vcpu.set_sregs(&sregs).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = value;
        vcpu.set_regs(&regs).unwrap();

        let error = vcpu
            .step()
            .expect_err("invalid mode transition must #GP(0)");
        assert!(
            error.to_string().contains("IDT entry 13 not present"),
            "{name}: wrong exception: {error}"
        );
        let after = vcpu.get_sregs().unwrap();
        assert_eq!(after.cr0, before.cr0, "{name}");
        assert_eq!(after.cr4, before.cr4, "{name}");
        assert_eq!(after.efer, before.efer, "{name}");
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }

    let (mut enter, _) = setup_vm_no_idt(&[0x0F, 0x22, 0xC0], None);
    let mut sregs = enter.get_sregs().unwrap();
    sregs.cr0 = 1;
    sregs.cr4 = 1 << 5;
    sregs.efer = 1 << 8;
    sregs.cs.l = false;
    sregs.tr.type_ = 9;
    enter.set_sregs(&sregs).unwrap();
    let mut regs = enter.get_regs().unwrap();
    regs.rax = (1 << 31) | 1;
    enter.set_regs(&regs).unwrap();
    assert!(enter.step().unwrap().is_none());
    let entered = enter.get_sregs().unwrap();
    assert_eq!(entered.cr0, (1 << 31) | (1 << 4) | 1);
    assert_ne!(entered.efer & (1 << 10), 0);
}
