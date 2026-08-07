//! Native handoff coverage for terminal legacy `0F 38` map cells.

use super::*;

#[test]
fn jit_commits_supported_prefix_before_legacy_0f38_ud() {
    for (name, invalid) in [
        ("unassigned map cell", &[0x0F, 0x38, 0x0C][..]),
        ("disabled VMX INVEPT", &[0x66, 0x0F, 0x38, 0x80][..]),
        ("disabled CET WRUSS", &[0x66, 0x0F, 0x38, 0xF5][..]),
        ("disabled CET WRSSD", &[0x0F, 0x38, 0xF6][..]),
        ("disabled CET WRSSQ", &[0x48, 0x0F, 0x38, 0xF6][..]),
        (
            "disabled Key Locker ENCODEKEY128",
            &[0xF3, 0x0F, 0x38, 0xFA][..],
        ),
    ] {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12];
        code.extend_from_slice(invalid);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        let mut before = vcpu.get_regs().unwrap();
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: #UD must preserve the native prefix"
        );

        let at_frontier = vcpu.get_regs().unwrap();
        assert_eq!(at_frontier.rsi, 0x1234_5678, "{name}: native prefix");
        assert_eq!(at_frontier.rdi, before.rdi, "{name}: following MOV");
        assert_eq!(at_frontier.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(at_frontier.rip, LOAD_ADDR + 5, "{name}: frontier RIP");

        let error = vcpu.step().expect_err(name);
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "{name}: {error:?}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, at_frontier.rip, "{name}: fault RIP");
        assert_eq!(after.rsi, at_frontier.rsi, "{name}: RSI");
        assert_eq!(after.rdi, at_frontier.rdi, "{name}: RDI");
        assert_eq!(after.rflags, at_frontier.rflags, "{name}: RFLAGS");
    }
}

#[test]
fn jit_commits_supported_prefix_before_every_profile_disabled_legacy_wrss_class() {
    let cases: &[(&str, &[u8])] = &[
        ("WRSSD", &[0x0F, 0x38, 0xF6, 0x00]),
        ("address-size WRSSD", &[0x67, 0x0F, 0x38, 0xF6, 0x00]),
        ("FS WRSSD", &[0x64, 0x0F, 0x38, 0xF6, 0x00]),
        ("GS WRSSD", &[0x65, 0x0F, 0x38, 0xF6, 0x00]),
        ("REX.R WRSSD", &[0x44, 0x0F, 0x38, 0xF6, 0x00]),
        ("REX.B WRSSD", &[0x41, 0x0F, 0x38, 0xF6, 0x00]),
        ("WRSSQ", &[0x48, 0x0F, 0x38, 0xF6, 0x00]),
        ("REX.WRB WRSSQ", &[0x4D, 0x0F, 0x38, 0xF6, 0x00]),
    ];

    for &(name, disabled) in cases {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12];
        code.extend_from_slice(disabled);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        let mut before = vcpu.get_regs().unwrap();
        before.rax = 0x0000_8000_0000_0000;
        before.r8 = 0x0000_8000_0000_1000;
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name} {disabled:02X?}: {error}")),
            "{name}: terminal #UD must preserve the native prefix"
        );

        let at_frontier = vcpu.get_regs().unwrap();
        assert_eq!(at_frontier.rsi, 0x1234_5678, "{name}: native prefix");
        assert_eq!(at_frontier.rax, before.rax, "{name}: address operand");
        assert_eq!(
            at_frontier.r8, before.r8,
            "{name}: extended address operand"
        );
        assert_eq!(at_frontier.rdi, before.rdi, "{name}: following MOV");
        assert_eq!(at_frontier.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(at_frontier.rip, LOAD_ADDR + 5, "{name}: frontier RIP");

        let error = vcpu.step().expect_err(name);
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "{name}: {error:?}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, at_frontier.rip, "{name}: fault RIP");
        assert_eq!(after.rax, at_frontier.rax, "{name}: RAX");
        assert_eq!(after.r8, at_frontier.r8, "{name}: R8");
        assert_eq!(after.rsi, at_frontier.rsi, "{name}: RSI");
        assert_eq!(after.rdi, at_frontier.rdi, "{name}: RDI");
        assert_eq!(after.rflags, at_frontier.rflags, "{name}: RFLAGS");
    }
}

#[test]
fn jit_commits_supported_prefix_before_every_profile_disabled_key_locker_class() {
    let cases: &[(&str, &[u8])] = &[
        ("AESENCWIDE128KL", &[0xF3, 0x0F, 0x38, 0xD8, 0x00]),
        ("AESDECWIDE128KL", &[0xF3, 0x0F, 0x38, 0xD8, 0x08]),
        ("AESENCWIDE256KL", &[0xF3, 0x0F, 0x38, 0xD8, 0x10]),
        ("AESDECWIDE256KL", &[0xF3, 0x0F, 0x38, 0xD8, 0x18]),
        ("LOADIWKEY", &[0xF3, 0x0F, 0x38, 0xDC, 0xC1]),
        ("AESENC128KL", &[0xF3, 0x0F, 0x38, 0xDC, 0x00]),
        ("AESDEC128KL", &[0xF3, 0x0F, 0x38, 0xDD, 0x00]),
        ("AESENC256KL", &[0xF3, 0x0F, 0x38, 0xDE, 0x00]),
        ("AESDEC256KL", &[0xF3, 0x0F, 0x38, 0xDF, 0x00]),
        ("ENCODEKEY128", &[0xF3, 0x0F, 0x38, 0xFA, 0xC1]),
        ("ENCODEKEY256", &[0xF3, 0x0F, 0x38, 0xFB, 0xC1]),
        (
            "66 before F3 LOADIWKEY",
            &[0x66, 0xF3, 0x0F, 0x38, 0xDC, 0xC1],
        ),
        (
            "66 after F3 LOADIWKEY",
            &[0xF3, 0x66, 0x0F, 0x38, 0xDC, 0xC1],
        ),
        ("REX.W LOADIWKEY", &[0xF3, 0x48, 0x0F, 0x38, 0xDC, 0xC1]),
    ];

    for &(name, disabled) in cases {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12];
        code.extend_from_slice(disabled);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        let mut before = vcpu.get_regs().unwrap();
        before.rax = u64::MAX;
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        before.xmm[0] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        before.xmm[1] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name} {disabled:02X?}: {error}")),
            "{name}: terminal #UD must preserve the native prefix"
        );

        let at_frontier = vcpu.get_regs().unwrap();
        assert_eq!(at_frontier.rsi, 0x1234_5678, "{name}: native prefix");
        assert_eq!(at_frontier.rax, before.rax, "{name}: operand state");
        assert_eq!(at_frontier.rdi, before.rdi, "{name}: following MOV");
        assert_eq!(at_frontier.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(at_frontier.xmm[0], before.xmm[0], "{name}: XMM0");
        assert_eq!(at_frontier.xmm[1], before.xmm[1], "{name}: XMM1");
        assert_eq!(at_frontier.rip, LOAD_ADDR + 5, "{name}: frontier RIP");

        let error = vcpu.step().expect_err(name);
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "{name}: {error:?}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, at_frontier.rip, "{name}: fault RIP");
        assert_eq!(after.rax, at_frontier.rax, "{name}: RAX");
        assert_eq!(after.rsi, at_frontier.rsi, "{name}: RSI");
        assert_eq!(after.rdi, at_frontier.rdi, "{name}: RDI");
        assert_eq!(after.rflags, at_frontier.rflags, "{name}: RFLAGS");
        assert_eq!(after.xmm[0], at_frontier.xmm[0], "{name}: XMM0");
        assert_eq!(after.xmm[1], at_frontier.xmm[1], "{name}: XMM1");
    }
}
