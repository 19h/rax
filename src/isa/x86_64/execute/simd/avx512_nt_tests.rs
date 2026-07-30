//! Direct-execution regressions for legacy, VEX, and EVEX MOVNTDQA loads.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x3000;
const SENTINEL: u64 = 0xA55A_6996_F00F_3CC3;
const WORDS: [u64; 8] = [
    0x0123_4567_89AB_CDEF,
    0xFEDC_BA98_7654_3210,
    0x1111_2222_3333_4444,
    0xAAAA_BBBB_CCCC_DDDD,
    0x1357_9BDF_2468_ACE0,
    0x0F1E_2D3C_4B5A_6978,
    0x8000_0000_0000_0001,
    0x7FFF_FFFF_FFFF_FFFE,
];

fn vcpu_with_ranges(code: &[u8], ranges: &[(GuestAddress, usize)]) -> X86_64Vcpu {
    let memory = Arc::new(GuestMemoryMmap::<()>::from_ranges(ranges).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = 0x2 | 0x8D5;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    seed_registers(&mut vcpu.regs);
    vcpu
}

fn vcpu(code: &[u8]) -> X86_64Vcpu {
    vcpu_with_ranges(code, &[(GuestAddress(0), 0x10000)])
}

fn partial_vcpu(code: &[u8], mapped_data_bytes: usize) -> X86_64Vcpu {
    vcpu_with_ranges(
        code,
        &[
            (GuestAddress(0), 0x2000),
            (GuestAddress(DATA), mapped_data_bytes),
        ],
    )
}

fn seed_registers(regs: &mut Registers) {
    regs.rcx = 0x1111_2222_3333_4444;
    regs.rdx = 0x5555_6666_7777_8888;
    regs.rbx = 0x9999_AAAA_BBBB_CCCC;
    regs.rsp = 0x1F00;
    regs.rbp = 0x1810;
    regs.rsi = 0x0123_4567_89AB_CDEF;
    regs.rdi = 0xFEDC_BA98_7654_3210;
    regs.r8 = 0x0808_0808_0808_0808;
    regs.r9 = 0x0909_0909_0909_0909;
    regs.r10 = 0x1010_1010_1010_1010;
    regs.r11 = 0x1111_1111_1111_1111;
    regs.r12 = 0x1212_1212_1212_1212;
    regs.r13 = 0x1313_1313_1313_1313;
    regs.r14 = 0x1414_1414_1414_1414;
    regs.r15 = 0x1515_1515_1515_1515;
    regs.r16 = 0x1616_1616_1616_1616;
    regs.r17 = 0x1717_1717_1717_1717;
    regs.r18 = 0x1818_1818_1818_1818;
    regs.r19 = 0x1919_1919_1919_1919;
    regs.r20 = 0x2020_2020_2020_2020;
    regs.r21 = 0x2121_2121_2121_2121;
    regs.r22 = 0x2222_2222_2222_2222;
    regs.r23 = 0x2323_2323_2323_2323;
    regs.r24 = 0x2424_2424_2424_2424;
    regs.r25 = 0x2525_2525_2525_2525;
    regs.r26 = 0x2626_2626_2626_2626;
    regs.r27 = 0x2727_2727_2727_2727;
    regs.r28 = 0x2828_2828_2828_2828;
    regs.r29 = 0x2929_2929_2929_2929;
    regs.r30 = 0x3030_3030_3030_3030;
    regs.r31 = 0x3131_3131_3131_3131;
    regs.xmm = std::array::from_fn(|index| {
        [
            SENTINEL.rotate_left((index * 7) as u32),
            (!SENTINEL).rotate_right((index * 11) as u32),
        ]
    });
    regs.ymm_high = std::array::from_fn(|index| {
        [
            SENTINEL.rotate_right((index * 5) as u32),
            (!SENTINEL).rotate_left((index * 3) as u32),
        ]
    });
    regs.zmm_high = std::array::from_fn(|index| {
        std::array::from_fn(|word| SENTINEL.rotate_left((index * 13 + word * 17) as u32))
    });
    regs.zmm_ext = std::array::from_fn(|index| {
        std::array::from_fn(|word| (!SENTINEL).rotate_right((index * 9 + word * 19) as u32))
    });
    regs.k = std::array::from_fn(|index| SENTINEL.rotate_left((index * 9) as u32));
    regs.mm = std::array::from_fn(|index| (!SENTINEL).rotate_right((index * 7) as u32));
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn assert_registers_equal(actual: &Registers, expected: &Registers, context: &str) {
    assert_eq!(gprs(actual), gprs(expected), "{context}: GPRs");
    assert_eq!(actual.xmm, expected.xmm, "{context}: XMM");
    assert_eq!(actual.ymm_high, expected.ymm_high, "{context}: YMM high");
    assert_eq!(actual.zmm_high, expected.zmm_high, "{context}: ZMM high");
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "{context}: ZMM16-ZMM31");
    assert_eq!(actual.k, expected.k, "{context}: opmasks");
    assert_eq!(actual.mm, expected.mm, "{context}: MMX");
    assert_eq!(actual.rflags, expected.rflags, "{context}: RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{context}: RIP");
}

fn set_vector(regs: &mut Registers, register: u8, value: [u64; 8]) {
    if register >= 16 {
        regs.zmm_ext[usize::from(register - 16)] = value;
        return;
    }

    let index = usize::from(register);
    regs.xmm[index].copy_from_slice(&value[..2]);
    regs.ymm_high[index].copy_from_slice(&value[2..4]);
    regs.zmm_high[index].copy_from_slice(&value[4..]);
}

fn set_loaded_vector(regs: &mut Registers, register: u8, width: usize) {
    assert!(matches!(width, 16 | 32 | 64));
    let mut value = [0u64; 8];
    value[..width / 8].copy_from_slice(&WORDS[..width / 8]);
    set_vector(regs, register, value);
}

fn write_words(vcpu: &mut X86_64Vcpu, count: usize) {
    for (index, word) in WORDS.iter().take(count).enumerate() {
        vcpu.write_mem(DATA + (index * 8) as u64, *word, 8).unwrap();
    }
}

fn vex_encoding(width: usize, destination: u8, w: bool) -> [u8; 5] {
    assert!(matches!(width, 16 | 32) && destination < 16);
    let mut p0 = 0xE2;
    if destination >= 8 {
        p0 &= !0x80;
    }
    [
        0xC4,
        p0,
        0x79 | (u8::from(width == 32) << 2) | (u8::from(w) << 7),
        0x2A,
        (destination & 7) << 3,
    ]
}

fn evex_encoding(width: usize, destination: u8) -> [u8; 6] {
    assert!(matches!(width, 16 | 32 | 64) && destination < 32);
    let mut p0 = 0xF2;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    let ll = match width {
        16 => 0,
        32 => 1,
        64 => 2,
        _ => unreachable!(),
    };
    [
        0x62,
        p0,
        0x7D,
        0x08 | (ll << 5),
        0x2A,
        (destination & 7) << 3,
    ]
}

fn expect_gp_noncommitting(vcpu: &mut X86_64Vcpu, context: &str) {
    let before = vcpu.regs.clone();
    let error = format!(
        "{:#}",
        vcpu.step()
            .expect_err("misaligned MOVNTDQA must raise #GP(0)")
    );
    assert!(
        error.contains("IDT entry 13 not present"),
        "{context}: expected #GP(0), got {error}"
    );
    assert_registers_equal(&vcpu.regs, &before, context);
}

fn expect_memory_fault_noncommitting(vcpu: &mut X86_64Vcpu, context: &str) {
    let before = vcpu.regs.clone();
    vcpu.step()
        .expect_err("unmapped later MOVNTDQA bytes must fault");
    assert_registers_equal(&vcpu.regs, &before, context);
}

#[test]
fn vex_movntdqa_all_destinations_widths_and_wig_values_zero_upper_bits() {
    for width in [16, 32] {
        for destination in 0..16 {
            for w in [false, true] {
                let code = vex_encoding(width, destination, w);
                let mut cpu = vcpu(&code);
                cpu.regs.rax = DATA;
                write_words(&mut cpu, width / 8);

                // Make every low bit that the VEX wrapper observes equal to the
                // load result. Architectural upper clearing must still occur.
                set_loaded_vector(&mut cpu.regs, destination, width);
                cpu.regs.zmm_high[usize::from(destination)] = [SENTINEL; 4];
                let mut expected = cpu.regs.clone();
                expected.zmm_high[usize::from(destination)] = [0; 4];
                expected.rip += code.len() as u64;

                assert!(cpu.step().unwrap().is_none());
                assert_registers_equal(
                    &cpu.regs,
                    &expected,
                    &format!("VEX width={width} destination={destination} W={w}"),
                );
            }
        }
    }
}

#[test]
fn vex_movntdqa_alignment_and_later_faults_are_precise_and_noncommitting() {
    for width in [16, 32] {
        for destination in 0..16 {
            for w in [false, true] {
                let code = vex_encoding(width, destination, w);
                let mut cpu = vcpu(&code);
                cpu.regs.rax = DATA + 1;
                expect_gp_noncommitting(
                    &mut cpu,
                    &format!("VEX misaligned width={width} destination={destination} W={w}"),
                );
            }
        }

        for w in [false, true] {
            let code = vex_encoding(width, 13, w);
            let mut cpu = partial_vcpu(&code, width / 2);
            cpu.regs.rax = DATA;
            write_words(&mut cpu, width / 16);
            expect_memory_fault_noncommitting(
                &mut cpu,
                &format!("VEX later fault width={width} W={w}"),
            );
        }
    }
}

#[test]
fn legacy_movntdqa_enforces_alignment_and_stages_the_complete_load() {
    let code = [0x66, 0x0F, 0x38, 0x2A, 0x08];

    let mut aligned = vcpu(&code);
    aligned.regs.rax = DATA;
    write_words(&mut aligned, 2);
    let mut expected = aligned.regs.clone();
    expected.xmm[1] = [WORDS[0], WORDS[1]];
    expected.rip += code.len() as u64;
    assert!(aligned.step().unwrap().is_none());
    assert_registers_equal(&aligned.regs, &expected, "legacy aligned success");

    let mut misaligned = vcpu(&code);
    misaligned.regs.rax = DATA + 1;
    expect_gp_noncommitting(&mut misaligned, "legacy misaligned");

    let mut later_fault = partial_vcpu(&code, 8);
    later_fault.regs.rax = DATA;
    write_words(&mut later_fault, 1);
    expect_memory_fault_noncommitting(&mut later_fault, "legacy later fault");
}

#[test]
fn evex_movntdqa_all_vector_lengths_align_and_commit_atomically() {
    for width in [16, 32, 64] {
        for destination in 0..32 {
            let code = evex_encoding(width, destination);
            let mut cpu = vcpu(&code);
            cpu.regs.rax = DATA;
            write_words(&mut cpu, width / 8);
            let mut expected = cpu.regs.clone();
            set_loaded_vector(&mut expected, destination, width);
            expected.rip += code.len() as u64;

            assert!(cpu.step().unwrap().is_none());
            assert_registers_equal(
                &cpu.regs,
                &expected,
                &format!("EVEX width={width} destination={destination}"),
            );

            let mut misaligned = vcpu(&code);
            misaligned.regs.rax = DATA + 1;
            expect_gp_noncommitting(
                &mut misaligned,
                &format!("EVEX misaligned width={width} destination={destination}"),
            );
        }

        let code = evex_encoding(width, 29);
        let mut later_fault = partial_vcpu(&code, width / 2);
        later_fault.regs.rax = DATA;
        write_words(&mut later_fault, width / 16);
        expect_memory_fault_noncommitting(
            &mut later_fault,
            &format!("EVEX later fault width={width}"),
        );
    }
}
