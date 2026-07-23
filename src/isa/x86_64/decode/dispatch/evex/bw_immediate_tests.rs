//! Direct-execution regressions for EVEX VPALIGNR/VDBPSADBW.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x2000;

#[derive(Clone, Copy, Debug)]
enum BwImmediateOp {
    AlignRight,
    DoubleBlockSad,
}

impl BwImmediateOp {
    fn opcode(self) -> u8 {
        match self {
            Self::AlignRight => 0x0F,
            Self::DoubleBlockSad => 0x42,
        }
    }

    fn elem_size(self) -> usize {
        match self {
            Self::AlignRight => 1,
            Self::DoubleBlockSad => 2,
        }
    }

    fn immediates(self) -> &'static [u8] {
        match self {
            Self::AlignRight => &[0, 1, 3, 7, 15, 16, 17, 31, 32, 0xFF],
            Self::DoubleBlockSad => &[0, 1, 0x1B, 0x4E, 0xB1, 0xFF],
        }
    }
}

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
    op: BwImmediateOp,
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
        op.opcode(),
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
        let mut bytes = [0u8; 64];
        for (lane, byte) in bytes.iter_mut().enumerate() {
            *byte = (lane as u8)
                .wrapping_mul(0x31)
                .wrapping_add(register.wrapping_mul(0x17))
                ^ if lane % 7 == 2 { 0xA5 } else { 0 };
        }
        set_zmm(&mut vcpu, register, vector_words(bytes));
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

fn raw_result(
    op: BwImmediateOp,
    source1: &[u8; 64],
    source2: &[u8; 64],
    immediate: u8,
    vl_bytes: usize,
) -> [u8; 64] {
    let mut result = [0u8; 64];
    match op {
        BwImmediateOp::AlignRight => {
            for block_base in (0..vl_bytes).step_by(16) {
                let mut concatenated = [0u8; 32];
                concatenated[..16].copy_from_slice(&source2[block_base..block_base + 16]);
                concatenated[16..].copy_from_slice(&source1[block_base..block_base + 16]);
                for lane in 0..16 {
                    let source = immediate as usize + lane;
                    result[block_base + lane] = if source < 32 { concatenated[source] } else { 0 };
                }
            }
        }
        BwImmediateOp::DoubleBlockSad => {
            let mut shuffled = [0u8; 64];
            for lane_base in (0..vl_bytes).step_by(16) {
                for dword in 0..4 {
                    let selector = ((immediate >> (dword * 2)) & 3) as usize;
                    let source = lane_base + selector * 4;
                    let destination = lane_base + dword * 4;
                    shuffled[destination..destination + 4]
                        .copy_from_slice(&source2[source..source + 4]);
                }
            }
            for block_base in (0..vl_bytes).step_by(8) {
                for output in 0..4 {
                    let source1_base = block_base + (output / 2) * 4;
                    let shuffled_base = block_base + output;
                    let sad = (0..4)
                        .map(|byte| {
                            (source1[source1_base + byte] as i16
                                - shuffled[shuffled_base + byte] as i16)
                                .unsigned_abs()
                        })
                        .sum::<u16>();
                    let destination = block_base + output * 2;
                    result[destination..destination + 2].copy_from_slice(&sad.to_le_bytes());
                }
            }
        }
    }
    result
}

fn expected_result(
    op: BwImmediateOp,
    destination: &[u8; 64],
    source1: &[u8; 64],
    source2: &[u8; 64],
    mask: u64,
    zeroing: bool,
    immediate: u8,
    vl_bytes: usize,
) -> [u8; 64] {
    let elem_size = op.elem_size();
    let raw = raw_result(op, source1, source2, immediate, vl_bytes);
    let mut result = *destination;
    for lane in 0..(vl_bytes / elem_size) {
        let base = lane * elem_size;
        if (mask >> lane) & 1 != 0 {
            result[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if zeroing {
            result[base..base + elem_size].fill(0);
        }
    }
    result[vl_bytes..].fill(0);
    result
}

#[test]
fn bw_immediates_cover_shapes_wig_extensions_aliases_masks_and_selectors() {
    let operands = [
        (1u8, 2u8, 3u8),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (1, 1, 2),
        (1, 2, 1),
        (1, 1, 1),
    ];
    for op in [BwImmediateOp::AlignRight, BwImmediateOp::DoubleBlockSad] {
        let widths: &[bool] = match op {
            BwImmediateOp::AlignRight => &[false, true],
            BwImmediateOp::DoubleBlockSad => &[false],
        };
        for &w in widths {
            for (ll, vl_bytes) in [(0u8, 16usize), (1, 32), (2, 64)] {
                for (destination, source1, source2) in operands {
                    for (mask_register, zeroing) in [(0u8, false), (1, false), (2, true)] {
                        for &immediate in op.immediates() {
                            let code = encoding(
                                op,
                                w,
                                ll,
                                destination,
                                source1,
                                source2,
                                mask_register,
                                zeroing,
                                immediate,
                            );
                            let mut vcpu = initialized_vcpu(&code);
                            let before = vcpu.regs.clone();
                            let before_vectors: [[u64; 8]; 32] =
                                std::array::from_fn(|register| zmm(&vcpu, register as u8));
                            let active_mask = if mask_register == 0 {
                                u64::MAX
                            } else {
                                before.k[mask_register as usize]
                            };
                            let expected = expected_result(
                                op,
                                &vector_bytes(before_vectors[destination as usize]),
                                &vector_bytes(before_vectors[source1 as usize]),
                                &vector_bytes(before_vectors[source2 as usize]),
                                active_mask,
                                zeroing,
                                immediate,
                                vl_bytes,
                            );

                            assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                            assert_eq!(
                                zmm(&vcpu, destination),
                                vector_words(expected),
                                "{op:?} {code:02X?}"
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
}

#[test]
fn dbpsadbw_maximum_sad_is_1020() {
    let code = encoding(
        BwImmediateOp::DoubleBlockSad,
        false,
        2,
        1,
        2,
        3,
        0,
        false,
        0,
    );
    let mut vcpu = initialized_vcpu(&code);
    set_zmm(&mut vcpu, 2, [0; 8]);
    set_zmm(&mut vcpu, 3, [u64::MAX; 8]);

    assert!(vcpu.step().unwrap().is_none());
    assert_eq!(zmm(&vcpu, 1), [0x03FC_03FC_03FC_03FC; 8]);
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = initialized_vcpu(code);
    let before = vcpu.regs.clone();

    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved BW immediate committed: {code:02X?}: {exit:?}"),
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
fn bw_immediate_reserved_fields_raise_precise_ud_without_commit() {
    for op in [BwImmediateOp::AlignRight, BwImmediateOp::DoubleBlockSad] {
        let valid = encoding(op, false, 0, 1, 2, 3, 0, false, 0xFF);
        let mut invalid = Vec::new();

        let mut reserved_ll = valid;
        reserved_ll[3] |= 0x60;
        invalid.push(reserved_ll);
        let mut embedded_broadcast = valid;
        embedded_broadcast[3] |= 0x10;
        invalid.push(embedded_broadcast);
        let mut zeroing_without_mask = valid;
        zeroing_without_mask[3] |= 0x80;
        invalid.push(zeroing_without_mask);
        for pp in [0u8, 2, 3] {
            let mut wrong_pp = valid;
            wrong_pp[2] = (wrong_pp[2] & !0x03) | pp;
            invalid.push(wrong_pp);
        }
        if matches!(op, BwImmediateOp::DoubleBlockSad) {
            let mut wrong_w = valid;
            wrong_w[2] |= 0x80;
            invalid.push(wrong_w);
        }

        for code in invalid {
            assert_reserved_ud(&code);
        }
    }
}

fn write_memory_vector(vcpu: &mut X86_64Vcpu, bytes: &[u8; 64]) {
    for (index, chunk) in bytes.chunks_exact(8).enumerate() {
        vcpu.write_mem(
            DATA + (index * 8) as u64,
            u64::from_le_bytes(chunk.try_into().unwrap()),
            8,
        )
        .unwrap();
    }
}

#[test]
fn bw_immediate_memory_sources_remain_supported() {
    for op in [BwImmediateOp::AlignRight, BwImmediateOp::DoubleBlockSad] {
        let w = matches!(op, BwImmediateOp::AlignRight);
        let mut code = encoding(op, w, 2, 1, 2, 0, 1, false, 0x1B);
        code[5] &= 0x3F;
        let mut vcpu = initialized_vcpu(&code);
        vcpu.regs.rax = DATA;
        let memory = vector_bytes([
            0x800F_0E0D_0C0B_0A09,
            0x0807_0605_0403_0201,
            0x7F80_FF00_0102_FE81,
            0x55AA_33CC_0FF0_6996,
            0xFEDC_BA98_7654_3210,
            0x0123_4567_89AB_CDEF,
            0x8080_7F7F_FFFF_0000,
            0x8000_8000_7FFF_7FFF,
        ]);
        write_memory_vector(&mut vcpu, &memory);
        let before = vcpu.regs.clone();
        let destination = vector_bytes(zmm(&vcpu, 1));
        let source1 = vector_bytes(zmm(&vcpu, 2));
        let expected = expected_result(
            op,
            &destination,
            &source1,
            &memory,
            before.k[1],
            false,
            0x1B,
            64,
        );

        assert!(vcpu.step().unwrap().is_none(), "{op:?} {code:02X?}");
        assert_eq!(zmm(&vcpu, 1), vector_words(expected), "{op:?}");
        assert_eq!(vcpu.regs.rip, CODE + 7, "{op:?}");
    }
}
