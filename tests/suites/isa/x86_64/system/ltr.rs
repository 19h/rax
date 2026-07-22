//! Architectural LTR coverage: operand decode, TSS descriptor validation,
//! locked busy transition, hidden task-register state, and precise faults.

use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::{Registers, Segment, VCpu};

use crate::common::{
    Bytes, CODE_ADDR, DATA_ADDR, GDT_BASE, GuestAddress, GuestMemoryMmap, run_until_hlt,
    setup_apx_vm, setup_vm, setup_vm_compat, setup_vm_no_idt,
};

fn set_gpr(regs: &mut Registers, index: u8, value: u64) {
    match index {
        0 => regs.rax = value,
        1 => regs.rcx = value,
        2 => regs.rdx = value,
        3 => regs.rbx = value,
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        6 => regs.rsi = value,
        7 => regs.rdi = value,
        8 => regs.r8 = value,
        9 => regs.r9 = value,
        10 => regs.r10 = value,
        11 => regs.r11 = value,
        12 => regs.r12 = value,
        13 => regs.r13 = value,
        14 => regs.r14 = value,
        15 => regs.r15 = value,
        16 => regs.r16 = value,
        17 => regs.r17 = value,
        18 => regs.r18 = value,
        19 => regs.r19 = value,
        20 => regs.r20 = value,
        21 => regs.r21 = value,
        22 => regs.r22 = value,
        23 => regs.r23 = value,
        24 => regs.r24 = value,
        25 => regs.r25 = value,
        26 => regs.r26 = value,
        27 => regs.r27 = value,
        28 => regs.r28 = value,
        29 => regs.r29 = value,
        30 => regs.r30 = value,
        31 => regs.r31 = value,
        _ => unreachable!(),
    }
}

fn get_gpr(regs: &Registers, index: u8) -> u64 {
    match index {
        0 => regs.rax,
        1 => regs.rcx,
        2 => regs.rdx,
        3 => regs.rbx,
        4 => regs.rsp,
        5 => regs.rbp,
        6 => regs.rsi,
        7 => regs.rdi,
        8 => regs.r8,
        9 => regs.r9,
        10 => regs.r10,
        11 => regs.r11,
        12 => regs.r12,
        13 => regs.r13,
        14 => regs.r14,
        15 => regs.r15,
        _ => unreachable!(),
    }
}

fn tss_descriptor(
    base: u64,
    raw_limit: u32,
    dpl: u8,
    present: bool,
    type_: u8,
    granularity: bool,
    avl: bool,
) -> [u8; 16] {
    assert!(raw_limit <= 0xF_FFFF);
    let mut low = u64::from(raw_limit & 0xFFFF)
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (u64::from(type_ & 0xF) << 40)
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

fn install_descriptor(
    vcpu: &mut X86_64Vcpu,
    memory: &GuestMemoryMmap,
    selector: u16,
    descriptor: &[u8],
) {
    assert_eq!(selector & 0x4, 0);
    let offset = u64::from(selector >> 3) * 8;
    memory
        .write_slice(descriptor, GuestAddress(GDT_BASE + offset))
        .unwrap();
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.gdt.limit = sregs
        .gdt
        .limit
        .max((offset + descriptor.len() as u64 - 1) as u16);
    vcpu.set_sregs(&sregs).unwrap();
}

fn install_valid_descriptor(vcpu: &mut X86_64Vcpu, memory: &GuestMemoryMmap) {
    install_descriptor(
        vcpu,
        memory,
        0x10,
        &tss_descriptor(0x1234_5678, 0x67, 0, true, 0x9, false, false),
    );
}

fn descriptor_bytes(memory: &GuestMemoryMmap, selector: u16, len: usize) -> Vec<u8> {
    let mut bytes = vec![0_u8; len];
    memory
        .read_slice(
            &mut bytes,
            GuestAddress(GDT_BASE + u64::from(selector >> 3) * 8),
        )
        .unwrap();
    bytes
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

fn seeded_tr() -> Segment {
    Segment {
        base: 0xDEAD_BEEF,
        limit: 0x1234,
        selector: 0x2468,
        type_: 0xB,
        present: true,
        dpl: 2,
        g: true,
        avl: true,
        unusable: false,
        ..Segment::default()
    }
}

fn segment_fingerprint(
    segment: &Segment,
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

#[test]
fn ltr_register_forms_cover_every_legacy_and_rex_source_and_ignore_operand_size() {
    for index in 0_u8..16 {
        for operand_form in [0_u8, 1, 2] {
            let mut code = Vec::new();
            match operand_form {
                0 => {
                    if index >= 8 {
                        code.push(0x41);
                    }
                }
                1 => {
                    code.push(0x66);
                    if index >= 8 {
                        code.push(0x41);
                    }
                }
                2 => code.push(0x48 | u8::from(index >= 8)),
                _ => unreachable!(),
            }
            code.extend_from_slice(&[0x0F, 0x00, 0xD8 | (index & 7), 0xF4]);

            let source = 0xA5A5_5A5A_0000_0010;
            let mut initial = Registers::default();
            set_gpr(&mut initial, index, source);
            let (mut vcpu, memory) = setup_vm(&code, Some(initial));
            install_valid_descriptor(&mut vcpu, &memory);

            let regs = run_until_hlt(&mut vcpu).unwrap();
            assert_eq!(
                get_gpr(&regs, index),
                source,
                "index={index}, operand_form={operand_form}"
            );
            let tr = vcpu.get_sregs().unwrap().tr;
            assert_eq!(tr.selector, 0x10);
            assert_eq!(tr.type_, 0xB);
            assert_eq!(descriptor_bytes(&memory, 0x10, 8)[5] & 0x0F, 0xB);
        }
    }
}

#[test]
fn ltr_rex2_reads_egpr_and_requires_apx() {
    let code = [0xD5, 0x91, 0x00, 0xDF, 0xF4]; // LTR R31W; HLT
    let mut initial = Registers::default();
    initial.r31 = 0x3131_3131_0000_0010;
    let (mut vcpu, memory) = setup_apx_vm(&code, Some(initial.clone()));
    install_valid_descriptor(&mut vcpu, &memory);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.r31, 0x3131_3131_0000_0010);
    assert_eq!(vcpu.get_sregs().unwrap().tr.selector, 0x10);

    let (mut disabled, _) = setup_vm_no_idt(&code, Some(initial));
    let error = exception_without_idt(&mut disabled);
    assert!(error.contains("IDT entry 6 not present"), "{error}");
}

#[test]
fn ltr_memory_address_forms_read_exactly_two_source_bytes() {
    for index in 0_u8..16 {
        let mut code = Vec::new();
        if index >= 8 {
            code.push(0x41);
        }
        match index & 7 {
            4 => code.extend_from_slice(&[0x0F, 0x00, 0x1C, 0x24]),
            5 => code.extend_from_slice(&[0x0F, 0x00, 0x5D, 0x00]),
            rm => code.extend_from_slice(&[0x0F, 0x00, 0x18 | rm]),
        }
        code.push(0xF4);

        let mut initial = Registers::default();
        set_gpr(&mut initial, index, DATA_ADDR);
        let (mut vcpu, memory) = setup_vm(&code, Some(initial));
        install_valid_descriptor(&mut vcpu, &memory);
        memory
            .write_slice(&[0x10, 0x00, 0xA5], GuestAddress(DATA_ADDR))
            .unwrap();

        let regs = run_until_hlt(&mut vcpu).unwrap();
        assert_eq!(vcpu.get_sregs().unwrap().tr.selector, 0x10, "index={index}");
        assert_eq!(get_gpr(&regs, index), DATA_ADDR, "index={index}");
        let mut source = [0_u8; 3];
        memory
            .read_slice(&mut source, GuestAddress(DATA_ADDR))
            .unwrap();
        assert_eq!(source, [0x10, 0x00, 0xA5], "index={index}");
    }

    for (name, code, configure) in [
        (
            "absolute SIB",
            vec![0x0F, 0x00, 0x1C, 0x25, 0x00, 0x20, 0x00, 0x00, 0xF4],
            None,
        ),
        (
            "RSP+RCX*2+4",
            vec![0x0F, 0x00, 0x5C, 0x4C, 0x04, 0xF4],
            Some((4_u8, DATA_ADDR - 0x24)),
        ),
        (
            "RIP relative",
            vec![0x0F, 0x00, 0x1D, 0xF9, 0x0F, 0x00, 0x00, 0xF4],
            None,
        ),
        (
            "addr32 EIP relative",
            vec![0x67, 0x0F, 0x00, 0x1D, 0xF8, 0x0F, 0x00, 0x00, 0xF4],
            None,
        ),
        (
            "addr32 absolute",
            vec![0x67, 0x0F, 0x00, 0x1C, 0x25, 0x00, 0x20, 0x00, 0x00, 0xF4],
            None,
        ),
    ] {
        let mut initial = Registers::default();
        if let Some((index, value)) = configure {
            set_gpr(&mut initial, index, value);
            if name == "RSP+RCX*2+4" {
                initial.rcx = 0x10;
            }
        }
        let (mut vcpu, memory) = setup_vm(&code, Some(initial));
        install_valid_descriptor(&mut vcpu, &memory);
        memory
            .write_slice(&[0x10, 0x00, 0xA5], GuestAddress(DATA_ADDR))
            .unwrap();

        run_until_hlt(&mut vcpu).unwrap();
        assert_eq!(vcpu.get_sregs().unwrap().tr.selector, 0x10, "{name}");
        let mut source = [0_u8; 3];
        memory
            .read_slice(&mut source, GuestAddress(DATA_ADDR))
            .unwrap();
        assert_eq!(source, [0x10, 0x00, 0xA5], "{name}");
    }
}

#[test]
fn ltr_loads_complete_busy_descriptor_and_preserves_rflags_source_and_rpl() {
    let code = [0x0F, 0x00, 0xD8, 0xF4];
    let base = 0xFFFF_8000_1234_5000;
    let raw_limit = 0xA_BCDE;
    let descriptor = tss_descriptor(base, raw_limit, 3, true, 0x9, true, true);
    let initial = Registers {
        rax: 0xA5A5_5A5A_0000_0013,
        rsp: 0x8000,
        rflags: 0x0CD7,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_vm(&code, Some(initial));
    install_descriptor(&mut vcpu, &memory, 0x13, &descriptor);

    let before_flags = vcpu.get_regs().unwrap().rflags;
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let tr = vcpu.get_sregs().unwrap().tr;
    assert_eq!(regs.rax, 0xA5A5_5A5A_0000_0013);
    assert_eq!(regs.rflags, before_flags);
    assert_eq!(tr.selector, 0x13);
    assert_eq!(tr.base, base);
    assert_eq!(tr.limit, (raw_limit << 12) | 0xFFF);
    assert_eq!(tr.type_, 0xB);
    assert!(tr.present);
    assert_eq!(tr.dpl, 3);
    assert!(tr.g);
    assert!(tr.avl);
    assert!(!tr.s);
    assert!(!tr.unusable);

    let busy = descriptor_bytes(&memory, 0x13, 16);
    let mut expected_busy = descriptor;
    expected_busy[5] |= 0x2;
    assert_eq!(busy, expected_busy);
}

#[test]
fn ltr_repeated_selector_faults_busy_and_preserves_first_commit() {
    let code = [0x0F, 0x00, 0xD8, 0x0F, 0x00, 0xD8];
    let initial = Registers {
        rax: 0x10,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_vm_no_idt(&code, Some(initial));
    install_valid_descriptor(&mut vcpu, &memory);

    assert!(vcpu.step().expect("first LTR must succeed").is_none());
    assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR + 3);
    let committed = vcpu.get_sregs().unwrap().tr;
    let busy = descriptor_bytes(&memory, 0x10, 16);
    let error = exception_without_idt(&mut vcpu);
    assert!(error.contains("IDT entry 13 not present"), "{error}");
    assert_eq!(
        segment_fingerprint(&vcpu.get_sregs().unwrap().tr),
        segment_fingerprint(&committed)
    );
    assert_eq!(descriptor_bytes(&memory, 0x10, 16), busy);
    assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR + 3);
}

#[test]
fn ltr_null_and_descriptor_faults_select_exact_vectors_and_do_not_commit_or_write() {
    for selector in 0_u16..=3 {
        let initial = Registers {
            rax: u64::from(selector),
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x00, 0xD8], Some(initial));
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.gdt.base = u64::MAX;
        sregs.gdt.limit = 0;
        sregs.tr = seeded_tr();
        let before = segment_fingerprint(&sregs.tr);
        vcpu.set_sregs(&sregs).unwrap();
        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains("IDT entry 13 not present"),
            "selector={selector}: {error}"
        );
        assert_eq!(segment_fingerprint(&vcpu.get_sregs().unwrap().tr), before);
    }

    let mut code_descriptor = tss_descriptor(0, 0x67, 0, true, 0x2, false, false);
    code_descriptor[5] |= 1 << 4;
    let busy = tss_descriptor(0, 0x67, 0, true, 0xB, false, false);
    let absent = tss_descriptor(0, 0x67, 0, false, 0x9, false, false);
    let noncanonical = tss_descriptor(0x0000_8000_0000_0000, 0x67, 0, true, 0x9, false, false);
    let mut reserved_high = tss_descriptor(0, 0x67, 0, true, 0x9, false, false);
    reserved_high[12] = 1;
    let mut reserved_l = tss_descriptor(0, 0x67, 0, true, 0x9, false, false);
    reserved_l[6] |= 1 << 5;
    let mut reserved_db = tss_descriptor(0, 0x67, 0, true, 0x9, false, false);
    reserved_db[6] |= 1 << 6;
    let mut absent_reserved = reserved_high;
    absent_reserved[5] &= !(1 << 7);

    for (name, selector, limit, descriptor, vector) in [
        (
            "TI",
            0x14_u16,
            0x1F,
            tss_descriptor(0, 0x67, 0, true, 0x9, false, false),
            13,
        ),
        (
            "limit",
            0x10,
            0x1E,
            tss_descriptor(0, 0x67, 0, true, 0x9, false, false),
            13,
        ),
        ("code/data", 0x10, 0x1F, code_descriptor, 13),
        ("busy", 0x10, 0x1F, busy, 13),
        ("not present", 0x10, 0x1F, absent, 11),
        ("noncanonical", 0x10, 0x1F, noncanonical, 13),
        ("reserved high", 0x10, 0x1F, reserved_high, 13),
        ("reserved L", 0x10, 0x1F, reserved_l, 13),
        ("reserved D/B", 0x10, 0x1F, reserved_db, 13),
        (
            "reserved precedes presence",
            0x10,
            0x1F,
            absent_reserved,
            13,
        ),
    ] {
        let initial = Registers {
            rax: u64::from(selector),
            ..Registers::default()
        };
        let (mut vcpu, memory) = setup_vm_no_idt(&[0x0F, 0x00, 0xD8], Some(initial));
        if selector & 0x4 == 0 {
            install_descriptor(&mut vcpu, &memory, selector, &descriptor);
        }
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.gdt.limit = limit;
        sregs.tr = seeded_tr();
        let before_tr = segment_fingerprint(&sregs.tr);
        vcpu.set_sregs(&sregs).unwrap();
        let before_descriptor = if selector & 0x4 == 0 {
            Some(descriptor_bytes(&memory, selector, 16))
        } else {
            None
        };

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{name}: {error}"
        );
        assert_eq!(
            segment_fingerprint(&vcpu.get_sregs().unwrap().tr),
            before_tr,
            "{name}"
        );
        if let Some(before_descriptor) = before_descriptor {
            assert_eq!(
                descriptor_bytes(&memory, selector, 16),
                before_descriptor,
                "{name}"
            );
        }
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }
}

#[test]
fn ltr_mode_privilege_lock_and_operand_fault_priority_is_precise() {
    for (name, code, configure, vector) in [
        (
            "LOCK",
            vec![0xF0, 0x0F, 0x00, 0xD8],
            (false, false, false),
            6,
        ),
        ("real mode", vec![0x0F, 0x00, 0x18], (true, false, false), 6),
        ("VM86", vec![0x0F, 0x00, 0x18], (false, true, false), 6),
        ("CPL3", vec![0x0F, 0x00, 0x18], (false, false, true), 13),
    ] {
        let (mut vcpu, _) = setup_vm_no_idt(&code, None);
        let (real, vm86, cpl3) = configure;
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x20_00000;
        if vm86 {
            regs.rflags |= 1 << 17;
        }
        vcpu.set_regs(&regs).unwrap();
        let mut sregs = vcpu.get_sregs().unwrap();
        if real {
            sregs.cr0 &= !1;
            // Real mode uses a four-byte IVT entry with no present bit. Make
            // the no-handler condition architectural via the IDTR limit.
            sregs.idt.limit = 0;
        }
        if cpl3 {
            sregs.cs.selector |= 3;
        }
        sregs.tr = seeded_tr();
        let before = segment_fingerprint(&sregs.tr);
        vcpu.set_sregs(&sregs).unwrap();

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{name}: {error}"
        );
        assert_eq!(
            segment_fingerprint(&vcpu.get_sregs().unwrap().tr),
            before,
            "{name}"
        );
    }

    let (mut vcpu, _) = setup_vm_no_idt(&[0x0F, 0x00, 0x18], None);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0x20_00000;
    vcpu.set_regs(&regs).unwrap();
    let error = format!("{:#}", vcpu.step().expect_err("unmapped source must fault"));
    assert!(!error.contains("IDT entry 13 not present"), "{error}");
}

#[test]
fn ltr_compatibility_and_legacy_tss_types_are_exact() {
    let code = [0x0F, 0x00, 0xD8, 0xF4];
    for (name, ia32e_active, type_, busy_type) in [
        ("compatibility 32-bit", true, 0x9, 0xB),
        ("legacy 16-bit", false, 0x1, 0x3),
        ("legacy 32-bit", false, 0x9, 0xB),
    ] {
        let initial = Registers {
            rax: 0x10,
            ..Registers::default()
        };
        let (mut vcpu, memory) = if ia32e_active {
            setup_vm_compat(&code, Some(initial))
        } else {
            setup_vm(&code, Some(initial))
        };
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cs.l = false;
        if ia32e_active {
            sregs.efer |= 1 << 10;
        } else {
            sregs.efer &= !(1 << 10);
        }
        vcpu.set_sregs(&sregs).unwrap();
        let descriptor = tss_descriptor(0xDEAD_BEEF, 0xABCDE, 2, true, type_, true, true);
        install_descriptor(&mut vcpu, &memory, 0x10, &descriptor[..8]);

        run_until_hlt(&mut vcpu).unwrap();
        let tr = vcpu.get_sregs().unwrap().tr;
        assert_eq!(tr.selector, 0x10, "{name}");
        assert_eq!(tr.base, 0xDEAD_BEEF, "{name}");
        assert_eq!(tr.limit, 0xABCDEFFF, "{name}");
        assert_eq!(tr.type_, busy_type, "{name}");
        assert_eq!(
            descriptor_bytes(&memory, 0x10, 8)[5] & 0x0F,
            busy_type,
            "{name}"
        );
    }

    let initial = Registers {
        rax: 0x10,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_vm_no_idt(&code[..3], Some(initial));
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cs.l = false;
    sregs.efer |= 1 << 10;
    sregs.tr = seeded_tr();
    let before = segment_fingerprint(&sregs.tr);
    vcpu.set_sregs(&sregs).unwrap();
    let descriptor = tss_descriptor(0, 0x67, 0, true, 0x1, false, false);
    install_descriptor(&mut vcpu, &memory, 0x10, &descriptor[..8]);
    let error = exception_without_idt(&mut vcpu);
    assert!(error.contains("IDT entry 13 not present"), "{error}");
    assert_eq!(segment_fingerprint(&vcpu.get_sregs().unwrap().tr), before);
    assert_eq!(descriptor_bytes(&memory, 0x10, 8), descriptor[..8]);
}

#[test]
fn ltr_available_tss_type_matrix_is_exact_in_every_protected_execution_mode() {
    for (mode, cs_l, ia32e_active, descriptor_len, available_types) in [
        ("64-bit", true, true, 16_usize, &[0x9_u8][..]),
        ("compatibility", false, true, 8, &[0x9][..]),
        ("legacy", false, false, 8, &[0x1, 0x9][..]),
    ] {
        for type_ in 0_u8..16 {
            let initial = Registers {
                rax: 0x10,
                ..Registers::default()
            };
            let (mut vcpu, memory) = setup_vm_no_idt(&[0x0F, 0x00, 0xD8], Some(initial));
            let mut sregs = vcpu.get_sregs().unwrap();
            sregs.cs.l = cs_l;
            if ia32e_active {
                sregs.efer |= 1 << 10;
            } else {
                sregs.efer &= !(1 << 10);
            }
            sregs.tr = seeded_tr();
            let before_tr = segment_fingerprint(&sregs.tr);
            vcpu.set_sregs(&sregs).unwrap();
            let descriptor = tss_descriptor(0x1234_5000, 0x67, 0, true, type_, false, false);
            install_descriptor(&mut vcpu, &memory, 0x10, &descriptor[..descriptor_len]);
            let before_descriptor = descriptor_bytes(&memory, 0x10, descriptor_len);

            if available_types.contains(&type_) {
                assert!(
                    vcpu.step().expect("available TSS type must load").is_none(),
                    "mode={mode}, type={type_:#x}"
                );
                let tr = vcpu.get_sregs().unwrap().tr;
                assert_eq!(tr.selector, 0x10, "mode={mode}, type={type_:#x}");
                assert_eq!(tr.type_, type_ | 0x2, "mode={mode}, type={type_:#x}");
                assert_eq!(
                    descriptor_bytes(&memory, 0x10, descriptor_len)[5] & 0x0F,
                    type_ | 0x2,
                    "mode={mode}, type={type_:#x}"
                );
            } else {
                let error = exception_without_idt(&mut vcpu);
                assert!(
                    error.contains("IDT entry 13 not present"),
                    "mode={mode}, type={type_:#x}: {error}"
                );
                assert_eq!(
                    segment_fingerprint(&vcpu.get_sregs().unwrap().tr),
                    before_tr,
                    "mode={mode}, type={type_:#x}"
                );
                assert_eq!(
                    descriptor_bytes(&memory, 0x10, descriptor_len),
                    before_descriptor,
                    "mode={mode}, type={type_:#x}"
                );
            }
        }
    }
}

#[test]
fn ltr_reads_complete_long_descriptor_before_validation_and_never_partially_commits() {
    let initial = Registers {
        rax: 0x10,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_vm_no_idt(&[0x0F, 0x00, 0xD8], Some(initial));
    let descriptor = tss_descriptor(0, 0x67, 0, true, 0x2, false, false);
    let low_addr = 16 * 1024 * 1024 - 8;
    memory
        .write_slice(&descriptor[..8], GuestAddress(low_addr))
        .unwrap();
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.gdt.base = low_addr - 0x10;
    sregs.gdt.limit = 0x1F;
    sregs.tr = seeded_tr();
    let before = segment_fingerprint(&sregs.tr);
    vcpu.set_sregs(&sregs).unwrap();

    let error = format!(
        "{:#}",
        vcpu.step().expect_err("upper descriptor read must fault")
    );
    assert!(!error.contains("IDT entry 13 not present"), "{error}");
    assert_eq!(segment_fingerprint(&vcpu.get_sregs().unwrap().tr), before);
    let mut observed = [0_u8; 8];
    memory
        .read_slice(&mut observed, GuestAddress(low_addr))
        .unwrap();
    assert_eq!(observed, descriptor[..8]);
}
