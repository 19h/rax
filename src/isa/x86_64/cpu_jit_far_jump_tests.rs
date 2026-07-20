//! Direct/native x86-64 JIT differentials for indirect far-JMP descriptor
//! state, call gates, memory effects, and precise replay.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const POINTER: u64 = 0x2000;
const GDT: u64 = 0x1000;

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
        type_: 0xB,
        present: true,
        s: true,
        l: true,
        ..crate::vm::vcpu::Segment::default()
    };
    vcpu.sregs.gdt.base = GDT;
    vcpu.sregs.gdt.limit = 0x7F;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn code_descriptor(dpl: u8, present: bool, l: bool, db: bool, accessed: bool) -> [u8; 8] {
    let raw = 0xFFFF_u64
        | ((0xA_u64 | u64::from(accessed)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from(l) << 53)
        | (u64::from(db) << 54);
    raw.to_le_bytes()
}

fn call_gate(target_selector: u16, target_offset: u64, dpl: u8, present: bool) -> [u8; 16] {
    let low = (target_offset & 0xFFFF)
        | (u64::from(target_selector) << 16)
        | (0xC << 40)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (((target_offset >> 16) & 0xFFFF) << 48);
    let high = (target_offset >> 32) & 0xFFFF_FFFF;
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&low.to_le_bytes());
    bytes[8..].copy_from_slice(&high.to_le_bytes());
    bytes
}

fn far_pointer(offset: u64, selector: u16, offset_bytes: usize) -> Vec<u8> {
    let mut bytes = offset.to_le_bytes()[..offset_bytes].to_vec();
    bytes.extend_from_slice(&selector.to_le_bytes());
    bytes
}

fn install_pointer(memory: &GuestMemoryMmap, offset: u64, selector: u16, offset_bytes: usize) {
    memory
        .write_slice(
            &far_pointer(offset, selector, offset_bytes),
            GuestAddress(POINTER),
        )
        .unwrap();
}

fn install_descriptor(memory: &GuestMemoryMmap, selector: u16, descriptor: &[u8]) {
    let address = GDT + u64::from(selector >> 3) * 8;
    memory
        .write_slice(descriptor, GuestAddress(address))
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

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

#[test]
fn jit_far_jump_widths_stack_and_apx_egpr_addresses_match_direct_at_dynamic_handoff() {
    for (name, instruction, offset_bytes, target, source) in [
        ("m16:32 RAX", &[0xFF, 0x28][..], 4, 0x1234_5678, 0_u8),
        ("m16:16 RSP", &[0x66, 0xFF, 0x2C, 0x24], 2, 0x5678, 4),
        (
            "m16:64 R16",
            &[0xD5, 0x18, 0xFF, 0x28],
            8,
            0xFFFF_8000_1234_5678,
            16,
        ),
    ] {
        let direct_memory = memory_with_code(instruction);
        let native_memory = memory_with_code(instruction);
        let descriptor = code_descriptor(0, true, true, false, false);
        for memory in [&direct_memory, &native_memory] {
            install_pointer(memory, target, 0x10, offset_bytes);
            install_descriptor(memory, 0x10, &descriptor);
        }
        let mut direct = test_vcpu(direct_memory.clone());
        let mut native = test_vcpu(native_memory.clone());
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(source == 16);
            // Docker Desktop's arm64-hosted amd64 translation drops AF across
            // the native trampoline; exact real-host preservation is covered
            // by the lowerer-level executable test.
            vcpu.regs.rflags &= !flags::bits::AF;
            match source {
                0 => vcpu.regs.rax = POINTER,
                4 => vcpu.regs.rsp = POINTER,
                16 => vcpu.regs.r16 = POINTER,
                _ => unreachable!(),
            }
        }

        assert!(direct.step().expect("direct far JMP").is_none(), "{name}");
        let region = native
            .jit_compile_region()
            .expect("compile far-JMP region")
            .unwrap_or_else(|| panic!("{name}: exact far JMP must be native eligible"));
        native.jit_run_region_verified(&region);

        assert_eq!(native.regs.rip, target, "{name}");
        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}");
        assert_eq!(
            segment_fingerprint(&native.sregs.cs),
            segment_fingerprint(&direct.sregs.cs),
            "{name}"
        );
        assert_eq!(native.sregs.cs.selector, 0x10, "{name}");
        assert_eq!(native.sregs.cs.type_ & 1, 1, "{name}");
        let mut direct_descriptor = [0_u8; 8];
        let mut native_descriptor = [0_u8; 8];
        direct_memory
            .read_slice(&mut direct_descriptor, GuestAddress(0x1010))
            .unwrap();
        native_memory
            .read_slice(&mut native_descriptor, GuestAddress(0x1010))
            .unwrap();
        assert_eq!(native_descriptor, direct_descriptor, "{name}");
        assert_eq!(native_descriptor[5] & 1, 1, "{name}");
    }
}

#[test]
fn jit_far_jump_call_gate_matches_direct_and_marks_only_target_code_accessed() {
    let instruction = [0x48, 0xFF, 0x28];
    let target = 0xFFFF_8000_2468_ACE0;
    let gate_selector = 0x18;
    let target_selector = 0x30;
    let gate = call_gate(target_selector, target, 0, true);
    let descriptor = code_descriptor(0, true, true, false, false);
    let direct_memory = memory_with_code(&instruction);
    let native_memory = memory_with_code(&instruction);
    for memory in [&direct_memory, &native_memory] {
        install_pointer(memory, u64::MAX, gate_selector, 8);
        install_descriptor(memory, gate_selector, &gate);
        install_descriptor(memory, target_selector, &descriptor);
    }
    let mut direct = test_vcpu(direct_memory.clone());
    let mut native = test_vcpu(native_memory.clone());
    direct.regs.rax = POINTER;
    native.regs.rax = POINTER;
    direct.regs.rflags &= !flags::bits::AF;
    native.regs.rflags &= !flags::bits::AF;

    assert!(direct.step().expect("direct call-gate JMP").is_none());
    let region = native
        .jit_compile_region()
        .expect("compile call-gate JMP")
        .expect("call-gate JMP must be native eligible");
    native.jit_run_region_verified(&region);

    assert_eq!(native.regs.rip, target);
    assert_eq!(
        segment_fingerprint(&native.sregs.cs),
        segment_fingerprint(&direct.sregs.cs)
    );
    assert_eq!(native.sregs.cs.selector, target_selector);
    let mut gate_after = [0_u8; 16];
    native_memory
        .read_slice(&mut gate_after, GuestAddress(0x1018))
        .unwrap();
    assert_eq!(gate_after, gate);
    let mut target_after = [0_u8; 8];
    native_memory
        .read_slice(&mut target_after, GuestAddress(0x1030))
        .unwrap();
    assert_eq!(target_after[5] & 1, 1);
}

#[test]
fn jit_far_jump_can_handoff_to_compatibility_code_and_use_the_ldt() {
    const LDT: u64 = 0x1800;
    const SELECTOR: u16 = 0x0C;
    let instruction = [0x48, 0xFF, 0x28];
    let target = 0x3456;
    let descriptor = code_descriptor(0, true, false, true, false);
    let direct_memory = memory_with_code(&instruction);
    let native_memory = memory_with_code(&instruction);
    for memory in [&direct_memory, &native_memory] {
        install_pointer(memory, target, SELECTOR, 8);
        memory
            .write_slice(&descriptor, GuestAddress(LDT + 8))
            .unwrap();
    }
    let mut direct = test_vcpu(direct_memory.clone());
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = POINTER;
        vcpu.regs.rflags &= !flags::bits::AF;
        vcpu.sregs.ldt = crate::vm::vcpu::Segment {
            base: LDT,
            limit: 0x1F,
            selector: 0x20,
            type_: 2,
            present: true,
            s: false,
            unusable: false,
            ..crate::vm::vcpu::Segment::default()
        };
    }

    assert!(direct.step().expect("direct LDT far JMP").is_none());
    let region = native
        .jit_compile_region()
        .expect("compile LDT far JMP")
        .expect("LDT far JMP must be native eligible");
    native.jit_run_region_verified(&region);

    assert_eq!(native.regs.rip, target);
    assert_eq!(
        segment_fingerprint(&native.sregs.cs),
        segment_fingerprint(&direct.sregs.cs)
    );
    assert_eq!(native.sregs.cs.selector, SELECTOR);
    assert!(!native.sregs.cs.l);
    assert!(native.sregs.cs.db);
    let mut descriptor_after = [0_u8; 8];
    native_memory
        .read_slice(&mut descriptor_after, GuestAddress(LDT + 8))
        .unwrap();
    assert_eq!(descriptor_after[5] & 1, 1);
}

#[test]
fn jit_far_jump_noncanonical_pointer_selects_ss_or_gp_and_never_commits() {
    for (name, instruction, stack_segment, expected_vector) in [
        ("SS default", &[0x48, 0xFF, 0x2C, 0x24][..], true, 12),
        ("DS default", &[0x48, 0xFF, 0x28], false, 13),
    ] {
        let memory = memory_with_code(instruction);
        let mut vcpu = test_vcpu(memory);
        let noncanonical = 0x0000_8000_0000_0000;
        if stack_segment {
            vcpu.regs.rsp = noncanonical;
        } else {
            vcpu.regs.rax = noncanonical;
        }
        let before_cs = segment_fingerprint(&vcpu.sregs.cs);
        let before_gprs = gprs(&vcpu.regs);
        let region = vcpu
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: compile failed: {error}"))
            .unwrap_or_else(|| panic!("{name}: far JMP must remain native eligible"));
        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        assert_eq!(gprs(&vcpu.regs), before_gprs, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.cs), before_cs, "{name}");

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name}: wrong exception vector: {error}"
        );
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.cs), before_cs, "{name}");
    }
}

#[test]
fn far_jump_helper_traces_logs_commits_and_rolls_back_every_deoptimization() {
    use crate::smir::lower::runtime::GuestRegs;

    const TRACE_SENTINEL: (u8, u64, u8, u64) = (0, 0xAA, 1, 0x55);
    const LOG_SENTINEL: (u64, u8, u64) = (0xBB, 1, 0x66);
    let memory = memory_with_code(&[]);
    let descriptor = code_descriptor(0, true, true, false, false);
    install_pointer(&memory, 0x1234_5678, 0x10, 8);
    install_descriptor(&memory, 0x10, &descriptor);
    let mut vcpu = test_vcpu(memory.clone());
    crate::vm::vcpu::VCpu::set_mem_recording(&mut vcpu, true);
    vcpu.jit_mem_trace = Some(Vec::new());
    vcpu.jit_mem_log = Some(vec![LOG_SENTINEL]);
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;

    assert_eq!(unsafe { rax_jit_far_jump(&mut state, POINTER, 2) }, 1);
    assert_eq!(state.exit_pc, 0x1234_5678);
    assert_eq!(state.cs_l, 1);
    assert_eq!(state.cpl, 0);
    assert_eq!(vcpu.regs.rip, 0x1234_5678);
    assert_eq!(vcpu.sregs.cs.selector, 0x10);
    let trace = vcpu.jit_mem_trace.as_ref().unwrap();
    assert_eq!(trace.len(), 4);
    assert_eq!((trace[0].0, trace[0].1, trace[0].2), (0, POINTER, 8));
    assert_eq!((trace[1].0, trace[1].1, trace[1].2), (0, POINTER + 8, 2));
    assert_eq!((trace[2].0, trace[2].1, trace[2].2), (0, 0x1010, 8));
    assert_eq!((trace[3].0, trace[3].1, trace[3].2), (1, 0x1010, 8));
    let old_low = u64::from_le_bytes(descriptor);
    assert_eq!(
        vcpu.jit_mem_log.as_deref(),
        Some(&[LOG_SENTINEL, (0x1010, 8, old_low)][..])
    );
    let mut records = Vec::new();
    crate::vm::vcpu::VCpu::drain_mem_records(&mut vcpu, &mut records);
    assert_eq!(
        records
            .iter()
            .map(|record| (record.access, record.addr, record.size))
            .collect::<Vec<_>>(),
        vec![
            (crate::vm::vcpu::MemAccess::Read, POINTER, 8),
            (crate::vm::vcpu::MemAccess::Read, POINTER + 8, 2),
            (crate::vm::vcpu::MemAccess::Read, 0x1010, 8),
            (crate::vm::vcpu::MemAccess::Write, 0x1010, 8),
        ]
    );

    let committed_cs = segment_fingerprint(&vcpu.sregs.cs);
    let committed_rip = vcpu.regs.rip;
    for (name, encoding, mark_code) in [
        ("unknown encoding", 3_u32, false),
        ("descriptor code page", 2, true),
    ] {
        if mark_code {
            install_descriptor(&memory, 0x10, &descriptor);
            vcpu.mmu.mark_code_page(0x1010);
        }
        vcpu.jit_mem_trace = Some(vec![TRACE_SENTINEL]);
        vcpu.jit_mem_log = Some(vec![LOG_SENTINEL]);
        assert_eq!(
            unsafe { rax_jit_far_jump(&mut state, POINTER, encoding) },
            0,
            "{name}"
        );
        assert_eq!(
            vcpu.jit_mem_trace.as_deref(),
            Some(&[TRACE_SENTINEL][..]),
            "{name}"
        );
        assert_eq!(
            vcpu.jit_mem_log.as_deref(),
            Some(&[LOG_SENTINEL][..]),
            "{name}"
        );
        let mut records = Vec::new();
        crate::vm::vcpu::VCpu::drain_mem_records(&mut vcpu, &mut records);
        assert!(records.is_empty(), "{name}: {records:?}");
        assert_eq!(vcpu.regs.rip, committed_rip, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.cs), committed_cs, "{name}");
    }
}

#[test]
fn far_jump_helper_cpl3_commits_normally_but_deopts_unrestorable_verifier_write() {
    use crate::smir::lower::runtime::GuestRegs;

    const SELECTOR: u16 = 0x13;
    let memory = memory_with_code(&[]);
    let descriptor = code_descriptor(3, true, true, false, false);
    install_pointer(&memory, 0x1234_5678, SELECTOR, 8);
    install_descriptor(&memory, SELECTOR, &descriptor);
    let mut vcpu = test_vcpu(memory.clone());
    vcpu.sregs.cs.selector = 3;
    vcpu.sregs.cs.dpl = 3;
    let original_cs = vcpu.sregs.cs.clone();
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;

    assert_eq!(unsafe { rax_jit_far_jump(&mut state, POINTER, 2) }, 1);
    assert_eq!(vcpu.regs.rip, 0x1234_5678);
    assert_eq!(vcpu.sregs.cs.selector, SELECTOR);
    let mut committed = [0_u8; 8];
    memory
        .read_slice(&mut committed, GuestAddress(0x1010))
        .unwrap();
    assert_eq!(committed[5] & 1, 1);

    install_descriptor(&memory, SELECTOR, &descriptor);
    vcpu.sregs.cs = original_cs.clone();
    vcpu.regs.rip = 0xCAFE;
    vcpu.jit_mem_trace = Some(Vec::new());
    vcpu.jit_mem_log = Some(Vec::new());
    assert_eq!(unsafe { rax_jit_far_jump(&mut state, POINTER, 2) }, 0);
    assert_eq!(vcpu.regs.rip, 0xCAFE);
    assert_eq!(
        segment_fingerprint(&vcpu.sregs.cs),
        segment_fingerprint(&original_cs)
    );
    assert!(vcpu.jit_mem_trace.as_ref().unwrap().is_empty());
    assert!(vcpu.jit_mem_log.as_ref().unwrap().is_empty());
    let mut unchanged = [0_u8; 8];
    memory
        .read_slice(&mut unchanged, GuestAddress(0x1010))
        .unwrap();
    assert_eq!(unchanged, descriptor);
}

#[test]
fn far_jump_helper_rejects_inconsistent_mode_state_before_every_memory_access() {
    use crate::smir::lower::runtime::GuestRegs;

    const TRACE_SENTINEL: (u8, u64, u8, u64) = (0, 0xAA, 1, 0x55);
    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory);
    crate::vm::vcpu::VCpu::set_mem_recording(&mut vcpu, true);
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;

    for (name, cr0, rflags) in [
        (
            "protected mode disabled",
            vcpu.sregs.cr0 & !1,
            vcpu.regs.rflags,
        ),
        (
            "VM86 active",
            vcpu.sregs.cr0,
            vcpu.regs.rflags | flags::bits::VM,
        ),
    ] {
        vcpu.sregs.cr0 = cr0;
        vcpu.regs.rflags = rflags;
        vcpu.jit_mem_trace = Some(vec![TRACE_SENTINEL]);
        assert_eq!(
            unsafe { rax_jit_far_jump(&mut state, POINTER, 2) },
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
    }
}

#[test]
fn jit_far_jump_fault_deoptimizes_before_direct_exception_and_preserves_cs_rip() {
    let memory = memory_with_code(&[0x48, 0xFF, 0x28]);
    install_pointer(&memory, 0x1234, 0x10, 8);
    install_descriptor(
        &memory,
        0x10,
        &code_descriptor(0, false, true, false, false),
    );
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = POINTER;
    let before_cs = segment_fingerprint(&vcpu.sregs.cs);
    let before_gprs = gprs(&vcpu.regs);
    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting far JMP")
        .expect("dynamic descriptor fault must not block admission");
    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(gprs(&vcpu.regs), before_gprs);
    assert_eq!(segment_fingerprint(&vcpu.sregs.cs), before_cs);
    let error = exception_without_idt(&mut vcpu);
    assert!(
        error.contains("IDT entry 11 not present"),
        "not-present code descriptor must deliver #NP: {error}"
    );
}

#[test]
fn jit_rejects_far_jump_outside_long_code_mode_while_direct_keeps_legacy_path() {
    let memory = memory_with_code(&[0xFF, 0x28, 0xF4]);
    install_pointer(&memory, 0x1234, 0x10, 4);
    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.regs.rax = POINTER;

    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode far JMP must retain direct segmentation semantics"
    );
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.regs.rip, 0x1234);
}
