//! End-to-end native-JIT coverage for state-backed and APX register MUL/IMUL.

use super::*;

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

fn seed(vcpu: &mut X86_64Vcpu, small: bool) -> Registers {
    let mut registers = vcpu.get_regs().unwrap();
    let base = if small { 2 } else { 0x8123_4567_89AB_CDEF };
    registers.rax = base;
    registers.rcx = base.wrapping_add(0x1111_2222_3333_4444);
    registers.rdx = base.wrapping_add(0x2222_3333_4444_5555);
    registers.rbx = base.wrapping_add(0x3333_4444_5555_6666);
    registers.rsp = base.wrapping_add(0x4444_5555_6666_7777);
    registers.rbp = base.wrapping_add(0x5555_6666_7777_8888);
    registers.rsi = base.wrapping_add(0x6666_7777_8888_9999);
    registers.rdi = base.wrapping_add(0x7777_8888_9999_AAAA);
    registers.r8 = base.wrapping_add(0x8888_9999_AAAA_BBBB);
    registers.r9 = base.wrapping_add(0x9999_AAAA_BBBB_CCCC);
    registers.r10 = base.wrapping_add(0xAAAA_BBBB_CCCC_DDDD);
    registers.r11 = base.wrapping_add(0xBBBB_CCCC_DDDD_EEEE);
    registers.r12 = base.wrapping_add(0xCCCC_DDDD_EEEE_FFFF);
    registers.r13 = base.wrapping_add(0xDDDD_EEEE_FFFF_0001);
    registers.r14 = base.wrapping_add(0xEEEE_FFFF_0001_1112);
    registers.r15 = base.wrapping_add(0xFFFF_0001_1112_2223);
    registers.r16 = base.wrapping_add(0x1020_3040_5060_7080);
    registers.r17 = base.wrapping_add(0x2030_4050_6070_8090);
    registers.r18 = base.wrapping_add(0x3040_5060_7080_90A0);
    registers.r19 = base.wrapping_add(0x4050_6070_8090_A0B0);
    registers.r20 = base.wrapping_add(0x5060_7080_90A0_B0C0);
    registers.r21 = base.wrapping_add(0x6070_8090_A0B0_C0D0);
    registers.r22 = base.wrapping_add(0x7080_90A0_B0C0_D0E0);
    registers.r23 = base.wrapping_add(0x8090_A0B0_C0D0_E0F0);
    registers.r24 = base.wrapping_add(0x90A0_B0C0_D0E0_F001);
    registers.r25 = base.wrapping_add(0xA0B0_C0D0_E0F0_0112);
    registers.r26 = base.wrapping_add(0xB0C0_D0E0_F001_1223);
    registers.r27 = base.wrapping_add(0xC0D0_E0F0_0112_2334);
    registers.r28 = base.wrapping_add(0xD0E0_F001_1223_3445);
    registers.r29 = base.wrapping_add(0xE0F0_0112_2334_4556);
    registers.r30 = base.wrapping_add(0xF001_1223_3445_5667);
    registers.r31 = base.wrapping_add(0x0112_2334_4556_6778);
    registers.rflags = 0x2 | 0x8D5;
    vcpu.set_regs(&registers).unwrap();
    registers
}

#[test]
fn jit_register_multiply_matches_direct_for_stack_implicit_immediate_and_apx_forms() {
    const CF_OF: u64 = (1 << 0) | (1 << 11);

    for (name, instruction, apx, suppress_flags) in [
        ("IMUL SP,BP", &[0x66, 0x0F, 0xAF, 0xE5][..], false, false),
        ("IMUL EBP,ESP", &[0x0F, 0xAF, 0xEC][..], false, false),
        ("IMUL RSP,RBP", &[0x48, 0x0F, 0xAF, 0xE5][..], false, false),
        (
            "IMUL RSP,RBP,-7",
            &[0x48, 0x6B, 0xE5, 0xF9][..],
            false,
            false,
        ),
        (
            "IMUL SP,BP,0x1234",
            &[0x66, 0x69, 0xE5, 0x34, 0x12][..],
            false,
            false,
        ),
        ("IMUL SPL", &[0x40, 0xF6, 0xEC][..], false, false),
        ("IMUL BP", &[0x66, 0xF7, 0xED][..], false, false),
        ("IMUL ESP", &[0xF7, 0xEC][..], false, false),
        ("IMUL RBP", &[0x48, 0xF7, 0xED][..], false, false),
        ("MUL SPL", &[0x40, 0xF6, 0xE4][..], false, false),
        ("MUL BP", &[0x66, 0xF7, 0xE5][..], false, false),
        ("MUL ESP", &[0xF7, 0xE4][..], false, false),
        ("MUL RBP", &[0x48, 0xF7, 0xE5][..], false, false),
        ("REX2 MUL R16", &[0xD5, 0x18, 0xF7, 0xE0][..], true, false),
        ("REX2 MUL R16B", &[0xD5, 0x10, 0xF6, 0xE0][..], true, false),
        (
            "APX MUL R16",
            &[0x62, 0xFC, 0xFC, 0x08, 0xF7, 0xE0][..],
            true,
            false,
        ),
        (
            "APX MUL RBP",
            &[0x62, 0xF4, 0xFC, 0x08, 0xF7, 0xE5][..],
            true,
            false,
        ),
        (
            "APX NF MUL RBP",
            &[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xE5][..],
            true,
            true,
        ),
        (
            "REX2 IMUL R16,R17",
            &[0xD5, 0xD8, 0xAF, 0xC1][..],
            true,
            false,
        ),
        (
            "APX NDD IMUL R16,R18,R17",
            &[0x62, 0xEC, 0xFC, 0x10, 0xAF, 0xD1][..],
            true,
            false,
        ),
        (
            "APX NF IMUL RSP,RBP",
            &[0x62, 0xF4, 0xFC, 0x0C, 0xAF, 0xE5][..],
            true,
            true,
        ),
        (
            "APX NF IMUL RSP,RBP,0x12345678",
            &[0x62, 0xF4, 0xFC, 0x0C, 0x69, 0xE5, 0x78, 0x56, 0x34, 0x12][..],
            true,
            true,
        ),
    ] {
        for small in [false, true] {
            let mut code = instruction.to_vec();
            code.push(0xF4);

            let mut direct = make_vcpu_code(&code);
            direct.set_apx_enabled(apx);
            let initial = seed(&mut direct, small);
            assert!(
                direct
                    .step()
                    .unwrap_or_else(|error| panic!("{name}: direct: {error}"))
                    .is_none(),
                "{name}: direct instruction must fall through"
            );
            let expected = direct.get_regs().unwrap();

            let mut jit = make_vcpu_code(&code);
            jit.set_apx_enabled(apx);
            seed(&mut jit, small);
            jit.set_jit_call(false);
            jit.set_jit_mem(false);
            assert!(
                jit.jit_try_block()
                    .unwrap_or_else(|error| panic!("{name}: JIT: {error}")),
                "{name}: register multiply must enter the native tier:\n{}",
                jit.jit_dump_region(LOAD_ADDR)
            );
            let actual = jit.get_regs().unwrap();

            assert_eq!(
                gprs(&actual),
                gprs(&expected),
                "{name}, small={small}: GPRs"
            );
            if suppress_flags {
                assert_eq!(expected.rflags, initial.rflags, "{name}: direct NF RFLAGS");
                assert_eq!(actual.rflags, initial.rflags, "{name}: JIT NF RFLAGS");
            } else {
                assert_eq!(
                    actual.rflags & CF_OF,
                    expected.rflags & CF_OF,
                    "{name}, small={small}: defined RFLAGS"
                );
            }
            assert_eq!(actual.rip, expected.rip, "{name}: RIP");
            assert_eq!(actual.xmm, expected.xmm, "{name}: XMM state");
            assert_eq!(actual.mm, expected.mm, "{name}: MMX state");
        }
    }
}
