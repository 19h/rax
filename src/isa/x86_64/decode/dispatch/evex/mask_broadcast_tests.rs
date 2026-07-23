//! Direct-execution regressions for EVEX VPBROADCASTMB2Q/MW2D.

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
    ignored_b: bool,
) -> [u8; 6] {
    assert!(destination < 32 && source < 8 && ll < 3);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if ignored_b {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        0x7E | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | source,
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
fn mask_broadcast_expands_every_shape_extension_and_k_source() {
    for (opcode, w, source_bits, elem_size) in [(0x2A, true, 8usize, 8usize), (0x3A, false, 16, 4)]
    {
        for (ll, vl_bytes) in [(0u8, 16usize), (1, 32), (2, 64)] {
            for destination in [1u8, 9, 17, 25] {
                for source in [0u8, 3, 7] {
                    for (ignored_x, ignored_b) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        let code =
                            encoding(opcode, w, ll, destination, source, ignored_x, ignored_b);
                        let mut vcpu = long_mode_vcpu(&code);
                        for index in 0..16 {
                            vcpu.regs.xmm[index] = [
                                0x1111_2222_3333_4444 ^ index as u64,
                                0xAAAA_BBBB_CCCC_DDDD ^ index as u64,
                            ];
                            vcpu.regs.ymm_high[index] = [0x5555_5555_5555_5555 ^ index as u64; 2];
                            vcpu.regs.zmm_high[index] = [0xCCCC_CCCC_CCCC_CCCC ^ index as u64; 4];
                            vcpu.regs.zmm_ext[index] = [0xF0F0_F0F0_F0F0_F0F0 ^ index as u64; 8];
                        }
                        vcpu.regs.k = std::array::from_fn(|index| {
                            0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32)
                        });
                        let before = vcpu.regs.clone();
                        let before_vectors: [[u64; 8]; 32] =
                            std::array::from_fn(|register| zmm(&vcpu, register as u8));
                        let source_mask = before.k[source as usize];

                        assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");

                        let source_mask =
                            source_mask & if source_bits == 8 { 0xFF } else { 0xFFFF };
                        let scalar_bytes = source_mask.to_le_bytes();
                        let mut expected_bytes = [0u8; 64];
                        for lane in 0..(vl_bytes / elem_size) {
                            expected_bytes[lane * elem_size..(lane + 1) * elem_size]
                                .copy_from_slice(&scalar_bytes[..elem_size]);
                        }
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

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = long_mode_vcpu(code);
    vcpu.regs.xmm[2] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
    vcpu.regs.ymm_high[2] = [0xAAAA_AAAA_AAAA_AAAA; 2];
    vcpu.regs.zmm_high[2] = [0xBBBB_BBBB_BBBB_BBBB; 4];
    vcpu.regs.k =
        std::array::from_fn(|index| 0xA55A_3CC3_F00F_9669u64.rotate_left((index * 7) as u32));
    let before = vcpu.regs.clone();

    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved mask broadcast committed: {code:02X?}: {exit:?}"),
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
fn mask_broadcast_reserved_fields_raise_precise_ud() {
    for (opcode, w) in [(0x2A, true), (0x3A, false)] {
        let valid = encoding(opcode, w, 0, 2, 1, false, false);
        let mut invalid = Vec::new();

        let mut vvvv = valid;
        vvvv[2] &= !0x08;
        invalid.push(vvvv);
        let mut v_prime = valid;
        v_prime[3] &= !0x08;
        invalid.push(v_prime);
        let mut writemask = valid;
        writemask[3] |= 0x01;
        invalid.push(writemask);
        let mut zeroing = valid;
        zeroing[3] |= 0x80;
        invalid.push(zeroing);
        let mut broadcast = valid;
        broadcast[3] |= 0x10;
        invalid.push(broadcast);
        let mut reserved_ll = valid;
        reserved_ll[3] |= 0x60;
        invalid.push(reserved_ll);
        let mut memory = valid;
        memory[5] &= 0x3F;
        invalid.push(memory);

        for code in invalid {
            assert_reserved_ud(&code);
        }

        let mut wrong_w = valid;
        wrong_w[2] ^= 0x80;
        assert_reserved_ud(&wrong_w);
    }
}
