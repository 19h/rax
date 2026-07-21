//! Production-helper and direct/native differentials for long-mode POP FS/GS.

use super::jit_selector_tests::{
    data_descriptor, exception_without_idt, gprs, install_data_descriptor, memory_with_code,
    segment_fingerprint, test_vcpu,
};
use super::*;
use crate::smir::lower::runtime::GuestRegs;
use vm_memory::{Bytes, GuestAddress};

const STACK: u64 = 0x3000;

fn write_stack(memory: &vm_memory::GuestMemoryMmap, selector: u16) {
    let mut source = [0xA5_u8; 8];
    source[..2].copy_from_slice(&selector.to_le_bytes());
    memory.write_slice(&source, GuestAddress(STACK)).unwrap();
}

#[test]
fn pop_segment_helper_stack_encoding_loads_exact_width_and_rejects_malformed_shapes() {
    for (selector_id, encoding, target_fs) in [(6_u32, 0x79_u32, true), (7, 0x5D, false)] {
        let memory = memory_with_code(&[]);
        write_stack(&memory, 0x10);
        let descriptor = data_descriptor(0x1234_5000, 0xA_BCDE, 0, true, 0x2, false);
        install_data_descriptor(&memory, &descriptor);
        let mut vcpu = test_vcpu(memory.clone());
        let mut state = GuestRegs::default();
        state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
        state.gpr[4] = STACK;

        assert_eq!(
            unsafe { rax_jit_system_selector_load(&mut state, STACK, encoding) },
            1,
            "selector ID {selector_id}"
        );
        let segment = if target_fs {
            &vcpu.sregs.fs
        } else {
            &vcpu.sregs.gs
        };
        assert_eq!(segment.selector, 0x10);
        assert_eq!(segment.base, 0x1234_5000);
        assert_eq!(segment.limit, 0xA_BCDE_FFF);
        assert_eq!(segment.type_, 0x3);
        assert_eq!(state.gpr[4], STACK, "helper caller owns RSP commit");
        let mut raw = [0_u8; 8];
        memory.read_slice(&mut raw, GuestAddress(0x1010)).unwrap();
        assert_ne!(u64::from_le_bytes(raw) & (1 << 40), 0);
    }

    let memory = memory_with_code(&[]);
    write_stack(&memory, 0);
    let mut vcpu = test_vcpu(memory);
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    for (name, operand, encoding) in [
        ("stack bit requires memory", STACK, 0x58_u32),
        ("system selector cannot use stack", STACK, 0x41),
        ("DS cannot use stack", STACK, 0x55),
        ("unknown bit", STACK, 0x8000),
        ("unmapped source", 0x20_000, 0x79),
        ("noncanonical start", 0x0000_8000_0000_0000, 0x79),
        ("crosses lower boundary", 0x0000_7FFF_FFFF_FFFC, 0x79),
        ("wraps", u64::MAX - 3, 0x79),
    ] {
        assert_eq!(
            unsafe { rax_jit_system_selector_load(&mut state, operand, encoding) },
            0,
            "{name}"
        );
    }
    vcpu.sregs.efer &= !(1 << 10);
    assert_eq!(
        unsafe { rax_jit_system_selector_load(&mut state, STACK, 0x79) },
        0,
        "stack source requires EFER.LMA"
    );
}

#[test]
fn jit_pop_segment_b2_b8_rex_w_apx_and_hidden_state_match_direct() {
    for (name, instruction, width, apx, target_fs) in [
        ("FS-B8", &[0x0F, 0xA1][..], 8_u64, false, true),
        ("GS-B2", &[0x66, 0x0F, 0xA9][..], 2, false, false),
        (
            "FS-66-REX.W-B8",
            &[0x66, 0x48, 0x0F, 0xA1][..],
            8,
            false,
            true,
        ),
        ("GS-REX2-B8", &[0xD5, 0x80, 0xA9][..], 8, true, false),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        let descriptor = data_descriptor(0x7654_3000, 0xF_FFFF, 0, true, 0x2, false);
        for memory in [&direct_memory, &native_memory] {
            install_data_descriptor(memory, &descriptor);
            write_stack(memory, 0x10);
        }
        let mut direct = test_vcpu(direct_memory.clone());
        let mut native = test_vcpu(native_memory.clone());
        for vcpu in [&mut direct, &mut native] {
            vcpu.regs.rsp = STACK;
            vcpu.set_apx_enabled(apx);
            vcpu.set_jit_mem(true);
            // arm64-hosted amd64 translation can drop AF in the native
            // trampoline; lowerer tests cover the physical flag round trip.
            vcpu.regs.rflags &= !flags::bits::AF;
        }

        assert!(direct.step().expect("direct POP FS/GS").is_none(), "{name}");
        let region = native
            .jit_compile_region()
            .expect("compile POP FS/GS")
            .unwrap_or_else(|| panic!("{name}: POP FS/GS must be native eligible"));
        native.jit_run_region_verified(&region);

        let (direct_segment, native_segment) = if target_fs {
            (&direct.sregs.fs, &native.sregs.fs)
        } else {
            (&direct.sregs.gs, &native.sregs.gs)
        };
        assert_eq!(
            segment_fingerprint(native_segment),
            segment_fingerprint(direct_segment),
            "{name}"
        );
        assert_eq!(native_segment.selector, 0x10, "{name}");
        assert_eq!(native_segment.base, 0x7654_3000, "{name}");
        assert_eq!(native_segment.type_, 0x3, "{name}");
        assert_eq!(native.regs.rsp, STACK + width, "{name}");
        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}");
        assert_eq!(native.regs.rip, instruction.len() as u64, "{name}");
        let mut direct_raw = [0_u8; 8];
        let mut native_raw = [0_u8; 8];
        direct_memory
            .read_slice(&mut direct_raw, GuestAddress(0x1010))
            .unwrap();
        native_memory
            .read_slice(&mut native_raw, GuestAddress(0x1010))
            .unwrap();
        assert_eq!(native_raw, direct_raw, "{name}");
        assert_ne!(u64::from_le_bytes(native_raw) & (1 << 40), 0, "{name}");
    }
}

#[test]
fn jit_pop_segment_descriptor_faults_deopt_with_rsp_and_segment_noncommitting() {
    for (name, descriptor, expected_vector) in [
        (
            "wrong type",
            data_descriptor(0, 0xFFFF, 0, true, 0x8, false),
            13,
        ),
        (
            "not present",
            data_descriptor(0, 0xFFFF, 0, false, 0x2, false),
            11,
        ),
    ] {
        let memory = memory_with_code(&[0x0F, 0xA1, 0xF4]);
        install_data_descriptor(&memory, &descriptor);
        write_stack(&memory, 0x10);
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.regs.rsp = STACK;
        vcpu.set_jit_mem(true);
        let before_regs = vcpu.regs.clone();
        let before_fs = segment_fingerprint(&vcpu.sregs.fs);

        let region = vcpu
            .jit_compile_region()
            .expect("compile faulting POP FS")
            .unwrap_or_else(|| panic!("{name}: dynamic fault must remain native eligible"));
        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs, "{name}");
        let mut raw = [0_u8; 8];
        memory.read_slice(&mut raw, GuestAddress(0x1010)).unwrap();
        assert_eq!(raw, descriptor, "{name}");

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name}: {error}"
        );
        assert_eq!(vcpu.regs.rsp, STACK, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs, "{name}");
    }
}

#[test]
fn jit_pop_segment_noncanonical_guard_deopts_before_direct_ss_delivery() {
    for (name, instruction, rsp) in [
        ("B8 crossing", &[0x0F, 0xA1][..], 0x0000_7FFF_FFFF_FFFC_u64),
        ("B8 wrap", &[0x0F, 0xA1][..], u64::MAX - 3),
        (
            "B2 crossing",
            &[0x66, 0x0F, 0xA1][..],
            0x0000_7FFF_FFFF_FFFF,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let memory = memory_with_code(&code);
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.rsp = rsp;
        vcpu.set_jit_mem(true);
        let before_fs = segment_fingerprint(&vcpu.sregs.fs);
        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically invalid POP FS")
            .unwrap_or_else(|| panic!("{name}: canonical guard must remain dynamic"));
        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        assert_eq!(vcpu.regs.rsp, rsp, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs, "{name}");

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains("IDT entry 12 not present"),
            "{name}: {error}"
        );
        assert_eq!(vcpu.regs.rsp, rsp, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs, "{name}");
    }
}

#[test]
fn jit_pop_segment_source_fault_deopts_without_rsp_or_segment_commit() {
    let memory = memory_with_code(&[0x0F, 0xA1, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rsp = 0x20_000;
    vcpu.set_jit_mem(true);
    let before_regs = vcpu.regs.clone();
    let before_fs = segment_fingerprint(&vcpu.sregs.fs);

    let region = vcpu
        .jit_compile_region()
        .expect("compile POP FS with a dynamic source fault")
        .expect("the source fault must remain dynamically native eligible");
    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(gprs(&vcpu.regs), gprs(&before_regs));
    assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs);

    vcpu.step()
        .expect_err("direct replay must report the unmapped stack source");
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rsp, before_regs.rsp);
    assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs);
}

#[test]
fn jit_pop_segment_accessed_store_fault_deopts_then_direct_faults_without_commit() {
    const PML4: u64 = 0x9000;
    const PDPT: u64 = 0xA000;
    const PD: u64 = 0xB000;
    const PT: u64 = 0xC000;
    const PAGE_FLAGS: u64 = 0x7; // Present | writable | user-accessible.
    const DESCRIPTOR_ADDR: u64 = 0x2000;

    let memory = memory_with_code(&[0x0F, 0xA1, 0xF4]);
    let descriptor = data_descriptor(0x1234_5000, 0xFFFF, 0, true, 0x2, false);
    memory
        .write_slice(&descriptor, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
    write_stack(&memory, 0x10);
    for (address, entry) in [
        (PML4, PDPT | PAGE_FLAGS),
        (PDPT, PD | PAGE_FLAGS),
        (PD, PT | PAGE_FLAGS),
    ] {
        memory
            .write_slice(&entry.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }
    for page in 0..16_u64 {
        let flags = if page == DESCRIPTOR_ADDR >> 12 {
            PAGE_FLAGS & !0x2
        } else {
            PAGE_FLAGS
        };
        memory
            .write_slice(
                &(page * 0x1000 | flags).to_le_bytes(),
                GuestAddress(PT + page * 8),
            )
            .unwrap();
    }

    let mut vcpu = test_vcpu(memory.clone());
    vcpu.sregs.gdt.base = DESCRIPTOR_ADDR - 0x10;
    vcpu.sregs.cr0 |= 1 << 31;
    vcpu.sregs.cr3 = PML4;
    vcpu.sregs.cr4 |= 1 << 5;
    vcpu.sregs.efer |= 1 << 8;
    vcpu.regs.rsp = STACK;
    vcpu.regs.rflags &= !flags::bits::AF;
    vcpu.set_jit_mem(true);
    let before_regs = vcpu.regs.clone();
    let before_fs = segment_fingerprint(&vcpu.sregs.fs);

    let region = vcpu
        .jit_compile_region()
        .expect("compile POP FS with a dynamic descriptor-store fault")
        .expect("the accessed-bit store fault must remain native eligible");
    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(gprs(&vcpu.regs), gprs(&before_regs));
    assert_eq!(vcpu.regs.rflags, before_regs.rflags);
    assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs);
    let mut observed = [0_u8; 8];
    memory
        .read_slice(&mut observed, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
    assert_eq!(observed, descriptor);

    assert!(matches!(
        vcpu.step(),
        Err(crate::error::Error::PageFault {
            vaddr: DESCRIPTOR_ADDR,
            error_code: 0x3,
        })
    ));
    assert_eq!(vcpu.regs.rsp, before_regs.rsp);
    assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs);
    memory
        .read_slice(&mut observed, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
    assert_eq!(observed, descriptor);
}
