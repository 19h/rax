//! Direct/native x86-64 JIT differentials for STI privilege virtualization and
//! the one-instruction interrupt shadow.

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
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.sregs.cr4 = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.regs.rbx = 0x0123_4567_89AB_CDEF;
    vcpu.regs.r15 = 0xF0E1_D2C3_B4A5_9687;
    vcpu.regs.r16 = 0x1122_3344_5566_7788;
    vcpu.regs.r31 = 0x8877_6655_4433_2211;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn scalar_state(vcpu: &X86_64Vcpu) -> ([u64; 10], bool) {
    (
        [
            vcpu.regs.rax,
            vcpu.regs.rbx,
            vcpu.regs.rsp,
            vcpu.regs.rbp,
            vcpu.regs.r15,
            vcpu.regs.r16,
            vcpu.regs.r31,
            vcpu.regs.rip,
            vcpu.regs.rflags,
            vcpu.sregs.cr4,
        ],
        vcpu.interrupt_inhibit,
    )
}

#[test]
fn jit_sti_matches_direct_for_if_vif_real_pvi_vme_apx_and_shadow_paths() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        configure: fn(&mut X86_64Vcpu),
        set: u64,
        inhibit: bool,
    }
    let cases = [
        Case {
            name: "real-mode-if-zero",
            instruction: &[0xFB],
            configure: |vcpu| {
                vcpu.sregs.cr0 = 0;
                vcpu.sregs.cs.selector = 3;
            },
            set: flags::bits::IF,
            inhibit: true,
        },
        Case {
            name: "real-mode-if-one",
            instruction: &[0xFB],
            configure: |vcpu| {
                vcpu.sregs.cr0 = 0;
                vcpu.sregs.cs.selector = 3;
                vcpu.regs.rflags |= flags::bits::IF;
            },
            set: flags::bits::IF,
            inhibit: false,
        },
        Case {
            name: "protected-cpl0-iopl0",
            instruction: &[0xFB],
            configure: |_| {},
            set: flags::bits::IF,
            inhibit: true,
        },
        Case {
            name: "protected-cpl3-iopl3-vip-ignored",
            instruction: &[0xFB],
            configure: |vcpu| {
                vcpu.sregs.cs.selector = 3;
                vcpu.regs.rflags |= flags::bits::IOPL_MASK | flags::bits::VIP;
            },
            set: flags::bits::IF,
            inhibit: true,
        },
        Case {
            name: "protected-cpl3-pvi",
            instruction: &[0xFB],
            configure: |vcpu| {
                vcpu.sregs.cs.selector = 3;
                vcpu.sregs.cr4 = 1 << 1;
            },
            set: flags::bits::VIF,
            inhibit: false,
        },
        Case {
            name: "virtual-8086-vme",
            instruction: &[0xFB],
            configure: |vcpu| {
                vcpu.sregs.cs.selector = 0;
                vcpu.sregs.cr4 = 1;
                vcpu.regs.rflags |= flags::bits::VM;
            },
            set: flags::bits::VIF,
            inhibit: false,
        },
        Case {
            name: "rex2-map0-all-unused-bits",
            instruction: &[0xD5, 0x7F, 0xFB],
            configure: |vcpu| vcpu.set_apx_enabled(true),
            set: flags::bits::IF,
            inhibit: true,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
        let memory = memory_with_code(&code);
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        (case.configure)(&mut direct);
        (case.configure)(&mut native);
        let initial = direct.regs.rflags;

        assert!(
            direct.step().expect("direct STI").is_none(),
            "{}",
            case.name
        );
        let region = native
            .jit_compile_region()
            .expect("compile STI region")
            .expect("STI must be native eligible");
        native.jit_run_region_native(&region);

        assert_eq!(
            scalar_state(&native),
            scalar_state(&direct),
            "{}",
            case.name
        );
        assert_eq!(native.regs.rflags, initial | case.set, "{}", case.name);
        assert_eq!(native.interrupt_inhibit, case.inhibit, "{}", case.name);
        assert_eq!(
            native.regs.rip,
            case.instruction.len() as u64,
            "{}",
            case.name
        );
    }
}

#[test]
fn jit_sti_dynamic_faults_deoptimize_at_the_exact_noncommitting_frontier() {
    for (name, instruction, apx_enabled, cr4, rflags, expected_vector) in [
        ("protected-cpl3", &[0xFB][..], false, 0, 0, 13),
        (
            "protected-pvi-vip",
            &[0xFB],
            false,
            1 << 1,
            flags::bits::VIP,
            13,
        ),
        (
            "virtual-8086-without-vme",
            &[0xFB],
            false,
            0,
            flags::bits::VM,
            13,
        ),
        (
            "rex2-apx-before-privilege",
            &[0xD5, 0x00, 0xFB],
            false,
            0,
            0,
            6,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
        let memory = memory_with_code(&code);
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cs.selector = 3;
        vcpu.sregs.cr4 = cr4;
        vcpu.regs.rflags |= rflags;
        vcpu.set_apx_enabled(apx_enabled);
        let before = scalar_state(&vcpu);

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically guarded STI")
            .expect("dynamic STI faults must not block admission");
        vcpu.jit_run_region_native(&region);

        assert_eq!(
            scalar_state(&vcpu),
            before,
            "{name}: native deopt committed state"
        );
        assert_eq!(vcpu.regs.rip, 0, "{name}: precise fault PC");
        let error = format!("{:#}", vcpu.step().expect_err("direct replay must fault"));
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name}: wrong exception priority: {error}"
        );
        assert!(!vcpu.interrupt_inhibit, "{name}");
    }
}

#[test]
fn jit_sti_verify_compares_and_adopts_interrupt_shadow_state() {
    let memory = memory_with_code(&[0xFB, 0xEB, 0x00, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified STI region")
        .expect("verified STI region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 1);
    assert_ne!(vcpu.regs.rflags & flags::bits::IF, 0);
    assert!(vcpu.interrupt_inhibit);
    assert!(!vcpu.can_inject_interrupt());
}

#[test]
fn jit_sti_shadow_forces_the_following_instruction_out_of_native_execution() {
    let memory = memory_with_code(&[0xFB, 0x90, 0xEB, 0x00, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile STI region")
        .expect("STI region must be native eligible");
    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rip, 1);
    assert!(vcpu.interrupt_inhibit);
    assert!(!vcpu.jit_try_block().unwrap());
    assert_eq!(vcpu.regs.rip, 1);
    assert!(vcpu.interrupt_inhibit);

    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(vcpu.regs.rip, 2);
    assert!(!vcpu.interrupt_inhibit);
    assert!(vcpu.can_inject_interrupt());
}

#[test]
fn jit_sti_shadow_stays_coherent_across_interpreter_callouts() {
    let memory = memory_with_code(&[
        0xE8, 0xFB, 0x00, 0x00, 0x00, // call 100h
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    memory
        .write_slice(&[0xFB, 0xC3], GuestAddress(0x100)) // sti; ret
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    native.set_jit_call(true);

    for _ in 0..4 {
        if direct.regs.rip == 7 {
            break;
        }
        assert!(direct.step().expect("direct call/STI sequence").is_none());
    }
    assert_eq!(direct.regs.rip, 7);
    assert!(!direct.interrupt_inhibit);

    let region = native
        .jit_compile_region()
        .expect("compile STI callout sequence")
        .expect("STI callout sequence must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rip, 7);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.interrupt_inhibit, direct.interrupt_inhibit);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
}

#[test]
fn jit_sti_helper_is_noncommitting_on_null_apx_privilege_vip_and_invalid_cpl() {
    assert_eq!(unsafe { rax_jit_sti(core::ptr::null_mut(), 0) }, 0);

    for (name, requires_apx, configure) in [
        (
            "apx",
            1,
            (|state: &mut GuestRegs| {
                state.apx_enabled = 0;
                state.cr0 = 1;
                state.cpl = 0;
            }) as fn(&mut GuestRegs),
        ),
        ("privilege", 0, |state: &mut GuestRegs| {
            state.cr0 = 1;
            state.cpl = 3;
        }),
        ("pvi-vip", 0, |state: &mut GuestRegs| {
            state.cr0 = 1;
            state.cr4 = 1 << 1;
            state.cpl = 3;
            state.interrupt_flags |= flags::bits::VIP;
        }),
        ("invalid-cpl", 0, |state: &mut GuestRegs| {
            state.cr0 = 1;
            state.cpl = 4;
        }),
    ] {
        let initial = flags::bits::VIP;
        let mut state = GuestRegs {
            interrupt_flags: initial,
            interrupt_inhibit: 0xA5,
            ..GuestRegs::default()
        };
        configure(&mut state);
        assert_eq!(
            unsafe { rax_jit_sti(&mut state, requires_apx) },
            0,
            "{name}"
        );
        assert_eq!(state.interrupt_flags, initial, "{name}");
        assert_eq!(state.interrupt_inhibit, 0xA5, "{name}");
    }
}

#[test]
fn direct_sti_rex2_decode_cache_hit_retains_apx_payload_and_shadow_semantics() {
    let memory = memory_with_code(&[0xD5, 0x7F, 0xFB]);
    let mut vcpu = test_vcpu(memory);
    vcpu.set_apx_enabled(true);

    for pass in 0..2 {
        vcpu.regs.rip = 0;
        vcpu.regs.rflags &= !flags::bits::IF;
        vcpu.interrupt_inhibit = false;
        assert!(vcpu.step().expect("REX2 STI direct decode").is_none());
        assert_eq!(vcpu.regs.rip, 3, "pass {pass}");
        assert_ne!(vcpu.regs.rflags & flags::bits::IF, 0, "pass {pass}");
        assert!(vcpu.interrupt_inhibit, "pass {pass}");
    }
}
