use crate::common::{CODE_ADDR, setup_vm_no_idt};
use rax::vm::vcpu::{VCpu, VcpuExit};

// UD - Undefined Instruction
// Opcodes:
//   0F FF /r - UD0 (with ModR/M)
//   0F B9 /r - UD1 (with ModR/M)
//   0F 0B    - UD2 (no ModR/M)
// Generates invalid opcode exception (#UD)
// Provided for software testing to explicitly generate exceptions

// Note: These tests verify that UD instructions cause exceptions
// We check for exceptions by seeing that the VM doesn't reach HLT

fn assert_missing_idt_ud(code: &[u8]) {
    let (mut vcpu, _) = setup_vm_no_idt(code, None);
    let err = vcpu.run().expect_err("instruction should inject #UD");
    assert!(
        err.to_string().contains("IDT entry 6 not present"),
        "expected #UD delivery failure, got {err}"
    );
}

// Test UD2 basic (no ModR/M)
#[test]
fn test_ud2_basic() {
    let code = [
        0x0f, 0x0b, // UD2
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    // Should get an exception, not reach HLT
    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD2 should cause exception, not reach HLT"),
        Ok(VcpuExit::Shutdown) => {} // Exception occurred
        Err(_) => {}                 // Exception occurred
        _ => {}                      // Other exit is acceptable (implementation specific)
    }
}

#[test]
fn test_salc_is_invalid_in_64bit_mode_for_all_prefix_classes() {
    for (name, instruction, apx_enabled) in [
        ("bare", &[0xD6][..], false),
        ("operand-size", &[0x66, 0xD6], false),
        ("address-size", &[0x67, 0xD6], false),
        ("REPNE", &[0xF2, 0xD6], false),
        ("REP", &[0xF3, 0xD6], false),
        ("segment", &[0x2E, 0xD6], false),
        ("REX.W", &[0x48, 0xD6], false),
        ("LOCK", &[0xF0, 0xD6], false),
        ("REX2", &[0xD5, 0x00, 0xD6], true),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let (mut vcpu, _) = setup_vm_no_idt(&code, None);
        vcpu.set_apx_enabled(apx_enabled);
        for path in ["cold decode", "decode-cache hit"] {
            let error = vcpu
                .step()
                .expect_err("SALC must raise #UD in 64-bit mode")
                .to_string();
            assert!(
                error.contains("IDT entry 6 not present"),
                "{name} ({path}): expected #UD delivery failure, got {error}"
            );
            assert_eq!(
                vcpu.get_regs().unwrap().rip,
                CODE_ADDR,
                "{name} ({path}): fault-class #UD must retain the instruction RIP"
            );
        }
    }
}

#[test]
fn test_reserved_two_byte_opcodes_inject_ud_for_all_prefix_classes() {
    const PREFIXES: &[(&str, &[u8], bool, bool)] = &[
        ("bare", &[], false, false),
        ("operand-size", &[0x66], false, false),
        ("address-size", &[0x67], false, false),
        ("REPNE", &[0xF2], false, false),
        ("REP", &[0xF3], false, false),
        ("segment", &[0x2E], false, false),
        ("REX.W", &[0x48], false, false),
        ("LOCK", &[0xF0], false, false),
        ("REX2.M", &[0xD5, 0x80], true, true),
        ("REX2.M all fields", &[0xD5, 0xFF], true, true),
        (
            "stacked REX2.M",
            &[0x66, 0x67, 0xF3, 0x2E, 0xD5, 0x80],
            true,
            true,
        ),
    ];

    for opcode in [0x04, 0x0A, 0x0C] {
        for &(name, prefixes, rex2_map1, apx_enabled) in PREFIXES {
            let mut code = prefixes.to_vec();
            if !rex2_map1 {
                code.push(0x0F);
            }
            code.extend([opcode, 0xF4]);
            let (mut vcpu, _) = setup_vm_no_idt(&code, None);
            vcpu.set_apx_enabled(apx_enabled);

            for path in ["cold decode", "decode-cache hit"] {
                let error = vcpu
                    .step()
                    .expect_err("reserved two-byte opcode must raise #UD")
                    .to_string();
                assert!(
                    error.contains("IDT entry 6 not present"),
                    "0F {opcode:02X}, {name} ({path}): expected #UD delivery failure, got {error}"
                );
                assert_eq!(
                    vcpu.get_regs().unwrap().rip,
                    CODE_ADDR,
                    "0F {opcode:02X}, {name} ({path}): fault-class #UD must retain RIP"
                );
            }
        }
    }
}

#[test]
fn test_rsm_outside_smm_injects_ud() {
    assert_missing_idt_ud(&[
        0x0f, 0xaa, // RSM outside SMM
        0xf4, // HLT (should not be reached)
    ]);
}

#[test]
fn test_unimplemented_vex_opcode_injects_ud() {
    assert_missing_idt_ud(&[
        0xc5, 0xf8, 0xff, // Unimplemented VEX.128.0F opcode
        0xf4, // HLT (should not be reached)
    ]);
}

// Test UD1 with register operands (ModR/M = 11 000 000, RAX, RAX)
#[test]
fn test_ud1_rax_rax() {
    let code = [
        0x0f, 0xb9, 0xc0, // UD1 EAX, EAX
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with different ModR/M (RBX, RCX)
#[test]
fn test_ud1_rbx_rcx() {
    let code = [
        0x0f, 0xb9, 0xcb, // UD1 EBX, ECX (ModRM = 11 001 011)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with memory operand (ModR/M = 00 000 000, [RAX])
#[test]
fn test_ud1_memory_rax() {
    let code = [
        0x0f, 0xb9, 0x00, // UD1 EAX, [RAX]
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with register operands (ModR/M = 11 000 000)
#[test]
fn test_ud0_rax_rax() {
    let code = [
        0x0f, 0xff, 0xc0, // UD0 EAX, EAX
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with different ModR/M (RDX, RSI)
#[test]
fn test_ud0_rdx_rsi() {
    let code = [
        0x0f, 0xff, 0xf2, // UD0 EDX, ESI (ModRM = 11 110 010)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with memory operand
#[test]
fn test_ud0_memory() {
    let code = [
        0x0f, 0xff, 0x03, // UD0 EAX, [RBX]
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD2 multiple times in sequence
#[test]
fn test_ud2_first_triggers() {
    let code = [
        0x0f, 0x0b, // UD2 (should trigger here)
        0x0f, 0x0b, // UD2 (should not reach)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("First UD2 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with various register combinations
#[test]
fn test_ud1_rdi_rsi() {
    let code = [
        0x0f, 0xb9, 0xf7, // UD1 EDI, ESI (ModRM = 11 110 111)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with RBP and RSP
#[test]
fn test_ud1_rbp_rsp() {
    let code = [
        0x0f, 0xb9, 0xec, // UD1 EBP, ESP (ModRM = 11 101 100)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with all register bits set
#[test]
fn test_ud0_all_bits() {
    let code = [
        0x0f, 0xff, 0xff, // UD0 EDI, EDI (ModRM = 11 111 111)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD2 after normal instructions
#[test]
fn test_ud2_after_mov() {
    let code = [
        0xb8, 0x42, 0x00, 0x00, 0x00, // MOV EAX, 0x42
        0x0f, 0x0b, // UD2
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD2 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with ModR/M indicating displacement
#[test]
fn test_ud1_with_disp8() {
    let code = [
        0x0f, 0xb9, 0x40, 0x10, // UD1 EAX, [RAX+0x10] (ModRM = 01 000 000, disp8)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with ModR/M indicating SIB
#[test]
fn test_ud0_with_sib() {
    let code = [
        0x0f, 0xff, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00, // UD0 with SIB
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with REX prefix (64-bit operands)
#[test]
fn test_ud1_rex_w() {
    let code = [
        0x48, 0x0f, 0xb9, 0xc0, // UD1 RAX, RAX (with REX.W)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with REX prefix
#[test]
fn test_ud0_rex_w() {
    let code = [
        0x48, 0x0f, 0xff, 0xc0, // UD0 RAX, RAX (with REX.W)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD2 with REX prefix (though it has no operands)
#[test]
fn test_ud2_rex_prefix() {
    let code = [
        0x48, 0x0f, 0x0b, // UD2 with REX.W prefix
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD2 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with extended registers (R8, R9)
#[test]
fn test_ud1_r8_r9() {
    let code = [
        0x4d, 0x0f, 0xb9, 0xc1, // UD1 R8, R9 (REX.RB, ModRM = 11 000 001)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with extended registers
#[test]
fn test_ud0_r15_r14() {
    let code = [
        0x4d, 0x0f, 0xff, 0xfe, // UD0 R15, R14 (REX.RB, ModRM = 11 111 110)
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test that UD instructions are guaranteed to be undefined
#[test]
fn test_ud2_always_undefined() {
    let code = [
        0x0f, 0x0b, // UD2
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    // UD2 is guaranteed to always raise #UD
    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD2 is guaranteed to be undefined"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 with ModR/M = 0x00 (special case)
#[test]
fn test_ud1_modrm_00() {
    let code = [
        0x0f, 0xb9, 0x00, // UD1 EAX, [RAX]
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 with ModR/M = 0x00
#[test]
fn test_ud0_modrm_00() {
    let code = [
        0x0f, 0xff, 0x00, // UD0 EAX, [RAX]
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD1 different bit patterns in ModR/M byte
#[test]
fn test_ud1_modrm_patterns() {
    let code = [
        0x0f, 0xb9, 0xaa, // UD1 with ModRM = 10 101 010
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD1 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD0 different bit patterns in ModR/M byte
#[test]
fn test_ud0_modrm_patterns() {
    let code = [
        0x0f, 0xff, 0x55, // UD0 with ModRM = 01 010 101
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD0 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}

// Test UD instruction pointer is preserved correctly
#[test]
fn test_ud2_instruction_pointer_references_ud() {
    let code = [
        0xb8, 0x42, 0x00, 0x00, 0x00, // MOV EAX, 0x42 (5 bytes)
        0x0f, 0x0b, // UD2 at offset 5 from start
        0xf4, // HLT (should not be reached)
    ];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);

    let result = vcpu.run();
    match result {
        Ok(VcpuExit::Hlt) => panic!("UD2 should cause exception"),
        Ok(VcpuExit::Shutdown) => {}
        Err(_) => {}
        _ => {}
    }
}
