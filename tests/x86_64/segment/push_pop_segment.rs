use crate::common::*;
use rax::cpu::Registers;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress};

fn assert_invalid_segment(code: &[u8]) {
    let (mut vcpu, _) = setup_vm(code, None);
    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("segment opcode should be invalid in 64-bit mode"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Comprehensive tests for PUSH and POP with segment registers
//
// PUSH ES/CS/SS/DS/FS/GS - Push segment register onto stack
// POP ES/SS/DS/FS/GS - Pop from stack into segment register
//
// Note: CS cannot be popped (use far RET instead)
// In 64-bit mode, segment values are pushed as 16-bit but stack pointer adjusts by 8

// ============================================================================
// PUSH segment registers
// ============================================================================

#[test]
fn test_push_es_invalid_in_64bit() {
    let code = [
        0x06, // PUSH ES
        0xf4, // HLT (should not be reached)
    ];
    assert_invalid_segment(&code);
}

#[test]
fn test_push_cs_invalid_in_64bit() {
    let code = [
        0x0e, // PUSH CS
        0xf4, // HLT (should not be reached)
    ];
    assert_invalid_segment(&code);
}

#[test]
fn test_push_ss_invalid_in_64bit() {
    let code = [
        0x16, // PUSH SS
        0xf4, // HLT (should not be reached)
    ];
    assert_invalid_segment(&code);
}

#[test]
fn test_push_ds_invalid_in_64bit() {
    let code = [
        0x1e, // PUSH DS
        0xf4, // HLT (should not be reached)
    ];
    assert_invalid_segment(&code);
}

#[test]
fn test_push_fs() {
    let code = [
        0x0f, 0xa0, // PUSH FS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, STACK_ADDR - 8);

    let mut buf = [0u8; 8];
    mem.read_slice(&mut buf, GuestAddress(STACK_ADDR - 8)).unwrap();
    let stack_value = u64::from_le_bytes(buf);
    assert_eq!(stack_value >> 16, 0);
}

#[test]
fn test_push_gs() {
    let code = [
        0x0f, 0xa8, // PUSH GS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, STACK_ADDR - 8);

    let mut buf = [0u8; 8];
    mem.read_slice(&mut buf, GuestAddress(STACK_ADDR - 8)).unwrap();
    let stack_value = u64::from_le_bytes(buf);
    assert_eq!(stack_value >> 16, 0);
}

// ============================================================================
// POP segment registers
// Note: Cannot POP CS
// ============================================================================

#[test]
fn test_pop_es_invalid_in_64bit() {
    let code = [
        0x07, // POP ES
        0xf4, // HLT (should not be reached)
    ];
    assert_invalid_segment(&code);
}

#[test]
fn test_pop_ss_invalid_in_64bit() {
    let code = [
        0x17, // POP SS
        0xf4, // HLT (should not be reached)
    ];
    assert_invalid_segment(&code);
}

#[test]
fn test_pop_ds_invalid_in_64bit() {
    let code = [
        0x1f, // POP DS
        0xf4, // HLT (should not be reached)
    ];
    assert_invalid_segment(&code);
}

#[test]
fn test_pop_fs() {
    let code = [
        0x0f, 0xa0, // PUSH FS
        0x0f, 0xa1, // POP FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_pop_gs() {
    let code = [
        0x0f, 0xa8, // PUSH GS
        0x0f, 0xa9, // POP GS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, STACK_ADDR);
}

// ============================================================================
// Push/Pop combinations and transfers
// ============================================================================

#[test]
fn test_push_pop_transfer_gs_to_fs() {
    let code = [
        0x0f, 0xa8, // PUSH GS
        0x0f, 0xa1, // POP FS
        0x8c, 0xe0, // MOV AX, FS (read FS)
        0x8c, 0xeb, // MOV BX, GS (read GS)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // FS should equal GS (transferred via stack)
    assert_eq!(regs.rax & 0xFFFF, regs.rbx & 0xFFFF);
}

#[test]
fn test_push_pop_transfer_fs_to_gs() {
    let code = [
        0x0f, 0xa0, // PUSH FS
        0x0f, 0xa9, // POP GS
        0x8c, 0xe0, // MOV AX, FS
        0x8c, 0xeb, // MOV BX, GS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, regs.rbx & 0xFFFF);
}

#[test]
fn test_push_pop_transfer_fs_to_gs() {
    let code = [
        0x0f, 0xa0, // PUSH FS
        0x0f, 0xa9, // POP GS
        0x8c, 0xe0, // MOV AX, FS
        0x8c, 0xe9, // MOV CX, GS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax & 0xFFFF, regs.rcx & 0xFFFF);
}

#[test]
fn test_multiple_pushes() {
    let code = [
        0x06, // PUSH ES
        0x0e, // PUSH CS
        0x16, // PUSH SS
        0x1e, // PUSH DS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Stack pointer should have decreased by 32 (4 pushes × 8 bytes)
    assert_eq!(regs.rsp, STACK_ADDR - 32);
}

#[test]
fn test_multiple_pops() {
    let code = [
        0x06, // PUSH ES
        0x16, // PUSH SS
        0x1e, // PUSH DS
        0x0f, 0xa0, // PUSH FS
        // Pop in reverse order
        0x0f, 0xa9, // POP GS (gets FS)
        0x1f, // POP DS (gets DS)
        0x17, // POP SS (gets SS)
        0x07, // POP ES (gets ES)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Stack should be balanced
    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_push_all_segments() {
    let code = [
        0x06, // PUSH ES
        0x0e, // PUSH CS
        0x16, // PUSH SS
        0x1e, // PUSH DS
        0x0f, 0xa0, // PUSH FS
        0x0f, 0xa8, // PUSH GS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 6 segments × 8 bytes = 48 bytes
    assert_eq!(regs.rsp, STACK_ADDR - 48);

    // Verify all values on stack are valid segment values (16-bit)
    for i in 0..6 {
        let mut buf = [0u8; 8];
        mem.read_slice(&mut buf, GuestAddress(STACK_ADDR - 8 * (i + 1))).unwrap();
        let value = u64::from_le_bytes(buf);
        assert_eq!(value >> 16, 0); // Upper bits should be zero
    }
}

#[test]
fn test_pop_all_segments_except_cs() {
    let code = [
        0x06, // PUSH ES
        0x16, // PUSH SS
        0x1e, // PUSH DS
        0x0f, 0xa0, // PUSH FS
        0x0f, 0xa8, // PUSH GS
        // Pop in reverse order
        0x07, // POP ES (gets GS)
        0x17, // POP SS (gets FS)
        0x1f, // POP DS (gets DS)
        0x0f, 0xa1, // POP FS (gets SS)
        0x0f, 0xa9, // POP GS (gets ES)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Stack should be balanced
    assert_eq!(regs.rsp, STACK_ADDR);
}

// ============================================================================
// Stack alignment and memory tests
// ============================================================================

#[test]
fn test_push_segment_memory_content() {
    let code = [
        0x8c, 0xc0, // MOV AX, ES (get ES value)
        0x06, // PUSH ES
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Read pushed value from stack
    let mut buf = [0u8; 8];
    mem.read_slice(&mut buf, GuestAddress(STACK_ADDR - 8)).unwrap();
    let stack_value = u64::from_le_bytes(buf);

    // Should match ES value
    assert_eq!(stack_value & 0xFFFF, regs.rax & 0xFFFF);
}

#[test]
fn test_pop_segment_from_prepared_stack() {
    let code = [
        // Prepare stack with known value (0)
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x50, // PUSH RAX
        // Pop into ES
        0x07, // POP ES
        // Read ES back
        0x8c, 0xc3, // MOV BX, ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFF, 0);
}

#[test]
fn test_segment_stack_with_general_registers() {
    let code = [
        0x06, // PUSH ES
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x50, // PUSH RAX
        0x58, // POP RAX (should be 0x42)
        0x1f, // POP DS (should be ES value)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x42);
    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_nested_push_pop_segments() {
    let code = [
        0x06, // PUSH ES
        0x1e, // PUSH DS
        0x06, // PUSH ES
        0x07, // POP ES
        0x1f, // POP DS
        0x07, // POP ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_push_pop_preserves_segment_value() {
    let code = [
        // Get original ES value
        0x8c, 0xc0, // MOV AX, ES
        // Push and pop ES
        0x06, // PUSH ES
        0x07, // POP ES
        // Get ES value again
        0x8c, 0xc3, // MOV BX, ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // ES should be unchanged
    assert_eq!(regs.rax & 0xFFFF, regs.rbx & 0xFFFF);
}

#[test]
fn test_stack_grows_downward() {
    let code = [
        0x48, 0x89, 0xe0, // MOV RAX, RSP (save initial SP)
        0x06, // PUSH ES
        0x48, 0x89, 0xe3, // MOV RBX, RSP (save SP after push)
        0x07, // POP ES
        0x48, 0x89, 0xe1, // MOV RCX, RSP (save SP after pop)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // RBX (after push) < RAX (initial)
    assert!(regs.rbx < regs.rax);
    // RCX (after pop) == RAX (back to initial)
    assert_eq!(regs.rcx, regs.rax);
}

#[test]
fn test_multiple_segment_round_trip() {
    let code = [
        // Save original values
        0x8c, 0xc0, // MOV AX, ES
        0x8c, 0xd9, // MOV CX, DS

        // Push both
        0x06, // PUSH ES
        0x1e, // PUSH DS

        // Pop in reverse order
        0x1f, // POP DS (gets DS back)
        0x07, // POP ES (gets ES back)

        // Read back
        0x8c, 0xc3, // MOV BX, ES
        0x8c, 0xda, // MOV DX, DS

        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // Values should match
    assert_eq!(regs.rax & 0xFFFF, regs.rbx & 0xFFFF); // ES unchanged
    assert_eq!(regs.rcx & 0xFFFF, regs.rdx & 0xFFFF); // DS unchanged
}

#[test]
fn test_push_pop_with_offset_stack() {
    let code = [
        // Adjust stack pointer
        0x48, 0x83, 0xec, 0x10, // SUB RSP, 16
        0x06, // PUSH ES
        0x07, // POP ES
        0x48, 0x83, 0xc4, 0x10, // ADD RSP, 16
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_interleaved_segment_general_pushes() {
    let code = [
        0x06, // PUSH ES
        0x48, 0xc7, 0xc0, 0x11, 0x00, 0x00, 0x00, // MOV RAX, 0x11
        0x50, // PUSH RAX
        0x16, // PUSH SS
        0x48, 0xc7, 0xc3, 0x22, 0x00, 0x00, 0x00, // MOV RBX, 0x22
        0x53, // PUSH RBX
        0x1e, // PUSH DS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // 5 pushes × 8 bytes = 40 bytes
    assert_eq!(regs.rsp, STACK_ADDR - 40);
}

#[test]
fn test_segment_push_pop_symmetry() {
    let code = [
        // Push all segment registers
        0x06, // PUSH ES
        0x16, // PUSH SS
        0x1e, // PUSH DS
        0x0f, 0xa0, // PUSH FS
        0x0f, 0xa8, // PUSH GS

        // Pop in same order (different destinations)
        0x0f, 0xa9, // POP GS
        0x0f, 0xa1, // POP FS
        0x1f, // POP DS
        0x17, // POP SS
        0x07, // POP ES

        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_push_es_multiple_times() {
    let code = [
        0x06, // PUSH ES
        0x06, // PUSH ES
        0x06, // PUSH ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 24);
}

#[test]
fn test_pop_ds_multiple_times() {
    let code = [
        0x1e, // PUSH DS
        0x1e, // PUSH DS
        0x1e, // PUSH DS
        0x1f, // POP DS
        0x1f, // POP DS
        0x1f, // POP DS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_push_pop_fs_gs_interleaved() {
    let code = [
        0x0f, 0xa0, // PUSH FS
        0x0f, 0xa8, // PUSH GS
        0x0f, 0xa1, // POP FS
        0x0f, 0xa9, // POP GS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
}

#[test]
fn test_segment_push_pop_lifo() {
    let code = [
        0x06, // PUSH ES
        0x1e, // PUSH DS
        0x0f, 0xa0, // PUSH FS
        // Pop in LIFO order
        0x0f, 0xa9, // POP GS (gets FS)
        0x07, // POP ES (gets DS)
        0x1f, // POP DS (gets ES)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_push_segment_after_modification() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX
        0x8e, 0xe0, // MOV FS, AX (set FS to 0)
        0x0f, 0xa0, // PUSH FS (push modified FS)
        0x58, // POP RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_pop_segment_clears_high_bits() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xFFFFFFFF
        0x50, // PUSH RAX
        0x07, // POP ES
        0x8c, 0xc3, // MOV BX, ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // ES should only have lower 16 bits
    assert_eq!(regs.rbx & 0xFFFF, 0xFFFF);
}

#[test]
fn test_push_all_pop_one() {
    let code = [
        0x06, // PUSH ES
        0x16, // PUSH SS
        0x1e, // PUSH DS
        0x07, // POP ES (gets DS)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Stack should have 2 items left
    assert_eq!(regs.rsp, STACK_ADDR - 16);
}
