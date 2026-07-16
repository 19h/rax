//! Intel APX PUSH2/POP2 Instruction Tests
//!
//! PUSH2 and POP2 are new APX instructions that transfer pairs of registers in
//! one instruction. PUSH2 is both-or-neither on faults, but the two writes are
//! not required to form one atomic 16-byte store.
//!
//! In Intel operand nomenclature, V is EVEX.VVVVV and B is ModRM.R/M:
//! - PUSH2 V, B: `[new RSP] = B`, `[new RSP+8] = V`, `RSP -= 16`
//! - POP2 V, B: `V = [RSP]`, `B = [RSP+8]`, `RSP += 16`
//!
//! Encoding/exception constraints:
//! - Uses EVEX MAP4 with ND=1, U=1, pp=00, and 64-bit operand size
//! - RSP is forbidden; POP2 destinations must be distinct
//! - The pre-instruction RSP must be 16-byte aligned
//! - Operands encoded in reg and vvvv fields
//! - Can use EGPR (R16-R31) via extended EVEX

use crate::common::*;

#[test]
fn test_push2_stack_layout_match_llvm() {
    // LLVM 23 assembles "push2 %rax, %rbx" as 62 f4 64 18 ff f0.
    // PUSH2 stores the first operand at the new RSP and the second at RSP+8.
    let code = [0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0x1111_2222_3333_4444;
    regs.rbx = 0xAAAA_BBBB_CCCC_DDDD;

    let (mut vcpu, mem) = setup_apx_vm(&code, Some(regs));
    vcpu.set_mem_recording(true);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);

    let mut first = [0u8; 8];
    let mut second = [0u8; 8];
    mem.read_slice(&mut first, GuestAddress(STACK_ADDR - 16))
        .unwrap();
    mem.read_slice(&mut second, GuestAddress(STACK_ADDR - 8))
        .unwrap();
    assert_eq!(u64::from_le_bytes(first), 0x1111_2222_3333_4444);
    assert_eq!(u64::from_le_bytes(second), 0xAAAA_BBBB_CCCC_DDDD);

    let mut records = Vec::new();
    vcpu.drain_mem_records(&mut records);
    let writes: Vec<_> = records
        .into_iter()
        .filter(|record| record.access == rax::vm::vcpu::MemAccess::Write)
        .collect();
    assert_eq!(writes.len(), 2, "PUSH2 logical qword records");
    assert_eq!(
        (writes[0].addr, writes[0].size, writes[0].value),
        (STACK_ADDR - 16, 8, 0x1111_2222_3333_4444)
    );
    assert_eq!(
        (writes[1].addr, writes[1].size, writes[1].value),
        (STACK_ADDR - 8, 8, 0xAAAA_BBBB_CCCC_DDDD)
    );
}

#[test]
fn test_push2_pop2_roundtrip_match_llvm() {
    // PUSH2 %rax,%rbx followed by the reversed POP2 %rbx,%rax restores both
    // registers under Intel's V/B transfer order.
    let code = [
        0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, 0x31, 0xC0, // XOR eax, eax
        0x31, 0xDB, // XOR ebx, ebx
        0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC3, 0xF4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x0102_0304_0506_0708;
    regs.rbx = 0x8877_6655_4433_2211;

    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
    assert_eq!(regs.rax, 0x0102_0304_0506_0708);
    assert_eq!(regs.rbx, 0x8877_6655_4433_2211);
}

#[test]
fn test_push2_pop2_ppx_hint_roundtrip_match_llvm() {
    // EVEX.W is the non-semantic PPX hint: LLVM 23 names the hinted forms
    // PUSH2P/POP2P and otherwise emits the same operands and transfer order.
    let code = [
        0x62, 0xF4, 0xE4, 0x18, 0xFF, 0xF0, 0x31, 0xC0, // PUSH2P; XOR eax,eax
        0x31, 0xDB, // XOR ebx,ebx
        0x62, 0xF4, 0xFC, 0x18, 0x8F, 0xC3, 0xF4, // POP2P; HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x0123_4567_89AB_CDEF;
    regs.rbx = 0xFEDC_BA98_7654_3210;

    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
    assert_eq!(regs.rax, 0x0123_4567_89AB_CDEF);
    assert_eq!(regs.rbx, 0xFEDC_BA98_7654_3210);
}

// ============================================================================
// Basic PUSH2 Tests
// ============================================================================

/// PUSH2 with two legacy registers
#[test]
fn test_push2_rax_rbx() {
    // PUSH2 rax, rbx
    let code = [
        0x62, 0xF4, 0x64, 0x18, // EVEX prefix for PUSH2
        0xFF, 0xF0, // PUSH2 reg encoding
        0xF4, // HLT
    ];
    let mut regs = Registers::default();
    regs.rax = 0x1111111111111111;
    regs.rbx = 0x2222222222222222;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
}

/// PUSH2 with different register pairs
#[test]
fn test_push2_rcx_rdx() {
    // PUSH2 rcx, rdx
    let code = [0x62, 0xF4, 0x6C, 0x18, 0xFF, 0xF1, 0xF4];
    let mut regs = Registers::default();
    regs.rcx = 0xAAAABBBBCCCCDDDD;
    regs.rdx = 0x1234567890ABCDEF;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
}

/// PUSH2 with R8-R15 registers
#[test]
fn test_push2_r8_r9() {
    // PUSH2 r8, r9
    let code = [0x62, 0xD4, 0x34, 0x18, 0xFF, 0xF0, 0xF4];
    let mut regs = Registers::default();
    regs.r8 = 0x8888888888888888;
    regs.r9 = 0x9999999999999999;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
}

/// PUSH2 with mixed legacy and extended registers
#[test]
fn test_push2_rax_r10() {
    // PUSH2 rax, r10
    let code = [0x62, 0xF4, 0x2C, 0x18, 0xFF, 0xF0, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0xDEADDEADDEADDEAD;
    regs.r10 = 0xBEEFBEEFBEEFBEEF;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
}

// ============================================================================
// Basic POP2 Tests
// ============================================================================

/// POP2 with two legacy registers
#[test]
fn test_pop2_rax_rbx() {
    // First push some values, then POP2
    // PUSH rbx; PUSH rax; POP2 %rbx,%rax (V=RAX, B=RBX)
    let code = [
        0x53, // PUSH rbx
        0x50, // PUSH rax
        0x62, 0xF4, 0x7C, 0x18, // EVEX prefix for POP2
        0x8F, 0xC3, // POP2 reg encoding
        0xF4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x1111111111111111;
    regs.rbx = 0x2222222222222222;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rax, 0x1111111111111111);
    assert_eq!(regs.rbx, 0x2222222222222222);
}

/// POP2 with different register pairs
#[test]
fn test_pop2_rcx_rdx() {
    // PUSH rdx; PUSH rcx; POP2 %rdx,%rcx (V=RCX, B=RDX)
    let code = [
        0x52, // PUSH rdx
        0x51, // PUSH rcx
        0x62, 0xF4, 0x74, 0x18, 0x8F, 0xC2, 0xF4,
    ];
    let mut regs = Registers::default();
    regs.rcx = 0xCAFEBABE12345678;
    regs.rdx = 0xFEEDFACE87654321;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rcx, 0xCAFEBABE12345678);
    assert_eq!(regs.rdx, 0xFEEDFACE87654321);
}

/// POP2 with R12-R13 registers
#[test]
fn test_pop2_r12_r13() {
    // Set up stack with values, then POP2
    let code = [
        0x41, 0x55, // PUSH r13
        0x41, 0x54, // PUSH r12
        0x62, 0xD4, 0x1C, 0x18, 0x8F, 0xC5, 0xF4,
    ];
    let mut regs = Registers::default();
    regs.r12 = 0x1212121212121212;
    regs.r13 = 0x1313131313131313;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.r12, 0x1212121212121212);
    assert_eq!(regs.r13, 0x1313131313131313);
}

// ============================================================================
// PUSH2/POP2 with EGPR (R16-R31)
// ============================================================================

/// PUSH2 with R16 and R17
#[test]
fn test_push2_r16_r17() {
    // PUSH2 r16, r17 - uses extended EVEX encoding
    let code = [
        0x62, 0xFC, 0x74, 0x10, // EVEX with EGPR bits
        0xFF, 0xF0, 0xF4,
    ];
    let mut initial = Registers::default();
    initial.r16 = 0x1616_1616_1616_1616;
    initial.r17 = 0x1717_1717_1717_1717;
    let (mut vcpu, mem) = setup_apx_vm(&code, Some(initial));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let mut stack = [0u8; 16];
    mem.read_slice(&mut stack, GuestAddress(STACK_ADDR - 16))
        .unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
    assert_eq!(
        u64::from_le_bytes(stack[..8].try_into().unwrap()),
        0x1616_1616_1616_1616
    );
    assert_eq!(
        u64::from_le_bytes(stack[8..].try_into().unwrap()),
        0x1717_1717_1717_1717
    );
}

/// POP2 with R20 and R21
#[test]
fn test_pop2_r20_r21() {
    let code = [0x62, 0xFC, 0x54, 0x10, 0x8F, 0xC4, 0xF4];
    let (mut vcpu, mem) = setup_apx_vm(&code, None);
    let low = 0x2121_2121_2121_2121u64;
    let high = 0x2020_2020_2020_2020u64;
    let mut stack = [0u8; 16];
    stack[..8].copy_from_slice(&low.to_le_bytes());
    stack[8..].copy_from_slice(&high.to_le_bytes());
    mem.write_slice(&stack, GuestAddress(STACK_ADDR)).unwrap();
    vcpu.set_mem_recording(true);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR + 16);
    assert_eq!(regs.r21, low, "EVEX V destination");
    assert_eq!(regs.r20, high, "ModRM B destination");
    let mut records = Vec::new();
    vcpu.drain_mem_records(&mut records);
    let reads: Vec<_> = records
        .into_iter()
        .filter(|record| record.access == rax::vm::vcpu::MemAccess::Read)
        .collect();
    assert_eq!(reads.len(), 2, "POP2 logical qword records");
    assert_eq!(
        (reads[0].addr, reads[0].size, reads[0].value),
        (STACK_ADDR, 8, low)
    );
    assert_eq!(
        (reads[1].addr, reads[1].size, reads[1].value),
        (STACK_ADDR + 8, 8, high)
    );
}

/// PUSH2 with mixed EGPR and legacy registers
#[test]
fn test_push2_rax_r24() {
    // PUSH2 rax, r24
    let code = [0x62, 0xF4, 0x3C, 0x10, 0xFF, 0xF0, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0xAAAAAAAAAAAAAAAA;
    regs.r24 = 0x2424_2424_2424_2424;
    let (mut vcpu, mem) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let mut stack = [0u8; 16];
    mem.read_slice(&mut stack, GuestAddress(STACK_ADDR - 16))
        .unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
    assert_eq!(
        u64::from_le_bytes(stack[..8].try_into().unwrap()),
        0xAAAAAAAAAAAAAAAA
    );
    assert_eq!(
        u64::from_le_bytes(stack[8..].try_into().unwrap()),
        0x2424_2424_2424_2424
    );
}

/// POP2 with mixed EGPR and legacy registers
#[test]
fn test_pop2_rbx_r28() {
    let code = [0x62, 0xF4, 0x1C, 0x10, 0x8F, 0xC3, 0xF4];
    let (mut vcpu, mem) = setup_apx_vm(&code, None);
    let low = 0x2828_2828_2828_2828u64;
    let high = 0xBBBB_BBBB_BBBB_BBBBu64;
    let mut stack = [0u8; 16];
    stack[..8].copy_from_slice(&low.to_le_bytes());
    stack[8..].copy_from_slice(&high.to_le_bytes());
    mem.write_slice(&stack, GuestAddress(STACK_ADDR)).unwrap();
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR + 16);
    assert_eq!(regs.r28, low, "EVEX V destination");
    assert_eq!(regs.rbx, high, "ModRM B destination");
}

/// PUSH2 with R30 and R31 (highest EGPR)
#[test]
fn test_push2_r30_r31() {
    // PUSH2 r30, r31
    let code = [0x62, 0xDC, 0x04, 0x10, 0xFF, 0xF6, 0xF4];
    let mut initial = Registers::default();
    initial.r30 = 0x3030_3030_3030_3030;
    initial.r31 = 0x3131_3131_3131_3131;
    let (mut vcpu, mem) = setup_apx_vm(&code, Some(initial));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let mut stack = [0u8; 16];
    mem.read_slice(&mut stack, GuestAddress(STACK_ADDR - 16))
        .unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
    assert_eq!(
        u64::from_le_bytes(stack[..8].try_into().unwrap()),
        0x3030_3030_3030_3030
    );
    assert_eq!(
        u64::from_le_bytes(stack[8..].try_into().unwrap()),
        0x3131_3131_3131_3131
    );
}

// ============================================================================
// PUSH2/POP2 Roundtrip Tests
// ============================================================================

/// Reversing the POP2 operands restores the PUSH2 register pair.
#[test]
fn test_push2_pop2_roundtrip() {
    // PUSH2 %rax,%rbx; POP2 %rbx,%rax
    let code = [
        0x62, 0xF4, 0x64, 0x18, // PUSH2
        0xFF, 0xF0, 0x62, 0xF4, 0x7C, 0x18, // POP2
        0x8F, 0xC3, 0xF4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xDEADBEEFCAFEBABE;
    regs.rbx = 0x123456789ABCDEF0;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
    assert_eq!(regs.rax, 0xDEADBEEFCAFEBABE);
    assert_eq!(regs.rbx, 0x123456789ABCDEF0);
}

/// Multiple PUSH2/POP2 operations
#[test]
fn test_push2_pop2_multiple() {
    // Two PUSH2 operations followed by reversed-operand POP2 operations.
    let code = [
        0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, // PUSH2 rax, rbx
        0x62, 0xF4, 0x6C, 0x18, 0xFF, 0xF1, // PUSH2 rcx, rdx
        0x62, 0xF4, 0x74, 0x18, 0x8F, 0xC2, // POP2 rdx, rcx
        0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC3, // POP2 rbx, rax
        0xF4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0x1111;
    regs.rbx = 0x2222;
    regs.rcx = 0x3333;
    regs.rdx = 0x4444;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
    assert_eq!(
        (regs.rax, regs.rbx, regs.rcx, regs.rdx),
        (0x1111, 0x2222, 0x3333, 0x4444)
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

/// PUSH2 with same register twice (allowed)
#[test]
fn test_push2_same_register() {
    // PUSH2 rax, rax
    let code = [
        0x62, 0xF4, 0x7C, 0x18, // vvvv = RAX
        0xFF, 0xF0, 0xF4,
    ];
    let mut regs = Registers::default();
    regs.rax = 0xDEADDEADDEADDEAD;
    let (mut vcpu, mem) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let mut stack = [0u8; 16];
    mem.read_slice(&mut stack, GuestAddress(STACK_ADDR - 16))
        .unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
    assert_eq!(u64::from_le_bytes(stack[..8].try_into().unwrap()), regs.rax);
    assert_eq!(u64::from_le_bytes(stack[8..].try_into().unwrap()), regs.rax);
}

/// POP2 with duplicate destinations is an architecturally invalid encoding.
#[test]
fn test_pop2_same_register() {
    // Set up stack, then POP2 rax, rax
    let code = [
        0x50, // PUSH rax
        0x50, // PUSH rax
        0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC0, 0xF4,
    ];
    let (mut vcpu, _) = setup_apx_vm_no_idt(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_pop2_memory_form_rejected_match_llvm() {
    // LLVM 23 rejects "pop2 [rax], rax"; POP2 only accepts register operands.
    // With no IDT, the injected #UD surfaces as an error instead of being handled.
    let code = [0x62, 0xF4, 0x7C, 0x18, 0x8F, 0x00, 0xF4];
    let (mut vcpu, _) = setup_apx_vm_no_idt(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_push2_memory_form_rejected_match_llvm() {
    // LLVM 23 rejects "push2 [rax], rax"; PUSH2 only accepts register operands.
    // With no IDT, the injected #UD surfaces as an error instead of being handled.
    let code = [0x62, 0xF4, 0x6C, 0x18, 0xFF, 0x30, 0xF4];
    let (mut vcpu, _) = setup_apx_vm_no_idt(&code, None);
    assert!(run_until_hlt(&mut vcpu).is_err());
}

#[test]
fn test_push2_pop2_rsp_operands_rejected_by_apx_spec() {
    for code in [
        &[0x62, 0xF4, 0x5C, 0x18, 0xFF, 0xF0, 0xF4][..],
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC4, 0xF4][..],
    ] {
        let (mut vcpu, _) = setup_apx_vm_no_idt(code, None);
        assert!(run_until_hlt(&mut vcpu).is_err());
    }
}

#[test]
fn test_push2_reserved_evex_payload_fields_rejected_by_apx_spec() {
    // Intel APX revision 8.0 permits only V4 and ND in EVEX payload byte 3,
    // requires ND=1, U=1, and pp=00. Each vector changes exactly one field
    // relative to LLVM 23's valid PUSH2 encoding.
    for (name, code) in [
        ("ND=0", &[0x62, 0xF4, 0x64, 0x08, 0xFF, 0xF0, 0xF4][..]),
        ("NF=1", &[0x62, 0xF4, 0x64, 0x1C, 0xFF, 0xF0, 0xF4][..]),
        ("z=1", &[0x62, 0xF4, 0x64, 0x98, 0xFF, 0xF0, 0xF4][..]),
        ("L=1", &[0x62, 0xF4, 0x64, 0x38, 0xFF, 0xF0, 0xF4][..]),
        ("aaa=1", &[0x62, 0xF4, 0x64, 0x19, 0xFF, 0xF0, 0xF4][..]),
        ("pp=01", &[0x62, 0xF4, 0x65, 0x18, 0xFF, 0xF0, 0xF4][..]),
        ("U=0", &[0x62, 0xF4, 0x60, 0x18, 0xFF, 0xF0, 0xF4][..]),
    ] {
        let (mut vcpu, _) = setup_apx_vm_no_idt(code, None);
        assert!(run_until_hlt(&mut vcpu).is_err(), "{name} must inject #UD");
    }
}

#[test]
fn test_push2_pop2_require_sixteen_byte_aligned_rsp() {
    for code in [
        &[0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, 0xF4][..],
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC3, 0xF4][..],
    ] {
        let mut regs = Registers {
            rsp: STACK_ADDR + 8,
            ..Registers::default()
        };
        regs.rax = 0x1111_2222_3333_4444;
        regs.rbx = 0xAAAA_BBBB_CCCC_DDDD;
        let (mut vcpu, _) = setup_apx_vm_no_idt(code, Some(regs));
        assert!(run_until_hlt(&mut vcpu).is_err());
    }
}

/// PUSH2 with zero values
#[test]
fn test_push2_zero_values() {
    let code = [0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, 0xF4];
    let mut regs = Registers::default();
    regs.rax = 0;
    regs.rbx = 0;
    let (mut vcpu, mem) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let mut stack = [0xFFu8; 16];
    mem.read_slice(&mut stack, GuestAddress(STACK_ADDR - 16))
        .unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
    assert_eq!(stack, [0; 16]);
}

/// PUSH2 with maximum values
#[test]
fn test_push2_max_values() {
    let code = [0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, 0xF4];
    let mut regs = Registers::default();
    regs.rax = u64::MAX;
    regs.rbx = u64::MAX;
    let (mut vcpu, mem) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let mut stack = [0u8; 16];
    mem.read_slice(&mut stack, GuestAddress(STACK_ADDR - 16))
        .unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
    assert_eq!(stack, [0xFF; 16]);
}

// ============================================================================
// Callee-Saved Register Pattern
// ============================================================================

/// Typical function prologue/epilogue pattern with PUSH2/POP2
#[test]
fn test_push2_function_prologue() {
    // PUSH2 pairs followed by reversed-operand POP2 pairs.
    let code = [
        0x62, 0xF4, 0x1C, 0x18, 0xFF, 0xF3, // PUSH2 rbx, r12
        0x62, 0xD4, 0x0C, 0x18, 0xFF, 0xF5, // PUSH2 r13, r14
        // Simulated function body (NOP)
        0x90, 0x62, 0xD4, 0x14, 0x18, 0x8F, 0xC6, // POP2 r14, r13
        0x62, 0xD4, 0x64, 0x18, 0x8F, 0xC4, // POP2 r12, rbx
        0xF4,
    ];
    let mut regs = Registers::default();
    regs.rbx = 0xBBBB;
    regs.r12 = 0x1212;
    regs.r13 = 0x1313;
    regs.r14 = 0x1414;
    let (mut vcpu, _) = setup_apx_vm(&code, Some(regs));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
    assert_eq!(
        (regs.rbx, regs.r12, regs.r13, regs.r14),
        (0xBBBB, 0x1212, 0x1313, 0x1414)
    );
}

// ============================================================================
// RSP Interaction Tests
// ============================================================================

/// PUSH2 modifies RSP correctly
#[test]
fn test_push2_rsp_modification() {
    let code = [0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, 0xF4];
    let (mut vcpu, _) = setup_apx_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR - 16);
}

/// POP2 modifies RSP correctly
#[test]
fn test_pop2_rsp_modification() {
    // Set up stack first
    let code = [
        0x50, // PUSH rax
        0x50, // PUSH rax
        0x62, 0xF4, 0x6C, 0x18, 0x8F, 0xC0, 0xF4,
    ];
    let (mut vcpu, _) = setup_apx_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.rsp, STACK_ADDR);
}

// ============================================================================
// Flag Preservation Tests
// ============================================================================

/// PUSH2 does not modify flags
#[test]
fn test_push2_preserves_flags() {
    // Set flags, then PUSH2
    let code = [
        0xF9, // STC (set CF)
        0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0, 0xF4,
    ];
    let (mut vcpu, _) = setup_apx_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_ne!(regs.rflags & 1, 0, "PUSH2 must preserve CF");
}

/// POP2 does not modify flags
#[test]
fn test_pop2_preserves_flags() {
    // Set flags, set up stack, then POP2
    let code = [
        0x50, // PUSH rax
        0x50, // PUSH rax
        0xF9, // STC (set CF)
        0x62, 0xF4, 0x6C, 0x18, 0x8F, 0xC0, 0xF4,
    ];
    let (mut vcpu, _) = setup_apx_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_ne!(regs.rflags & 1, 0, "POP2 must preserve CF");
}
