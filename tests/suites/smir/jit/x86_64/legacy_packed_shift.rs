//! End-to-end native-JIT coverage for register-only legacy SSE2 packed shifts.

use super::*;

#[derive(Clone, Copy, Debug)]
struct Operation {
    name: &'static str,
    opcode: u8,
    group: Option<u8>,
    element_bits: u8,
    arithmetic: bool,
    left: bool,
    byte_lane: bool,
}

impl Operation {
    const fn immediate(
        name: &'static str,
        opcode: u8,
        group: u8,
        element_bits: u8,
        arithmetic: bool,
        left: bool,
        byte_lane: bool,
    ) -> Self {
        Self {
            name,
            opcode,
            group: Some(group),
            element_bits,
            arithmetic,
            left,
            byte_lane,
        }
    }

    const fn register(
        name: &'static str,
        opcode: u8,
        element_bits: u8,
        arithmetic: bool,
        left: bool,
    ) -> Self {
        Self {
            name,
            opcode,
            group: None,
            element_bits,
            arithmetic,
            left,
            byte_lane: false,
        }
    }
}

const OPERATIONS: [Operation; 18] = [
    Operation::immediate("PSRLW imm", 0x71, 2, 16, false, false, false),
    Operation::immediate("PSRAW imm", 0x71, 4, 16, true, false, false),
    Operation::immediate("PSLLW imm", 0x71, 6, 16, false, true, false),
    Operation::immediate("PSRLD imm", 0x72, 2, 32, false, false, false),
    Operation::immediate("PSRAD imm", 0x72, 4, 32, true, false, false),
    Operation::immediate("PSLLD imm", 0x72, 6, 32, false, true, false),
    Operation::immediate("PSRLQ imm", 0x73, 2, 64, false, false, false),
    Operation::immediate("PSRLDQ", 0x73, 3, 8, false, false, true),
    Operation::immediate("PSLLQ imm", 0x73, 6, 64, false, true, false),
    Operation::immediate("PSLLDQ", 0x73, 7, 8, false, true, true),
    Operation::register("PSRLW xmm", 0xD1, 16, false, false),
    Operation::register("PSRLD xmm", 0xD2, 32, false, false),
    Operation::register("PSRLQ xmm", 0xD3, 64, false, false),
    Operation::register("PSRAW xmm", 0xE1, 16, true, false),
    Operation::register("PSRAD xmm", 0xE2, 32, true, false),
    Operation::register("PSLLW xmm", 0xF1, 16, false, true),
    Operation::register("PSLLD xmm", 0xF2, 32, false, true),
    Operation::register("PSLLQ xmm", 0xF3, 64, false, true),
];

fn shifted_lane(value: u64, bits: u8, count: u64, operation: Operation) -> u64 {
    let bits = u64::from(bits);
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let value = value & mask;
    if operation.left {
        return (count < bits).then(|| (value << count) & mask).unwrap_or(0);
    }
    if !operation.arithmetic {
        return (count < bits).then(|| value >> count).unwrap_or(0);
    }

    let sign = value & (1u64 << (bits - 1)) != 0;
    if count >= bits {
        return if sign { mask } else { 0 };
    }
    let signed = if bits == 64 {
        value as i64
    } else {
        ((value << (64 - bits)) as i64) >> (64 - bits)
    };
    (signed >> count) as u64 & mask
}

fn apply_operation(
    xmm: &mut [[u64; 2]; 16],
    operation: Operation,
    rex: u8,
    modrm: u8,
    immediate: u8,
) {
    let destination = if operation.group.is_some() {
        usize::from((modrm & 7) | ((rex & 1) << 3))
    } else {
        usize::from(((modrm >> 3) & 7) | ((rex & 4) << 1))
    };
    let count = if operation.group.is_some() {
        u64::from(immediate)
    } else {
        let source = usize::from((modrm & 7) | ((rex & 1) << 3));
        xmm[source][0]
    };
    let input = u128::from(xmm[destination][0]) | (u128::from(xmm[destination][1]) << 64);
    let output = if operation.byte_lane {
        let bits = count.saturating_mul(8);
        if bits >= 128 {
            0
        } else if operation.left {
            input << bits
        } else {
            input >> bits
        }
    } else {
        let bits = u64::from(operation.element_bits);
        let lane_mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let mut result = 0u128;
        for lane in 0..(128 / bits) {
            let value = ((input >> (lane * bits)) as u64) & lane_mask;
            result |= u128::from(shifted_lane(
                value,
                operation.element_bits,
                count,
                operation,
            )) << (lane * bits);
        }
        result
    };
    xmm[destination] = [output as u64, (output >> 64) as u64];
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
        0xA100_0000_0000_0000 | ((profile as u64) << 16) | index as u64
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    let count_values = [
        0,
        1,
        15,
        16,
        17,
        31,
        32,
        33,
        63,
        64,
        65,
        127,
        128,
        255,
        0x0000_0001_0000_0000,
        u64::MAX,
    ];
    for index in 0..16 {
        registers.xmm[index] = [
            count_values[(index + profile) % count_values.len()],
            0x8123_C567_09AB_CDEFu64.rotate_left((index * 11 + profile * 7) as u32)
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

/// The independent scanner reports two canonical prefix images (`66` and
/// `66 48`). Ten immediate forms contribute 10 × 2 × 8 = 160 register cells;
/// eight shared-count forms contribute 8 × 2 × 64 = 1,024 cells. Total:
/// 1,184 newly admitted cells across ten mnemonics.
#[test]
fn jit_all_1184_scanner_legacy_packed_shift_gaps_match_direct_and_manual_equations() {
    assert!(std::is_x86_feature_detected!("sse2"));
    assert!(std::is_x86_feature_detected!("avx"));

    let mut cases = 0usize;
    for (operation_index, operation) in OPERATIONS.into_iter().enumerate() {
        for (prefix_index, prefix) in [&[0x66][..], &[0x66, 0x48][..]].into_iter().enumerate() {
            let rex = prefix.get(1).copied().unwrap_or(0);
            let mut code = Vec::new();
            if let Some(group) = operation.group {
                for destination in 0..8 {
                    code.extend_from_slice(prefix);
                    code.extend_from_slice(&[
                        0x0F,
                        operation.opcode,
                        0xC0 | (group << 3) | destination,
                        0,
                    ]);
                    cases += 1;
                }
            } else {
                for modrm in 0xC0..=0xFF {
                    code.extend_from_slice(prefix);
                    code.extend_from_slice(&[0x0F, operation.opcode, modrm]);
                    cases += 1;
                }
            }
            code.push(0xF4);
            let profile = operation_index * 2 + prefix_index;
            let label = format!("{} {prefix:02X?}", operation.name);

            let mut direct = make_vcpu_code(&code);
            let initial = setup(&mut direct, profile);
            let mut manual_xmm = initial.xmm;
            if let Some(group) = operation.group {
                for destination in 0..8 {
                    apply_operation(
                        &mut manual_xmm,
                        operation,
                        rex,
                        0xC0 | (group << 3) | destination,
                        0,
                    );
                }
            } else {
                for modrm in 0xC0..=0xFF {
                    apply_operation(&mut manual_xmm, operation, rex, modrm, 0);
                }
            }
            run_interp(&mut direct);
            let expected = direct.get_regs().unwrap();
            assert_eq!(expected.xmm, manual_xmm, "{label}: direct equation");
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
    assert_eq!(cases, 10 * 2 * 8 + 8 * 2 * 64);
}
