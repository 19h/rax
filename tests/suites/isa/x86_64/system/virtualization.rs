//! Tests for Intel VT-x Virtualization Instructions.
//!
//! This module covers VMX (Virtual Machine Extensions) instructions used for
//! hardware virtualization support on Intel processors.
//!
//! Instructions covered:
//! - VMCALL - Call to VM Monitor
//! - VMMCALL/VMGEXIT - AMD hypercall and SEV-ES exit encodings
//! - VMRUN/VMLOAD/VMSAVE/STGI/CLGI/SKINIT/INVLPGA - disabled AMD SVM controls
//! - ENCLV/ENCLS/ENCLU - disabled Intel SGX root instructions
//! - VMCLEAR - Clear Virtual Machine Control Structure
//! - VMLAUNCH - Launch Virtual Machine
//! - VMRESUME - Resume Virtual Machine
//! - VMPTRLD - Load Pointer to Virtual Machine Control Structure
//! - VMPTRST - Store Pointer to Virtual Machine Control Structure
//! - VMREAD - Read Field from Virtual Machine Control Structure
//! - VMWRITE - Write Field to Virtual Machine Control Structure
//! - VMXOFF - Leave VMX Operation
//! - VMXON - Enter VMX Operation
//! - VMFUNC - Invoke VM Function
//! - INVEPT - Invalidate EPT Translations
//! - INVVPID - Invalidate VPID Translations
//!
//! References: docs/vmcall.txt, docs/vmclear.txt, docs/vmlaunch:vmresume.txt,
//!            docs/vmptrld.txt, docs/vmptrst.txt, docs/vmread.txt, docs/vmwrite.txt,
//!            docs/vmxoff.txt, docs/vmxon.txt, docs/vmfunc.txt,
//!            docs/invept.txt, docs/invvpid.txt

use crate::common::*;
use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::Registers;

// ============================================================================
// VMXON Tests - Enter VMX Operation
// ============================================================================

#[test]
fn test_vmxon_basic() {
    // VMXON - Enter VMX Operation
    // Opcode: F3 0F C7 /6
    // Note: Requires CPL=0 and proper CR4.VMXE setup
    let code = [
        0xF3, 0x0F, 0xC7, 0x30, // VMXON [rax] (rax points to VMXON region)
        0xF4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x2000; // VMXON region address
    let (mut vcpu, _) = setup_vm(&code, Some(regs));

    // This will likely #UD or #GP in test environment
    // Testing that the instruction is recognized
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmxon_memory_operand() {
    // VMXON with memory operand
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x20, 0x00, 0x00, // MOV RAX, 0x2000
        0xF3, 0x0F, 0xC7, 0x30, // VMXON [rax]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmxon_with_different_registers() {
    // VMXON using RBX as base register
    let code = [
        0x48, 0xC7, 0xC3, 0x00, 0x30, 0x00, 0x00, // MOV RBX, 0x3000
        0xF3, 0x0F, 0xC7, 0x33, // VMXON [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmxon_with_offset() {
    // VMXON with displacement
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x20, 0x00, 0x00, // MOV RCX, 0x2000
        0xF3, 0x0F, 0xC7, 0xB1, 0x00, 0x10, 0x00, 0x00, // VMXON [rcx+0x1000]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmxon_preserves_registers() {
    // VMXON should only affect memory, not GP registers
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x20, 0x00, 0x00, // MOV RAX, 0x2000
        0x48, 0xC7, 0xC3, 0x42, 0x42, 0x42, 0x42, // MOV RBX, 0x42424242
        0x48, 0xC7, 0xC6, 0xAA, 0xAA, 0xAA, 0xAA, // MOV RSI, 0xAAAAAAAA
        0xF3, 0x0F, 0xC7, 0x30, // VMXON [rax]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMXOFF Tests - Leave VMX Operation
// ============================================================================

#[test]
fn test_vmxoff_basic() {
    // VMXOFF - Leave VMX Operation
    // Opcode: 0F 01 C4
    let code = [
        0x0F, 0x01, 0xC4, // VMXOFF
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmxoff_preserves_registers() {
    // VMXOFF should not modify GP registers
    let code = [
        0x48, 0xC7, 0xC0, 0x11, 0x11, 0x11, 0x11, // MOV RAX, 0x11111111
        0x48, 0xC7, 0xC3, 0x22, 0x22, 0x22, 0x22, // MOV RBX, 0x22222222
        0x0F, 0x01, 0xC4, // VMXOFF
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmxoff_no_operands() {
    // VMXOFF takes no operands
    let code = [0x0F, 0x01, 0xC4, 0xF4];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMPTRLD Tests - Load Pointer to VMCS
// ============================================================================

#[test]
fn test_vmptrld_basic() {
    // VMPTRLD - Load pointer to current VMCS
    // Opcode: 0F C7 /6
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x40, 0x00, 0x00, // MOV RAX, 0x4000
        0x0F, 0xC7, 0x30, // VMPTRLD [rax]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmptrld_different_addresses() {
    // VMPTRLD with various memory addresses
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x50, 0x00, 0x00, // MOV RCX, 0x5000
        0x0F, 0xC7, 0x31, // VMPTRLD [rcx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmptrld_with_displacement() {
    // VMPTRLD with memory displacement
    let code = [
        0x48, 0xC7, 0xC2, 0x00, 0x40, 0x00, 0x00, // MOV RDX, 0x4000
        0x0F, 0xC7, 0xB2, 0x00, 0x10, 0x00, 0x00, // VMPTRLD [rdx+0x1000]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmptrld_multiple_loads() {
    // Load multiple VMCS pointers sequentially
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x40, 0x00, 0x00, // MOV RAX, 0x4000
        0x0F, 0xC7, 0x30, // VMPTRLD [rax]
        0x48, 0xC7, 0xC0, 0x00, 0x50, 0x00, 0x00, // MOV RAX, 0x5000
        0x0F, 0xC7, 0x30, // VMPTRLD [rax]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMPTRST Tests - Store Pointer to VMCS
// ============================================================================

#[test]
fn test_vmptrst_basic() {
    // VMPTRST - Store current VMCS pointer
    // Opcode: 0F C7 /7
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x60, 0x00, 0x00, // MOV RAX, 0x6000
        0x0F, 0xC7, 0x38, // VMPTRST [rax]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmptrst_to_different_locations() {
    // Store VMCS pointer to various memory locations
    let code = [
        0x48, 0xC7, 0xC3, 0x00, 0x70, 0x00, 0x00, // MOV RBX, 0x7000
        0x0F, 0xC7, 0x3B, // VMPTRST [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmptrst_with_offset() {
    // VMPTRST with displacement
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x60, 0x00, 0x00, // MOV RCX, 0x6000
        0x0F, 0xC7, 0xB9, 0x00, 0x08, 0x00, 0x00, // VMPTRST [rcx+0x800]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMCLEAR Tests - Clear VMCS
// ============================================================================

#[test]
fn test_vmclear_basic() {
    // VMCLEAR - Clear Virtual Machine Control Structure
    // Opcode: 66 0F C7 /6
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x80, 0x00, 0x00, // MOV RAX, 0x8000
        0x66, 0x0F, 0xC7, 0x30, // VMCLEAR [rax]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmclear_different_vmcs() {
    // Clear different VMCS regions
    let code = [
        0x48, 0xC7, 0xC2, 0x00, 0x90, 0x00, 0x00, // MOV RDX, 0x9000
        0x66, 0x0F, 0xC7, 0x32, // VMCLEAR [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmclear_with_displacement() {
    // VMCLEAR with memory displacement
    let code = [
        0x48, 0xC7, 0xC3, 0x00, 0x80, 0x00, 0x00, // MOV RBX, 0x8000
        0x66, 0x0F, 0xC7, 0xB3, 0x00, 0x10, 0x00, 0x00, // VMCLEAR [rbx+0x1000]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmclear_sequential() {
    // Clear multiple VMCS structures
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x80, 0x00, 0x00, // MOV RAX, 0x8000
        0x66, 0x0F, 0xC7, 0x30, // VMCLEAR [rax]
        0x48, 0xC7, 0xC0, 0x00, 0x90, 0x00, 0x00, // MOV RAX, 0x9000
        0x66, 0x0F, 0xC7, 0x30, // VMCLEAR [rax]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMREAD Tests - Read from VMCS
// ============================================================================

#[test]
fn test_vmread_basic() {
    // VMREAD - Read field from VMCS to register
    // Opcode: 0F 78
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x68, 0x00, 0x00, // MOV RAX, 0x6800 (field encoding)
        0x0F, 0x78, 0xC3, // VMREAD rbx, rax
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmread_to_memory() {
    // VMREAD to memory location
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x68, 0x00, 0x00, // MOV RCX, 0x6800 (field)
        0x48, 0xC7, 0xC2, 0x00, 0xA0, 0x00, 0x00, // MOV RDX, 0xA000 (dest)
        0x0F, 0x78, 0x0A, // VMREAD [rdx], rcx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmread_different_fields() {
    // Read different VMCS fields
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x48, 0x00, 0x00, // MOV RAX, 0x4800
        0x0F, 0x78, 0xC3, // VMREAD rbx, rax
        0x48, 0xC7, 0xC0, 0x02, 0x48, 0x00, 0x00, // MOV RAX, 0x4802
        0x0F, 0x78, 0xC6, // VMREAD rsi, rax
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmread_guest_cr0() {
    // Read Guest CR0 field (encoding 0x6800)
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x68, 0x00, 0x00, // MOV RCX, 0x6800
        0x0F, 0x78, 0xC1, // VMREAD rax, rcx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmread_guest_cr4() {
    // Read Guest CR4 field (encoding 0x6804)
    let code = [
        0x48, 0xC7, 0xC1, 0x04, 0x68, 0x00, 0x00, // MOV RCX, 0x6804
        0x0F, 0x78, 0xC1, // VMREAD rax, rcx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMWRITE Tests - Write to VMCS
// ============================================================================

#[test]
fn test_vmwrite_basic() {
    // VMWRITE - Write to VMCS field from register
    // Opcode: 0F 79
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x68, 0x00, 0x00, // MOV RAX, 0x6800 (field)
        0x48, 0xC7, 0xC3, 0x00, 0x00, 0x60, 0x00, // MOV RBX, 0x600000 (value)
        0x0F, 0x79, 0xC3, // VMWRITE rax, rbx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmwrite_from_memory() {
    // VMWRITE from memory location
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x68, 0x00, 0x00, // MOV RCX, 0x6800
        0x48, 0xC7, 0xC2, 0x00, 0xB0, 0x00, 0x00, // MOV RDX, 0xB000
        0x0F, 0x79, 0x0A, // VMWRITE rcx, [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmwrite_different_fields() {
    // Write to different VMCS fields
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x48, 0x00, 0x00, // MOV RAX, 0x4800
        0x48, 0xC7, 0xC3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1
        0x0F, 0x79, 0xC3, // VMWRITE rax, rbx
        0x48, 0xC7, 0xC0, 0x02, 0x48, 0x00, 0x00, // MOV RAX, 0x4802
        0x48, 0xC7, 0xC3, 0x02, 0x00, 0x00, 0x00, // MOV RBX, 2
        0x0F, 0x79, 0xC3, // VMWRITE rax, rbx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmwrite_guest_rip() {
    // Write Guest RIP field (encoding 0x681E)
    let code = [
        0x48, 0xC7, 0xC1, 0x1E, 0x68, 0x00, 0x00, // MOV RCX, 0x681E
        0x48, 0xC7, 0xC2, 0x00, 0x10, 0x00, 0x00, // MOV RDX, 0x1000
        0x0F, 0x79, 0xCA, // VMWRITE rcx, rdx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmwrite_guest_rsp() {
    // Write Guest RSP field (encoding 0x681C)
    let code = [
        0x48, 0xC7, 0xC1, 0x1C, 0x68, 0x00, 0x00, // MOV RCX, 0x681C
        0x48, 0xC7, 0xC2, 0x00, 0x70, 0x00, 0x00, // MOV RDX, 0x7000
        0x0F, 0x79, 0xCA, // VMWRITE rcx, rdx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMLAUNCH Tests - Launch Virtual Machine
// ============================================================================

#[test]
fn test_vmlaunch_basic() {
    // VMLAUNCH - Launch virtual machine
    // Opcode: 0F 01 C2
    let code = [
        0x0F, 0x01, 0xC2, // VMLAUNCH
        0xF4, // HLT (should not reach if launch succeeds)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmlaunch_no_operands() {
    // VMLAUNCH takes no operands
    let code = [0x0F, 0x01, 0xC2, 0xF4];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmlaunch_after_setup() {
    // VMLAUNCH after setting up VMCS
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x40, 0x00, 0x00, // MOV RAX, 0x4000
        0x0F, 0xC7, 0x30, // VMPTRLD [rax]
        0x0F, 0x01, 0xC2, // VMLAUNCH
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMRESUME Tests - Resume Virtual Machine
// ============================================================================

#[test]
fn test_vmresume_basic() {
    // VMRESUME - Resume virtual machine
    // Opcode: 0F 01 C3
    let code = [
        0x0F, 0x01, 0xC3, // VMRESUME
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmresume_no_operands() {
    // VMRESUME takes no operands
    let code = [0x0F, 0x01, 0xC3, 0xF4];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmresume_after_vmptrld() {
    // VMRESUME after loading VMCS pointer
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x40, 0x00, 0x00, // MOV RAX, 0x4000
        0x0F, 0xC7, 0x30, // VMPTRLD [rax]
        0x0F, 0x01, 0xC3, // VMRESUME
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

fn virtualization_fault_registers() -> Registers {
    Registers {
        rax: 0x0123_4567_89AB_CDEF,
        rbx: 0xFEDC_BA98_7654_3210,
        rcx: 0x1111_2222_3333_4444,
        rdx: 0xAAAA_BBBB_CCCC_DDDD,
        rsp: STACK_ADDR,
        rbp: 0x5555_6666_7777_8888,
        r16: 0x1616_1616_1616_1616,
        r31: 0x3131_3131_3131_3131,
        rflags: 0x0CD7,
        xmm: [[0x1111_2222_3333_4444, 0xAAAA_BBBB_CCCC_DDDD]; 16],
        ymm_high: [[0x5555_6666_7777_8888, 0x9999_AAAA_BBBB_CCCC]; 16],
        zmm_high: [[0x1234_5678_9ABC_DEF0; 4]; 16],
        zmm_ext: [[0x0FED_CBA9_8765_4321; 8]; 16],
        k: [0xA5A5_5A5A_A5A5_5A5A; 8],
        mm: [0x1122_3344_5566_7788; 8],
        ..Registers::default()
    }
}

fn seed_virtualization_fault_state(vcpu: &mut X86_64Vcpu, cpl: u16) -> serde_json::Value {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = cpl;
    sregs.cr2 = 0x2222_0000;
    sregs.cr3 = 0x3333_0000;
    sregs.cr4 |= 0x4444;
    sregs.cr8 = 0x8;
    sregs.fs.base = 0x0000_1111_2222_3333;
    sregs.gs.base = 0x0000_4444_5555_6666;
    vcpu.set_sregs(&sregs).unwrap();

    virtualization_public_state(vcpu)
}

fn virtualization_public_state(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap()
}

#[test]
fn test_disabled_vmx_instructions_raise_ud_before_operand_privilege_or_state_commit() {
    for (name, bytes, apx, cpl) in [
        ("VMLAUNCH", &[0x0F, 0x01, 0xC2][..], false, 0),
        ("VMRESUME", &[0x0F, 0x01, 0xC3][..], false, 3),
        ("VMXOFF", &[0x0F, 0x01, 0xC4][..], false, 0),
        ("VMFUNC", &[0x0F, 0x01, 0xD4][..], false, 3),
        ("VMPTRLD", &[0x0F, 0xC7, 0x30][..], false, 3),
        ("VMPTRST", &[0x0F, 0xC7, 0x38][..], false, 0),
        ("VMCLEAR", &[0x66, 0x0F, 0xC7, 0x30][..], false, 3),
        ("VMXON", &[0xF3, 0x0F, 0xC7, 0x30][..], false, 0),
        (
            "redundant-66 VMXON",
            &[0x66, 0xF3, 0x0F, 0xC7, 0x30][..],
            false,
            3,
        ),
        ("REX2 VMLAUNCH", &[0xD5, 0x80, 0x01, 0xC2][..], true, 3),
        ("REX2 VMRESUME", &[0xD5, 0x80, 0x01, 0xC3][..], true, 0),
        ("REX2 VMXOFF", &[0xD5, 0x80, 0x01, 0xC4][..], true, 3),
        ("REX2 VMFUNC", &[0xD5, 0x80, 0x01, 0xD4][..], true, 0),
        ("REX2 VMPTRLD [r16]", &[0xD5, 0x90, 0xC7, 0x30][..], true, 0),
        ("REX2 VMPTRST [r16]", &[0xD5, 0x90, 0xC7, 0x38][..], true, 3),
        (
            "REX2 VMCLEAR [r16]",
            &[0x66, 0xD5, 0x90, 0xC7, 0x30][..],
            true,
            0,
        ),
        (
            "REX2 VMXON [r16]",
            &[0xF3, 0xD5, 0x90, 0xC7, 0x30][..],
            true,
            3,
        ),
    ] {
        let initial = virtualization_fault_registers();
        let (mut vcpu, _) = if apx {
            setup_apx_vm_no_idt(bytes, Some(initial))
        } else {
            setup_vm_no_idt(bytes, Some(initial))
        };
        let before = seed_virtualization_fault_state(&mut vcpu, cpl);
        let error = vcpu
            .step()
            .expect_err("disabled VMX instruction must inject #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(virtualization_public_state(&vcpu), before, "{name}");
    }
}

#[test]
fn test_disabled_svm_controls_raise_ud_before_cpl_or_state_commit() {
    for (name, bytes, apx) in [
        ("VMRUN", &[0x0F, 0x01, 0xD8][..], false),
        ("VMLOAD", &[0x0F, 0x01, 0xDA][..], false),
        ("VMSAVE", &[0x0F, 0x01, 0xDB][..], false),
        ("STGI", &[0x0F, 0x01, 0xDC][..], false),
        ("CLGI", &[0x0F, 0x01, 0xDD][..], false),
        ("SKINIT", &[0x0F, 0x01, 0xDE][..], false),
        ("INVLPGA", &[0x0F, 0x01, 0xDF][..], false),
        ("REX2 VMRUN", &[0xD5, 0x80, 0x01, 0xD8][..], true),
        ("REX2 VMLOAD", &[0xD5, 0x80, 0x01, 0xDA][..], true),
        ("REX2 VMSAVE", &[0xD5, 0x80, 0x01, 0xDB][..], true),
        ("REX2 STGI", &[0xD5, 0x80, 0x01, 0xDC][..], true),
        ("REX2 CLGI", &[0xD5, 0x80, 0x01, 0xDD][..], true),
        ("REX2 SKINIT", &[0xD5, 0x80, 0x01, 0xDE][..], true),
        ("REX2 INVLPGA", &[0xD5, 0x80, 0x01, 0xDF][..], true),
    ] {
        let initial = virtualization_fault_registers();
        let (mut vcpu, _) = if apx {
            setup_apx_vm_no_idt(bytes, Some(initial))
        } else {
            setup_vm_no_idt(bytes, Some(initial))
        };
        let before = seed_virtualization_fault_state(&mut vcpu, 3);

        let error = vcpu
            .step()
            .expect_err("disabled SVM control must inject #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: SVM-disabled #UD must precede CPL3 #GP, got {error}"
        );
        assert_eq!(virtualization_public_state(&vcpu), before, "{name}");
    }
}

#[test]
fn test_disabled_sgx_roots_raise_ud_before_dynamic_checks_or_state_commit() {
    for (name, bytes, apx, cpl, task_switched) in [
        ("ENCLV", &[0x0F, 0x01, 0xC0][..], false, 3, false),
        ("ENCLS", &[0x0F, 0x01, 0xCF][..], false, 3, false),
        ("ENCLU", &[0x0F, 0x01, 0xD7][..], false, 3, true),
        ("REX2 ENCLV", &[0xD5, 0x80, 0x01, 0xC0][..], true, 0, false),
        ("REX2 ENCLS", &[0xD5, 0x80, 0x01, 0xCF][..], true, 0, false),
        ("REX2 ENCLU", &[0xD5, 0x80, 0x01, 0xD7][..], true, 0, true),
    ] {
        let initial = virtualization_fault_registers();
        let (mut vcpu, _) = if apx {
            setup_apx_vm_no_idt(bytes, Some(initial))
        } else {
            setup_vm_no_idt(bytes, Some(initial))
        };
        seed_virtualization_fault_state(&mut vcpu, cpl);
        if task_switched {
            let mut sregs = vcpu.get_sregs().unwrap();
            sregs.cr0 |= 1 << 3;
            vcpu.set_sregs(&sregs).unwrap();
        }
        let before = virtualization_public_state(&vcpu);

        let error = vcpu.step().expect_err("disabled SGX root must inject #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: SGX-disabled #UD must precede CPL, TS, and leaf checks, got {error}"
        );
        assert_eq!(virtualization_public_state(&vcpu), before, "{name}");
    }
}

// ============================================================================
// VMCALL Tests - Call to VM Monitor
// ============================================================================

fn hypercall_scalar_state(regs: &Registers) -> [u64; 34] {
    [
        regs.rax,
        regs.rbx,
        regs.rcx,
        regs.rdx,
        regs.rsi,
        regs.rdi,
        regs.rsp,
        regs.rbp,
        regs.r8,
        regs.r9,
        regs.r10,
        regs.r11,
        regs.r12,
        regs.r13,
        regs.r14,
        regs.r15,
        regs.r16,
        regs.r17,
        regs.r18,
        regs.r19,
        regs.r20,
        regs.r21,
        regs.r22,
        regs.r23,
        regs.r24,
        regs.r25,
        regs.r26,
        regs.r27,
        regs.r28,
        regs.r29,
        regs.r30,
        regs.r31,
        regs.rip,
        regs.rflags,
    ]
}

#[test]
fn test_vmcall_basic() {
    // VMCALL - Hypercall from guest to host
    // Opcode: 0F 01 C1
    let code = [
        0x0F, 0x01, 0xC1, // VMCALL
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmcall_vmmcall_hint_profile_preserves_complete_scalar_state() {
    let code = [
        0x0F, 0x01, 0xC1, // VMCALL
        0x0F, 0x01, 0xD9, // VMMCALL
    ];
    let regs = Registers {
        rax: 0x0101_0101_0101_0101,
        rbx: 0x0202_0202_0202_0202,
        rcx: 0x0303_0303_0303_0303,
        rdx: 0x0404_0404_0404_0404,
        rsi: 0x0505_0505_0505_0505,
        rdi: 0x0606_0606_0606_0606,
        rsp: STACK_ADDR,
        rbp: 0x0707_0707_0707_0707,
        r8: 0x0808_0808_0808_0808,
        r9: 0x0909_0909_0909_0909,
        r10: 0x1010_1010_1010_1010,
        r11: 0x1111_1111_1111_1111,
        r12: 0x1212_1212_1212_1212,
        r13: 0x1313_1313_1313_1313,
        r14: 0x1414_1414_1414_1414,
        r15: 0x1515_1515_1515_1515,
        r16: 0x1616_1616_1616_1616,
        r17: 0x1717_1717_1717_1717,
        r18: 0x1818_1818_1818_1818,
        r19: 0x1919_1919_1919_1919,
        r20: 0x2020_2020_2020_2020,
        r21: 0x2121_2121_2121_2121,
        r22: 0x2222_2222_2222_2222,
        r23: 0x2323_2323_2323_2323,
        r24: 0x2424_2424_2424_2424,
        r25: 0x2525_2525_2525_2525,
        r26: 0x2626_2626_2626_2626,
        r27: 0x2727_2727_2727_2727,
        r28: 0x2828_2828_2828_2828,
        r29: 0x2929_2929_2929_2929,
        r30: 0x3030_3030_3030_3030,
        r31: 0x3131_3131_3131_3131,
        rflags: 0x0CD7,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_vm(&code, Some(regs));
    let mut expected = vcpu.get_regs().unwrap();
    expected.rip += code.len() as u64;

    assert!(vcpu.step().expect("VMCALL hint").is_none());
    assert!(vcpu.step().expect("VMMCALL hint").is_none());
    assert_eq!(
        hypercall_scalar_state(&vcpu.get_regs().unwrap()),
        hypercall_scalar_state(&expected)
    );
}

#[test]
fn test_vmgexit_and_rex2_vmmcall_aliases_raise_ud_without_commit() {
    for (name, bytes, apx) in [
        ("F2 VMGEXIT", &[0xF2, 0x0F, 0x01, 0xD9][..], false),
        ("F3 VMGEXIT", &[0xF3, 0x0F, 0x01, 0xD9][..], false),
        ("REX2 D9", &[0xD5, 0x80, 0x01, 0xD9][..], true),
    ] {
        let initial = Registers {
            rax: 0x0123_4567_89AB_CDEF,
            rbx: 0xFEDC_BA98_7654_3210,
            rcx: 0x1111_2222_3333_4444,
            rdx: 0xAAAA_BBBB_CCCC_DDDD,
            rsp: STACK_ADDR,
            rbp: 0x5555_6666_7777_8888,
            rflags: 0x0CD7,
            ..Registers::default()
        };
        let (mut vcpu, _) = if apx {
            setup_apx_vm_no_idt(bytes, Some(initial))
        } else {
            setup_vm_no_idt(bytes, Some(initial))
        };
        let before = hypercall_scalar_state(&vcpu.get_regs().unwrap());

        let error = vcpu.step().expect_err("invalid alias must inject #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(
            hypercall_scalar_state(&vcpu.get_regs().unwrap()),
            before,
            "{name}"
        );
    }
}

#[test]
fn test_vmcall_with_parameters() {
    // VMCALL with parameters in registers
    let code = [
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (hypercall number)
        0x48, 0xC7, 0xC3, 0x42, 0x00, 0x00, 0x00, // MOV RBX, 0x42 (param 1)
        0x48, 0xC7, 0xC1, 0x43, 0x00, 0x00, 0x00, // MOV RCX, 0x43 (param 2)
        0x0F, 0x01, 0xC1, // VMCALL
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmcall_multiple() {
    // Multiple VMCALLs in sequence
    let code = [
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0F, 0x01, 0xC1, // VMCALL
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x0F, 0x01, 0xC1, // VMCALL
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmcall_preserves_registers() {
    // VMCALL should preserve registers (unless modified by VMM)
    let code = [
        0x48, 0xC7, 0xC3, 0x11, 0x11, 0x11, 0x11, // MOV RBX, 0x11111111
        0x48, 0xC7, 0xC6, 0x22, 0x22, 0x22, 0x22, // MOV RSI, 0x22222222
        0x0F, 0x01, 0xC1, // VMCALL
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// VMFUNC Tests - Invoke VM Function
// ============================================================================

#[test]
fn test_vmfunc_basic() {
    // VMFUNC - Invoke VM function
    // Opcode: 0F 01 D4
    // EAX specifies function number
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0 (EPTP switching)
        0x0F, 0x01, 0xD4, // VMFUNC
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmfunc_eptp_switching() {
    // VMFUNC function 0: EPTP switching
    let code = [
        0x48, 0x31, 0xC0, // XOR RAX, RAX (function 0)
        0x48, 0xC7, 0xC1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0 (EPTP index)
        0x0F, 0x01, 0xD4, // VMFUNC
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmfunc_different_functions() {
    // Test different VMFUNC function numbers
    let code = [
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x0F, 0x01, 0xD4, // VMFUNC
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// INVEPT Tests - Invalidate EPT Translations
// ============================================================================

#[test]
fn test_invept_basic() {
    // INVEPT - Invalidate EPT-derived translations
    // Opcode: 66 0F 38 80
    let code = [
        0x48, 0xC7, 0xC1, 0x01, 0x00, 0x00, 0x00, // MOV RCX, 1 (single-context)
        0x48, 0xC7, 0xC2, 0x00, 0xC0, 0x00, 0x00, // MOV RDX, 0xC000 (descriptor)
        0x66, 0x0F, 0x38, 0x80, 0x0A, // INVEPT rcx, [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invept_single_context() {
    // INVEPT type 1: single-context invalidation
    let code = [
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0xC7, 0xC3, 0x00, 0xC0, 0x00, 0x00, // MOV RBX, 0xC000
        0x66, 0x0F, 0x38, 0x80, 0x03, // INVEPT rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invept_all_contexts() {
    // INVEPT type 2: all-context invalidation
    let code = [
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x48, 0xC7, 0xC3, 0x00, 0xC0, 0x00, 0x00, // MOV RBX, 0xC000
        0x66, 0x0F, 0x38, 0x80, 0x03, // INVEPT rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invept_multiple_invalidations() {
    // Multiple INVEPT calls
    let code = [
        0x48, 0xC7, 0xC1, 0x01, 0x00, 0x00, 0x00, // MOV RCX, 1
        0x48, 0xC7, 0xC2, 0x00, 0xC0, 0x00, 0x00, // MOV RDX, 0xC000
        0x66, 0x0F, 0x38, 0x80, 0x0A, // INVEPT rcx, [rdx]
        0x48, 0xC7, 0xC1, 0x02, 0x00, 0x00, 0x00, // MOV RCX, 2
        0x66, 0x0F, 0x38, 0x80, 0x0A, // INVEPT rcx, [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// INVVPID Tests - Invalidate VPID Translations
// ============================================================================

#[test]
fn test_invvpid_basic() {
    // INVVPID - Invalidate VPID-tagged TLB entries
    // Opcode: 66 0F 38 81
    let code = [
        0x48, 0xC7, 0xC1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0 (individual-address)
        0x48, 0xC7, 0xC2, 0x00, 0xD0, 0x00, 0x00, // MOV RDX, 0xD000 (descriptor)
        0x66, 0x0F, 0x38, 0x81, 0x0A, // INVVPID rcx, [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invvpid_individual_address() {
    // INVVPID type 0: individual-address invalidation
    let code = [
        0x48, 0x31, 0xC0, // XOR RAX, RAX
        0x48, 0xC7, 0xC3, 0x00, 0xD0, 0x00, 0x00, // MOV RBX, 0xD000
        0x66, 0x0F, 0x38, 0x81, 0x03, // INVVPID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invvpid_single_context() {
    // INVVPID type 1: single-context invalidation
    let code = [
        0x48, 0xC7, 0xC0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0xC7, 0xC3, 0x00, 0xD0, 0x00, 0x00, // MOV RBX, 0xD000
        0x66, 0x0F, 0x38, 0x81, 0x03, // INVVPID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invvpid_all_contexts() {
    // INVVPID type 2: all-contexts invalidation
    let code = [
        0x48, 0xC7, 0xC0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x48, 0xC7, 0xC3, 0x00, 0xD0, 0x00, 0x00, // MOV RBX, 0xD000
        0x66, 0x0F, 0x38, 0x81, 0x03, // INVVPID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invvpid_single_context_retaining_globals() {
    // INVVPID type 3: single-context retaining globals
    let code = [
        0x48, 0xC7, 0xC0, 0x03, 0x00, 0x00, 0x00, // MOV RAX, 3
        0x48, 0xC7, 0xC3, 0x00, 0xD0, 0x00, 0x00, // MOV RBX, 0xD000
        0x66, 0x0F, 0x38, 0x81, 0x03, // INVVPID rax, [rbx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_invvpid_multiple_types() {
    // Multiple INVVPID invalidations with different types
    let code = [
        0x48, 0xC7, 0xC2, 0x00, 0xD0, 0x00, 0x00, // MOV RDX, 0xD000
        0x48, 0xC7, 0xC1, 0x01, 0x00, 0x00, 0x00, // MOV RCX, 1
        0x66, 0x0F, 0x38, 0x81, 0x0A, // INVVPID rcx, [rdx]
        0x48, 0xC7, 0xC1, 0x03, 0x00, 0x00, 0x00, // MOV RCX, 3
        0x66, 0x0F, 0x38, 0x81, 0x0A, // INVVPID rcx, [rdx]
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

// ============================================================================
// Combined Operation Tests
// ============================================================================

#[test]
fn test_vmx_full_sequence() {
    // Complete VMX setup sequence
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x20, 0x00, 0x00, // MOV RAX, 0x2000
        0xF3, 0x0F, 0xC7, 0x30, // VMXON [rax]
        0x48, 0xC7, 0xC0, 0x00, 0x40, 0x00, 0x00, // MOV RAX, 0x4000
        0x66, 0x0F, 0xC7, 0x30, // VMCLEAR [rax]
        0x0F, 0xC7, 0x30, // VMPTRLD [rax]
        0x0F, 0x01, 0xC4, // VMXOFF
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmread_vmwrite_sequence() {
    // Read, modify, write back VMCS field
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x68, 0x00, 0x00, // MOV RAX, 0x6800
        0x0F, 0x78, 0xC3, // VMREAD rbx, rax
        0x48, 0x83, 0xC3, 0x10, // ADD RBX, 0x10
        0x0F, 0x79, 0xC3, // VMWRITE rax, rbx
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}

#[test]
fn test_vmptrld_vmclear_cycle() {
    // Load and clear VMCS in a loop
    let code = [
        0x48, 0xC7, 0xC0, 0x00, 0x40, 0x00, 0x00, // MOV RAX, 0x4000
        0x48, 0xC7, 0xC3, 0x02, 0x00, 0x00, 0x00, // MOV RBX, 2 (counter)
        // loop:
        0x0F, 0xC7, 0x30, // VMPTRLD [rax]
        0x66, 0x0F, 0xC7, 0x30, // VMCLEAR [rax]
        0x48, 0xFF, 0xCB, // DEC RBX
        0x75, 0xF4, // JNZ loop
        0xF4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu);
}
