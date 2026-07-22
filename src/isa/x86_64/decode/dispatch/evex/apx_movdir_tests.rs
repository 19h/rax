//! Direct-execution regressions for APX-promoted MOVDIR64B and MOVDIRI.

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

#[test]
fn direct_apx_movdiri_executes_both_widths_and_egpr_sib_addresses() {
    // LLVM 23: `movdiri dword ptr [r16], r17d`.
    let dword_code = [0x62, 0xEC, 0x7C, 0x08, 0xF9, 0x08];
    let memory = memory_with_code(&dword_code);
    memory
        .write_slice(&[0xA5; 8], GuestAddress(0x2000))
        .unwrap();
    let mut dword = test_vcpu(memory.clone());
    dword.regs.r16 = 0x2000;
    dword.regs.r17 = 0xA1B2_C3D4_5566_7788;
    let rflags = dword.regs.rflags;

    assert!(dword.step().expect("APX MOVDIRI dword").is_none());
    let mut observed = [0u8; 8];
    memory
        .read_slice(&mut observed, GuestAddress(0x2000))
        .unwrap();
    assert_eq!(observed, [0x88, 0x77, 0x66, 0x55, 0xA5, 0xA5, 0xA5, 0xA5]);
    assert_eq!(dword.regs.r17, 0xA1B2_C3D4_5566_7788);
    assert_eq!(dword.regs.rflags, rflags);
    assert_eq!(dword.regs.rip, dword_code.len() as u64);

    // LLVM 23: `movdiri qword ptr [r20 + 4*r21 + 64], r31`.
    let qword_code = [0x62, 0x6C, 0xF8, 0x08, 0xF9, 0x7C, 0xAC, 0x40];
    let memory = memory_with_code(&qword_code);
    let mut qword = test_vcpu(memory.clone());
    qword.regs.r20 = 0x2000;
    qword.regs.r21 = 3;
    qword.regs.r31 = 0x0123_4567_89AB_CDEF;
    let rflags = qword.regs.rflags;

    assert!(qword.step().expect("APX MOVDIRI qword EGPR SIB").is_none());
    let mut observed = [0u8; 8];
    memory
        .read_slice(&mut observed, GuestAddress(0x204C))
        .unwrap();
    assert_eq!(observed, 0x0123_4567_89AB_CDEF_u64.to_le_bytes());
    assert_eq!(qword.regs.r31, 0x0123_4567_89AB_CDEF);
    assert_eq!(qword.regs.rflags, rflags);
    assert_eq!(qword.regs.rip, qword_code.len() as u64);
}

#[test]
fn direct_apx_movdir64b_copies_from_egpr_address_to_egpr_destination() {
    // LLVM 23: `movdir64b r16, [r17]`.
    let code = [0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x01];
    let memory = memory_with_code(&code);
    let source: Vec<u8> = (0u8..64).map(|byte| byte.wrapping_mul(7) ^ 0xA5).collect();
    memory.write_slice(&source, GuestAddress(0x2003)).unwrap();
    let mut vcpu = test_vcpu(memory.clone());
    vcpu.regs.r16 = 0x3000;
    vcpu.regs.r17 = 0x2003;
    let rflags = vcpu.regs.rflags;

    assert!(vcpu.step().expect("APX MOVDIR64B EGPR").is_none());
    let mut observed = [0u8; 64];
    memory
        .read_slice(&mut observed, GuestAddress(0x3000))
        .unwrap();
    assert_eq!(observed.as_slice(), source.as_slice());
    assert_eq!(vcpu.regs.r16, 0x3000);
    assert_eq!(vcpu.regs.r17, 0x2003);
    assert_eq!(vcpu.regs.rflags, rflags);
    assert_eq!(vcpu.regs.rip, code.len() as u64);
}

#[test]
fn direct_apx_movdir_address_size_and_fs_apply_to_the_correct_operands() {
    // MOVDIRI's destination memory uses FS and modulo-2^32 address arithmetic.
    let movdiri_code = [0x64, 0x67, 0x62, 0x6C, 0xF8, 0x08, 0xF9, 0x7C, 0xAC, 0x40];
    let memory = memory_with_code(&movdiri_code);
    let mut movdiri = test_vcpu(memory.clone());
    movdiri.sregs.fs.base = 0x1000;
    movdiri.regs.r20 = 0xFFFF_0000_0000_2000;
    movdiri.regs.r21 = 3;
    movdiri.regs.r31 = 0x0123_4567_89AB_CDEF;

    assert!(movdiri.step().expect("APX MOVDIRI FS addr32").is_none());
    let mut qword = [0u8; 8];
    memory.read_slice(&mut qword, GuestAddress(0x304C)).unwrap();
    assert_eq!(qword, movdiri.regs.r31.to_le_bytes());

    // MOVDIR64B applies FS to its source memory, but addr32 truncation applies
    // independently to both the source effective address and destination GPR.
    let movdir64b_code = [0x64, 0x67, 0x62, 0x7C, 0x79, 0x08, 0xF8, 0x4C, 0xAC, 0x40];
    let memory = memory_with_code(&movdir64b_code);
    let source = [0x5A; 64];
    memory.write_slice(&source, GuestAddress(0x304C)).unwrap();
    let mut movdir64b = test_vcpu(memory.clone());
    movdir64b.sregs.fs.base = 0x1000;
    movdir64b.regs.r9 = 0xFFFF_0000_0000_4000;
    movdir64b.regs.r20 = 0xFFFF_0000_0000_2000;
    movdir64b.regs.r21 = 3;

    assert!(movdir64b.step().expect("APX MOVDIR64B FS addr32").is_none());
    let mut observed = [0u8; 64];
    memory
        .read_slice(&mut observed, GuestAddress(0x4000))
        .unwrap();
    assert_eq!(observed, source);
}

#[test]
fn direct_apx_movdir_reserved_fields_and_disabled_feature_fault_without_commit() {
    let invalid = [
        (&[0x62, 0xEC, 0x7D, 0x08, 0xF9, 0x08][..], "MOVDIRI pp"),
        (&[0x62, 0xEC, 0x7C, 0x18, 0xF9, 0x08][..], "MOVDIRI ND"),
        (&[0x62, 0xEC, 0x7C, 0x0C, 0xF9, 0x08][..], "MOVDIRI NF"),
        (&[0x62, 0xEC, 0x7C, 0x88, 0xF9, 0x08][..], "MOVDIRI z"),
        (&[0x62, 0xEC, 0x7C, 0x28, 0xF9, 0x08][..], "MOVDIRI LL"),
        (&[0x62, 0xEC, 0x7C, 0x09, 0xF9, 0x08][..], "MOVDIRI aaa"),
        (&[0x62, 0xEC, 0x74, 0x08, 0xF9, 0x08][..], "MOVDIRI V3:0"),
        (&[0x62, 0xEC, 0x7C, 0x00, 0xF9, 0x08][..], "MOVDIRI V4"),
        (&[0x62, 0xEC, 0x7C, 0x08, 0xF9, 0xC8][..], "MOVDIRI mod=3"),
        (&[0x62, 0xEC, 0xFD, 0x08, 0xF8, 0x01][..], "MOVDIR64B W=1"),
        (&[0x62, 0xEC, 0x7D, 0x18, 0xF8, 0x01][..], "MOVDIR64B ND"),
        (&[0x62, 0xEC, 0x7D, 0x0C, 0xF8, 0x01][..], "MOVDIR64B NF"),
        (&[0x62, 0xEC, 0x7D, 0x88, 0xF8, 0x01][..], "MOVDIR64B z"),
        (&[0x62, 0xEC, 0x7D, 0x28, 0xF8, 0x01][..], "MOVDIR64B LL"),
        (&[0x62, 0xEC, 0x7D, 0x09, 0xF8, 0x01][..], "MOVDIR64B aaa"),
        (&[0x62, 0xEC, 0x75, 0x08, 0xF8, 0x01][..], "MOVDIR64B V3:0"),
        (&[0x62, 0xEC, 0x7D, 0x00, 0xF8, 0x01][..], "MOVDIR64B V4"),
        (&[0x62, 0xEC, 0x7D, 0x08, 0xF8, 0xC1][..], "MOVDIR64B mod=3"),
    ];

    for (code, name) in invalid {
        let memory = memory_with_code(code);
        memory
            .write_slice(&[0xA5; 64], GuestAddress(0x3000))
            .unwrap();
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.regs.r16 = 0x3000;
        vcpu.regs.r17 = 0x0123_4567_89AB_CDEF;
        let before = register_image(&vcpu);
        let error = format!("{:#}", vcpu.step().expect_err(name));
        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        assert_eq!(register_image(&vcpu), before, "{name}");
        let mut observed = [0u8; 64];
        memory
            .read_slice(&mut observed, GuestAddress(0x3000))
            .unwrap();
        assert_eq!(observed, [0xA5; 64], "{name}");
    }

    let code = [0x62, 0xEC, 0x7C, 0x08, 0xF9, 0x08];
    let memory = memory_with_code(&code);
    memory
        .write_slice(&[0xA5; 8], GuestAddress(0x3000))
        .unwrap();
    let mut disabled = test_vcpu(memory.clone());
    disabled.set_apx_enabled(false);
    disabled.regs.r16 = 0x3000;
    disabled.regs.r17 = 0x0123_4567_89AB_CDEF;
    let before = register_image(&disabled);
    let error = format!("{:#}", disabled.step().expect_err("APX disabled"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(register_image(&disabled), before);
    let mut observed = [0u8; 8];
    memory
        .read_slice(&mut observed, GuestAddress(0x3000))
        .unwrap();
    assert_eq!(observed, [0xA5; 8]);
}

#[test]
fn direct_apx_movdir_fault_priority_is_precise_and_noncommitting() {
    // A disabled APX profile must #UD before MOVDIR64B's alignment or source
    // memory checks. With APX enabled, the misaligned destination is #GP(0)
    // before the unmapped source is read.
    let code = [0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x01];
    for (enabled, expected_vector) in [(false, 6), (true, 13)] {
        let memory = memory_with_code(&code);
        let mut vcpu = test_vcpu(memory);
        vcpu.set_apx_enabled(enabled);
        vcpu.regs.r16 = 0x3001;
        vcpu.regs.r17 = 0x2_0000;
        let before = register_image(&vcpu);

        let error = format!("{:#}", vcpu.step().expect_err("faulting MOVDIR64B"));
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "APX enabled={enabled}: {error}",
        );
        assert_eq!(register_image(&vcpu), before);
        assert_eq!(vcpu.regs.rip, 0);
    }

    let code = [0x62, 0xEC, 0xFC, 0x08, 0xF9, 0x08];
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.regs.r16 = 0x2_0000;
    vcpu.regs.r17 = 0x0123_4567_89AB_CDEF;
    let before = register_image(&vcpu);
    assert!(vcpu.step().is_err());
    assert_eq!(register_image(&vcpu), before);
    assert_eq!(vcpu.regs.rip, 0);
}
