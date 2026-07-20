//! Direct/native x86-64 JIT differentials for SMSW destinations and faults.

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
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
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
            vcpu.step().expect("direct SMSW instruction").is_none(),
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

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

#[test]
fn jit_smsw_register_widths_stack_aliases_and_rex2_match_direct() {
    let memory = memory_with_code(&[
        0x66, 0x0F, 0x01, 0xE0, // SMSW AX
        0x0F, 0x01, 0xE1, // SMSW ECX
        0x48, 0x0F, 0x01, 0xE2, // SMSW RDX
        0x66, 0x0F, 0x01, 0xE4, // SMSW SP
        0x0F, 0x01, 0xE5, // SMSW EBP
        0xD5, 0x91, 0x01, 0xE7, // SMSW R31D
        0xEB, 0x00, // JMP HLT
        0xF4,
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.regs.rax = 0xAAAA_BBBB_CCCC_DDDD;
        vcpu.regs.rcx = u64::MAX;
        vcpu.regs.rdx = 0x2222;
        vcpu.regs.r31 = 0x3131_3131_3131_3131;
    }

    run_direct_to(&mut direct, 24);
    let region = native
        .jit_compile_region()
        .expect("compile SMSW register region")
        .expect("all SMSW register widths must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 24);
}

#[test]
fn jit_smsw_memory_forms_match_direct_for_legacy_stack_and_egpr_addresses() {
    let code = [
        0x0F, 0x01, 0x63, 0x02, // SMSW word ptr [RBX+2]
        0x48, 0x0F, 0x01, 0x64, 0x4C, 0x04, // SMSW word ptr [RSP+RCX*2+4]
        0xD5, 0xB3, 0x01, 0x24, 0xD1, // SMSW word ptr [R25+R26*8]
        0xEB, 0x00, 0xF4,
    ];
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    let mut direct = test_vcpu(direct_memory.clone());
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.set_jit_mem(true);
        vcpu.regs.rbx = 0x3000;
        vcpu.regs.rsp = 0x4000;
        vcpu.regs.rcx = 0x10;
        vcpu.regs.r25 = 0x5000;
        vcpu.regs.r26 = 4;
    }
    for memory in [&direct_memory, &native_memory] {
        for address in [0x3002, 0x4024, 0x5020] {
            memory
                .write_slice(&[0xA5; 4], GuestAddress(address - 1))
                .unwrap();
        }
    }

    run_direct_to(&mut direct, 17);
    let region = native
        .jit_compile_region()
        .expect("compile SMSW memory region")
        .expect("helper-backed SMSW memory forms must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 17);
    for address in [0x3002, 0x4024, 0x5020] {
        let mut direct_observed = [0; 4];
        let mut native_observed = [0; 4];
        direct_memory
            .read_slice(&mut direct_observed, GuestAddress(address - 1))
            .unwrap();
        native_memory
            .read_slice(&mut native_observed, GuestAddress(address - 1))
            .unwrap();
        assert_eq!(native_observed, direct_observed, "{address:#x}");
        assert_eq!(native_observed, [0xA5, 0x33, 0x00, 0xA5], "{address:#x}");
    }
}

#[test]
fn jit_smsw_apx_and_umip_fault_priority_is_precise_and_noncommitting() {
    for (name, apx_enabled, expected_vector) in [("APX", false, 6), ("UMIP", true, 13)] {
        let memory = memory_with_code(&[
            0xD5, 0x91, 0x01, 0xE7, // SMSW R31D
            0xEB, 0x00, 0xF4,
        ]);
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cs.selector = 3;
        vcpu.sregs.cr4 |= 1 << 11;
        vcpu.set_apx_enabled(apx_enabled);
        vcpu.regs.r31 = 0x3131_3131_3131_3131;
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically guarded SMSW")
            .expect("dynamic APX/UMIP state must not block admission");
        vcpu.jit_run_region_native(&region);

        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{name}");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name} fault priority changed: {error}"
        );
    }
}

#[test]
fn jit_smsw_umip_guard_precedes_memory_and_memory_fault_restarts_exactly() {
    for (name, umip, expected_vector) in [("UMIP", true, Some(13)), ("memory", false, None)] {
        let memory = memory_with_code(&[0x0F, 0x01, 0x20, 0xEB, 0x00, 0xF4]);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.rax = 0x20_000;
        vcpu.sregs.cs.selector = 3;
        if umip {
            vcpu.sregs.cr4 |= 1 << 11;
        }
        vcpu.set_jit_mem(true);
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile faulting SMSW memory form")
            .expect("dynamic SMSW memory fault must not block admission");
        vcpu.jit_run_region_native(&region);
        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{name}");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");

        let error = exception_without_idt(&mut vcpu);
        if let Some(vector) = expected_vector {
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "UMIP must precede the invalid memory address: {error}"
            );
        } else {
            assert!(
                !error.contains("IDT entry 13 not present"),
                "UMIP-clear execution must reach the memory fault: {error}"
            );
        }
    }
}

#[test]
fn jit_rejects_smsw_outside_cs_l_and_preserves_direct_mode_widths() {
    for (db, code, expected) in [
        (true, &[0x0F, 0x01, 0xE0, 0xEB, 0x00, 0xF4][..], 0x0005_0033),
        (
            false,
            &[0x0F, 0x01, 0xE0, 0xEB, 0x00, 0xF4][..],
            0xAAAA_BBBB_CCCC_0033,
        ),
    ] {
        let memory = memory_with_code(code);
        let mut compatibility = test_vcpu(memory);
        compatibility.sregs.cs.l = false;
        compatibility.sregs.cs.db = db;
        compatibility.regs.rax = 0xAAAA_BBBB_CCCC_DDDD;
        assert!(
            compatibility.jit_compile_region().unwrap().is_none(),
            "compatibility-mode SMSW must retain direct width/address semantics"
        );
        assert!(compatibility.step().unwrap().is_none());
        assert_eq!(compatibility.regs.rax, expected, "CS.D={db}");
    }
}
