//! End-to-end JIT/fallback coverage for register-only legacy GFNI
//! instructions.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GfniKind {
    Multiply,
    Affine,
    AffineInverse,
}

const KINDS: [GfniKind; 3] = [
    GfniKind::Multiply,
    GfniKind::Affine,
    GfniKind::AffineInverse,
];
const IMMEDIATE: u8 = 0;

fn append_encoding(code: &mut Vec<u8>, kind: GfniKind, prefix: &[u8], modrm: u8) {
    code.extend_from_slice(prefix);
    match kind {
        GfniKind::Multiply => code.extend_from_slice(&[0x0F, 0x38, 0xCF, modrm]),
        GfniKind::Affine => {
            code.extend_from_slice(&[0x0F, 0x3A, 0xCE, modrm, IMMEDIATE]);
        }
        GfniKind::AffineInverse => {
            code.extend_from_slice(&[0x0F, 0x3A, 0xCF, modrm, IMMEDIATE]);
        }
    }
}

fn rex(prefix: &[u8]) -> u8 {
    prefix
        .last()
        .copied()
        .filter(|byte| (0x40..=0x4F).contains(byte))
        .unwrap_or(0)
}

fn gf_multiply(a: u8, b: u8) -> u8 {
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

fn gf_inverse(value: u8) -> u8 {
    let mut result = 1u8;
    let mut power = value;
    let mut exponent = 254u8;
    while exponent != 0 {
        if exponent & 1 != 0 {
            result = gf_multiply(result, power);
        }
        power = gf_multiply(power, power);
        exponent >>= 1;
    }
    result
}

fn vector_byte(vector: &[u64; 2], index: usize) -> u8 {
    (vector[index / 8] >> ((index % 8) * 8)) as u8
}

fn apply(xmm: &mut [[u64; 2]; 16], kind: GfniKind, prefix: &[u8], modrm: u8) {
    let rex = rex(prefix);
    let destination = usize::from(((modrm >> 3) & 7) | ((rex & 0x04) << 1));
    let source = usize::from((modrm & 7) | ((rex & 0x01) << 3));
    let input = xmm[destination];
    let matrix_or_multiplier = xmm[source];
    let mut result = [0u8; 16];
    for (lane, output) in result.iter_mut().enumerate() {
        let input_byte = vector_byte(&input, lane);
        *output = match kind {
            GfniKind::Multiply => {
                gf_multiply(input_byte, vector_byte(&matrix_or_multiplier, lane))
            }
            GfniKind::Affine | GfniKind::AffineInverse => {
                let input_byte = if kind == GfniKind::AffineInverse {
                    gf_inverse(input_byte)
                } else {
                    input_byte
                };
                let qword_base = lane & !7;
                let mut transformed = 0u8;
                for bit in 0..8 {
                    let matrix_row =
                        vector_byte(&matrix_or_multiplier, qword_base + 7 - bit);
                    let parity = (matrix_row & input_byte).count_ones() as u8 & 1;
                    transformed |= (parity ^ ((IMMEDIATE >> bit) & 1)) << bit;
                }
                transformed
            }
        };
    }
    xmm[destination] = [
        u64::from_le_bytes(result[..8].try_into().unwrap()),
        u64::from_le_bytes(result[8..].try_into().unwrap()),
    ];
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

fn setup(vcpu: &mut X86_64Vcpu, profile: usize) -> Registers {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr4 |= 1 << 9; // CR4.OSFXSR
    vcpu.set_sregs(&sregs).unwrap();

    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = 0x0123_4567_89AB_CDEF ^ profile as u64;
    registers.rcx = 0xFEDC_BA98_7654_3210 ^ (profile as u64).rotate_left(7);
    registers.rdx = 0x8000_0000_0000_0001;
    registers.rbx = 0x7FFF_FFFF_FFFF_FFFE;
    registers.rsp = 0x1111_2222_3333_4444;
    registers.rbp = 0x5555_6666_7777_8888;
    registers.rsi = 0x9999_AAAA_BBBB_CCCC;
    registers.rdi = 0xDDDD_EEEE_FFFF_0001;
    registers.r8 = 0x1020_4081_0204_0810;
    registers.r9 = 0x8040_2010_0804_0201;
    registers.r10 = 0xA5A5_5A5A_6996_9669;
    registers.r11 = 0x6996_9669_A5A5_5A5A;
    registers.r12 = 0x0123_0123_4567_4567;
    registers.r13 = 0x89AB_89AB_CDEF_CDEF;
    registers.r14 = 0x0F0F_F0F0_33CC_CC33;
    registers.r15 = 0x55AA_AA55_5AA5_A55A;
    registers.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + profile) as u32)
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        registers.xmm[index] = [
            0x0123_4567_89AB_CDEFu64.rotate_left((index * 11 + profile * 3) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444),
            0xFEDC_BA98_7654_3210u64.rotate_left((index * 7 + profile * 5) as u32)
                ^ (index as u64).wrapping_mul(0x8040_2010_0804_0201),
        ];
        registers.ymm_high[index] = [
            0xB100_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
            0xB200_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
        ];
        registers.zmm_high[index] = std::array::from_fn(|word| {
            0xC000_0000_0000_0000 | ((word as u64) << 56) | ((profile as u64) << 16) | index as u64
        });
        registers.zmm_ext[index] = std::array::from_fn(|word| {
            0xD000_0000_0000_0000 | ((word as u64) << 56) | ((profile as u64) << 16) | index as u64
        });
    }
    vcpu.set_regs(&registers).unwrap();
    registers
}

fn assert_full_state(actual: &Registers, expected: &Registers, label: &str) {
    assert_eq!(gprs(actual), gprs(expected), "{label}: GPR state");
    assert_eq!(actual.xmm, expected.xmm, "{label}: XMM");
    assert_eq!(actual.ymm_high, expected.ymm_high, "{label}: YMM");
    assert_eq!(actual.zmm_high, expected.zmm_high, "{label}: ZMM");
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "{label}: ZMM16-31");
    assert_eq!(actual.k, expected.k, "{label}: opmask");
    assert_eq!(actual.mm, expected.mm, "{label}: MMX");
    assert_eq!(actual.rflags, expected.rflags, "{label}: RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{label}: RIP");
}

/// The independent iced-x86 scanner reports two canonical mandatory-prefix
/// images (without REX and with ignored REX.W) across all 64 register ModR/M
/// cells for each of three instructions:
///
/// 3 instructions × 2 prefix images × 64 ModR/M cells = 384 cells.
#[test]
fn jit_all_384_scanner_legacy_gfni_gaps_match_direct_and_intel_equations() {
    let host_native =
        std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("gfni");
    let groups: &[&[u8]] = &[&[0x66], &[0x66, 0x48]];
    let mut tested = 0usize;
    for (kind_index, kind) in KINDS.into_iter().enumerate() {
        for (prefix_index, &prefix) in groups.iter().enumerate() {
            let profile = kind_index * groups.len() + prefix_index;
            let mut code = Vec::new();
            for modrm in 0xC0..=0xFF {
                append_encoding(&mut code, kind, prefix, modrm);
                tested += 1;
            }
            code.push(0xF4);
            let label = format!("legacy GFNI {kind:?} {prefix:02X?}");

            let mut direct = make_vcpu_code(&code);
            let initial = setup(&mut direct, profile);
            let mut manual_xmm = initial.xmm;
            for modrm in 0xC0..=0xFF {
                apply(&mut manual_xmm, kind, prefix, modrm);
            }
            run_interp(&mut direct);
            let expected = direct.get_regs().unwrap();
            assert_eq!(expected.xmm, manual_xmm, "{label}: direct equation");
            assert_eq!(gprs(&expected), gprs(&initial), "{label}: GPR state");
            assert_eq!(expected.ymm_high, initial.ymm_high, "{label}: YMM");
            assert_eq!(expected.zmm_high, initial.zmm_high, "{label}: ZMM");
            assert_eq!(expected.zmm_ext, initial.zmm_ext, "{label}: ZMM16-31");
            assert_eq!(expected.k, initial.k, "{label}: opmask");
            assert_eq!(expected.mm, initial.mm, "{label}: MMX");
            assert_eq!(expected.rflags, initial.rflags, "{label}: RFLAGS");

            let mut jit = make_vcpu_code(&code);
            setup(&mut jit, profile);
            jit.set_jit_call(false);
            jit.set_jit_mem(false);
            let admitted = jit
                .jit_try_block()
                .unwrap_or_else(|error| panic!("{label}: native admission: {error:?}"));
            assert_eq!(admitted, host_native, "{label}: dynamic GFNI feature gate");
            if admitted {
                assert_eq!(
                    jit.get_regs().unwrap().rip,
                    LOAD_ADDR + code.len() as u64 - 1,
                    "{label}: HLT frontier"
                );
            }
            run_interp(&mut jit);
            assert_full_state(&jit.get_regs().unwrap(), &expected, &label);
        }
    }
    assert_eq!(tested, KINDS.len() * groups.len() * 64);
}
