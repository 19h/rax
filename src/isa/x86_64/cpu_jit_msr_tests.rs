//! Native x86-64 JIT differentials for MSR state and disabled MSR extensions.

use super::*;
use crate::isa::x86_64::execute::system::{
    IA32_APIC_BASE, IA32_BIOS_SIGN_ID, IA32_CSTAR, IA32_EFER, IA32_FMASK, IA32_FS_BASE,
    IA32_GS_BASE, IA32_KERNEL_GS_BASE, IA32_LSTAR, IA32_MISC_ENABLE, IA32_MISC_ENABLE_RESET,
    IA32_PAT, IA32_PLATFORM_ID, IA32_STAR, IA32_SYSENTER_CS, IA32_SYSENTER_EIP, IA32_SYSENTER_ESP,
    IA32_TSC, IA32_TSC_ADJUST, IA32_TSC_AUX, IA32_TSC_DEADLINE, IA32_UMWAIT_CONTROL, IA32_XSS,
};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const APIC_BASE_PROFILE_VALUE: u64 = (1 << 8) | (1 << 11) | 0xFEE0_0000;
const RFLAGS_VM: u64 = 1 << 17;
const PML4: u64 = 0x1000;
const PDPT: u64 = 0x2000;
const PD: u64 = 0x3000;
const PT: u64 = 0x4000;
const PAGE_FLAGS: u64 = 0x7; // Present | writable | user-accessible.

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    for (address, entry) in [
        (PML4, PDPT | PAGE_FLAGS),
        (PDPT, PD | PAGE_FLAGS),
        (PD, PT | PAGE_FLAGS),
    ] {
        memory
            .write_slice(&entry.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }
    for page in 0..16_u64 {
        let entry = page * 0x1000 | PAGE_FLAGS;
        memory
            .write_slice(&entry.to_le_bytes(), GuestAddress(PT + page * 8))
            .unwrap();
    }
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = (1 << 8) | (1 << 10) | (1 << 11);
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.tr.type_ = 9;
    vcpu.sregs.cr0 = 0x8005_0033;
    vcpu.sregs.cr3 = PML4;
    vcpu.sregs.cr4 = 1 << 5; // PAE is required by IA-32e paging.
    vcpu.sregs.star = 0x0023_0010_DEAD_BEEF;
    vcpu.sregs.lstar = 0xFFFF_8000_1234_5678;
    vcpu.sregs.cstar = 0xFFFF_8000_ABCD_EF01;
    vcpu.sregs.fmask = 0x0200;
    vcpu.sregs.sysenter_cs = 8;
    vcpu.sregs.sysenter_esp = 0x0000_7FFF_FFFF_E000;
    vcpu.sregs.sysenter_eip = 0xFFFF_8000_0000_2000;
    vcpu.sregs.fs.base = 0x0000_7FFF_1111_0000;
    vcpu.sregs.gs.base = 0;
    vcpu.kernel_gs_base = 0xFFFF_8000_2222_0000;
    vcpu.tsc_adjust = 0x1234_5678;
    vcpu.tsc_aux = 0x89AB_CDEF;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rbx = 0xBBBB_BBBB_BBBB_BBBB;
    vcpu.regs.rsi = 0x6666_6666_6666_6666;
    vcpu.regs.rdi = 0x7777_7777_7777_7777;
    vcpu.regs.r8 = 0x8888_8888_8888_8888;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.r16 = 0x1616_1616_1616_1616;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn msr_state(vcpu: &X86_64Vcpu) -> [u64; 17] {
    [
        vcpu.tsc_adjust,
        u64::from(vcpu.tsc_aux),
        vcpu.sregs.efer,
        vcpu.sregs.star,
        vcpu.sregs.lstar,
        vcpu.sregs.cstar,
        vcpu.sregs.fmask,
        vcpu.sregs.sysenter_cs,
        vcpu.sregs.sysenter_esp,
        vcpu.sregs.sysenter_eip,
        vcpu.misc_enable,
        vcpu.pat,
        vcpu.umwait_control,
        vcpu.sregs.fs.base,
        vcpu.sregs.gs.base,
        vcpu.kernel_gs_base,
        vcpu.sregs.cr0,
    ]
}

fn scalar_state(vcpu: &X86_64Vcpu) -> [u64; 34] {
    [
        vcpu.regs.rax,
        vcpu.regs.rbx,
        vcpu.regs.rcx,
        vcpu.regs.rdx,
        vcpu.regs.rsi,
        vcpu.regs.rdi,
        vcpu.regs.rsp,
        vcpu.regs.rbp,
        vcpu.regs.r8,
        vcpu.regs.r9,
        vcpu.regs.r10,
        vcpu.regs.r11,
        vcpu.regs.r12,
        vcpu.regs.r13,
        vcpu.regs.r14,
        vcpu.regs.r15,
        vcpu.regs.r16,
        vcpu.regs.r17,
        vcpu.regs.r18,
        vcpu.regs.r19,
        vcpu.regs.r20,
        vcpu.regs.r21,
        vcpu.regs.r22,
        vcpu.regs.r23,
        vcpu.regs.r24,
        vcpu.regs.r25,
        vcpu.regs.r26,
        vcpu.regs.r27,
        vcpu.regs.r28,
        vcpu.regs.r29,
        vcpu.regs.r30,
        vcpu.regs.r31,
        vcpu.regs.rip,
        vcpu.regs.rflags,
    ]
}

fn install_msr_inputs(vcpu: &mut X86_64Vcpu, index: u32, value: u64) {
    vcpu.regs.rcx = 0xA5A5_5A5A_0000_0000 | u64::from(index);
    vcpu.regs.rax = 0xFFFF_FFFF_0000_0000 | (value & u64::from(u32::MAX));
    vcpu.regs.rdx = 0xAAAA_AAAA_0000_0000 | (value >> 32);
}

#[test]
fn jit_wrmsr_matches_direct_for_every_profiled_nonvolatile_selector() {
    let cases = [
        (IA32_APIC_BASE, APIC_BASE_PROFILE_VALUE),
        (IA32_BIOS_SIGN_ID, 0),
        (IA32_XSS, 0),
        (IA32_UMWAIT_CONTROL, 0x0000_0000_0001_86A0),
        (IA32_TSC_ADJUST, 0xCAFE_BABE_DEAD_BEEF),
        (IA32_SYSENTER_CS, 0x10),
        (IA32_SYSENTER_ESP, 0x0000_7FFF_FFFF_D000),
        (IA32_SYSENTER_EIP, 0xFFFF_8000_0000_3000),
        (IA32_MISC_ENABLE, IA32_MISC_ENABLE_RESET),
        (IA32_PAT, 0x0706_0504_0100_0706),
        (IA32_TSC_DEADLINE, 0x0123_4567_89AB_CDEF),
        (IA32_EFER, 0xD01),
        (IA32_STAR, 0x0033_0020_CAFE_BABE),
        (IA32_LSTAR, 0xFFFF_8000_4444_5000),
        (IA32_CSTAR, 0x0123_4567_89AB_CDEF),
        (IA32_FMASK, 0x0000_0000_0004_0ED5),
        (IA32_FS_BASE, 0x0000_7FFF_3333_0000),
        (IA32_GS_BASE, 0),
        (IA32_KERNEL_GS_BASE, 0xFFFF_8000_5555_0000),
        (IA32_TSC_AUX, 0xDEAD_BEEF),
    ];

    for (index, value) in cases {
        let memory = memory_with_code(&[0x0F, 0x30, 0xEB, 0x00, 0xF4]);
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        install_msr_inputs(&mut direct, index, value);
        install_msr_inputs(&mut native, index, value);

        assert!(direct.step().expect("direct WRMSR").is_none());
        let region = native
            .jit_compile_region()
            .expect("compile WRMSR region")
            .expect("WRMSR region must be native eligible");
        assert!(
            region.uses_timestamp,
            "dynamic MSR selector is verifier-volatile"
        );
        native.jit_run_region_native(&region);

        assert_eq!(msr_state(&native), msr_state(&direct), "MSR {index:#x}");
        assert_eq!(
            scalar_state(&native),
            scalar_state(&direct),
            "MSR {index:#x}"
        );
        assert_eq!(native.regs.rip, 2, "successful WRMSR exact frontier");
    }
}

#[test]
fn jit_rdmsr_matches_direct_and_zero_extends_for_every_stable_selector() {
    let selectors = [
        IA32_APIC_BASE,
        IA32_BIOS_SIGN_ID,
        IA32_PLATFORM_ID,
        IA32_TSC_ADJUST,
        IA32_SYSENTER_CS,
        IA32_SYSENTER_ESP,
        IA32_SYSENTER_EIP,
        IA32_MISC_ENABLE,
        IA32_PAT,
        IA32_TSC_DEADLINE,
        IA32_XSS,
        IA32_UMWAIT_CONTROL,
        IA32_EFER,
        IA32_STAR,
        IA32_LSTAR,
        IA32_CSTAR,
        IA32_FMASK,
        IA32_FS_BASE,
        IA32_GS_BASE,
        IA32_KERNEL_GS_BASE,
        IA32_TSC_AUX,
    ];

    for index in selectors {
        let memory = memory_with_code(&[0x0F, 0x32, 0xEB, 0x00, 0xF4]);
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        install_msr_inputs(&mut direct, index, u64::MAX);
        install_msr_inputs(&mut native, index, u64::MAX);

        assert!(direct.step().expect("direct RDMSR").is_none());
        assert!(direct.step().expect("direct handoff branch").is_none());
        let region = native
            .jit_compile_region()
            .expect("compile RDMSR region")
            .expect("RDMSR region must be native eligible");
        native.jit_run_region_native(&region);

        assert_eq!(msr_state(&native), msr_state(&direct), "MSR {index:#x}");
        assert_eq!(
            scalar_state(&native),
            scalar_state(&direct),
            "MSR {index:#x}"
        );
        assert_eq!(native.regs.rax >> 32, 0, "MSR {index:#x}: RAX");
        assert_eq!(native.regs.rdx >> 32, 0, "MSR {index:#x}: RDX");
        assert_eq!(native.regs.rip, 4);
    }
}

#[test]
fn jit_msr_faults_are_dynamic_precise_and_noncommitting() {
    let cases: &[(&str, bool, u32, u64, fn(&mut X86_64Vcpu))] = &[
        ("CPL3 read", false, IA32_STAR, 0, |vcpu| {
            vcpu.sregs.cs.selector = 3
        }),
        ("VM86 read", false, IA32_STAR, 0, |vcpu| {
            vcpu.regs.rflags |= RFLAGS_VM
        }),
        ("unknown read", false, 0xDEAD_BEEF, 0, |_| {}),
        ("unknown write", true, 0xDEAD_BEEF, 0x1111, |_| {}),
        (
            "noncanonical LSTAR",
            true,
            IA32_LSTAR,
            0x0000_8000_0000_0000,
            |_| {},
        ),
        ("reserved EFER", true, IA32_EFER, 1 << 12, |_| {}),
        ("live EFER.LME change", true, IA32_EFER, 1 << 11, |_| {}),
        ("wide SYSENTER_CS", true, IA32_SYSENTER_CS, 1 << 32, |_| {}),
        ("PAT reserved type", true, IA32_PAT, 2, |_| {}),
        (
            "XSS component outside CPUID profile",
            true,
            IA32_XSS,
            1,
            |_| {},
        ),
        (
            "UMWAIT_CONTROL reserved bit",
            true,
            IA32_UMWAIT_CONTROL,
            2,
            |_| {},
        ),
        (
            "MISC_ENABLE read-only profile bit",
            true,
            IA32_MISC_ENABLE,
            IA32_MISC_ENABLE_RESET ^ (1 << 11),
            |_| {},
        ),
        (
            "MISC_ENABLE inert XD-disable bit",
            true,
            IA32_MISC_ENABLE,
            IA32_MISC_ENABLE_RESET | (1 << 34),
            |_| {},
        ),
    ];

    for (name, write, index, value, configure) in cases {
        let opcode = if *write { 0x30 } else { 0x32 };
        let memory = memory_with_code(&[0x0F, opcode, 0xEB, 0x00, 0xF4]);
        let mut native = test_vcpu(memory);
        install_msr_inputs(&mut native, *index, *value);
        configure(&mut native);
        let before = (msr_state(&native), scalar_state(&native));

        let region = native
            .jit_compile_region()
            .expect("compile dynamically guarded MSR region")
            .expect("dynamic MSR faults must not block native admission");
        native.jit_run_region_native(&region);

        assert_eq!(native.regs.rip, 0, "{name}: precise replay PC");
        assert_eq!(
            (msr_state(&native), scalar_state(&native)),
            before,
            "{name}"
        );
        assert!(
            native.step().is_err(),
            "{name}: direct replay must deliver #GP(0)"
        );
    }
}

#[test]
fn jit_wrmsr_tsc_rebases_the_guest_clock_at_the_helper_instant() {
    let memory = memory_with_code(&[0x0F, 0x30, 0xEB, 0x00, 0xF4]);
    let mut native = test_vcpu(memory);
    let desired = native.tsc().wrapping_add(30_000_000);
    install_msr_inputs(&mut native, IA32_TSC, desired);
    let base_before = native.tsc().wrapping_sub(native.tsc_adjust);
    let region = native
        .jit_compile_region()
        .expect("compile IA32_TSC WRMSR")
        .expect("IA32_TSC WRMSR must be native eligible");
    native.jit_run_region_native(&region);
    let base_after = native.tsc().wrapping_sub(native.tsc_adjust);
    let helper_base = desired.wrapping_sub(native.tsc_adjust);

    assert!(
        (base_before..=base_after).contains(&helper_base),
        "helper base {helper_base:#x} escaped {base_before:#x}..={base_after:#x}"
    );
    assert_eq!(native.regs.rip, 2);
}

#[test]
fn jit_rdmsr_tsc_uses_the_adjusted_guest_clock_and_preserves_handoff_state() {
    let memory = memory_with_code(&[0x0F, 0x32, 0xEB, 0x00, 0xF4]);
    let mut native = test_vcpu(memory);
    native.tsc_adjust = 0x1_0000_0000;
    install_msr_inputs(&mut native, IA32_TSC, u64::MAX);
    let preserved = [
        native.regs.rcx,
        native.regs.rbx,
        native.regs.rsp,
        native.regs.rbp,
        native.regs.r8,
        native.regs.r15,
        native.regs.r16,
        native.regs.r31,
        native.regs.rflags,
    ];
    let before = native.tsc();
    let region = native
        .jit_compile_region()
        .expect("compile IA32_TSC RDMSR")
        .expect("IA32_TSC RDMSR must be native eligible");
    native.jit_run_region_native(&region);
    let value = (native.regs.rdx << 32) | native.regs.rax;
    let after = native.tsc();

    assert!((before..=after).contains(&value));
    assert_eq!(native.regs.rax >> 32, 0);
    assert_eq!(native.regs.rdx >> 32, 0);
    assert_eq!(
        [
            native.regs.rcx,
            native.regs.rbx,
            native.regs.rsp,
            native.regs.rbp,
            native.regs.r8,
            native.regs.r15,
            native.regs.r16,
            native.regs.r31,
            native.regs.rflags,
        ],
        preserved
    );
    assert_eq!(native.regs.rip, 4);
}

#[test]
fn jit_verify_treats_dynamic_msr_regions_as_clock_domain_dependent() {
    let memory = memory_with_code(&[0x0F, 0x32, 0xEB, 0x00, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    install_msr_inputs(&mut vcpu, IA32_STAR, 0);
    let expected = vcpu.sregs.star;
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified RDMSR")
        .expect("verified RDMSR must be native eligible");
    assert!(region.uses_timestamp);

    vcpu.jit_run_region_verified(&region);

    assert_eq!((vcpu.regs.rdx << 32) | vcpu.regs.rax, expected);
    assert_eq!(vcpu.regs.rip, 4);
}

#[test]
fn jit_callout_roundtrips_callee_msr_state_back_into_native_execution() {
    // CALL 0xA; RDMSR; JMP next; HLT; 0xA: WRMSR; RET.
    let code = [
        0xE8, 0x05, 0x00, 0x00, 0x00, 0x0F, 0x32, 0xEB, 0x00, 0xF4, 0x0F, 0x30, 0xC3,
    ];
    let value = 0xCAFE_BABE_DEAD_BEEF;
    let mut direct = test_vcpu(memory_with_code(&code));
    let mut native = test_vcpu(memory_with_code(&code));
    for vcpu in [&mut direct, &mut native] {
        install_msr_inputs(vcpu, IA32_STAR, value);
    }
    native.set_jit_call(true);

    for _ in 0..5 {
        assert!(
            direct
                .step()
                .expect("direct CALL/WRMSR/RET/RDMSR/JMP")
                .is_none()
        );
    }
    let region = native
        .jit_compile_region()
        .expect("compile MSR callout region")
        .expect("MSR callout region must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.star, value);
    assert_eq!((native.regs.rdx << 32) | native.regs.rax, value);
    assert_eq!(msr_state(&native), msr_state(&direct));
    assert_eq!(scalar_state(&native), scalar_state(&direct));
    assert_eq!(native.regs.rip, 9);
}

#[test]
fn jit_verify_snapshots_compares_and_adopts_callout_only_msr_state() {
    // CALL 8; JMP next; HLT; 8: WRMSR; RET. The outer native region has no
    // X86Msr op, so verifier state coverage—not the timestamp skip—owns STAR.
    let code = [
        0xE8, 0x03, 0x00, 0x00, 0x00, 0xEB, 0x00, 0xF4, 0x0F, 0x30, 0xC3,
    ];
    let value = 0x0123_4567_89AB_CDEF;
    let mut vcpu = test_vcpu(memory_with_code(&code));
    install_msr_inputs(&mut vcpu, IA32_STAR, value);
    vcpu.set_jit_call(true);

    let region = vcpu
        .jit_compile_region()
        .expect("compile callout-only WRMSR region")
        .expect("callout-only WRMSR region must be native eligible");
    assert!(!region.uses_timestamp, "outer region contains no MSR read");
    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.sregs.star, value);
    assert_eq!(vcpu.regs.rip, 7);
}

#[test]
fn jit_disabled_msr_extensions_exit_at_the_exact_faulting_frontier() {
    for (name, instruction, apx) in [
        ("WRMSRNS", &[0x0F, 0x01, 0xC6][..], false),
        ("RDMSRLIST", &[0xF2, 0x0F, 0x01, 0xC6][..], false),
        ("WRMSRLIST", &[0xF3, 0x0F, 0x01, 0xC6][..], false),
        ("VEX2", &[0xC5, 0xF8, 0x01, 0xC6][..], false),
        ("VEX3", &[0xC4, 0xE1, 0x78, 0x01, 0xC6][..], false),
        ("EVEX", &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC6][..], false),
        ("REX2", &[0xD5, 0x80, 0x01, 0xC6][..], true),
    ] {
        let mut code = vec![
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp disabled MSR extension
            0x90, 0x90, // unreachable padding
        ];
        code.extend_from_slice(instruction);
        let memory = memory_with_code(&code);
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(apx);
            vcpu.regs.rcx = u64::MAX;
            vcpu.regs.rdx = 0x5555_6666_7777_8888;
            vcpu.regs.rsi = 0x0000_8000_0000_0001;
            vcpu.regs.rdi = 0xFFFF_7FFF_FFFF_FFF9;
            // Apple host translation clears AF while bridging POPFQ. AF is
            // independently covered by native x86-64 CI; keep this exact
            // frontier differential portable through the amd64 container.
            vcpu.regs.rflags &= !(1 << 4);
        }

        assert!(direct.step().expect("direct MOV").is_none(), "{name}");
        assert!(direct.step().expect("direct JMP").is_none(), "{name}");
        assert_eq!(direct.regs.rip, 9, "{name}");

        let region = native
            .jit_compile_region()
            .expect("compile region ending at disabled MSR extension")
            .expect("supported prefix must remain native before the #UD frontier");

        native.jit_run_region_native(&region);
        assert_eq!(native.regs.rax, 0x1234_5678, "{name}");
        assert_eq!(native.regs.rip, 9, "{name}");
        assert_eq!(
            (msr_state(&native), scalar_state(&native)),
            (msr_state(&direct), scalar_state(&direct)),
            "{name}: native/direct state at the exact #UD frontier"
        );

        let before = (msr_state(&native), scalar_state(&native));
        let error = native
            .step()
            .expect_err("disabled MSR extension frontier must deliver #UD");
        assert!(
            error
                .to_string()
                .contains("triple fault while delivering vector 6"),
            "{name}: expected #UD-to-triple-fault delivery chain, got {error}"
        );
        assert_eq!(
            (msr_state(&native), scalar_state(&native)),
            before,
            "{name}"
        );
    }
}
