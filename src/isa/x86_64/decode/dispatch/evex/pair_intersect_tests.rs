//! Direct-execution regressions for EVEX VP2INTERSECTD/Q.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::Registers;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x4000;

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
    vcpu.set_vp2intersect_enabled(true);
    vcpu
}

fn encoding(w: bool, ll: u8, destination: u8, source1: u8, source2: u8) -> [u8; 6] {
    assert!(ll < 3 && destination < 8 && source1 < 32 && source2 < 32);
    let mut p0 = 0xF2;
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    let p1 = if w { 0x80 } else { 0 } | (((source1 & 0x0F) ^ 0x0F) << 3) | 0x07;
    let p2 = (ll << 5) | if source1 & 0x10 == 0 { 0x08 } else { 0 };
    [
        0x62,
        p0,
        p1,
        p2,
        0x68,
        0xC0 | (destination << 3) | (source2 & 0x07),
    ]
}

fn memory_encoding(
    w: bool,
    ll: u8,
    destination: u8,
    source1: u8,
    broadcast: bool,
    displacement: i8,
) -> [u8; 7] {
    let register = encoding(w, ll, destination, source1, 0);
    let mut memory = [0u8; 7];
    memory[..5].copy_from_slice(&register[..5]);
    memory[3] |= if broadcast { 0x10 } else { 0 };
    memory[5] = 0x40 | (destination << 3);
    memory[6] = displacement as u8;
    memory
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

fn vector_from_elements(elements: &[u64], elem_size: usize) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    for (lane, value) in elements.iter().enumerate() {
        let base = lane * elem_size;
        bytes[base..base + elem_size].copy_from_slice(&value.to_le_bytes()[..elem_size]);
    }
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
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
            std::array::from_fn(|word| {
                0x0123_4567_89AB_CDEFu64.rotate_left((register as usize * 7 + word * 11) as u32)
                    ^ ((register as u64) << 56)
                    ^ word as u64
            }),
        );
    }
    for register in 0u8..16 {
        vcpu.set_reg(
            register,
            0xFEDC_BA98_7654_3210u64.rotate_left((register * 5) as u32),
            8,
        );
    }
    vcpu.regs.k = std::array::from_fn(|index| {
        0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32) ^ (1u64 << index)
    });
    vcpu.regs.mm = std::array::from_fn(|index| 0x8877_6655_4433_2211u64.rotate_left(index as u32));
    vcpu.mxcsr = 0x1F80 | 0x3F;
    vcpu
}

fn expected_masks(source1: &[u64], source2: &[u64]) -> (u64, u64) {
    let mut mask1 = 0u64;
    let mut mask2 = 0u64;
    for (lane1, value1) in source1.iter().enumerate() {
        for (lane2, value2) in source2.iter().enumerate() {
            if value1 == value2 {
                mask1 |= 1 << lane1;
                mask2 |= 1 << lane2;
            }
        }
    }
    (mask1, mask2)
}

fn assert_non_mask_state_preserved(vcpu: &X86_64Vcpu, before: &Registers, mxcsr: u32) {
    assert_eq!(vcpu.regs.xmm, before.xmm, "XMM state");
    assert_eq!(vcpu.regs.ymm_high, before.ymm_high, "YMM state");
    assert_eq!(vcpu.regs.zmm_high, before.zmm_high, "ZMM state");
    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "ZMM16-31 state");
    assert_eq!(gprs(&vcpu.regs), gprs(before), "GPR state");
    assert_eq!(vcpu.regs.mm, before.mm, "MMX state");
    assert_eq!(vcpu.regs.rflags, before.rflags, "RFLAGS");
    assert_eq!(vcpu.mxcsr, mxcsr, "MXCSR");
}

#[test]
fn pair_intersect_odd_destination_aliases_preceding_even_pair() {
    let code = encoding(false, 0, 1, 3, 4);
    let mut vcpu = long_mode_vcpu(&code);
    set_zmm(
        &mut vcpu,
        3,
        [
            0x0000_0002_0000_0001,
            0x0000_0004_0000_0003,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
    );
    set_zmm(
        &mut vcpu,
        4,
        [
            0x0000_0002_0000_0009,
            0x0000_0004_0000_0008,
            0,
            0,
            0,
            0,
            0,
            0,
        ],
    );
    vcpu.regs.k = [
        0x1111_1111_1111_1111,
        0x2222_2222_2222_2222,
        0x3333_3333_3333_3333,
        0x4444_4444_4444_4444,
        0x5555_5555_5555_5555,
        0x6666_6666_6666_6666,
        0x7777_7777_7777_7777,
        0x8888_8888_8888_8888,
    ];

    assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
    assert_eq!(
        vcpu.regs.k,
        [
            0b1010,
            0b1010,
            0x3333_3333_3333_3333,
            0x4444_4444_4444_4444,
            0x5555_5555_5555_5555,
            0x6666_6666_6666_6666,
            0x7777_7777_7777_7777,
            0x8888_8888_8888_8888,
        ],
        "Intel SDM masks off ModR/M.reg bit 0 when selecting the pair"
    );
    assert_eq!(vcpu.regs.rip, CODE + code.len() as u64);
}

#[test]
fn pair_intersect_covers_all_shapes_destinations_extensions_aliases_and_matches() {
    let operand_sets = [(3u8, 4u8), (11, 12), (19, 20), (27, 28), (3, 3)];
    let mut executed = 0usize;

    for w in [false, true] {
        let elem_size = if w { 8usize } else { 4 };
        for ll in 0u8..=2 {
            let lanes = [16usize, 32, 64][ll as usize] / elem_size;
            for destination in 0u8..8 {
                for (source1_register, source2_register) in operand_sets {
                    let code = encoding(w, ll, destination, source1_register, source2_register);
                    let mut vcpu = initialized_vcpu(&code);
                    let source1: Vec<u64> =
                        (0..lanes).map(|lane| 0x1000 + lane as u64 * 3).collect();
                    let mut source2: Vec<u64> =
                        (0..lanes).map(|lane| 0x8000 + lane as u64 * 5).collect();
                    source2[0] = source1[lanes - 1];
                    source2[lanes - 1] = source1[0];
                    if lanes > 2 {
                        source2[1] = source1[0];
                    }
                    if source1_register == source2_register {
                        source2.clone_from(&source1);
                    }
                    set_zmm(
                        &mut vcpu,
                        source1_register,
                        vector_from_elements(&source1, elem_size),
                    );
                    set_zmm(
                        &mut vcpu,
                        source2_register,
                        vector_from_elements(&source2, elem_size),
                    );
                    let source1 = if source1_register == source2_register {
                        source2.clone()
                    } else {
                        source1
                    };
                    let before = vcpu.regs.clone();
                    let mxcsr = vcpu.mxcsr;
                    let mut expected = before.k;
                    let pair = usize::from(destination & 0x06);
                    (expected[pair], expected[pair + 1]) = expected_masks(&source1, &source2);

                    assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                    assert_eq!(vcpu.regs.k, expected, "{code:02X?}");
                    assert_non_mask_state_preserved(&vcpu, &before, mxcsr);
                    assert_eq!(vcpu.regs.rip, CODE + code.len() as u64, "{code:02X?}");
                    executed += 1;
                }
            }
        }
    }

    assert_eq!(executed, 240);
}

#[test]
fn pair_intersect_memory_uses_full_and_broadcast_compressed_disp8_scales() {
    for w in [false, true] {
        let elem_size = if w { 8usize } else { 4 };
        for ll in 0u8..=2 {
            let vl_bytes = [16usize, 32, 64][ll as usize];
            let lanes = vl_bytes / elem_size;
            for broadcast in [false, true] {
                for displacement in [-2i8, 2] {
                    let code = memory_encoding(w, ll, 7, 17, broadcast, displacement);
                    let mut vcpu = initialized_vcpu(&code);
                    vcpu.regs.rax = DATA;
                    let source1: Vec<u64> =
                        (0..lanes).map(|lane| 0x4000 + lane as u64 * 7).collect();
                    set_zmm(&mut vcpu, 17, vector_from_elements(&source1, elem_size));

                    let source2 = if broadcast {
                        vec![source1[lanes / 2]; lanes]
                    } else {
                        let mut values: Vec<u64> =
                            (0..lanes).map(|lane| 0x9000 + lane as u64 * 11).collect();
                        values[0] = source1[lanes - 1];
                        values[lanes - 1] = source1[0];
                        values
                    };
                    let tuple_scale = if broadcast { elem_size } else { vl_bytes };
                    let address = (DATA as i64 + displacement as i64 * tuple_scale as i64) as u64;
                    if broadcast {
                        vcpu.write_mem(address, source2[0], elem_size as u8)
                            .unwrap();
                    } else {
                        for (lane, value) in source2.iter().enumerate() {
                            vcpu.write_mem(
                                address + (lane * elem_size) as u64,
                                *value,
                                elem_size as u8,
                            )
                            .unwrap();
                        }
                    }

                    let before = vcpu.regs.clone();
                    let mxcsr = vcpu.mxcsr;
                    let mut expected = before.k;
                    (expected[6], expected[7]) = expected_masks(&source1, &source2);

                    assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                    assert_eq!(vcpu.regs.k, expected, "{code:02X?}");
                    assert_non_mask_state_preserved(&vcpu, &before, mxcsr);
                    assert_eq!(vcpu.regs.rip, CODE + code.len() as u64, "{code:02X?}");
                }
            }
        }
    }
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = initialized_vcpu(code);
    vcpu.regs.rax = 0x2_0000;
    let before = vcpu.regs.clone();
    let mxcsr = vcpu.mxcsr;

    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved VP2INTERSECT committed: {code:02X?}: {exit:?}"),
    };
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "wrong exception for {code:02X?}: {error:?}"
    );
    assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
    assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
    assert_non_mask_state_preserved(&vcpu, &before, mxcsr);
}

#[test]
fn pair_intersect_reserved_fields_and_disabled_feature_raise_precise_ud() {
    for w in [false, true] {
        let valid = encoding(w, 0, 2, 17, 19);
        let mut invalid = Vec::new();

        let mut reserved_ll = valid;
        reserved_ll[3] |= 0x60;
        invalid.push(reserved_ll);
        let mut masking = valid;
        masking[3] |= 0x01;
        invalid.push(masking);
        let mut zeroing = valid;
        zeroing[3] |= 0x80;
        invalid.push(zeroing);
        let mut register_broadcast = valid;
        register_broadcast[3] |= 0x10;
        invalid.push(register_broadcast);
        let mut destination_r = valid;
        destination_r[1] &= !0x80;
        invalid.push(destination_r);
        let mut destination_r_prime = valid;
        destination_r_prime[1] &= !0x10;
        invalid.push(destination_r_prime);
        let mut wrong_pp = valid;
        wrong_pp[2] = (wrong_pp[2] & !0x03) | 1;
        invalid.push(wrong_pp);
        for code in invalid {
            assert_reserved_ud(&code);
        }

        let mut extended_memory_destination = memory_encoding(w, 2, 2, 17, false, 1);
        extended_memory_destination[1] &= !0x80;
        assert_reserved_ud(&extended_memory_destination);

        let mut vcpu = initialized_vcpu(&valid);
        vcpu.set_vp2intersect_enabled(false);
        let before = vcpu.regs.clone();
        let error = vcpu.step().expect_err("disabled VP2INTERSECT must #UD");
        assert!(format!("{error:?}").contains("IDT entry 6 not present"));
        assert_eq!(vcpu.regs.k, before.k);
        assert_eq!(vcpu.regs.rip, before.rip);
    }
}

#[test]
fn pair_intersect_memory_fault_is_precise_and_noncommitting() {
    let code = memory_encoding(false, 2, 3, 17, false, 0);
    let mut vcpu = initialized_vcpu(&code);
    vcpu.regs.rax = 0xFFF0;
    let before = vcpu.regs.clone();
    let mxcsr = vcpu.mxcsr;

    assert!(vcpu.step().is_err(), "cross-boundary source must fault");
    assert_eq!(vcpu.regs.k, before.k);
    assert_eq!(vcpu.regs.rip, before.rip);
    assert_non_mask_state_preserved(&vcpu, &before, mxcsr);
}
