//! Tests for User-Mode Instruction Prevention (UMIP).

use rax::isa::x86_64::X86_64Vcpu;

use crate::common::{
    CR4_UMIP, DATA_ADDR, INT_HANDLER_ADDR, Registers, VCpu, enable_cr4_bits, run_until_hlt,
    setup_vm_no_idt, setup_vm_with_cr4,
};

fn make_cpl3(vcpu: &mut X86_64Vcpu) {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.selector = 0x1b;
    sregs.cs.dpl = 3;
    sregs.ss.selector = 0x23;
    sregs.ss.dpl = 3;
    vcpu.set_sregs(&sregs).unwrap();
}

fn faulted_to_handler(vcpu: &mut X86_64Vcpu) -> bool {
    let _ = vcpu.step();
    vcpu.get_regs().unwrap().rip == INT_HANDLER_ADDR
}

fn user_mode_no_idt_step_error(insn: &[u8]) -> String {
    let mut code = Vec::from(insn);
    code.push(0xf4);
    let regs = Registers {
        rax: DATA_ADDR,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(regs));
    enable_cr4_bits(&mut vcpu, CR4_UMIP);
    make_cpl3(&mut vcpu);

    match vcpu.step() {
        Err(err) => format!("{err:?}"),
        Ok(exit) => panic!("expected exception delivery error, got {exit:?}"),
    }
}

#[test]
fn test_umip_blocks_user_mode_store_state_instructions() {
    let cases: &[(&str, &[u8])] = &[
        ("sgdt_m", &[0x0f, 0x01, 0x00]),
        ("sidt_m", &[0x0f, 0x01, 0x08]),
        ("sldt_r32", &[0x0f, 0x00, 0xc0]),
        ("str_r32", &[0x0f, 0x00, 0xc8]),
        ("smsw_r32", &[0x0f, 0x01, 0xe0]),
    ];

    for (name, insn) in cases {
        let mut code = Vec::from(*insn);
        code.push(0xf4);
        let regs = Registers {
            rax: DATA_ADDR,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_with_cr4(&code, Some(regs), CR4_UMIP);
        make_cpl3(&mut vcpu);

        assert!(
            faulted_to_handler(&mut vcpu),
            "{name} at CPL3 with CR4.UMIP set should raise #GP"
        );
    }
}

#[test]
fn test_umip_preserves_ud_for_invalid_sgdt_sidt_register_forms() {
    let cases: &[(&str, &[u8])] = &[
        ("sgdt_r64_invalid", &[0x0f, 0x01, 0xc0]),
        ("sidt_r64_invalid", &[0x0f, 0x01, 0xcc]),
    ];

    for (name, insn) in cases {
        let err = user_mode_no_idt_step_error(insn);
        assert!(
            err.contains("IDT entry 6 not present"),
            "{name} at CPL3 with CR4.UMIP set should preserve #UD precedence, got {err}"
        );
    }
}

#[test]
fn test_group6_real_and_virtual_8086_modes_raise_ud_before_umip_or_memory() {
    for (name, virtual_8086) in [("real", false), ("virtual-8086", true)] {
        for (instruction, selector_name) in [
            (&[0x0F, 0x00, 0x00][..], "SLDT"),
            (&[0x0F, 0x00, 0x08][..], "STR"),
            (&[0x0F, 0x00, 0x10][..], "LLDT"),
            (&[0x0F, 0x00, 0x18][..], "LTR"),
            (&[0x0F, 0x00, 0x20][..], "VERR"),
            (&[0x0F, 0x00, 0x28][..], "VERW"),
        ] {
            let regs = Registers {
                rax: 0x0200_0000,
                rflags: 0x2
                    | if virtual_8086 {
                        rax::isa::x86_64::flags::bits::VM
                    } else {
                        0
                    },
                ..Registers::default()
            };
            let (mut vcpu, _) = setup_vm_no_idt(instruction, Some(regs));
            let mut sregs = vcpu.get_sregs().unwrap();
            if !virtual_8086 {
                sregs.cr0 &= !1;
                // Real mode IVT entries do not carry a present bit.
                sregs.idt.limit = 0;
            }
            sregs.cr4 |= CR4_UMIP;
            sregs.cs.selector = 3;
            sregs.cs.dpl = 3;
            vcpu.set_sregs(&sregs).unwrap();

            let error = format!("{:?}", vcpu.step().expect_err("Group 6 must raise #UD"));
            assert!(
                error.contains("IDT entry 6 not present"),
                "{selector_name} in {name} mode must raise #UD before UMIP or the unmapped destination: {error}"
            );
            assert_eq!(vcpu.get_regs().unwrap().rip, crate::common::CODE_ADDR);
        }
    }
}

#[test]
fn test_umip_does_not_block_cpl0_store_state_instructions() {
    let mut code = vec![0x48, 0xb8];
    code.extend_from_slice(&DATA_ADDR.to_le_bytes()); // mov rax, DATA_ADDR
    code.extend_from_slice(&[
        0x0f, 0x01, 0x00, // sgdt [rax]
        0x0f, 0x01, 0x48, 0x10, // sidt [rax+0x10]
        0x0f, 0x00, 0xc2, // sldt edx
        0x0f, 0x00, 0xcb, // str ebx
        0x0f, 0x01, 0xe1, // smsw ecx
        0xf4,
    ]);

    let (mut vcpu, _) = setup_vm_with_cr4(&code, None, CR4_UMIP);
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx & 0xffff, 0x0033);
}
