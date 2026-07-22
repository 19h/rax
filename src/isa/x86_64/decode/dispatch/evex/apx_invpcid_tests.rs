//! Direct-execution regressions for legacy and APX-promoted INVPCID.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DESCRIPTOR: u64 = 0x2000;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.sregs.cr4 = 1 << 17; // PCIDE permits nonzero descriptor PCIDs.
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_apx_enabled(true);
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    {
        vcpu.set_jit_mem(false);
        vcpu.set_jit_call(false);
    }
    vcpu
}

fn write_descriptor(memory: &GuestMemoryMmap, addr: u64, low: u64, linear: u64) {
    let mut descriptor = [0_u8; 16];
    descriptor[..8].copy_from_slice(&low.to_le_bytes());
    descriptor[8..].copy_from_slice(&linear.to_le_bytes());
    memory.write_slice(&descriptor, GuestAddress(addr)).unwrap();
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

fn assert_fault(code: &[u8], configure: impl FnOnce(&mut X86_64Vcpu), vector: u8) {
    let memory = memory_with_code(code);
    write_descriptor(&memory, DESCRIPTOR, 0, 0x4000);
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.r16 = 0;
    vcpu.regs.r17 = DESCRIPTOR;
    configure(&mut vcpu);
    let before = register_image(&vcpu);
    let error = format!("{:#}", vcpu.step().expect_err("INVPCID must fault"));
    if vector == 14 {
        // With paging disabled this test memory reports the inaccessible
        // descriptor as a host-backed guest-memory read failure. Paged JIT
        // coverage below checks the architectural #PF replay path itself.
        assert!(
            error.contains("failed to read") || error.contains("IDT entry 14 not present"),
            "wrong INVPCID memory fault: {error}"
        );
    } else {
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "wrong INVPCID exception: {error}"
        );
    }
    assert_eq!(register_image(&vcpu), before);
    assert_eq!(vcpu.regs.rip, 0);
}

#[test]
fn direct_apx_invpcid_accepts_wig_and_exact_egpr_sib_operands() {
    // LLVM 23: `{evex} invpcid r16, [r17]`; W is architecturally ignored.
    for code in [
        &[0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01][..],
        &[0x62, 0xEC, 0xFE, 0x08, 0xF2, 0x01],
    ] {
        let memory = memory_with_code(code);
        write_descriptor(&memory, DESCRIPTOR, 0x321, 0x4567);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.r16 = 1;
        vcpu.regs.r17 = DESCRIPTOR;
        let flags = vcpu.regs.rflags;

        assert!(vcpu.step().expect("APX INVPCID WIG").is_none());
        assert_eq!(vcpu.regs.rip, code.len() as u64);
        assert_eq!(vcpu.regs.r16, 1);
        assert_eq!(vcpu.regs.r17, DESCRIPTOR);
        assert_eq!(vcpu.regs.rflags, flags);
    }

    // LLVM 23: `{evex} invpcid r31, [r20 + 8*r29 + 64]`.
    let code = [0x62, 0x2C, 0x7A, 0x08, 0xF2, 0x7C, 0xEC, 0x40];
    let memory = memory_with_code(&code);
    write_descriptor(&memory, DESCRIPTOR, 0xFFF, 0x0000_8000_0000_0000);
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.r31 = 3; // Type 3 ignores both PCID and linear-address fields.
    vcpu.regs.r20 = DESCRIPTOR - 0x40;
    vcpu.regs.r29 = 0;

    assert!(vcpu.step().expect("APX INVPCID EGPR SIB").is_none());
    assert_eq!(vcpu.regs.rip, code.len() as u64);
    assert_eq!(vcpu.regs.r31, 3);

    // Address-size and FS overrides are the legacy prefixes permitted before
    // an extended-EVEX instruction. Address arithmetic truncates before FS.
    let code = [0x64, 0x67, 0x62, 0x2C, 0x7A, 0x08, 0xF2, 0x7C, 0xEC, 0x40];
    let memory = memory_with_code(&code);
    write_descriptor(&memory, 0x2040, 0, 0x4000);
    let mut vcpu = test_vcpu(memory);
    vcpu.sregs.fs.base = 0x1000;
    vcpu.regs.r20 = 0xFFFF_0000_0000_1000;
    vcpu.regs.r29 = 0;
    vcpu.regs.r31 = 2;

    assert!(vcpu.step().expect("APX INVPCID FS addr32").is_none());
    assert_eq!(vcpu.regs.rip, code.len() as u64);
}

#[test]
fn direct_legacy_invpcid_preserves_flags_and_accepts_all_defined_types() {
    // LLVM 23: `invpcid rax, [rbx]`.
    let code = [0x66, 0x0F, 0x38, 0x82, 0x03];
    for (invpcid_type, low, linear) in [
        (0, 0x123, 0x0000_7FFF_FFFF_F000),
        (1, 0xABC, 0x0000_8000_0000_0000),
        (2, 0xFFF, 0x0000_8000_0000_0000),
        (3, 0xFFF, 0xFFFF_7FFF_FFFF_FFFF),
    ] {
        let memory = memory_with_code(&code);
        write_descriptor(&memory, DESCRIPTOR, low, linear);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.rax = invpcid_type;
        vcpu.regs.rbx = DESCRIPTOR;
        let flags = vcpu.regs.rflags;

        assert!(vcpu.step().expect("legacy INVPCID").is_none());
        assert_eq!(vcpu.regs.rip, code.len() as u64);
        assert_eq!(vcpu.regs.rax, invpcid_type);
        assert_eq!(vcpu.regs.rbx, DESCRIPTOR);
        assert_eq!(vcpu.regs.rflags, flags);
    }
}

#[test]
fn direct_compatibility_mode_invpcid_uses_r32_and_32_bit_default_addresses() {
    let code = [0x66, 0x0F, 0x38, 0x82, 0x03];
    let memory = memory_with_code(&code);
    write_descriptor(&memory, DESCRIPTOR, 0xFFF, 0x0000_8000_0000_0000);
    let mut vcpu = test_vcpu(memory);
    vcpu.sregs.cs.l = false;
    vcpu.sregs.cs.db = true;
    vcpu.regs.rax = 0xFFFF_FFFF_0000_0002;
    vcpu.regs.rbx = 0xFFFF_0000_0000_2000;
    let flags = vcpu.regs.rflags;

    assert!(vcpu.step().expect("compatibility INVPCID r32").is_none());
    assert_eq!(vcpu.regs.rip, code.len() as u64);
    assert_eq!(vcpu.regs.rax, 0xFFFF_FFFF_0000_0002);
    assert_eq!(vcpu.regs.rbx, 0xFFFF_0000_0000_2000);
    assert_eq!(vcpu.regs.rflags, flags);
}

#[test]
fn direct_apx_invpcid_rejects_every_reserved_field_prefix_and_register_form() {
    let invalid = [
        (&[0x62, 0xEC, 0x7C, 0x08, 0xF2, 0x01][..], "pp"),
        (&[0x62, 0xEC, 0x7E, 0x18, 0xF2, 0x01][..], "ND"),
        (&[0x62, 0xEC, 0x7E, 0x0C, 0xF2, 0x01][..], "NF"),
        (&[0x62, 0xEC, 0x7E, 0x88, 0xF2, 0x01][..], "z"),
        (&[0x62, 0xEC, 0x7E, 0x28, 0xF2, 0x01][..], "LL"),
        (&[0x62, 0xEC, 0x7E, 0x09, 0xF2, 0x01][..], "aaa"),
        (&[0x62, 0xEC, 0x76, 0x08, 0xF2, 0x01][..], "V3:0"),
        (&[0x62, 0xEC, 0x7E, 0x00, 0xF2, 0x01][..], "V4"),
        (&[0x62, 0xEC, 0x7E, 0x08, 0xF2, 0xC1][..], "mod=3"),
        (&[0x66, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01][..], "66"),
        (&[0xF2, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01][..], "F2"),
        (&[0xF3, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01][..], "F3"),
        (&[0x40, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01][..], "REX"),
        (&[0xF0, 0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01][..], "LOCK"),
    ];

    for (code, name) in invalid {
        let memory = memory_with_code(code);
        write_descriptor(&memory, DESCRIPTOR, 0, 0x4000);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.r16 = 0;
        vcpu.regs.r17 = DESCRIPTOR;
        let before = register_image(&vcpu);
        let error = format!("{:#}", vcpu.step().expect_err(name));
        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        assert_eq!(register_image(&vcpu), before, "{name}");
    }
}

#[test]
fn direct_invpcid_fault_priority_and_descriptor_checks_are_noncommitting() {
    let apx = [0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01];
    assert_fault(
        &apx,
        |vcpu| {
            vcpu.set_apx_enabled(false);
            vcpu.sregs.cs.selector = 3;
            vcpu.regs.r17 = 0x2_0000;
        },
        6,
    );
    assert_fault(
        &apx,
        |vcpu| {
            vcpu.sregs.cs.selector = 3;
            vcpu.regs.r17 = 0x2_0000;
        },
        13,
    );
    assert_fault(&apx, |vcpu| vcpu.regs.r17 = 0x2_0000, 14);

    // [r20] selects SS by default; a noncanonical 16-byte source is #SS(0).
    let apx_ss = [0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x04, 0x24];
    assert_fault(&apx_ss, |vcpu| vcpu.regs.r20 = 0x0000_8000_0000_0000, 12);
    assert_fault(&apx, |vcpu| vcpu.regs.r17 = 0x0000_7FFF_FFFF_FFF8, 13);

    for legacy_invalid in [
        &[0xF0, 0x66, 0x0F, 0x38, 0x82, 0x03][..],
        &[0x66, 0x0F, 0x38, 0x82, 0xC3],
    ] {
        assert_fault(
            legacy_invalid,
            |vcpu| {
                vcpu.regs.rax = 0;
                vcpu.regs.rbx = DESCRIPTOR;
            },
            6,
        );
    }

    let legacy = [0x66, 0x0F, 0x38, 0x82, 0x03];
    for (name, invpcid_type, low, linear, pcide) in [
        ("type", 4, 0, 0x4000, true),
        ("reserved", 0, 1 << 12, 0x4000, true),
        ("PCIDE", 1, 1, 0x4000, false),
        ("linear", 0, 0, 0x0000_8000_0000_0000, true),
    ] {
        let memory = memory_with_code(&legacy);
        write_descriptor(&memory, DESCRIPTOR, low, linear);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.rax = invpcid_type;
        vcpu.regs.rbx = DESCRIPTOR;
        if !pcide {
            vcpu.sregs.cr4 &= !(1 << 17);
        }
        let before = register_image(&vcpu);
        let error = format!("{:#}", vcpu.step().expect_err(name));
        assert!(
            error.contains("IDT entry 13 not present"),
            "{name}: {error}"
        );
        assert_eq!(register_image(&vcpu), before, "{name}");
    }
}
