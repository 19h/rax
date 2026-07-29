//! Direct-execution regressions for exact packed I32-to-F64 conversions.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use super::avx512::{read_reg_bytes, write_vec_vl};
use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::VCpu;

const CODE: u64 = 0x1000;
const UNMAPPED: u64 = 0x2_0000;
const SENTINEL: u64 = 0xCAFE_BABE_DEAD_BEEF;

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

#[allow(clippy::too_many_arguments)]
fn encoding(
    signed: bool,
    ll: u8,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    embedded_control: bool,
    memory: bool,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if !memory && source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if !memory && source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7E,
        (u8::from(zeroing) << 7) | (ll << 5) | (u8::from(embedded_control) << 4) | 0x08 | mask,
        if signed { 0xE6 } else { 0x7A },
        (if memory { 0 } else { 0xC0 }) | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn fill_destination(vcpu: &mut X86_64Vcpu, register: u8) {
    let mut bytes = [0u8; 64];
    for lane in bytes.chunks_exact_mut(8) {
        lane.copy_from_slice(&SENTINEL.to_le_bytes());
    }
    write_vec_vl(vcpu, register, 64, &bytes);
}

fn set_source(vcpu: &mut X86_64Vcpu, register: u8, values: &[u32; 8]) {
    let mut bytes = [0u8; 64];
    for (lane, value) in values.iter().enumerate() {
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[32..].fill(0xA5);
    write_vec_vl(vcpu, register, 64, &bytes);
}

fn lane(bytes: &[u8; 64], index: usize) -> u64 {
    u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
}

fn gpr_snapshot(vcpu: &X86_64Vcpu) -> [u64; 32] {
    std::array::from_fn(|register| vcpu.get_reg(register as u8, 8))
}

fn vector_snapshot(vcpu: &X86_64Vcpu) -> [[u8; 64]; 32] {
    std::array::from_fn(|register| read_reg_bytes(vcpu, register as u8, 64))
}

fn expected(raw: u32, signed: bool) -> u64 {
    if signed {
        f64::from(raw as i32).to_bits()
    } else {
        f64::from(raw).to_bits()
    }
}

#[test]
fn register_embedded_rounding_attempt_is_fixed_512_bit_exact_and_ignores_all_ll_values() {
    let values = [
        0,
        1,
        u32::MAX,
        i32::MAX as u32,
        i32::MIN as u32,
        0x0102_0304,
        0x7FFF_FFFE,
        0x8000_0001,
    ];
    let operands = [(17u8, 18u8), (31, 31)];
    let masks = [
        (0u8, false, u64::MAX),
        (3, false, 0b1010_1101),
        (3, true, 0b0101_0011),
    ];

    for signed in [false, true] {
        for (destination, source) in operands {
            for (mask, zeroing, mask_bits) in masks {
                let mut reference = None;
                for ll in 0..=3 {
                    let code =
                        encoding(signed, ll, destination, source, mask, zeroing, true, false);
                    let mut cpu = vcpu(&code);
                    fill_destination(&mut cpu, destination);
                    set_source(&mut cpu, source, &values);
                    let old_destination = read_reg_bytes(&cpu, destination, 64);
                    cpu.regs.k[usize::from(mask)] = mask_bits;
                    cpu.mxcsr = 0x1F80 | (u32::from(ll) << 13) | (1 << 5);
                    let registers_before = cpu.regs.clone();
                    let gprs_before = gpr_snapshot(&cpu);
                    let vectors_before = vector_snapshot(&cpu);
                    let mxcsr_before = cpu.mxcsr;

                    assert!(cpu.step().unwrap().is_none(), "{code:02X?}");
                    assert_eq!(cpu.regs.rip, CODE + 6, "{code:02X?}");
                    assert_eq!(cpu.regs.rflags, registers_before.rflags, "{code:02X?}");
                    assert_eq!(cpu.regs.k, registers_before.k, "{code:02X?}");
                    assert_eq!(gpr_snapshot(&cpu), gprs_before, "{code:02X?}");
                    assert_eq!(cpu.mxcsr, mxcsr_before, "{code:02X?}");

                    let actual = read_reg_bytes(&cpu, destination, 64);
                    for (index, raw) in values.iter().copied().enumerate() {
                        let active = mask == 0 || mask_bits & (1 << index) != 0;
                        let expected = if active {
                            expected(raw, signed)
                        } else if zeroing {
                            0
                        } else {
                            lane(&old_destination, index)
                        };
                        assert_eq!(lane(&actual, index), expected, "lane={index} {code:02X?}");
                    }
                    for register in 0..32 {
                        if register != usize::from(destination) {
                            assert_eq!(
                                read_reg_bytes(&cpu, register as u8, 64),
                                vectors_before[register],
                                "unrelated ZMM{register} {code:02X?}"
                            );
                        }
                    }
                    if let Some(expected) = reference {
                        assert_eq!(actual, expected, "ignored L'L changed {code:02X?}");
                    } else {
                        reference = Some(actual);
                    }
                }
            }
        }
    }
}

fn assert_ud_before_state_or_memory(code: &[u8]) {
    let mut cpu = vcpu(code);
    cpu.regs.rax = UNMAPPED;
    cpu.regs.k[1] = 0xA55A_3CC3_F00F_9696;
    cpu.mxcsr = 0xDFA5;
    let registers_before = cpu.regs.clone();
    let gprs_before = gpr_snapshot(&cpu);
    let mxcsr_before = cpu.mxcsr;
    let error = cpu
        .step()
        .expect_err("reserved I32-to-F64 encoding must #UD");
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "{code:02X?}: {error:?}"
    );
    assert_eq!(gpr_snapshot(&cpu), gprs_before, "{code:02X?}");
    assert_eq!(cpu.regs.rip, registers_before.rip, "{code:02X?}");
    assert_eq!(cpu.regs.rflags, registers_before.rflags, "{code:02X?}");
    assert_eq!(cpu.regs.xmm, registers_before.xmm, "{code:02X?}");
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high, "{code:02X?}");
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high, "{code:02X?}");
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext, "{code:02X?}");
    assert_eq!(cpu.regs.k, registers_before.k, "{code:02X?}");
    assert_eq!(cpu.regs.mm, registers_before.mm, "{code:02X?}");
    assert_eq!(cpu.mxcsr, mxcsr_before, "{code:02X?}");
}

#[test]
fn reserved_controls_fault_before_state_or_memory_access() {
    for signed in [false, true] {
        let mut reserved_vvvv = encoding(signed, 0, 1, 2, 1, false, true, false);
        reserved_vvvv[2] &= !0x08;
        let mut reserved_v_prime = encoding(signed, 0, 1, 2, 1, false, true, false);
        reserved_v_prime[3] &= !0x08;
        for code in [
            encoding(signed, 3, 1, 2, 0, false, false, false),
            encoding(signed, 3, 1, 0, 1, false, true, true),
            encoding(signed, 0, 1, 2, 0, true, true, false),
            reserved_vvvv,
            reserved_v_prime,
        ] {
            assert_ud_before_state_or_memory(&code);
        }
    }
}
