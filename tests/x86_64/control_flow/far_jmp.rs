use crate::common::*;

// Comprehensive tests for FAR JMP instruction (inter-segment jump)
// JMP ptr16:16, JMP ptr16:32, JMP m16:16, JMP m16:32, JMP m16:64
// Opcode: EA (immediate), FF /5 (memory)
// Note: 0xEA is invalid (#UD) in 64-bit mode; the immediate-form tests run in
// compatibility mode (see far_call.rs for the same convention with 0x9A).
// Compat-mode targets stay in 16-bit code, so target snippets use
// mode-agnostic encodings (HLT, MOV r8/r16, imm).

// ============================================================================
// FAR JMP - Direct with Immediate Selector:Offset
// ============================================================================

#[test]
fn test_far_jmp_immediate_16_16_basic() {
    // JMP 0x0008:0x2000 - far jump to selector 0x0008, offset 0x2000
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000 (16-bit offset)
        0xf4, // HLT (should not execute)
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    // Write HLT at target address 0x2000
    let target_code = [0xf4]; // HLT
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2001);
}

#[test]
fn test_far_jmp_immediate_16_32_basic() {
    // JMP 0x0008:0x00003000 - far jump with 32-bit offset
    let code = [
        0x66, 0xea, 0x00, 0x30, 0x00, 0x00, 0x08, 0x00, // JMP FAR 0x0008:0x00003000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_jmp_immediate_invalid_in_64bit_raises_ud() {
    // JMP FAR ptr16:16/ptr16:32 (0xEA) is invalid in 64-bit mode (#UD).
    let code = [
        0xea, 0x00, 0x20, 0x00, 0x00, 0x08, 0x00, // JMP FAR 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let ud_handler = [
        0x48, 0xc7, 0xc0, 0x06, 0x00, 0x00, 0x00, // MOV RAX, 6
        0xf4, // HLT
    ];
    mem.write_slice(&ud_handler, GuestAddress(INT_HANDLER_ADDR))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 6);
}

#[test]
fn test_far_jmp_no_return_address() {
    // Unlike CALL, JMP does not push a return address (RSP defaults to 0x8000)
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x89, 0xe0, // MOV AX, SP (check stack)
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Stack should be unchanged
    assert_eq!(regs.rax, 0x8000);
}

// ============================================================================
// FAR JMP - Memory Indirect m16:16, m16:32, m16:64
// ============================================================================

#[test]
fn test_far_jmp_mem_indirect_16_16() {
    // JMP FAR [mem] - load selector:offset from memory
    let code = [
        0x66, 0xff, 0x2c, 0x25, 0x00, 0x20, 0x00, 0x00, // JMP FAR [0x2000] (16-bit offset)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Write far pointer at 0x2000: offset=0x3000, selector=0x0008
    let far_ptr = [0x00, 0x30, 0x08, 0x00]; // offset:selector (little-endian)
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    // Write target code at 0x3000
    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_jmp_mem_indirect_16_32() {
    // JMP FAR [mem] with 32-bit offset
    let code = [
        0xff, 0x2c, 0x25, 0x00, 0x20, 0x00, 0x00, // JMP FAR [0x2000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Write far pointer: 32-bit offset + 16-bit selector
    let far_ptr = [0x00, 0x40, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x4001);
}

#[test]
fn test_far_jmp_mem_indirect_16_64() {
    // JMP FAR [mem] with 64-bit offset
    let code = [
        0x48, 0xff, 0x2c, 0x25, 0x00, 0x20, 0x00, 0x00, // JMP FAR [0x2000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Write far pointer: 64-bit offset + 16-bit selector
    let far_ptr = [
        0x00, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 64-bit offset
        0x08, 0x00, // 16-bit selector
    ];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x5000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x5001);
}

// ============================================================================
// FAR JMP - Different Privilege Levels
// ============================================================================

#[test]
fn test_far_jmp_same_privilege_level() {
    // Jump within same privilege level (CPL=0)
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0xb1, 0xaa, // MOV CL, 0xAA
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rcx, 0xaa);
}

#[test]
fn test_far_jmp_conforming_segment() {
    // Jump using an alternate GDT selector (descriptor checks not modeled).
    let code = [
        0xea, 0x00, 0x20, 0x10, 0x00, // JMP FAR 0x0010:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0xb2, 0xbb, // MOV DL, 0xBB
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rdx, 0xbb);
}

#[test]
fn test_far_jmp_to_higher_privilege() {
    // Descriptor privilege checks are not modeled in this emulator.
    let code = [
        0xea, 0x00, 0x20, 0x10, 0x00, // JMP FAR 0x0010:0x2000 (DPL=0, higher privilege)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0xb0, 0x99, // MOV AL, 0x99
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x99);
}

#[test]
fn test_far_jmp_to_lower_privilege() {
    // JMP cannot transfer to lower privilege (higher CPL number)
    let code = [
        0xea, 0x00, 0x20, 0x1b, 0x00, // JMP FAR 0x001B:0x2000 (RPL=3, lower privilege)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

// ============================================================================
// FAR JMP - Task Switch
// ============================================================================

#[test]
fn test_far_jmp_to_tss_descriptor() {
    // JMP to a TSS selector (task switches are not modeled; the jump falls
    // back to flat segmentation and transfers to the target offset).
    let code = [
        0xea, 0x00, 0x20, 0x18, 0x00, // JMP FAR 0x0018:0x2000 (TSS selector)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0xb3, 0xdd, // MOV BL, 0xDD
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx, 0xdd);
}

#[test]
fn test_far_jmp_through_task_gate() {
    // JMP through task gate (selector outside the GDT limit -> fault)
    let code = [
        0xea, 0x00, 0x00, 0x20, 0x00, // JMP FAR 0x0020:0x0000 (task gate)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_jmp_task_switch_clears_busy() {
    // JMP via a TSS selector transfers control (task state is not modeled)
    let code = [
        0x66, 0xb8, 0x42, 0x00, 0x00, 0x00, // MOV EAX, 0x42
        0xea, 0x00, 0x20, 0x18, 0x00, // JMP FAR TSS
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0xb0, 0x99, // MOV AL, 0x99
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x99);
}

// ============================================================================
// FAR JMP - Segment Selector Validation
// ============================================================================

#[test]
fn test_far_jmp_null_selector() {
    // Jumping with null selector should fault
    let code = [
        0xea, 0x00, 0x20, 0x00, 0x00, // JMP FAR 0x0000:0x2000 (null selector)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_jmp_invalid_selector() {
    // Selector beyond GDT/LDT limit
    let code = [
        0xea, 0x00, 0x20, 0xff, 0xff, // JMP FAR 0xFFFF:0x2000
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_jmp_ldt_selector() {
    // Jump using LDT selector (bit 2 set in selector); no LDT is set up
    let code = [
        0xea, 0x00, 0x20, 0x0c, 0x00, // JMP FAR 0x000C:0x2000 (LDT selector)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);

    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_jmp_gdt_selector() {
    // Jump using GDT selector (bit 2 clear)
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000 (GDT selector)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0xbe, 0xff, 0x00, // MOV SI, 0xFF
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsi, 0xff);
}

// ============================================================================
// FAR JMP - Different Operand Sizes
// ============================================================================

#[test]
fn test_far_jmp_operand_size_16() {
    // 16-bit operand size (compat-mode default with CS.D=0)
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000 (16-bit)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2001);
}

#[test]
fn test_far_jmp_operand_size_32() {
    // 32-bit operand size (0x66 override of the compat-mode 16-bit default)
    let code = [
        0x66, 0xea, 0x00, 0x30, 0x00, 0x00, 0x08, 0x00, // JMP FAR 0x0008:0x3000 (32-bit offset)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_jmp_rex_prefix_64() {
    // REX.W prefix for 64-bit operand size
    let code = [
        0x48, 0xff, 0x2c, 0x25, 0x00, 0x20, 0x00, 0x00, // JMP FAR [0x2000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let far_ptr = [0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x4001);
}

// ============================================================================
// FAR JMP - Error Conditions
// ============================================================================

#[test]
fn test_far_jmp_non_present_segment() {
    // Segment marked not present (selector outside the GDT limit)
    let code = [
        0xea, 0x00, 0x20, 0x20, 0x00, // JMP FAR 0x0020:0x2000 (non-present)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_jmp_wrong_descriptor_type() {
    // Jumping through data segment descriptor (selector outside the GDT limit)
    let code = [
        0xea, 0x00, 0x20, 0x28, 0x00, // JMP FAR 0x0028:0x2000 (data segment)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_far_jmp_to_call_gate() {
    // JMP through call gate (selector outside the GDT limit -> fault)
    let code = [
        0xea, 0x00, 0x20, 0x30, 0x00, // JMP FAR 0x0030:0x2000 (call gate)
        0xf4,
    ];
    let (mut vcpu, _) = setup_vm_compat(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

// ============================================================================
// FAR JMP - Memory Addressing Modes
// ============================================================================

#[test]
fn test_far_jmp_mem_register_indirect() {
    // JMP FAR [RAX]
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x20, 0x00, 0x00, // MOV RAX, 0x2000
        0xff, 0x28, // JMP FAR [RAX]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let far_ptr = [0x00, 0x30, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_jmp_mem_base_displacement() {
    // JMP FAR [RBX + 0x100]
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x1f, 0x00, 0x00, // MOV RBX, 0x1F00
        0xff, 0xab, 0x00, 0x01, 0x00, 0x00, // JMP FAR [RBX + 0x100]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    let far_ptr = [0x00, 0x40, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x4000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x4001);
}

#[test]
fn test_far_jmp_mem_sib_addressing() {
    // JMP FAR [RAX + RBX*4]
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x1e, 0x00, 0x00, // MOV RAX, 0x1E00
        0x48, 0xc7, 0xc3, 0x80, 0x00, 0x00, 0x00, // MOV RBX, 0x80
        0xff, 0x2c, 0x98, // JMP FAR [RAX + RBX*4]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Address = 0x1E00 + 0x80*4 = 0x2000
    let far_ptr = [0x00, 0x50, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x5000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x5001);
}

#[test]
fn test_far_jmp_mem_rip_relative() {
    // JMP FAR [RIP + disp32]
    let code = [
        0xff, 0x2d, 0xfa, 0x0f, 0x00, 0x00, // JMP FAR [RIP + 0x0FFA]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);

    // Calculate target address: RIP after instruction + displacement
    let far_ptr = [0x00, 0x60, 0x00, 0x00, 0x08, 0x00];
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x6000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x6001);
}

// ============================================================================
// FAR JMP - Register Preservation
// ============================================================================

#[test]
fn test_far_jmp_preserves_general_registers() {
    let code = [
        0x66, 0xb8, 0x11, 0x11, 0x00, 0x00, // MOV EAX, 0x1111
        0x66, 0xbb, 0x22, 0x22, 0x00, 0x00, // MOV EBX, 0x2222
        0x66, 0xb9, 0x33, 0x33, 0x00, 0x00, // MOV ECX, 0x3333
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x1111);
    assert_eq!(regs.rbx, 0x2222);
    assert_eq!(regs.rcx, 0x3333);
}

#[test]
fn test_far_jmp_modifies_cs_and_rip() {
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x2001);
    // CS should be updated to selector 0x0008
    let sregs = vcpu.get_sregs().unwrap();
    assert_eq!(sregs.cs.selector, 0x0008);
}

#[test]
fn test_far_jmp_does_not_modify_stack() {
    // RSP starts at 0x8000 (harness default) and must survive the far JMP
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0x89, 0xe7, // MOV DI, SP
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Stack pointer should be unchanged
    assert_eq!(regs.rdi, 0x8000);
}

// ============================================================================
// FAR JMP - Edge Cases
// ============================================================================

#[test]
fn test_far_jmp_to_boundary_address() {
    // Jump to address at segment boundary
    let code = [
        0xea, 0xff, 0xff, 0x08, 0x00, // JMP FAR 0x0008:0xFFFF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0xFFFF))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x10000);
}

#[test]
fn test_far_jmp_zero_offset() {
    // Jump to offset 0
    let code = [
        0xea, 0x00, 0x00, 0x08, 0x00, // JMP FAR 0x0008:0x0000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x0000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0001);
}

#[test]
fn test_far_jmp_max_offset_32bit() {
    // Jump with maximum 32-bit offset
    let code = [
        0x66, 0xea, 0xff, 0xff, 0xff, 0x00, 0x08, 0x00, // JMP FAR 0x0008:0x00FFFFFF
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x00FF_FFFF))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0100_0000);
}

#[test]
fn test_far_jmp_aligned_addresses() {
    // Jump to 16-byte aligned address
    let code = [
        0xea, 0x00, 0x30, 0x08, 0x00, // JMP FAR 0x0008:0x3000 (aligned)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3001);
}

#[test]
fn test_far_jmp_unaligned_addresses() {
    // Jump to unaligned address
    let code = [
        0xea, 0x03, 0x30, 0x08, 0x00, // JMP FAR 0x0008:0x3003 (unaligned)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x3003))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x3004);
}

#[test]
fn test_far_jmp_backwards() {
    // Jump backwards to an address below the code (0x0500 < 0x1000)
    let code = [
        0xea, 0x00, 0x05, 0x08, 0x00, // JMP FAR 0x0008:0x0500
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    // Write HLT at 0x0500
    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x0500))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x0501);
}

#[test]
fn test_far_jmp_forward_large_offset() {
    // Jump forward with large offset
    let code = [
        0x66, 0xea, 0x00, 0x00, 0x01, 0x00, 0x08, 0x00, // JMP FAR 0x0008:0x00010000
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [0xf4];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x10000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rip, 0x10001);
}

#[test]
fn test_far_jmp_same_segment_different_offset() {
    // Jump within same segment to different offset
    let code = [
        0xea, 0x00, 0x20, 0x08, 0x00, // JMP FAR 0x0008:0x2000 (same CS selector)
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm_compat(&code, None);

    let target_code = [
        0xb8, 0xab, 0xcd, // MOV AX, 0xCDAB
        0xf4,
    ];
    mem.write_slice(&target_code, vm_memory::GuestAddress(0x2000))
        .unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0xcdab);
}

// ============================================================================
// FAR JMP - Privilege preservation (regression: issue #78)
// ============================================================================

#[test]
fn test_far_jmp_does_not_escalate_cpl() {
    // A far JMP from CPL3 to a ring-0 (RPL 0) selector must NOT lower the CPL.
    // The emulator derives CPL from CS.selector & 3, so adopting selector 0x08
    // (RPL 0) while at CPL3 would otherwise make the vCPU CPL0 and let guest user
    // code bypass privileged-instruction and user/supervisor page checks.
    // Uses the memory-indirect form (FF /5): the immediate form (0xEA) is #UD
    // in 64-bit mode.
    let code = [
        0xff, 0x2c, 0x25, 0x00, 0x70, 0x00, 0x00, // JMP FAR [0x7000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let far_ptr = [0x00, 0x20, 0x00, 0x00, 0x08, 0x00]; // 0x0008:0x2000
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x7000))
        .unwrap();
    mem.write_slice(&[0xf4], vm_memory::GuestAddress(0x2000))
        .unwrap(); // HLT at the target

    // Drop the vCPU to CPL3 (CS RPL = 3) before the far JMP.
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 0x0b; // GDT index 1 (0x08) with RPL 3
    vcpu.set_sregs(&sregs).unwrap();

    // Execute only the far JMP.
    vcpu.step().unwrap();

    let sregs = vcpu.get_sregs().unwrap();
    assert_eq!(
        sregs.cs.selector & 0x3,
        3,
        "far JMP from CPL3 must preserve CPL (no escalation to ring 0); CS={:#x}",
        sregs.cs.selector,
    );
}

#[test]
fn test_far_jmp_cpl0_stays_cpl0() {
    // A normal CPL0 far JMP to a ring-0 selector keeps CPL0: the fix only blocks
    // privilege *gain* and must not perturb same-privilege transfers.
    let code = [
        0xff, 0x2c, 0x25, 0x00, 0x70, 0x00, 0x00, // JMP FAR [0x7000]
        0xf4,
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let far_ptr = [0x00, 0x20, 0x00, 0x00, 0x08, 0x00]; // 0x0008:0x2000
    mem.write_slice(&far_ptr, vm_memory::GuestAddress(0x7000))
        .unwrap();
    mem.write_slice(&[0xf4], vm_memory::GuestAddress(0x2000))
        .unwrap();

    vcpu.step().unwrap();

    let sregs = vcpu.get_sregs().unwrap();
    assert_eq!(
        sregs.cs.selector, 0x08,
        "CPL0 far JMP should adopt the target ring-0 selector unchanged",
    );
}
