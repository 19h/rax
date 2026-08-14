//! End-to-end native-JIT coverage for same-width word `MOVSX`/`MOVZX`.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Extension {
    Sign,
    Zero,
}

impl Extension {
    const ALL: [Self; 2] = [Self::Sign, Self::Zero];

    fn opcode(self) -> u8 {
        match self {
            Self::Sign => 0xBF,
            Self::Zero => 0xB7,
        }
    }
}

#[derive(Clone, Debug)]
struct Case {
    extension: Extension,
    bytes: Vec<u8>,
    dst: u8,
    src: u8,
    apx: bool,
}

fn legacy_case(extension: Extension, dst: u8, src: u8, force_rex: bool) -> Case {
    assert!(dst < 16 && src < 16);
    let rex = 0x40 | ((dst >> 3) << 2) | (src >> 3);
    let mut bytes = vec![0x66];
    if force_rex || rex != 0x40 {
        bytes.push(rex);
    }
    bytes.extend([
        0x0F,
        extension.opcode(),
        0xC0 | ((dst & 7) << 3) | (src & 7),
    ]);
    Case {
        extension,
        bytes,
        dst,
        src,
        apx: false,
    }
}

fn rex2_case(extension: Extension, dst: u8, src: u8, ignored_x: u8) -> Case {
    assert!(dst < 32 && src < 32);
    assert_eq!(ignored_x & !0x22, 0);
    let payload = 0x80
        | ((dst & 0x10) << 2)
        | ((dst & 0x08) >> 1)
        | (src & 0x10)
        | ((src & 0x08) >> 3)
        | ignored_x;
    Case {
        extension,
        bytes: vec![
            0x66,
            0xD5,
            payload,
            extension.opcode(),
            0xC0 | ((dst & 7) << 3) | (src & 7),
        ],
        dst,
        src,
        apx: true,
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::with_capacity(32);
    for extension in Extension::ALL {
        for (dst, src, force_rex) in [
            (4, 5, false),
            (5, 4, true),
            (4, 4, true),
            (5, 5, false),
            (5, 12, false),
            (13, 4, false),
            (4, 12, true),
            (13, 5, true),
        ] {
            cases.push(legacy_case(extension, dst, src, force_rex));
        }
        for (ordinal, (dst, src)) in [
            (16, 17),
            (17, 16),
            (31, 31),
            (4, 31),
            (31, 5),
            (5, 16),
            (16, 4),
            (4, 4),
        ]
        .into_iter()
        .enumerate()
        {
            cases.push(rex2_case(
                extension,
                dst,
                src,
                [0x00, 0x02, 0x20, 0x22][ordinal & 3],
            ));
        }
    }
    assert_eq!(cases.len(), 32);
    cases
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn set_gpr(regs: &mut Registers, index: usize, value: u64) {
    match index {
        0 => regs.rax = value,
        1 => regs.rcx = value,
        2 => regs.rdx = value,
        3 => regs.rbx = value,
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        6 => regs.rsi = value,
        7 => regs.rdi = value,
        8 => regs.r8 = value,
        9 => regs.r9 = value,
        10 => regs.r10 = value,
        11 => regs.r11 = value,
        12 => regs.r12 = value,
        13 => regs.r13 = value,
        14 => regs.r14 = value,
        15 => regs.r15 = value,
        16 => regs.r16 = value,
        17 => regs.r17 = value,
        18 => regs.r18 = value,
        19 => regs.r19 = value,
        20 => regs.r20 = value,
        21 => regs.r21 = value,
        22 => regs.r22 = value,
        23 => regs.r23 = value,
        24 => regs.r24 = value,
        25 => regs.r25 = value,
        26 => regs.r26 = value,
        27 => regs.r27 = value,
        28 => regs.r28 = value,
        29 => regs.r29 = value,
        30 => regs.r30 = value,
        31 => regs.r31 = value,
        _ => unreachable!(),
    }
}

fn setup(vcpu: &mut X86_64Vcpu, apx: bool, ordinal: usize) -> Registers {
    let mut regs = vcpu.get_regs().unwrap();
    for index in 0..32 {
        let value = 0x89AB_CDEF_0123_4567u64.rotate_left((index * 11) as u32)
            ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081);
        set_gpr(&mut regs, index, value);
    }
    regs.rsp = 0x11_8001 | ((ordinal as u64) & 0x7F);
    regs.rflags = 0x2 | 0x8D5;
    regs.xmm = std::array::from_fn(|index| {
        [
            0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7) as u32),
            0xA55A_3CC3_F00F_6996u64.rotate_right((index * 9) as u32),
        ]
    });
    regs.ymm_high = std::array::from_fn(|index| {
        [
            0x0123_4567_89AB_CDEFu64.rotate_left((index * 5) as u32),
            0xFEDC_BA98_7654_3210u64.rotate_right((index * 3) as u32),
        ]
    });
    regs.zmm_high = std::array::from_fn(|index| {
        std::array::from_fn(|word| {
            0xC33C_5AA5_F00F_6996u64.rotate_left(((index * 13 + word * 17) & 63) as u32)
        })
    });
    regs.zmm_ext = std::array::from_fn(|index| {
        std::array::from_fn(|word| {
            0x5AA5_C33C_6996_F00Fu64.rotate_right(((index * 19 + word * 23) & 63) as u32)
        })
    });
    regs.k = std::array::from_fn(|index| 0x0102_0408_1020_4081u64.rotate_left(index as u32));
    regs.mm = std::array::from_fn(|index| 0xA5A5_5A5A_6996_9669u64.rotate_left(index as u32));
    vcpu.set_regs(&regs).unwrap();
    vcpu.set_apx_enabled(apx);
    regs
}

fn assert_register_state(actual: &Registers, expected: &Registers, label: &str) {
    assert_eq!(gprs(actual), gprs(expected), "{label}: GPR file");
    assert_eq!(actual.rflags, expected.rflags, "{label}: RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{label}: RIP");
    assert_eq!(actual.xmm, expected.xmm, "{label}: XMM state");
    assert_eq!(
        actual.ymm_high, expected.ymm_high,
        "{label}: YMM-high state"
    );
    assert_eq!(
        actual.zmm_high, expected.zmm_high,
        "{label}: ZMM-high state"
    );
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "{label}: ZMM16-31 state");
    assert_eq!(actual.k, expected.k, "{label}: opmask state");
    assert_eq!(actual.mm, expected.mm, "{label}: MMX state");
}

#[test]
fn jit_same_width_movx_state_categories_match_direct_and_manual_semantics() {
    for (ordinal, case) in cases().into_iter().enumerate() {
        let mut code = case.bytes.clone();
        code.push(0xF4);
        let label = format!(
            "{:?} dst={} src={} {:02X?}",
            case.extension, case.dst, case.src, case.bytes
        );

        let mut direct = make_vcpu_code(&code);
        let initial = setup(&mut direct, case.apx, ordinal);
        assert!(direct.step().unwrap().is_none(), "{label}: direct step");
        let expected = direct.get_regs().unwrap();
        let initial_gprs = gprs(&initial);
        let expected_value = (initial_gprs[usize::from(case.dst)] & !0xFFFF)
            | (initial_gprs[usize::from(case.src)] & 0xFFFF);
        assert_eq!(
            gprs(&expected)[usize::from(case.dst)],
            expected_value,
            "{label}: manual partial-register result"
        );
        assert_eq!(expected.rflags, initial.rflags, "{label}: direct flags");

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, case.apx, ordinal);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{label}: {error:?}")),
            "{label}: native admission\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_register_state(&actual, &expected, &label);
    }
}

#[test]
fn jit_same_width_rex2_movx_guard_is_dynamic_precise_and_noncommitting() {
    let case = rex2_case(Extension::Sign, 16, 5, 0x22);
    let mut code = case.bytes.clone();
    code.push(0xF4);
    let mut vcpu = make_vcpu_code(&code);
    let initial = setup(&mut vcpu, true, 0);
    vcpu.set_jit_call(false);
    vcpu.set_jit_mem(false);

    assert!(vcpu.jit_try_block().expect("enabled REX2 MOVSX JIT"));
    let enabled = vcpu.get_regs().unwrap();
    assert_eq!(
        enabled.r16,
        (initial.r16 & !0xFFFF) | (initial.rbp & 0xFFFF)
    );

    let mut reset = initial.clone();
    reset.rip = LOAD_ADDR;
    vcpu.set_regs(&reset).unwrap();
    vcpu.set_apx_enabled(false);
    assert!(vcpu.jit_try_block().expect("cached disabled-APX guard"));
    let guarded = vcpu.get_regs().unwrap();
    assert_register_state(&guarded, &reset, "disabled APX guard");
    assert_eq!(guarded.rip, LOAD_ADDR, "guard fault PC");

    let error = format!("{:#}", vcpu.step().expect_err("disabled APX MOVSX"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    let after_fault = vcpu.get_regs().unwrap();
    assert_register_state(&after_fault, &reset, "disabled APX direct fault");
}
