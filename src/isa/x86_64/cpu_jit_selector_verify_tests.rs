//! Direct/native VERR/VERW differentials, helper semantics, and replay safety.

use super::jit_selector_tests::{
    data_descriptor, gprs, install_data_descriptor, memory_with_code, test_vcpu,
};
use super::*;
use crate::smir::lower::{
    X86_SELECTOR_VERIFY_HELPER_APX, X86_SELECTOR_VERIFY_HELPER_MEMORY,
    X86_SELECTOR_VERIFY_HELPER_TAG, X86_SELECTOR_VERIFY_HELPER_WRITE,
};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const VERIFY_READ: u32 = X86_SELECTOR_VERIFY_HELPER_TAG;
const VERIFY_WRITE: u32 = VERIFY_READ | X86_SELECTOR_VERIFY_HELPER_WRITE;
const MEMORY_SOURCE: u32 = X86_SELECTOR_VERIFY_HELPER_MEMORY;
const REQUIRES_APX: u32 = X86_SELECTOR_VERIFY_HELPER_APX;

fn helper(vcpu: &mut X86_64Vcpu, operand: u64, encoding: u32) -> u64 {
    let mut state = crate::smir::lower::runtime::GuestRegs::default();
    state.ctx = (vcpu as *mut X86_64Vcpu) as u64;
    unsafe { rax_jit_system_selector_load(&mut state, operand, encoding) }
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..64 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct VERR/VERW instruction").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

fn verify_descriptor(type_: u8, dpl: u8, present: bool) -> [u8; 8] {
    data_descriptor(0, 0xFFFF, dpl, present, type_, false)
}

#[test]
fn selector_verify_helper_matches_type_privilege_and_presence_semantics() {
    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory.clone());

    for present in [false, true] {
        for (name, descriptor, read, write) in [
            ("read-only data", verify_descriptor(0x0, 0, present), 2, 1),
            ("writable data", verify_descriptor(0x2, 0, present), 2, 2),
            (
                "execute-only code",
                verify_descriptor(0x8, 0, present),
                1,
                1,
            ),
            ("readable code", verify_descriptor(0xA, 0, present), 2, 1),
            (
                "conforming readable code",
                verify_descriptor(0xE, 0, present),
                2,
                1,
            ),
        ] {
            install_data_descriptor(&memory, &descriptor);
            assert_eq!(
                helper(&mut vcpu, 0x10, VERIFY_READ),
                read,
                "{name} P={present}"
            );
            assert_eq!(
                helper(&mut vcpu, 0x10, VERIFY_WRITE),
                write,
                "{name} P={present}"
            );
        }
    }

    install_data_descriptor(&memory, &verify_descriptor(0x2, 2, true));
    vcpu.sregs.cs.selector = 0x2;
    assert_eq!(helper(&mut vcpu, 0x12, VERIFY_WRITE), 2);
    assert_eq!(helper(&mut vcpu, 0x13, VERIFY_WRITE), 1, "RPL exceeds DPL");
    vcpu.sregs.cs.selector = 0x3;
    assert_eq!(helper(&mut vcpu, 0x12, VERIFY_WRITE), 1, "CPL exceeds DPL");

    install_data_descriptor(&memory, &verify_descriptor(0xE, 0, true));
    assert_eq!(
        helper(&mut vcpu, 0x13, VERIFY_READ),
        2,
        "conforming readable code bypasses CPL/RPL checks"
    );
}

#[test]
fn selector_verify_helper_completes_semantic_failures_and_memory_sources_natively() {
    let memory = memory_with_code(&[]);
    install_data_descriptor(&memory, &verify_descriptor(0x2, 0, false));
    memory
        .write_slice(&0x10_u16.to_le_bytes(), GuestAddress(0x3000))
        .unwrap();
    let mut vcpu = test_vcpu(memory);

    assert_eq!(helper(&mut vcpu, 0, VERIFY_READ), 1, "null selector");
    vcpu.sregs.gdt.limit = 0x0F;
    assert_eq!(helper(&mut vcpu, 0x10, VERIFY_READ), 1, "GDT bounds");
    vcpu.sregs.gdt.limit = 0x1F;
    assert_eq!(helper(&mut vcpu, 0x14, VERIFY_READ), 1, "unusable LDT");

    vcpu.sregs.gdt.base = u64::MAX - 7;
    assert_eq!(
        helper(&mut vcpu, 0x10, VERIFY_READ),
        1,
        "descriptor-address overflow is a completed selector failure"
    );
    vcpu.sregs.gdt.base = 0x1000;
    assert_eq!(
        helper(&mut vcpu, 0x3000, VERIFY_WRITE | MEMORY_SOURCE),
        2,
        "fixed two-byte source-memory form"
    );
}

#[test]
fn selector_verify_helper_rolls_back_every_replayed_memory_probe() {
    const TRACE_SENTINEL: (u8, u64, u8, u64) = (0, 0xAA, 1, 0x55);
    const LOG_SENTINEL: (u64, u8, u64) = (0xBB, 1, 0x66);
    let memory = memory_with_code(&[]);
    memory
        .write_slice(&0x10_u16.to_le_bytes(), GuestAddress(0x3000))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    crate::vm::vcpu::VCpu::set_mem_recording(&mut vcpu, true);

    for (name, operand, encoding, gdt_base, apx, cr0, vm) in [
        (
            "unmapped descriptor after mapped source",
            0x3000,
            VERIFY_READ | MEMORY_SOURCE,
            0x20_000,
            true,
            1,
            false,
        ),
        (
            "unmapped source",
            0x20_000,
            VERIFY_READ | MEMORY_SOURCE,
            0x1000,
            true,
            1,
            false,
        ),
        (
            "APX disabled",
            0x10,
            VERIFY_READ | REQUIRES_APX,
            0x1000,
            false,
            1,
            false,
        ),
        ("real mode", 0x10, VERIFY_READ, 0x1000, true, 0, false),
        ("VM86", 0x10, VERIFY_READ, 0x1000, true, 1, true),
        (
            "unknown encoding",
            0x10,
            VERIFY_READ | 0x8000,
            0x1000,
            true,
            1,
            false,
        ),
    ] {
        vcpu.sregs.gdt.base = gdt_base;
        vcpu.set_apx_enabled(apx);
        vcpu.sregs.cr0 = cr0;
        vcpu.regs.rflags = 0x2 | u64::from(vm) * flags::bits::VM;
        vcpu.jit_mem_trace = Some(vec![TRACE_SENTINEL]);
        vcpu.jit_mem_log = Some(vec![LOG_SENTINEL]);
        assert_eq!(helper(&mut vcpu, operand, encoding), 0, "{name}");
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
fn jit_selector_verify_read_write_presence_and_zf_only_match_direct() {
    let code = [
        0x0F, 0x00, 0xE0, // VERR AX
        0x0F, 0x94, 0xC3, // SETZ BL
        0x0F, 0x00, 0xE8, // VERW AX
        0x0F, 0x94, 0xC1, // SETZ CL
        0xEB, 0x00, // JMP HLT
        0xF4,
    ];
    for (name, descriptor, expected_bl, expected_cl, expected_zf) in [
        (
            "non-present read-only",
            verify_descriptor(0x0, 0, false),
            1,
            0,
            false,
        ),
        (
            "non-present writable",
            verify_descriptor(0x2, 0, false),
            1,
            1,
            true,
        ),
        (
            "present execute-only",
            verify_descriptor(0x8, 0, true),
            0,
            0,
            false,
        ),
        (
            "present readable code",
            verify_descriptor(0xA, 0, true),
            1,
            0,
            false,
        ),
    ] {
        let (mut direct, mut native, _, _) = differential_vcpus(&code, descriptor);
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_jit_mem(true);
            vcpu.regs.rax = 0x10;
            vcpu.regs.rbx = 0xAAAA_BBBB_CCCC_DD00;
            vcpu.regs.rcx = 0x1111_2222_3333_4400;
            vcpu.regs.rflags = 0x2 | flags::bits::CF | flags::bits::PF | flags::bits::OF;
        }

        run_direct_to(&mut direct, 14);
        let region = native
            .jit_compile_region()
            .expect("compile VERR/VERW region")
            .unwrap_or_else(|| panic!("{name}: VERR/VERW must be native eligible"));
        native.jit_run_region_native(&region);

        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}");
        assert_eq!(native.regs.rip, 14, "{name}");
        assert_eq!(native.regs.rbx as u8, expected_bl, "{name}");
        assert_eq!(native.regs.rcx as u8, expected_cl, "{name}");
        assert_eq!(
            native.regs.rflags & flags::bits::ZF != 0,
            expected_zf,
            "{name}"
        );
        assert_ne!(native.regs.rflags & flags::bits::CF, 0, "{name}");
        assert_ne!(native.regs.rflags & flags::bits::PF, 0, "{name}");
        assert_ne!(native.regs.rflags & flags::bits::OF, 0, "{name}");
    }
}

#[test]
fn jit_selector_verify_apx_register_and_memory_addresses_match_direct() {
    let code = [
        0xD5, 0x91, 0x00, 0xE7, // VERR R31W
        0x0F, 0x94, 0xC3, // SETZ BL
        0xD5, 0xB3, 0x00, 0x2C, 0xD1, // VERW word ptr [R25+R26*8]
        0x0F, 0x94, 0xC1, // SETZ CL
        0xEB, 0x00, // JMP HLT
        0xF4,
    ];
    let (mut direct, mut native, direct_memory, native_memory) =
        differential_vcpus(&code, verify_descriptor(0x2, 0, false));
    let source_address = 0x3000 + 4 * 8;
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.set_jit_mem(true);
        vcpu.regs.r31 = 0x10;
        vcpu.regs.r25 = 0x3000;
        vcpu.regs.r26 = 4;
    }
    for memory in [&direct_memory, &native_memory] {
        memory
            .write_slice(&0x10_u16.to_le_bytes(), GuestAddress(source_address))
            .unwrap();
    }

    run_direct_to(&mut direct, 17);
    let region = native
        .jit_compile_region()
        .expect("compile APX VERR/VERW region")
        .expect("APX VERR/VERW register and memory forms must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(gprs(&native.regs), gprs(&direct.regs));
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 17);
    assert_eq!(native.regs.rbx as u8, 1);
    assert_eq!(native.regs.rcx as u8, 1);
}
