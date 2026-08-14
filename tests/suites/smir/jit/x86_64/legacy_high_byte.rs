//! End-to-end native replay coverage for legacy AH/CH/DH/BH operations.

use super::*;

fn legacy_gprs(registers: &Registers) -> [u64; 32] {
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

const DEFAULT_RCX: u64 = 0x1357_9BDF_2468_0305;
const DEFAULT_RFLAGS: u64 = 0x2 | 0x8D5;

fn seed(vcpu: &mut X86_64Vcpu, rax: u64, rcx: u64, rflags: u64) -> Registers {
    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = rax;
    registers.rbx = 0xFEDC_BA98_7654_3210;
    registers.rcx = rcx;
    registers.rdx = 0x0F1E_2D3C_4B5A_6978;
    registers.rsi = 0x1111_2222_3333_4444;
    registers.rdi = 0x5555_6666_7777_8888;
    registers.rbp = 0x9999_AAAA_BBBB_CCCC;
    registers.r8 = 0x0101_0202_0303_0404;
    registers.r9 = 0x0505_0606_0707_0808;
    registers.r10 = 0x0909_0A0A_0B0B_0C0C;
    registers.r11 = 0x0D0D_0E0E_0F0F_1010;
    registers.r12 = 0x1111_1212_1313_1414;
    registers.r13 = 0x1515_1616_1717_1818;
    registers.r14 = 0x1919_1A1A_1B1B_1C1C;
    registers.r15 = 0x1D1D_1E1E_1F1F_2020;
    registers.r16 = 0x2121_2222_2323_2424;
    registers.r17 = 0x2525_2626_2727_2828;
    registers.r18 = 0x2929_2A2A_2B2B_2C2C;
    registers.r19 = 0x2D2D_2E2E_2F2F_3030;
    registers.r20 = 0x3131_3232_3333_3434;
    registers.r21 = 0x3535_3636_3737_3838;
    registers.r22 = 0x3939_3A3A_3B3B_3C3C;
    registers.r23 = 0x3D3D_3E3E_3F3F_4040;
    registers.r24 = 0x4141_4242_4343_4444;
    registers.r25 = 0x4545_4646_4747_4848;
    registers.r26 = 0x4949_4A4A_4B4B_4C4C;
    registers.r27 = 0x4D4D_4E4E_4F4F_5050;
    registers.r28 = 0x5151_5252_5353_5454;
    registers.r29 = 0x5555_5656_5757_5858;
    registers.r30 = 0x5959_5A5A_5B5B_5C5C;
    registers.r31 = 0x5D5D_5E5E_5F5F_6060;
    registers.rflags = rflags;
    vcpu.set_regs(&registers).unwrap();
    registers
}

fn compare_direct_and_jit(name: &str, instruction: &[u8], rax: u64) {
    compare_direct_and_jit_state(name, instruction, rax, DEFAULT_RCX, DEFAULT_RFLAGS);
}

fn compare_direct_and_jit_state(name: &str, instruction: &[u8], rax: u64, rcx: u64, rflags: u64) {
    compare_direct_and_jit_state_flags(name, instruction, rax, rcx, rflags, None);
}

fn compare_direct_and_jit_defined_flags(name: &str, instruction: &[u8], rax: u64, mask: u64) {
    compare_direct_and_jit_state_flags(
        name,
        instruction,
        rax,
        DEFAULT_RCX,
        DEFAULT_RFLAGS,
        Some(mask),
    );
}

fn compare_direct_and_jit_state_flags(
    name: &str,
    instruction: &[u8],
    rax: u64,
    rcx: u64,
    rflags: u64,
    flag_mask: Option<u64>,
) {
    let mut code = instruction.to_vec();
    code.push(0xF4);

    let (mut direct, _) = make_vcpu_mem(&code);
    seed(&mut direct, rax, rcx, rflags);
    assert!(
        direct
            .step()
            .unwrap_or_else(|error| panic!("{name}: {error:?}"))
            .is_none(),
        "{name}: direct instruction must fall through"
    );
    let expected = direct.get_regs().unwrap();

    let (mut jit, _) = make_vcpu_mem(&code);
    seed(&mut jit, rax, rcx, rflags);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{name}: {error:?}")),
        "{name}: high-byte register instruction must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    let actual = jit.get_regs().unwrap();

    assert_eq!(legacy_gprs(&actual), legacy_gprs(&expected), "{name}: GPRs");
    if let Some(mask) = flag_mask {
        assert_eq!(
            actual.rflags & mask,
            expected.rflags & mask,
            "{name}: defined RFLAGS"
        );
    } else {
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
    }
    assert_eq!(actual.rip, expected.rip, "{name}: RIP");
    assert_eq!(actual.xmm, expected.xmm, "{name}: XMM state");
    assert_eq!(actual.mm, expected.mm, "{name}: MMX state");
}

fn crc32c_byte(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte);
    for _ in 0..8 {
        crc = (crc >> 1) ^ (0x82F6_3B78 & 0u32.wrapping_sub(crc & 1));
    }
    crc
}

#[test]
fn jit_high_byte_multiply_matches_direct_for_all_56_scanner_cells() {
    const PREFIXES: &[&[u8]] = &[&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];
    const DEFINED_FLAGS: u64 = (1 << 0) | (1 << 11); // CF | OF

    let mut cases = 0usize;
    for prefix in PREFIXES {
        for extension in [4u8, 5] {
            for rm in 4u8..8 {
                let mut instruction = prefix.to_vec();
                instruction.extend([0xF6, 0xC0 | (extension << 3) | rm]);
                compare_direct_and_jit_defined_flags(
                    &format!("{instruction:02X?}"),
                    &instruction,
                    0x8123_4567_89AB_CDEF,
                    DEFINED_FLAGS,
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 56);
}

#[test]
fn jit_high_byte_crc32c_matches_castagnoli_oracle_for_all_32_scanner_cells() {
    if !std::is_x86_feature_detected!("sse4.2") {
        return;
    }

    const RSP_SEED: u64 = 0x7777_8888_9999_AAAA;
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut registers = seed(vcpu, 0x0123_4567_89AB_CDEF, DEFAULT_RCX, DEFAULT_RFLAGS);
        registers.rsp = RSP_SEED;
        vcpu.set_regs(&registers).unwrap();
        registers
    };

    let mut cases = 0usize;
    for destination in 0usize..8 {
        for rm in 4u8..8 {
            let instruction = [
                0xF2,
                0x0F,
                0x38,
                0xF0,
                0xC0 | ((destination as u8) << 3) | rm,
            ];
            let mut code = instruction.to_vec();
            code.push(0xF4);

            let mut direct = make_vcpu_code(&code);
            let initial = setup(&mut direct);
            let mut oracle_gprs = legacy_gprs(&initial);
            let source = (oracle_gprs[usize::from(rm - 4)] >> 8) as u8;
            oracle_gprs[destination] =
                u64::from(crc32c_byte(oracle_gprs[destination] as u32, source));
            assert!(
                direct.step().unwrap().is_none(),
                "direct {instruction:02X?}"
            );
            let direct_regs = direct.get_regs().unwrap();
            assert_eq!(
                legacy_gprs(&direct_regs),
                oracle_gprs,
                "direct Castagnoli result {instruction:02X?}"
            );
            assert_eq!(
                direct_regs.rflags, initial.rflags,
                "direct RFLAGS {instruction:02X?}"
            );

            let mut jit = make_vcpu_code(&code);
            setup(&mut jit);
            jit.set_jit_call(false);
            jit.set_jit_mem(false);
            assert!(
                jit.jit_try_block().unwrap(),
                "native admission {instruction:02X?}:\n{}",
                jit.jit_dump_region(LOAD_ADDR)
            );
            let actual = jit.get_regs().unwrap();
            assert_eq!(
                legacy_gprs(&actual),
                oracle_gprs,
                "JIT Castagnoli result {instruction:02X?}"
            );
            assert_eq!(
                actual.rflags, initial.rflags,
                "JIT RFLAGS {instruction:02X?}"
            );
            assert_eq!(actual.rip, direct_regs.rip, "JIT RIP {instruction:02X?}");
            assert_eq!(actual.xmm, direct_regs.xmm, "JIT XMM {instruction:02X?}");
            assert_eq!(actual.mm, direct_regs.mm, "JIT MMX {instruction:02X?}");
            cases += 1;
        }
    }
    assert_eq!(cases, 32);
}

#[test]
fn jit_legacy_high_byte_mov_immediate_exhausts_all_7168_scanner_images() {
    const PREFIXES: &[&[u8]] = &[&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];
    const PRESERVED_FLAGS: u64 =
        DEFAULT_RFLAGS | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 18) | (1 << 21);

    let mut cases = 0usize;
    for prefix in PREFIXES {
        for opcode in 0xB4u8..=0xB7 {
            for immediate in u8::MIN..=u8::MAX {
                let mut instruction = prefix.to_vec();
                instruction.extend([opcode, immediate]);
                let mut code = instruction.clone();
                code.push(0xF4);

                let mut direct = make_vcpu_code(&code);
                let initial = seed(
                    &mut direct,
                    0x0123_4567_89AB_CDEF,
                    DEFAULT_RCX,
                    PRESERVED_FLAGS,
                );
                assert!(
                    direct.step().unwrap().is_none(),
                    "direct {instruction:02X?}"
                );
                let direct_regs = direct.get_regs().unwrap();

                let mut manual = initial.clone();
                let parent = match opcode {
                    0xB4 => &mut manual.rax,
                    0xB5 => &mut manual.rcx,
                    0xB6 => &mut manual.rdx,
                    0xB7 => &mut manual.rbx,
                    _ => unreachable!(),
                };
                *parent = (*parent & !0xFF00) | (u64::from(immediate) << 8);
                manual.rip = LOAD_ADDR + instruction.len() as u64;
                assert_eq!(
                    legacy_gprs(&direct_regs),
                    legacy_gprs(&manual),
                    "manual GPRs {instruction:02X?}"
                );
                assert_eq!(
                    direct_regs.rflags, manual.rflags,
                    "manual RFLAGS {instruction:02X?}"
                );
                assert_eq!(direct_regs.rip, manual.rip, "manual RIP {instruction:02X?}");

                let mut jit = make_vcpu_code(&code);
                seed(
                    &mut jit,
                    0x0123_4567_89AB_CDEF,
                    DEFAULT_RCX,
                    PRESERVED_FLAGS,
                );
                jit.set_jit_call(false);
                jit.set_jit_mem(false);
                assert!(
                    jit.jit_try_block().unwrap(),
                    "native admission {instruction:02X?}:\n{}",
                    jit.jit_dump_region(LOAD_ADDR)
                );
                let actual = jit.get_regs().unwrap();
                assert_eq!(
                    legacy_gprs(&actual),
                    legacy_gprs(&manual),
                    "JIT GPRs {instruction:02X?}"
                );
                assert_eq!(
                    actual.rflags, manual.rflags,
                    "JIT RFLAGS {instruction:02X?}"
                );
                assert_eq!(actual.rip, manual.rip, "JIT RIP {instruction:02X?}");
                assert_eq!(actual.xmm, direct_regs.xmm, "JIT XMM {instruction:02X?}");
                assert_eq!(actual.mm, direct_regs.mm, "JIT MMX {instruction:02X?}");
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 7_168);
}

#[test]
fn jit_high_byte_group3_slash1_test_exhausts_all_7168_scanner_images() {
    const PREFIXES: &[&[u8]] = &[&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];
    const CF: u64 = 1 << 0;
    const PF: u64 = 1 << 2;
    const AF: u64 = 1 << 4;
    const ZF: u64 = 1 << 6;
    const SF: u64 = 1 << 7;
    const OF: u64 = 1 << 11;
    const DEFINED_STATUS: u64 = CF | PF | ZF | SF | OF;
    const STATUS: u64 = DEFINED_STATUS | AF;
    const INPUT_RFLAGS: u64 =
        0x2 | STATUS | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 18) | (1 << 21);

    let mut cases = 0usize;
    for prefix in PREFIXES {
        for rm in 4u8..8 {
            for immediate in u8::MIN..=u8::MAX {
                let mut instruction = prefix.to_vec();
                instruction.extend([0xF6, 0xC8 | rm, immediate]);
                let mut alias_code = instruction.clone();
                alias_code.push(0xF4);

                let mut direct_alias = make_vcpu_code(&alias_code);
                let initial = seed(
                    &mut direct_alias,
                    0x0123_4567_89AB_CDEF,
                    DEFAULT_RCX,
                    INPUT_RFLAGS,
                );
                let source = ((legacy_gprs(&initial)[usize::from(rm - 4)] >> 8) & 0xFF) as u8;
                let result = source & immediate;
                let expected_defined = ((result.count_ones() & 1 == 0) as u64) << 2
                    | ((result == 0) as u64) << 6
                    | ((result & 0x80 != 0) as u64) << 7;

                assert!(
                    direct_alias.step().unwrap().is_none(),
                    "direct /1 {instruction:02X?}"
                );
                let alias_regs = direct_alias.get_regs().unwrap();
                assert_eq!(
                    legacy_gprs(&alias_regs),
                    legacy_gprs(&initial),
                    "direct /1 GPRs {instruction:02X?}"
                );
                assert_eq!(
                    alias_regs.rflags & DEFINED_STATUS,
                    expected_defined,
                    "direct /1 defined flags {instruction:02X?} source={source:02X}"
                );
                assert_eq!(
                    alias_regs.rflags & !STATUS,
                    initial.rflags & !STATUS,
                    "direct /1 preserved flags {instruction:02X?}"
                );
                assert_eq!(
                    alias_regs.rip,
                    LOAD_ADDR + instruction.len() as u64,
                    "direct /1 RIP {instruction:02X?}"
                );

                let canonical = [0xF6, 0xC0 | rm, immediate, 0xF4];
                let mut direct_canonical = make_vcpu_code(&canonical);
                seed(
                    &mut direct_canonical,
                    0x0123_4567_89AB_CDEF,
                    DEFAULT_RCX,
                    INPUT_RFLAGS,
                );
                assert!(
                    direct_canonical.step().unwrap().is_none(),
                    "direct /0 {canonical:02X?}"
                );
                let canonical_regs = direct_canonical.get_regs().unwrap();
                assert_eq!(
                    legacy_gprs(&canonical_regs),
                    legacy_gprs(&initial),
                    "direct /0 GPRs {canonical:02X?}"
                );
                assert_eq!(
                    canonical_regs.rflags & DEFINED_STATUS,
                    expected_defined,
                    "direct /0 defined flags {canonical:02X?}"
                );
                assert_eq!(
                    canonical_regs.rflags & !STATUS,
                    initial.rflags & !STATUS,
                    "direct /0 preserved flags {canonical:02X?}"
                );

                let mut jit = make_vcpu_code(&alias_code);
                seed(
                    &mut jit,
                    0x0123_4567_89AB_CDEF,
                    DEFAULT_RCX,
                    INPUT_RFLAGS,
                );
                jit.set_jit_call(false);
                jit.set_jit_mem(false);
                assert!(
                    jit.jit_try_block().unwrap(),
                    "native /1 admission {instruction:02X?}:\n{}",
                    jit.jit_dump_region(LOAD_ADDR)
                );
                let actual = jit.get_regs().unwrap();
                assert_eq!(
                    legacy_gprs(&actual),
                    legacy_gprs(&initial),
                    "JIT /1 GPRs {instruction:02X?}"
                );
                assert_eq!(
                    actual.rflags & DEFINED_STATUS,
                    expected_defined,
                    "JIT /1 defined flags {instruction:02X?} source={source:02X}"
                );
                assert_eq!(
                    actual.rflags & !STATUS,
                    initial.rflags & !STATUS,
                    "JIT /1 preserved flags {instruction:02X?}"
                );
                assert_eq!(actual.rip, alias_regs.rip, "JIT /1 RIP {instruction:02X?}");
                assert_eq!(actual.xmm, alias_regs.xmm, "JIT /1 XMM {instruction:02X?}");
                assert_eq!(actual.mm, alias_regs.mm, "JIT /1 MMX {instruction:02X?}");
                assert_eq!(
                    actual.rflags & (DEFINED_STATUS | !STATUS),
                    canonical_regs.rflags & (DEFINED_STATUS | !STATUS),
                    "JIT /1 versus canonical /0 flags {instruction:02X?}"
                );
                cases += 1;
            }
        }
    }

    assert_eq!(cases, 7_168);
}

#[test]
fn jit_legacy_high_byte_replay_matches_direct_for_aliases_flags_and_prefixes() {
    let common_rax = 0x0123_4567_89AB_CDEF;
    let cases: &[(&str, &[u8], u64)] = &[
        ("add ah,al", &[0x00, 0xC4], common_rax),
        ("or ah,al", &[0x0A, 0xE0], common_rax),
        ("adc ch,bh", &[0x10, 0xFD], common_rax),
        ("sbb bh,dh", &[0x1A, 0xFE], common_rax),
        ("and ah,bl", &[0x20, 0xDC], common_rax),
        ("sub ah,al", &[0x2A, 0xE0], common_rax),
        ("xor bh,dh", &[0x30, 0xF7], common_rax),
        ("cmp ah,al", &[0x3A, 0xE0], common_rax),
        ("test al,ah", &[0x84, 0xE0], common_rax),
        ("xchg al,ah", &[0x86, 0xE0], common_rax),
        ("mov ah,bl", &[0x88, 0xDC], common_rax),
        ("mov al,ah", &[0x8A, 0xC4], common_rax),
        ("sub ah,0x81", &[0x80, 0xEC, 0x81], common_rax),
        ("mov bh,0x5a", &[0xC6, 0xC7, 0x5A], common_rax),
        ("test ch,0xa5", &[0xF6, 0xC5, 0xA5], common_rax),
        ("not dh", &[0xF6, 0xD6], common_rax),
        ("neg bh", &[0xF6, 0xDF], common_rax),
        ("inc dh", &[0xFE, 0xC6], common_rax),
        ("dec bh", &[0xFE, 0xCF], common_rax),
        ("setbe ah", &[0x0F, 0x96, 0xC4], common_rax),
        ("cmpxchg al,ah failure", &[0x0F, 0xB0, 0xE0], common_rax),
        ("cmpxchg ch,dh failure", &[0x0F, 0xB0, 0xF5], common_rax),
        (
            "cmpxchg ch,dh success",
            &[0x0F, 0xB0, 0xF5],
            (common_rax & !0xFF) | 0x03,
        ),
        ("xadd ah,bh", &[0x0F, 0xC0, 0xFC], common_rax),
        ("rol ah,0", &[0xC0, 0xC4, 0x00], common_rax),
        ("ror ch,1", &[0xD0, 0xCD], common_rax),
        ("rcl dh,2", &[0xC0, 0xD6, 0x02], common_rax),
        ("rcr bh,cl", &[0xD2, 0xDF], common_rax),
        ("shl ah,8", &[0xC0, 0xE4, 0x08], common_rax),
        ("sal ah,8", &[0xC0, 0xF4, 0x08], common_rax),
        ("sal bh,cl", &[0xD2, 0xF7], common_rax),
        ("shr ch,9", &[0xC0, 0xED, 0x09], common_rax),
        ("sar dh,31", &[0xC0, 0xFE, 0x1F], common_rax),
        (
            "prefixed shl ah,8",
            &[0x65, 0x66, 0x67, 0xF3, 0xC0, 0xE4, 0x08],
            common_rax,
        ),
        (
            "prefixed add ah,ch",
            &[0x65, 0x66, 0x67, 0xF3, 0x00, 0xEC],
            common_rax,
        ),
    ];

    for (name, instruction, rax) in cases {
        compare_direct_and_jit(name, instruction, *rax);
    }

    compare_direct_and_jit_defined_flags(
        "combined-prefix group3 /1 test ch,0xa5",
        &[0x65, 0x66, 0x67, 0xF3, 0xF6, 0xCD, 0xA5],
        common_rax,
        (1 << 0) | (1 << 2) | (1 << 6) | (1 << 7) | (1 << 11),
    );

    for (name, instruction) in [
        ("rcl ah,1 with CF clear", &[0xD0, 0xD4][..]),
        ("rcr ch,2 with CF clear", &[0xC0, 0xDD, 0x02][..]),
        ("rcl dh,cl with CF clear", &[0xD2, 0xD6][..]),
    ] {
        compare_direct_and_jit_state(name, instruction, common_rax, DEFAULT_RCX, 0x2);
    }

    for (name, instruction, rcx) in [
        (
            "rol ah,2 with status clear",
            &[0xC0, 0xC4, 0x02][..],
            DEFAULT_RCX,
        ),
        (
            "ror ch,0x20 masked zero",
            &[0xC0, 0xCD, 0x20][..],
            DEFAULT_RCX,
        ),
        (
            "rcl dh,0xa1 masked one",
            &[0xC0, 0xD6, 0xA1][..],
            DEFAULT_RCX,
        ),
        (
            "shl ah,0x28 masked boundary",
            &[0xC0, 0xE4, 0x28][..],
            DEFAULT_RCX,
        ),
        ("sar bh,cl oversized", &[0xD2, 0xFF][..], 0xFF),
    ] {
        compare_direct_and_jit_state(name, instruction, common_rax, rcx, 0x2);
    }
}

fn setcc_condition_holds(opcode: u8, rflags: u64) -> bool {
    let cf = rflags & (1 << 0) != 0;
    let pf = rflags & (1 << 2) != 0;
    let zf = rflags & (1 << 6) != 0;
    let sf = rflags & (1 << 7) != 0;
    let of = rflags & (1 << 11) != 0;
    match opcode {
        0x90 => of,
        0x91 => !of,
        0x92 => cf,
        0x93 => !cf,
        0x94 => zf,
        0x95 => !zf,
        0x96 => cf || zf,
        0x97 => !cf && !zf,
        0x98 => sf,
        0x99 => !sf,
        0x9A => pf,
        0x9B => !pf,
        0x9C => sf != of,
        0x9D => sf == of,
        0x9E => zf || sf != of,
        0x9F => !zf && sf == of,
        _ => unreachable!("not a SETcc opcode: {opcode:02X}"),
    }
}

fn status_image(selector: usize) -> u64 {
    [0u8, 2, 4, 6, 7, 11]
        .into_iter()
        .enumerate()
        .fold(0, |flags, (index, bit)| {
            flags | (((selector >> index) & 1) as u64) << bit
        })
}

#[test]
fn jit_high_byte_setcc_exhausts_all_3584_scanner_images_and_status_truth_table() {
    const PREFIXES: &[&[u8]] = &[&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];
    let mut cases = 0usize;
    let mut newly_admitted = 0usize;

    for (prefix_index, prefix) in PREFIXES.iter().enumerate() {
        for opcode in 0x90u8..=0x9F {
            for ignored_reg in 0u8..8 {
                for rm in 4u8..8 {
                    let mut instruction = prefix.to_vec();
                    instruction.extend([0x0F, opcode, 0xC0 | (ignored_reg << 3) | rm]);
                    let selector =
                        prefix_index * 32 + usize::from(ignored_reg) * 4 + usize::from(rm - 4);
                    let rflags =
                        0x2 | (1 << 10) | ((selector as u64 & 1) << 18) | status_image(selector);
                    let mut code = instruction.clone();
                    code.push(0xF4);

                    let (mut direct, _) = make_vcpu_mem(&code);
                    let initial = seed(
                        &mut direct,
                        0x0123_4567_89AB_CDEF,
                        0x1357_9BDF_2468_ACE0,
                        rflags,
                    );
                    assert!(
                        direct.step().unwrap().is_none(),
                        "direct {instruction:02X?} flags={rflags:04X}"
                    );
                    let direct_regs = direct.get_regs().unwrap();

                    let mut expected_gprs = legacy_gprs(&initial);
                    let parent = usize::from(rm - 4);
                    expected_gprs[parent] = (expected_gprs[parent] & !0xFF00)
                        | (u64::from(setcc_condition_holds(opcode, rflags)) << 8);
                    assert_eq!(
                        legacy_gprs(&direct_regs),
                        expected_gprs,
                        "direct/manual {instruction:02X?} flags={rflags:04X}"
                    );
                    assert_eq!(
                        direct_regs.rflags, initial.rflags,
                        "direct flags {instruction:02X?}"
                    );

                    let (mut jit, _) = make_vcpu_mem(&code);
                    seed(
                        &mut jit,
                        0x0123_4567_89AB_CDEF,
                        0x1357_9BDF_2468_ACE0,
                        rflags,
                    );
                    jit.set_jit_call(false);
                    jit.set_jit_mem(false);
                    assert!(
                        jit.jit_try_block().unwrap(),
                        "native admission {instruction:02X?}:\n{}",
                        jit.jit_dump_region(LOAD_ADDR)
                    );
                    let actual = jit.get_regs().unwrap();
                    assert_eq!(
                        legacy_gprs(&actual),
                        expected_gprs,
                        "JIT/manual {instruction:02X?} flags={rflags:04X}"
                    );
                    assert_eq!(
                        actual.rflags, initial.rflags,
                        "JIT flags {instruction:02X?}"
                    );
                    assert_eq!(actual.rip, direct_regs.rip, "JIT RIP {instruction:02X?}");
                    assert_eq!(actual.xmm, direct_regs.xmm, "JIT XMM {instruction:02X?}");
                    assert_eq!(actual.mm, direct_regs.mm, "JIT MMX {instruction:02X?}");
                    cases += 1;
                    newly_admitted += usize::from(ignored_reg != 0);
                }
            }
        }
    }

    assert_eq!(cases, 7 * 16 * 8 * 4);
    assert_eq!(newly_admitted, 7 * 16 * 7 * 4);
}

#[test]
fn jit_legacy_high_byte_replay_matches_direct_for_every_admitted_register_cell() {
    let rax = 0x0123_4567_89AB_CDEF;
    let mut cases = 0usize;

    for opcode in [
        0x00, 0x02, 0x08, 0x0A, 0x10, 0x12, 0x18, 0x1A, 0x20, 0x22, 0x28, 0x2A, 0x30, 0x32, 0x38,
        0x3A, 0x84, 0x86, 0x88, 0x8A,
    ] {
        for fields in 0u8..=0x3F {
            if fields & 7 >= 4 || (fields >> 3) & 7 >= 4 {
                let bytes = [opcode, 0xC0 | fields];
                compare_direct_and_jit(&format!("{bytes:02X?}"), &bytes, rax);
                cases += 1;
            }
        }
    }

    for (opcode, extensions, immediate) in [
        (0xFE, 0b0000_0011u8, None),
        (0x80, 0b1111_1111, Some(0xA5)),
        (0xC6, 0b0000_0001, Some(0x5A)),
        (0xF6, 0b0000_0001, Some(0xA5)),
        (0xF6, 0b0011_1100, None),
    ] {
        for extension in 0u8..8 {
            if extensions & (1 << extension) == 0 {
                continue;
            }
            for rm in 4u8..8 {
                let mut bytes = vec![opcode, 0xC0 | (extension << 3) | rm];
                if let Some(immediate) = immediate {
                    bytes.push(immediate);
                }
                if opcode == 0xF6 && matches!(extension, 4 | 5) {
                    compare_direct_and_jit_defined_flags(
                        &format!("{bytes:02X?}"),
                        &bytes,
                        rax,
                        (1 << 0) | (1 << 11),
                    );
                } else {
                    compare_direct_and_jit(&format!("{bytes:02X?}"), &bytes, rax);
                }
                cases += 1;
            }
        }
    }

    for opcode in 0x90u8..=0x9F {
        for ignored_reg in 0u8..8 {
            for rm in 4u8..8 {
                let bytes = [0x0F, opcode, 0xC0 | (ignored_reg << 3) | rm];
                compare_direct_and_jit(&format!("{bytes:02X?}"), &bytes, rax);
                cases += 1;
            }
        }
    }

    for opcode in [0xB0, 0xC0] {
        for fields in 0u8..=0x3F {
            if fields & 7 >= 4 || (fields >> 3) & 7 >= 4 {
                let bytes = [0x0F, opcode, 0xC0 | fields];
                compare_direct_and_jit(&format!("{bytes:02X?}"), &bytes, rax);
                cases += 1;
            }
        }
    }

    for extension in 0u8..8 {
        for rm in 4u8..8 {
            let bytes = [0xD0, 0xC0 | (extension << 3) | rm];
            compare_direct_and_jit(&format!("{bytes:02X?}"), &bytes, rax);
            cases += 1;

            for count in 0u8..32 {
                let bytes = [0xC0, 0xC0 | (extension << 3) | rm, count];
                compare_direct_and_jit(&format!("{bytes:02X?}"), &bytes, rax);
                cases += 1;

                let bytes = [0xD2, 0xC0 | (extension << 3) | rm];
                let rcx = (DEFAULT_RCX & !0xFF) | u64::from(count);
                compare_direct_and_jit_state(
                    &format!("{bytes:02X?} CL={count}"),
                    &bytes,
                    rax,
                    rcx,
                    DEFAULT_RFLAGS,
                );
                cases += 1;
            }
        }
    }

    assert_eq!(cases, 3_712);
}

#[test]
fn jit_legacy_carry_rotate_nonunit_counts_match_direct_via_state_backed_lowering() {
    let rax = 0x8123_4567_89AB_CDEF;
    for (name, instruction, rcx, rflags) in [
        ("rcl al,0", &[0xC0, 0xD0, 0x00][..], DEFAULT_RCX, 0x8D7),
        ("rcr al,2", &[0xC0, 0xD8, 0x02][..], DEFAULT_RCX, 0x2),
        ("rcl eax,cl", &[0xD3, 0xD0][..], 5, 0x8D7),
        ("rcl ecx,cl alias", &[0xD3, 0xD1][..], 9, 0x2),
        (
            "rcr ax,17",
            &[0x66, 0xC1, 0xD8, 0x11][..],
            DEFAULT_RCX,
            0x8D7,
        ),
        ("rcl eax,32", &[0xC1, 0xD0, 0x20][..], DEFAULT_RCX, 0x2),
        (
            "rcl rax,64",
            &[0x48, 0xC1, 0xD0, 0x40][..],
            DEFAULT_RCX,
            0x8D7,
        ),
        ("rcl rsp,2", &[0x48, 0xC1, 0xD4, 0x02][..], DEFAULT_RCX, 0x2),
    ] {
        compare_direct_and_jit_state(name, instruction, rax, rcx, rflags);
    }
}
