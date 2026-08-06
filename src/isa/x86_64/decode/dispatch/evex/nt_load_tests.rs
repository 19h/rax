//! Direct-execution regressions for EVEX `VMOVNTDQA`.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x2000;

fn long_mode_vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = 0x2 | 0x8D5;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn encoding(ll: u8, destination: u8) -> [u8; 6] {
    assert!(ll < 3 && destination < 32);
    [
        0x62,
        0x62 | (u8::from(destination & 8 == 0) << 7) | (u8::from(destination < 16) << 4),
        0x7D,
        (ll << 5) | 0x08,
        0x2A,
        (destination & 7) << 3,
    ]
}

fn zmm(vcpu: &X86_64Vcpu, register: u8) -> [u64; 8] {
    if register >= 16 {
        return vcpu.regs.zmm_ext[usize::from(register - 16)];
    }
    let mut value = [0; 8];
    let index = usize::from(register);
    value[..2].copy_from_slice(&vcpu.regs.xmm[index]);
    value[2..4].copy_from_slice(&vcpu.regs.ymm_high[index]);
    value[4..].copy_from_slice(&vcpu.regs.zmm_high[index]);
    value
}

fn set_zmm(vcpu: &mut X86_64Vcpu, register: u8, value: [u64; 8]) {
    if register >= 16 {
        vcpu.regs.zmm_ext[usize::from(register - 16)] = value;
        return;
    }
    let index = usize::from(register);
    vcpu.regs.xmm[index].copy_from_slice(&value[..2]);
    vcpu.regs.ymm_high[index].copy_from_slice(&value[2..4]);
    vcpu.regs.zmm_high[index].copy_from_slice(&value[4..]);
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn initialized_vcpu(code: &[u8]) -> X86_64Vcpu {
    let mut vcpu = long_mode_vcpu(code);
    vcpu.regs.rax = DATA;
    for register in 0u8..32 {
        set_zmm(
            &mut vcpu,
            register,
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((usize::from(register) * 11 + word * 7) as u32)
                    ^ (u64::from(register) << 56)
                    ^ (word as u64).wrapping_mul(0x1111_2222_4444_8889)
            }),
        );
    }
    for word in 0..8u64 {
        vcpu.write_mem(
            DATA + word * 8,
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((word * 9) as u32)
                ^ word.wrapping_mul(0x0102_0408_1020_4081),
            8,
        )
        .unwrap();
    }
    vcpu
}

fn source_words() -> [u64; 8] {
    std::array::from_fn(|word| {
        0xF0E1_D2C3_B4A5_9687u64.rotate_left((word * 9) as u32)
            ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
    })
}

#[test]
fn nt_load_covers_all_widths_low_high_destinations_and_upper_zeroing() {
    for (ll, width) in [(0u8, 16usize), (1, 32), (2, 64)] {
        for destination in [0u8, 9, 17, 31] {
            let code = encoding(ll, destination);
            let mut vcpu = initialized_vcpu(&code);
            let before = vcpu.regs.clone();
            let before_vectors: [[u64; 8]; 32] =
                std::array::from_fn(|register| zmm(&vcpu, register as u8));

            assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");

            let words = width / 8;
            let mut expected = [0u64; 8];
            expected[..words].copy_from_slice(&source_words()[..words]);
            assert_eq!(zmm(&vcpu, destination), expected, "{code:02X?}");
            for register in 0u8..32 {
                if register != destination {
                    assert_eq!(
                        zmm(&vcpu, register),
                        before_vectors[usize::from(register)],
                        "{code:02X?}: ZMM{register}"
                    );
                }
            }
            assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPRs");
            assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmasks");
            assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: RFLAGS");
            assert_eq!(vcpu.regs.rip, CODE + code.len() as u64, "{code:02X?}");
        }
    }
}

fn assert_precise_exception(code: &[u8], vector: u8, address: u64) {
    let mut vcpu = initialized_vcpu(code);
    vcpu.regs.rax = address;
    let before = vcpu.regs.clone();
    let mxcsr_before = vcpu.mxcsr;
    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved/faulting VMOVNTDQA committed: {code:02X?}: {exit:?}"),
    };
    assert!(
        format!("{error:?}").contains(&format!("IDT entry {vector} not present")),
        "wrong exception for {code:02X?}: {error:?}"
    );
    assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
    assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPRs");
    assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
    assert_eq!(
        vcpu.regs.ymm_high, before.ymm_high,
        "{code:02X?}: YMM state"
    );
    assert_eq!(
        vcpu.regs.zmm_high, before.zmm_high,
        "{code:02X?}: ZMM state"
    );
    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
    assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmasks");
    assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}: MMX state");
    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: RFLAGS");
    assert_eq!(vcpu.mxcsr, mxcsr_before, "{code:02X?}: MXCSR");
}

#[test]
fn nt_load_reserved_fields_raise_precise_ud_without_commit() {
    let valid = encoding(2, 17);
    let mut invalid = Vec::new();
    for encoded_vvvv in 0u8..=0x0E {
        let mut code = valid;
        code[2] = (code[2] & !0x78) | (encoded_vvvv << 3);
        invalid.push(code);
    }
    for (byte, bit) in [(2usize, 0x80u8), (3, 0x80), (3, 0x10), (3, 0x01)] {
        let mut code = valid;
        code[byte] |= bit;
        invalid.push(code);
    }
    let mut reserved_v_prime = valid;
    reserved_v_prime[3] &= !0x08;
    invalid.push(reserved_v_prime);
    let mut reserved_ll = valid;
    reserved_ll[3] |= 0x60;
    invalid.push(reserved_ll);
    let mut register_form = valid;
    register_form[5] |= 0xC0;
    invalid.push(register_form);
    for pp in [0u8, 2, 3] {
        let mut code = valid;
        code[2] = (code[2] & !3) | pp;
        invalid.push(code);
    }

    for code in invalid {
        assert_precise_exception(&code, 6, DATA);
    }
}

#[test]
fn nt_load_alignment_fault_precedes_memory_and_commits_nothing() {
    for ll in 0u8..=2 {
        assert_precise_exception(&encoding(ll, 17), 13, DATA + 1);
    }
}
