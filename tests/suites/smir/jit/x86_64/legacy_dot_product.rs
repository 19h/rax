//! End-to-end native-JIT coverage for register-only legacy SSE4.1 `DPPS` and
//! `DPPD`.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DotKind {
    Dpps,
    Dppd,
}

impl DotKind {
    const ALL: [Self; 2] = [Self::Dpps, Self::Dppd];

    fn opcode(self) -> u8 {
        match self {
            Self::Dpps => 0x40,
            Self::Dppd => 0x41,
        }
    }

    fn lanes(self) -> usize {
        match self {
            Self::Dpps => 4,
            Self::Dppd => 2,
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

fn f32_lane(value: [u64; 2], lane: usize) -> f32 {
    f32::from_bits((value[lane / 2] >> ((lane % 2) * 32)) as u32)
}

fn set_f32_lane(value: &mut [u64; 2], lane: usize, result: f32) {
    let shift = (lane % 2) * 32;
    let mask = u64::from(u32::MAX) << shift;
    value[lane / 2] = (value[lane / 2] & !mask) | (u64::from(result.to_bits()) << shift);
}

fn apply(xmm: &mut [[u64; 2]; 16], kind: DotKind, rex: u8, modrm: u8, immediate: u8) {
    let destination = destination(rex, modrm);
    let source = source(rex, modrm);
    let first = xmm[destination];
    let second = xmm[source];
    match kind {
        DotKind::Dpps => {
            let mut products = [0.0f32; 4];
            for lane in 0..4 {
                if immediate & (1 << (lane + 4)) != 0 {
                    products[lane] = f32_lane(first, lane) * f32_lane(second, lane);
                }
            }
            let result = (products[0] + products[1]) + (products[2] + products[3]);
            for lane in 0..4 {
                set_f32_lane(
                    &mut xmm[destination],
                    lane,
                    if immediate & (1 << lane) != 0 {
                        result
                    } else {
                        0.0
                    },
                );
            }
        }
        DotKind::Dppd => {
            let mut products = [0.0f64; 2];
            for lane in 0..2 {
                if immediate & (1 << (lane + 4)) != 0 {
                    products[lane] = f64::from_bits(first[lane]) * f64::from_bits(second[lane]);
                }
            }
            let result = products[0] + products[1];
            for lane in 0..2 {
                xmm[destination][lane] = if immediate & (1 << lane) != 0 {
                    result.to_bits()
                } else {
                    0
                };
            }
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

fn pack_f32(low: f32, high: f32) -> u64 {
    u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32)
}

fn setup(vcpu: &mut X86_64Vcpu, kind: DotKind, profile: usize) -> Registers {
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
    registers.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        0xA5A5_5A5A_6996_9669u64.rotate_left((index * 9 + profile) as u32)
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        let base = (index as i32 + profile as i32) % 7 - 3;
        registers.xmm[index] = match kind {
            DotKind::Dpps => [
                pack_f32(base as f32, (base + 1) as f32),
                pack_f32((2 - base) as f32, (-base - 1) as f32),
            ],
            DotKind::Dppd => [f64::from(base).to_bits(), f64::from(2 - base).to_bits()],
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

fn run_case(code: &[u8], kind: DotKind, profile: usize, manual_xmm: [[u64; 2]; 16], label: &str) {
    let mut direct = make_vcpu_code(code);
    let initial = setup(&mut direct, kind, profile);
    run_interp(&mut direct);
    let expected = direct.get_regs().unwrap();
    assert_eq!(expected.xmm, manual_xmm, "{label}: direct finite equation");
    assert_eq!(gprs(&expected), gprs(&initial), "{label}: GPR state");
    assert_eq!(expected.ymm_high, initial.ymm_high, "{label}: YMM");
    assert_eq!(expected.zmm_high, initial.zmm_high, "{label}: ZMM");
    assert_eq!(expected.zmm_ext, initial.zmm_ext, "{label}: ZMM16-31");
    assert_eq!(expected.k, initial.k, "{label}: opmask");
    assert_eq!(expected.mm, initial.mm, "{label}: MMX");
    assert_eq!(expected.rflags, initial.rflags, "{label}: RFLAGS");

    let mut jit = make_vcpu_code(code);
    setup(&mut jit, kind, profile);
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
/// REX.W for each legacy dot-product opcode. Each byte image has 64 register
/// ModR/M cells and an immediate byte fixed to zero by the scanner.
///
/// Total: 2 families × 2 REX images × 64 register cells = 256 cells.
#[test]
fn jit_all_256_scanner_legacy_dot_product_gaps_match_direct_and_finite_equations() {
    assert!(std::is_x86_feature_detected!("sse4.1"));
    assert!(std::is_x86_feature_detected!("avx"));
    let mut cases = 0usize;
    for (kind_index, kind) in DotKind::ALL.into_iter().enumerate() {
        for (rex_index, rex) in SCANNER_REX_IMAGES.into_iter().enumerate() {
            let profile = kind_index * SCANNER_REX_IMAGES.len() + rex_index;
            let mut initial_vcpu = make_vcpu_code(&[0xF4]);
            let initial = setup(&mut initial_vcpu, kind, profile);
            let mut manual_xmm = initial.xmm;
            let mut code = Vec::new();
            for modrm in 0xC0..=0xFF {
                code.push(0x66);
                code.extend(rex);
                code.extend([0x0F, 0x3A, kind.opcode(), modrm, 0]);
                apply(&mut manual_xmm, kind, rex.unwrap_or(0), modrm, 0);
                cases += 1;
            }
            code.push(0xF4);
            run_case(
                &code,
                kind,
                profile,
                manual_xmm,
                &format!("{kind:?} scanner rex={rex:02X?}"),
            );
        }
    }
    assert_eq!(cases, DotKind::ALL.len() * SCANNER_REX_IMAGES.len() * 64);
}

#[test]
fn jit_all_512_immediate_controls_match_direct_and_finite_equations() {
    assert!(std::is_x86_feature_detected!("sse4.1"));
    assert!(std::is_x86_feature_detected!("avx"));
    let mut cases = 0usize;
    for (kind_index, kind) in DotKind::ALL.into_iter().enumerate() {
        for chunk_start in (0usize..256).step_by(15) {
            let profile = 8 + kind_index * 32 + chunk_start / 15;
            let mut initial_vcpu = make_vcpu_code(&[0xF4]);
            let initial = setup(&mut initial_vcpu, kind, profile);
            let mut manual_xmm = initial.xmm;
            let mut code = Vec::new();
            for (destination, immediate) in
                (chunk_start..usize::min(chunk_start + 15, 256)).enumerate()
            {
                let rex = 0x41 | if destination >= 8 { 0x04 } else { 0 };
                let modrm = 0xC0 | ((destination as u8 & 7) << 3) | 7;
                code.extend([0x66, rex, 0x0F, 0x3A, kind.opcode(), modrm, immediate as u8]);
                apply(&mut manual_xmm, kind, rex, modrm, immediate as u8);
                cases += 1;
            }
            code.push(0xF4);
            run_case(
                &code,
                kind,
                profile,
                manual_xmm,
                &format!("{kind:?} immediates {chunk_start}..{}", chunk_start + 15),
            );
        }
    }
    assert_eq!(cases, DotKind::ALL.len() * 256);
    assert_eq!(DotKind::Dpps.lanes(), 4);
    assert_eq!(DotKind::Dppd.lanes(), 2);
}
