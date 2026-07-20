//! Direct/native x86-64 JIT differentials for LMSW sources and fault frontiers.

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
    vcpu.sregs.cr0 = 0x0005_0031;
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
fn jit_lmsw_register_stack_aliases_and_rex2_match_direct_at_exact_handoff() {
    for (name, instruction, source_index) in [
        ("RAX", &[0x0F, 0x01, 0xF0][..], 0_usize),
        ("RSP", &[0x0F, 0x01, 0xF4], 4),
        ("RBP", &[0x0F, 0x01, 0xF5], 5),
        ("R15", &[0x41, 0x0F, 0x01, 0xF7], 15),
        ("R31", &[0xD5, 0x91, 0x01, 0xF7], 31),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x48, 0xFF, 0xC3, 0xF4]); // INC RBX; HLT
        let memory = memory_with_code(&code);
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(source_index >= 16);
            vcpu.regs.rbx = 0xB0B0_B0B0_B0B0_B0B0;
            let mut values = gprs(&vcpu.regs);
            values[source_index] = 0x1234_5678_9ABC_DEF0;
            vcpu.regs.rax = values[0];
            vcpu.regs.rsp = values[4];
            vcpu.regs.rbp = values[5];
            vcpu.regs.r15 = values[15];
            vcpu.regs.r31 = values[31];
        }

        assert!(direct.step().expect("direct LMSW").is_none(), "{name}");
        let region = native
            .jit_compile_region()
            .expect("compile LMSW register region")
            .expect("strict LMSW register form must be native eligible");
        native.jit_run_region_native(&region);

        assert_eq!(native.sregs.cr0, direct.sregs.cr0, "{name}");
        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}");
        assert_eq!(native.regs.rip, instruction.len() as u64, "{name}");
        assert_eq!(native.regs.rbx, 0xB0B0_B0B0_B0B0_B0B0, "{name}");
        assert_eq!(native.sregs.cr0 & 1, 1, "LMSW cannot clear PE: {name}");
    }
}

#[test]
fn jit_lmsw_memory_rex2_address_reads_two_bytes_and_matches_direct() {
    let code = [
        0xD5, 0xB3, 0x01, 0x34, 0xD1, // LMSW word ptr [R25+R26*8]
        0x48, 0xFF, 0xC3, // INC RBX (must remain outside the region frontier)
        0xF4,
    ];
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    let mut direct = test_vcpu(direct_memory.clone());
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.set_jit_mem(true);
        vcpu.regs.r25 = 0x3000;
        vcpu.regs.r26 = 4;
        vcpu.regs.rbx = 0xB0B0_B0B0_B0B0_B0B0;
    }
    let source_addr = 0x3000 + 4 * 8;
    for memory in [&direct_memory, &native_memory] {
        memory
            .write_slice(&[0x0E, 0xCA, 0xA5], GuestAddress(source_addr))
            .unwrap();
    }

    assert!(direct.step().expect("direct memory LMSW").is_none());
    let region = native
        .jit_compile_region()
        .expect("compile LMSW memory region")
        .expect("helper-backed LMSW memory source must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.cr0, direct.sregs.cr0);
    assert_eq!(native.sregs.cr0 & 0xF, 0xF);
    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 5);
    assert_eq!(native.regs.rbx, 0xB0B0_B0B0_B0B0_B0B0);
    let mut native_source = [0; 3];
    native_memory
        .read_slice(&mut native_source, GuestAddress(source_addr))
        .unwrap();
    assert_eq!(native_source, [0x0E, 0xCA, 0xA5]);
}

#[test]
fn jit_lmsw_apx_then_cpl_fault_priority_is_precise_and_noncommitting() {
    for (name, apx_enabled, cpl, expected_vector) in [
        ("APX", false, 0_u16, 6),
        ("CPL", true, 3_u16, 13),
        ("APX before CPL", false, 3_u16, 6),
    ] {
        let memory = memory_with_code(&[0xD5, 0x91, 0x01, 0xF7, 0xF4]);
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cs.selector = cpl;
        vcpu.set_apx_enabled(apx_enabled);
        vcpu.regs.r31 = 0xF;
        let before_regs = vcpu.regs.clone();
        let before_cr0 = vcpu.sregs.cr0;

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically guarded LMSW")
            .expect("dynamic APX/CPL state must not block admission");
        vcpu.jit_run_region_native(&region);

        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(vcpu.regs.rflags, before_regs.rflags, "{name}");
        assert_eq!(vcpu.sregs.cr0, before_cr0, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name} fault priority changed: {error}"
        );
    }
}

#[test]
fn jit_lmsw_cpl_guard_precedes_memory_and_memory_fault_restarts_exactly() {
    for (name, cpl, expected_vector) in [("CPL", 3_u16, Some(13)), ("memory", 0, None)] {
        let memory = memory_with_code(&[0x0F, 0x01, 0x30, 0xF4]);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.rax = 0x20_000;
        vcpu.sregs.cs.selector = cpl;
        vcpu.set_jit_mem(true);
        let before_regs = vcpu.regs.clone();
        let before_cr0 = vcpu.sregs.cr0;

        let region = vcpu
            .jit_compile_region()
            .expect("compile faulting LMSW memory form")
            .expect("dynamic LMSW source fault must not block admission");
        vcpu.jit_run_region_native(&region);
        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(vcpu.regs.rflags, before_regs.rflags, "{name}");
        assert_eq!(vcpu.sregs.cr0, before_cr0, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");

        let error = exception_without_idt(&mut vcpu);
        if let Some(vector) = expected_vector {
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "CPL check must precede the invalid source address: {error}"
            );
        } else {
            assert!(
                !error.contains("IDT entry 13 not present"),
                "CPL0 execution must reach the memory fault: {error}"
            );
        }
    }
}

#[test]
fn jit_rejects_lmsw_outside_cs_l_and_preserves_direct_fixed_source_width() {
    let memory = memory_with_code(&[0x66, 0x0F, 0x01, 0xF0, 0xF4]);
    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.regs.rax = 0xAAAA_BBBB_CCCC_DDDE;
    let old_cr0 = compatibility.sregs.cr0;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode LMSW must retain direct execution"
    );
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.sregs.cr0, (old_cr0 & !0xF) | 0xF);
    assert_eq!(compatibility.regs.rip, 4);
}
