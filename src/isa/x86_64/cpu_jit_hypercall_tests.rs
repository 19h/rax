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
fn jit_disabled_vmx_controls_exit_at_the_exact_faulting_frontier() {
    for (name, modrm) in [
        ("VMLAUNCH", 0xC2),
        ("VMRESUME", 0xC3),
        ("VMXOFF", 0xC4),
        ("VMFUNC", 0xD4),
    ] {
        let memory = memory_with_code(&[
            0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,12345678h
            0xEB, 0x02, // jmp disabled VMX control
            0x90, 0x90, // unreachable padding
            0x0F, 0x01, modrm,
        ]);
        let mut vcpu = test_vcpu(memory);
        let region = vcpu
            .jit_compile_region()
            .expect("compile region ending at disabled VMX control")
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
