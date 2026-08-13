//! End-to-end native-JIT coverage for register-only legacy SSE/SSE2 scalar
//! floating-point conversions.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    CvtSi2Ss,
    CvtSi2Sd,
    CvtSs2Si,
    CvtSd2Si,
    CvttSs2Si,
    CvttSd2Si,
    CvtSs2Sd,
    CvtSd2Ss,
}

impl Family {
    const ALL: [Self; 8] = [
        Self::CvtSi2Ss,
        Self::CvtSi2Sd,
        Self::CvtSs2Si,
        Self::CvtSd2Si,
        Self::CvttSs2Si,
        Self::CvttSd2Si,
        Self::CvtSs2Sd,
        Self::CvtSd2Ss,
    ];

    fn prefix(self) -> u8 {
        match self {
            Self::CvtSi2Ss | Self::CvtSs2Si | Self::CvttSs2Si | Self::CvtSs2Sd => 0xF3,
            Self::CvtSi2Sd | Self::CvtSd2Si | Self::CvttSd2Si | Self::CvtSd2Ss => 0xF2,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::CvtSi2Ss | Self::CvtSi2Sd => 0x2A,
            Self::CvttSs2Si | Self::CvttSd2Si => 0x2C,
            Self::CvtSs2Si | Self::CvtSd2Si => 0x2D,
            Self::CvtSs2Sd | Self::CvtSd2Ss => 0x5A,
        }
    }
}

const SCANNER_REX_IMAGES: [Option<u8>; 2] = [None, Some(0x48)];

fn destination(rex: u8, modrm: u8) -> usize {
    usize::from(((modrm >> 3) & 7) | ((rex & 0x04) << 1))
}

fn source(rex: u8, modrm: u8) -> usize {
    usize::from((modrm & 7) | ((rex & 0x01) << 3))
}

fn apply(gprs: &mut [u64; 32], xmm: &mut [[u64; 2]; 16], family: Family, rex: u8, modrm: u8) {
    let destination = destination(rex, modrm);
    let source = source(rex, modrm);
    let w64 = rex & 0x08 != 0;
    match family {
        Family::CvtSi2Ss => {
            let value = if w64 {
                gprs[source] as i64 as f32
            } else {
                gprs[source] as u32 as i32 as f32
            };
            xmm[destination][0] =
                (xmm[destination][0] & !u64::from(u32::MAX)) | u64::from(value.to_bits());
        }
        Family::CvtSi2Sd => {
            let value = if w64 {
                gprs[source] as i64 as f64
            } else {
                f64::from(gprs[source] as u32 as i32)
            };
            xmm[destination][0] = value.to_bits();
        }
        Family::CvtSs2Si | Family::CvttSs2Si => {
            let value = f32::from_bits(xmm[source][0] as u32);
            gprs[destination] = if w64 {
                value as i64 as u64
            } else {
                u64::from(value as i32 as u32)
            };
        }
        Family::CvtSd2Si | Family::CvttSd2Si => {
            let value = f64::from_bits(xmm[source][0]);
            gprs[destination] = if w64 {
                value as i64 as u64
            } else {
                u64::from(value as i32 as u32)
            };
        }
        Family::CvtSs2Sd => {
            xmm[destination][0] = f64::from(f32::from_bits(xmm[source][0] as u32)).to_bits();
        }
        Family::CvtSd2Ss => {
            let converted = f64::from_bits(xmm[source][0]) as f32;
            xmm[destination][0] =
                (xmm[destination][0] & !u64::from(u32::MAX)) | u64::from(converted.to_bits());
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

fn setup(vcpu: &mut X86_64Vcpu, family: Family, profile: usize) -> Registers {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr4 |= 1 << 9; // CR4.OSFXSR
    vcpu.set_sregs(&sregs).unwrap();

    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = (-31i64 + profile as i64) as u64;
    registers.rcx = 29 + profile as u64;
    registers.rdx = (-23i64 - profile as i64) as u64;
    registers.rbx = 19 + profile as u64;
    registers.rsp = 17 + profile as u64;
    registers.rbp = (-13i64 - profile as i64) as u64;
    registers.rsi = 11 + profile as u64;
    registers.rdi = (-7i64 - profile as i64) as u64;
    registers.r8 = 5 + profile as u64;
    registers.r9 = (-3i64 - profile as i64) as u64;
    registers.r10 = 2 + profile as u64;
    registers.r11 = -1i64 as u64;
    registers.r12 = 37 + profile as u64;
    registers.r13 = (-41i64 - profile as i64) as u64;
    registers.r14 = 43 + profile as u64;
    registers.r15 = (-47i64 - profile as i64) as u64;
    registers.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + profile) as u32)
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        let signed = index as i32 * 5 - 31 + profile as i32;
        registers.xmm[index] = match family {
            Family::CvtSs2Si | Family::CvttSs2Si | Family::CvtSs2Sd => [
                u64::from((signed as f32).to_bits()) | (0xA5A5_5A5Au64 << 32),
                0xA100_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
            ],
            _ => [
                (signed as f64).to_bits(),
                0xA100_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
            ],
        };
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

/// The independent scanner reports mandatory F2/F3 and mandatory F2/F3 plus
/// REX.W for every scalar-conversion opcode. Each byte image has 64 register
/// ModR/M cells.
///
/// Total: 8 families × 2 REX images × 64 register cells = 1,024 cells.
#[test]
fn jit_all_1024_scanner_legacy_scalar_fp_convert_gaps_match_direct_and_equations() {
    assert!(std::is_x86_feature_detected!("avx"));
    let mut cases = 0usize;
    for (family_index, family) in Family::ALL.into_iter().enumerate() {
        for (rex_index, rex) in SCANNER_REX_IMAGES.into_iter().enumerate() {
            let mut code = Vec::new();
            for modrm in 0xC0..=0xFF {
                code.push(family.prefix());
                code.extend(rex);
                code.extend([0x0F, family.opcode(), modrm]);
                cases += 1;
            }
            code.push(0xF4);
            let profile = family_index * SCANNER_REX_IMAGES.len() + rex_index;
            let label = format!("{family:?} rex={rex:02X?}");

            let mut direct = make_vcpu_code(&code);
            let initial = setup(&mut direct, family, profile);
            let mut manual_gprs = gprs(&initial);
            let mut manual_xmm = initial.xmm;
            for modrm in 0xC0..=0xFF {
                apply(
                    &mut manual_gprs,
                    &mut manual_xmm,
                    family,
                    rex.unwrap_or(0),
                    modrm,
                );
            }
            run_interp(&mut direct);
            let expected = direct.get_regs().unwrap();
            assert_eq!(gprs(&expected), manual_gprs, "{label}: direct GPR equation");
            assert_eq!(expected.xmm, manual_xmm, "{label}: direct FP equation");
            assert_eq!(expected.ymm_high, initial.ymm_high, "{label}: YMM");
            assert_eq!(expected.zmm_high, initial.zmm_high, "{label}: ZMM");
            assert_eq!(expected.zmm_ext, initial.zmm_ext, "{label}: ZMM16-31");
            assert_eq!(expected.k, initial.k, "{label}: opmask");
            assert_eq!(expected.mm, initial.mm, "{label}: MMX");
            assert_eq!(expected.rflags, initial.rflags, "{label}: RFLAGS");

            let mut jit = make_vcpu_code(&code);
            setup(&mut jit, family, profile);
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
    assert_eq!(cases, Family::ALL.len() * SCANNER_REX_IMAGES.len() * 64);
}
