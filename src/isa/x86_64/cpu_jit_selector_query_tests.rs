//! Direct/native LAR/LSL differentials, helper semantics, and replay safety.

use super::jit_selector_tests::{
    data_descriptor, gprs, install_data_descriptor, memory_with_code, test_vcpu,
};
use super::*;
use crate::isa::x86_64::execute::system::X86SelectorQueryAccess;
use crate::smir::lower::{
    X86_SELECTOR_QUERY_HELPER_APX, X86_SELECTOR_QUERY_HELPER_DST_SHIFT,
    X86_SELECTOR_QUERY_HELPER_LIMIT, X86_SELECTOR_QUERY_HELPER_MEMORY,
    X86_SELECTOR_QUERY_HELPER_TAG, X86_SELECTOR_QUERY_HELPER_WIDTH_SHIFT,
};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn query_encoding(kind: X86SelectorQueryAccess, dst: u8, width: u32) -> u32 {
    X86_SELECTOR_QUERY_HELPER_TAG
        | (u32::from(dst) << X86_SELECTOR_QUERY_HELPER_DST_SHIFT)
        | (width << X86_SELECTOR_QUERY_HELPER_WIDTH_SHIFT)
        | if kind == X86SelectorQueryAccess::Limit {
            X86_SELECTOR_QUERY_HELPER_LIMIT
        } else {
            0
        }
}

fn helper(
    vcpu: &mut X86_64Vcpu,
    operand: u64,
    encoding: u32,
    configure: impl FnOnce(&mut crate::smir::lower::runtime::GuestRegs),
) -> (u64, crate::smir::lower::runtime::GuestRegs) {
    let mut state = crate::smir::lower::runtime::GuestRegs::default();
    for (index, value) in state.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    state.ctx = (vcpu as *mut X86_64Vcpu) as u64;
    configure(&mut state);
    let result = unsafe { rax_jit_system_selector_load(&mut state, operand, encoding) };
    (result, state)
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..64 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct LAR/LSL instruction").is_none(),
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

fn system_descriptor(type_: u8, dpl: u8, present: bool, raw_limit: u32, high: u64) -> [u8; 16] {
    assert!(raw_limit <= 0xF_FFFF);
    let low = u64::from(raw_limit & 0xFFFF)
        | (u64::from(type_ & 0xF) << 40)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from((raw_limit >> 16) & 0xF) << 48)
        | (1 << 52);
    let mut descriptor = [0_u8; 16];
    descriptor[..8].copy_from_slice(&low.to_le_bytes());
    descriptor[8..].copy_from_slice(&high.to_le_bytes());
    descriptor
}

fn install_system_descriptor(memory: &GuestMemoryMmap, descriptor: &[u8; 16]) {
    memory
        .write_slice(descriptor, GuestAddress(0x1010))
        .unwrap();
}

fn descriptor_low(descriptor: &[u8]) -> u64 {
    u64::from_le_bytes(descriptor[..8].try_into().unwrap())
}

fn access_rights(raw: u64) -> u64 {
    ((raw >> 40) & 0xFFFF) << 8
}

fn expanded_limit(raw: u64) -> u64 {
    let mut limit = (raw & 0xFFFF) | (((raw >> 48) & 0xF) << 16);
    if raw & (1 << 55) != 0 {
        limit = (limit << 12) | 0xFFF;
    }
    limit
}

#[test]
fn selector_query_helper_matches_values_widths_presence_privilege_and_aliasing() {
    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory.clone());
    let sentinel = 0xA500_0000_0000_0001;

    for present in [false, true] {
        let descriptor = data_descriptor(0, 0xABCDE, 0, present, 0x2, false);
        let raw = descriptor_low(&descriptor);
        install_data_descriptor(&memory, &descriptor);
        for (kind, width, expected) in [
            (X86SelectorQueryAccess::AccessRights, 0, access_rights(raw)),
            (X86SelectorQueryAccess::AccessRights, 1, access_rights(raw)),
            (X86SelectorQueryAccess::AccessRights, 2, access_rights(raw)),
            (X86SelectorQueryAccess::Limit, 1, expanded_limit(raw)),
        ] {
            let (status, state) = helper(&mut vcpu, 0x10, query_encoding(kind, 1, width), |_| {});
            assert_eq!(status, 2, "{kind:?} width={width} P={present}");
            let expected = match width {
                0 => (sentinel & !0xFFFF) | (expected & 0xFFFF),
                1 | 2 => expected & u64::from(u32::MAX),
                _ => unreachable!(),
            };
            assert_eq!(state.gpr[1], expected, "{kind:?} width={width} P={present}");
            for (index, value) in state.gpr.iter().enumerate() {
                if index != 1 {
                    assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64);
                }
            }
        }
    }

    let descriptor = data_descriptor(0, 0x12345, 2, true, 0x2, false);
    install_data_descriptor(&memory, &descriptor);
    vcpu.sregs.cs.selector = 0x2;
    assert_eq!(
        helper(
            &mut vcpu,
            0x12,
            query_encoding(X86SelectorQueryAccess::Limit, 0, 1),
            |_| {}
        )
        .0,
        2
    );
    assert_eq!(
        helper(
            &mut vcpu,
            0x13,
            query_encoding(X86SelectorQueryAccess::Limit, 0, 1),
            |_| {}
        )
        .0,
        1,
        "RPL exceeds DPL"
    );
    vcpu.sregs.cs.selector = 0x3;
    assert_eq!(
        helper(
            &mut vcpu,
            0x12,
            query_encoding(X86SelectorQueryAccess::Limit, 0, 1),
            |_| {}
        )
        .0,
        1,
        "CPL exceeds DPL"
    );

    let conforming = data_descriptor(0, 0x12345, 0, true, 0xE, false);
    install_data_descriptor(&memory, &conforming);
    let (status, state) = helper(
        &mut vcpu,
        0x13,
        query_encoding(X86SelectorQueryAccess::AccessRights, 0, 1),
        |state| state.gpr[0] = 0x13,
    );
    assert_eq!(status, 2, "conforming code bypasses CPL/RPL checks");
    assert_eq!(state.gpr[0], access_rights(descriptor_low(&conforming)));
}

#[test]
fn selector_query_helper_enforces_ia32e_system_type_and_high_descriptor_rules() {
    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory.clone());
    for (type_, lar, lsl) in [
        (0x0, false, false),
        (0x2, true, true),
        (0x4, false, false),
        (0x9, true, true),
        (0xB, true, true),
        (0xC, true, false),
        (0xE, false, false),
    ] {
        let descriptor = system_descriptor(type_, 0, false, 0x34567, 0x1234_5678);
        install_system_descriptor(&memory, &descriptor);
        for (kind, valid) in [
            (X86SelectorQueryAccess::AccessRights, lar),
            (X86SelectorQueryAccess::Limit, lsl),
        ] {
            let (status, state) = helper(&mut vcpu, 0x10, query_encoding(kind, 2, 1), |_| {});
            assert_eq!(status, 1 + u64::from(valid), "type={type_:#x} {kind:?}");
            let expected = if valid {
                match kind {
                    X86SelectorQueryAccess::AccessRights => {
                        access_rights(descriptor_low(&descriptor))
                    }
                    X86SelectorQueryAccess::Limit => expanded_limit(descriptor_low(&descriptor)),
                }
            } else {
                0xA500_0000_0000_0002
            };
            assert_eq!(state.gpr[2], expected, "type={type_:#x} {kind:?}");
        }
    }

    let mut reserved = system_descriptor(0x9, 0, true, 0x34567, 0);
    reserved[13] = 1;
    install_system_descriptor(&memory, &reserved);
    assert_eq!(
        helper(
            &mut vcpu,
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 2, 1),
            |_| {}
        )
        .0,
        1,
        "bits 12:8 of doubleword +12 must be zero"
    );

    install_system_descriptor(&memory, &system_descriptor(0x9, 0, true, 0x34567, 0));
    vcpu.sregs.gdt.limit = 0x17;
    assert_eq!(
        helper(
            &mut vcpu,
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 2, 1),
            |_| {}
        )
        .0,
        1,
        "a truncated 16-byte descriptor is a completed selector failure"
    );
}

#[test]
fn selector_query_helper_completes_semantic_failures_without_destination_commit() {
    let memory = memory_with_code(&[]);
    let invalid = system_descriptor(0xE, 0, true, 0, 0);
    install_system_descriptor(&memory, &invalid);
    let mut vcpu = test_vcpu(memory);
    for (name, operand, configure) in [
        ("null", 0_u64, 0_u8),
        ("GDT bounds", 0x10, 1),
        ("unusable LDT", 0x14, 2),
        ("invalid type", 0x10, 3),
        ("descriptor address overflow", 0x10, 4),
    ] {
        vcpu.sregs.gdt.base = if configure == 4 { u64::MAX - 7 } else { 0x1000 };
        vcpu.sregs.gdt.limit = if configure == 1 { 0x0F } else { 0x1F };
        vcpu.sregs.ldt.selector = if configure == 2 { 0 } else { 0x1357 };
        vcpu.sregs.ldt.unusable = configure == 2;
        let (status, state) = helper(
            &mut vcpu,
            operand,
            query_encoding(X86SelectorQueryAccess::AccessRights, 7, 2),
            |_| {},
        );
        assert_eq!(status, 1, "{name}");
        assert_eq!(state.gpr[7], 0xA500_0000_0000_0007, "{name}");
    }
}

#[test]
fn selector_query_helper_rolls_back_every_replayed_probe_and_rejects_bad_encodings() {
    const TRACE_SENTINEL: (u8, u64, u8, u64) = (0, 0xAA, 1, 0x55);
    const LOG_SENTINEL: (u64, u8, u64) = (0xBB, 1, 0x66);
    let memory = memory_with_code(&[]);
    memory
        .write_slice(&0x10_u16.to_le_bytes(), GuestAddress(0x3000))
        .unwrap();
    memory
        .write_slice(
            &system_descriptor(0x9, 0, true, 0x34567, 0)[..8],
            GuestAddress(0xFFF8),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    crate::vm::vcpu::VCpu::set_mem_recording(&mut vcpu, true);

    for (name, operand, encoding, gdt_base, apx, cr0, vm, cs_l) in [
        (
            "unmapped descriptor after mapped source",
            0x3000,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 1) | X86_SELECTOR_QUERY_HELPER_MEMORY,
            0x20_000,
            true,
            1,
            false,
            true,
        ),
        (
            "unmapped source",
            0x20_000,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 1) | X86_SELECTOR_QUERY_HELPER_MEMORY,
            0x1000,
            true,
            1,
            false,
            true,
        ),
        (
            "unmapped high descriptor",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 1),
            0xFFE8,
            true,
            1,
            false,
            true,
        ),
        (
            "APX disabled",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 31, 1) | X86_SELECTOR_QUERY_HELPER_APX,
            0x1000,
            false,
            1,
            false,
            true,
        ),
        (
            "real mode",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 1),
            0x1000,
            true,
            0,
            false,
            true,
        ),
        (
            "VM86",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 1),
            0x1000,
            true,
            1,
            true,
            true,
        ),
        (
            "compatibility mode",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 1),
            0x1000,
            true,
            1,
            false,
            false,
        ),
        (
            "unknown option",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 1) | (1 << 15),
            0x1000,
            true,
            1,
            false,
            true,
        ),
        (
            "reserved width",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 1, 3),
            0x1000,
            true,
            1,
            false,
            true,
        ),
        (
            "EGPR without APX",
            0x10,
            query_encoding(X86SelectorQueryAccess::Limit, 31, 1),
            0x1000,
            true,
            1,
            false,
            true,
        ),
    ] {
        vcpu.sregs.gdt.base = gdt_base;
        vcpu.sregs.gdt.limit = 0x1F;
        vcpu.sregs.cs.l = cs_l;
        vcpu.set_apx_enabled(apx);
        vcpu.sregs.cr0 = cr0;
        vcpu.regs.rflags = 0x2 | u64::from(vm) * flags::bits::VM;
        vcpu.jit_mem_trace = Some(vec![TRACE_SENTINEL]);
        vcpu.jit_mem_log = Some(vec![LOG_SENTINEL]);
        let (status, state) = helper(&mut vcpu, operand, encoding, |_| {});
        assert_eq!(status, 0, "{name}");
        for (index, value) in state.gpr.iter().enumerate() {
            assert_eq!(*value, 0xA500_0000_0000_0000 | index as u64, "{name}");
        }
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
    }
}

fn differential_vcpus(
    code: &[u8],
    descriptor: [u8; 8],
) -> (
    X86_64Vcpu,
    X86_64Vcpu,
    Arc<GuestMemoryMmap>,
    Arc<GuestMemoryMmap>,
) {
    let direct_memory = memory_with_code(code);
    let native_memory = memory_with_code(code);
    install_data_descriptor(&direct_memory, &descriptor);
    install_data_descriptor(&native_memory, &descriptor);
    let direct = test_vcpu(direct_memory.clone());
    let native = test_vcpu(native_memory.clone());
    (direct, native, direct_memory, native_memory)
}

#[test]
fn jit_selector_query_widths_alias_values_and_zf_only_match_direct() {
    let descriptor = data_descriptor(0, 0xABCDE, 0, false, 0x2, false);
    for (name, instruction, dst_index) in [
        ("LAR W16", &[0x66, 0x0F, 0x02, 0xC8][..], 1_usize),
        ("LAR W32", &[0x0F, 0x02, 0xC8], 1),
        ("LAR W64", &[0x48, 0x0F, 0x02, 0xC8], 1),
        ("LSL alias", &[0x0F, 0x03, 0xC0], 0),
        ("REX LSL", &[0x45, 0x0F, 0x03, 0xF7], 14),
        ("APX LAR", &[0xD5, 0xD5, 0x02, 0xF7], 30),
        ("APX LSL W64", &[0xD5, 0xDD, 0x03, 0xF7], 30),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x48, 0x8D, 0x5B, 0x01, 0xF4]); // LEA RBX,[RBX+1]; HLT
        let hlt = (instruction.len() + 4) as u64;
        let (mut direct, mut native, _, _) = differential_vcpus(&code, descriptor);
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(name.starts_with("APX"));
            vcpu.set_jit_mem(true);
            vcpu.regs.rax = 0xCAFE_BABE_0000_0010;
            vcpu.regs.rcx = 0xA5A5_5A5A_DEAD_BEEF;
            vcpu.regs.r14 = 0xA5A5_5A5A_DEAD_BEEF;
            vcpu.regs.r15 = 0xCAFE_BABE_0000_0010;
            vcpu.regs.r30 = 0xA5A5_5A5A_DEAD_BEEF;
            vcpu.regs.r31 = 0xCAFE_BABE_0000_0010;
            vcpu.regs.rbx = 0x1234_5678_9ABC_DEF0;
            vcpu.regs.rflags =
                0x2 | flags::bits::CF | flags::bits::PF | flags::bits::SF | flags::bits::OF;
        }

        run_direct_to(&mut direct, hlt);
        let region = native
            .jit_compile_region()
            .expect("compile LAR/LSL region")
            .unwrap_or_else(|| panic!("{name}: strict LAR/LSL must be native eligible"));
        native.jit_run_region_native(&region);

        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}");
        assert_eq!(native.regs.rip, hlt, "{name}");
        assert_eq!(native.regs.rbx, 0x1234_5678_9ABC_DEF1, "{name}");
        assert_ne!(
            gprs(&native.regs)[dst_index],
            0xA5A5_5A5A_DEAD_BEEF,
            "{name}"
        );
    }
}

#[test]
fn jit_selector_query_apx_memory_source_and_address_match_direct() {
    let code = [
        0xD5, 0xF7, 0x03, 0x9C, 0xD1, 0x20, 0, 0, 0, // LSL R27D,[R25+R26*8+32]
        0x48, 0x8D, 0x5B, 0x01, // LEA RBX,[RBX+1]
        0xF4,
    ];
    let descriptor = data_descriptor(0, 0x34567, 0, true, 0x2, false);
    let (mut direct, mut native, direct_memory, native_memory) =
        differential_vcpus(&code, descriptor);
    let source_address = 0x3000 + 4 * 8 + 0x20;
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.set_jit_mem(true);
        vcpu.regs.r25 = 0x3000;
        vcpu.regs.r26 = 4;
        vcpu.regs.r27 = 0xA5A5_5A5A_DEAD_BEEF;
        vcpu.regs.rbx = 0x1234_5678_9ABC_DEF0;
        vcpu.regs.rflags =
            0x2 | flags::bits::CF | flags::bits::PF | flags::bits::SF | flags::bits::OF;
    }
    for memory in [&direct_memory, &native_memory] {
        memory
            .write_slice(&0x10_u16.to_le_bytes(), GuestAddress(source_address))
            .unwrap();
    }

    run_direct_to(&mut direct, 13);
    let region = native
        .jit_compile_region()
        .expect("compile APX LSL memory region")
        .expect("APX LSL memory form must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 13);
    assert_eq!(native.regs.r27, expanded_limit(descriptor_low(&descriptor)));
    assert_eq!(native.regs.rbx, 0x1234_5678_9ABC_DEF1);
}

#[test]
fn jit_selector_query_dynamic_mode_apx_and_memory_faults_replay_without_commit() {
    for (name, code, configure, expected_vector) in [
        (
            "APX before source",
            &[0xD5, 0xD5, 0x02, 0x08, 0xF4][..],
            0_u8,
            Some(6),
        ),
        (
            "real mode before source",
            &[0x0F, 0x02, 0x08, 0xF4],
            1,
            Some(6),
        ),
        ("VM86 before source", &[0x0F, 0x03, 0x08, 0xF4], 2, Some(6)),
        ("source memory", &[0x0F, 0x02, 0x08, 0xF4], 3, None),
        ("descriptor memory", &[0x0F, 0x03, 0xC8, 0xF4], 4, None),
    ] {
        let memory = memory_with_code(code);
        let mut vcpu = test_vcpu(memory);
        vcpu.set_apx_enabled(configure != 0);
        vcpu.set_jit_mem(true);
        vcpu.regs.rax = if configure == 4 { 0x10 } else { 0x20_000 };
        vcpu.regs.rcx = 0xA5A5_5A5A_DEAD_BEEF;
        vcpu.regs.rflags =
            0x2 | flags::bits::CF | flags::bits::PF | flags::bits::SF | flags::bits::OF;
        if configure == 1 {
            vcpu.sregs.cr0 = 0;
        }
        if configure == 2 {
            vcpu.regs.rflags |= flags::bits::VM;
        }
        if configure == 4 {
            vcpu.sregs.gdt.base = 0x20_000;
        }
        let before = vcpu.regs.clone();

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically guarded LAR/LSL")
            .expect("dynamic LAR/LSL faults must not block admission");
        vcpu.jit_run_region_native(&region);

        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{name}");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        let error = exception_without_idt(&mut vcpu);
        if let Some(vector) = expected_vector {
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "{name} priority changed: {error}"
            );
        } else {
            assert!(
                !error.contains("IDT entry 6 not present"),
                "{name} must reach the memory fault: {error}"
            );
        }
    }
}

#[test]
fn direct_selector_query_lock_and_mode_checks_precede_memory() {
    for (name, code, configure) in [
        ("LOCK", &[0xF0, 0x0F, 0x02, 0x08][..], 0_u8),
        ("real mode", &[0x0F, 0x02, 0x08], 1),
        ("VM86", &[0x0F, 0x03, 0x08], 2),
        ("APX", &[0xD5, 0xD5, 0x02, 0x08], 3),
    ] {
        let memory = memory_with_code(code);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.rax = 0x20_000;
        if configure == 1 {
            vcpu.sregs.cr0 = 0;
        }
        if configure == 2 {
            vcpu.regs.rflags |= flags::bits::VM;
        }
        if configure == 3 {
            vcpu.set_apx_enabled(false);
        }
        let before = vcpu.regs.clone();
        let error = exception_without_idt(&mut vcpu);
        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{name}");
    }
}

#[test]
fn direct_selector_query_compatibility_uses_lma_type_rules_and_stays_out_of_jit() {
    let code = [0x0F, 0x03, 0xC8, 0xF4]; // LSL ECX,AX
    let descriptor = system_descriptor(0x1, 0, false, 0x34567, 0);
    let memory = memory_with_code(&code);
    install_system_descriptor(&memory, &descriptor);

    let mut legacy = test_vcpu(memory.clone());
    legacy.sregs.efer = 0;
    legacy.sregs.cs.l = false;
    legacy.sregs.cs.db = true;
    legacy.regs.rax = 0x10;
    legacy.regs.rcx = 0xA5A5_5A5A_DEAD_BEEF;
    assert!(legacy.step().unwrap().is_none());
    assert_eq!(legacy.regs.rcx, expanded_limit(descriptor_low(&descriptor)));
    assert_ne!(legacy.regs.rflags & flags::bits::ZF, 0);

    let sentinel = 0xA5A5_5A5A_DEAD_BEEF;
    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.regs.rax = 0x10;
    compatibility.regs.rcx = sentinel;
    compatibility.regs.rflags |= flags::bits::ZF;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode LSL must retain direct execution"
    );
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.regs.rcx, sentinel);
    assert_eq!(compatibility.regs.rflags & flags::bits::ZF, 0);
}

#[test]
fn direct_selector_query_materializes_lazy_status_before_changing_only_zf() {
    let code = [
        0x83, 0xC2, 0x01, // ADD EDX,1: 0x7fffffff + 1 = 0x80000000
        0x0F, 0x02, 0xC8, // LAR ECX,AX
        0xF4,
    ];
    let memory = memory_with_code(&code);
    install_data_descriptor(&memory, &data_descriptor(0, 0xFFFF, 0, true, 0x2, false));

    for (name, selector, expected_zf) in [("invalid", 0_u64, false), ("valid", 0x10, true)] {
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.regs.rax = selector;
        vcpu.regs.rdx = 0x7FFF_FFFF;
        vcpu.regs.rflags = 0x2 | flags::bits::CF | flags::bits::ZF;
        assert!(vcpu.step().unwrap().is_none(), "{name} ADD");
        assert!(vcpu.step().unwrap().is_none(), "{name} LAR");

        let status = vcpu.regs.rflags
            & (flags::bits::CF
                | flags::bits::PF
                | flags::bits::AF
                | flags::bits::ZF
                | flags::bits::SF
                | flags::bits::OF);
        let expected = flags::bits::PF
            | flags::bits::AF
            | flags::bits::SF
            | flags::bits::OF
            | if expected_zf { flags::bits::ZF } else { 0 };
        assert_eq!(status, expected, "{name}");
    }
}
