//! End-to-end native-JIT coverage for register-destination legacy MMX/SSE
//! `EXTRACTPS` and `PEXTRB/D/Q/W`.

use super::*;

const IMMEDIATE: u8 = 0xE5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    ExtractPs,
    PextrB,
    PextrD,
    PextrQ,
    PextrWMap1Mmx,
    PextrWMap1Xmm,
    PextrWMap3,
}

impl Family {
    fn map1(self) -> bool {
        matches!(self, Self::PextrWMap1Mmx | Self::PextrWMap1Xmm)
    }

    fn mmx(self) -> bool {
        self == Self::PextrWMap1Mmx
    }

    fn scalar_bytes(self) -> usize {
        match self {
            Self::PextrB => 1,
            Self::PextrWMap1Mmx | Self::PextrWMap1Xmm | Self::PextrWMap3 => 2,
            Self::ExtractPs | Self::PextrD => 4,
            Self::PextrQ => 8,
        }
    }

    fn lane_mask(self) -> u8 {
        match self {
            Self::PextrB => 0x0F,
            Self::PextrWMap1Mmx => 0x03,
            Self::PextrWMap1Xmm | Self::PextrWMap3 => 0x07,
            Self::ExtractPs | Self::PextrD => 0x03,
            Self::PextrQ => 0x01,
        }
    }
}

fn append_encoding(code: &mut Vec<u8>, family: Family, prefix: &[u8], modrm: u8) {
    code.extend_from_slice(prefix);
    if family.map1() {
        code.extend_from_slice(&[0x0F, 0xC5, modrm, IMMEDIATE]);
    } else {
        let opcode = match family {
            Family::PextrB => 0x14,
            Family::PextrWMap3 => 0x15,
            Family::PextrD | Family::PextrQ => 0x16,
            Family::ExtractPs => 0x17,
            Family::PextrWMap1Mmx | Family::PextrWMap1Xmm => unreachable!(),
        };
        code.extend_from_slice(&[0x0F, 0x3A, opcode, modrm, IMMEDIATE]);
    }
}

fn rex(prefix: &[u8]) -> u8 {
    prefix
        .last()
        .copied()
        .filter(|byte| (0x40..=0x4F).contains(byte))
        .unwrap_or(0)
}

fn extract_scalar(bytes: &[u8], lane: usize, width: usize) -> u64 {
    let mut scalar = [0u8; 8];
    scalar[..width].copy_from_slice(&bytes[lane * width..lane * width + width]);
    u64::from_le_bytes(scalar)
}

fn apply(
    gprs: &mut [u64; 32],
    xmm: &[[u64; 2]; 16],
    mm: &[u64; 8],
    family: Family,
    prefix: &[u8],
    modrm: u8,
) {
    let rex = rex(prefix);
    let reg = (modrm >> 3) & 7;
    let rm = modrm & 7;
    let rex_r = (rex & 0x04) << 1;
    let rex_b = (rex & 0x01) << 3;
    let (destination, source) = if family.map1() {
        (
            reg | rex_r,
            if family.mmx() { rm } else { rm | rex_b },
        )
    } else {
        (rm | rex_b, reg | rex_r)
    };
    let lane = usize::from(IMMEDIATE & family.lane_mask());
    let width = family.scalar_bytes();
    let value = if family.mmx() {
        extract_scalar(&mm[usize::from(source)].to_le_bytes(), lane, width)
    } else {
        let vector = xmm[usize::from(source)];
        let bytes = [vector[0].to_le_bytes(), vector[1].to_le_bytes()].concat();
        extract_scalar(&bytes, lane, width)
    };
    gprs[usize::from(destination)] = value;
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

/// The independent iced-x86 scanner reports:
///
/// * `EXTRACTPS`: 2 prefix images × 64 ModR/M cells = 128;
/// * `PEXTRB`: 2 × 64 = 128;
/// * `PEXTRD`: 1 × 64 = 64;
/// * `PEXTRQ`: 1 × 64 = 64;
/// * map-1 and map-3 XMM `PEXTRW`: 2 maps × 2 prefixes × 64 = 256;
/// * MMX `PEXTRW` guest RSP/RBP destinations: 3 prefixes × 2 × 8 = 48.
///
/// Total: 128 + 128 + 64 + 64 + 256 + 48 = 688 newly admitted cells.
#[test]
fn jit_all_688_scanner_legacy_scalar_extract_gaps_match_direct_and_intel_equations() {
    assert!(std::is_x86_feature_detected!("avx"));
    assert!(std::is_x86_feature_detected!("sse4.1"));

    let groups: &[(Family, &[u8])] = &[
        (Family::ExtractPs, &[0x66]),
        (Family::ExtractPs, &[0x66, 0x48]),
        (Family::PextrB, &[0x66]),
        (Family::PextrB, &[0x66, 0x48]),
        (Family::PextrD, &[0x66]),
        (Family::PextrQ, &[0x66, 0x48]),
        (Family::PextrWMap1Xmm, &[0x66]),
        (Family::PextrWMap1Xmm, &[0x66, 0x48]),
        (Family::PextrWMap3, &[0x66]),
        (Family::PextrWMap3, &[0x66, 0x48]),
        (Family::PextrWMap1Mmx, &[]),
        (Family::PextrWMap1Mmx, &[0x41]),
        (Family::PextrWMap1Mmx, &[0x48]),
    ];
    let mut cases = 0usize;
    for (profile, &(family, prefix)) in groups.iter().enumerate() {
        let modrms: Vec<u8> = if family.mmx() {
            [4u8, 5]
                .into_iter()
                .flat_map(|destination| {
                    (0..8).map(move |source| 0xC0 | (destination << 3) | source)
                })
                .collect()
        } else {
            (0xC0..=0xFF).collect()
        };
        let mut code = Vec::new();
        for &modrm in &modrms {
            append_encoding(&mut code, family, prefix, modrm);
            cases += 1;
        }
        code.push(0xF4);
        let label = format!("{family:?} {prefix:02X?}");

        let mut direct = make_vcpu_code(&code);
        let initial = setup(&mut direct, profile);
        let mut manual_gprs = gprs(&initial);
        for &modrm in &modrms {
            apply(
                &mut manual_gprs,
                &initial.xmm,
                &initial.mm,
                family,
                prefix,
                modrm,
            );
        }
        run_interp(&mut direct);
        let expected = direct.get_regs().unwrap();
        assert_eq!(gprs(&expected), manual_gprs, "{label}: direct equation");
        assert_eq!(expected.xmm, initial.xmm, "{label}: XMM");
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
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{label}: native admission: {error:?}")),
            "{label}: every scanner-gap cell must enter the native tier:\n{}",
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
    assert_eq!(cases, 2 * 64 + 2 * 64 + 64 + 64 + 2 * 2 * 64 + 3 * 2 * 8);
}
