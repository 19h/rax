//! Disabled WRMSRNS, RDMSRLIST, and WRMSRLIST profile coverage.
//!
//! Intel SDM encodings: NP/F2/F3 0F 01 C6. The fixed guest profile keeps
//! CPUID.07H.01H:EAX[19] (WRMSRNS) and EAX[27] (MSRLIST) clear.

use crate::common::*;
use rax::vm::vcpu::Registers;

#[derive(Clone, Copy)]
enum ExecutionMode {
    Long { cpl: u16 },
    Compatibility,
    Protected,
    Real,
    Virtual8086,
}

#[test]
fn test_msr_extension_features_are_not_enumerated() {
    let code = [
        0xB8, 0x07, 0x00, 0x00, 0x00, // MOV EAX, 7
        0xB9, 0x01, 0x00, 0x00, 0x00, // MOV ECX, 1
        0x0F, 0xA2, // CPUID
        0xF4,
    ];
    let (mut vcpu, _) = setup_vm(&code, None);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(
        regs.rax & ((1 << 19) | (1 << 27)),
        0,
        "CPUID must not advertise WRMSRNS or MSRLIST"
    );
}

fn assert_disabled_msr_extension_ud(name: &str, bytes: &[u8], apx: bool, mode: ExecutionMode) {
    let initial = Registers {
        rax: 0xAAAA_BBBB_CCCC_DDDD,
        rbx: 0x1111_2222_3333_4444,
        rcx: u64::MAX,
        rdx: 0x5555_6666_7777_8888,
        // Unaligned, non-canonical, and unmapped table addresses must remain
        // unobserved while MSRLIST is disabled.
        rsi: 0x0000_8000_0000_0001,
        rdi: 0xFFFF_7FFF_FFFF_FFF9,
        rsp: STACK_ADDR,
        rbp: 0x9999_AAAA_BBBB_CCCC,
        r16: 0x1616_1616_1616_1616,
        r31: 0x3131_3131_3131_3131,
        rflags: 0x0CD7,
        xmm: [[0x1111_2222_3333_4444, 0xAAAA_BBBB_CCCC_DDDD]; 16],
        ..Registers::default()
    };
    let (mut vcpu, _) = if apx {
        setup_apx_vm_no_idt(bytes, Some(initial))
    } else {
        setup_vm_no_idt(bytes, Some(initial))
    };
    let mut sregs = vcpu.get_sregs().unwrap();
    let (long_mode, default_32, protected_mode, cpl, vm86) = match mode {
        ExecutionMode::Long { cpl } => (true, false, true, cpl, false),
        ExecutionMode::Compatibility => (false, true, true, 0, false),
        ExecutionMode::Protected => (false, true, true, 0, false),
        ExecutionMode::Real => (false, false, false, 0, false),
        ExecutionMode::Virtual8086 => (false, false, true, 3, true),
    };
    sregs.cs.l = long_mode;
    sregs.cs.db = default_32;
    sregs.cs.selector = cpl;
    if protected_mode {
        sregs.cr0 |= 1;
    } else {
        sregs.cr0 &= !1;
    }
    if matches!(
        mode,
        ExecutionMode::Long { .. } | ExecutionMode::Compatibility
    ) {
        sregs.efer |= 1 << 10;
    } else {
        sregs.efer &= !(1 << 10);
    }
    sregs.cr2 = 0x2222_0000;
    sregs.cr3 = 0x3333_0000;
    sregs.cr4 |= 0x4444;
    sregs.cr8 = 0x8;
    sregs.fs.base = 0x0000_1111_2222_3333;
    sregs.gs.base = 0x0000_4444_5555_6666;
    vcpu.set_sregs(&sregs).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    if vm86 {
        regs.rflags |= 1 << 17;
    } else {
        regs.rflags &= !(1 << 17);
    }
    vcpu.set_regs(&regs).unwrap();
    let before =
        serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();

    let error = vcpu
        .step()
        .expect_err("disabled MSR extension must inject #UD");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "{name}: expected #UD before mode, CPL, MSR, or list-memory checks, got {error}"
    );
    let after =
        serde_json::to_value((vcpu.get_regs().unwrap(), vcpu.get_sregs().unwrap())).unwrap();
    assert_eq!(after, before, "{name}");
}

#[test]
fn test_disabled_msr_extension_aliases_fault_before_operands_or_state_commit() {
    for (name, bytes) in [
        ("WRMSRNS", &[0x0F, 0x01, 0xC6][..]),
        ("RDMSRLIST", &[0xF2, 0x0F, 0x01, 0xC6][..]),
        ("WRMSRLIST", &[0xF3, 0x0F, 0x01, 0xC6][..]),
        ("66-prefixed 0F 01 C6", &[0x66, 0x0F, 0x01, 0xC6][..]),
    ] {
        for cpl in [0, 3] {
            assert_disabled_msr_extension_ud(name, bytes, false, ExecutionMode::Long { cpl });
        }
    }

    for (name, bytes, mode) in [
        (
            "WRMSRNS protected mode",
            &[0x0F, 0x01, 0xC6][..],
            ExecutionMode::Protected,
        ),
        (
            "WRMSRNS real mode",
            &[0x0F, 0x01, 0xC6][..],
            ExecutionMode::Real,
        ),
        (
            "WRMSRNS virtual-8086 mode",
            &[0x0F, 0x01, 0xC6][..],
            ExecutionMode::Virtual8086,
        ),
        (
            "WRMSRNS compatibility mode",
            &[0x0F, 0x01, 0xC6][..],
            ExecutionMode::Compatibility,
        ),
        (
            "RDMSRLIST compatibility mode",
            &[0xF2, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Compatibility,
        ),
        (
            "RDMSRLIST protected mode",
            &[0xF2, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Protected,
        ),
        (
            "RDMSRLIST real mode",
            &[0xF2, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Real,
        ),
        (
            "RDMSRLIST virtual-8086 mode",
            &[0xF2, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Virtual8086,
        ),
        (
            "WRMSRLIST compatibility mode",
            &[0xF3, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Compatibility,
        ),
        (
            "WRMSRLIST protected mode",
            &[0xF3, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Protected,
        ),
        (
            "WRMSRLIST real mode",
            &[0xF3, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Real,
        ),
        (
            "WRMSRLIST virtual-8086 mode",
            &[0xF3, 0x0F, 0x01, 0xC6][..],
            ExecutionMode::Virtual8086,
        ),
    ] {
        assert_disabled_msr_extension_ud(name, bytes, false, mode);
    }
}

#[test]
fn test_disabled_msr_extensions_accept_ignored_legacy_prefixes_before_ud() {
    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67, 0x40, 0x48, 0x4F] {
        assert_disabled_msr_extension_ud(
            "ignored legacy prefix",
            &[prefix, 0x0F, 0x01, 0xC6],
            false,
            ExecutionMode::Long { cpl: 0 },
        );
    }
}

#[test]
fn test_disabled_msr_extension_lock_vector_and_rex2_forms_are_precise_ud() {
    for (name, bytes, apx) in [
        ("LOCK", &[0xF0, 0x0F, 0x01, 0xC6][..], false),
        ("VEX2", &[0xC5, 0xF8, 0x01, 0xC6][..], false),
        ("VEX3", &[0xC4, 0xE1, 0x78, 0x01, 0xC6][..], false),
        ("EVEX", &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC6][..], false),
        ("REX2", &[0xD5, 0x80, 0x01, 0xC6][..], true),
    ] {
        assert_disabled_msr_extension_ud(name, bytes, apx, ExecutionMode::Long { cpl: 3 });
    }
}
