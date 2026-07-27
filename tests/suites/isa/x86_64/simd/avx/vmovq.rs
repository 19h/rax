use crate::common::*;

#[derive(Clone, Copy, Debug)]
enum Direction {
    Load,
    Store,
}

impl Direction {
    fn pp(self) -> u8 {
        match self {
            Self::Load => 2,
            Self::Store => 1,
        }
    }

    fn opcode(self) -> u8 {
        match self {
            Self::Load => 0x7E,
            Self::Store => 0xD6,
        }
    }

    fn reg_rm(self, destination: u8, source: u8) -> (u8, u8) {
        match self {
            Self::Load => (destination, source),
            Self::Store => (source, destination),
        }
    }
}

fn c5_encoding(direction: Direction, destination: u8, source: u8) -> Vec<u8> {
    let (reg, rm) = direction.reg_rm(destination, source);
    assert!(reg < 16 && rm < 8);
    vec![
        0xC5,
        (if reg < 8 { 0x80 } else { 0 }) | 0x78 | direction.pp(),
        direction.opcode(),
        0xC0 | ((reg & 7) << 3) | rm,
        0xF4,
    ]
}

fn c4_encoding(
    direction: Direction,
    w: bool,
    ignored_x: bool,
    destination: u8,
    source: u8,
) -> Vec<u8> {
    let (reg, rm) = direction.reg_rm(destination, source);
    assert!(reg < 16 && rm < 16);
    let mut p0 = 0xE1;
    if reg >= 8 {
        p0 &= !0x80;
    }
    if ignored_x {
        p0 &= !0x40;
    }
    if rm >= 8 {
        p0 &= !0x20;
    }
    vec![
        0xC4,
        p0,
        (u8::from(w) << 7) | 0x78 | direction.pp(),
        direction.opcode(),
        0xC0 | ((reg & 7) << 3) | (rm & 7),
        0xF4,
    ]
}

fn vector(regs: &Registers, index: usize) -> [u64; 8] {
    [
        regs.xmm[index][0],
        regs.xmm[index][1],
        regs.ymm_high[index][0],
        regs.ymm_high[index][1],
        regs.zmm_high[index][0],
        regs.zmm_high[index][1],
        regs.zmm_high[index][2],
        regs.zmm_high[index][3],
    ]
}

fn set_vector(regs: &mut Registers, index: usize, value: [u64; 8]) {
    regs.xmm[index] = [value[0], value[1]];
    regs.ymm_high[index] = [value[2], value[3]];
    regs.zmm_high[index] = [value[4], value[5], value[6], value[7]];
}

fn patterned_registers() -> Registers {
    let mut regs = Registers {
        rax: 0x0123_4567_89AB_CDEF,
        rbx: 0x1122_3344_5566_7788,
        rflags: 0x2 | 0x8D5 | (1 << 10),
        k: [
            0x6996_F00F_3CC3_A55A,
            0,
            1,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            u64::MAX,
        ],
        ..Registers::default()
    };
    for index in 0..16 {
        set_vector(
            &mut regs,
            index,
            std::array::from_fn(|word| {
                0xC33C_F00F_6996_A55Au64.rotate_left((index * 11 + word * 17) as u32)
                    ^ (index as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
            }),
        );
    }
    regs
}

fn assert_register_state_eq(actual: &Registers, expected: &Registers, context: &str) {
    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rbx, regs.rcx, regs.rdx, regs.rsi, regs.rdi, regs.rsp, regs.rbp,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    assert_eq!(gprs(actual), gprs(expected), "{context}: GPRs");
    assert_eq!(actual.rip, expected.rip, "{context}: RIP");
    assert_eq!(actual.rflags, expected.rflags, "{context}: RFLAGS");
    assert_eq!(actual.xmm, expected.xmm, "{context}: XMM");
    assert_eq!(actual.ymm_high, expected.ymm_high, "{context}: YMM high");
    assert_eq!(actual.zmm_high, expected.zmm_high, "{context}: ZMM high");
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "{context}: extended ZMM");
    assert_eq!(actual.k, expected.k, "{context}: opmask");
    assert_eq!(actual.mm, expected.mm, "{context}: MMX");
}

fn assert_vmovq_result(code: &[u8], destination: u8, source: u8) {
    let (mut vcpu, _) = setup_vm(code, Some(patterned_registers()));
    let before = vcpu.get_regs().unwrap();
    let low_qword = vector(&before, usize::from(source))[0];
    let actual = run_until_hlt(&mut vcpu).unwrap();
    let mut expected = before;
    set_vector(
        &mut expected,
        usize::from(destination),
        [low_qword, 0, 0, 0, 0, 0, 0, 0],
    );
    expected.rip = actual.rip;
    assert_register_state_eq(
        &actual,
        &expected,
        &format!("destination={destination} source={source} code={code:02X?}"),
    );
}

#[test]
fn vmovq_register_aliases_cover_compact_extended_wig_x_and_high_registers() {
    assert_vmovq_result(&c5_encoding(Direction::Load, 9, 7), 9, 7);
    assert_vmovq_result(&c5_encoding(Direction::Store, 7, 9), 7, 9);

    for direction in [Direction::Load, Direction::Store] {
        for w in [false, true] {
            for ignored_x in [false, true] {
                assert_vmovq_result(&c4_encoding(direction, w, ignored_x, 15, 14), 15, 14);
                assert_vmovq_result(&c4_encoding(direction, w, ignored_x, 0, 0), 0, 0);
            }
        }
    }
}

#[test]
fn vmovq_writes_clear_zmm_upper_when_the_low_256_bit_result_is_unchanged() {
    let value = 0x0123_4567_89AB_CDEF;
    for code in [
        vec![0xC4, 0xE1, 0xF9, 0x6E, 0xC0, 0xF4], // VMOVQ xmm0,rax
        vec![0xC5, 0xFA, 0x7E, 0xC0, 0xF4],       // VMOVQ xmm0,xmm0
        vec![0xC5, 0xF9, 0xD6, 0xC0, 0xF4],       // VMOVQ xmm0,xmm0
    ] {
        let mut initial = patterned_registers();
        initial.rax = value;
        initial.xmm[0] = [value, 0];
        initial.ymm_high[0] = [0, 0];
        initial.zmm_high[0] = [1, 2, 3, 4];
        let (mut vcpu, _) = setup_vm(&code, Some(initial));
        let actual = run_until_hlt(&mut vcpu).unwrap();
        assert_eq!(vector(&actual, 0), [value, 0, 0, 0, 0, 0, 0, 0]);
    }
}

fn assert_reserved_vex_move_ud_noncommitting(code: &[u8], name: &str) {
    let (mut vcpu, _) = setup_vm_no_idt(code, Some(patterned_registers()));
    for path in ["cold decode", "decode-cache hit"] {
        let before = vcpu.get_regs().unwrap();
        let error = vcpu
            .step()
            .expect_err("reserved VEX move encoding must raise #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name} ({path}): expected #UD delivery failure, got {error}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_register_state_eq(&after, &before, &format!("{name} ({path})"));
    }
}

#[test]
fn vex_vmovd_vmovq_reserved_l_and_vvvv_raise_ud_without_committing() {
    for (pp, opcode, mnemonic) in [
        (1, 0x6E, "VMOVD/Q r/m-to-XMM"),
        (1, 0x7E, "VMOVD/Q XMM-to-r/m"),
        (2, 0x7E, "VMOVQ XMM-to-XMM load alias"),
        (1, 0xD6, "VMOVQ XMM-to-XMM store alias"),
    ] {
        for w in [false, true] {
            let valid_p1 = (u8::from(w) << 7) | 0x78 | pp;
            for (p1, field) in [(valid_p1 | 0x04, "L=1"), (valid_p1 & !0x08, "vvvv!=1111")] {
                let code = [0xC4, 0xE1, p1, opcode, 0xC1, 0xF4];
                assert_reserved_vex_move_ud_noncommitting(
                    &code,
                    &format!("{mnemonic} W={} {field}", u8::from(w)),
                );
            }
        }
    }
}
