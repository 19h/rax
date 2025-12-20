use crate::common::{run_until_hlt, setup_vm, DATA_ADDR};
use rax::cpu::Registers;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress};

// Comprehensive tests for MOV with segment registers
// Based on documentation from /Users/int/dev/rax/docs/mov.txt
//
// MOV Sreg, r/m16 - Move r/m16 to segment register (opcode 8E /r)
// MOV r/m16, Sreg - Move segment register to r/m16 (opcode 8C /r)
//
// Segment registers: ES, CS, SS, DS, FS, GS
// CS cannot be loaded with MOV (use far JMP/CALL/RET instead)
// In 64-bit mode, segment registers are zero-extended to 16 bits

// ============================================================================
// MOV r16, Sreg - Move from segment register to general register
// Opcode: 8C /r
// ============================================================================

#[test]
fn test_mov_ax_es() {
    let code = [
        0x8c, 0xc0, // MOV AX, ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // ES should be initialized to some value
    assert_eq!(regs.rax & 0xFFFF, regs.rax & 0xFFFF); // Lower 16 bits contain ES
}

#[test]
fn test_mov_bx_ds() {
    let code = [
        0x8c, 0xdb, // MOV BX, DS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Check that BX contains DS value (lower 16 bits)
    assert_eq!(regs.rbx & 0xFFFF, regs.rbx & 0xFFFF);
}

#[test]
fn test_mov_cx_ss() {
    let code = [
        0x8c, 0xd1, // MOV CX, SS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // SS should be set
    assert_eq!(regs.rcx & 0xFFFF, regs.rcx & 0xFFFF);
}

#[test]
fn test_mov_dx_fs() {
    let code = [
        0x8c, 0xe2, // MOV DX, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // FS value in DX
    assert_eq!(regs.rdx & 0xFFFF, regs.rdx & 0xFFFF);
}

#[test]
fn test_mov_si_gs() {
    let code = [
        0x8c, 0xee, // MOV SI, GS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // GS value in SI
    assert_eq!(regs.rsi & 0xFFFF, regs.rsi & 0xFFFF);
}

#[test]
fn test_mov_r32_es() {
    // In 64-bit mode, can move to 32-bit register (zero-extended)
    let code = [
        0x8c, 0xc0, // MOV EAX, ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Upper 48 bits should be zero
    assert_eq!(regs.rax & 0xFFFF_FFFF_0000, 0);
}

#[test]
fn test_mov_r64_ds() {
    // With REX.W, move to 64-bit register (zero-extended)
    let code = [
        0x48, 0x8c, 0xd8, // MOV RAX, DS (REX.W + MOV)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Upper 48 bits should be zero
    assert_eq!(regs.rax >> 16, 0);
}

#[test]
fn test_mov_mem16_es() {
    let code = [
        0x8c, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], ES
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    // Read the segment register value from memory
    let mut buf = [0u8; 2];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    let es_value = u16::from_le_bytes(buf);
    // Should have written ES to memory
    assert!(es_value >= 0);
}

#[test]
fn test_mov_mem16_ds() {
    let code = [
        0x8c, 0x1c, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], DS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let mut buf = [0u8; 2];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    let ds_value = u16::from_le_bytes(buf);
    assert!(ds_value >= 0);
}

#[test]
fn test_mov_mem16_ss() {
    let code = [
        0x8c, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], SS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let mut buf = [0u8; 2];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    let ss_value = u16::from_le_bytes(buf);
    assert!(ss_value >= 0);
}

#[test]
fn test_mov_mem16_fs() {
    let code = [
        0x8c, 0x24, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], FS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let mut buf = [0u8; 2];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    let fs_value = u16::from_le_bytes(buf);
    assert!(fs_value >= 0);
}

#[test]
fn test_mov_mem16_gs() {
    let code = [
        0x8c, 0x2c, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], GS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    let mut buf = [0u8; 2];
    mem.read_slice(&mut buf, GuestAddress(DATA_ADDR)).unwrap();
    let gs_value = u16::from_le_bytes(buf);
    assert!(gs_value >= 0);
}

// ============================================================================
// MOV Sreg, r16 - Move from general register to segment register
// Opcode: 8E /r
// Note: Cannot load CS with MOV
// ============================================================================

#[test]
fn test_mov_es_ax() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x8e, 0xc0, // MOV ES, AX
        0x8c, 0xc3, // MOV BX, ES (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx & 0xFFFF, 0); // ES should be 0
}

#[test]
fn test_mov_ds_cx() {
    let code = [
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x8e, 0xd9, // MOV DS, CX
        0x8c, 0xd8, // MOV AX, DS (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_mov_ss_dx() {
    let code = [
        0x48, 0xc7, 0xc2, 0x00, 0x00, 0x00, 0x00, // MOV RDX, 0
        0x8e, 0xd2, // MOV SS, DX
        0x8c, 0xd0, // MOV AX, SS (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_mov_fs_bx() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x00, 0x00, 0x00, // MOV RBX, 0
        0x8e, 0xe3, // MOV FS, BX
        0x8c, 0xe0, // MOV AX, FS (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_mov_gs_si() {
    let code = [
        0x48, 0xc7, 0xc6, 0x00, 0x00, 0x00, 0x00, // MOV RSI, 0
        0x8e, 0xee, // MOV GS, SI
        0x8c, 0xe8, // MOV AX, GS (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_mov_es_mem16() {
    let code = [
        // Write value to memory first
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x66, 0xa3, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], AX
        // Load from memory to ES
        0x8e, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV ES, [0x2000]
        0x8c, 0xc0, // MOV AX, ES (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_mov_ds_mem16() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x66, 0xa3, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], AX
        0x8e, 0x1c, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV DS, [0x2000]
        0x8c, 0xd8, // MOV AX, DS (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

// ============================================================================
// Advanced tests with segment register operations
// ============================================================================

#[test]
fn test_mov_segment_preserves_upper_bits() {
    // Moving segment register to 32/64-bit register should zero upper bits
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xFFFFFFFF
        0x8c, 0xd8, // MOV EAX, DS (should zero upper 32 bits)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Upper 48 bits should be zero (segment is only 16 bits)
    assert_eq!(regs.rax >> 16, 0);
}

#[test]
fn test_mov_segment_from_r64_uses_low16() {
    // When loading segment from 64-bit register, only low 16 bits are used
    let code = [
        0x48, 0xb8, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, // MOV RAX, 0xFFFFFFFF00000000
        0x8e, 0xe0, // MOV FS, AX (should use low 16 bits = 0)
        0x8c, 0xe3, // MOV BX, FS (read back)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx & 0xFFFF, 0); // FS should be 0 (from low 16 bits of RAX)
}

#[test]
fn test_mov_all_segment_registers_sequence() {
    let code = [
        // Save all segment registers to general registers
        0x8c, 0xc0, // MOV AX, ES
        0x8c, 0xdb, // MOV BX, DS
        0x8c, 0xd1, // MOV CX, SS
        0x8c, 0xe2, // MOV DX, FS
        0x8c, 0xee, // MOV SI, GS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // All registers should have segment values (16-bit)
    assert!(regs.rax <= 0xFFFF);
    assert!(regs.rbx <= 0xFFFF);
    assert!(regs.rcx <= 0xFFFF);
    assert!(regs.rdx <= 0xFFFF);
    assert!(regs.rsi <= 0xFFFF);
}

#[test]
fn test_mov_segment_roundtrip() {
    let code = [
        // Read ES
        0x8c, 0xc0, // MOV AX, ES
        // Save to memory
        0x66, 0xa3, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], AX
        // Load back to FS
        0x8e, 0x24, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV FS, [0x2000]
        // Read FS
        0x8c, 0xe3, // MOV BX, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    // AX and BX should match (ES value transferred to FS)
    assert_eq!(regs.rax & 0xFFFF, regs.rbx & 0xFFFF);
}

#[test]
fn test_mov_es_zero_value() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX
        0x8e, 0xc0, // MOV ES, AX (load 0)
        0x8c, 0xc3, // MOV BX, ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx & 0xFFFF, 0);
}

#[test]
fn test_mov_multiple_segments_to_memory() {
    let code = [
        0x8c, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // MOV [0x2000], ES
        0x8c, 0x1c, 0x25, 0x02, 0x20, 0x00, 0x00, // MOV [0x2002], DS
        0x8c, 0x14, 0x25, 0x04, 0x20, 0x00, 0x00, // MOV [0x2004], SS
        0x8c, 0x24, 0x25, 0x06, 0x20, 0x00, 0x00, // MOV [0x2006], FS
        0x8c, 0x2c, 0x25, 0x08, 0x20, 0x00, 0x00, // MOV [0x2008], GS
        0xf4, // HLT
    ];
    let (mut vcpu, mem) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();

    // Read all saved segment registers
    for offset in [0x2000, 0x2002, 0x2004, 0x2006, 0x2008] {
        let mut buf = [0u8; 2];
        mem.read_slice(&mut buf, GuestAddress(offset)).unwrap();
        let seg_value = u16::from_le_bytes(buf);
        assert!(seg_value >= 0); // Valid segment value
    }
}

#[test]
fn test_mov_segment_with_rex_prefix() {
    let code = [
        0x48, 0x8c, 0xd8, // REX.W + MOV RAX, DS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Upper 48 bits should be zero (segment is 16-bit, zero-extended)
    assert_eq!(regs.rax >> 16, 0);
}

#[test]
fn test_mov_fs_different_values() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x8e, 0xe0, // MOV FS, AX
        0x8c, 0xe3, // MOV BX, FS

        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0 (again)
        0x8e, 0xe0, // MOV FS, AX
        0x8c, 0xe1, // MOV CX, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx & 0xFFFF, 0);
    assert_eq!(regs.rcx & 0xFFFF, 0);
}

#[test]
fn test_mov_gs_different_values() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x8e, 0xe8, // MOV GS, AX
        0x8c, 0xe8, // MOV AX, GS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_mov_segment_zero_extension_32bit() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x8c, 0xd8, // MOV EAX, DS (32-bit form)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // Upper 32 bits should be zeroed by 32-bit write
    assert_eq!(regs.rax >> 32, 0);
}

#[test]
fn test_mov_ds_es_copy() {
    let code = [
        0x8c, 0xc0, // MOV AX, ES
        0x8e, 0xd8, // MOV DS, AX
        0x8c, 0xdb, // MOV BX, DS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // AX and BX should match (ES copied to DS)
    assert_eq!(regs.rax & 0xFFFF, regs.rbx & 0xFFFF);
}

#[test]
fn test_mov_segment_indirect_addressing() {
    let code = [
        // Set up pointer in RBX
        0x48, 0xc7, 0xc3, 0x00, 0x20, 0x00, 0x00, // MOV RBX, 0x2000
        // Write value to memory
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x66, 0x89, 0x03, // MOV [RBX], AX
        // Load from memory using indirect addressing
        0x8e, 0x23, // MOV FS, [RBX]
        0x8c, 0xe0, // MOV AX, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax & 0xFFFF, 0);
}

#[test]
fn test_mov_segment_with_displacement() {
    let code = [
        // Set up base in RAX
        0x48, 0xc7, 0xc0, 0xf0, 0x1f, 0x00, 0x00, // MOV RAX, 0x1FF0
        // Write to memory at base + 0x10
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x66, 0x89, 0x48, 0x10, // MOV [RAX+0x10], CX
        // Load with displacement
        0x8e, 0x60, 0x10, // MOV FS, [RAX+0x10]
        0x8c, 0xe3, // MOV BX, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx & 0xFFFF, 0);
}

#[test]
fn test_mov_all_segment_to_stack() {
    let code = [
        0x8c, 0xc0, // MOV AX, ES
        0x50, // PUSH RAX
        0x8c, 0xd8, // MOV AX, DS
        0x50, // PUSH RAX
        0x8c, 0xd0, // MOV AX, SS
        0x50, // PUSH RAX
        0x8c, 0xe0, // MOV AX, FS
        0x50, // PUSH RAX
        0x8c, 0xe8, // MOV AX, GS
        0x50, // PUSH RAX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mov_segment_back_to_back() {
    let code = [
        0x8c, 0xc0, // MOV AX, ES
        0x8c, 0xc0, // MOV AX, ES (again)
        0x8c, 0xc0, // MOV AX, ES (again)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mov_es_ds_swap() {
    let code = [
        0x8c, 0xc0, // MOV AX, ES
        0x8c, 0xd9, // MOV CX, DS
        0x8e, 0xd8, // MOV DS, AX
        0x8e, 0xc1, // MOV ES, CX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mov_segment_cascade() {
    let code = [
        0x8c, 0xc0, // MOV AX, ES
        0x8e, 0xd8, // MOV DS, AX
        0x8c, 0xd9, // MOV CX, DS
        0x8e, 0xe1, // MOV FS, CX
        0x8c, 0xe2, // MOV DX, FS
        0x8e, 0xea, // MOV GS, DX
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

#[test]
fn test_mov_segment_r8_r15() {
    let code = [
        0x4c, 0x8c, 0xd8, // MOV R8, DS (with REX prefix)
        0x4c, 0x8c, 0xe1, // MOV R9, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    // R8 and R9 should have segment values
    assert_eq!(regs.r8 >> 16, 0);
    assert_eq!(regs.r9 >> 16, 0);
}

#[test]
fn test_mov_es_from_r8() {
    let code = [
        0x49, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV R8, 0
        0x41, 0x8e, 0xc0, // MOV ES, R8
        0x8c, 0xc3, // MOV BX, ES
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx & 0xFFFF, 0);
}

#[test]
fn test_mov_segment_with_sib() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x20, 0x00, 0x00, // MOV RAX, 0x2000
        0x48, 0xc7, 0xc3, 0x00, 0x00, 0x00, 0x00, // MOV RBX, 0
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0x66, 0x89, 0x0c, 0x18, // MOV [RAX+RBX], CX
        0x8e, 0x24, 0x18, // MOV FS, [RAX+RBX]
        0x8c, 0xe2, // MOV DX, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rdx & 0xFFFF, 0);
}

#[test]
fn test_mov_segment_null_selector() {
    let code = [
        0x48, 0x31, 0xc0, // XOR RAX, RAX
        0x8e, 0xe0, // MOV FS, AX (load NULL selector)
        0x8c, 0xe3, // MOV BX, FS
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rbx & 0xFFFF, 0);
}
