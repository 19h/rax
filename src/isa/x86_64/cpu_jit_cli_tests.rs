//! Direct/native x86-64 JIT differentials for CLI privilege virtualization.

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
    vcpu.regs.rflags =
        0x2 | 0x08D5 | flags::bits::DF | flags::bits::IF | flags::bits::VIF | flags::bits::VIP;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn scalar_state(vcpu: &X86_64Vcpu) -> [u64; 10] {
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
    ]
}

#[test]
fn jit_cli_matches_direct_for_if_vif_real_pvi_vme_and_apx_paths() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        configure: fn(&mut X86_64Vcpu),
        cleared: u64,
    }
    let cases = [
        Case {
            name: "real-mode",
            instruction: &[0xFA],
            configure: |vcpu| {
                vcpu.sregs.cr0 = 0;
                vcpu.sregs.cs.selector = 3;
            },
            cleared: flags::bits::IF,
        },
        Case {
            name: "protected-cpl0-iopl0",
            instruction: &[0xFA],
            configure: |_| {},
            cleared: flags::bits::IF,
        },
        Case {
            name: "protected-cpl3-iopl3",
            instruction: &[0xFA],
            configure: |vcpu| {
                vcpu.sregs.cs.selector = 3;
                vcpu.regs.rflags |= flags::bits::IOPL_MASK;
            },
            cleared: flags::bits::IF,
        },
        Case {
            name: "protected-cpl3-pvi",
            instruction: &[0xFA],
            configure: |vcpu| {
                vcpu.sregs.cs.selector = 3;
                vcpu.sregs.cr4 = 1 << 1;
            },
            cleared: flags::bits::VIF,
        },
        Case {
            name: "virtual-8086-vme",
            instruction: &[0xFA],
            configure: |vcpu| {
                vcpu.sregs.cs.selector = 0;
                vcpu.sregs.cr4 = 1;
                vcpu.regs.rflags |= flags::bits::VM;
            },
            cleared: flags::bits::VIF,
        },
        Case {
            name: "rex2-map0-all-unused-bits",
            instruction: &[0xD5, 0x7F, 0xFA],
            configure: |vcpu| vcpu.set_apx_enabled(true),
            cleared: flags::bits::IF,
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
            direct.step().expect("direct CLI").is_none(),
            "{}",
            case.name
        );
        let region = native
            .jit_compile_region()
            .expect("compile CLI region")
            .expect("CLI must be native eligible");
        native.jit_run_region_native(&region);

        assert_eq!(
            scalar_state(&native),
            scalar_state(&direct),
            "{}",
            case.name
        );
        assert_eq!(native.regs.rflags, initial & !case.cleared, "{}", case.name);
        assert_eq!(
            native.regs.rip,
            case.instruction.len() as u64,
            "{}",
            case.name
        );
    }
}

#[test]
fn jit_cli_dynamic_faults_deoptimize_at_the_exact_noncommitting_frontier() {
    for (name, instruction, apx_enabled, cr4, rflags, expected_vector) in [
        ("protected-cpl3", &[0xFA][..], false, 0, 0, 13),
        (
            "virtual-8086-without-vme",
            &[0xFA],
            false,
            0,
            flags::bits::VM,
            13,
        ),
        (
            "rex2-apx-before-privilege",
            &[0xD5, 0x00, 0xFA],
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
            .expect("compile dynamically guarded CLI")
            .expect("dynamic CLI faults must not block admission");
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
    }
}

#[test]
fn jit_cli_verify_compares_and_adopts_interrupt_control_state() {
    let memory = memory_with_code(&[0xFA, 0xEB, 0x00, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified CLI region")
        .expect("verified CLI region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 1);
    assert_eq!(vcpu.regs.rflags & flags::bits::IF, 0);
    assert_ne!(vcpu.regs.rflags & flags::bits::VIF, 0);
}

#[test]
fn jit_cli_control_shadow_stays_coherent_across_interpreter_callouts() {
    let memory = memory_with_code(&[
        0xE8, 0xFB, 0x00, 0x00, 0x00, // call 100h
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    memory
        .write_slice(&[0xFA, 0xC3], GuestAddress(0x100)) // cli; ret
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    native.set_jit_call(true);

    for _ in 0..4 {
        if direct.regs.rip == 7 {
            break;
        }
        assert!(direct.step().expect("direct call/CLI sequence").is_none());
    }
    assert_eq!(direct.regs.rip, 7);

    let region = native
        .jit_compile_region()
        .expect("compile CLI callout sequence")
        .expect("CLI callout sequence must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rip, 7);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rflags & flags::bits::IF, 0);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
}

#[test]
fn jit_cli_helper_is_noncommitting_on_null_apx_privilege_and_invalid_cpl() {
    assert_eq!(unsafe { rax_jit_cli(core::ptr::null_mut(), 0) }, 0);

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
        ("invalid-cpl", 0, |state: &mut GuestRegs| {
            state.cr0 = 1;
            state.cpl = 4;
        }),
    ] {
        let initial = flags::bits::IF | flags::bits::VIF | flags::bits::VIP;
        let mut state = GuestRegs {
            interrupt_flags: initial,
            ..GuestRegs::default()
        };
        configure(&mut state);
        assert_eq!(
            unsafe { rax_jit_cli(&mut state, requires_apx) },
            0,
            "{name}"
        );
        assert_eq!(state.interrupt_flags, initial, "{name}");
    }
}

#[test]
fn direct_cli_rex2_decode_cache_hit_retains_apx_and_ignored_payload_semantics() {
    let memory = memory_with_code(&[0xD5, 0x7F, 0xFA]);
    let mut vcpu = test_vcpu(memory);
    vcpu.set_apx_enabled(true);

    for pass in 0..2 {
        vcpu.regs.rip = 0;
        vcpu.regs.rflags |= flags::bits::IF;
        assert!(vcpu.step().expect("REX2 CLI direct decode").is_none());
        assert_eq!(vcpu.regs.rip, 3, "pass {pass}");
        assert_eq!(vcpu.regs.rflags & flags::bits::IF, 0, "pass {pass}");
    }
}
