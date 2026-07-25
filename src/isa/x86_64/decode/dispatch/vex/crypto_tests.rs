//! Direct-execution regressions for VEX VPCLMULQDQ and GFNI.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CODE: u64 = 0x1000;
const OPERANDS: [(u8, u8, u8); 8] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 3),
    (1, 2, 1),
    (1, 2, 2),
    (9, 9, 11),
    (9, 10, 9),
    (15, 15, 15),
];
const GFNI_OPERANDS: [(u8, u8, u8); 10] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 3),
    (1, 2, 1),
    (1, 2, 2),
    (9, 9, 11),
    (9, 10, 9),
    (15, 15, 15),
    (15, 8, 13),
    (13, 14, 15),
];

#[derive(Clone, Copy, Debug)]
enum GfniKind {
    Multiply,
    Affine,
    AffineInverse,
}

fn encoding(
    w: bool,
    ymm: bool,
    destination: u8,
    source1: u8,
    source2: u8,
    clear_ignored_x: bool,
    immediate: u8,
) -> [u8; 6] {
    assert!(destination < 16 && source1 < 16 && source2 < 16);
    let mut p0 = 0xE3;
    if destination >= 8 {
        p0 &= !0x80;
    }
    if clear_ignored_x {
        p0 &= !0x40;
    }
    if source2 >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (u8::from(ymm) << 2) | 1,
        0x44,
        0xC0 | ((destination & 7) << 3) | (source2 & 7),
        immediate,
    ]
}

fn gfni_encoding(
    kind: GfniKind,
    ymm: bool,
    destination: u8,
    source1: u8,
    source2: u8,
    clear_ignored_x: bool,
    immediate: u8,
) -> Vec<u8> {
    assert!(destination < 16 && source1 < 16 && source2 < 16);
    let (map, w, opcode, has_immediate) = match kind {
        GfniKind::Multiply => (2, false, 0xCF, false),
        GfniKind::Affine => (3, true, 0xCE, true),
        GfniKind::AffineInverse => (3, true, 0xCF, true),
    };
    let mut p0 = 0xE0 | map;
    if destination >= 8 {
        p0 &= !0x80;
    }
    if clear_ignored_x {
        p0 &= !0x40;
    }
    if source2 >= 8 {
        p0 &= !0x20;
    }
    let mut bytes = vec![
        0xC4,
        p0,
        (u8::from(w) << 7) | (((!source1) & 0x0F) << 3) | (u8::from(ymm) << 2) | 1,
        opcode,
        0xC0 | ((destination & 7) << 3) | (source2 & 7),
    ];
    if has_immediate {
        bytes.push(immediate);
    }
    bytes
}

fn long_mode_vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn zmm(vcpu: &X86_64Vcpu, register: u8) -> [u64; 8] {
    let index = usize::from(register);
    let mut value = [0u64; 8];
    value[..2].copy_from_slice(&vcpu.regs.xmm[index]);
    value[2..4].copy_from_slice(&vcpu.regs.ymm_high[index]);
    value[4..].copy_from_slice(&vcpu.regs.zmm_high[index]);
    value
}

fn set_zmm(vcpu: &mut X86_64Vcpu, register: u8, value: [u64; 8]) {
    let index = usize::from(register);
    vcpu.regs.xmm[index].copy_from_slice(&value[..2]);
    vcpu.regs.ymm_high[index].copy_from_slice(&value[2..4]);
    vcpu.regs.zmm_high[index].copy_from_slice(&value[4..]);
}

fn gprs(registers: &Registers) -> [u64; 32] {
    [
        registers.rax,
        registers.rcx,
        registers.rdx,
        registers.rbx,
        registers.rsp,
        registers.rbp,
        registers.rsi,
        registers.rdi,
        registers.r8,
        registers.r9,
        registers.r10,
        registers.r11,
        registers.r12,
        registers.r13,
        registers.r14,
        registers.r15,
        registers.r16,
        registers.r17,
        registers.r18,
        registers.r19,
        registers.r20,
        registers.r21,
        registers.r22,
        registers.r23,
        registers.r24,
        registers.r25,
        registers.r26,
        registers.r27,
        registers.r28,
        registers.r29,
        registers.r30,
        registers.r31,
    ]
}

fn initialized_vcpu(code: &[u8], ordinal: usize) -> X86_64Vcpu {
    let mut vcpu = long_mode_vcpu(code);
    for register in 0u8..16 {
        set_zmm(
            &mut vcpu,
            register,
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3u64.rotate_left(
                    ((ordinal * 3 + usize::from(register) * 11 + word * 17) & 63) as u32,
                ) ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ u64::from(register).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            }),
        );
    }
    for register in 0u8..16 {
        vcpu.set_reg(
            register,
            0x0123_4567_89AB_CDEFu64.rotate_left((u32::from(register) * 7) & 63)
                ^ (ordinal as u64).wrapping_mul(0x1020_4081_0204_0810),
            8,
        );
    }
    vcpu.regs.k =
        std::array::from_fn(|register| 0x6996_F00F_3CC3_A55Au64.rotate_left((register * 7) as u32));
    vcpu.regs.mm = std::array::from_fn(|register| 0x8877_6655_4433_2211 ^ register as u64);
    vcpu.regs.rflags = 0x2 | 0x0CD5;
    vcpu.mxcsr = [
        0x1F80,
        0x1F80 | 0x15,
        0x1F80 | (1 << 13) | (1 << 6),
        0x1F80 | (3 << 13) | (1 << 15),
    ][ordinal % 4];
    vcpu
}

fn clmul(a: u64, b: u64) -> [u64; 2] {
    let mut product = 0u128;
    for bit in 0..64 {
        if b & (1u64 << bit) != 0 {
            product ^= u128::from(a) << bit;
        }
    }
    [product as u64, (product >> 64) as u64]
}

fn expected(
    vectors: &[[u64; 8]; 16],
    ymm: bool,
    source1: u8,
    source2: u8,
    immediate: u8,
) -> [u64; 8] {
    let mut result = [0u64; 8];
    let blocks = if ymm { 2 } else { 1 };
    for block in 0..blocks {
        let first = vectors[usize::from(source1)][block * 2 + usize::from(immediate & 1)];
        let second = vectors[usize::from(source2)][block * 2 + usize::from((immediate >> 4) & 1)];
        result[block * 2..block * 2 + 2].copy_from_slice(&clmul(first, second));
    }
    result
}

fn assert_case(code: [u8; 6], ordinal: usize, destination: u8, source1: u8, source2: u8) {
    let mut vcpu = initialized_vcpu(&code, ordinal);
    let before_vectors: [[u64; 8]; 16] = std::array::from_fn(|register| zmm(&vcpu, register as u8));
    let before_gprs = gprs(&vcpu.regs);
    let before_flags = vcpu.regs.rflags;
    let before_masks = vcpu.regs.k;
    let before_mmx = vcpu.regs.mm;
    let before_mxcsr = vcpu.mxcsr;
    let ymm = code[2] & 0x04 != 0;
    let immediate = code[5];

    assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");

    let result = expected(&before_vectors, ymm, source1, source2, immediate);
    for register in 0u8..16 {
        let expected = if register == destination {
            result
        } else {
            before_vectors[usize::from(register)]
        };
        assert_eq!(zmm(&vcpu, register), expected, "zmm{register} {code:02X?}");
    }
    assert_eq!(gprs(&vcpu.regs), before_gprs, "{code:02X?}");
    assert_eq!(vcpu.regs.rflags, before_flags, "{code:02X?}");
    assert_eq!(vcpu.regs.k, before_masks, "{code:02X?}");
    assert_eq!(vcpu.regs.mm, before_mmx, "{code:02X?}");
    assert_eq!(vcpu.mxcsr, before_mxcsr, "{code:02X?}");
    assert_eq!(vcpu.regs.rip, CODE + code.len() as u64, "{code:02X?}");
}

fn gf_mul_reference(a: u8, b: u8) -> u8 {
    let mut product = 0u16;
    for bit in 0..8 {
        if b & (1 << bit) != 0 {
            product ^= u16::from(a) << bit;
        }
    }
    for degree in (8..=14).rev() {
        if product & (1 << degree) != 0 {
            product ^= 0x11B << (degree - 8);
        }
    }
    product as u8
}

fn gf_inverse_reference(value: u8) -> u8 {
    let mut result = 1u8;
    let mut power = value;
    let mut exponent = 254u8;
    while exponent != 0 {
        if exponent & 1 != 0 {
            result = gf_mul_reference(result, power);
        }
        power = gf_mul_reference(power, power);
        exponent >>= 1;
    }
    result
}

fn vector_byte(vector: &[u64; 8], index: usize) -> u8 {
    (vector[index / 8] >> ((index % 8) * 8)) as u8
}

fn gfni_expected(
    vectors: &[[u64; 8]; 16],
    kind: GfniKind,
    ymm: bool,
    source1: u8,
    source2: u8,
    immediate: u8,
) -> [u64; 8] {
    let mut result = [0u64; 8];
    let bytes = if ymm { 32 } else { 16 };
    for lane in 0..bytes {
        let input = vector_byte(&vectors[usize::from(source1)], lane);
        let output = match kind {
            GfniKind::Multiply => {
                gf_mul_reference(input, vector_byte(&vectors[usize::from(source2)], lane))
            }
            GfniKind::Affine | GfniKind::AffineInverse => {
                let input = if matches!(kind, GfniKind::AffineInverse) {
                    gf_inverse_reference(input)
                } else {
                    input
                };
                let qword_base = lane & !7;
                let mut output = 0u8;
                for bit in 0..8 {
                    let matrix_row =
                        vector_byte(&vectors[usize::from(source2)], qword_base + 7 - bit);
                    let parity = (matrix_row & input).count_ones() as u8 & 1;
                    output |= (parity ^ ((immediate >> bit) & 1)) << bit;
                }
                output
            }
        };
        result[lane / 8] |= u64::from(output) << ((lane % 8) * 8);
    }
    result
}

fn assert_gfni_case(
    code: Vec<u8>,
    ordinal: usize,
    kind: GfniKind,
    destination: u8,
    source1: u8,
    source2: u8,
) {
    let mut vcpu = initialized_vcpu(&code, ordinal);
    let before_vectors: [[u64; 8]; 16] = std::array::from_fn(|register| zmm(&vcpu, register as u8));
    let before_gprs = gprs(&vcpu.regs);
    let before_flags = vcpu.regs.rflags;
    let before_masks = vcpu.regs.k;
    let before_mmx = vcpu.regs.mm;
    let before_mxcsr = vcpu.mxcsr;
    let ymm = code[2] & 0x04 != 0;
    let immediate = code.get(5).copied().unwrap_or(0);

    assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");

    let result = gfni_expected(&before_vectors, kind, ymm, source1, source2, immediate);
    for register in 0u8..16 {
        let expected = if register == destination {
            result
        } else {
            before_vectors[usize::from(register)]
        };
        assert_eq!(zmm(&vcpu, register), expected, "zmm{register} {code:02X?}");
    }
    assert_eq!(gprs(&vcpu.regs), before_gprs, "{code:02X?}");
    assert_eq!(vcpu.regs.rflags, before_flags, "{code:02X?}");
    assert_eq!(vcpu.regs.k, before_masks, "{code:02X?}");
    assert_eq!(vcpu.regs.mm, before_mmx, "{code:02X?}");
    assert_eq!(vcpu.mxcsr, before_mxcsr, "{code:02X?}");
    assert_eq!(vcpu.regs.rip, CODE + code.len() as u64, "{code:02X?}");
}

#[test]
fn direct_vex_vpclmulqdq_accepts_wig_w1() {
    let code = encoding(true, false, 9, 10, 11, true, 0xEF);
    assert_case(code, 0, 9, 10, 11);
}

#[test]
fn direct_vex_vpclmulqdq_zeroes_every_bit_above_vl() {
    for (ordinal, ymm) in [false, true].into_iter().enumerate() {
        let code = encoding(false, ymm, 1, 2, 3, false, 0x11);
        assert_case(code, ordinal, 1, 2, 3);
    }
}

#[test]
fn direct_vex_vpclmulqdq_covers_all_immediates_widths_wig_extensions_and_aliases() {
    let mut tested = 0usize;
    for immediate in u8::MIN..=u8::MAX {
        for ymm in [false, true] {
            for w in [false, true] {
                let (destination, source1, source2) = OPERANDS[tested % OPERANDS.len()];
                let code = encoding(
                    w,
                    ymm,
                    destination,
                    source1,
                    source2,
                    tested & 1 != 0,
                    immediate,
                );
                assert_case(code, tested, destination, source1, source2);
                tested += 1;
            }
        }
    }
    assert_eq!(tested, 1_024);
}

#[test]
fn direct_vex_gfni_matches_llvm_encodings_and_independent_field_equations() {
    assert_eq!(
        gfni_encoding(GfniKind::Multiply, false, 9, 10, 11, false, 0),
        [0xC4, 0x42, 0x29, 0xCF, 0xCB]
    );
    assert_eq!(
        gfni_encoding(GfniKind::Multiply, true, 13, 14, 15, false, 0),
        [0xC4, 0x42, 0x0D, 0xCF, 0xEF]
    );
    assert_eq!(
        gfni_encoding(GfniKind::Affine, false, 9, 10, 11, false, 0x63),
        [0xC4, 0x43, 0xA9, 0xCE, 0xCB, 0x63]
    );
    assert_eq!(
        gfni_encoding(GfniKind::AffineInverse, true, 13, 14, 15, false, 0xA5),
        [0xC4, 0x43, 0x8D, 0xCF, 0xEF, 0xA5]
    );

    let mut tested = 0usize;
    for seed in u8::MIN..=u8::MAX {
        for ymm in [false, true] {
            let (destination, source1, source2) = GFNI_OPERANDS[tested % GFNI_OPERANDS.len()];
            let code = gfni_encoding(
                GfniKind::Multiply,
                ymm,
                destination,
                source1,
                source2,
                tested & 1 != 0,
                seed,
            );
            assert_gfni_case(
                code,
                tested,
                GfniKind::Multiply,
                destination,
                source1,
                source2,
            );
            tested += 1;
        }
    }
    for kind in [GfniKind::Affine, GfniKind::AffineInverse] {
        for immediate in u8::MIN..=u8::MAX {
            for ymm in [false, true] {
                let (destination, source1, source2) = GFNI_OPERANDS[tested % GFNI_OPERANDS.len()];
                let code = gfni_encoding(
                    kind,
                    ymm,
                    destination,
                    source1,
                    source2,
                    tested & 1 != 0,
                    immediate,
                );
                assert_gfni_case(code, tested, kind, destination, source1, source2);
                tested += 1;
            }
        }
    }
    assert_eq!(tested, 1_536);
}
