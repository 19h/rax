//! End-to-end native-JIT coverage for register-only legacy MMX/SSE packed
//! floating-point conversions.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Cvtpi2ps,
    Cvttps2pi,
    Cvtps2pi,
    Cvtps2pd,
    Cvtpi2pd,
    Cvttpd2pi,
    Cvtpd2pi,
    Cvtpd2ps,
}

impl Kind {
    const FP32: [Self; 4] = [
        Self::Cvtpi2ps,
        Self::Cvttps2pi,
        Self::Cvtps2pi,
        Self::Cvtps2pd,
    ];

    const FP64: [Self; 4] = [
        Self::Cvtpi2pd,
        Self::Cvttpd2pi,
        Self::Cvtpd2pi,
        Self::Cvtpd2ps,
    ];

    fn opcode(self) -> u8 {
        match self {
            Self::Cvtpi2ps | Self::Cvtpi2pd => 0x2A,
            Self::Cvttps2pi | Self::Cvttpd2pi => 0x2C,
            Self::Cvtps2pi | Self::Cvtpd2pi => 0x2D,
            Self::Cvtps2pd | Self::Cvtpd2ps => 0x5A,
        }
    }
}

const FP32_PREFIXES: [&[u8]; 5] = [&[], &[0x48], &[0x44], &[0x41], &[0x4D]];
const FP64_PREFIXES: [&[u8]; 2] = [&[0x66], &[0x66, 0x48]];

fn rex(prefix: &[u8]) -> u8 {
    prefix
        .iter()
        .copied()
        .find(|byte| (0x40..=0x4F).contains(byte))
        .unwrap_or(0)
}

fn destination(kind: Kind, prefix: &[u8], modrm: u8) -> usize {
    let reg = (modrm >> 3) & 7;
    let rex_r = (rex(prefix) & 0x04) << 1;
    usize::from(match kind {
        Kind::Cvtpi2ps | Kind::Cvtps2pd | Kind::Cvtpi2pd | Kind::Cvtpd2ps => reg | rex_r,
        Kind::Cvttps2pi | Kind::Cvtps2pi | Kind::Cvttpd2pi | Kind::Cvtpd2pi => reg,
    })
}

fn source(kind: Kind, prefix: &[u8], modrm: u8) -> usize {
    let rm = modrm & 7;
    let rex_b = (rex(prefix) & 0x01) << 3;
    usize::from(match kind {
        Kind::Cvtpi2ps | Kind::Cvtpi2pd => rm,
        Kind::Cvttps2pi
        | Kind::Cvtps2pi
        | Kind::Cvtps2pd
        | Kind::Cvttpd2pi
        | Kind::Cvtpd2pi
        | Kind::Cvtpd2ps => rm | rex_b,
    })
}

fn apply(xmm: &mut [[u64; 2]; 16], mm: &mut [u64; 8], kind: Kind, prefix: &[u8], modrm: u8) {
    let destination = destination(kind, prefix, modrm);
    let source = source(kind, prefix, modrm);
    match kind {
        Kind::Cvtpi2ps => {
            let input = mm[source];
            let low = (input as u32 as i32) as f32;
            let high = ((input >> 32) as u32 as i32) as f32;
            xmm[destination][0] = u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32);
        }
        Kind::Cvttps2pi | Kind::Cvtps2pi => {
            let input = xmm[source][0];
            let low = f32::from_bits(input as u32) as i32 as u32;
            let high = f32::from_bits((input >> 32) as u32) as i32 as u32;
            mm[destination] = u64::from(low) | (u64::from(high) << 32);
        }
        Kind::Cvtps2pd => {
            let input = xmm[source][0];
            xmm[destination] = [
                f64::from(f32::from_bits(input as u32)).to_bits(),
                f64::from(f32::from_bits((input >> 32) as u32)).to_bits(),
            ];
        }
        Kind::Cvtpi2pd => {
            let input = mm[source];
            xmm[destination] = [
                (input as u32 as i32 as f64).to_bits(),
                ((input >> 32) as u32 as i32 as f64).to_bits(),
            ];
        }
        Kind::Cvttpd2pi | Kind::Cvtpd2pi => {
            let input = xmm[source];
            let low = f64::from_bits(input[0]) as i32 as u32;
            let high = f64::from_bits(input[1]) as i32 as u32;
            mm[destination] = u64::from(low) | (u64::from(high) << 32);
        }
        Kind::Cvtpd2ps => {
            let input = xmm[source];
            let low = f64::from_bits(input[0]) as f32;
            let high = f64::from_bits(input[1]) as f32;
            xmm[destination] = [
                u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32),
                0,
            ];
        }
    }
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
    registers.rcx = 0x1234_5678_9ABC_DEF0 ^ (profile as u64).rotate_left(3);
    registers.rdx = 0x2345_6789_ABCD_EF01 ^ (profile as u64).rotate_left(7);
    registers.rbx = 0x3456_789A_BCDE_F012 ^ (profile as u64).rotate_left(11);
    registers.rbp = 0x4567_89AB_CDEF_0123 ^ (profile as u64).rotate_left(13);
    registers.rsi = 0x5678_9ABC_DEF0_1234 ^ (profile as u64).rotate_left(17);
    registers.rdi = 0x6789_ABCD_EF01_2345 ^ (profile as u64).rotate_left(19);
    registers.r8 = 0x789A_BCDE_F012_3456;
    registers.r9 = 0x89AB_CDEF_0123_4567;
    registers.r10 = 0x9ABC_DEF0_1234_5678;
    registers.r11 = 0xABCD_EF01_2345_6789;
    registers.r12 = 0xBCDE_F012_3456_789A;
    registers.r13 = 0xCDEF_0123_4567_89AB;
    registers.r14 = 0xDEF0_1234_5678_9ABC;
    registers.r15 = 0xEF01_2345_6789_ABCD;
    registers.r16 = 0x1010_0000_0000_0010;
    registers.r17 = 0x1111_0000_0000_0011;
    registers.r18 = 0x1212_0000_0000_0012;
    registers.r19 = 0x1313_0000_0000_0013;
    registers.r20 = 0x1414_0000_0000_0014;
    registers.r21 = 0x1515_0000_0000_0015;
    registers.r22 = 0x1616_0000_0000_0016;
    registers.r23 = 0x1717_0000_0000_0017;
    registers.r24 = 0x1818_0000_0000_0018;
    registers.r25 = 0x1919_0000_0000_0019;
    registers.r26 = 0x1A1A_0000_0000_001A;
    registers.r27 = 0x1B1B_0000_0000_001B;
    registers.r28 = 0x1C1C_0000_0000_001C;
    registers.r29 = 0x1D1D_0000_0000_001D;
    registers.r30 = 0x1E1E_0000_0000_001E;
    registers.r31 = 0x1F1F_0000_0000_001F;
    registers.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        let low = (index as i32 * 37) - 129;
        let high = 257 - index as i32 * 53;
        u64::from(low as u32) | (u64::from(high as u32) << 32)
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        let low = (index as i32 * 5) - 31;
        let high = 47 - index as i32 * 7;
        registers.xmm[index] = [
            u64::from((low as f32).to_bits()) | (u64::from((high as f32).to_bits()) << 32),
            0xA100_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
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

fn setup_f64(vcpu: &mut X86_64Vcpu, profile: usize) -> Registers {
    let mut registers = setup(vcpu, profile);
    for index in 0..16 {
        let low = (index as i32 * 5) - 31 + (profile % 3) as i32;
        let high = 47 - index as i32 * 7 - (profile % 5) as i32;
        registers.xmm[index] = [(low as f64).to_bits(), (high as f64).to_bits()];
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

fn verify_scanner_cells(
    kinds: &[Kind],
    prefixes: &[&[u8]],
    setup_case: fn(&mut X86_64Vcpu, usize) -> Registers,
) -> usize {
    let mut cases = 0usize;
    for (kind_index, kind) in kinds.iter().copied().enumerate() {
        for (prefix_index, prefix) in prefixes.iter().copied().enumerate() {
            let mut code = Vec::new();
            for modrm in 0xC0..=0xFF {
                code.extend_from_slice(prefix);
                code.extend_from_slice(&[0x0F, kind.opcode(), modrm]);
                cases += 1;
            }
            code.push(0xF4);
            let profile = kind_index * prefixes.len() + prefix_index;
            let label = format!("{kind:?} {prefix:02X?}");

            let mut direct = make_vcpu_code(&code);
            let initial = setup_case(&mut direct, profile);
            let mut manual_xmm = initial.xmm;
            let mut manual_mm = initial.mm;
            for modrm in 0xC0..=0xFF {
                apply(&mut manual_xmm, &mut manual_mm, kind, prefix, modrm);
            }
            run_interp(&mut direct);
            let expected = direct.get_regs().unwrap();
            assert_eq!(expected.xmm, manual_xmm, "{label}: direct vs IEEE XMM");
            assert_eq!(expected.mm, manual_mm, "{label}: direct vs Intel MMX");
            assert_eq!(expected.ymm_high, initial.ymm_high, "{label}: YMM");
            assert_eq!(expected.zmm_high, initial.zmm_high, "{label}: ZMM");
            assert_eq!(expected.zmm_ext, initial.zmm_ext, "{label}: ZMM16-31");
            assert_eq!(expected.k, initial.k, "{label}: opmask");
            assert_eq!(gprs(&expected), gprs(&initial), "{label}: GPRs");
            assert_eq!(expected.rflags, initial.rflags, "{label}: flags");

            let mut jit = make_vcpu_code(&code);
            setup_case(&mut jit, profile);
            jit.set_jit_call(false);
            jit.set_jit_mem(false);
            assert!(
                jit.jit_try_block()
                    .unwrap_or_else(|error| panic!("{label}: native admission: {error:?}")),
                "{label}: every register cell must enter the native tier:\n{}",
                jit.jit_dump_region(LOAD_ADDR)
            );
            assert_eq!(
                jit.get_regs().unwrap().rip,
                LOAD_ADDR + code.len() as u64 - 1,
                "{label}: HLT frontier"
            );
            run_interp(&mut jit);
            assert_full_state(&jit.get_regs().unwrap(), &expected, &label);
        }
    }
    cases
}

/// The independent scanner reports five canonical prefix images for each
/// no-mandatory-prefix opcode: no REX plus representative `REX.W`, `REX.R`,
/// `REX.B`, and `REX.WRB`. Every image has 64 register ModR/M cells.
///
/// Total: 4 opcodes × 5 prefix images × 64 register cells = 1,280 cells.
#[test]
fn jit_all_1280_scanner_legacy_packed_fp_convert_gaps_match_direct_and_ieee_equations() {
    assert!(std::is_x86_feature_detected!("avx"));
    assert_eq!(
        verify_scanner_cells(&Kind::FP32, &FP32_PREFIXES, setup),
        4 * 5 * 64
    );
}

/// The independent scanner reports mandatory 66 and mandatory 66 plus REX.W
/// for each packed-double opcode. Each image has 64 register ModR/M cells.
///
/// Total: 4 opcodes × 2 prefix images × 64 register cells = 512 cells.
#[test]
fn jit_all_512_scanner_legacy_packed_f64_convert_gaps_match_direct_and_ieee_equations() {
    assert!(std::is_x86_feature_detected!("avx"));
    assert_eq!(
        verify_scanner_cells(&Kind::FP64, &FP64_PREFIXES, setup_f64),
        4 * 2 * 64
    );
}
