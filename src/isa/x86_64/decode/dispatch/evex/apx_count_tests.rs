//! Direct-execution regressions for APX-promoted POPCNT, TZCNT, and LZCNT.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DATA_ADDRESS: u64 = 0x2000;
const STATUS_MASK: u64 = 0x08D5;
const INITIAL_RFLAGS: u64 = 0x2 | STATUS_MASK | flags::bits::DF;
const INITIAL_R8: u64 = 0xA1B2_C3D4_E5F6_7788;

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
        2 => (old & !width_mask(bytes)) | (value & width_mask(bytes)),
        4 => value as u32 as u64,
        8 => value,
        _ => unreachable!(),
    }
}

fn count_encoding(bytes: u8, opcode: u8, nf: bool, memory: bool) -> Vec<u8> {
    let p1 = match bytes {
        2 => 0x7D,
        4 => 0x7C,
        8 => 0xFC,
        _ => unreachable!(),
    };
    let p2 = 0x08 | if nf { 0x04 } else { 0 };
    let modrm = if memory { 0x03 } else { 0xC3 };
    vec![0x62, 0x74, p1, p2, opcode, modrm]
}

fn reference_count(opcode: u8, source: u64, bytes: u8) -> u64 {
    let value = source & width_mask(bytes);
    let bits = u32::from(bytes) * 8;
    match opcode {
        0x88 => u64::from(value.count_ones()),
        0xF4 => u64::from(if value == 0 {
            bits
        } else {
            value.trailing_zeros()
        }),
        0xF5 => u64::from(if value == 0 {
            bits
        } else {
            value.leading_zeros() - (64 - bits)
        }),
        _ => unreachable!(),
    }
}

fn reference_rflags(opcode: u8, source: u64, result: u64, bytes: u8, nf: bool) -> u64 {
    if nf {
        return INITIAL_RFLAGS;
    }
    let value = source & width_mask(bytes);
    let status = match opcode {
        0x88 => u64::from(value == 0) * flags::bits::ZF,
        0xF4 | 0xF5 => {
            u64::from(value == 0) * flags::bits::CF
                | u64::from(result == 0) * flags::bits::ZF
                | (INITIAL_RFLAGS
                    & (flags::bits::PF | flags::bits::AF | flags::bits::SF | flags::bits::OF))
        }
        _ => unreachable!(),
    };
    (INITIAL_RFLAGS & !STATUS_MASK) | status
}

#[test]
fn direct_apx_count_covers_every_width_nf_state_source_class_and_flag_contract() {
    for (opcode, name) in [(0x88, "POPCNT"), (0xF4, "TZCNT"), (0xF5, "LZCNT")] {
        for bytes in [2, 4, 8] {
            let bits = u32::from(bytes) * 8;
            let high_bit = 1_u64 << (bits - 1);
            let values = match opcode {
                0x88 => [0, high_bit | 0x8001],
                0xF4 => [0, 0x100],
                0xF5 => [0, high_bit],
                _ => unreachable!(),
            };
            for source in values {
                for nf in [false, true] {
                    for memory_form in [false, true] {
                        let code = count_encoding(bytes, opcode, nf, memory_form);
                        let memory = memory_with_code(&code);
                        let mut vcpu = test_vcpu(memory.clone());
                        vcpu.regs.r8 = INITIAL_R8;
                        if memory_form {
                            vcpu.regs.rbx = DATA_ADDRESS;
                            memory
                                .write_slice(
                                    &source.to_le_bytes()[..usize::from(bytes)],
                                    GuestAddress(DATA_ADDRESS),
                                )
                                .unwrap();
                        } else {
                            vcpu.regs.rbx = source;
                        }

                        assert!(
                            vcpu.step()
                                .unwrap_or_else(|error| panic!(
                                    "{name} width={bits} NF={nf} memory={memory_form} source={source:#x}: {error:#}"
                                ))
                                .is_none()
                        );

                        let result = reference_count(opcode, source, bytes);
                        assert_eq!(
                            vcpu.regs.r8,
                            merge_gpr(INITIAL_R8, result, bytes),
                            "{name} width={bits} NF={nf} memory={memory_form} result"
                        );
                        assert_eq!(
                            vcpu.regs.rflags,
                            reference_rflags(opcode, source, result, bytes, nf),
                            "{name} width={bits} NF={nf} memory={memory_form} RFLAGS"
                        );
                        assert_eq!(vcpu.regs.rip, code.len() as u64);
                    }
                }
            }
        }
    }
}

#[test]
fn direct_apx_count_uses_egpr_register_and_memory_index_extensions() {
    // POPCNT r24,r16: R' and B4 select both EGPR banks.
    let code = [0x62, 0x6C, 0xFC, 0x08, 0x88, 0xC0];
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.regs.r16 = 0xF0F0_0000_0000_0001;
    vcpu.regs.r24 = u64::MAX;
    assert!(vcpu.step().expect("APX POPCNT r24,r16").is_none());
    assert_eq!(vcpu.regs.r24, 9);

    // U=0 is EVEX.X4=1 for memory forms, selecting R16 as the SIB index.
    let code = [0x62, 0x74, 0xF8, 0x08, 0xF4, 0x04, 0x03];
    let memory = memory_with_code(&code);
    memory
        .write_slice(&0x100_u64.to_le_bytes(), GuestAddress(DATA_ADDRESS + 0x18))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rbx = DATA_ADDRESS;
    vcpu.regs.r16 = 0x18;
    assert!(vcpu.step().expect("APX TZCNT r8,[rbx+r16]").is_none());
    assert_eq!(vcpu.regs.r8, 8);
}

#[test]
fn direct_apx_count_materializes_only_the_flags_each_form_defines() {
    for nf in [false, true] {
        // ADD EAX,1 leaves a lazy PF/AF/SF/OF image. A zero-source TZCNT either
        // replaces CF/ZF after materializing those retained bits or, under NF,
        // leaves the complete lazy image untouched.
        let mut code = vec![0x83, 0xC0, 0x01];
        code.extend_from_slice(&count_encoding(8, 0xF4, nf, false));
        let mut vcpu = test_vcpu(memory_with_code(&code));
        vcpu.regs.rflags = 0x2;
        vcpu.regs.rax = 0x7FFF_FFFF;
        vcpu.regs.rbx = 0;

        assert!(vcpu.step().expect("lazy ADD").is_none());
        assert!(vcpu.step().expect("APX TZCNT after lazy ADD").is_none());
        vcpu.materialize_flags();

        let expected = if nf { 0x896 } else { 0x897 };
        assert_eq!(vcpu.regs.rflags, expected, "NF={nf} lazy flag image");
    }
}

#[test]
fn direct_apx_count_reserved_fields_fault_without_commit() {
    let invalid = [
        (&[0x62, 0x74, 0x7E, 0x08, 0x88, 0xC3][..], "F3 pp"),
        (&[0x62, 0x74, 0x7F, 0x08, 0xF4, 0xC3][..], "F2 pp"),
        (&[0x62, 0x74, 0x7C, 0x18, 0xF5, 0xC3][..], "ND"),
        (&[0x62, 0x74, 0x7C, 0x88, 0x88, 0xC3][..], "z"),
        (&[0x62, 0x74, 0x7C, 0x28, 0xF4, 0xC3][..], "LL"),
        (&[0x62, 0x74, 0x7C, 0x09, 0xF5, 0xC3][..], "payload bit 0"),
        (&[0x62, 0x74, 0x7C, 0x0A, 0x88, 0xC3][..], "payload bit 1"),
        (&[0x62, 0x74, 0x74, 0x08, 0xF4, 0xC3][..], "V3:0"),
        (&[0x62, 0x74, 0x7C, 0x00, 0xF5, 0xC3][..], "V4"),
        (&[0x62, 0x74, 0x78, 0x08, 0x88, 0xC3][..], "register U"),
        (
            &[0x66, 0x62, 0x74, 0x7C, 0x08, 0x88, 0xC3][..],
            "leading 66",
        ),
    ];

    for (code, name) in invalid {
        let mut vcpu = test_vcpu(memory_with_code(code));
        vcpu.regs.r8 = INITIAL_R8;
        vcpu.regs.rbx = 0x0123_4567_89AB_CDEF;
        let before = register_image(&vcpu);
        let error = format!("{:#}", vcpu.step().expect_err(name));
        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        assert_eq!(register_image(&vcpu), before, "{name}");
        assert_eq!(vcpu.regs.rip, 0, "{name} RIP");
    }
}

#[test]
fn direct_apx_count_memory_fault_is_noncommitting_and_preserves_lazy_flags() {
    let mut code = vec![0x83, 0xC0, 0x01];
    code.extend_from_slice(&count_encoding(8, 0xF5, false, true));
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.regs.rflags = 0x2;
    vcpu.regs.rax = 0x7FFF_FFFF;
    vcpu.regs.r8 = INITIAL_R8;
    vcpu.regs.rbx = 0x2_0000;

    assert!(vcpu.step().expect("lazy ADD").is_none());
    assert!(vcpu.step().is_err());
    assert_eq!(vcpu.regs.r8, INITIAL_R8);
    assert_eq!(vcpu.regs.rip, 3);
    vcpu.materialize_flags();
    assert_eq!(vcpu.regs.rflags, 0x896);
}
