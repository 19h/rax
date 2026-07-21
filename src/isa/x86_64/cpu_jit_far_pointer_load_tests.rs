//! Production-helper and direct/native differentials for long-mode
//! `LSS/LFS/LGS`.

use super::jit_selector_tests::{
    data_descriptor, exception_without_idt, gprs, install_data_descriptor, memory_with_code,
    segment_fingerprint, test_vcpu,
};
use super::*;
use crate::smir::lower::runtime::GuestRegs;
use vm_memory::{Bytes, GuestAddress};

const POINTER: u64 = 0x3000;

fn write_pointer(memory: &vm_memory::GuestMemoryMmap, offset: u64, selector: u16, width: usize) {
    let mut pointer = offset.to_le_bytes()[..width].to_vec();
    pointer.extend_from_slice(&selector.to_le_bytes());
    memory.write_slice(&pointer, GuestAddress(POINTER)).unwrap();
}

fn far_encoding(selector_id: u32, dst: u8, width: usize, requires_apx: bool) -> u32 {
    let width_code = match width {
        2 => 0,
        4 => 1,
        8 => 2,
        _ => unreachable!(),
    };
    1 | (u32::from(requires_apx) << 1)
        | (selector_id << 2)
        | (1 << 7)
        | (u32::from(dst) << 8)
        | (width_code << 13)
}

fn selected_segment(vcpu: &X86_64Vcpu, selector_id: u32) -> &crate::vm::vcpu::Segment {
    match selector_id {
        4 => &vcpu.sregs.ss,
        6 => &vcpu.sregs.fs,
        7 => &vcpu.sregs.gs,
        _ => unreachable!(),
    }
}

#[test]
fn far_pointer_helper_commits_all_targets_and_gpr_widths_only_after_segment_success() {
    for (name, selector_id, dst, width, requires_apx, offset, expected) in [
        (
            "LFS m16:16",
            6_u32,
            1_u8,
            2_usize,
            false,
            0x1234_BEEF_u64,
            0xA5A5_5A5A_DEAD_BEEF,
        ),
        ("LGS m16:32", 7, 0, 4, false, 0x1234_89AB_CDEF, 0x89AB_CDEF),
        (
            "LSS m16:64 EGPR",
            4,
            31,
            8,
            true,
            0x0123_4567_89AB_CDEF,
            0x0123_4567_89AB_CDEF,
        ),
    ] {
        let memory = memory_with_code(&[]);
        write_pointer(&memory, offset, 0x10, width);
        let descriptor = data_descriptor(0x1234_5000, 0xA_BCDE, 0, true, 0x2, false);
        install_data_descriptor(&memory, &descriptor);
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.set_apx_enabled(requires_apx);
        let mut state = GuestRegs::default();
        state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
        state.gpr[dst as usize] = 0xA5A5_5A5A_DEAD_BEEF;
        state.interrupt_inhibit = 0;

        assert_eq!(
            unsafe {
                rax_jit_system_selector_load(
                    &mut state,
                    POINTER,
                    far_encoding(selector_id, dst, width, requires_apx),
                )
            },
            1,
            "{name}"
        );
        assert_eq!(state.gpr[dst as usize], expected, "{name}");
        let segment = selected_segment(&vcpu, selector_id);
        assert_eq!(segment.selector, 0x10, "{name}");
        assert_eq!(segment.base, 0x1234_5000, "{name}");
        assert_eq!(segment.limit, 0xA_BCDE_FFF, "{name}");
        assert_eq!(segment.type_, 0x3, "{name}");
        assert_eq!(
            state.interrupt_inhibit,
            u64::from(selector_id == 4),
            "{name}"
        );
        assert_eq!(state.fs_base, vcpu.sregs.fs.base, "{name}");
        assert_eq!(state.gs_base, vcpu.sregs.gs.base, "{name}");
        let mut raw = [0_u8; 8];
        memory.read_slice(&mut raw, GuestAddress(0x1010)).unwrap();
        assert_ne!(u64::from_le_bytes(raw) & (1 << 40), 0, "{name}");
    }
}

#[test]
fn far_pointer_helper_rejects_every_malformed_mode_range_and_apx_shape_without_commit() {
    let memory = memory_with_code(&[]);
    write_pointer(&memory, 0x89AB_CDEF, 0, 4);
    let mut vcpu = test_vcpu(memory);
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    state.gpr[1] = 0xA5A5_5A5A_DEAD_BEEF;

    for (name, operand, encoding) in [
        ("far requires memory", POINTER, 0x80 | (6 << 2) | (1 << 13)),
        (
            "far rejects memory64",
            POINTER,
            far_encoding(6, 1, 4, false) | (1 << 5),
        ),
        (
            "far rejects stack source",
            POINTER,
            far_encoding(6, 1, 4, false) | (1 << 6),
        ),
        ("far rejects LDTR", POINTER, far_encoding(0, 1, 4, false)),
        ("far rejects TR", POINTER, far_encoding(1, 1, 4, false)),
        ("far rejects ES", POINTER, far_encoding(2, 1, 4, false)),
        ("far rejects DS", POINTER, far_encoding(5, 1, 4, false)),
        (
            "far rejects reserved width",
            POINTER,
            1 | (6 << 2) | (1 << 7) | (1 << 8) | (3 << 13),
        ),
        (
            "EGPR destination requires APX",
            POINTER,
            far_encoding(6, 31, 4, false),
        ),
        (
            "unknown high bit",
            POINTER,
            far_encoding(6, 1, 4, false) | 0x8000,
        ),
        ("unmapped source", 0x20_000, far_encoding(6, 1, 4, false)),
        (
            "noncanonical start",
            0x0000_8000_0000_0000,
            far_encoding(6, 1, 4, false),
        ),
        (
            "full pointer crosses canonical boundary",
            0x0000_7FFF_FFFF_FFFB,
            far_encoding(6, 1, 4, false),
        ),
        (
            "full pointer wraps",
            u64::MAX - 4,
            far_encoding(6, 1, 4, false),
        ),
    ] {
        let before_fs = segment_fingerprint(&vcpu.sregs.fs);
        assert_eq!(
            unsafe { rax_jit_system_selector_load(&mut state, operand, encoding) },
            0,
            "{name}"
        );
        assert_eq!(state.gpr[1], 0xA5A5_5A5A_DEAD_BEEF, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs, "{name}");
    }

    let valid = far_encoding(6, 1, 4, false);
    let mode_cases: [(&str, fn(&mut X86_64Vcpu), fn(&mut X86_64Vcpu)); 4] = [
        (
            "EFER.LMA",
            |vcpu: &mut X86_64Vcpu| vcpu.sregs.efer &= !(1 << 10),
            |vcpu: &mut X86_64Vcpu| vcpu.sregs.efer |= 1 << 10,
        ),
        (
            "CS.L",
            |vcpu: &mut X86_64Vcpu| vcpu.sregs.cs.l = false,
            |vcpu: &mut X86_64Vcpu| vcpu.sregs.cs.l = true,
        ),
        (
            "CR0.PE",
            |vcpu: &mut X86_64Vcpu| vcpu.sregs.cr0 &= !1,
            |vcpu: &mut X86_64Vcpu| vcpu.sregs.cr0 |= 1,
        ),
        (
            "RFLAGS.VM",
            |vcpu: &mut X86_64Vcpu| vcpu.regs.rflags |= flags::bits::VM,
            |vcpu: &mut X86_64Vcpu| vcpu.regs.rflags &= !flags::bits::VM,
        ),
    ];
    for (name, mutate, restore) in mode_cases {
        mutate(&mut vcpu);
        assert_eq!(
            unsafe { rax_jit_system_selector_load(&mut state, POINTER, valid) },
            0,
            "{name}"
        );
        assert_eq!(state.gpr[1], 0xA5A5_5A5A_DEAD_BEEF, "{name}");
        restore(&mut vcpu);
    }

    let apx = far_encoding(6, 31, 4, true);
    state.gpr[31] = 0x3131_3131_3131_3131;
    assert_eq!(
        unsafe { rax_jit_system_selector_load(&mut state, POINTER, apx) },
        0,
        "APX enablement is dynamic"
    );
    assert_eq!(state.gpr[31], 0x3131_3131_3131_3131);
}

#[test]
fn jit_far_pointer_load_widths_targets_alias_and_rex2_egprs_match_direct() {
    for (name, instruction, width, offset, source, dst, target) in [
        (
            "LFS m16:16 RCX,[RAX]",
            &[0x66, 0x0F, 0xB4, 0x08][..],
            2_usize,
            0x1234_BEEF_u64,
            0_u8,
            1_u8,
            6_u32,
        ),
        (
            "LGS m16:32 EAX,[RAX] alias",
            &[0x0F, 0xB5, 0x00][..],
            4,
            0x1234_89AB_CDEF,
            0,
            0,
            7,
        ),
        (
            "LSS m16:64 R31,[R16]",
            &[0xD5, 0xDC, 0xB2, 0x38][..],
            8,
            0x0123_4567_89AB_CDEF,
            16,
            31,
            4,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        let descriptor = data_descriptor(0x7654_3000, 0xF_FFFF, 0, true, 0x2, false);
        for memory in [&direct_memory, &native_memory] {
            write_pointer(memory, offset, 0x10, width);
            install_data_descriptor(memory, &descriptor);
        }
        let mut direct = test_vcpu(direct_memory.clone());
        let mut native = test_vcpu(native_memory.clone());
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(source >= 16 || dst >= 16);
            vcpu.set_jit_mem(true);
            vcpu.regs.rflags &= !flags::bits::AF;
            match source {
                0 => vcpu.regs.rax = POINTER,
                16 => vcpu.regs.r16 = POINTER,
                _ => unreachable!(),
            }
            if dst == 1 {
                vcpu.regs.rcx = 0xA5A5_5A5A_DEAD_BEEF;
            }
            if dst == 31 {
                vcpu.regs.r31 = 0x3131_3131_3131_3131;
            }
        }

        assert!(
            direct.step().expect("direct LSS/LFS/LGS").is_none(),
            "{name}"
        );
        let region = native
            .jit_compile_region()
            .expect("compile LSS/LFS/LGS")
            .unwrap_or_else(|| panic!("{name}: exact far-pointer load must be native eligible"));
        native.jit_run_region_verified(&region);

        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}");
        assert_eq!(native.regs.rip, instruction.len() as u64, "{name}");
        assert_eq!(
            segment_fingerprint(selected_segment(&native, target)),
            segment_fingerprint(selected_segment(&direct, target)),
            "{name}"
        );
        assert_eq!(selected_segment(&native, target).selector, 0x10, "{name}");
        assert_eq!(
            selected_segment(&native, target).base,
            0x7654_3000,
            "{name}"
        );
        assert_eq!(native.interrupt_inhibit, direct.interrupt_inhibit, "{name}");
        assert_eq!(native.interrupt_inhibit, target == 4, "{name}");
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
fn jit_lfs_fs_override_uses_old_base_before_committing_new_hidden_state() {
    let instruction = [0x64, 0x0F, 0xB4, 0x10]; // LFS EDX,m16:32 FS:[RAX]
    let code = [0x64, 0x0F, 0xB4, 0x10, 0xF4];
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    let descriptor = data_descriptor(0x7654_3000, 0xF_FFFF, 0, true, 0x2, false);
    for memory in [&direct_memory, &native_memory] {
        write_pointer(memory, 0x89AB_CDEF, 0x10, 4);
        install_data_descriptor(memory, &descriptor);
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.sregs.fs.base = 0x100;
        vcpu.sregs.fs.selector = 0x28;
        vcpu.regs.rax = POINTER - 0x100;
        vcpu.regs.rdx = 0xA5A5_5A5A_DEAD_BEEF;
        vcpu.regs.rflags &= !flags::bits::AF;
        vcpu.set_jit_mem(true);
    }

    direct.step().expect("direct FS-relative LFS");
    let region = native
        .jit_compile_region()
        .expect("compile FS-relative LFS")
        .expect("state-backed FS-relative far pointer must be native eligible");
    native.jit_run_region_verified(&region);

    assert_eq!(native.regs.rdx, 0x89AB_CDEF);
    assert_eq!(native.regs.rdx, direct.regs.rdx);
    assert_eq!(native.regs.rax, POINTER - 0x100);
    assert_eq!(native.regs.rip, instruction.len() as u64);
    assert_eq!(native.sregs.fs.base, 0x7654_3000);
    assert_eq!(
        segment_fingerprint(&native.sregs.fs),
        segment_fingerprint(&direct.sregs.fs)
    );
}

#[test]
fn jit_far_pointer_load_descriptor_fault_deopts_without_gpr_or_segment_commit() {
    for (name, instruction, target, descriptor, expected_vector) in [
        (
            "LFS not present",
            &[0x0F, 0xB4, 0x08][..],
            6_u32,
            data_descriptor(0, 0xFFFF, 0, false, 0x2, false),
            11_u8,
        ),
        (
            "LSS not present",
            &[0x48, 0x0F, 0xB2, 0x08][..],
            4,
            data_descriptor(0, 0xFFFF, 0, false, 0x2, false),
            12,
        ),
        (
            "LGS wrong type",
            &[0x0F, 0xB5, 0x08][..],
            7,
            data_descriptor(0, 0xFFFF, 0, true, 0x8, false),
            13,
        ),
    ] {
        let width = if instruction[0] == 0x48 { 8 } else { 4 };
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let memory = memory_with_code(&code);
        write_pointer(&memory, 0x0123_4567_89AB_CDEF, 0x10, width);
        install_data_descriptor(&memory, &descriptor);
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.regs.rax = POINTER;
        vcpu.regs.rcx = 0xA5A5_5A5A_DEAD_BEEF;
        vcpu.regs.rflags &= !flags::bits::AF;
        vcpu.set_jit_mem(true);
        let before_regs = vcpu.regs.clone();
        let before_segment = segment_fingerprint(selected_segment(&vcpu, target));

        let region = vcpu
            .jit_compile_region()
            .expect("compile faulting LSS/LFS/LGS")
            .unwrap_or_else(|| panic!("{name}: dynamic descriptor fault must stay eligible"));
        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(
            segment_fingerprint(selected_segment(&vcpu, target)),
            before_segment,
            "{name}"
        );
        assert!(!vcpu.interrupt_inhibit, "{name}");

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name}: {error}"
        );
        assert_eq!(gprs(&vcpu.regs), gprs(&before_regs), "{name}");
        assert_eq!(
            segment_fingerprint(selected_segment(&vcpu, target)),
            before_segment,
            "{name}"
        );
    }
}

#[test]
fn jit_far_pointer_load_source_and_canonical_faults_deopt_without_commit() {
    for (name, instruction, source, expected_vector) in [
        (
            "unmapped LFS source",
            &[0x0F, 0xB4, 0x08][..],
            0x20_000_u64,
            None,
        ),
        (
            "LFS full pointer crosses canonical boundary",
            &[0x0F, 0xB4, 0x08][..],
            0x0000_7FFF_FFFF_FFFB,
            Some(13_u8),
        ),
        (
            "LSS full pointer wraps",
            &[0x48, 0x0F, 0xB2, 0x0C, 0x24][..],
            u64::MAX - 8,
            Some(12_u8),
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let memory = memory_with_code(&code);
        let mut vcpu = test_vcpu(memory);
        if instruction.last() == Some(&0x24) {
            vcpu.regs.rsp = source;
        } else {
            vcpu.regs.rax = source;
        }
        vcpu.regs.rcx = 0xA5A5_5A5A_DEAD_BEEF;
        vcpu.regs.rflags &= !flags::bits::AF;
        vcpu.set_jit_mem(true);
        let before = vcpu.regs.clone();
        let before_fs = segment_fingerprint(&vcpu.sregs.fs);
        let before_ss = segment_fingerprint(&vcpu.sregs.ss);

        let region = vcpu
            .jit_compile_region()
            .expect("compile dynamically faulting far-pointer load")
            .unwrap_or_else(|| panic!("{name}: dynamic source fault must stay eligible"));
        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rip, 0, "{name}");
        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.ss), before_ss, "{name}");

        match expected_vector {
            Some(vector) => {
                let error = exception_without_idt(&mut vcpu);
                assert!(
                    error.contains(&format!("IDT entry {vector} not present")),
                    "{name}: {error}"
                );
            }
            None => {
                vcpu.step()
                    .expect_err("direct replay must report the unmapped pointer source");
            }
        }
        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs, "{name}");
        assert_eq!(segment_fingerprint(&vcpu.sregs.ss), before_ss, "{name}");
    }
}

#[test]
fn jit_far_pointer_load_accessed_store_fault_deopts_then_direct_faults_without_commit() {
    const PML4: u64 = 0x9000;
    const PDPT: u64 = 0xA000;
    const PD: u64 = 0xB000;
    const PT: u64 = 0xC000;
    const PAGE_FLAGS: u64 = 0x7; // Present | writable | user-accessible.
    const DESCRIPTOR_ADDR: u64 = 0x2000;

    let memory = memory_with_code(&[0x0F, 0xB4, 0x08, 0xF4]);
    write_pointer(&memory, 0x89AB_CDEF, 0x10, 4);
    let descriptor = data_descriptor(0x1234_5000, 0xFFFF, 0, true, 0x2, false);
    memory
        .write_slice(&descriptor, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
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
    vcpu.regs.rax = POINTER;
    vcpu.regs.rcx = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.regs.rflags &= !flags::bits::AF;
    vcpu.set_jit_mem(true);
    let before_regs = vcpu.regs.clone();
    let before_fs = segment_fingerprint(&vcpu.sregs.fs);

    let region = vcpu
        .jit_compile_region()
        .expect("compile LFS with a dynamic descriptor-store fault")
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
    assert_eq!(vcpu.regs.rcx, before_regs.rcx);
    assert_eq!(segment_fingerprint(&vcpu.sregs.fs), before_fs);
    memory
        .read_slice(&mut observed, GuestAddress(DESCRIPTOR_ADDR))
        .unwrap();
    assert_eq!(observed, descriptor);
}

#[test]
fn far_pointer_load_native_admission_is_dynamic_for_apx_and_long_mode() {
    let memory = memory_with_code(&[0xD5, 0xDC, 0xB4, 0x38, 0xF4]);
    write_pointer(&memory, 0x0123_4567_89AB_CDEF, 0, 8);
    let mut apx_disabled = test_vcpu(memory.clone());
    apx_disabled.regs.r16 = POINTER;
    apx_disabled.regs.r31 = 0x3131_3131_3131_3131;
    apx_disabled.set_jit_mem(true);
    let region = apx_disabled
        .jit_compile_region()
        .unwrap()
        .expect("REX2 far-pointer load has a dynamic APX guard");
    apx_disabled.jit_run_region_native(&region);
    assert_eq!(apx_disabled.regs.rip, 0);
    assert_eq!(apx_disabled.regs.r31, 0x3131_3131_3131_3131);
    assert!(exception_without_idt(&mut apx_disabled).contains("IDT entry 6 not present"));

    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.regs.r16 = POINTER;
    compatibility.set_apx_enabled(true);
    compatibility.set_jit_mem(true);
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "the runtime long-mode gate must reject compatibility execution"
    );
}
