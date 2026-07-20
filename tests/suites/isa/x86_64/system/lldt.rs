//! Architectural LLDT coverage: operand decode, GDT validation, hidden state,
//! fault priority, and non-commit behavior.

use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::{Registers, Segment, VCpu};

use crate::common::{
    Bytes, CODE_ADDR, DATA_ADDR, GDT_BASE, GuestAddress, GuestMemoryMmap, run_until_hlt,
    setup_apx_vm, setup_vm, setup_vm_compat, setup_vm_no_idt, write_mem_at_u16,
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
        16 => regs.r16,
        17 => regs.r17,
        18 => regs.r18,
        19 => regs.r19,
        20 => regs.r20,
        21 => regs.r21,
        22 => regs.r22,
        23 => regs.r23,
        24 => regs.r24,
        25 => regs.r25,
        26 => regs.r26,
        27 => regs.r27,
        28 => regs.r28,
        29 => regs.r29,
        30 => regs.r30,
        31 => regs.r31,
        _ => unreachable!(),
    }
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
        &ldt_descriptor(0x1234_5678, 0x0FFFF, 0, true, false, false),
    );
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

fn segment_fingerprint(segment: &Segment) -> (u64, u32, u16, u8, bool, u8, bool, bool, bool) {
    (
        segment.base,
        segment.limit,
        segment.selector,
        segment.type_,
        segment.present,
        segment.dpl,
        segment.g,
        segment.avl,
        segment.unusable,
    )
}

#[test]
fn lldt_register_forms_cover_every_legacy_and_rex_source_and_ignore_operand_size() {
    for index in 0_u8..16 {
        for operand_prefix in [None, Some(0x66), Some(0x48)] {
            let mut code = Vec::new();
            if let Some(prefix) = operand_prefix {
                code.push(prefix);
            }
            if index >= 8 {
                code.push(0x41); // REX.B
            }
            code.extend_from_slice(&[0x0F, 0x00, 0xD0 | (index & 7), 0xF4]);

            let source = 0xA5A5_5A5A_0000_0010;
            let mut initial = Registers::default();
            set_gpr(&mut initial, index, source);
            let (mut vcpu, memory) = setup_vm(&code, Some(initial));
            install_valid_descriptor(&mut vcpu, &memory);

            let regs = run_until_hlt(&mut vcpu).unwrap();
            assert_eq!(
                get_gpr(&regs, index),
                source,
                "index={index}, prefix={operand_prefix:?}"
            );
            assert_eq!(vcpu.get_sregs().unwrap().ldt.selector, 0x10);
        }
    }
}

#[test]
fn lldt_rex2_reads_egpr_and_requires_apx() {
    let code = [0xD5, 0x91, 0x00, 0xD7, 0xF4]; // LLDT R31W; HLT
    let mut initial = Registers::default();
    initial.r31 = 0x3131_3131_0000_0010;
    let (mut vcpu, memory) = setup_apx_vm(&code, Some(initial.clone()));
    install_valid_descriptor(&mut vcpu, &memory);
    let regs = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(regs.r31, 0x3131_3131_0000_0010);
    assert_eq!(vcpu.get_sregs().unwrap().ldt.selector, 0x10);

    let (mut disabled, _) = setup_vm_no_idt(&code, Some(initial));
    let error = exception_without_idt(&mut disabled);
    assert!(error.contains("IDT entry 6 not present"), "{error}");
}

#[test]
fn lldt_memory_address_forms_read_exactly_two_source_bytes() {
    for index in 0_u8..16 {
        let mut code = Vec::new();
        if index >= 8 {
            code.push(0x41); // REX.B
        }
        match index & 7 {
            4 => code.extend_from_slice(&[0x0F, 0x00, 0x14, 0x24]), // [RSP/R12]
            5 => code.extend_from_slice(&[0x0F, 0x00, 0x55, 0x00]), // [RBP/R13+0]
            rm => code.extend_from_slice(&[0x0F, 0x00, 0x10 | rm]),
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
        assert_eq!(
            vcpu.get_sregs().unwrap().ldt.selector,
            0x10,
            "index={index}"
        );
        assert_eq!(get_gpr(&regs, index), DATA_ADDR, "index={index}");
    }

    for (name, code, configure, source_addr) in [
        (
            "absolute SIB",
            vec![0x0F, 0x00, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00, 0xF4],
            None,
            DATA_ADDR,
        ),
        (
            "RAX",
            vec![0x0F, 0x00, 0x10, 0xF4],
            Some((0_u8, DATA_ADDR)),
            DATA_ADDR,
        ),
        (
            "RSP+RCX*2+4",
            vec![0x0F, 0x00, 0x54, 0x4C, 0x04, 0xF4],
            Some((4_u8, DATA_ADDR - 0x24)),
            DATA_ADDR,
        ),
        (
            "RIP relative",
            vec![0x0F, 0x00, 0x15, 0xF9, 0x0F, 0x00, 0x00, 0xF4],
            None,
            DATA_ADDR,
        ),
        (
            "addr32 EIP relative",
            vec![0x67, 0x0F, 0x00, 0x15, 0xF8, 0x0F, 0x00, 0x00, 0xF4],
            None,
            DATA_ADDR,
        ),
        (
            "addr32 absolute",
            vec![0x67, 0x0F, 0x00, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00, 0xF4],
            None,
            DATA_ADDR,
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
            .write_slice(&[0x10, 0x00, 0xA5], GuestAddress(source_addr))
            .unwrap();

        run_until_hlt(&mut vcpu).unwrap();
        assert_eq!(vcpu.get_sregs().unwrap().ldt.selector, 0x10, "{name}");
        let mut source = [0_u8; 3];
        memory
            .read_slice(&mut source, GuestAddress(source_addr))
            .unwrap();
        assert_eq!(source, [0x10, 0x00, 0xA5], "{name}");
    }
}

#[test]
fn lldt_loads_complete_hidden_descriptor_and_preserves_rflags_and_source() {
    let code = [0x0F, 0x00, 0xD0, 0xF4]; // LLDT AX; HLT
    let base = 0xFFFF_8000_1234_5000;
    let raw_limit = 0xA_BCDE;
    let mut initial = Registers {
        rax: 0xA5A5_5A5A_0000_0013,
        rflags: 0x0CD7,
        ..Registers::default()
    };
    initial.rsp = 0x8000;
    let (mut vcpu, memory) = setup_vm(&code, Some(initial));
    install_descriptor(
        &mut vcpu,
        &memory,
        0x13,
        &ldt_descriptor(base, raw_limit, 3, true, true, true),
    );

    let before_flags = vcpu.get_regs().unwrap().rflags;
    let regs = run_until_hlt(&mut vcpu).unwrap();
    let ldtr = vcpu.get_sregs().unwrap().ldt;
    assert_eq!(regs.rax, 0xA5A5_5A5A_0000_0013);
    assert_eq!(regs.rflags, before_flags);
    assert_eq!(ldtr.selector, 0x13);
    assert_eq!(ldtr.base, base);
    assert_eq!(ldtr.limit, (raw_limit << 12) | 0xFFF);
    assert_eq!(ldtr.type_, 0x2);
    assert!(ldtr.present);
    assert_eq!(ldtr.dpl, 3);
    assert!(ldtr.g);
    assert!(ldtr.avl);
    assert!(!ldtr.s);
    assert!(!ldtr.unusable);
}

#[test]
fn lldt_null_selector_values_invalidate_ldtr_without_gdt_access() {
    for selector in 0_u64..=3 {
        let code = [0x0F, 0x00, 0xD0, 0xF4];
        let initial = Registers {
            rax: selector,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm(&code, Some(initial));
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.gdt.base = u64::MAX;
        sregs.gdt.limit = 0;
        sregs.ldt = Segment {
            base: 0xDEAD_BEEF,
            selector: 0x2468,
            present: true,
            ..Segment::default()
        };
        vcpu.set_sregs(&sregs).unwrap();

        run_until_hlt(&mut vcpu).unwrap();
        let ldtr = vcpu.get_sregs().unwrap().ldt;
        assert_eq!(u64::from(ldtr.selector), selector);
        assert_eq!(ldtr.base, 0);
        assert!(!ldtr.present);
        assert!(ldtr.unusable);
    }
}

#[test]
fn lldt_descriptor_faults_select_exact_vectors_and_do_not_commit() {
    for (name, selector, limit, descriptor, vector) in [
        (
            "TI",
            0x14,
            0x1F,
            ldt_descriptor(0, 0, 0, true, false, false),
            13,
        ),
        (
            "limit",
            0x10,
            0x1E,
            ldt_descriptor(0, 0, 0, true, false, false),
            13,
        ),
        (
            "wrong type",
            0x10,
            0x1F,
            {
                let mut value = ldt_descriptor(0, 0, 0, true, false, false);
                value[5] = (value[5] & 0xF0) | 0x9;
                value
            },
            13,
        ),
        (
            "not present",
            0x10,
            0x1F,
            ldt_descriptor(0, 0, 0, false, false, false),
            11,
        ),
        (
            "noncanonical base",
            0x10,
            0x1F,
            ldt_descriptor(0x0000_8000_0000_0000, 0, 0, true, false, false),
            13,
        ),
        (
            "reserved high",
            0x10,
            0x1F,
            {
                let mut value = ldt_descriptor(0, 0, 0, true, false, false);
                value[12] = 1;
                value
            },
            13,
        ),
        (
            "reserved L",
            0x10,
            0x1F,
            {
                let mut value = ldt_descriptor(0, 0, 0, true, false, false);
                value[6] |= 0x20;
                value
            },
            13,
        ),
    ] {
        let code = [0x0F, 0x00, 0xD0, 0xF4];
        let initial = Registers {
            rax: u64::from(selector),
            ..Registers::default()
        };
        let (mut vcpu, memory) = setup_vm_no_idt(&code, Some(initial));
        if selector & 0x4 == 0 {
            install_descriptor(&mut vcpu, &memory, selector, &descriptor);
        }
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.gdt.limit = limit;
        sregs.ldt = Segment {
            base: 0xDEAD_BEEF,
            limit: 0x1234,
            selector: 0x2468,
            type_: 0x2,
            present: true,
            unusable: false,
            ..Segment::default()
        };
        let before = segment_fingerprint(&sregs.ldt);
        vcpu.set_sregs(&sregs).unwrap();

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{name}: {error}"
        );
        assert_eq!(
            segment_fingerprint(&vcpu.get_sregs().unwrap().ldt),
            before,
            "{name}"
        );
        assert_eq!(vcpu.get_regs().unwrap().rip, CODE_ADDR, "{name}");
    }
}

#[test]
fn lldt_mode_privilege_lock_and_operand_fault_priority_is_precise() {
    for (name, code, configure, vector) in [
        (
            "LOCK",
            vec![0xF0, 0x0F, 0x00, 0xD0],
            (false, false, false),
            6,
        ),
        ("real mode", vec![0x0F, 0x00, 0x10], (true, false, false), 6),
        ("VM86", vec![0x0F, 0x00, 0x10], (false, true, false), 6),
        ("CPL3", vec![0x0F, 0x00, 0x10], (false, false, true), 13),
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
        }
        if cpl3 {
            sregs.cs.selector |= 3;
        }
        vcpu.set_sregs(&sregs).unwrap();

        let error = exception_without_idt(&mut vcpu);
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{name}: {error}"
        );
    }

    let code = [0x0F, 0x00, 0x10];
    let (mut vcpu, _) = setup_vm_no_idt(&code, None);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0x20_00000;
    vcpu.set_regs(&regs).unwrap();
    let error = format!("{:#}", vcpu.step().expect_err("unmapped source must fault"));
    assert!(!error.contains("IDT entry 13 not present"), "{error}");
}

#[test]
fn lldt_compatibility_mode_uses_legacy_eight_byte_descriptor() {
    let code = [0x0F, 0x00, 0xD0, 0xF4];
    let initial = Registers {
        rax: 0x10,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_vm_compat(&code, Some(initial));
    let descriptor = ldt_descriptor(0x1234_5678, 0xF_FFFF, 0, true, true, false);
    install_descriptor(&mut vcpu, &memory, 0x10, &descriptor[..8]);

    run_until_hlt(&mut vcpu).unwrap();
    let ldtr = vcpu.get_sregs().unwrap().ldt;
    assert_eq!(ldtr.selector, 0x10);
    assert_eq!(ldtr.base, 0x1234_5678);
    assert_eq!(ldtr.limit, u32::MAX);
}

#[test]
fn lldt_memory_source_helper_writes_and_reads_are_independent() {
    let code = [0x0F, 0x00, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00, 0xF4];
    let (mut vcpu, memory) = setup_vm(&code, None);
    install_valid_descriptor(&mut vcpu, &memory);
    write_mem_at_u16(&memory, DATA_ADDR, 0x10);
    run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(vcpu.get_sregs().unwrap().ldt.selector, 0x10);
}
