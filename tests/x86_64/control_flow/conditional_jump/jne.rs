use rax::cpu::Registers;

use crate::common::{run_until_hlt, setup_vm};

// JNE/JNZ - Jump if Not Equal / Jump if Not Zero
// Jumps to target if ZF = 0

// Basic JNE with ZF clear
#[test]
fn test_jne_taken_zf_clear() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (clears ZF)
        0x75, 0x02, // JNE +2 (should jump)
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 2, "RAX should be 2");
}

// JNE with ZF set (should not jump)
#[test]
fn test_jne_not_taken_zf_set() {
    let code = [
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1 (sets ZF)
        0x75, 0x05, // JNE +5 (should not jump)
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42 (should execute)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x42, "RAX should be 0x42");
}

// JNZ (alias for JNE) with ZF clear
#[test]
fn test_jnz_taken() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (clears ZF)
        0x75, 0x02, // JNZ +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE forward jump
#[test]
fn test_jne_forward() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x48, 0x85, 0xc0, // TEST RAX, RAX (clears ZF)
        0x75, 0x08, // JNE +8
        0x48, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff, // MOV RAX, -1 (skipped)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 5, "RAX should remain 5");
}

// JNE backward jump (loop)
#[test]
fn test_jne_backward_loop() {
    let code = [
        0x48, 0xc7, 0xc1, 0x03, 0x00, 0x00, 0x00, // MOV RCX, 3
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0 (loop start, offset 11)
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1
        0x48, 0x83, 0xe9, 0x01, // SUB RCX, 1
        0x75, 0xf5, // JNZ -11 (back to loop start if RCX != 0)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 3, "RAX should be 3");
    assert_eq!(regs.rcx, 0, "RCX should be 0");
}

// JNE with CMP instruction (not equal)
#[test]
fn test_jne_after_cmp_not_equal() {
    let code = [
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x48, 0xc7, 0xc3, 0x43, 0x00, 0x00, 0x00, // MOV RBX, 0x43
        0x48, 0x39, 0xd8, // CMP RAX, RBX (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE with CMP instruction (equal)
#[test]
fn test_jne_after_cmp_equal() {
    let code = [
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x48, 0xc7, 0xc3, 0x42, 0x00, 0x00, 0x00, // MOV RBX, 0x42
        0x48, 0x39, 0xd8, // CMP RAX, RBX (sets ZF)
        0x75, 0x05, // JNE +5 (should not jump)
        0x48, 0xc7, 0xc0, 0x99, 0x00, 0x00, 0x00, // MOV RAX, 0x99
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x99);
}

// JNE with SUB resulting in non-zero
#[test]
fn test_jne_after_sub_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x48, 0x83, 0xe8, 0x03, // SUB RAX, 3 (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 2);
}

// JNE preserves all registers
#[test]
fn test_jne_preserves_registers() {
    let code = [
        0x48, 0xc7, 0xc0, 0x11, 0x00, 0x00, 0x00, // MOV RAX, 0x11
        0x48, 0xc7, 0xc3, 0x22, 0x00, 0x00, 0x00, // MOV RBX, 0x22
        0x48, 0xc7, 0xc1, 0x33, 0x00, 0x00, 0x00, // MOV RCX, 0x33
        0x48, 0xc7, 0xc2, 0x01, 0x00, 0x00, 0x00, // MOV RDX, 1
        0x48, 0x85, 0xd2, // TEST RDX, RDX (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x11, "RAX preserved");
    assert_eq!(regs.rbx, 0x22, "RBX preserved");
    assert_eq!(regs.rcx, 0x33, "RCX preserved");
    assert_eq!(regs.rdx, 1, "RDX is 1");
}

// JNE does not affect flags
#[test]
fn test_jne_preserves_flags() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (clears ZF)
        0x75, 0x02, // JNE +2 (does not modify flags)
        0xf4, 0xf4, // HLT, HLT
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert!(regs.rflags & 0x40 == 0, "ZF should remain clear");
}

// JNE with maximum forward offset (127 bytes)
#[test]
fn test_jne_max_forward_offset() {
    let mut code = vec![
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (clears ZF)
        0x75, 0x7f, // JNE +127
    ];
    // Add 127 bytes of padding
    code.resize(12 + 127, 0x90); // NOP padding
    code.push(0xf4); // HLT at offset 139

    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE with maximum backward offset (-128 bytes)
#[test]
fn test_jne_max_backward_offset() {
    let mut code = vec![];
    // HLT at start
    code.push(0xf4);
    // Add padding to reach -128 offset
    code.resize(129, 0x90); // NOPs
    // Setup ZF and jump back
    code.extend_from_slice(&[
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (clears ZF)
        0x75, 0x80, // JNE -128 (back to HLT)
    ]);

    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE with zero offset (jumps to next instruction)
#[test]
fn test_jne_zero_offset() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (clears ZF)
        0x75, 0x00, // JNE +0 (next instruction)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE in if-then-else pattern
#[test]
fn test_jne_if_then_else() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x48, 0xc7, 0xc3, 0x06, 0x00, 0x00, 0x00, // MOV RBX, 6
        0x48, 0x39, 0xd8, // CMP RAX, RBX (clears ZF)
        0x75, 0x08, // JNE +8 (to then branch)
        // else branch:
        0x48, 0xc7, 0xc1, 0x00, 0x00, 0x00, 0x00, // MOV RCX, 0
        0xeb, 0x07, // JMP +7 (skip then branch)
        // then branch:
        0x48, 0xc7, 0xc1, 0x01, 0x00, 0x00, 0x00, // MOV RCX, 1
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 1, "Then branch executed");
}

// JNE with DEC instruction
#[test]
fn test_jne_after_dec_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x02, 0x00, 0x00, 0x00, // MOV RAX, 2
        0x48, 0xff, 0xc8, // DEC RAX (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 1);
}

// JNE with INC instruction
#[test]
fn test_jne_after_inc_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0
        0x48, 0xff, 0xc0, // INC RAX (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 1);
}

// JNE chained with other conditional jumps
#[test]
fn test_jne_chained() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x85, 0xc0, // TEST RAX, RAX (clears ZF)
        0x75, 0x05, // JNE +5
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1 (skipped)
        // jumped here
        0x48, 0xc7, 0xc3, 0x02, 0x00, 0x00, 0x00, // MOV RBX, 2
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 2);
}

// JNE with AND instruction
#[test]
fn test_jne_after_and_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x0f, 0x00, 0x00, 0x00, // MOV RAX, 0x0F
        0x48, 0x25, 0x03, 0x00, 0x00, 0x00, // AND RAX, 0x03 (result 3, clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 3);
}

// JNE with OR instruction
#[test]
fn test_jne_after_or_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x09, 0xc0, // OR RAX, RAX (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE practical: loop counter
#[test]
fn test_jne_loop_counter() {
    let code = [
        0x48, 0xc7, 0xc1, 0x05, 0x00, 0x00, 0x00, // MOV RCX, 5 (counter)
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0 (sum)
        // loop (offset 14):
        0x48, 0x83, 0xc0, 0x01, // ADD RAX, 1
        0x48, 0x83, 0xe9, 0x01, // SUB RCX, 1
        0x75, 0xf5, // JNE -11 (loop while RCX != 0)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 5, "Sum is 5");
    assert_eq!(regs.rcx, 0, "Counter is 0");
}

// JNE with 32-bit operands (EAX)
#[test]
fn test_jne_32bit() {
    let code = [
        0xb8, 0x01, 0x00, 0x00, 0x00, // MOV EAX, 1
        0x85, 0xc0, // TEST EAX, EAX (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE with 16-bit operands (AX)
#[test]
fn test_jne_16bit() {
    let code = [
        0x66, 0xb8, 0x01, 0x00, // MOV AX, 1
        0x66, 0x85, 0xc0, // TEST AX, AX (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE with 8-bit operands (AL)
#[test]
fn test_jne_8bit() {
    let code = [
        0xb0, 0x01, // MOV AL, 1
        0x84, 0xc0, // TEST AL, AL (clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE with NEG instruction resulting in non-zero
#[test]
fn test_jne_after_neg_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x05, 0x00, 0x00, 0x00, // MOV RAX, 5
        0x48, 0xf7, 0xd8, // NEG RAX (-5, clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let _ = run_until_hlt(&mut vcpu).unwrap();
}

// JNE in state machine pattern
#[test]
fn test_jne_state_machine() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (state)
        0x48, 0xc7, 0xc3, 0x00, 0x00, 0x00, 0x00, // MOV RBX, 0 (result)
        // check state 0:
        0x48, 0x83, 0xf8, 0x00, // CMP RAX, 0 (offset 18)
        0x75, 0x08, // JNE +8 (not state 0)
        0x48, 0xc7, 0xc3, 0x42, 0x00, 0x00, 0x00, // MOV RBX, 0x42 (state 0)
        0xeb, 0x0f, // JMP +15 (exit)
        // not state 0, check state 1:
        0x48, 0x83, 0xf8, 0x01, // CMP RAX, 1
        0x75, 0x08, // JNE +8 (not state 1)
        0x48, 0xc7, 0xc3, 0x99, 0x00, 0x00, 0x00, // MOV RBX, 0x99 (state 1)
        0xeb, 0x00, // JMP +0 (exit)
        // exit:
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 0x99, "State 1 handled");
}

// JNE with while loop pattern
#[test]
fn test_jne_while_loop() {
    let code = [
        0x48, 0xc7, 0xc0, 0x0a, 0x00, 0x00, 0x00, // MOV RAX, 10
        0x48, 0xc7, 0xc3, 0x00, 0x00, 0x00, 0x00, // MOV RBX, 0
        // loop (offset 14):
        0x48, 0x85, 0xc0, // TEST RAX, RAX
        0x74, 0x08, // JE +8 (exit if RAX == 0)
        0x48, 0x83, 0xc3, 0x01, // ADD RBX, 1
        0x48, 0x83, 0xe8, 0x01, // SUB RAX, 1
        0xeb, 0xf1, // JMP -15 (loop)
        // exit:
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0);
    assert_eq!(regs.rbx, 10);
}

// JNE with shift instruction resulting in non-zero
#[test]
fn test_jne_after_shift_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0xd1, 0xe0, // SHL RAX, 1 (result 2, clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 2);
}

// JNE multiple sequential checks
#[test]
fn test_jne_multiple_checks() {
    let code = [
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1
        0x48, 0x83, 0xf8, 0x00, // CMP RAX, 0
        0x75, 0x05, // JNE +5 (first check passed)
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1 (skipped)
        // first check passed:
        0x48, 0xc7, 0xc1, 0x02, 0x00, 0x00, 0x00, // MOV RCX, 2
        0x48, 0x83, 0xf9, 0x00, // CMP RCX, 0
        0x75, 0x05, // JNE +5 (second check passed)
        0x48, 0xc7, 0xc3, 0x02, 0x00, 0x00, 0x00, // MOV RBX, 2 (skipped)
        // second check passed:
        0x48, 0xc7, 0xc3, 0x03, 0x00, 0x00, 0x00, // MOV RBX, 3
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rbx, 3, "Both checks passed");
}

// JNE early exit from nested loops
#[test]
fn test_jne_early_exit() {
    let code = [
        0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00, // MOV RAX, 0 (found flag)
        0x48, 0xc7, 0xc1, 0x05, 0x00, 0x00, 0x00, // MOV RCX, 5 (counter)
        // loop (offset 14):
        0x48, 0x83, 0xf9, 0x03, // CMP RCX, 3
        0x75, 0x05, // JNE +5 (not found)
        0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00, // MOV RAX, 1 (found!)
        // check if found:
        0x48, 0x85, 0xc0, // TEST RAX, RAX (offset 28)
        0x75, 0x05, // JNE +5 (exit if found)
        0x48, 0x83, 0xe9, 0x01, // SUB RCX, 1
        0xeb, 0xe9, // JMP -23 (loop)
        // exit:
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 1, "Found");
    assert_eq!(regs.rcx, 3, "Stopped at 3");
}

// JNE with XOR instruction
#[test]
fn test_jne_after_xor_nonzero() {
    let code = [
        0x48, 0xc7, 0xc0, 0x0f, 0x00, 0x00, 0x00, // MOV RAX, 0x0F
        0x48, 0x35, 0x03, 0x00, 0x00, 0x00, // XOR RAX, 0x03 (result 0x0C, clears ZF)
        0x75, 0x02, // JNE +2
        0xf4, 0xf4, // HLT, HLT (should not execute)
        0xf4, // HLT (target)
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x0C);
}
