//! Direct-execution regressions for F16C VEX precision conversion.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::VCpu;

const CODE: u64 = 0x1000;
const SENTINEL: u64 = 0xA55A_6996_F00F_3CC3;

fn vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = 0x2 | (1 << 0) | (1 << 6) | (1 << 10);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn encoding(ymm: bool, destination: u8, source: u8) -> [u8; 5] {
    assert!(destination < 16 && source < 16);
    let mut p0 = 0xE2;
    if destination >= 8 {
        p0 &= !0x80;
    }
    if source >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x79 | (u8::from(ymm) << 2),
        0x13,
        0xC0 | ((destination & 7) << 3) | (source & 7),
    ]
}

fn set_fp16_source(vcpu: &mut X86_64Vcpu, register: u8, lanes: [u16; 8]) {
    let mut bytes = [0u8; 16];
    for (lane, value) in lanes.into_iter().enumerate() {
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    let index = usize::from(register);
    vcpu.regs.xmm[index][0] = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    vcpu.regs.xmm[index][1] = u64::from_le_bytes(bytes[8..].try_into().unwrap());
}

fn fill_destination(vcpu: &mut X86_64Vcpu, register: u8) {
    let index = usize::from(register);
    vcpu.regs.xmm[index] = [SENTINEL; 2];
    vcpu.regs.ymm_high[index] = [SENTINEL; 2];
    vcpu.regs.zmm_high[index] = [SENTINEL; 4];
}

#[test]
fn vcvtph2ps_masked_invalid_quiets_snan_and_clears_unchanged_low_destination() {
    let code = encoding(false, 1, 2);
    let mut cpu = vcpu(&code);
    set_fp16_source(
        &mut cpu,
        2,
        [
            0x3C00, 0xC000, 0x7C01, 0x0001, 0x7E01, 0x7C00, 0xFC00, 0x8000,
        ],
    );
    let source_before = cpu.regs.xmm[2];

    // Preload the exact low result. A change-detection-only VEX wrapper would
    // see no low-state write and incorrectly retain the stale ZMM upper half.
    cpu.regs.xmm[1] = [0xC000_0000_3F80_0000, 0x3380_0000_7FC0_2000];
    cpu.regs.ymm_high[1] = [0; 2];
    cpu.regs.zmm_high[1] = [SENTINEL; 4];
    let rflags_before = cpu.regs.rflags;

    assert!(cpu.step().unwrap().is_none());
    assert_eq!(
        cpu.regs.xmm[1],
        [0xC000_0000_3F80_0000, 0x3380_0000_7FC0_2000]
    );
    assert_eq!(cpu.regs.ymm_high[1], [0; 2]);
    assert_eq!(cpu.regs.zmm_high[1], [0; 4]);
    assert_eq!(cpu.regs.xmm[2], source_before);
    assert_eq!(cpu.mxcsr, 0x1F81);
    assert_eq!(cpu.regs.rflags, rflags_before);
    assert_eq!(cpu.regs.rip, CODE + code.len() as u64);
}

fn assert_unmasked_invalid_is_precise(vector: u8, cr4: u64) {
    let code = encoding(true, 9, 10);
    let mut cpu = vcpu(&code);
    fill_destination(&mut cpu, 9);
    set_fp16_source(&mut cpu, 10, [0x7C01; 8]);
    cpu.mxcsr = 0x1F80 & !(1 << 7);
    cpu.sregs.cr4 = cr4;
    let registers_before = cpu.regs.clone();

    let error = cpu
        .step()
        .expect_err("unmasked VCVTPH2PS invalid exception must not retire");
    assert!(
        format!("{error:?}").contains(&format!("IDT entry {vector} not present")),
        "wrong exception: {error:?}"
    );
    assert_eq!(cpu.regs.rip, registers_before.rip);
    assert_eq!(cpu.regs.xmm, registers_before.xmm);
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high);
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high);
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext);
    assert_ne!(cpu.mxcsr & 1, 0);
}

#[test]
fn vcvtph2ps_unmasked_invalid_obeys_osxmmexcpt_without_destination_commit() {
    for (vector, cr4) in [(19, 1 << 10), (6, 0)] {
        assert_unmasked_invalid_is_precise(vector, cr4);
    }
}

#[test]
fn vcvtph2ps_later_memory_fault_precedes_invalid_status_and_destination_commit() {
    // VEX.128.66.0F38.W0 VCVTPH2PS xmm1, qword ptr [rax].
    let code = [0xC4, 0xE2, 0x79, 0x13, 0x08];
    let mut cpu = vcpu(&code);
    cpu.regs.rax = 0xFFFE;
    cpu.write_mem(0xFFFE, 0x7C01, 2).unwrap();
    fill_destination(&mut cpu, 1);
    let registers_before = cpu.regs.clone();
    let mxcsr_before = cpu.mxcsr;

    assert!(cpu.step().is_err());
    assert_eq!(cpu.regs.rip, registers_before.rip);
    assert_eq!(cpu.regs.xmm, registers_before.xmm);
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high);
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high);
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext);
    assert_eq!(cpu.mxcsr, mxcsr_before);
}

#[test]
fn vcvtph2ps_reserved_w_and_vvvv_fault_before_memory_or_state_access() {
    let valid = [0xC4, 0xE2, 0x79, 0x13, 0x08];
    let mut w1 = valid;
    w1[2] |= 0x80;
    let mut vvvv = valid;
    vvvv[2] &= !0x08;

    for code in [w1, vvvv] {
        let mut cpu = vcpu(&code);
        cpu.regs.rax = 0x2_0000;
        fill_destination(&mut cpu, 1);
        let registers_before = cpu.regs.clone();
        let mxcsr_before = cpu.mxcsr;
        let error = cpu.step().expect_err("reserved VCVTPH2PS must #UD");
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "{code:02X?}: {error:?}"
        );
        assert_eq!(cpu.regs.rip, registers_before.rip, "{code:02X?}");
        assert_eq!(cpu.regs.xmm, registers_before.xmm, "{code:02X?}");
        assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high, "{code:02X?}");
        assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high, "{code:02X?}");
        assert_eq!(cpu.mxcsr, mxcsr_before, "{code:02X?}");
    }
}
