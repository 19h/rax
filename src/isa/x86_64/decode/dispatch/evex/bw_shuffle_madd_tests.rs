//! Direct-execution regressions for EVEX VPSHUFB/VPMADDUBSW/VPMADDWD.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x2000;

#[derive(Clone, Copy, Debug)]
enum BwOp {
    ShuffleBytes,
    MaddUnsignedSignedBytes,
    MaddSignedWords,
}

impl BwOp {
    fn encoding(self) -> (u8, u8, usize) {
        match self {
            Self::ShuffleBytes => (2, 0x00, 1),
            Self::MaddUnsignedSignedBytes => (2, 0x04, 2),
            Self::MaddSignedWords => (1, 0xF5, 4),
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
    op: BwOp,
    w: bool,
    ll: u8,
    destination: u8,
    source1: u8,
    source2: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (map, opcode, _) = op.encoding();
    assert!(destination < 32 && source1 < 32 && source2 < 32 && ll < 3 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF0 | map;
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
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source2 & 0x07),
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
            let base = (lane as u8)
                .wrapping_mul(0x31)
                .wrapping_add(register.wrapping_mul(0x17));
            *byte = base ^ if lane % 5 == 1 { 0x80 } else { 0 };
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

fn raw_result(op: BwOp, source1: &[u8; 64], source2: &[u8; 64], vl_bytes: usize) -> [u8; 64] {
    let mut result = [0u8; 64];
    match op {
        BwOp::ShuffleBytes => {
            for lane in 0..vl_bytes {
                let control = source2[lane];
                result[lane] = if control & 0x80 != 0 {
                    0
                } else {
                    source1[(lane & !0x0F) | (control as usize & 0x0F)]
                };
            }
        }
        BwOp::MaddUnsignedSignedBytes => {
            for lane in 0..(vl_bytes / 2) {
                let base = lane * 2;
                let sum = source1[base] as i32 * source2[base] as i8 as i32
                    + source1[base + 1] as i32 * source2[base + 1] as i8 as i32;
                let value = sum.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                result[base..base + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        BwOp::MaddSignedWords => {
            for lane in 0..(vl_bytes / 4) {
                let base = lane * 4;
                let a0 = i16::from_le_bytes(source1[base..base + 2].try_into().unwrap()) as i32;
                let a1 = i16::from_le_bytes(source1[base + 2..base + 4].try_into().unwrap()) as i32;
                let b0 = i16::from_le_bytes(source2[base..base + 2].try_into().unwrap()) as i32;
                let b1 = i16::from_le_bytes(source2[base + 2..base + 4].try_into().unwrap()) as i32;
                let value = a0.wrapping_mul(b0).wrapping_add(a1.wrapping_mul(b1));
                result[base..base + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    result
}

fn expected_result(
    op: BwOp,
    destination: &[u8; 64],
    source1: &[u8; 64],
    source2: &[u8; 64],
    mask: u64,
    zeroing: bool,
    vl_bytes: usize,
) -> [u8; 64] {
    let (_, _, elem_size) = op.encoding();
    let raw = raw_result(op, source1, source2, vl_bytes);
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
fn bw_shuffle_madd_covers_shapes_wig_extensions_aliases_masks_and_edges() {
    let operands = [
        (1u8, 2u8, 3u8),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (1, 1, 2),
        (1, 2, 1),
        (1, 1, 1),
    ];
    for op in [
        BwOp::ShuffleBytes,
        BwOp::MaddUnsignedSignedBytes,
        BwOp::MaddSignedWords,
    ] {
        for w in [false, true] {
            for (ll, vl_bytes) in [(0u8, 16usize), (1, 32), (2, 64)] {
                for (destination, source1, source2) in operands {
                    for (mask_register, zeroing) in [(0u8, false), (1, false), (2, true)] {
                        let code = encoding(
                            op,
                            w,
                            ll,
                            destination,
                            source1,
                            source2,
                            mask_register,
                            zeroing,
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
                        assert_eq!(vcpu.regs.rip, CODE + 6, "{code:02X?}: RIP");
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

#[test]
fn madd_saturation_and_wrap_boundaries_are_exact() {
    let cases = [
        (
            BwOp::MaddUnsignedSignedBytes,
            [0xFFFF_FFFF_FFFF_FFFFu64; 8],
            [0x8080_7F7F_8080_7F7Fu64; 8],
            [0x8000_7FFF_8000_7FFFu64; 8],
        ),
        (
            BwOp::MaddSignedWords,
            [0x8000_8000_8000_8000u64; 8],
            [0x8000_8000_8000_8000u64; 8],
            [0x8000_0000_8000_0000u64; 8],
        ),
    ];
    for (op, source1, source2, expected) in cases {
        let code = encoding(op, true, 2, 1, 2, 3, 0, false);
        let mut vcpu = initialized_vcpu(&code);
        set_zmm(&mut vcpu, 2, source1);
        set_zmm(&mut vcpu, 3, source2);

        assert!(vcpu.step().unwrap().is_none(), "{op:?}");
        assert_eq!(zmm(&vcpu, 1), expected, "{op:?}");
    }
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = initialized_vcpu(code);
    let before = vcpu.regs.clone();

    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved BW shuffle/madd committed: {code:02X?}: {exit:?}"),
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
fn bw_shuffle_madd_reserved_fields_raise_precise_ud_without_commit() {
    for op in [
        BwOp::ShuffleBytes,
        BwOp::MaddUnsignedSignedBytes,
        BwOp::MaddSignedWords,
    ] {
        for w in [false, true] {
            let valid = encoding(op, w, 0, 1, 2, 3, 0, false);
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

            for code in invalid {
                assert_reserved_ud(&code);
            }
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
fn bw_shuffle_madd_memory_sources_remain_supported() {
    for op in [
        BwOp::ShuffleBytes,
        BwOp::MaddUnsignedSignedBytes,
        BwOp::MaddSignedWords,
    ] {
        let mut code = encoding(op, true, 2, 1, 2, 0, 1, false);
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
        let expected = expected_result(op, &destination, &source1, &memory, before.k[1], false, 64);

        assert!(vcpu.step().unwrap().is_none(), "{op:?} {code:02X?}");
        assert_eq!(zmm(&vcpu, 1), vector_words(expected), "{op:?}");
        assert_eq!(vcpu.regs.rip, CODE + 6, "{op:?}");
    }
}
