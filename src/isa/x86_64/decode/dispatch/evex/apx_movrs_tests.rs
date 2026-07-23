//! Direct-execution regressions for legacy and APX-promoted MOVRS.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DATA_ADDRESS: u64 = 0x2000;
const STATUS_MASK: u64 = 0x08D5;
const INITIAL_RFLAGS: u64 = 0x2 | STATUS_MASK | flags::bits::DF;
const INITIAL_DESTINATION: u64 = 0xA1B2_C3D4_E5F6_7788;
const SOURCE: u64 = 0x0123_4567_89AB_CDEF;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rflags = INITIAL_RFLAGS;
    vcpu.set_apx_enabled(true);
    vcpu
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

fn width_mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1_u64 << (u32::from(bytes) * 8)) - 1
    }
}

fn merge_gpr(old: u64, value: u64, bytes: u8) -> u64 {
    match bytes {
        1 | 2 => (old & !width_mask(bytes)) | (value & width_mask(bytes)),
        4 => value as u32 as u64,
        8 => value,
        _ => unreachable!(),
    }
}

fn legacy_encoding(bytes: u8) -> Vec<u8> {
    match bytes {
        1 => vec![0x44, 0x0F, 0x38, 0x8A, 0x03],
        2 => vec![0x66, 0x44, 0x0F, 0x38, 0x8B, 0x03],
        4 => vec![0x44, 0x0F, 0x38, 0x8B, 0x03],
        8 => vec![0x4C, 0x0F, 0x38, 0x8B, 0x03],
        _ => unreachable!(),
    }
}

fn apx_encoding(bytes: u8) -> Vec<u8> {
    let (p1, opcode) = match bytes {
        1 => (0x78, 0x8A),
        2 => (0x79, 0x8B),
        4 => (0x78, 0x8B),
        8 => (0xF8, 0x8B),
        _ => unreachable!(),
    };
    vec![0x62, 0xEC, p1, 0x08, opcode, 0x44, 0x91, 0x20]
}

#[test]
fn direct_legacy_movrs_covers_every_width_partial_write_and_flags() {
    for bytes in [1, 2, 4, 8] {
        let code = legacy_encoding(bytes);
        let memory = memory_with_code(&code);
        memory
            .write_slice(
                &SOURCE.to_le_bytes()[..usize::from(bytes)],
                GuestAddress(DATA_ADDRESS),
            )
            .unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.r8 = INITIAL_DESTINATION;
        vcpu.regs.rbx = DATA_ADDRESS;

        assert!(vcpu.step().expect("legacy MOVRS").is_none());
        assert_eq!(
            vcpu.regs.r8,
            merge_gpr(INITIAL_DESTINATION, SOURCE, bytes),
            "width={bytes}"
        );
        assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS, "width={bytes}");
        assert_eq!(vcpu.regs.rip, code.len() as u64, "width={bytes}");
    }

    // REX.W takes precedence over a redundant 66 operand-size prefix.
    let code = [0x66, 0x4C, 0x0F, 0x38, 0x8B, 0x03];
    let memory = memory_with_code(&code);
    memory
        .write_slice(&SOURCE.to_le_bytes(), GuestAddress(DATA_ADDRESS))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.r8 = INITIAL_DESTINATION;
    vcpu.regs.rbx = DATA_ADDRESS;
    assert!(vcpu.step().expect("REX.W+66 MOVRS").is_none());
    assert_eq!(vcpu.regs.r8, SOURCE);
    assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);
}

#[test]
fn direct_legacy_movrs_distinguishes_high_bytes_from_rex_low_bytes() {
    // MOVRS AH,[RBX].
    let code = [0x0F, 0x38, 0x8A, 0x23];
    let memory = memory_with_code(&code);
    memory
        .write_slice(&[0x5A], GuestAddress(DATA_ADDRESS))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = INITIAL_DESTINATION;
    vcpu.regs.rbx = DATA_ADDRESS;
    vcpu.regs.rsp = 0x1122_3344_5566_7788;
    assert!(vcpu.step().expect("MOVRS AH").is_none());
    assert_eq!(vcpu.regs.rax, (INITIAL_DESTINATION & !0xFF00) | 0x5A00);
    assert_eq!(vcpu.regs.rsp, 0x1122_3344_5566_7788);

    // The same ModR/M.reg field under any REX prefix denotes SPL.
    let code = [0x40, 0x0F, 0x38, 0x8A, 0x23];
    let memory = memory_with_code(&code);
    memory
        .write_slice(&[0xA5], GuestAddress(DATA_ADDRESS))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = INITIAL_DESTINATION;
    vcpu.regs.rbx = DATA_ADDRESS;
    vcpu.regs.rsp = 0x1122_3344_5566_7788;
    assert!(vcpu.step().expect("MOVRS SPL").is_none());
    assert_eq!(vcpu.regs.rax, INITIAL_DESTINATION);
    assert_eq!(vcpu.regs.rsp, 0x1122_3344_5566_77A5);
    assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);
}

#[test]
fn direct_apx_movrs_covers_every_width_egpr_addressing_and_flags() {
    for bytes in [1, 2, 4, 8] {
        let code = apx_encoding(bytes);
        let memory = memory_with_code(&code);
        memory
            .write_slice(
                &SOURCE.to_le_bytes()[..usize::from(bytes)],
                GuestAddress(DATA_ADDRESS),
            )
            .unwrap();
        let mut vcpu = test_vcpu(memory);
        vcpu.regs.r16 = INITIAL_DESTINATION;
        vcpu.regs.r17 = DATA_ADDRESS - 0x20;
        vcpu.regs.r18 = 0;

        assert!(vcpu.step().expect("APX MOVRS").is_none());
        assert_eq!(
            vcpu.regs.r16,
            merge_gpr(INITIAL_DESTINATION, SOURCE, bytes),
            "width={bytes}"
        );
        assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS, "width={bytes}");
        assert_eq!(vcpu.regs.rip, code.len() as u64, "width={bytes}");
    }

    // The APX scalable form likewise gives W=1 precedence over pp=66.
    let code = [0x62, 0xEC, 0xF9, 0x08, 0x8B, 0x44, 0x91, 0x20];
    let memory = memory_with_code(&code);
    memory
        .write_slice(&SOURCE.to_le_bytes(), GuestAddress(DATA_ADDRESS))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.r16 = INITIAL_DESTINATION;
    vcpu.regs.r17 = DATA_ADDRESS - 0x20;
    vcpu.regs.r18 = 0;
    assert!(vcpu.step().expect("APX W1+66 MOVRS").is_none());
    assert_eq!(vcpu.regs.r16, SOURCE);
    assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);
}

#[test]
fn direct_apx_movrs_honors_fs_addr32_and_egpr_sib_extensions() {
    // FS MOVRS R16,[R17D+R18D*4+32]. Address-size truncation discards the
    // high half of R17 before the FS base is applied.
    let code = [0x64, 0x67, 0x62, 0xEC, 0xF8, 0x08, 0x8B, 0x44, 0x91, 0x20];
    let memory = memory_with_code(&code);
    memory
        .write_slice(&SOURCE.to_le_bytes(), GuestAddress(DATA_ADDRESS))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.sregs.fs.base = 0x1000;
    vcpu.regs.r17 = 0xFFFF_FFFF_0000_0BE0;
    vcpu.regs.r18 = 0x100;
    vcpu.regs.r16 = INITIAL_DESTINATION;

    assert!(vcpu.step().expect("FS addr32 APX MOVRS").is_none());
    assert_eq!(vcpu.regs.r16, SOURCE);
    assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS);
}

fn assert_ud(code: &[u8], name: &str) {
    let mut vcpu = test_vcpu(memory_with_code(code));
    vcpu.regs.rax = SOURCE;
    vcpu.regs.rbx = DATA_ADDRESS;
    vcpu.regs.r16 = INITIAL_DESTINATION;
    vcpu.regs.r17 = DATA_ADDRESS - 0x20;
    let before = register_image(&vcpu);
    let error = format!("{:#}", vcpu.step().expect_err(name));
    assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
    assert_eq!(register_image(&vcpu), before, "{name}");
    assert_eq!(vcpu.regs.rip, 0, "{name} RIP");
}

#[test]
fn direct_movrs_reserved_forms_fault_without_commit() {
    for (code, name) in [
        (&[0xF0, 0x0F, 0x38, 0x8B, 0x03][..], "legacy LOCK"),
        (&[0xF2, 0x0F, 0x38, 0x8B, 0x03][..], "legacy F2"),
        (&[0xF3, 0x0F, 0x38, 0x8B, 0x03][..], "legacy F3"),
        (&[0x44, 0x0F, 0x38, 0x8B, 0xC3][..], "legacy register"),
        (
            &[0x62, 0xEC, 0x7A, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "APX F3 pp",
        ),
        (
            &[0x62, 0xEC, 0x7B, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "APX F2 pp",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x18, 0x8B, 0x44, 0x91, 0x20][..],
            "APX ND",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x0C, 0x8B, 0x44, 0x91, 0x20][..],
            "APX NF",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x88, 0x8B, 0x44, 0x91, 0x20][..],
            "APX z",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x28, 0x8B, 0x44, 0x91, 0x20][..],
            "APX LL",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x48, 0x8B, 0x44, 0x91, 0x20][..],
            "APX L-prime",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x09, 0x8B, 0x44, 0x91, 0x20][..],
            "APX payload 0",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x0A, 0x8B, 0x44, 0x91, 0x20][..],
            "APX payload 1",
        ),
        (
            &[0x62, 0xEC, 0x38, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "APX V3:0",
        ),
        (
            &[0x62, 0xEC, 0x78, 0x00, 0x8B, 0x44, 0x91, 0x20][..],
            "APX V4",
        ),
        (
            &[0x62, 0xEC, 0xF8, 0x08, 0x8A, 0x44, 0x91, 0x20][..],
            "APX byte W",
        ),
        (
            &[0x62, 0xEC, 0x79, 0x08, 0x8A, 0x44, 0x91, 0x20][..],
            "APX byte 66",
        ),
        (&[0x62, 0xEC, 0x78, 0x08, 0x8B, 0xC0][..], "APX register"),
        (
            &[0x66, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "leading 66",
        ),
        (
            &[0xF0, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "leading LOCK",
        ),
        (
            &[0xF2, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "leading F2",
        ),
        (
            &[0xF3, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "leading F3",
        ),
        (
            &[0x48, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20][..],
            "leading REX",
        ),
    ] {
        assert_ud(code, name);
    }

    let code = [0x0F, 0x38, 0x8B, 0x03];
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.sregs.cs.l = false;
    vcpu.regs.rbx = DATA_ADDRESS;
    let before = register_image(&vcpu);
    let error = format!("{:#}", vcpu.step().expect_err("non-64-bit MOVRS"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(register_image(&vcpu), before);
}

#[test]
fn direct_apx_movrs_feature_and_memory_faults_are_precise_and_noncommitting() {
    let code = apx_encoding(8);
    let memory = memory_with_code(&code);

    let mut disabled = test_vcpu(memory.clone());
    disabled.set_apx_enabled(false);
    disabled.regs.r16 = INITIAL_DESTINATION;
    disabled.regs.r17 = 0x2_0000;
    disabled.regs.r18 = 0;
    let before = register_image(&disabled);
    let error = format!("{:#}", disabled.step().expect_err("APX disabled"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(register_image(&disabled), before);

    let mut enabled = test_vcpu(memory);
    enabled.regs.r16 = INITIAL_DESTINATION;
    enabled.regs.r17 = 0x2_0000;
    enabled.regs.r18 = 0;
    let before = register_image(&enabled);
    assert!(enabled.step().is_err());
    assert_eq!(register_image(&enabled), before);
    assert_eq!(enabled.regs.rip, 0);

    let code = legacy_encoding(8);
    let mut legacy = test_vcpu(memory_with_code(&code));
    legacy.regs.r8 = INITIAL_DESTINATION;
    legacy.regs.rbx = 0x2_0000;
    let before = register_image(&legacy);
    assert!(legacy.step().is_err());
    assert_eq!(register_image(&legacy), before);
    assert_eq!(legacy.regs.rip, 0);
}
