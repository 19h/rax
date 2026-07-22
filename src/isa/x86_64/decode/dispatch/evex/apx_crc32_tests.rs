//! Direct/native regressions for APX-promoted CRC32.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VCpu;
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
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_apx_enabled(true);
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    {
        vcpu.set_jit_mem(true);
        vcpu.set_jit_call(false);
    }
    vcpu
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

fn reference_crc32c(mut crc: u32, data: u64, width: u8) -> u32 {
    const POLY_REFLECTED: u32 = 0x82F6_3B78;
    for byte in data.to_le_bytes().into_iter().take(width as usize) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ POLY_REFLECTED
            };
        }
    }
    crc
}

#[test]
fn direct_apx_crc32_covers_scalable_widths_egprs_and_redundant_66() {
    let cases = [
        (&[0x62, 0xEC, 0x7C, 0x08, 0xF0, 0xE1][..], 1_u8, "r20d,r17b"),
        (&[0x62, 0xEC, 0xFC, 0x08, 0xF0, 0xE1][..], 1, "r20,r17b"),
        (&[0x62, 0xEC, 0x7D, 0x08, 0xF1, 0xE1][..], 2, "r20d,r17w"),
        (&[0x62, 0xEC, 0x7C, 0x08, 0xF1, 0xE1][..], 4, "r20d,r17d"),
        (&[0x62, 0xEC, 0xFC, 0x08, 0xF1, 0xE1][..], 8, "r20,r17"),
        // W=1 selects OSIZE64 regardless of the otherwise legal 66 pp value.
        (&[0x62, 0xEC, 0xFD, 0x08, 0xF1, 0xE1][..], 8, "r20,r17 (66)"),
    ];

    for (code, width, name) in cases {
        let mut vcpu = test_vcpu(memory_with_code(code));
        vcpu.regs.r20 = 0xFFFF_FFFF_1020_3040;
        vcpu.regs.r17 = 0x0123_4567_89AB_CDEF;
        let source = vcpu.regs.r17;
        let rflags = vcpu.regs.rflags;

        assert!(
            vcpu.step()
                .unwrap_or_else(|error| panic!("{name}: {error:#}"))
                .is_none()
        );

        assert_eq!(
            vcpu.regs.r20,
            u64::from(reference_crc32c(0x1020_3040, source, width)),
            "{name}"
        );
        assert_eq!(vcpu.regs.r17, source, "{name} source preservation");
        assert_eq!(vcpu.regs.rflags, rflags, "{name} RFLAGS");
        assert_eq!(vcpu.regs.rip, code.len() as u64, "{name} RIP");
    }
}

#[test]
fn direct_apx_crc32_byte_register_codes_exclude_legacy_high_bytes() {
    // ModR/M.rm=4 denotes SPL, not AH, under extended EVEX.
    let code = [0x62, 0xF4, 0x7C, 0x08, 0xF0, 0xC4];
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.regs.rax = 0xFFFF_FFFF_1234_A540;
    vcpu.regs.rsp = 0x9000_005A;

    assert!(vcpu.step().expect("APX CRC32 EAX,SPL").is_none());
    assert_eq!(
        vcpu.regs.rax,
        u64::from(reference_crc32c(0x1234_A540, 0x5A, 1))
    );
}

#[test]
fn direct_apx_crc32_memory_uses_fs_and_addr32_egpr_addresses() {
    let fs_code = [0x64, 0x62, 0xEC, 0xF8, 0x08, 0xF1, 0x64, 0x91, 0x20];
    let memory = memory_with_code(&fs_code);
    let fs_data = 0x0123_4567_89AB_CDEF_u64;
    memory
        .write_slice(&fs_data.to_le_bytes(), GuestAddress(0x412C))
        .unwrap();
    let mut fs = test_vcpu(memory);
    fs.sregs.fs.base = 0x4000;
    fs.regs.r17 = 0x100;
    fs.regs.r18 = 3;
    fs.regs.r20 = 0xFFFF_FFFF_1020_3040;

    assert!(fs.step().expect("APX CRC32 FS EGPR SIB").is_none());
    assert_eq!(
        fs.regs.r20,
        u64::from(reference_crc32c(0x1020_3040, fs_data, 8))
    );

    let addr32_code = [0x67, 0x62, 0x14, 0x7C, 0x08, 0xF1, 0x64, 0x91, 0x20];
    let memory = memory_with_code(&addr32_code);
    let addr32_data = 0xDEAD_BEEF_u32;
    memory
        .write_slice(&addr32_data.to_le_bytes(), GuestAddress(0x202C))
        .unwrap();
    let mut addr32 = test_vcpu(memory);
    addr32.regs.r9 = 0xFFFF_FFFF_0000_2000;
    addr32.regs.r10 = 0xFFFF_FFFF_0000_0003;
    addr32.regs.r12 = 0xFFFF_FFFF_5060_7080;

    assert!(addr32.step().expect("APX CRC32 addr32 EGPR SIB").is_none());
    assert_eq!(
        addr32.regs.r12,
        u64::from(reference_crc32c(0x5060_7080, u64::from(addr32_data), 4))
    );
}

#[test]
fn direct_apx_crc32_invalid_fields_and_disabled_apx_fault_without_commit() {
    let invalid = [
        (&[0x62, 0xF4, 0x7D, 0x08, 0xF0, 0xC1][..], "F0 with 66"),
        (&[0x62, 0xF4, 0x7E, 0x08, 0xF1, 0xC1][..], "F3 pp"),
        (&[0x62, 0xF4, 0x7F, 0x08, 0xF1, 0xC1][..], "F2 pp"),
        (&[0x62, 0xF4, 0x7C, 0x18, 0xF1, 0xC1][..], "ND"),
        (&[0x62, 0xF4, 0x7C, 0x0C, 0xF1, 0xC1][..], "NF"),
        (&[0x62, 0xF4, 0x7C, 0x88, 0xF1, 0xC1][..], "z"),
        (&[0x62, 0xF4, 0x7C, 0x28, 0xF1, 0xC1][..], "LL"),
        (&[0x62, 0xF4, 0x7C, 0x09, 0xF1, 0xC1][..], "aaa"),
        (&[0x62, 0xF4, 0x74, 0x08, 0xF1, 0xC1][..], "V3:0"),
        (&[0x62, 0xF4, 0x7C, 0x00, 0xF1, 0xC1][..], "V4"),
        (&[0x62, 0xF4, 0x78, 0x08, 0xF1, 0xC1][..], "register U"),
    ];

    for (code, name) in invalid {
        let mut vcpu = test_vcpu(memory_with_code(code));
        vcpu.regs.rax = 0xFFFF_FFFF_1020_3040;
        vcpu.regs.rcx = 0x0123_4567_89AB_CDEF;
        let before = register_image(&vcpu);
        let error = format!("{:#}", vcpu.step().expect_err(name));
        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        assert_eq!(register_image(&vcpu), before, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name} RIP");
    }

    let code = [0x62, 0xF4, 0x7C, 0x08, 0xF1, 0xC1];
    let mut disabled = test_vcpu(memory_with_code(&code));
    disabled.set_apx_enabled(false);
    let before = register_image(&disabled);
    let error = format!("{:#}", disabled.step().expect_err("APX disabled"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(register_image(&disabled), before);
    assert_eq!(disabled.regs.rip, 0);
}

#[test]
fn direct_apx_crc32_memory_fault_is_noncommitting() {
    let code = [0x62, 0xF4, 0x7C, 0x08, 0xF1, 0x03];
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.regs.rax = 0xFFFF_FFFF_1020_3040;
    vcpu.regs.rbx = 0x2_0000;
    let before = register_image(&vcpu);

    assert!(vcpu.step().is_err());
    assert_eq!(register_image(&vcpu), before);
    assert_eq!(vcpu.regs.rip, 0);
}
