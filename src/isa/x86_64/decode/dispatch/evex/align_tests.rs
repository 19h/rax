//! Direct-execution regressions for EVEX VALIGND/Q.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

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

fn encoding(
    w: bool,
    ll: u8,
    destination: u8,
    source1: u8,
    source2: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> [u8; 7] {
    assert!(destination < 32 && source1 < 32 && source2 < 32 && ll < 3 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF3;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        (((!source1) & 0x0F) << 3) | 0x05 | if w { 0x80 } else { 0 },
        (ll << 5) | if source1 < 16 { 0x08 } else { 0 } | mask | if zeroing { 0x80 } else { 0 },
        0x03,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
        immediate,
    ]
}

fn zmm(vcpu: &X86_64Vcpu, register: u8) -> [u64; 8] {
    if register >= 16 {
        return vcpu.regs.zmm_ext[(register - 16) as usize];
    }
    let index = register as usize;
    let mut value = [0u64; 8];
    value[..2].copy_from_slice(&vcpu.regs.xmm[index]);
    value[2..4].copy_from_slice(&vcpu.regs.ymm_high[index]);
    value[4..].copy_from_slice(&vcpu.regs.zmm_high[index]);
    value
}

fn set_zmm(vcpu: &mut X86_64Vcpu, register: u8, value: [u64; 8]) {
    if register >= 16 {
        vcpu.regs.zmm_ext[(register - 16) as usize] = value;
        return;
    }
    let index = register as usize;
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
    for register in 0u8..32 {
        set_zmm(
            &mut vcpu,
            register,
            std::array::from_fn(|lane| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register as usize * 13 + lane * 7) as u32)
                    ^ ((register as u64) << 56)
                    ^ (lane as u64 * 0x1111_2222_4444_8889)
            }),
        );
    }
    for register in 0u8..16 {
        vcpu.set_reg(
            register,
            0xFEDC_BA98_7654_3210u64.rotate_left((register * 3) as u32),
            8,
        );
    }
    vcpu.regs.k = std::array::from_fn(|index| {
        0xA55A_3CC3_F00F_9696u64.rotate_left((index * 7) as u32) ^ (1u64 << index)
    });
    vcpu.regs.mm = std::array::from_fn(|index| 0x8877_6655_4433_2211 ^ index as u64);
    vcpu
}

fn vector_bytes(vector: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (index, word) in vector.iter().enumerate() {
        bytes[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn vector_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[test]
fn vector_align_covers_every_shape_extension_alias_mask_and_immediate_boundary() {
    let operands = [
        (1u8, 2u8, 3u8),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (1, 1, 2),
        (1, 2, 1),
        (1, 1, 1),
    ];
    for (w, elem_size) in [(false, 4usize), (true, 8usize)] {
        for (ll, vl_bytes) in [(0u8, 16usize), (1, 32), (2, 64)] {
            let lanes = vl_bytes / elem_size;
            for (destination, source1, source2) in operands {
                for immediate in [0u8, 1, (lanes - 1) as u8, lanes as u8, 0xFF] {
                    for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                        let code = encoding(
                            w,
                            ll,
                            destination,
                            source1,
                            source2,
                            mask,
                            zeroing,
                            immediate,
                        );
                        let mut vcpu = initialized_vcpu(&code);
                        let before = vcpu.regs.clone();
                        let before_vectors: [[u64; 8]; 32] =
                            std::array::from_fn(|register| zmm(&vcpu, register as u8));

                        assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");

                        let source1_bytes = vector_bytes(before_vectors[source1 as usize]);
                        let source2_bytes = vector_bytes(before_vectors[source2 as usize]);
                        let mut expected = vector_bytes(before_vectors[destination as usize]);
                        let shift = immediate as usize & (lanes - 1);
                        let active_mask = if mask == 0 {
                            u64::MAX
                        } else {
                            before.k[mask as usize]
                        };
                        for lane in 0..lanes {
                            let destination_base = lane * elem_size;
                            if (active_mask >> lane) & 1 == 0 {
                                if zeroing {
                                    expected[destination_base..destination_base + elem_size]
                                        .fill(0);
                                }
                                continue;
                            }
                            let source_lane = lane + shift;
                            let (source, source_base) = if source_lane < lanes {
                                (&source2_bytes, source_lane * elem_size)
                            } else {
                                (&source1_bytes, (source_lane - lanes) * elem_size)
                            };
                            expected[destination_base..destination_base + elem_size]
                                .copy_from_slice(&source[source_base..source_base + elem_size]);
                        }
                        expected[vl_bytes..].fill(0);

                        assert_eq!(
                            zmm(&vcpu, destination),
                            vector_words(expected),
                            "{code:02X?}"
                        );
                        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
                        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPR state");
                        assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}: MMX state");
                        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
                        assert_eq!(vcpu.regs.rip, CODE + 7, "{code:02X?}: RIP");
                        for register in 0u8..32 {
                            if register != destination {
                                assert_eq!(
                                    zmm(&vcpu, register),
                                    before_vectors[register as usize],
                                    "{code:02X?}: zmm{register}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = initialized_vcpu(code);
    let before = vcpu.regs.clone();

    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved vector align committed: {code:02X?}: {exit:?}"),
    };
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "wrong exception for {code:02X?}: {error:?}"
    );
    assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
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
    assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
    assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPR state");
    assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}: MMX state");
    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
}

#[test]
fn vector_align_reserved_fields_raise_precise_ud_without_commit() {
    for w in [false, true] {
        let valid = encoding(w, 0, 1, 2, 3, 0, false, 0xFF);
        let mut invalid = Vec::new();

        let mut embedded_broadcast = valid;
        embedded_broadcast[3] |= 0x10;
        invalid.push(embedded_broadcast);
        let mut reserved_ll = valid;
        reserved_ll[3] |= 0x60;
        invalid.push(reserved_ll);
        let mut zeroing_without_mask = valid;
        zeroing_without_mask[3] |= 0x80;
        invalid.push(zeroing_without_mask);
        for pp in [0u8, 2, 3] {
            let mut wrong_pp = valid;
            wrong_pp[2] = (wrong_pp[2] & !0x03) | pp;
            invalid.push(wrong_pp);
        }

        for code in invalid {
            assert_reserved_ud(&code);
        }
    }
}

#[test]
fn vector_align_memory_broadcast_remains_supported() {
    // valignq $1, qword ptr [rax]{1to2}, xmm2, xmm1
    let code = [0x62, 0xF3, 0xED, 0x18, 0x03, 0x08, 0x01];
    let mut vcpu = initialized_vcpu(&code);
    vcpu.regs.rax = DATA;
    let scalar = 0x8877_6655_4433_2211;
    vcpu.write_mem(DATA, scalar, 8).unwrap();
    let source1 = zmm(&vcpu, 2);
    let before = vcpu.regs.clone();

    assert!(vcpu.step().unwrap().is_none());

    assert_eq!(vcpu.regs.xmm[1], [scalar, source1[0]]);
    assert_eq!(vcpu.regs.ymm_high[1], [0; 2]);
    assert_eq!(vcpu.regs.zmm_high[1], [0; 4]);
    assert_eq!(vcpu.regs.k, before.k);
    assert_eq!(gprs(&vcpu.regs), gprs(&before));
    assert_eq!(vcpu.regs.mm, before.mm);
    assert_eq!(vcpu.regs.rflags, before.rflags);
    assert_eq!(vcpu.regs.rip, CODE + 7);
}
