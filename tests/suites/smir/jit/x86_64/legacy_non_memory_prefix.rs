//! End-to-end native replay for decoder-accepted register instructions carrying
//! a reserved segment-override or address-size prefix.

use super::*;

#[derive(Clone, Copy, Debug)]
struct Family {
    name: &'static str,
    opcode: u8,
    immediate: bool,
}

const PREFIXES: [&[u8]; 3] = [&[0x64], &[0x65], &[0x67]];

// Exact family set responsible for 4,224 of the 4,320 scanner cells closed by
// non-memory prefix canonicalization. Each family contributes three prefix
// images and all 64 register ModR/M cells.
const FAMILIES: [Family; 22] = [
    Family {
        name: "ADDPS",
        opcode: 0x58,
        immediate: false,
    },
    Family {
        name: "MULPS",
        opcode: 0x59,
        immediate: false,
    },
    Family {
        name: "SUBPS",
        opcode: 0x5C,
        immediate: false,
    },
    Family {
        name: "MINPS",
        opcode: 0x5D,
        immediate: false,
    },
    Family {
        name: "DIVPS",
        opcode: 0x5E,
        immediate: false,
    },
    Family {
        name: "MAXPS",
        opcode: 0x5F,
        immediate: false,
    },
    Family {
        name: "CMPPS",
        opcode: 0xC2,
        immediate: true,
    },
    Family {
        name: "UCOMISS",
        opcode: 0x2E,
        immediate: false,
    },
    Family {
        name: "COMISS",
        opcode: 0x2F,
        immediate: false,
    },
    Family {
        name: "CVTPI2PS",
        opcode: 0x2A,
        immediate: false,
    },
    Family {
        name: "CVTTPS2PI",
        opcode: 0x2C,
        immediate: false,
    },
    Family {
        name: "CVTPS2PI",
        opcode: 0x2D,
        immediate: false,
    },
    Family {
        name: "CVTPS2PD",
        opcode: 0x5A,
        immediate: false,
    },
    Family {
        name: "MOVHLPS",
        opcode: 0x12,
        immediate: false,
    },
    Family {
        name: "MOVLHPS",
        opcode: 0x16,
        immediate: false,
    },
    Family {
        name: "PMULUDQ",
        opcode: 0xF4,
        immediate: false,
    },
    Family {
        name: "RSQRTPS",
        opcode: 0x52,
        immediate: false,
    },
    Family {
        name: "RCPPS",
        opcode: 0x53,
        immediate: false,
    },
    Family {
        name: "SHUFPS",
        opcode: 0xC6,
        immediate: true,
    },
    Family {
        name: "SQRTPS",
        opcode: 0x51,
        immediate: false,
    },
    Family {
        name: "UNPCKLPS",
        opcode: 0x14,
        immediate: false,
    },
    Family {
        name: "UNPCKHPS",
        opcode: 0x15,
        immediate: false,
    },
];

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

fn setup(vcpu: &mut X86_64Vcpu, profile: usize) {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr4 |= 1 << 9; // CR4.OSFXSR
    vcpu.set_sregs(&sregs).unwrap();

    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = 0x0123_4567_89AB_CDEF ^ profile as u64;
    registers.rcx = 0x1234_5678_9ABC_DEF0 ^ (profile as u64).rotate_left(3);
    registers.rdx = 0x2345_6789_ABCD_EF01 ^ (profile as u64).rotate_left(7);
    registers.rbx = 0x3456_789A_BCDE_F012 ^ (profile as u64).rotate_left(11);
    registers.rsp = 0x1111_2222_3333_4444;
    registers.rbp = 0x5555_6666_7777_8888;
    registers.rsi = 0x9999_AAAA_BBBB_CCCC;
    registers.rdi = 0xDDDD_EEEE_FFFF_0001;
    registers.r8 = 0x789A_BCDE_F012_3456;
    registers.r9 = 0x89AB_CDEF_0123_4567;
    registers.r10 = 0x9ABC_DEF0_1234_5678;
    registers.r11 = 0xABCD_EF01_2345_6789;
    registers.r12 = 0xBCDE_F012_3456_789A;
    registers.r13 = 0xCDEF_0123_4567_89AB;
    registers.r14 = 0xDEF0_1234_5678_9ABC;
    registers.r15 = 0xEF01_2345_6789_ABCD;
    registers.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        0x1020_3040_5060_7080u64.rotate_left((index * 7 + profile) as u32)
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        // Each eight-instruction batch updates one ModR/M.reg row. Keep every
        // input finite, normal, nonzero, and within a narrow exact-binary range
        // so repeated arithmetic cannot introduce accidental overflow,
        // underflow, NaNs, or divide-by-zero while source/register routing still
        // varies by lane, register, and profile.
        let phase = ((profile * 3 + index * 5) & 7) as f32;
        let low = 0.75f32 + phase * 0.03125;
        let high = 1.0f32 + phase * 0.03125;
        registers.xmm[index] = [
            u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32),
            u64::from((low + 0.5).to_bits()) | (u64::from((high + 0.5).to_bits()) << 32),
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

fn compare_direct_and_native(code: &[u8], profile: usize, label: &str) {
    let mut direct = make_vcpu_code(code);
    setup(&mut direct, profile);
    run_interp(&mut direct);
    let expected = direct.get_regs().unwrap();

    let mut jit = make_vcpu_code(code);
    setup(&mut jit, profile);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{label}: native admission: {error:?}")),
        "{label}: every prefixed register cell must enter the native tier:\n{}",
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

fn host_supports_family(family: Family) -> bool {
    family.opcode != 0xC2
        || (std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw"))
}

#[test]
fn jit_all_host_supported_scanner_non_memory_prefix_cells_match_direct_full_state() {
    assert!(std::is_x86_feature_detected!("avx"));

    let full_vector_bridge =
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw");
    let mut catalogued_cases = 0usize;
    let mut native_cases = 0usize;
    for (family_index, family) in FAMILIES.into_iter().enumerate() {
        for (prefix_index, prefix) in PREFIXES.into_iter().enumerate() {
            if !host_supports_family(family) {
                catalogued_cases += 64;
                eprintln!(
                    "skipping {} {prefix:02X?} native differential: full AVX-512F/BW state bridge unavailable",
                    family.name
                );
                continue;
            }
            for reg in 0u8..8 {
                let first = 0xC0 | (reg << 3);
                let last = first | 7;
                let mut code = Vec::new();
                for modrm in first..=last {
                    code.extend_from_slice(prefix);
                    code.extend_from_slice(&[0x0F, family.opcode, modrm]);
                    if family.immediate {
                        code.push(0);
                    }
                    catalogued_cases += 1;
                    native_cases += 1;
                }
                code.push(0xF4);
                compare_direct_and_native(
                    &code,
                    (family_index * PREFIXES.len() + prefix_index) * 8 + usize::from(reg),
                    &format!("{} {prefix:02X?} {first:02X}-{last:02X}", family.name),
                );
            }
        }
    }

    // PEXTRW destinations and PINSRW sources that name guest RSP/RBP account
    // for the remaining 96 scanner cells. The state-backed wrappers must keep
    // the native host stack and frame pointers private.
    for (opcode, stack_in_reg_field) in [(0xC5, true), (0xC4, false)] {
        for (prefix_index, prefix) in PREFIXES.into_iter().enumerate() {
            let mut code = Vec::new();
            for first in 0u8..8 {
                for stack in [4u8, 5] {
                    let fields = if stack_in_reg_field {
                        (stack << 3) | first
                    } else {
                        (first << 3) | stack
                    };
                    code.extend_from_slice(prefix);
                    code.extend_from_slice(&[0x0F, opcode, 0xC0 | fields, 0]);
                    catalogued_cases += 1;
                    native_cases += 1;
                }
            }
            code.push(0xF4);
            compare_direct_and_native(
                &code,
                FAMILIES.len() * PREFIXES.len()
                    + usize::from(opcode == 0xC4) * PREFIXES.len()
                    + prefix_index,
                &format!("opcode {opcode:02X} {prefix:02X?}"),
            );
        }
    }

    assert_eq!(catalogued_cases, 22 * 3 * 64 + 2 * 3 * 16);
    assert_eq!(native_cases, if full_vector_bridge { 4_320 } else { 4_128 });
    eprintln!("executed {native_cases} native/direct full-state scanner cells");
}
