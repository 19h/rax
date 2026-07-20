use crate::common::{run_until_hlt, setup_vm, setup_vm_no_idt};
use rax::vm::vcpu::{Registers, VCpu};

// MONITOR/MWAIT - Set Up Monitor Address / Monitor Wait
//
// MONITOR sets up a linear address range to be monitored by hardware
// and prepares the processor to enter an optimized state while waiting
// for an event.
//
// MWAIT causes the processor to enter an optimized state while waiting
// for a write to the address range set up by the MONITOR instruction.
//
// Opcodes:
//   0F 01 C8    MONITOR    - Set up monitor address
//   0F 01 C9    MWAIT      - Monitor wait
//
// These instructions require CPL = 0 (kernel mode) on most processors.

const ALIGNED_ADDR: u64 = 0x3000;

fn assert_gp(code: &[u8]) {
    let (mut vcpu, _) = setup_vm_no_idt(code, None);
    let error = run_until_hlt(&mut vcpu).expect_err("reserved RCX extension must raise #GP(0)");
    assert!(
        error.to_string().contains("IDT entry 13 not present"),
        "unexpected MONITOR/MWAIT extension fault: {error:#}"
    );
}

#[test]
fn test_monitor_basic() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_basic() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_rejects_undefined_rcx_extension() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xb9, 0x01, 0x00, 0x00, 0x00, // MOV ECX, 1
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_with_rdx_hints() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0xba, 0x01, 0x00, 0x00, 0x00, // MOV EDX, 1
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_with_c0_hint() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb8, 0xf0, 0x00, 0x00, 0x00, // MOV EAX, 0xF0 (C0 hint)
        0x31, 0xc9, // XOR ECX, ECX
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_with_c1_substate_hint() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb8, 0x03, 0x00, 0x00, 0x00, // MOV EAX, 3 (C1 substate hint)
        0x31, 0xc9, // XOR ECX, ECX
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_rejects_unenumerated_interrupt_break_extension() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x01, 0x00, 0x00, 0x00, // MOV ECX, 1 (interrupt break enabled)
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_different_addresses() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_aligned_address() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&(ALIGNED_ADDR & !0x3f).to_le_bytes()); // 64-byte aligned
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_unaligned_address() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&(ALIGNED_ADDR + 7).to_le_bytes()); // Unaligned
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_sequential() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0x48, 0x05, 0x40, 0x00, 0x00, 0x00, // ADD RAX, 64
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_sequential() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0x0f, 0x01, 0xc9, // MWAIT
        0x0f, 0x01, 0xc8, // MONITOR
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_rbx() {
    let code = [
        0x48, 0xbb, // MOV RBX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x48, 0x89, 0xd8, // MOV RAX, RBX
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_rcx() {
    let code = [
        0x48, 0xb9, // MOV RCX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x48, 0x89, 0xc8, // MOV RAX, RCX
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_rdx() {
    let code = [
        0x48, 0xba, // MOV RDX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x48, 0x89, 0xd0, // MOV RAX, RDX
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_rsi() {
    let code = [
        0x48, 0xbe, // MOV RSI, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x48, 0x89, 0xf0, // MOV RAX, RSI
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_rdi() {
    let code = [
        0x48, 0xbf, // MOV RDI, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x48, 0x89, 0xf8, // MOV RAX, RDI
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_r8() {
    let code = [
        0x49, 0xb8, // MOV R8, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x4c, 0x89, 0xc0, // MOV RAX, R8
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_r9() {
    let code = [
        0x49, 0xb9, // MOV R9, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x4c, 0x89, 0xc8, // MOV RAX, R9
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_with_zero_ecx_extension() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x00, 0x00, 0x00, 0x00, // MOV ECX, 0
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_rejects_reserved_ecx_bit_0() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x01, 0x00, 0x00, 0x00, // MOV ECX, 1
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_mwait_rejects_reserved_ecx_bit_5() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x20, 0x00, 0x00, 0x00, // MOV ECX, 0x20
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_mwait_rejects_multiple_reserved_ecx_bits() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x30, 0x00, 0x00, 0x00, // MOV ECX, 0x30
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_mwait_pattern_1() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x00, 0x00, 0x00, 0x00, // MOV ECX, 0
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_mwait_pattern_rejects_reserved_mwait_extension() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xb9, 0x00, 0x00, 0x00, 0x00, // MOV ECX, 0
        0xba, 0x00, 0x00, 0x00, 0x00, // MOV EDX, 0
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x10, 0x00, 0x00, 0x00, // MOV ECX, 0x10
        0xba, 0x00, 0x00, 0x00, 0x00, // MOV EDX, 0
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_rejects_undefined_extension_with_hint() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xb9, 0x01, 0x00, 0x00, 0x00, // MOV ECX, 1
        0xba, 0x01, 0x00, 0x00, 0x00, // MOV EDX, 1
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_rejects_max_undefined_extension() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xb9, 0xff, 0xff, 0xff, 0xff, // MOV ECX, 0xFFFFFFFF
        0xba, 0xff, 0xff, 0xff, 0xff, // MOV EDX, 0xFFFFFFFF
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_mwait_rejects_max_reserved_extension() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0xff, 0xff, 0xff, 0xff, // MOV ECX, 0xFFFFFFFF
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_null_address() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_high_address() {
    let code = [
        0x48, 0xb8, 0xff, 0xff, 0xff, 0x7f, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0x7FFFFFFF
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let error = run_until_hlt(&mut vcpu).expect_err("MONITOR must perform a faulting byte read");
    assert!(
        error.to_string().contains("0x7fffffff"),
        "unexpected MONITOR address fault: {error:#}"
    );
}

#[test]
fn test_monitor_page_boundary() {
    let code = [
        0x48, 0xb8, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // MOV RAX, 0x1000 (page boundary)
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_cache_line_boundary() {
    let code = [
        0x48, 0xb8, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x00,
        0x00, // MOV RAX, 0x300000 (64-byte aligned)
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_loop() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0x0f, 0x01, 0xc9, // MWAIT
        0x0f, 0x01, 0xc8, // MONITOR
        0x0f, 0x01, 0xc9, // MWAIT
        0x0f, 0x01, 0xc8, // MONITOR
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_different_registers_sequence() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0x48, 0xbb, // MOV RBX, imm64
    ]);
    full_code.extend_from_slice(&(ALIGNED_ADDR + 0x40).to_le_bytes());
    full_code.extend_from_slice(&[
        0x48, 0x89, 0xd8, // MOV RAX, RBX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_sequence_rejects_later_reserved_extension() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x00, 0x00, 0x00, 0x00, // MOV ECX, 0
        0x0f, 0x01, 0xc9, // MWAIT
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x10, 0x00, 0x00, 0x00, // MOV ECX, 0x10
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_mwait_comprehensive() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xb9, 0x00, 0x00, 0x00, 0x00, // MOV ECX, 0
        0xba, 0x00, 0x00, 0x00, 0x00, // MOV EDX, 0
        0x0f, 0x01, 0xc8, // MONITOR
        0x31, 0xc9, // XOR ECX, ECX
        0xba, 0x00, 0x00, 0x00, 0x00, // MOV EDX, 0
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_monitor_with_various_ecx() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0xb9, 0x03, 0x00, 0x00, 0x00, // MOV ECX, 3
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_with_various_edx() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0xba, 0x03, 0x00, 0x00, 0x00, // MOV EDX, 3
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_with_various_ecx() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb9, 0x03, 0x00, 0x00, 0x00, // MOV ECX, 3
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    assert_gp(&full_code);
}

#[test]
fn test_monitor_address_offset_pattern() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0x48, 0x83, 0xc0, 0x10, // ADD RAX, 16
        0x0f, 0x01, 0xc8, // MONITOR
        0x48, 0x83, 0xc0, 0x10, // ADD RAX, 16
        0x0f, 0x01, 0xc8, // MONITOR
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mwait_state_transition_pattern() {
    let code = [
        0x48, 0xb8, // MOV RAX, imm64
    ];
    let mut full_code = code.to_vec();
    full_code.extend_from_slice(&ALIGNED_ADDR.to_le_bytes());
    full_code.extend_from_slice(&[
        0x31, 0xc9, // XOR ECX, ECX
        0x31, 0xd2, // XOR EDX, EDX
        0x0f, 0x01, 0xc8, // MONITOR
        0xb8, 0xf0, 0x00, 0x00, 0x00, // MOV EAX, 0xF0 (C0 hint)
        0x31, 0xc9, // XOR ECX, ECX
        0x0f, 0x01, 0xc9, // MWAIT
        0x48, 0xc7, 0xc0, 0x00, 0x30, 0x00, 0x00, // MOV RAX, ALIGNED_ADDR
        0x0f, 0x01, 0xc8, // MONITOR
        0x31, 0xc0, // XOR EAX, EAX (C1 hint)
        0x31, 0xc9, // XOR ECX, ECX
        0x0f, 0x01, 0xc9, // MWAIT
        0x48, 0xc7, 0xc0, 0x00, 0x30, 0x00, 0x00, // MOV RAX, ALIGNED_ADDR
        0x0f, 0x01, 0xc8, // MONITOR
        0xb8, 0x10, 0x00, 0x00, 0x00, // MOV EAX, 0x10 (C2 hint)
        0x31, 0xc9, // XOR ECX, ECX
        0x0f, 0x01, 0xc9, // MWAIT
        0xf4, // HLT
    ]);
    let (mut vcpu, _) = setup_vm(&full_code, None);
    run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn monitor_cpl_fault_precedes_reserved_rcx_extension() {
    let code = [0x0F, 0x01, 0xC8, 0xF4];
    let mut regs = Registers::default();
    regs.rax = ALIGNED_ADDR;
    regs.rcx = 1;
    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(regs));
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = (sregs.cs.selector & !3) | 3;
    vcpu.set_sregs(&sregs).unwrap();

    let error = vcpu
        .step()
        .expect_err("CPL != 0 MONITOR must raise #UD before checking RCX");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "unexpected MONITOR privilege fault: {error:#}"
    );
}

#[test]
fn monitor_noncanonical_linear_address_raises_gp() {
    let code = [0x0F, 0x01, 0xC8, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0x0000_8000_0000_0000;
    regs.rcx = 0;
    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(regs));

    let error =
        run_until_hlt(&mut vcpu).expect_err("noncanonical MONITOR address must raise #GP(0)");
    assert!(
        error.to_string().contains("IDT entry 13 not present"),
        "unexpected MONITOR canonicality fault: {error:#}"
    );
}

#[test]
fn monitor_noncanonical_ss_address_raises_ss() {
    let code = [0x36, 0x0F, 0x01, 0xC8, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0x0000_8000_0000_0000;
    regs.rcx = 0;
    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(regs));

    let error = run_until_hlt(&mut vcpu).expect_err("noncanonical SS:MONITOR must raise #SS(0)");
    assert!(
        error.to_string().contains("IDT entry 12 not present"),
        "unexpected SS:MONITOR canonicality fault: {error:#}"
    );
}

#[test]
fn monitor_addr32_ignores_non_fs_gs_segment_bases_in_long_mode() {
    // DS override + address-size override + MONITOR; HLT.
    let code = [0x3E, 0x67, 0x0F, 0x01, 0xC8, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0xFFFF_FFFF_0000_3000;
    regs.rcx = 0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.ds.base = 0x0200_0000;
    vcpu.set_sregs(&sregs).unwrap();

    let after = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(after.rax, 0xFFFF_FFFF_0000_3000);
}

#[test]
fn monitor_fs_override_contributes_the_long_mode_segment_base() {
    // FS override + MONITOR; HLT. The unsegmented RAX address is outside the
    // 16 MiB fixture, while FS.base + RAX wraps to the mapped address 0x3000.
    let code = [0x64, 0x0F, 0x01, 0xC8, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0x0200_0000;
    regs.rcx = 0;
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.fs.base = 0xFFFF_FFFF_FE00_3000;
    vcpu.set_sregs(&sregs).unwrap();

    let after = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(after.rax, 0x0200_0000);
}
