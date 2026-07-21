use crate::common::*;
use rax::vm::vcpu::Segment;

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

fn install_pointer_and_descriptor(
    memory: &std::sync::Arc<vm_memory::GuestMemoryMmap>,
    offset: u64,
    width: usize,
    present: bool,
) {
    let mut pointer = offset.to_le_bytes()[..width].to_vec();
    pointer.extend_from_slice(&0x10_u16.to_le_bytes());
    memory
        .write_slice(&pointer, GuestAddress(DATA_ADDR))
        .unwrap();
    let descriptor = [
        0xFF,
        0xFF,
        0,
        0,
        0,
        if present { 0x92 } else { 0x12 },
        0xCF,
        0,
    ];
    memory
        .write_slice(&descriptor, GuestAddress(GDT_BASE + 0x10))
        .unwrap();
}

#[test]
fn lss_lfs_lgs_commit_exact_gpr_width_cache_accessed_bit_and_fault_frontier() {
    for (name, code, width, offset, expected, target) in [
        (
            "LFS m16:16",
            &[0x66, 0x0F, 0xB4, 0x0C, 0x25, 0x00, 0x20, 0x00, 0x00][..],
            2_usize,
            0x1234_BEEF_u64,
            0xA5A5_5A5A_DEAD_BEEF,
            6_u8,
        ),
        (
            "LGS m16:32",
            &[0x0F, 0xB5, 0x04, 0x25, 0x00, 0x20, 0x00, 0x00][..],
            4,
            0x1234_89AB_CDEF,
            0x89AB_CDEF,
            7,
        ),
        (
            "LSS m16:64",
            &[0x48, 0x0F, 0xB2, 0x14, 0x25, 0x00, 0x20, 0x00, 0x00][..],
            8,
            0x0123_4567_89AB_CDEF,
            0x0123_4567_89AB_CDEF,
            4,
        ),
    ] {
        let initial = Registers {
            rax: 0xA5A5_5A5A_DEAD_BEEF,
            rcx: 0xA5A5_5A5A_DEAD_BEEF,
            rdx: 0xA5A5_5A5A_DEAD_BEEF,
            rflags: 0x08D7,
            ..Registers::default()
        };
        let (mut vcpu, memory) = setup_vm_no_idt(code, Some(initial));
        install_pointer_and_descriptor(&memory, offset, width, true);
        vcpu.step()
            .unwrap_or_else(|error| panic!("{name}: {error}"));

        let regs = vcpu.get_regs().unwrap();
        let observed = match target {
            4 => regs.rdx,
            6 => regs.rcx,
            7 => regs.rax,
            _ => unreachable!(),
        };
        assert_eq!(observed, expected, "{name}");
        assert_eq!(regs.rip, CODE_ADDR + code.len() as u64, "{name}");
        assert_eq!(regs.rflags, 0x08D7, "{name}");
        let sregs = vcpu.get_sregs().unwrap();
        let segment = match target {
            4 => &sregs.ss,
            6 => &sregs.fs,
            7 => &sregs.gs,
            _ => unreachable!(),
        };
        assert_eq!(segment.selector, 0x10, "{name}");
        assert_eq!(segment.limit, u32::MAX, "{name}");
        assert_eq!(segment.type_, 0x3, "{name}");
        assert!(segment.present && !segment.unusable, "{name}");
        let mut descriptor = [0_u8; 8];
        memory
            .read_slice(&mut descriptor, GuestAddress(GDT_BASE + 0x10))
            .unwrap();
        assert_eq!(descriptor[5], 0x93, "{name}: accessed bit");
    }
}

#[test]
fn lfs_rex2_extends_both_destination_and_address_and_checks_apx_dynamically() {
    // REX2 map 1, W=1, R4=R3=1, B4=1: LFS R31,m16:64 [R16].
    let code = [0xD5, 0xDC, 0xB4, 0x38];
    let initial = Registers {
        r16: DATA_ADDR,
        r31: 0x3131_3131_3131_3131,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_vm_no_idt(&code, Some(initial.clone()));
    install_pointer_and_descriptor(&memory, 0x0123_4567_89AB_CDEF, 8, true);

    let error = vcpu
        .step()
        .expect_err("disabled APX must raise #UD")
        .to_string();
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(vcpu.get_regs().unwrap().r31, initial.r31);

    vcpu.set_apx_enabled(true);
    vcpu.step().expect("enabled APX LFS R31,[R16]");
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.r31, 0x0123_4567_89AB_CDEF);
    assert_eq!(regs.r16, DATA_ADDR);
    assert_eq!(regs.rip, CODE_ADDR + code.len() as u64);
    assert_eq!(vcpu.get_sregs().unwrap().fs.selector, 0x10);
}

#[test]
fn lss_lfs_lgs_invalid_register_lock_and_prefix_order_forms_are_ud_noncommitting() {
    for (name, code, apx) in [
        ("LSS register", &[0x0F, 0xB2, 0xC1][..], false),
        ("LFS register", &[0x0F, 0xB4, 0xC1], false),
        ("LGS register", &[0x0F, 0xB5, 0xC1], false),
        ("LOCK LFS", &[0xF0, 0x0F, 0xB4, 0x08], false),
        ("REX before REX2", &[0x48, 0xD5, 0x80, 0xB4, 0x08], true),
    ] {
        let initial = Registers {
            rax: DATA_ADDR,
            rcx: 0xA5A5_5A5A_DEAD_BEEF,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_no_idt(code, Some(initial.clone()));
        vcpu.set_apx_enabled(apx);
        let before_fs = vcpu.get_sregs().unwrap().fs;
        let error = vcpu
            .step()
            .expect_err("invalid far-pointer load must #UD")
            .to_string();
        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rip, CODE_ADDR, "{name}");
        assert_eq!(regs.rcx, initial.rcx, "{name}");
        assert_eq!(
            segment_fingerprint(&vcpu.get_sregs().unwrap().fs),
            segment_fingerprint(&before_fs),
            "{name}"
        );
    }
}

#[test]
fn far_pointer_full_range_canonical_fault_uses_effective_segment_and_does_not_commit() {
    for (name, code, source, vector) in [
        (
            "LFS DS-based #GP",
            &[0x48, 0x0F, 0xB4, 0x08][..],
            0x0000_7FFF_FFFF_FFF7_u64,
            13_u8,
        ),
        (
            "LSS SS-based #SS",
            &[0x48, 0x0F, 0xB2, 0x0C, 0x24][..],
            u64::MAX - 8,
            12,
        ),
    ] {
        let initial = Registers {
            rax: source,
            rsp: source,
            rcx: 0xA5A5_5A5A_DEAD_BEEF,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_no_idt(code, Some(initial.clone()));
        let before = vcpu.get_sregs().unwrap();
        let error = vcpu
            .step()
            .expect_err("noncanonical far pointer must fault")
            .to_string();
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{name}: {error}"
        );
        let regs = vcpu.get_regs().unwrap();
        assert_eq!(regs.rip, CODE_ADDR, "{name}");
        assert_eq!(regs.rcx, initial.rcx, "{name}");
        let after = vcpu.get_sregs().unwrap();
        assert_eq!(
            segment_fingerprint(&after.fs),
            segment_fingerprint(&before.fs),
            "{name}"
        );
        assert_eq!(
            segment_fingerprint(&after.ss),
            segment_fingerprint(&before.ss),
            "{name}"
        );
    }
}

#[test]
fn lfs_descriptor_fault_does_not_commit_gpr_selector_or_cache() {
    // REX.W LFS RAX,m16:64 [RIP + 0x0ff8] -> DATA_ADDR.
    let code = [0x48, 0x0F, 0xB4, 0x05, 0xF8, 0x0F, 0x00, 0x00];
    let initial = Registers {
        rax: 0xA5A5_5A5A_DEAD_BEEF,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_vm_no_idt(&code, Some(initial.clone()));
    memory
        .write_slice(
            &0x0123_4567_89AB_CDEF_u64.to_le_bytes(),
            GuestAddress(DATA_ADDR),
        )
        .unwrap();
    memory
        .write_slice(&0x10_u16.to_le_bytes(), GuestAddress(DATA_ADDR + 8))
        .unwrap();
    // Writable data descriptor with P=0: the architectural result is #NP(0x10).
    memory
        .write_slice(
            &[0xFF, 0xFF, 0, 0, 0, 0x12, 0xCF, 0],
            GuestAddress(GDT_BASE + 0x10),
        )
        .unwrap();
    let before_fs = vcpu.get_sregs().unwrap().fs;

    let error = vcpu
        .step()
        .expect_err("LFS must validate the selected descriptor before committing")
        .to_string();
    assert!(error.contains("IDT entry 11 not present"), "{error}");
    let observed = vcpu.get_regs().unwrap();
    assert_eq!(observed.rax, initial.rax);
    assert_eq!(observed.rip, CODE_ADDR);
    assert_eq!(
        segment_fingerprint(&vcpu.get_sregs().unwrap().fs),
        segment_fingerprint(&before_fs)
    );
}
