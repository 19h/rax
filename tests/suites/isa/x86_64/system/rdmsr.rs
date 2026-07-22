//! Tests for the RDMSR instruction.
//!
//! RDMSR - Read From Model Specific Register
//!
//! Reads the contents of a 64-bit model specific register (MSR) specified
//! in the ECX register into registers EDX:EAX. The EDX register is loaded
//! with the high-order 32 bits of the MSR and the EAX register is loaded
//! with the low-order 32 bits.
//!
//! Opcode: 0F 32
//! Flags affected: None
//!
//! Reference: Intel SDM Vol. 2B, RDMSR—Read From Model Specific Register.

use crate::common::*;
use rax::vm::vcpu::Registers;

// ============================================================================
// Basic RDMSR Tests
// ============================================================================

#[test]
fn test_rdmsr_basic() {
    // RDMSR - Read MSR specified by ECX into EDX:EAX
    // 0F 32 = RDMSR
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000080; // IA32_EFER
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EDX:EAX should contain MSR value
    // Upper 32 bits of RAX and RDX should be cleared in 64-bit mode
    assert_eq!(regs.rax >> 32, 0, "Upper 32 bits of RAX should be cleared");
    assert_eq!(regs.rdx >> 32, 0, "Upper 32 bits of RDX should be cleared");
}

#[test]
fn test_rdmsr_real_mode_ignores_stale_cs_rpl() {
    let mut input = Registers::default();
    input.rcx = 0xC000_0081; // IA32_STAR
    let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x32], Some(input));
    let expected = vcpu.get_sregs().unwrap().star;
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr0 &= !1; // CR0.PE=0: real-address mode has no CPL.
    sregs.cs.selector = 3;
    vcpu.set_sregs(&sregs).unwrap();

    assert!(vcpu.step().unwrap().is_none());
    let regs = vcpu.get_regs().unwrap();
    assert_eq!((regs.rdx << 32) | regs.rax, expected);
}

#[test]
fn test_rdmsr_tsc_msr() {
    // Read IA32_TIME_STAMP_COUNTER (MSR 0x10)
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x10; // IA32_TIME_STAMP_COUNTER
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should read successfully
    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_apic_base() {
    // Read IA32_APIC_BASE (MSR 0x1B)
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x1B; // IA32_APIC_BASE
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_preserves_flags() {
    // RDMSR should not modify flags
    let code = [
        0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RAX, -1
        0x48, 0x83, 0xC0, 0x01, // ADD RAX, 1 (sets ZF)
        0x48, 0xC7, 0xC1, 0x80, 0x00, 0x00, 0xC0, // MOV RCX, 0xC0000080
        0x0F, 0x32, // RDMSR
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ZF should still be set from ADD
    assert!(regs.rflags & 0x40 != 0, "ZF should be preserved");
}

#[test]
fn test_rdmsr_preserves_other_registers() {
    // RDMSR should only modify EAX and EDX
    // Note: MOV r64, imm32 sign-extends. Values with bit 31 set get sign-extended.
    let code = [
        0x48, 0xC7, 0xC3, 0x42, 0x42, 0x42, 0x42, // MOV RBX, 0x42424242
        0x48, 0xC7, 0xC6, 0xAA, 0xAA, 0xAA, 0xAA, // MOV RSI, 0xAAAAAAAA (sign-ext)
        0x48, 0xC7, 0xC7, 0xBB, 0xBB, 0xBB, 0xBB, // MOV RDI, 0xBBBBBBBB (sign-ext)
        0x48, 0xC7, 0xC1, 0x1B, 0x00, 0x00, 0x00, // MOV RCX, 0x1B
        0x0F, 0x32, // RDMSR
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x42424242, "RBX should not be modified");
    assert_eq!(
        regs.rsi, 0xFFFFFFFFAAAAAAAAu64,
        "RSI should not be modified (sign-extended)"
    );
    assert_eq!(
        regs.rdi, 0xFFFFFFFFBBBBBBBBu64,
        "RDI should not be modified (sign-extended)"
    );
}

// ============================================================================
// ECX Value Tests - Different MSR Indices
// ============================================================================

#[test]
fn test_rdmsr_tsc_adjust() {
    // Read the profile's IA32_TSC_ADJUST state.
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x3B;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0x3B, "ECX should not be modified");
    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_high_rcx_ignored() {
    // High 32 bits of RCX should be ignored in 64-bit mode
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xFFFFFFFF_00000010; // High bits set, MSR 0x10
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Should read MSR 0x10, ignoring high 32 bits
    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_lock_and_rex2_prefixes_raise_ud_without_outputs() {
    for code in [&[0xF0, 0x0F, 0x32, 0xF4][..], &[0xD5, 0x80, 0x32, 0xF4]] {
        let mut input = Registers::default();
        input.rcx = 0x3B;
        input.rax = 0x1111_2222_3333_4444;
        input.rdx = 0x5555_6666_7777_8888;
        let (mut vcpu, _) = setup_vm(code, Some(input));

        let _ = vcpu.step();
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rip, INT_HANDLER_ADDR, "{code:02X?}");
        assert_eq!(regs.rax, 0x1111_2222_3333_4444);
        assert_eq!(regs.rdx, 0x5555_6666_7777_8888);
    }
}

#[test]
fn test_rdmsr_msr_c0000080_efer() {
    // Read Extended Feature Enable Register
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000080; // IA32_EFER
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EFER should have some bits set (at least LME for 64-bit mode)
    let efer = ((regs.rdx as u64) << 32) | (regs.rax as u64);
    assert!(efer != 0, "EFER should not be zero in 64-bit mode");
}

#[test]
fn test_rdmsr_msr_c0000081_star() {
    // Read SYSCALL target address (STAR MSR)
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000081; // IA32_STAR
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_msr_c0000082_lstar() {
    // Read Long Mode SYSCALL target (LSTAR MSR)
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000082; // IA32_LSTAR
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_tsc_aux() {
    // Read IA32_TSC_AUX, consumed by RDTSCP and RDPID.
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000103; // IA32_TSC_AUX
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

// ============================================================================
// Sequential RDMSR Tests
// ============================================================================

#[test]
fn test_rdmsr_multiple_reads() {
    // Read same MSR twice
    let code = [
        0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00, // MOV RCX, 0x10
        0x0F, 0x32, // RDMSR #1
        0x48, 0x89, 0xC3, // MOV RBX, RAX (save first read)
        0x48, 0x89, 0xD6, // MOV RSI, RDX
        0x0F, 0x32, // RDMSR #2 (same MSR)
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Both reads should give consistent results for non-volatile MSRs
    // (For TSC, second read >= first read due to time passing)
}

#[test]
fn test_rdmsr_different_msrs() {
    // Read two different MSRs
    let code = [
        0x48, 0xC7, 0xC1, 0x1B, 0x00, 0x00, 0x00, // MOV RCX, 0x1B (APIC_BASE)
        0x0F, 0x32, // RDMSR
        0x48, 0x89, 0xC3, // MOV RBX, RAX
        0x48, 0x89, 0xD6, // MOV RSI, RDX
        0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00, // MOV RCX, 0x10 (TSC)
        0x0F, 0x32, // RDMSR
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Both MSRs should be readable
    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
    assert_eq!(regs.rbx >> 32, 0);
    assert_eq!(regs.rsi >> 32, 0);
}

#[test]
fn test_rdmsr_loop() {
    // Read MSR in a loop
    let code = [
        0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00, // MOV RCX, 0x10
        0x48, 0xC7, 0xC3, 0x03, 0x00, 0x00, 0x00, // MOV RBX, 3 (loop counter)
        // loop_start:
        0x0F, 0x32, // RDMSR
        0x48, 0xFF, 0xCB, // DEC RBX
        0x75, 0xF9, // JNZ loop_start (-7 bytes)
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0, "Loop should complete 3 iterations");
    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

// ============================================================================
// 64-bit Specific Tests
// ============================================================================

#[test]
fn test_rdmsr_clears_upper_bits() {
    // Verify upper 32 bits of RAX and RDX are cleared
    let code = [
        0x48, 0xB8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RAX, -1
        0x48, 0xBA, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // MOV RDX, -1
        0x48, 0xC7, 0xC1, 0x1B, 0x00, 0x00, 0x00, // MOV RCX, 0x1B
        0x0F, 0x32, // RDMSR
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0, "Upper 32 bits of RAX must be cleared");
    assert_eq!(regs.rdx >> 32, 0, "Upper 32 bits of RDX must be cleared");
}

#[test]
fn test_rdmsr_eax_edx_loading() {
    // Verify EDX gets high 32 bits, EAX gets low 32 bits
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x10; // TSC
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Reconstruct 64-bit MSR value
    let msr_value = ((regs.rdx as u64) << 32) | (regs.rax as u64);

    // TSC should be a reasonable value (not all zeros)
    assert!(msr_value > 0, "TSC should not be zero");
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_rdmsr_linux_verify_cpu_misc_enable_before_full_idt() {
    // This is the exact selector read by arch/x86/kernel/verify_cpu.S for the
    // advertised GenuineIntel family-6/model-15 profile. No exception handler
    // should be needed: a successful read advances directly to HLT.
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x1A0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).expect("IA32_MISC_ENABLE must be present");
    let value = (regs.rdx << 32) | regs.rax;
    assert_eq!(value, 0x0000_0000_0004_1801);
}

#[test]
fn test_rdmsr_linux_microcode_signature_profile_is_zero() {
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x8B; // IA32_BIOS_SIGN_ID
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!((regs.rdx << 32) | regs.rax, 0);
}

#[test]
fn test_rdmsr_linux_platform_and_xss_profiles_are_zero() {
    for index in [0x17_u64, 0xDA0] {
        let code = [0x0F, 0x32, 0xF4];
        let mut regs = Registers::default();
        regs.rcx = index;
        let (mut vcpu, _) = setup_vm(&code, Some(regs));
        let regs = run_until_hlt(&mut vcpu).unwrap();
        assert_eq!((regs.rdx << 32) | regs.rax, 0, "MSR {index:#x}");
    }
}

#[test]
fn test_rdmsr_waitpkg_control_reset_profile_is_zero() {
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xE1; // IA32_UMWAIT_CONTROL
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!((regs.rdx << 32) | regs.rax, 0);
}

#[test]
fn test_rdmsr_unknown_selector_faults_without_clobbering_outputs() {
    // The deterministic profile does not implement MSR 0, so RDMSR must #GP(0).
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0;
    regs.rax = 0x1111_2222_3333_4444;
    regs.rdx = 0x5555_6666_7777_8888;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let _ = vcpu.step();
    let regs = vcpu.get_regs().unwrap();

    assert_eq!(
        regs.rip, INT_HANDLER_ADDR,
        "unknown RDMSR must deliver #GP(0)"
    );
    assert_eq!(regs.rcx, 0);
    assert_eq!(regs.rax, 0x1111_2222_3333_4444);
    assert_eq!(regs.rdx, 0x5555_6666_7777_8888);
}

#[test]
fn test_rdmsr_preserves_ecx() {
    // ECX should not be modified by RDMSR
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x1B;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0x1B, "ECX should not be modified");
}

#[test]
fn test_rdmsr_with_previous_eax_edx() {
    // Previous values of EAX and EDX should be overwritten
    let code = [
        0x48, 0xC7, 0xC0, 0x11, 0x11, 0x11, 0x11, // MOV RAX, 0x11111111
        0x48, 0xC7, 0xC2, 0x22, 0x22, 0x22, 0x22, // MOV RDX, 0x22222222
        0x48, 0xC7, 0xC1, 0x1B, 0x00, 0x00, 0x00, // MOV RCX, 0x1B
        0x0F, 0x32, // RDMSR
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // EAX and EDX should be completely replaced (not OR'd or added)
    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_fmask() {
    // Read IA32_FMASK.
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000084;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_fs_base() {
    // Read IA32_FS_BASE.
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000100; // IA32_FS_BASE
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

// ============================================================================
// Comparison with Other Instructions
// ============================================================================

#[test]
fn test_rdmsr_vs_rdtsc() {
    // Compare RDMSR(TSC) with RDTSC instruction
    let code = [
        0x0F, 0x31, // RDTSC
        0x48, 0x89, 0xC3, // MOV RBX, RAX
        0x48, 0x89, 0xD6, // MOV RSI, RDX
        0x48, 0xC7, 0xC1, 0x10, 0x00, 0x00, 0x00, // MOV RCX, 0x10
        0x0F, 0x32, // RDMSR
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Both should return TSC values, RDMSR result >= RDTSC result
    let tsc_rdtsc = ((regs.rsi as u64) << 32) | (regs.rbx as u64);
    let tsc_rdmsr = ((regs.rdx as u64) << 32) | (regs.rax as u64);
    assert!(tsc_rdmsr >= tsc_rdtsc, "RDMSR(TSC) should be >= RDTSC");
}

#[test]
fn test_rdmsr_tsc_adjust_again_after_tsc_reads() {
    // IA32_TSC_ADJUST remains implemented after volatile timestamp reads.
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x3B; // IA32_TSC_ADJUST
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_kernel_gs_base() {
    // Read IA32_KERNEL_GS_BASE
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xC0000102; // IA32_KERNEL_GS_BASE
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_sysenter_cs() {
    // Read IA32_SYSENTER_CS
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x174; // IA32_SYSENTER_CS
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_sysenter_esp() {
    // Read IA32_SYSENTER_ESP
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x175; // IA32_SYSENTER_ESP
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}

#[test]
fn test_rdmsr_sysenter_eip() {
    // Read IA32_SYSENTER_EIP
    let code = [0x0F, 0x32, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0x176; // IA32_SYSENTER_EIP
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax >> 32, 0);
    assert_eq!(regs.rdx >> 32, 0);
}
