//! Direct-execution regressions for EVEX VPBROADCASTB/W/D/Q GPR sources.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CODE: u64 = 0x1000;

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
    opcode: u8,
    w: bool,
    ll: u8,
    destination: u8,
    source: u8,
    ignored_x: bool,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    assert!(destination < 32 && source < 16 && ll < 3 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
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

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

#[test]
fn gpr_broadcast_covers_every_shape_extension_mask_and_ignored_x() {
    for (opcode, w, elem_size) in [
        (0x7A, false, 1usize),
        (0x7B, false, 2),
        (0x7C, false, 4),
        (0x7C, true, 8),
    ] {
        for (ll, vl_bytes) in [(0u8, 16usize), (1, 32), (2, 64)] {
            for destination in [1u8, 9, 17, 25] {
                for source in [0u8, 4, 5, 7, 8, 12, 13, 15] {
                    for ignored_x in [false, true] {
                        for (mask, zeroing) in [(0u8, false), (1, false), (2, true)] {
                            let code = encoding(
                                opcode,
                                w,
                                ll,
                                destination,
                                source,
                                ignored_x,
                                mask,
                                zeroing,
                            );
                            let mut vcpu = long_mode_vcpu(&code);
                            for register in 0..16 {
                                vcpu.set_reg(
                                    register,
                                    0x0123_4567_89AB_CDEFu64.rotate_left((register * 5) as u32)
                                        ^ (register as u64 * 0x1111_1111_1111_1111),
                                    8,
                                );
                            }
                            for index in 0..16 {
                                vcpu.regs.xmm[index] = [
                                    0x1111_2222_3333_4444 ^ index as u64,
                                    0xAAAA_BBBB_CCCC_DDDD ^ index as u64,
                                ];
                                vcpu.regs.ymm_high[index] =
                                    [0x5555_5555_5555_5555 ^ index as u64; 2];
                                vcpu.regs.zmm_high[index] =
                                    [0xCCCC_CCCC_CCCC_CCCC ^ index as u64; 4];
                                vcpu.regs.zmm_ext[index] =
                                    [0xF0F0_F0F0_F0F0_F0F0 ^ index as u64; 8];
                            }
                            vcpu.regs.k[1] = 0xA55A_A55A_A55A_A55A;
                            vcpu.regs.k[2] = 0x5AA5_5AA5_5AA5_5AA5;
                            let before = vcpu.regs.clone();
                            let before_vectors: [[u64; 8]; 32] =
                                std::array::from_fn(|register| zmm(&vcpu, register as u8));
                            let source_value =
                                vcpu.get_reg(source, if elem_size == 8 { 8 } else { 4 });

                            assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");

                            let mut expected_bytes = [0u8; 64];
                            for (index, word) in
                                before_vectors[destination as usize].iter().enumerate()
                            {
                                expected_bytes[index * 8..index * 8 + 8]
                                    .copy_from_slice(&word.to_le_bytes());
                            }
                            let scalar = source_value.to_le_bytes();
                            let active_mask = if mask == 0 {
                                u64::MAX
                            } else {
                                before.k[mask as usize]
                            };
                            for lane in 0..(vl_bytes / elem_size) {
                                let base = lane * elem_size;
                                if (active_mask >> lane) & 1 != 0 {
                                    expected_bytes[base..base + elem_size]
                                        .copy_from_slice(&scalar[..elem_size]);
                                } else if zeroing {
                                    expected_bytes[base..base + elem_size].fill(0);
                                }
                            }
                            expected_bytes[vl_bytes..].fill(0);
                            let expected = std::array::from_fn(|index| {
                                u64::from_le_bytes(
                                    expected_bytes[index * 8..index * 8 + 8].try_into().unwrap(),
                                )
                            });
                            assert_eq!(zmm(&vcpu, destination), expected, "{code:02X?}");
                            assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
                            assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPR state");
                            assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}: MMX state");
                            assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
                            assert_eq!(vcpu.regs.rip, CODE + 6, "{code:02X?}: RIP");
                            for register in 0..32 {
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
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = long_mode_vcpu(code);
    for register in 0..16 {
        vcpu.set_reg(register, 0x1234_5678_9ABC_DEF0 ^ register as u64, 8);
    }
    vcpu.regs.xmm[2] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
    vcpu.regs.ymm_high[2] = [0xAAAA_AAAA_AAAA_AAAA; 2];
    vcpu.regs.zmm_high[2] = [0xBBBB_BBBB_BBBB_BBBB; 4];
    vcpu.regs.k =
        std::array::from_fn(|index| 0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32));
    let before = vcpu.regs.clone();

    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved GPR broadcast committed: {code:02X?}: {exit:?}"),
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
fn gpr_broadcast_reserved_fields_raise_precise_ud_without_commit() {
    for (opcode, w) in [(0x7A, false), (0x7B, false), (0x7C, false), (0x7C, true)] {
        let valid = encoding(opcode, w, 0, 2, 1, false, 0, false);
        let mut invalid = Vec::new();

        let mut vvvv = valid;
        vvvv[2] &= !0x08;
        invalid.push(vvvv);
        let mut v_prime = valid;
        v_prime[3] &= !0x08;
        invalid.push(v_prime);
        let mut embedded_broadcast = valid;
        embedded_broadcast[3] |= 0x10;
        invalid.push(embedded_broadcast);
        let mut reserved_ll = valid;
        reserved_ll[3] |= 0x60;
        invalid.push(reserved_ll);
        let mut zeroing_without_mask = valid;
        zeroing_without_mask[3] |= 0x80;
        invalid.push(zeroing_without_mask);
        let mut memory = valid;
        memory[5] &= 0x3F;
        invalid.push(memory);
        for pp in [0u8, 2, 3] {
            let mut wrong_pp = valid;
            wrong_pp[2] = (wrong_pp[2] & !0x03) | pp;
            invalid.push(wrong_pp);
        }

        if opcode != 0x7C {
            let mut wrong_w = valid;
            wrong_w[2] ^= 0x80;
            invalid.push(wrong_w);
        }

        for code in invalid {
            assert_reserved_ud(&code);
        }
    }
}
