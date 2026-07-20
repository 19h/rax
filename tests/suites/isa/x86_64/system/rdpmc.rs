//! Architectural RDPMC coverage for the deterministic legacy-PMU profile.

use crate::common::{
    CODE_ADDR, VCpu, run_until_hlt, setup_apx_vm_no_idt, setup_vm, setup_vm_no_idt,
};
use rax::vm::vcpu::Registers;

const CR0_PE: u64 = 1;
const CR4_PCE: u64 = 1 << 8;
const RFLAGS_VM: u64 = 1 << 17;
const STATUS_FLAGS: u64 = 0x08D5;
const PMC_MASK: u64 = (1_u64 << 40) - 1;

fn set_pmc_controls(
    vcpu: &mut rax::isa::x86_64::X86_64Vcpu,
    protected_mode: bool,
    pce: bool,
    cpl: u16,
    virtual_8086: bool,
) {
    let mut sregs = vcpu.get_sregs().unwrap();
    if protected_mode {
        sregs.cr0 |= CR0_PE;
    } else {
        sregs.cr0 &= !CR0_PE;
    }
    if pce {
        sregs.cr4 |= CR4_PCE;
    } else {
        sregs.cr4 &= !CR4_PCE;
    }
    sregs.cs.selector = (sregs.cs.selector & !3) | cpl;
    sregs.cs.dpl = cpl as u8;
    sregs.ss.selector = (sregs.ss.selector & !3) | cpl;
    sregs.ss.dpl = cpl as u8;
    vcpu.set_sregs(&sregs).unwrap();

    let mut regs = vcpu.get_regs().unwrap();
    if virtual_8086 {
        regs.rflags |= RFLAGS_VM;
    } else {
        regs.rflags &= !RFLAGS_VM;
    }
    vcpu.set_regs(&regs).unwrap();
}

fn assert_gp_noncommitting(
    selector: u64,
    configure: impl FnOnce(&mut rax::isa::x86_64::X86_64Vcpu),
) {
    let initial = Registers {
        rax: 0x1111,
        rcx: selector,
        rdx: 0x3333,
        rbx: 0x4444,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x33], Some(initial));
    configure(&mut vcpu);

    let err = vcpu
        .step()
        .expect_err("invalid or unprivileged RDPMC must inject #GP(0)");
    assert!(
        err.to_string().contains("IDT entry 13 not present"),
        "expected #GP delivery failure, got {err}"
    );
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rip, CODE_ADDR);
    assert_eq!(regs.rax, 0x1111);
    assert_eq!(regs.rcx, selector);
    assert_eq!(regs.rdx, 0x3333);
    assert_eq!(regs.rbx, 0x4444);
}

#[test]
fn rdpmc_legacy_selectors_and_fast_mode_have_exact_widths() {
    for selector in (0_u64..8).chain((0_u64..8).map(|index| index | 0x8000_0000)) {
        let initial = Registers {
            rax: u64::MAX,
            rcx: selector,
            rdx: u64::MAX,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm(&[0x0F, 0x33, 0xF4], Some(initial));
        let regs = run_until_hlt(&mut vcpu).unwrap();
        let value = (regs.rdx << 32) | regs.rax;

        assert_eq!(regs.rax >> 32, 0, "selector={selector:#010x}");
        assert_eq!(regs.rdx >> 32, 0, "selector={selector:#010x}");
        assert_eq!(regs.rcx, selector, "RDPMC must preserve RCX");
        if selector & 0x8000_0000 != 0 {
            assert_eq!(regs.rdx, 0, "fast read must clear EDX");
            assert!(value <= u64::from(u32::MAX));
        } else {
            assert_eq!(value & !PMC_MASK, 0, "legacy PMC is exactly 40 bits");
        }
    }
}

#[test]
fn rdpmc_ignores_rcx_high_half_and_preserves_flags_and_nonoutputs() {
    let initial = Registers {
        rax: u64::MAX,
        rcx: 0xFFFF_FFFF_0000_0007,
        rdx: u64::MAX,
        rbx: 0x4242_4242_4242_4242,
        rsi: 0x2A2A_2A2A_2A2A_2A2A,
        rdi: 0x1919_1919_1919_1919,
        r8: 0x8888_8888_8888_8888,
        r15: 0x1515_1515_1515_1515,
        rflags: 0x2 | STATUS_FLAGS | (1 << 10),
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_vm(&[0x0F, 0x33, 0xF4], Some(initial));
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rcx, 0xFFFF_FFFF_0000_0007);
    assert_eq!(regs.rbx, 0x4242_4242_4242_4242);
    assert_eq!(regs.rsi, 0x2A2A_2A2A_2A2A_2A2A);
    assert_eq!(regs.rdi, 0x1919_1919_1919_1919);
    assert_eq!(regs.r8, 0x8888_8888_8888_8888);
    assert_eq!(regs.r15, 0x1515_1515_1515_1515);
    assert_eq!(
        regs.rflags & (STATUS_FLAGS | (1 << 10)),
        STATUS_FLAGS | (1 << 10)
    );
}

#[test]
fn rdpmc_invalid_legacy_selectors_raise_gp_without_committing() {
    for selector in [8, 0x2000_0000, 0x4000_0000, 0x8000_0008, u32::MAX as u64] {
        assert_gp_noncommitting(selector, |vcpu| {
            set_pmc_controls(vcpu, true, true, 3, false)
        });
    }
}

#[test]
fn rdpmc_privilege_gate_models_cpl_pce_real_mode_and_virtual_8086() {
    assert_gp_noncommitting(0, |vcpu| set_pmc_controls(vcpu, true, false, 3, false));
    assert_gp_noncommitting(0, |vcpu| set_pmc_controls(vcpu, true, false, 0, true));

    for (protected_mode, pce, cpl, virtual_8086) in [
        (true, false, 0, false),
        (true, true, 3, false),
        (true, true, 0, true),
        (false, false, 3, false),
    ] {
        let initial = Registers {
            rcx: 0,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x33], Some(initial));
        set_pmc_controls(&mut vcpu, protected_mode, pce, cpl, virtual_8086);
        assert!(vcpu.step().expect("permitted RDPMC").is_none());
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rip, CODE_ADDR + 2);
        assert_eq!(regs.rax >> 32, 0);
        assert_eq!(regs.rdx >> 32, 0);
    }
}

#[test]
fn rdpmc_ignores_legacy_and_rex_prefixes() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // ordinary REX and REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        let initial = Registers {
            rcx: 0,
            ..Registers::default()
        };
        let code = [prefix, 0x0F, 0x33];
        let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));
        assert!(vcpu.step().expect("ignored RDPMC prefix").is_none());
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR + code.len() as u64);
    }
}

#[test]
fn rdpmc_lock_and_rex2_raise_ud_before_dynamic_checks() {
    for code in [&[0xF0, 0x0F, 0x33][..], &[0xD5, 0x80, 0x33]] {
        let initial = Registers {
            rax: 0x1111,
            rcx: 8,
            rdx: 0x3333,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_apx_vm_no_idt(code, Some(initial));
        set_pmc_controls(&mut vcpu, true, false, 3, false);
        let err = vcpu.step().expect_err("LOCK/REX2 RDPMC must inject #UD");
        assert!(
            err.to_string().contains("IDT entry 6 not present"),
            "expected #UD delivery failure, got {err}"
        );
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rip, CODE_ADDR);
        assert_eq!(regs.rax, 0x1111);
        assert_eq!(regs.rcx, 8);
        assert_eq!(regs.rdx, 0x3333);
    }
}

#[test]
fn cpuid_leaf_a_reports_legacy_profile_without_architectural_pmu() {
    let initial = Registers {
        rax: 0x0A,
        rcx: 0,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_vm(&[0x0F, 0xA2, 0xF4], Some(initial));
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!([regs.rax, regs.rbx, regs.rcx, regs.rdx], [0, 0, 0, 0]);
}
