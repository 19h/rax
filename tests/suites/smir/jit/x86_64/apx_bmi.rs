//! Intel APX promoted-BMI native-JIT and strict-frontier tests.

use super::*;

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn seed_bmi_registers(vcpu: &mut X86_64Vcpu, apx: bool) {
    vcpu.set_apx_enabled(apx);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rax = 0x0FF0_F0F0_8000_0018;
    regs.rbx = 0xF0FF_00FF_1234_5678;
    regs.rcx = (8 << 8) | 4;
    regs.rdx = 0;
    regs.r8 = 0xA5A5_5A5A_DEAD_BEEF;
    regs.r9 = 0x0102_0304_0506_0708;
    regs.r10 = 0x1112_1314_1516_1718;
    regs.r11 = 0x2122_2324_2526_2728;
    regs.r12 = 0x3132_3334_3536_3738;
    regs.r13 = 0x4142_4344_4546_4748;
    regs.r14 = 0x5152_5354_5556_5758;
    regs.r15 = 0x6162_6364_6566_6768;
    regs.rflags = 0xCD7;
    vcpu.set_regs(&regs).unwrap();
}

#[test]
fn jit_apx_bmi_nf0_register_forms_match_vex_interpreter_for_both_widths() {
    struct Case {
        name: &'static str,
        apx: &'static [u8],
        vex: &'static [u8],
        bmi2: bool,
    }

    if !std::is_x86_feature_detected!("bmi1") {
        return;
    }
    let bmi2 = std::is_x86_feature_detected!("bmi2");
    let cases = [
        Case {
            name: "ANDN r64 NF=0",
            apx: &[0x62, 0x72, 0xFC, 0x08, 0xF2, 0xC3],
            vex: &[0xC4, 0x62, 0xF8, 0xF2, 0xC3],
            bmi2: false,
        },
        Case {
            name: "ANDN r32 NF=0",
            apx: &[0x62, 0x72, 0x7C, 0x08, 0xF2, 0xC3],
            vex: &[0xC4, 0x62, 0x78, 0xF2, 0xC3],
            bmi2: false,
        },
        Case {
            name: "BLSR r64 NF=0 alias",
            apx: &[0x62, 0xF2, 0xFC, 0x08, 0xF3, 0xC8],
            vex: &[0xC4, 0xE2, 0xF8, 0xF3, 0xC8],
            bmi2: false,
        },
        Case {
            name: "BLSR r32 NF=0 alias",
            apx: &[0x62, 0xF2, 0x7C, 0x08, 0xF3, 0xC8],
            vex: &[0xC4, 0xE2, 0x78, 0xF3, 0xC8],
            bmi2: false,
        },
        Case {
            name: "BLSMSK r64 NF=0 zero source",
            apx: &[0x62, 0xF2, 0xF4, 0x08, 0xF3, 0xD2],
            vex: &[0xC4, 0xE2, 0xF0, 0xF3, 0xD2],
            bmi2: false,
        },
        Case {
            name: "BLSMSK r32 NF=0 zero source",
            apx: &[0x62, 0xF2, 0x74, 0x08, 0xF3, 0xD2],
            vex: &[0xC4, 0xE2, 0x70, 0xF3, 0xD2],
            bmi2: false,
        },
        Case {
            name: "BLSI r64 NF=0",
            apx: &[0x62, 0xF2, 0xBC, 0x08, 0xF3, 0xDB],
            vex: &[0xC4, 0xE2, 0xB8, 0xF3, 0xDB],
            bmi2: false,
        },
        Case {
            name: "BLSI r32 NF=0",
            apx: &[0x62, 0xF2, 0x3C, 0x08, 0xF3, 0xDB],
            vex: &[0xC4, 0xE2, 0x38, 0xF3, 0xDB],
            bmi2: false,
        },
        Case {
            name: "BZHI r64 NF=0",
            apx: &[0x62, 0xF2, 0xF4, 0x08, 0xF5, 0xC3],
            vex: &[0xC4, 0xE2, 0xF0, 0xF5, 0xC3],
            bmi2: true,
        },
        Case {
            name: "BZHI r32 NF=0",
            apx: &[0x62, 0xF2, 0x74, 0x08, 0xF5, 0xC3],
            vex: &[0xC4, 0xE2, 0x70, 0xF5, 0xC3],
            bmi2: true,
        },
        Case {
            name: "BEXTR r64 NF=0",
            apx: &[0x62, 0xF2, 0xF4, 0x08, 0xF7, 0xC3],
            vex: &[0xC4, 0xE2, 0xF0, 0xF7, 0xC3],
            bmi2: false,
        },
        Case {
            name: "BEXTR r32 NF=0",
            apx: &[0x62, 0xF2, 0x74, 0x08, 0xF7, 0xC3],
            vex: &[0xC4, 0xE2, 0x70, 0xF7, 0xC3],
            bmi2: false,
        },
    ];

    for case in cases {
        if case.bmi2 && !bmi2 {
            continue;
        }

        let mut reference_code = case.vex.to_vec();
        reference_code.push(0xF4);
        let mut reference = make_vcpu_code(&reference_code);
        seed_bmi_registers(&mut reference, false);
        assert!(
            reference.step().unwrap().is_none(),
            "{} VEX interpreter",
            case.name
        );
        let expected = reference.get_regs().unwrap();

        let mut jit_code = case.apx.to_vec();
        jit_code.push(0xF4);
        let mut jit = make_vcpu_code(&jit_code);
        seed_bmi_registers(&mut jit, true);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();

        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
    }
}

#[test]
fn jit_executes_supported_prefix_before_reserved_apx_bmi_frontiers() {
    for (name, invalid) in [
        (
            "ANDN reserved payload bit",
            &[0x62, 0x72, 0xFC, 0x18, 0xF2][..],
        ),
        ("ANDN reserved pp", &[0x62, 0x72, 0xFD, 0x08, 0xF2][..]),
        ("BLS reserved /0", &[0x62, 0xF2, 0xFC, 0x08, 0xF3, 0x04][..]),
        (
            "BLS reserved /4 requiring SIB",
            &[0x62, 0xF2, 0xFC, 0x08, 0xF3, 0x24][..],
        ),
        (
            "ANDN register U=0",
            &[0x62, 0x72, 0xF8, 0x08, 0xF2, 0xC3][..],
        ),
        ("PDEP reserved NF", &[0x62, 0xE2, 0xE7, 0x04, 0xF5][..]),
        ("RORX reserved VVVV", &[0x62, 0xE3, 0xEF, 0x08, 0xF0][..]),
        (
            "RORX register U=0",
            &[0x62, 0xE3, 0xFB, 0x08, 0xF0, 0xE3][..],
        ),
    ] {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12]; // mov esi,0x12345678
        code.extend_from_slice(invalid);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        vcpu.set_apx_enabled(true);
        let mut before = vcpu.get_regs().unwrap();
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: reserved APX BMI frontier must retain the native prefix"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rsi, 0x1234_5678, "{name}: native prefix result");
        assert_eq!(after.rdi, before.rdi, "{name}: following instruction");
        assert_eq!(after.rflags, before.rflags, "{name}: flags");
        assert_eq!(after.rip, LOAD_ADDR + 5, "{name}: exact handoff PC");
    }
}
