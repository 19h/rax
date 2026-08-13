//! End-to-end native-JIT coverage for register-only legacy SSE4.1 `PTEST`.

use super::*;

const SCANNER_REX_IMAGES: [Option<u8>; 2] = [None, Some(0x48)];
const DEFINED_FLAG_MASK: u64 = 0x8D5;

fn first_source(rex: u8, modrm: u8) -> usize {
    usize::from(((modrm >> 3) & 7) | ((rex & 0x04) << 1))
}

fn second_source(rex: u8, modrm: u8) -> usize {
    usize::from((modrm & 7) | ((rex & 0x01) << 3))
}

fn apply_flags(xmm: &[[u64; 2]; 16], rflags: u64, rex: u8, modrm: u8) -> u64 {
    let first = xmm[first_source(rex, modrm)];
    let second = xmm[second_source(rex, modrm)];
    let intersection = (first[0] & second[0]) | (first[1] & second[1]);
    let outside = (second[0] & !first[0]) | (second[1] & !first[1]);
    let mut result = rflags & !DEFINED_FLAG_MASK;
    result |= u64::from(outside == 0);
    result |= u64::from(intersection == 0) << 6;
    result
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
    registers.rax = 0x0123_4567_89AB_CDEF;
    registers.rcx = 0x1020_4081_0204_0810;
    registers.rdx = 0xA55A_6996_F00F_3CC3;
    registers.rbx = 0x6996_F00F_3CC3_A55A;
    registers.rsp = 0x11_0000;
    registers.rbp = 0x1221_3443_5665_7887;
    registers.rsi = 0x8778_6556_4334_2112;
    registers.rdi = 0xDEAD_BEEF_CAFE_BABE;
    registers.r8 = 0x0102_0408_1020_4081;
    registers.r9 = 0x8040_2010_0804_0201;
    registers.r10 = 0x1111_2222_3333_4444;
    registers.r11 = 0x4444_3333_2222_1111;
    registers.r12 = 0x5555_AAAA_3333_CCCC;
    registers.r13 = 0xCCCC_3333_AAAA_5555;
    registers.r14 = 0x8000_0000_0000_0001;
    registers.r15 = 0x7FFF_FFFF_FFFF_FFFE;
    registers.rflags = 0x2 | DEFINED_FLAG_MASK | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + profile) as u32)
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        registers.xmm[index] = [
            0x0123_4567_89AB_CDEFu64.rotate_left((index * 9 + profile) as u32),
            0xF0E1_D2C3_B4A5_9687u64.rotate_right((index * 11 + profile) as u32),
        ];
        registers.ymm_high[index] = [
            0xB100_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
            0xB200_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
        ];
        registers.zmm_high[index] = std::array::from_fn(|word| {
            0xC000_0000_0000_0000
                | ((word as u64) << 56)
                | ((profile as u64) << 16)
                | index as u64
        });
        registers.zmm_ext[index] = std::array::from_fn(|word| {
            0xD000_0000_0000_0000
                | ((word as u64) << 56)
                | ((profile as u64) << 16)
                | index as u64
        });
    }
    vcpu.set_regs(&registers).unwrap();
    registers
}

fn install(vcpu: &mut X86_64Vcpu, registers: &Registers) {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr4 |= 1 << 9; // CR4.OSFXSR
    vcpu.set_sregs(&sregs).unwrap();
    vcpu.set_regs(registers).unwrap();
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

fn run_case(code: &[u8], initial: &Registers, manual_rflags: u64, label: &str) {
    let mut direct = make_vcpu_code(code);
    install(&mut direct, initial);
    run_interp(&mut direct);
    let expected = direct.get_regs().unwrap();
    assert_eq!(expected.rflags, manual_rflags, "{label}: direct flag equation");
    assert_eq!(gprs(&expected), gprs(initial), "{label}: GPR state");
    assert_eq!(expected.xmm, initial.xmm, "{label}: XMM");
    assert_eq!(expected.ymm_high, initial.ymm_high, "{label}: YMM");
    assert_eq!(expected.zmm_high, initial.zmm_high, "{label}: ZMM");
    assert_eq!(expected.zmm_ext, initial.zmm_ext, "{label}: ZMM16-31");
    assert_eq!(expected.k, initial.k, "{label}: opmask");
    assert_eq!(expected.mm, initial.mm, "{label}: MMX");

    let mut jit = make_vcpu_code(code);
    install(&mut jit, initial);
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
    assert_full_state(&jit.get_regs().unwrap(), &expected, label);
}

/// The independent scanner reports mandatory 66H and mandatory 66H plus
/// REX.W. Each byte image has 64 register ModR/M cells.
/// Total: 2 REX images x 64 register cells = 128 cells.
#[test]
fn jit_all_128_scanner_legacy_ptest_gaps_match_direct_and_flag_equations() {
    assert!(std::is_x86_feature_detected!("sse4.1"));
    assert!(std::is_x86_feature_detected!("avx"));
    let mut cases = 0usize;
    for (rex_index, rex) in SCANNER_REX_IMAGES.into_iter().enumerate() {
        let mut initial_vcpu = make_vcpu_code(&[0xF4]);
        let initial = setup(&mut initial_vcpu, rex_index);
        let mut manual_rflags = initial.rflags;
        let mut code = Vec::new();
        for modrm in 0xC0..=0xFF {
            code.push(0x66);
            code.extend(rex);
            code.extend([0x0F, 0x38, 0x17, modrm]);
            manual_rflags = apply_flags(&initial.xmm, manual_rflags, rex.unwrap_or(0), modrm);
            cases += 1;
        }
        code.push(0xF4);
        run_case(
            &code,
            &initial,
            manual_rflags,
            &format!("PTEST scanner rex={rex:02X?}"),
        );
    }
    assert_eq!(cases, SCANNER_REX_IMAGES.len() * 64);
}

#[test]
fn jit_ptest_covers_all_four_zf_cf_outcomes_and_high_registers() {
    assert!(std::is_x86_feature_detected!("sse4.1"));
    assert!(std::is_x86_feature_detected!("avx"));
    let rex = 0x45;
    let modrm = 0xCA; // PTEST XMM9, XMM10
    let mut observed = [false; 4];
    for profile in 0..4 {
        let mut initial_vcpu = make_vcpu_code(&[0xF4]);
        let mut initial = setup(&mut initial_vcpu, 8 + profile);
        initial.xmm[9] = [0, 0];
        initial.xmm[10] = [0, 0];
        match profile {
            0 => initial.xmm[9][0] = 1,
            1 => initial.xmm[10][0] = 1,
            2 => {
                initial.xmm[9][0] = 1;
                initial.xmm[10][0] = 1;
            }
            3 => {
                initial.xmm[9][0] = 1;
                initial.xmm[10][0] = 3;
            }
            _ => unreachable!(),
        }
        let manual_rflags = apply_flags(&initial.xmm, initial.rflags, rex, modrm);
        let outcome = (usize::from(manual_rflags & (1 << 6) != 0) << 1)
            | usize::from(manual_rflags & 1 != 0);
        observed[outcome] = true;
        run_case(
            &[0x66, rex, 0x0F, 0x38, 0x17, modrm, 0xF4],
            &initial,
            manual_rflags,
            &format!("PTEST truth-table profile {profile}"),
        );
    }
    assert_eq!(observed, [true; 4]);
}
