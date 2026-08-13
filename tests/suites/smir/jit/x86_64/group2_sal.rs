//! End-to-end native differential coverage for legacy Group-2 `/6` SAL.

use super::*;

const AF: u64 = 1 << 4;

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

fn seed(vcpu: &mut X86_64Vcpu, rcx: u64, rflags: u64) {
    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = 0x8123_4567_89AB_CDA5;
    registers.rbx = 0xFEDC_BA98_7654_3210;
    registers.rcx = rcx;
    registers.rdx = 0x0F1E_2D3C_4B5A_6978;
    registers.rsi = 0x1111_2222_3333_4444;
    registers.rdi = 0x5555_6666_7777_8888;
    registers.rsp = 0x0011_0000;
    registers.rbp = 0x9999_AAAA_BBBB_CCCC;
    registers.r8 = 0x0101_0202_0303_04A5;
    registers.r9 = 0x0505_0606_0707_0808;
    registers.r10 = 0x0909_0A0A_0B0B_0C0C;
    registers.r11 = 0x0D0D_0E0E_0F0F_1010;
    registers.r12 = 0x1111_1212_1313_1414;
    registers.r13 = 0x1515_1616_1717_1818;
    registers.r14 = 0x1919_1A1A_1B1B_1C1C;
    registers.r15 = 0x9D1D_1E1E_1F1F_20A5;
    registers.rflags = rflags;
    vcpu.set_regs(&registers).unwrap();
}

fn compare(name: &str, instruction: &[u8], rcx: u64, masked_nonzero: bool, rflags: u64) {
    let mut code = instruction.to_vec();
    code.push(0xF4);

    let mut direct = make_vcpu_code(&code);
    seed(&mut direct, rcx, rflags);
    assert!(
        direct
            .step()
            .unwrap_or_else(|error| panic!("{name}: direct: {error:?}"))
            .is_none(),
        "{name}: direct instruction must fall through"
    );
    let expected = direct.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    seed(&mut jit, rcx, rflags);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block()
            .unwrap_or_else(|error| panic!("{name}: JIT: {error:?}")),
        "{name}: register SAL must enter the native tier:\n{}",
        jit.jit_dump_region(LOAD_ADDR)
    );
    let actual = jit.get_regs().unwrap();

    assert_eq!(gprs(&actual), gprs(&expected), "{name}: GPRs");
    assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{name}: RIP");
    assert_eq!(actual.xmm, expected.xmm, "{name}: XMM");
    assert_eq!(actual.ymm_high, expected.ymm_high, "{name}: YMM high");
    assert_eq!(actual.zmm_high, expected.zmm_high, "{name}: ZMM high");
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "{name}: ZMM16-31");
    assert_eq!(actual.k, expected.k, "{name}: opmask");
    assert_eq!(actual.mm, expected.mm, "{name}: MMX");
    if masked_nonzero {
        assert_eq!(actual.rflags & AF, 0, "{name}: nonzero `/6` AF");
    } else {
        assert_eq!(actual.rflags & AF, rflags & AF, "{name}: zero-count AF");
    }
}

#[test]
fn jit_group2_sal_register_forms_match_direct_full_state_and_af_policy() {
    let cases: &[(&str, &[u8], u64, bool)] = &[
        ("sal al,0", &[0xC0, 0xF0, 0x00], 0, false),
        ("sal al,1", &[0xC0, 0xF0, 0x01], 0, true),
        ("sal al,8", &[0xC0, 0xF0, 0x08], 0, true),
        ("sal al,9", &[0xC0, 0xF0, 0x09], 0, true),
        ("sal al,32", &[0xC0, 0xF0, 0x20], 0, false),
        ("sal r8b,255", &[0x41, 0xC0, 0xF0, 0xFF], 0, true),
        ("sal ax,16", &[0x66, 0xC1, 0xF0, 0x10], 0, true),
        ("sal ax,17", &[0x66, 0xC1, 0xF0, 0x11], 0, true),
        ("sal ax,32", &[0x66, 0xC1, 0xF0, 0x20], 0, false),
        ("sal eax,31", &[0xC1, 0xF0, 0x1F], 0, true),
        ("sal eax,32", &[0xC1, 0xF0, 0x20], 0, false),
        ("sal rax,63", &[0x48, 0xC1, 0xF0, 0x3F], 0, true),
        ("sal rax,64", &[0x48, 0xC1, 0xF0, 0x40], 0, false),
        (
            "sal cl,cl alias",
            &[0xD2, 0xF1],
            0x1357_9BDF_2468_A501,
            true,
        ),
        ("sal ecx,cl alias", &[0xD3, 0xF1], 0x8000_0001, true),
        (
            "sal rcx,cl alias",
            &[0x48, 0xD3, 0xF1],
            0x8000_0000_0000_0001,
            true,
        ),
        (
            "sal rcx,cl masked zero alias",
            &[0x48, 0xD3, 0xF1],
            0x8000_0000_0000_0040,
            false,
        ),
        ("sal esp,1", &[0xD1, 0xF4], 0, true),
        ("sal rsp,1", &[0x48, 0xD1, 0xF4], 0, true),
        ("sal rbp,cl", &[0x48, 0xD3, 0xF5], 8, true),
        ("sal r15,cl", &[0x49, 0xD3, 0xF7], 63, true),
        ("inert REP sal al,1", &[0xF3, 0xC0, 0xF0, 1], 0, true),
    ];

    let mut profiles = 0usize;
    for (name, instruction, rcx, masked_nonzero) in cases {
        for rflags in [0x2, 0x2 | 0x8D5] {
            compare(name, instruction, *rcx, *masked_nonzero, rflags);
            profiles += 1;
        }
    }
    assert_eq!(profiles, cases.len() * 2);
}
