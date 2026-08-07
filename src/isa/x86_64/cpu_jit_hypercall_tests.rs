//! Native x86-64 JIT differentials for deterministic virtualization profiles.

use super::*;
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
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    // The Linux/amd64 validation container can run through Apple host
    // translation, whose POPFQ bridge clears AF even for the pre-existing
    // SERIALIZE preservation test. Seed every other status flag here; native
    // x86-64 coverage independently exercises AF round-tripping.
    vcpu.regs.rflags = 0x2 | 0x08C5 | (1 << 10);
    vcpu.regs.rbx = 0xBBBB_BBBB_BBBB_BBBB;
    vcpu.regs.rcx = 0xCCCC_CCCC_CCCC_CCCC;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.r8 = 0x8888_8888_8888_8888;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.r16 = 0x1616_1616_1616_1616;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn scalar_state(vcpu: &X86_64Vcpu) -> [u64; 34] {
    let regs = &vcpu.regs;
    [
        regs.rax,
        regs.rbx,
        regs.rcx,
        regs.rdx,
        regs.rsi,
        regs.rdi,
        regs.rsp,
        regs.rbp,
        regs.r8,
        regs.r9,
        regs.r10,
        regs.r11,
        regs.r12,
        regs.r13,
        regs.r14,
        regs.r15,
        regs.r16,
        regs.r17,
        regs.r18,
        regs.r19,
        regs.r20,
        regs.r21,
        regs.r22,
        regs.r23,
        regs.r24,
        regs.r25,
        regs.r26,
        regs.r27,
        regs.r28,
        regs.r29,
        regs.r30,
        regs.r31,
        regs.rip,
        regs.rflags,
    ]
}

fn system_state(vcpu: &X86_64Vcpu) -> [u64; 8] {
    [
        vcpu.sregs.cr0,
        vcpu.sregs.cr2,
        vcpu.sregs.cr3,
        vcpu.sregs.cr4,
        vcpu.sregs.cr8,
        vcpu.sregs.fs.base,
        vcpu.sregs.gs.base,
        vcpu.kernel_gs_base,
    ]
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..16 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step()
                .expect("direct hypercall-hint sequence")
                .is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

#[test]
fn jit_vmcall_vmmcall_hints_match_direct_without_a_frontier() {
    let memory = memory_with_code(&[
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
        0x0F, 0x01, 0xC1, // vmcall (configured hint)
        0xB9, 0x89, 0x67, 0x45, 0x23, // mov ecx,23456789h
        0x0F, 0x01, 0xD9, // vmmcall (configured hint)
        0xBA, 0x9A, 0x78, 0x56, 0x34, // mov edx,3456789Ah
        0xEB, 0x00, // jmp hlt
        0xF4, // hlt
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 23);
    let region = native
        .jit_compile_region()
        .expect("compile hypercall-hint region")
        .expect("VMCALL/VMMCALL hints must remain inside the native region");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rax, 0x1234_5678);
    assert_eq!(native.regs.rcx, 0x2345_6789);
    assert_eq!(native.regs.rdx, 0x3456_789A);
    assert_eq!(scalar_state(&native), scalar_state(&direct));
    assert_eq!(system_state(&native), system_state(&direct));
}

#[test]
fn vmgexit_and_rex2_vmmcall_aliases_are_precise_ud_without_commit() {
    for (name, bytes, enable_apx) in [
        ("repne-vmgexit", &[0xF2, 0x0F, 0x01, 0xD9][..], false),
        ("rep-vmgexit", &[0xF3, 0x0F, 0x01, 0xD9][..], false),
        ("rex2-vmmcall", &[0xD5, 0x80, 0x01, 0xD9][..], true),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(bytes));
        vcpu.set_apx_enabled(enable_apx);
        let before_scalar = scalar_state(&vcpu);
        let before_system = system_state(&vcpu);

        let error = vcpu.step().expect_err("invalid alias must inject #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(scalar_state(&vcpu), before_scalar, "{name}");
        assert_eq!(system_state(&vcpu), before_system, "{name}");
    }
}

#[test]
fn jit_disabled_vmx_instructions_exit_at_the_exact_faulting_frontier() {
    for (name, instruction, apx) in [
        ("VMLAUNCH", &[0x0F, 0x01, 0xC2][..], false),
        ("VMRESUME", &[0x0F, 0x01, 0xC3][..], false),
        ("VMXOFF", &[0x0F, 0x01, 0xC4][..], false),
        ("VMFUNC", &[0x0F, 0x01, 0xD4][..], false),
        ("VMPTRLD", &[0x0F, 0xC7, 0x37][..], false),
        ("VMPTRST", &[0x0F, 0xC7, 0x3F][..], false),
        ("VMCLEAR", &[0x66, 0x0F, 0xC7, 0x37][..], false),
        ("VMXON", &[0xF3, 0x0F, 0xC7, 0x37][..], false),
        (
            "redundant-66 VMXON",
            &[0x66, 0xF3, 0x0F, 0xC7, 0x37][..],
            false,
        ),
        (
            "REX2 VMPTRLD [r16], APX disabled",
            &[0xD5, 0x90, 0xC7, 0x30][..],
            false,
        ),
        (
            "REX2 VMPTRLD [r16], APX enabled",
            &[0xD5, 0x90, 0xC7, 0x30][..],
            true,
        ),
        (
            "REX2 VMXON [r16]",
            &[0xF3, 0xD5, 0x90, 0xC7, 0x30][..],
            true,
        ),
    ] {
        let mut code = vec![
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp disabled VMX control
            0x90, 0x90, // unreachable padding
        ];
        code.extend_from_slice(instruction);
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.set_apx_enabled(apx);
        vcpu.regs.rdi = 0x0000_8000_0000_0000;
        vcpu.regs.r16 = 0x0000_8000_0000_1000;
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at disabled VMX instruction")
            .expect("supported prefix must remain native before VMX frontier");

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rax, 0x1234_5678, "{name}");
        assert_eq!(vcpu.regs.rip, 9, "{name}");

        let before_scalar = scalar_state(&vcpu);
        let before_system = system_state(&vcpu);
        let error = vcpu.step().expect_err("VMX frontier must deliver #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(scalar_state(&vcpu), before_scalar, "{name}");
        assert_eq!(system_state(&vcpu), before_system, "{name}");
    }
}

#[test]
fn jit_disabled_svm_controls_exit_at_the_exact_faulting_frontier() {
    for (name, modrm) in [
        ("VMRUN", 0xD8),
        ("VMLOAD", 0xDA),
        ("VMSAVE", 0xDB),
        ("STGI", 0xDC),
        ("CLGI", 0xDD),
        ("SKINIT", 0xDE),
        ("INVLPGA", 0xDF),
    ] {
        let memory = memory_with_code(&[
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp disabled SVM control
            0x90, 0x90, // unreachable padding
            0x0F, 0x01, modrm,
        ]);
        let mut vcpu = test_vcpu(memory);
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at disabled SVM control")
            .expect("supported prefix must remain native before SVM frontier");

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rax, 0x1234_5678, "{name}");
        assert_eq!(vcpu.regs.rip, 9, "{name}");

        let before_scalar = scalar_state(&vcpu);
        let before_system = system_state(&vcpu);
        let error = vcpu.step().expect_err("SVM frontier must deliver #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(scalar_state(&vcpu), before_scalar, "{name}");
        assert_eq!(system_state(&vcpu), before_system, "{name}");
    }
}

#[test]
fn jit_disabled_sgx_roots_exit_at_the_exact_faulting_frontier() {
    for (name, modrm) in [("ENCLV", 0xC0), ("ENCLS", 0xCF), ("ENCLU", 0xD7)] {
        let memory = memory_with_code(&[
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp disabled SGX root
            0x90, 0x90, // unreachable padding
            0x0F, 0x01, modrm,
        ]);
        let mut vcpu = test_vcpu(memory);
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at disabled SGX root")
            .expect("supported prefix must remain native before SGX frontier");

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rax, 0x1234_5678, "{name}");
        assert_eq!(vcpu.regs.rip, 9, "{name}");

        let before_scalar = scalar_state(&vcpu);
        let before_system = system_state(&vcpu);
        let error = vcpu.step().expect_err("SGX frontier must deliver #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(scalar_state(&vcpu), before_scalar, "{name}");
        assert_eq!(system_state(&vcpu), before_system, "{name}");
    }
}

#[test]
fn jit_disabled_pconfig_exits_at_the_exact_faulting_frontier() {
    for (name, instruction, apx) in [
        ("legacy", &[0x0F, 0x01, 0xC5][..], false),
        ("VEX2", &[0xC5, 0xF8, 0x01, 0xC5][..], false),
        ("VEX3", &[0xC4, 0xE1, 0x78, 0x01, 0xC5][..], false),
        ("EVEX", &[0x62, 0xF1, 0x7C, 0x08, 0x01, 0xC5][..], false),
        ("REX2", &[0xD5, 0x80, 0x01, 0xC5][..], true),
    ] {
        let mut code = vec![
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp disabled PCONFIG
            0x90, 0x90, // unreachable padding
        ];
        code.extend_from_slice(instruction);
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.set_apx_enabled(apx);
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at disabled PCONFIG")
            .expect("supported prefix must remain native before PCONFIG frontier");

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rax, 0x1234_5678, "{name}");
        assert_eq!(vcpu.regs.rip, 9, "{name}");

        let before_scalar = scalar_state(&vcpu);
        let before_system = system_state(&vcpu);
        let error = vcpu.step().expect_err("PCONFIG frontier must deliver #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(scalar_state(&vcpu), before_scalar, "{name}");
        assert_eq!(system_state(&vcpu), before_system, "{name}");
    }
}

#[test]
fn jit_disabled_group7_residual_forms_exit_at_the_exact_faulting_frontier() {
    for (name, instruction, apx) in [
        ("XRESLDTRK", &[0xF2, 0x0F, 0x01, 0xE9][..], false),
        (
            "RSTORSSP memory",
            &[0xF3, 0x0F, 0x01, 0x6C, 0x24, 0x7F][..],
            false,
        ),
        ("INVLPGB", &[0x0F, 0x01, 0xFE][..], false),
        ("reserved REX2", &[0xD5, 0x80, 0x01, 0xC7][..], true),
    ] {
        let mut code = vec![
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp disabled Group 7 form
            0x90, 0x90, // unreachable padding
        ];
        code.extend_from_slice(instruction);
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.set_apx_enabled(apx);
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at disabled Group 7 form")
            .expect("supported prefix must remain native before Group 7 frontier");

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rax, 0x1234_5678, "{name}");
        assert_eq!(vcpu.regs.rip, 9, "{name}");

        let before_scalar = scalar_state(&vcpu);
        let before_system = system_state(&vcpu);
        let error = vcpu
            .step()
            .expect_err("disabled Group 7 frontier must deliver #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(scalar_state(&vcpu), before_scalar, "{name}");
        assert_eq!(system_state(&vcpu), before_system, "{name}");
    }
}

#[test]
fn jit_reserved_group9_forms_exit_at_the_exact_faulting_frontier() {
    for (name, instruction, apx) in [
        ("/0 register", &[0x0F, 0xC7, 0xC0][..], false),
        (
            "/2 memory",
            &[0xF3, 0x0F, 0xC7, 0x54, 0x24, 0x7F][..],
            false,
        ),
        (
            "REX2 /0, APX disabled",
            &[0xD5, 0x80, 0xC7, 0xC0][..],
            false,
        ),
        ("REX2 /0, APX enabled", &[0xD5, 0x80, 0xC7, 0xC0][..], true),
    ] {
        let mut code = vec![
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp reserved Group 9 form
            0x90, 0x90, // unreachable padding
        ];
        code.extend_from_slice(instruction);
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.set_apx_enabled(apx);
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at reserved Group 9 form")
            .expect("supported prefix must remain native before Group 9 frontier");

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rax, 0x1234_5678, "{name}");
        assert_eq!(vcpu.regs.rip, 9, "{name}");

        let before_scalar = scalar_state(&vcpu);
        let before_system = system_state(&vcpu);
        let error = vcpu
            .step()
            .expect_err("reserved Group 9 frontier must deliver #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}"
        );
        assert_eq!(scalar_state(&vcpu), before_scalar, "{name}");
        assert_eq!(system_state(&vcpu), before_system, "{name}");
    }
}
