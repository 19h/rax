//! End-to-end native-JIT coverage for register-only legacy MMX/SSE
//! `PMULUDQ` and SSE4.1 `PMULDQ`.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    MmxUnsigned,
    XmmUnsigned,
    XmmSigned,
}

fn apply_xmm(xmm: &mut [[u64; 2]; 16], shape: Shape, modrm: u8) {
    let destination = usize::from((modrm >> 3) & 7);
    let source = usize::from(modrm & 7);
    let lhs = xmm[destination];
    let rhs = xmm[source];
    for lane in 0..2 {
        xmm[destination][lane] = if shape == Shape::XmmSigned {
            (i64::from(lhs[lane] as u32 as i32)).wrapping_mul(i64::from(rhs[lane] as u32 as i32))
                as u64
        } else {
            u64::from(lhs[lane] as u32) * u64::from(rhs[lane] as u32)
        };
    }
}

fn apply_mmx(mm: &mut [u64; 8], modrm: u8) {
    let destination = usize::from((modrm >> 3) & 7);
    let source = usize::from(modrm & 7);
    let lhs = mm[destination] as u32;
    let rhs = mm[source] as u32;
    mm[destination] = u64::from(lhs) * u64::from(rhs);
}

fn setup(vcpu: &mut X86_64Vcpu, profile: usize) -> Registers {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr4 |= 1 << 9; // CR4.OSFXSR
    vcpu.set_sregs(&sregs).unwrap();

    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = 0x0123_4567_89AB_CDEF ^ profile as u64;
    registers.rbx = 0xFEDC_BA98_7654_3210 ^ (profile as u64).rotate_left(13);
    registers.rcx = 0x8000_0000_0000_0001;
    registers.rdx = 0x7FFF_FFFF_FFFF_FFFE;
    registers.rsi = 0x1111_2222_3333_4444;
    registers.rdi = 0x5555_6666_7777_8888;
    registers.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        0x8000_0001_FFFF_FFFFu64.rotate_left((index * 7 + profile * 11) as u32)
            ^ (index as u64).wrapping_mul(0x0102_0408_1020_4081)
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        registers.xmm[index] = [
            0x8000_0001_FFFF_FFFFu64.rotate_left((index * 5 + profile * 3) as u32)
                ^ (index as u64).wrapping_mul(0x0101_1111_2222_3333),
            0xFFFF_FFFE_7FFF_FFFFu64.rotate_left((index * 11 + profile * 7) as u32)
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
    assert_eq!(actual.xmm, expected.xmm, "{label}: XMM");
    assert_eq!(actual.ymm_high, expected.ymm_high, "{label}: YMM");
    assert_eq!(actual.zmm_high, expected.zmm_high, "{label}: ZMM");
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "{label}: ZMM16-31");
    assert_eq!(actual.k, expected.k, "{label}: opmask");
    assert_eq!(actual.mm, expected.mm, "{label}: MMX");
    assert_eq!(actual.rax, expected.rax, "{label}: RAX");
    assert_eq!(actual.rbx, expected.rbx, "{label}: RBX");
    assert_eq!(actual.rcx, expected.rcx, "{label}: RCX");
    assert_eq!(actual.rdx, expected.rdx, "{label}: RDX");
    assert_eq!(actual.rsi, expected.rsi, "{label}: RSI");
    assert_eq!(actual.rdi, expected.rdi, "{label}: RDI");
    assert_eq!(actual.rflags, expected.rflags, "{label}: RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{label}: RIP");
}

/// The independent scanner reports:
///
/// * 128 SSE2 `PMULUDQ` cells: 2 prefix images × 64 ModR/M registers;
/// * 128 SSE4.1 `PMULDQ` cells: 2 prefix images × 64 ModR/M registers;
/// * 320 MMX `PMULUDQ` cells: 5 prefix images × 64 ModR/M registers.
///
/// Total: 128 + 128 + 320 = 576 newly admitted cells.
#[test]
fn jit_all_576_scanner_legacy_widening_multiply_gaps_match_direct_and_intel_equations() {
    assert!(std::is_x86_feature_detected!("avx"));
    assert!(std::is_x86_feature_detected!("sse4.1"));

    let families: &[(Shape, &[&[u8]])] = &[
        (Shape::XmmUnsigned, &[&[0x66], &[0x66, 0x48]]),
        (Shape::XmmSigned, &[&[0x66], &[0x66, 0x48]]),
        (
            Shape::MmxUnsigned,
            &[&[], &[0x41], &[0x44], &[0x48], &[0x4D]],
        ),
    ];
    let mut cases = 0usize;
    for (family_index, (shape, prefixes)) in families.iter().enumerate() {
        for (prefix_index, prefix) in prefixes.iter().enumerate() {
            let mut code = Vec::new();
            for modrm in 0xC0..=0xFF {
                code.extend_from_slice(prefix);
                match shape {
                    Shape::MmxUnsigned | Shape::XmmUnsigned => {
                        code.extend_from_slice(&[0x0F, 0xF4, modrm]);
                    }
                    Shape::XmmSigned => {
                        code.extend_from_slice(&[0x0F, 0x38, 0x28, modrm]);
                    }
                }
                cases += 1;
            }
            code.push(0xF4);
            let profile = family_index * 8 + prefix_index;
            let label = format!("{shape:?} {prefix:02X?}");

            let mut direct = make_vcpu_code(&code);
            let initial = setup(&mut direct, profile);
            let mut manual_xmm = initial.xmm;
            let mut manual_mm = initial.mm;
            for modrm in 0xC0..=0xFF {
                if *shape == Shape::MmxUnsigned {
                    apply_mmx(&mut manual_mm, modrm);
                } else {
                    apply_xmm(&mut manual_xmm, *shape, modrm);
                }
            }
            run_interp(&mut direct);
            let expected = direct.get_regs().unwrap();
            assert_eq!(expected.xmm, manual_xmm, "{label}: direct vs Intel XMM");
            assert_eq!(expected.mm, manual_mm, "{label}: direct vs Intel MMX");
            assert_eq!(expected.ymm_high, initial.ymm_high, "{label}: YMM");
            assert_eq!(expected.zmm_high, initial.zmm_high, "{label}: ZMM");
            assert_eq!(expected.zmm_ext, initial.zmm_ext, "{label}: ZMM16-31");
            assert_eq!(expected.rflags, initial.rflags, "{label}: flags");

            let mut jit = make_vcpu_code(&code);
            setup(&mut jit, profile);
            jit.set_jit_call(false);
            jit.set_jit_mem(false);
            assert!(
                jit.jit_try_block()
                    .unwrap_or_else(|error| panic!("{label}: native admission: {error:?}")),
                "{label}: all register cells must enter the native tier:\n{}",
                jit.jit_dump_region(LOAD_ADDR)
            );
            assert_eq!(
                jit.get_regs().unwrap().rip,
                LOAD_ADDR + code.len() as u64 - 1,
                "{label}: HLT frontier"
            );
            run_interp(&mut jit);
            let actual = jit.get_regs().unwrap();
            assert_full_state(&actual, &expected, &label);
        }
    }
    assert_eq!(cases, 576);
}
