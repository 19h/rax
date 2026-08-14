//! Direct-decoder prefix, APX-admission, and exception-priority coverage for
//! the dedicated one-byte `INT3` (`CC`) encoding.

use crate::common::{
    Bytes, CODE_ADDR, GuestAddress, INT_HANDLER_ADDR, run_until_hlt, setup_vm, setup_vm_no_idt,
};
use rax::vm::vcpu::VCpu;

#[test]
fn int3_ignored_prefix_classes_save_the_exact_post_instruction_rip() {
    let encodings: &[(&str, &[u8])] = &[
        ("bare", &[0xCC]),
        ("ES", &[0x26, 0xCC]),
        ("CS", &[0x2E, 0xCC]),
        ("SS", &[0x36, 0xCC]),
        ("DS", &[0x3E, 0xCC]),
        ("FS", &[0x64, 0xCC]),
        ("GS", &[0x65, 0xCC]),
        ("operand-size", &[0x66, 0xCC]),
        ("address-size", &[0x67, 0xCC]),
        ("REX", &[0x40, 0xCC]),
        ("REX.WRB", &[0x4B, 0xCC]),
        ("REPNE", &[0xF2, 0xCC]),
        ("REP", &[0xF3, 0xCC]),
        (
            "ordered prefix stack",
            &[0x66, 0x67, 0xF3, 0x2E, 0x48, 0xCC],
        ),
    ];

    for &(name, instruction) in encodings {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let (mut vcpu, memory) = setup_vm(&code, None);
        memory
            .write_slice(
                &[0x48, 0x8B, 0x04, 0x24, 0x48, 0xCF],
                GuestAddress(INT_HANDLER_ADDR),
            )
            .unwrap();

        let regs = run_until_hlt(&mut vcpu).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            regs.rax,
            CODE_ADDR + instruction.len() as u64,
            "{name}: #BP must save the post-INT3 RIP"
        );
    }
}

#[test]
fn every_rex2_map_zero_payload_is_apx_gated_before_int3_delivery() {
    for payload in 0_u8..=0x7F {
        let instruction = [0xD5, payload, 0xCC];

        let (mut disabled, _) = setup_vm_no_idt(&instruction, None);
        let disabled_error = disabled
            .step()
            .expect_err("APX-disabled REX2 INT3 must raise #UD")
            .to_string();
        assert!(
            disabled_error.contains("IDT entry 6 not present"),
            "payload {payload:#04x}: {disabled_error}"
        );
        assert_eq!(disabled.get_regs().unwrap().rip, CODE_ADDR);

        let (mut enabled, _) = setup_vm_no_idt(&instruction, None);
        enabled.set_apx_enabled(true);
        let enabled_error = enabled
            .step()
            .expect_err("APX-enabled REX2 INT3 must deliver #BP")
            .to_string();
        assert!(
            enabled_error.contains("IDT entry 3 not present"),
            "payload {payload:#04x}: {enabled_error}"
        );
        assert_eq!(enabled.get_regs().unwrap().rip, CODE_ADDR);
    }
}

#[test]
fn int3_lock_and_rex_before_rex2_raise_fault_class_invalid_opcode() {
    for (name, instruction, apx_enabled) in [
        ("LOCK", &[0xF0, 0xCC][..], false),
        ("LOCK REX2", &[0xF0, 0xD5, 0x00, 0xCC], true),
        ("REX before REX2", &[0x48, 0xD5, 0x00, 0xCC], true),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(instruction, None);
        vcpu.set_apx_enabled(apx_enabled);
        for path in ["cold decode", "decode-cache hit"] {
            let error = vcpu
                .step()
                .expect_err("invalid INT3 prefix must raise #UD")
                .to_string();
            assert!(
                error.contains("IDT entry 6 not present"),
                "{name} ({path}): {error}"
            );
            assert_eq!(
                vcpu.get_regs().unwrap().rip,
                CODE_ADDR,
                "{name} ({path}): #UD must retain the instruction RIP"
            );
        }
    }
}
