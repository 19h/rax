//! Direct/native x86-64 JIT differentials for Intel SYSENTER/SYSEXIT fixed
//! segment state, dynamic targets, privilege faults, and precise replay.

use super::*;
use crate::smir::lower::runtime::GuestRegs;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs = crate::vm::vcpu::Segment {
        selector: 0,
        type_: 0x0B,
        present: true,
        s: true,
        l: true,
        g: true,
        ..crate::vm::vcpu::Segment::default()
    };
    vcpu.sregs.ss = crate::vm::vcpu::Segment {
        selector: 0x10,
        type_: 0x03,
        present: true,
        s: true,
        db: true,
        g: true,
        ..crate::vm::vcpu::Segment::default()
    };
    vcpu.sregs.sysenter_cs = 8;
    vcpu.sregs.sysenter_esp = 0xFFFF_8000_0000_8000;
    vcpu.sregs.sysenter_eip = 0xFFFF_8000_0000_6000;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.regs.rcx = 0xFFFF_8000_1234_9000;
    vcpu.regs.rdx = 0xFFFF_8000_5678_A000;
    vcpu.regs.rbx = 0x0123_4567_89AB_CDEF;
    vcpu.regs.r15 = 0xF0E1_D2C3_B4A5_9687;
    vcpu.regs.r16 = 0x1122_3344_5566_7788;
    vcpu.regs.r31 = 0x8877_6655_4433_2211;
    vcpu.regs.rflags = 0x2
        | flags::bits::CF
        | flags::bits::PF
        | flags::bits::ZF
        | flags::bits::SF
        | flags::bits::OF
        | flags::bits::DF
        | flags::bits::IF
        | flags::bits::VIF
        | flags::bits::VIP
        | flags::bits::IOPL_MASK;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn segment_fingerprint(
    segment: &crate::vm::vcpu::Segment,
) -> (
    u64,
    u32,
    u16,
    u8,
    bool,
    u8,
    bool,
    bool,
    bool,
    bool,
    bool,
    bool,
) {
    (
        segment.base,
        segment.limit,
        segment.selector,
        segment.type_,
        segment.present,
        segment.dpl,
        segment.db,
        segment.s,
        segment.l,
        segment.g,
        segment.avl,
        segment.unusable,
    )
}

fn architectural_fingerprint(
    vcpu: &X86_64Vcpu,
) -> (
    [u64; 32],
    u64,
    bool,
    (
        u64,
        u32,
        u16,
        u8,
        bool,
        u8,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
    ),
    (
        u64,
        u32,
        u16,
        u8,
        bool,
        u8,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
    ),
) {
    (
        gprs(&vcpu.regs),
        vcpu.regs.rflags,
        vcpu.interrupt_inhibit,
        segment_fingerprint(&vcpu.sregs.cs),
        segment_fingerprint(&vcpu.sregs.ss),
    )
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

#[test]
fn jit_sysenter_sysexit_match_direct_for_ia32e_32_and_64_bit_transitions() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        configure: fn(&mut X86_64Vcpu),
        expected_rip: u64,
        expected_rsp: u64,
        expected_cs: u16,
        expected_ss: u16,
        expected_cpl: u8,
        expected_cs_l: bool,
    }
    let cases = [
        Case {
            name: "SYSENTER",
            instruction: &[0x0F, 0x34],
            configure: |vcpu| vcpu.regs.rflags |= flags::bits::VM,
            expected_rip: 0xFFFF_8000_0000_6000,
            expected_rsp: 0xFFFF_8000_0000_8000,
            expected_cs: 8,
            expected_ss: 16,
            expected_cpl: 0,
            expected_cs_l: true,
        },
        Case {
            name: "REX.W SYSENTER ignored",
            instruction: &[0x48, 0x0F, 0x34],
            configure: |_| {},
            expected_rip: 0xFFFF_8000_0000_6000,
            expected_rsp: 0xFFFF_8000_0000_8000,
            expected_cs: 8,
            expected_ss: 16,
            expected_cpl: 0,
            expected_cs_l: true,
        },
        Case {
            name: "SYSEXIT32",
            instruction: &[0x0F, 0x35],
            configure: |_| {},
            expected_rip: 0x5678_A000,
            expected_rsp: 0x1234_9000,
            expected_cs: 0x1B,
            expected_ss: 0x23,
            expected_cpl: 3,
            expected_cs_l: false,
        },
        Case {
            name: "SYSEXIT64",
            instruction: &[0x48, 0x0F, 0x35],
            configure: |_| {},
            expected_rip: 0xFFFF_8000_5678_A000,
            expected_rsp: 0xFFFF_8000_1234_9000,
            expected_cs: 0x2B,
            expected_ss: 0x33,
            expected_cpl: 3,
            expected_cs_l: true,
        },
    ];

    for case in cases {
        let direct_memory = memory_with_code(case.instruction);
        let native_memory = memory_with_code(case.instruction);
        let mut direct = test_vcpu(direct_memory);
        let mut native = test_vcpu(native_memory);
        (case.configure)(&mut direct);
        (case.configure)(&mut native);

        assert!(
            direct
                .step()
                .expect("direct fast system transfer")
                .is_none()
        );
        let region = native
            .jit_compile_region()
            .expect("compile fast-system-transfer region")
            .unwrap_or_else(|| panic!("{} must be native eligible", case.name));
        native.jit_run_region_verified(&region);

        assert_eq!(
            architectural_fingerprint(&native),
            architectural_fingerprint(&direct),
            "{}",
            case.name
        );
        assert_eq!(native.regs.rip, case.expected_rip, "{}", case.name);
        assert_eq!(native.regs.rsp, case.expected_rsp, "{}", case.name);
        assert_eq!(native.sregs.cs.selector, case.expected_cs, "{}", case.name);
        assert_eq!(native.sregs.ss.selector, case.expected_ss, "{}", case.name);
        assert_eq!(native.sregs.cs.dpl, case.expected_cpl, "{}", case.name);
        assert_eq!(native.sregs.ss.dpl, case.expected_cpl, "{}", case.name);
        assert_eq!(native.sregs.cs.l, case.expected_cs_l, "{}", case.name);
        assert_eq!(native.sregs.cs.db, !case.expected_cs_l, "{}", case.name);
        assert_eq!(native.sregs.cs.base, 0, "{}", case.name);
        assert_eq!(native.sregs.ss.base, 0, "{}", case.name);
        assert_eq!(native.sregs.cs.limit, 0xF_FFFF, "{}", case.name);
        assert_eq!(native.sregs.ss.limit, 0xF_FFFF, "{}", case.name);
        if case.expected_cs == 8 {
            assert_eq!(
                native.regs.rflags & (flags::bits::IF | flags::bits::VM),
                0,
                "{}",
                case.name
            );
        } else {
            assert_ne!(native.regs.rflags & flags::bits::IF, 0, "{}", case.name);
        }
    }
}

#[test]
fn jit_fast_system_transfer_faults_deoptimize_without_any_architectural_commit() {
    for (name, instruction, configure) in [
        (
            "SYSENTER protected mode disabled",
            &[0x0F, 0x34][..],
            (|vcpu: &mut X86_64Vcpu| vcpu.sregs.cr0 &= !1) as fn(&mut X86_64Vcpu),
        ),
        ("SYSENTER null selector", &[0x0F, 0x34], |vcpu| {
            vcpu.sregs.sysenter_cs = 3
        }),
        ("SYSENTER noncanonical EIP", &[0x0F, 0x34], |vcpu| {
            vcpu.sregs.sysenter_eip = 0x0000_8000_0000_0000
        }),
        ("SYSEXIT nonzero CPL", &[0x48, 0x0F, 0x35], |vcpu| {
            vcpu.sregs.cs.selector = 3;
            vcpu.sregs.cs.dpl = 3;
        }),
        ("SYSEXITQ noncanonical RIP", &[0x48, 0x0F, 0x35], |vcpu| {
            vcpu.regs.rdx = 0x0000_8000_0000_0000
        }),
    ] {
        let memory = memory_with_code(instruction);
        let mut vcpu = test_vcpu(memory);
        configure(&mut vcpu);
        let before = architectural_fingerprint(&vcpu);
        let before_rip = vcpu.regs.rip;
        let region = vcpu
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: compile failed: {error}"))
            .unwrap_or_else(|| panic!("{name}: dynamic fault must remain native eligible"));
        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rip, before_rip, "{name}: precise replay PC");
        assert_eq!(architectural_fingerprint(&vcpu), before, "{name}");

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains("IDT entry 13 not present"),
            "{name}: direct replay must deliver #GP(0): {error}"
        );
    }
}

#[test]
fn fast_system_transfer_helper_validates_discriminators_and_commits_atomically() {
    assert_eq!(
        unsafe { rax_jit_fast_system_transfer(core::ptr::null_mut(), 0, 0) },
        0
    );

    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory);
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    state.cr0 = vcpu.sregs.cr0;
    state.efer = vcpu.sregs.efer;
    state.cs_l = 1;
    state.cpl = 0;
    state.sysenter_cs = 8;
    state.sysenter_esp = 0xFFFF_8000_0000_8000;
    state.sysenter_eip = 0xFFFF_8000_0000_6000;
    state.gpr[1] = 0xFFFF_8000_1234_9000;
    state.gpr[2] = 0xFFFF_8000_5678_A000;
    state.gpr[4] = 0x8000;
    state.exit_pc = 0x1000;
    state.interrupt_flags = flags::bits::IF
        | flags::bits::VM
        | flags::bits::VIF
        | flags::bits::VIP
        | flags::bits::IOPL_MASK;

    for (name, kind, operand64, mutate) in [
        (
            "unknown kind",
            2,
            0,
            (|_: &mut GuestRegs| {}) as fn(&mut GuestRegs),
        ),
        ("invalid operand flag", 0, 2, |_| {}),
        ("SYSENTER cannot be operand64", 0, 1, |_| {}),
        ("non-long source", 0, 0, |state| state.cs_l = 0),
        ("missing LMA", 0, 0, |state| state.efer = 0),
        ("invalid CPL", 0, 0, |state| state.cpl = 4),
        ("dynamic general protection", 0, 0, |state| {
            state.sysenter_cs = 0
        }),
    ] {
        let mut candidate = state;
        mutate(&mut candidate);
        let before = candidate;
        let before_cs = segment_fingerprint(&vcpu.sregs.cs);
        let before_ss = segment_fingerprint(&vcpu.sregs.ss);
        assert_eq!(
            unsafe { rax_jit_fast_system_transfer(&mut candidate, kind, operand64) },
            0,
            "{name}"
        );
        assert_eq!(candidate, before, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.cs), before_cs, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.ss), before_ss, "{name}");
    }

    let mut sysenter = state;
    assert_eq!(
        unsafe { rax_jit_fast_system_transfer(&mut sysenter, 0, 0) },
        1
    );
    assert_eq!(sysenter.exit_pc, 0xFFFF_8000_0000_6000);
    assert_eq!(sysenter.gpr[4], 0xFFFF_8000_0000_8000);
    assert_eq!(sysenter.cpl, 0);
    assert_eq!(sysenter.cs_l, 1);
    assert_eq!(
        sysenter.interrupt_flags & (flags::bits::IF | flags::bits::VM),
        0
    );
    assert_eq!(vcpu.sregs.cs.selector, 8);
    assert_eq!(vcpu.sregs.ss.selector, 16);

    let mut sysexit = state;
    sysexit.interrupt_flags &= !flags::bits::VM;
    assert_eq!(
        unsafe { rax_jit_fast_system_transfer(&mut sysexit, 1, 1) },
        1
    );
    assert_eq!(sysexit.exit_pc, 0xFFFF_8000_5678_A000);
    assert_eq!(sysexit.gpr[4], 0xFFFF_8000_1234_9000);
    assert_eq!(sysexit.cpl, 3);
    assert_eq!(sysexit.cs_l, 1);
    assert_eq!(vcpu.sregs.cs.selector, 0x2B);
    assert_eq!(vcpu.sregs.ss.selector, 0x33);
}

#[test]
fn jit_rejects_fast_system_transfer_outside_long_code_mode_and_direct_remains_authoritative() {
    let memory = memory_with_code(&[0x0F, 0x34]);
    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;

    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode decode must remain a direct frontier"
    );
    assert!(compatibility.step().expect("direct SYSENTER").is_none());
    assert_eq!(compatibility.regs.rip, 0xFFFF_8000_0000_6000);
    assert!(compatibility.sregs.cs.l);
}
