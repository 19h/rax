//! Direct/native x86-64 JIT differentials for SLDT/STR destinations and faults.

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
    vcpu.sregs.ldt.selector = 0x1357;
    vcpu.sregs.tr.selector = 0xBEEF;
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
            vcpu.step().expect("direct SLDT/STR instruction").is_none(),
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
fn jit_selector_register_widths_stack_aliases_rex2_and_both_sources_match_direct() {
    let memory = memory_with_code(&[
        0x66, 0x0F, 0x00, 0xC0, // SLDT AX
        0x0F, 0x00, 0xC9, // STR ECX
        0x48, 0x0F, 0x00, 0xC2, // SLDT RDX
        0x66, 0x0F, 0x00, 0xCC, // STR SP
        0x0F, 0x00, 0xC5, // SLDT EBP
        0xD5, 0x91, 0x00, 0xCF, // STR R31D
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
        .expect("compile SLDT/STR register region")
        .expect("all selector register widths must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 24);
    assert_eq!(native.regs.rax & 0xFFFF, 0x1357);
    assert_eq!(native.regs.rcx, 0xBEEF);
    assert_eq!(native.regs.rdx, 0x1357);
    assert_eq!(native.regs.rsp & 0xFFFF, 0xBEEF);
    assert_eq!(native.regs.rbp, 0x1357);
    assert_eq!(native.regs.r31, 0xBEEF);
}

#[test]
fn jit_selector_memory_forms_match_direct_and_store_exactly_two_bytes() {
    let code = [
        0x0F, 0x00, 0x43, 0x02, // SLDT word ptr [RBX+2]
        0x48, 0x0F, 0x00, 0x4C, 0x4C, 0x04, // STR word ptr [RSP+RCX*2+4]
        0xD5, 0xB3, 0x00, 0x0C, 0xD1, // STR word ptr [R25+R26*8]
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
        .expect("compile selector memory region")
        .expect("helper-backed selector memory forms must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rip, 17);
    for (address, expected) in [(0x3002, 0x1357_u16), (0x4024, 0xBEEF), (0x5020, 0xBEEF)] {
        let mut direct_observed = [0; 4];
        let mut native_observed = [0; 4];
        direct_memory
            .read_slice(&mut direct_observed, GuestAddress(address - 1))
            .unwrap();
        native_memory
            .read_slice(&mut native_observed, GuestAddress(address - 1))
            .unwrap();
        assert_eq!(native_observed, direct_observed, "{address:#x}");
        assert_eq!(
            native_observed,
            [0xA5, expected as u8, (expected >> 8) as u8, 0xA5],
            "{address:#x}"
        );
    }
    assert_eq!(native.regs.rflags, direct.regs.rflags);
}

#[test]
fn jit_selector_apx_mode_umip_and_memory_fault_priority_is_precise_noncommitting() {
    for (name, code, apx, cr0, vm, umip, expected_vector) in [
        (
            "APX",
            &[0xD5, 0x91, 0x00, 0xC7, 0xEB, 0x00, 0xF4][..],
            false,
            0,
            true,
            true,
            6,
        ),
        (
            "real mode",
            &[0x0F, 0x00, 0x08, 0xEB, 0x00, 0xF4],
            true,
            0,
            false,
            true,
            6,
        ),
        (
            "VM86",
            &[0x0F, 0x00, 0x08, 0xEB, 0x00, 0xF4],
            true,
            1,
            true,
            true,
            6,
        ),
        (
            "UMIP",
            &[0x0F, 0x00, 0x08, 0xEB, 0x00, 0xF4],
            true,
            1,
            false,
            true,
            13,
        ),
    ] {
        let memory = memory_with_code(code);
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cs.selector = 3;
        vcpu.sregs.cr0 = cr0;
        vcpu.sregs.cr4 = u64::from(umip) << 11;
        if vm {
            vcpu.regs.rflags |= flags::bits::VM;
        }
        vcpu.set_apx_enabled(apx);
        vcpu.set_jit_mem(true);
        vcpu.regs.rax = 0x20_000;
        vcpu.regs.r31 = 0x3131_3131_3131_3131;
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically guarded selector store")
            .expect("dynamic selector-store faults must not block admission");
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

    let memory = memory_with_code(&[0x0F, 0x00, 0x08, 0xEB, 0x00, 0xF4]);
    let mut memory_fault = test_vcpu(memory);
    memory_fault.sregs.cs.selector = 3;
    memory_fault.sregs.cr4 = 0;
    memory_fault.regs.rax = 0x20_000;
    memory_fault.set_jit_mem(true);
    let before = memory_fault.regs.clone();
    let region = memory_fault
        .jit_compile_region()
        .expect("compile memory-faulting selector store")
        .expect("dynamic memory fault must not block admission");
    memory_fault.jit_run_region_native(&region);
    assert_eq!(gprs(&memory_fault.regs), gprs(&before));
    assert_eq!(memory_fault.regs.rflags, before.rflags);
    assert_eq!(memory_fault.regs.rip, 0);
    let error = exception_without_idt(&mut memory_fault);
    assert!(
        !error.contains("IDT entry 13 not present"),
        "UMIP-clear selector store must reach the memory fault: {error}"
    );
}

#[test]
fn selector_helper_reads_authoritative_state_and_rejects_invalid_inputs() {
    use crate::smir::lower::runtime::GuestRegs;

    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory);
    vcpu.sregs.ldt.selector = 0x2468;
    vcpu.sregs.tr.selector = 0xBEEF;
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;

    assert_eq!(unsafe { rax_jit_system_selector(&mut state, 0) }, 0x2468);
    assert_eq!(unsafe { rax_jit_system_selector(&mut state, 1) }, 0xBEEF);
    assert_eq!(unsafe { rax_jit_system_selector(&mut state, 2) }, 0);
    assert_eq!(
        unsafe { rax_jit_system_selector(std::ptr::null_mut(), 0) },
        0
    );
    state.ctx = 0;
    assert_eq!(unsafe { rax_jit_system_selector(&mut state, 0) }, 0);
}

#[test]
fn jit_callout_lldt_then_native_sldt_uses_authoritative_selector_and_verifies() {
    let memory = memory_with_code(&[
        0xE8, 0xFB, 0x00, 0x00, 0x00, // CALL 100h
        0x0F, 0x00, 0xC3, // SLDT EBX
        0x0F, 0x00, 0xCA, // STR EDX
        0xEB, 0x00, // JMP HLT
        0xF4,
    ]);
    memory
        .write_slice(
            &[
                0xB8, 0x68, 0x24, 0x00, 0x00, // MOV EAX,2468h
                0x0F, 0x00, 0xD0, // LLDT AX (direct callout frontier)
                0xC3, // RET
            ],
            GuestAddress(0x100),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.sregs.ldt.selector = 0x1357;
    vcpu.sregs.tr.selector = 0xBEEF;
    vcpu.regs.rsp = 0x8000;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(true);

    let region = vcpu
        .jit_compile_region()
        .expect("compile CALL/LLDT/SLDT region")
        .expect("selector read after interpreter callout must remain native");
    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.sregs.ldt.selector, 0x2468);
    assert_eq!(vcpu.sregs.tr.selector, 0xBEEF);
    assert_eq!(vcpu.regs.rax, 0x2468);
    assert_eq!(vcpu.regs.rbx, 0x2468);
    assert_eq!(vcpu.regs.rdx, 0xBEEF);
    assert_eq!(vcpu.regs.rsp, 0x8000);
    assert_eq!(vcpu.regs.rip, 13);
}

#[test]
fn jit_rejects_selector_stores_outside_cs_l_and_direct_preserves_mode_widths() {
    for (db, code, expected) in [
        (true, &[0x0F, 0x00, 0xC0, 0xEB, 0x00, 0xF4][..], 0x1357),
        (
            false,
            &[0x0F, 0x00, 0xC0, 0xEB, 0x00, 0xF4][..],
            0xAAAA_BBBB_CCCC_1357,
        ),
    ] {
        let memory = memory_with_code(code);
        let mut compatibility = test_vcpu(memory);
        compatibility.sregs.cs.l = false;
        compatibility.sregs.cs.db = db;
        compatibility.regs.rax = 0xAAAA_BBBB_CCCC_DDDD;
        assert!(
            compatibility.jit_compile_region().unwrap().is_none(),
            "compatibility-mode SLDT must retain direct width/address semantics"
        );
        assert!(compatibility.step().unwrap().is_none());
        assert_eq!(compatibility.regs.rax, expected, "CS.D={db}");
    }
}
