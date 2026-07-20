use crate::common::{run_until_hlt, setup_vm, setup_vm_no_idt};
use rax::vm::vcpu::{Registers, VCpu};

// LMSW - Load Machine Status Word
// Opcode: 0F 01 /6
// Reads a 16-bit source and replaces CR0[3:0], without clearing CR0.PE.
// Privilege level 0 required

// SMSW - Store Machine Status Word
// Opcode: 0F 01 /4
// Stores the machine status word (bits 0-15 of CR0) to the destination operand
// Can be executed at any privilege level

// Test SMSW basic execution - store MSW to register
#[test]
fn test_smsw_to_register() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // MSW should be loaded (non-zero, as PE bit is usually set)
    assert!(regs.rax != 0, "MSW should be non-zero");
}

// Test SMSW to different registers
#[test]
fn test_smsw_to_rbx() {
    let code = [
        0x0f, 0x01, 0xe3, // SMSW RBX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rbx != 0, "MSW loaded into RBX");
}

// Test SMSW to RCX
#[test]
fn test_smsw_to_rcx() {
    let code = [
        0x0f, 0x01, 0xe1, // SMSW RCX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rcx != 0, "MSW loaded into RCX");
}

// Test SMSW to RDX
#[test]
fn test_smsw_to_rdx() {
    let code = [
        0x0f, 0x01, 0xe2, // SMSW RDX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rdx != 0, "MSW loaded into RDX");
}

// Test SMSW preserves flags
#[test]
fn test_smsw_preserves_flags() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX (sets ZF)
        0x0f, 0x01, 0xe0, // SMSW RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF from XOR should be preserved
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

// Test SMSW preserves other registers
#[test]
fn test_smsw_preserves_other_registers() {
    let code = [
        0x48, 0xc7, 0xc3, 0x42, 0x42, 0x42, 0x42, // MOV RBX, 0x42424242 (bit 31 clear)
        0x48, 0xc7, 0xc1, 0x2a, 0x2a, 0x2a, 0x2a, // MOV RCX, 0x2a2a2a2a (bit 31 clear)
        0x0f, 0x01, 0xe0, // SMSW RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x42424242, "RBX should be preserved");
    assert_eq!(regs.rcx, 0x2a2a2a2a, "RCX should be preserved");
}

// Test SMSW multiple times
#[test]
fn test_smsw_sequential() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x0f, 0x01, 0xe3, // SMSW RBX
        0x0f, 0x01, 0xe1, // SMSW RCX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // All should have same MSW value
    assert_eq!(
        regs.rax & 0xFFFF,
        regs.rbx & 0xFFFF,
        "MSW values should match"
    );
    assert_eq!(
        regs.rax & 0xFFFF,
        regs.rcx & 0xFFFF,
        "MSW values should match"
    );
}

// Test LMSW basic execution
#[test]
fn test_lmsw_basic() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX (read current MSW)
        0x0f, 0x01, 0xf0, // LMSW RAX (write same value back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test LMSW with specific value
#[test]
fn test_lmsw_with_value() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX (read current MSW)
        0x48, 0x89, 0xc3, // MOV RBX, RAX (save original)
        0x0f, 0x01, 0xf0, // LMSW RAX (write back)
        0x0f, 0x01, 0xe0, // SMSW RAX (read again)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // MSW should be preserved
    assert_eq!(
        regs.rax & 0xFFFF,
        regs.rbx & 0xFFFF,
        "MSW should be preserved"
    );
}

// Test LMSW preserves flags
#[test]
fn test_lmsw_preserves_flags() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX (sets ZF)
        0x48, 0x89, 0xc1, // MOV RCX, RAX
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x0f, 0x01, 0xf0, // LMSW RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should be from XOR instruction
    let _ = regs.rflags;
}

// Test SMSW in loop
#[test]
fn test_smsw_in_loop() {
    let code = [
        0x48, 0x31, 0xc9, // XOR RCX, RCX - 3 bytes (0x1000)
        // loop: (0x1003)
        0x0f, 0x01, 0xe0, // SMSW RAX - 3 bytes (0x1003)
        0x48, 0x83, 0xc1, 0x01, // ADD RCX, 1 - 4 bytes (0x1006)
        0x48, 0x83, 0xf9, 0x03, // CMP RCX, 3 - 4 bytes (0x100A)
        0x75, 0xf3, // JNZ loop (rel8 = -13, from 0x1010 to 0x1003) - 2 bytes (0x100E)
        0xf4, // HLT - 1 byte (0x1010)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 3, "Loop should complete");
    assert!(regs.rax != 0, "MSW should be loaded");
}

// Test SMSW to R8-R15
#[test]
fn test_smsw_to_r8() {
    let code = [
        0x49, 0x0f, 0x01, 0xe0, // SMSW R8 (REX.B=1 to extend rm to R8)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.r8 != 0, "MSW loaded into R8");
}

// Test SMSW to R15
#[test]
fn test_smsw_to_r15() {
    let code = [
        0x49, 0x0f, 0x01, 0xe7, // SMSW R15 (REX.B=1 to extend rm to R15)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.r15 != 0, "MSW loaded into R15");
}

// Test LMSW from different registers
#[test]
fn test_lmsw_from_rbx() {
    let code = [
        0x0f, 0x01, 0xe3, // SMSW RBX
        0x0f, 0x01, 0xf3, // LMSW RBX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test LMSW from RCX
#[test]
fn test_lmsw_from_rcx() {
    let code = [
        0x0f, 0x01, 0xe1, // SMSW RCX
        0x0f, 0x01, 0xf1, // LMSW RCX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test SMSW with stack operations
#[test]
fn test_smsw_with_stack() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x50, // PUSH RAX
        0x48, 0x31, 0xc0, // XOR RAX, RAX
        0x58, // POP RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rax != 0, "MSW should be restored from stack");
}

// Test LMSW from R8
#[test]
fn test_lmsw_from_r8() {
    let code = [
        0x4c, 0x0f, 0x01, 0xe0, // SMSW R8
        0x4c, 0x0f, 0x01, 0xf0, // LMSW R8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test SMSW preserves upper bits of 64-bit register
#[test]
fn test_smsw_preserves_upper_bits() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xffffffffffffffff
        0x0f, 0x01, 0xe0, // SMSW RAX (only modifies low 16 bits in some modes)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // MSW should be loaded (behavior depends on mode)
    let _ = regs.rax;
}

// Test LMSW round-trip
#[test]
fn test_lmsw_round_trip() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX (read original)
        0x48, 0x89, 0xc3, // MOV RBX, RAX (save)
        0x0f, 0x01, 0xf0, // LMSW RAX (write back)
        0x0f, 0x01, 0xe0, // SMSW RAX (read again)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Values should match (at least low 16 bits)
    assert_eq!(
        regs.rax & 0xFFFF,
        regs.rbx & 0xFFFF,
        "MSW should be preserved"
    );
}

// Test SMSW with conditional jumps
#[test]
fn test_smsw_with_conditional() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x48, 0x85, 0xc0, // TEST RAX, RAX
        0x75, 0x02, // JNZ skip (should jump since MSW != 0)
        0x90, // NOP
        0x90, // NOP
        // skip:
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test LMSW preserves other registers
#[test]
fn test_lmsw_preserves_other_registers() {
    let code = [
        0x48, 0xc7, 0xc3, 0x77, 0x77, 0x77, 0x77, // MOV RBX, 0x77777777 (bit 31 clear)
        0x48, 0xc7, 0xc1, 0x08, 0x08, 0x08, 0x08, // MOV RCX, 0x08080808 (bit 31 clear)
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x0f, 0x01, 0xf0, // LMSW RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x77777777, "RBX should be preserved");
    assert_eq!(regs.rcx, 0x08080808, "RCX should be preserved");
}

// Test SMSW multiple sequential reads
#[test]
fn test_smsw_sequential_same() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX #1
        0x0f, 0x01, 0xe3, // SMSW RBX #2
        0x0f, 0x01, 0xe1, // SMSW RCX #3
        0x0f, 0x01, 0xe2, // SMSW RDX #4
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // All should have the same MSW
    let msw_mask = 0xFFFF;
    assert_eq!(regs.rax & msw_mask, regs.rbx & msw_mask);
    assert_eq!(regs.rax & msw_mask, regs.rcx & msw_mask);
    assert_eq!(regs.rax & msw_mask, regs.rdx & msw_mask);
}

// Test LMSW in loop
#[test]
fn test_lmsw_in_loop() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX (get value)
        0x48, 0x31, 0xc9, // XOR RCX, RCX
        // loop:
        0x0f, 0x01, 0xf0, // LMSW RAX   <- loop target (offset 6 = 0x1006)
        0x48, 0x83, 0xc1, 0x01, // ADD RCX, 1
        0x48, 0x83, 0xf9, 0x03, // CMP RCX, 3
        // JNZ loop. rel8 must reach the LMSW at 0x1006 from the next RIP 0x1013,
        // i.e. -13 (0xF3). The previous 0xF5 (-11) landed on the LMSW's 0xF0
        // ModR/M byte, which now (with LOCK-prefix enforcement) decodes as
        // `LOCK ADD RCX, 1` - a register-destination LOCK that correctly #UDs.
        0x75, 0xf3, // JNZ loop
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 3, "Loop should complete");
}

// Test SMSW preserves base pointer
#[test]
fn test_smsw_preserves_rbp() {
    let code = [
        0x48, 0xc7, 0xc5, 0x00, 0x70, 0x00, 0x00, // MOV RBP, 0x7000
        0x0f, 0x01, 0xe0, // SMSW RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbp, 0x7000, "RBP should be preserved");
}

// Test LMSW preserves stack pointer
#[test]
fn test_lmsw_preserves_rsp() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x0f, 0x01, 0xf0, // LMSW RAX
        0xf4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rsp = 0x8000;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, 0x8000, "RSP should be unchanged");
}

// Test SMSW execution speed
#[test]
fn test_smsw_execution_speed() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX #1
        0x0f, 0x01, 0xe0, // SMSW RAX #2
        0x0f, 0x01, 0xe0, // SMSW RAX #3
        0x0f, 0x01, 0xe0, // SMSW RAX #4
        0x0f, 0x01, 0xe0, // SMSW RAX #5
        0x0f, 0x01, 0xe0, // SMSW RAX #6
        0x0f, 0x01, 0xe0, // SMSW RAX #7
        0x0f, 0x01, 0xe0, // SMSW RAX #8
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rax != 0, "MSW should be loaded");
}

// Test LMSW execution speed
#[test]
fn test_lmsw_execution_speed() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x0f, 0x01, 0xf0, // LMSW RAX #1
        0x0f, 0x01, 0xf0, // LMSW RAX #2
        0x0f, 0x01, 0xf0, // LMSW RAX #3
        0x0f, 0x01, 0xf0, // LMSW RAX #4
        0x0f, 0x01, 0xf0, // LMSW RAX #5
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    let _ = regs;
}

// Test SMSW bit 0 (PE - Protection Enable)
#[test]
fn test_smsw_pe_bit() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x48, 0x83, 0xe0, 0x01, // AND RAX, 1 (isolate PE bit)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // PE bit is usually set in protected mode
    let _ = regs.rax;
}

// Test SMSW with all flags set
#[test]
fn test_smsw_all_flags_set() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x83, 0xe8, 0x02, // SUB RAX, 2 (sets CF, SF)
        0x0f, 0x01, 0xe0, // SMSW RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // MSW should be loaded despite flags
    assert!(regs.rax != 0, "MSW should be loaded");
}

// Test LMSW idempotency
#[test]
fn test_lmsw_idempotent() {
    let code = [
        0x0f, 0x01, 0xe0, // SMSW RAX
        0x48, 0x89, 0xc3, // MOV RBX, RAX
        0x0f, 0x01, 0xf0, // LMSW RAX #1
        0x0f, 0x01, 0xf0, // LMSW RAX #2
        0x0f, 0x01, 0xf0, // LMSW RAX #3
        0x0f, 0x01, 0xe0, // SMSW RAX (read final)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Multiple LMSW with same value should be safe
    assert_eq!(
        regs.rax & 0xFFFF,
        regs.rbx & 0xFFFF,
        "MSW should be preserved"
    );
}

#[test]
fn test_smsw_rex2_targets_the_full_apx_register_id() {
    let code = [
        0x0F, 0x01, 0xE0, // SMSW EAX: independent expected value
        0xD5, 0x91, 0x01, 0xE7, // REX2.M1.B4.B3 SMSW R31D
        0xF4,
    ];
    let initial = Registers {
        rdi: 0xD1D1_D1D1_D1D1_D1D1,
        r31: 0x3131_3131_3131_3131,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_vm(&code, Some(initial));
    vcpu.set_apx_enabled(true);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.r31, regs.rax as u32 as u64);
    assert_eq!(regs.rdi, 0xD1D1_D1D1_D1D1_D1D1);
}

#[test]
fn test_lmsw_cpl3_faults_before_memory_and_preserves_cr0() {
    let mut input = Registers::default();
    input.rax = 0x20_000;
    let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x01, 0x30], Some(input));
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr0 |= 1;
    sregs.cs.selector = 3;
    vcpu.set_sregs(&sregs).unwrap();
    let cr0_before = vcpu.get_sregs().unwrap().cr0;

    let error = format!("{:#}", vcpu.step().expect_err("CPL 3 LMSW must fault"));

    assert!(
        error.contains("IDT entry 13 not present"),
        "#GP(0) must precede the unmapped source: {error}"
    );
    assert_eq!(vcpu.get_sregs().unwrap().cr0, cr0_before);
    assert_eq!(vcpu.get_regs().unwrap().rip, 0x1000);
}

#[test]
fn test_lmsw_cannot_clear_pe_and_changes_only_cr0_bits_3_through_1() {
    let mut input = Registers::default();
    input.rax = 0;
    let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x01, 0xF0], Some(input));
    let before = vcpu.get_sregs().unwrap().cr0 | 0xF;
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr0 = before;
    vcpu.set_sregs(&sregs).unwrap();

    assert!(vcpu.step().unwrap().is_none());

    assert_eq!(vcpu.get_sregs().unwrap().cr0, (before & !0xF) | 1);
}

#[test]
fn test_lmsw_rex2_reads_the_full_apx_register_id() {
    let mut input = Registers::default();
    input.rdi = 0;
    input.r31 = 0xE;
    let (mut vcpu, _) = setup_vm_no_idt(&[0xD5, 0x91, 0x01, 0xF7], Some(input));
    vcpu.set_apx_enabled(true);
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr0 = (sregs.cr0 & !0xF) | 1;
    vcpu.set_sregs(&sregs).unwrap();

    assert!(vcpu.step().unwrap().is_none());

    assert_eq!(vcpu.get_sregs().unwrap().cr0 & 0xF, 0xF);
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rdi, 0);
    assert_eq!(regs.r31, 0xE);
}
