//! Direct/native x86-64 JIT differentials for SLDT/STR/LLDT state and faults.

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
    vcpu.sregs.gdt.base = 0x1000;
    vcpu.sregs.gdt.limit = 0x1F;
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

fn ldt_descriptor(
    base: u64,
    raw_limit: u32,
    dpl: u8,
    present: bool,
    granularity: bool,
    avl: bool,
) -> [u8; 16] {
    assert!(raw_limit <= 0xF_FFFF);
    let mut low = u64::from(raw_limit & 0xFFFF)
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (0x2_u64 << 40)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from((raw_limit >> 16) & 0xF) << 48)
        | (u64::from(avl) << 52)
        | (((base >> 24) & 0xFF) << 56);
    if granularity {
        low |= 1 << 55;
    }
    let high = (base >> 32) & 0xFFFF_FFFF;
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&low.to_le_bytes());
    bytes[8..].copy_from_slice(&high.to_le_bytes());
    bytes
}

fn install_lldt_descriptor(memory: &GuestMemoryMmap, descriptor: &[u8; 16]) {
    memory
        .write_slice(descriptor, GuestAddress(0x1010))
        .unwrap();
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
    let mut vcpu = test_vcpu(memory.clone());
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
fn lldt_helper_rolls_back_speculative_records_and_never_probes_mmio() {
    use crate::smir::lower::runtime::GuestRegs;

    const TRACE_SENTINEL: (u8, u64, u8, u64) = (0, 0xAA, 1, 0x55);
    const LAPIC_BASE: u64 = 0xFEE0_0000;
    let memory = memory_with_code(&[]);
    let mut invalid = ldt_descriptor(0x1234_5000, 0xFFFF, 0, true, false, false);
    invalid[5] = (invalid[5] & 0xF0) | 0x9;
    install_lldt_descriptor(&memory, &invalid);
    memory
        .write_slice(&0x10_u16.to_le_bytes(), GuestAddress(0x3000))
        .unwrap();
    let mut vcpu = test_vcpu(memory.clone());
    crate::vm::vcpu::VCpu::set_mem_recording(&mut vcpu, true);
    let before_ldtr = segment_fingerprint(&vcpu.sregs.ldt);
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;

    for (name, operand, encoding, gdt_base) in [
        ("invalid register descriptor", 0x10, 0, 0x1000),
        ("invalid memory descriptor", 0x3000, 1, 0x1000),
        ("MMIO source", LAPIC_BASE, 1, 0x1000),
        ("MMIO descriptor", 0x10, 0, LAPIC_BASE - 0x10),
    ] {
        vcpu.sregs.gdt.base = gdt_base;
        vcpu.jit_mem_trace = Some(vec![TRACE_SENTINEL]);
        assert_eq!(
            unsafe { rax_jit_system_selector_load(&mut state, operand, encoding) },
            0,
            "{name}"
        );
        assert_eq!(
            vcpu.jit_mem_trace.as_deref(),
            Some(&[TRACE_SENTINEL][..]),
            "{name}"
        );
        let mut records = Vec::new();
        crate::vm::vcpu::VCpu::drain_mem_records(&mut vcpu, &mut records);
        assert!(records.is_empty(), "{name}: {records:?}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.ldt), before_ldtr, "{name}");
    }

    install_lldt_descriptor(
        &memory,
        &ldt_descriptor(0x1234_5000, 0xFFFF, 0, true, false, false),
    );
    vcpu.sregs.gdt.base = 0x1000;
    vcpu.jit_mem_trace = Some(vec![TRACE_SENTINEL]);
    assert_eq!(
        unsafe { rax_jit_system_selector_load(&mut state, 0x10, 0) },
        1
    );
    let trace = vcpu.jit_mem_trace.as_ref().unwrap();
    assert_eq!(trace.len(), 3);
    assert_eq!(trace[0], TRACE_SENTINEL);
    assert_eq!((trace[1].0, trace[1].1, trace[1].2), (0, 0x1010, 8));
    assert_eq!((trace[2].0, trace[2].1, trace[2].2), (0, 0x1018, 8));
    let mut records = Vec::new();
    crate::vm::vcpu::VCpu::drain_mem_records(&mut vcpu, &mut records);
    assert_eq!(records.len(), 2);
    assert_eq!(vcpu.sregs.ldt.selector, 0x10);
}

#[test]
fn jit_lldt_register_stack_aliases_rex2_and_hidden_cache_match_direct_at_handoff() {
    for (name, instruction, source_index) in [
        ("RAX", &[0x0F, 0x00, 0xD0][..], 0_usize),
        ("RSP", &[0x0F, 0x00, 0xD4], 4),
        ("RBP", &[0x0F, 0x00, 0xD5], 5),
        ("R15", &[0x41, 0x0F, 0x00, 0xD7], 15),
        ("R31", &[0xD5, 0x91, 0x00, 0xD7], 31),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x48, 0xFF, 0xC3, 0xF4]); // INC RBX; HLT
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        let descriptor = ldt_descriptor(0xFFFF_8000_1234_5000, 0xA_BCDE, 3, true, true, true);
        install_lldt_descriptor(&direct_memory, &descriptor);
        install_lldt_descriptor(&native_memory, &descriptor);
        let mut direct = test_vcpu(direct_memory);
        let mut native = test_vcpu(native_memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(source_index >= 16);
            vcpu.set_jit_mem(true);
            vcpu.regs.rbx = 0xB0B0_B0B0_B0B0_B0B0;
            match source_index {
                0 => vcpu.regs.rax = 0xA5A5_5A5A_0000_0010,
                4 => vcpu.regs.rsp = 0xA5A5_5A5A_0000_0010,
                5 => vcpu.regs.rbp = 0xA5A5_5A5A_0000_0010,
                15 => vcpu.regs.r15 = 0xA5A5_5A5A_0000_0010,
                31 => vcpu.regs.r31 = 0xA5A5_5A5A_0000_0010,
                _ => unreachable!(),
            }
        }

        assert!(direct.step().expect("direct LLDT").is_none(), "{name}");
        let region = native
            .jit_compile_region()
            .expect("compile LLDT register region")
            .expect("strict LLDT register form must be native eligible");
        native.jit_run_region_native(&region);

        assert_eq!(
            segment_fingerprint(&native.sregs.ldt),
            segment_fingerprint(&direct.sregs.ldt),
            "{name}"
        );
        assert_eq!(native.sregs.ldt.selector, 0x10, "{name}");
        assert_eq!(native.sregs.ldt.base, 0xFFFF_8000_1234_5000, "{name}");
        assert_eq!(native.sregs.ldt.limit, 0xA_BCDE_FFF, "{name}");
        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}");
        assert_eq!(native.regs.rip, instruction.len() as u64, "{name}");
        assert_eq!(native.regs.rbx, 0xB0B0_B0B0_B0B0_B0B0, "{name}");
    }
}

#[test]
fn jit_lldt_rex2_memory_source_reads_two_bytes_and_matches_direct() {
    let code = [
        0xD5, 0xB3, 0x00, 0x14, 0xD1, // LLDT word ptr [R25+R26*8]
        0x48, 0xFF, 0xC3, // INC RBX (outside the LLDT serialization frontier)
        0xF4,
    ];
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    let descriptor = ldt_descriptor(0x1234_5678, 0xF_FFFF, 0, true, true, false);
    for memory in [&direct_memory, &native_memory] {
        install_lldt_descriptor(memory, &descriptor);
        memory
            .write_slice(&[0x10, 0x00, 0xA5], GuestAddress(0x3020))
            .unwrap();
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.set_jit_mem(true);
        vcpu.regs.r25 = 0x3000;
        vcpu.regs.r26 = 4;
        vcpu.regs.rbx = 0xB0B0_B0B0_B0B0_B0B0;
    }

    assert!(direct.step().expect("direct memory LLDT").is_none());
    let region = native
        .jit_compile_region()
        .expect("compile LLDT memory region")
        .expect("helper-backed LLDT memory source must be native eligible");
    native.jit_run_region_verified(&region);

    assert_eq!(
        segment_fingerprint(&native.sregs.ldt),
        segment_fingerprint(&direct.sregs.ldt)
    );
    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 5);
    assert_eq!(native.regs.rbx, 0xB0B0_B0B0_B0B0_B0B0);
    let mut source = [0_u8; 3];
    native_memory
        .read_slice(&mut source, GuestAddress(0x3020))
        .unwrap();
    assert_eq!(source, [0x10, 0x00, 0xA5]);
}

#[test]
fn jit_lldt_null_selectors_match_direct_and_never_access_the_gdt() {
    for selector in 0_u64..=3 {
        let direct_memory = memory_with_code(&[0x0F, 0x00, 0xD0, 0xF4]);
        let native_memory = memory_with_code(&[0x0F, 0x00, 0xD0, 0xF4]);
        let mut direct = test_vcpu(direct_memory);
        let mut native = test_vcpu(native_memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_jit_mem(true);
            vcpu.regs.rax = selector;
            vcpu.sregs.gdt.base = 0x20_000;
            vcpu.sregs.gdt.limit = 0;
            vcpu.sregs.ldt.base = 0xDEAD_BEEF;
            vcpu.sregs.ldt.limit = 0x1234;
            vcpu.sregs.ldt.selector = 0x2468;
            vcpu.sregs.ldt.present = true;
            vcpu.sregs.ldt.unusable = false;
        }

        assert!(direct.step().expect("direct null LLDT").is_none());
        let region = native
            .jit_compile_region()
            .expect("compile null LLDT")
            .expect("null LLDT must be native eligible without a mapped GDT");
        native.jit_run_region_verified(&region);

        assert_eq!(
            segment_fingerprint(&native.sregs.ldt),
            segment_fingerprint(&direct.sregs.ldt),
            "selector={selector}"
        );
        assert_eq!(u64::from(native.sregs.ldt.selector), selector);
        assert!(native.sregs.ldt.unusable);
        assert!(!native.sregs.ldt.present);
        assert_eq!(native.sregs.ldt.base, 0);
        assert_eq!(native.regs.rip, 3);
    }
}

#[test]
fn jit_lldt_cross_region_source_and_descriptor_faults_are_noncommitting() {
    for name in ["source", "descriptor"] {
        let memory = if name == "source" {
            memory_with_code(&[0x0F, 0x00, 0x10, 0xF4]) // LLDT word ptr [RAX]
        } else {
            let memory =
                Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x2000)]).unwrap());
            memory
                .write_slice(&[0x0F, 0x00, 0xD0, 0xF4], GuestAddress(0))
                .unwrap();
            let descriptor = ldt_descriptor(0x1234_5000, 0xFFFF, 0, true, false, false);
            memory
                .write_slice(&descriptor[..8], GuestAddress(0x1FF8))
                .unwrap();
            memory
        };
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.set_jit_mem(true);
        vcpu.regs.rax = if name == "source" { 0xFFFF } else { 0x10 };
        if name == "source" {
            install_lldt_descriptor(
                &memory,
                &ldt_descriptor(0x1234_5000, 0xFFFF, 0, true, false, false),
            );
        } else {
            // Selector 0x10 resolves to 0x1FF8: the low descriptor qword is
            // mapped, while the upper qword begins at the unmapped 0x2000.
            vcpu.sregs.gdt.base = 0x1FE8;
            vcpu.sregs.gdt.limit = 0x1F;
        }
        vcpu.sregs.ldt.base = 0xDEAD_BEEF;
        vcpu.sregs.ldt.limit = 0x1234;
        vcpu.sregs.ldt.selector = 0x2468;
        vcpu.sregs.ldt.present = true;
        vcpu.sregs.ldt.unusable = false;
        let before_regs = vcpu.regs.clone();
        let before_ldtr = segment_fingerprint(&vcpu.sregs.ldt);

        let region = vcpu
            .jit_compile_region()
            .expect("compile boundary-faulting LLDT")
            .unwrap_or_else(|| panic!("{name}: LLDT boundary fault must remain native eligible"));
        vcpu.jit_run_region_native(&region);

        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(vcpu.regs.rflags, before_regs.rflags, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.ldt), before_ldtr, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        let error = format!(
            "{:#}",
            vcpu.step()
                .expect_err("direct LLDT boundary access must fault")
        );
        assert!(
            !error.contains("IDT entry 13 not present"),
            "{name} must reach its memory fault: {error}"
        );
        assert_eq!(segment_fingerprint(&vcpu.sregs.ldt), before_ldtr, "{name}");
    }
}

#[test]
fn jit_lldt_apx_mode_cpl_and_descriptor_faults_deopt_without_commit() {
    for (name, instruction, configure, descriptor, expected_vector) in [
        (
            "APX",
            &[0xD5, 0x91, 0x00, 0xD7][..],
            (false, true, false, 0_u16),
            ldt_descriptor(0, 0, 0, true, false, false),
            6,
        ),
        (
            "real mode",
            &[0x0F, 0x00, 0xD0],
            (true, false, false, 0),
            ldt_descriptor(0, 0, 0, true, false, false),
            6,
        ),
        (
            "VM86",
            &[0x0F, 0x00, 0xD0],
            (true, true, true, 0),
            ldt_descriptor(0, 0, 0, true, false, false),
            6,
        ),
        (
            "CPL",
            &[0x0F, 0x00, 0xD0],
            (true, true, false, 3),
            ldt_descriptor(0, 0, 0, true, false, false),
            13,
        ),
        (
            "wrong type",
            &[0x0F, 0x00, 0xD0],
            (true, true, false, 0),
            {
                let mut value = ldt_descriptor(0, 0, 0, true, false, false);
                value[5] = (value[5] & 0xF0) | 0x9;
                value
            },
            13,
        ),
        (
            "not present",
            &[0x0F, 0x00, 0xD0],
            (true, true, false, 0),
            ldt_descriptor(0, 0, 0, false, false, false),
            11,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let memory = memory_with_code(&code);
        install_lldt_descriptor(&memory, &descriptor);
        let mut vcpu = test_vcpu(memory);
        let (apx, pe, vm86, cpl) = configure;
        vcpu.set_apx_enabled(apx);
        vcpu.set_jit_mem(true);
        vcpu.sregs.cr0 = (vcpu.sregs.cr0 & !1) | u64::from(pe);
        vcpu.sregs.cs.selector = cpl;
        vcpu.regs.rax = 0xA5A5_5A5A_0000_0010;
        vcpu.regs.r31 = 0xA5A5_5A5A_0000_0010;
        if vm86 {
            vcpu.regs.rflags |= flags::bits::VM;
        }
        vcpu.sregs.ldt.base = 0xDEAD_BEEF;
        vcpu.sregs.ldt.limit = 0x1234;
        vcpu.sregs.ldt.selector = 0x2468;
        vcpu.sregs.ldt.present = true;
        vcpu.sregs.ldt.unusable = false;
        let before_regs = vcpu.regs.clone();
        let before_ldtr = segment_fingerprint(&vcpu.sregs.ldt);

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically guarded LLDT")
            .unwrap_or_else(|| panic!("{name}: dynamic LLDT faults must not block admission"));
        vcpu.jit_run_region_native(&region);

        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(vcpu.regs.rflags, before_regs.rflags, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.ldt), before_ldtr, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name} fault priority changed: {error}"
        );
    }
}

#[test]
fn jit_lldt_cpl_guard_precedes_source_memory_and_memory_fault_restarts_exactly() {
    for (name, cpl, expected_vector) in [("CPL", 3_u16, Some(13)), ("memory", 0, None)] {
        let memory = memory_with_code(&[0x0F, 0x00, 0x10, 0xF4]); // LLDT [RAX]; HLT
        let mut vcpu = test_vcpu(memory);
        vcpu.sregs.cs.selector = cpl;
        vcpu.regs.rax = 0x20_000;
        vcpu.set_jit_mem(true);
        let before_regs = vcpu.regs.clone();
        let before_ldtr = segment_fingerprint(&vcpu.sregs.ldt);

        let region = vcpu
            .jit_compile_region()
            .expect("compile faulting LLDT memory form")
            .unwrap_or_else(|| {
                panic!("{name}: dynamic LLDT source fault must not block admission")
            });
        vcpu.jit_run_region_native(&region);
        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(vcpu.regs.rflags, before_regs.rflags, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.ldt), before_ldtr, "{name}");
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
fn jit_lldt_verified_run_compares_and_adopts_complete_hidden_state() {
    let memory = memory_with_code(&[0x0F, 0x00, 0xD0, 0xF4]);
    let descriptor = ldt_descriptor(0xFFFF_8000_CAFE_0000, 0x4_5678, 2, true, true, true);
    install_lldt_descriptor(&memory, &descriptor);
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 0x10;
    vcpu.set_jit_mem(true);

    let region = vcpu
        .jit_compile_region()
        .expect("compile verified LLDT")
        .expect("strict LLDT must be native eligible");
    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 3);
    assert_eq!(vcpu.sregs.ldt.selector, 0x10);
    assert_eq!(vcpu.sregs.ldt.base, 0xFFFF_8000_CAFE_0000);
    assert_eq!(vcpu.sregs.ldt.limit, 0x4567_8FFF);
    assert_eq!(vcpu.sregs.ldt.dpl, 2);
    assert!(vcpu.sregs.ldt.g);
    assert!(vcpu.sregs.ldt.avl);
    assert!(!vcpu.sregs.ldt.unusable);
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
                0xB8, 0x10, 0x00, 0x00, 0x00, // MOV EAX,10h
                0x0F, 0x00, 0xD0, // LLDT AX (direct callout frontier)
                0xC3, // RET
            ],
            GuestAddress(0x100),
        )
        .unwrap();
    install_lldt_descriptor(
        &memory,
        &ldt_descriptor(0x1234_5000, 0xFFFF, 0, true, false, false),
    );
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

    assert_eq!(vcpu.sregs.ldt.selector, 0x10);
    assert_eq!(vcpu.sregs.tr.selector, 0xBEEF);
    assert_eq!(vcpu.regs.rax, 0x10);
    assert_eq!(vcpu.regs.rbx, 0x10);
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

#[test]
fn jit_rejects_lldt_outside_cs_l_and_direct_uses_legacy_descriptor_width() {
    let memory = memory_with_code(&[0x0F, 0x00, 0xD0, 0xF4]);
    let descriptor = ldt_descriptor(0x1234_5678, 0xF_FFFF, 0, true, true, false);
    install_lldt_descriptor(&memory, &descriptor);
    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.regs.rax = 0x10;
    compatibility.set_jit_mem(true);

    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode LLDT must retain its 8-byte descriptor semantics"
    );
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.sregs.ldt.selector, 0x10);
    assert_eq!(compatibility.sregs.ldt.base, 0x1234_5678);
    assert_eq!(compatibility.sregs.ldt.limit, u32::MAX);
}
