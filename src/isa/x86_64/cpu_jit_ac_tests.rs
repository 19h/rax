//! Native x86-64 JIT differentials for CLAC/STAC state and #UD handoffs.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const AC: u64 = 1 << 18;
const VM: u64 = 1 << 17;

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 |= 1;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct CLAC/STAC instruction").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

#[test]
fn jit_clac_stac_match_direct_and_commit_ac_through_the_vcpu_abi() {
    for (modrm, initial_ac, expected_ac) in [(0xCA, true, false), (0xCB, false, true)] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x01, modrm, 0xEB, 0x00, 0xF4], GuestAddress(0))
            .unwrap();
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        for vcpu in [&mut direct, &mut native] {
            if initial_ac {
                vcpu.regs.rflags |= AC;
            }
            vcpu.regs.rax = 0x1122_3344_5566_7788;
        }

        run_direct_to(&mut direct, 5);
        let region = native
            .jit_compile_region()
            .expect("compile CLAC/STAC region")
            .expect("CLAC/STAC must be native eligible");
        native.jit_run_region_native(&region);

        assert_eq!(native.regs.rflags, direct.regs.rflags);
        assert_eq!(native.regs.rflags & AC != 0, expected_ac);
        assert_eq!(native.regs.rax, direct.regs.rax);
        assert_eq!(native.regs.rsp, direct.regs.rsp);
        assert_eq!(native.regs.rbp, direct.regs.rbp);
        assert_eq!(native.regs.rip, 5);
    }
}

#[test]
fn jit_verify_replays_clac_stac_and_adopts_the_ac_result() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0x0F, 0x01, 0xCB, 0xEB, 0x00, 0xF4], GuestAddress(0))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified STAC region")
        .expect("STAC must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_ne!(vcpu.regs.rflags & AC, 0);
    assert_eq!(vcpu.regs.rip, 5);
}

#[test]
fn jit_clac_stac_protected_and_vm86_faults_handoff_precisely_as_ud() {
    for (modrm, initial_ac, vm86) in [
        (0xCA, true, false),
        (0xCB, false, false),
        (0xCA, true, true),
        (0xCB, false, true),
    ] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x01, modrm, 0xEB, 0x00, 0xF4], GuestAddress(0))
            .unwrap();
        let mut vcpu = test_vcpu(memory);
        if vm86 {
            vcpu.regs.rflags |= VM;
        } else {
            vcpu.sregs.cs.selector = 3;
        }
        if initial_ac {
            vcpu.regs.rflags |= AC;
        }
        let before = vcpu.regs.rflags;

        let region = vcpu
            .jit_compile_region()
            .expect("compile guarded CLAC/STAC region")
            .expect("dynamic privilege must not prevent compilation");
        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0);
        assert_eq!(vcpu.regs.rflags, before);
        assert!(
            exception_without_idt(&mut vcpu).contains("IDT entry 6 not present"),
            "CLAC/STAC privilege and VM86 failures must deliver #UD"
        );
    }
}

#[test]
fn direct_clac_stac_real_mode_bypass_and_lock_fault_are_precise() {
    for (modrm, expected_ac) in [(0xCA, false), (0xCB, true)] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        memory
            .write_slice(&[0x0F, 0x01, modrm], GuestAddress(0))
            .unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.efer = 0;
        vcpu.sregs.cs.l = false;
        vcpu.sregs.cs.db = true;
        vcpu.sregs.cs.selector = 3;
        vcpu.sregs.cr0 &= !1;
        vcpu.regs.rflags |= AC;
        assert!(vcpu.step().unwrap().is_none());
        assert_eq!(vcpu.regs.rflags & AC != 0, expected_ac);
        assert_eq!(vcpu.regs.rip, 3);
    }

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory
        .write_slice(&[0xF0, 0x0F, 0x01, 0xCA], GuestAddress(0))
        .unwrap();
    let mut locked = test_vcpu(memory);
    locked.regs.rflags |= AC;
    assert!(exception_without_idt(&mut locked).contains("IDT entry 6 not present"));
    assert_eq!(locked.regs.rip, 0);
    assert_ne!(locked.regs.rflags & AC, 0, "LOCK #UD must not commit CLAC");
}

#[test]
fn jit_clac_stac_state_is_coherent_across_interpreter_callouts() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // STAC; CALL 100h; JMP HLT; HLT. The direct callee clears AC and returns.
    memory
        .write_slice(
            &[
                0x0F, 0x01, 0xCB, 0xE8, 0xF8, 0x00, 0x00, 0x00, 0xEB, 0x00, 0xF4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    memory
        .write_slice(&[0x0F, 0x01, 0xCA, 0xC3], GuestAddress(0x100))
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_jit_call(true);
    }

    run_direct_to(&mut direct, 10);
    let region = native
        .jit_compile_region()
        .expect("compile callout CLAC/STAC region")
        .expect("CLAC/STAC callout sequence must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rflags & AC, 0);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rip, 10);
}
