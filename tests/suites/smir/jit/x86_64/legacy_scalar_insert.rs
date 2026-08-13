//! End-to-end native-JIT coverage for register-source legacy MMX/SSE
//! `PINSRB/PINSRD/PINSRQ/PINSRW`.

use super::*;

const IMMEDIATE: u8 = 0xE5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    PinsB,
    PinsD,
    PinsQ,
    PinsWMap1Mmx,
    PinsWMap1Xmm,
}

impl Family {
    fn map1(self) -> bool {
        matches!(self, Self::PinsWMap1Mmx | Self::PinsWMap1Xmm)
    }

    fn mmx(self) -> bool {
        self == Self::PinsWMap1Mmx
    }

    fn scalar_bytes(self) -> usize {
        match self {
            Self::PinsB => 1,
            Self::PinsWMap1Mmx | Self::PinsWMap1Xmm => 2,
            Self::PinsD => 4,
            Self::PinsQ => 8,
        }
    }

    fn lanes(self) -> u8 {
        match self {
            Self::PinsB => 16,
            Self::PinsD | Self::PinsWMap1Mmx => 4,
            Self::PinsQ => 2,
            Self::PinsWMap1Xmm => 8,
        }
    }
}

fn append_encoding(code: &mut Vec<u8>, family: Family, prefix: &[u8], modrm: u8) {
    code.extend_from_slice(prefix);
    if family.map1() {
        code.extend_from_slice(&[0x0F, 0xC4, modrm, IMMEDIATE]);
    } else {
        let opcode = match family {
            Family::PinsB => 0x20,
            Family::PinsD | Family::PinsQ => 0x22,
            Family::PinsWMap1Mmx | Family::PinsWMap1Xmm => unreachable!(),
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

fn apply(
    gprs: &[u64; 32],
    xmm: &mut [[u64; 2]; 16],
    mm: &mut [u64; 8],
    family: Family,
    prefix: &[u8],
    modrm: u8,
) {
    let rex = rex(prefix);
    let destination = (modrm >> 3) & 7;
    let destination = if family.mmx() {
        destination
    } else {
        destination | ((rex & 0x04) << 1)
    };
    let source = (modrm & 7) | ((rex & 0x01) << 3);
    let width = family.scalar_bytes();
    let lane = usize::from(IMMEDIATE & (family.lanes() - 1));
    let source = gprs[usize::from(source)].to_le_bytes();
    if family.mmx() {
        let mut destination_bytes = mm[usize::from(destination)].to_le_bytes();
        destination_bytes[lane * width..lane * width + width]
            .copy_from_slice(&source[..width]);
        mm[usize::from(destination)] = u64::from_le_bytes(destination_bytes);
    } else {
        let destination = &mut xmm[usize::from(destination)];
        let mut bytes = [destination[0].to_le_bytes(), destination[1].to_le_bytes()].concat();
        bytes[lane * width..lane * width + width].copy_from_slice(&source[..width]);
        destination[0] = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        destination[1] = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    }
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
/// * `PINSRB`: 2 prefix images × 64 ModR/M cells = 128;
/// * `PINSRD`: 1 × 64 = 64;
/// * `PINSRQ`: 1 × 64 = 64;
/// * XMM `PINSRW`: 2 × 64 = 128;
/// * MMX `PINSRW` guest RSP/RBP sources: 3 prefixes × 2 × 8 = 48.
///
/// Total: 128 + 64 + 64 + 128 + 48 = 432 newly admitted cells.
#[test]
fn jit_all_432_scanner_legacy_scalar_insert_gaps_match_direct_and_intel_equations() {
    assert!(std::is_x86_feature_detected!("avx"));
    assert!(std::is_x86_feature_detected!("sse4.1"));

    let groups: &[(Family, &[u8])] = &[
        (Family::PinsB, &[0x66]),
        (Family::PinsB, &[0x66, 0x48]),
        (Family::PinsD, &[0x66]),
        (Family::PinsQ, &[0x66, 0x48]),
        (Family::PinsWMap1Xmm, &[0x66]),
        (Family::PinsWMap1Xmm, &[0x66, 0x48]),
        (Family::PinsWMap1Mmx, &[]),
        (Family::PinsWMap1Mmx, &[0x44]),
        (Family::PinsWMap1Mmx, &[0x48]),
    ];
    let mut cases = 0usize;
    for (profile, &(family, prefix)) in groups.iter().enumerate() {
        let modrms: Vec<u8> = if family.mmx() {
            (0..8)
                .flat_map(|destination| {
                    [4u8, 5]
                        .into_iter()
                        .map(move |source| 0xC0 | (destination << 3) | source)
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
        let initial_gprs = gprs(&initial);
        let mut manual_xmm = initial.xmm;
        let mut manual_mm = initial.mm;
        for &modrm in &modrms {
            apply(
                &initial_gprs,
                &mut manual_xmm,
                &mut manual_mm,
                family,
                prefix,
                modrm,
            );
        }
        run_interp(&mut direct);
        let expected = direct.get_regs().unwrap();
        assert_eq!(expected.xmm, manual_xmm, "{label}: direct XMM equation");
        assert_eq!(expected.mm, manual_mm, "{label}: direct MMX equation");
        assert_eq!(gprs(&expected), initial_gprs, "{label}: GPR state");
        assert_eq!(expected.ymm_high, initial.ymm_high, "{label}: YMM");
        assert_eq!(expected.zmm_high, initial.zmm_high, "{label}: ZMM");
        assert_eq!(expected.zmm_ext, initial.zmm_ext, "{label}: ZMM16-31");
        assert_eq!(expected.k, initial.k, "{label}: opmask");
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
    assert_eq!(cases, 2 * 64 + 64 + 64 + 2 * 64 + 3 * 2 * 8);
}
