//! Native-prefix handoff coverage for forbidden legacy prefixes before VEX/EVEX.

use super::*;

#[test]
fn jit_commits_supported_prefix_before_legacy_prefixed_vector_ud() {
    for (name, invalid) in [
        ("66 before VEX2", &[0x66, 0xC5, 0xF9, 0xEF, 0xC0][..]),
        (
            "REX hidden by 67 before VEX2",
            &[0x40, 0x67, 0xC5, 0xF9, 0xEF, 0xC0][..],
        ),
        ("F2 before VEX3", &[0xF2, 0xC4, 0xE1, 0x79, 0xEF, 0xC0][..]),
        (
            "REX hidden by FS before VEX3",
            &[0x48, 0x64, 0xC4, 0xE1, 0x79, 0xEF, 0xC0][..],
        ),
        (
            "F3 before EVEX",
            &[0xF3, 0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC0][..],
        ),
        ("LOCK before VEX2", &[0xF0, 0xC5, 0xF9, 0xEF, 0xC0][..]),
    ] {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12];
        code.extend_from_slice(invalid);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        let mut before = vcpu.get_regs().unwrap();
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        before.xmm[0] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        before.ymm_high[0] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
        before.zmm_high[0] = [1, 2, 3, 4];
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: terminal #UD must preserve the native prefix"
        );

        let at_frontier = vcpu.get_regs().unwrap();
        assert_eq!(at_frontier.rsi, 0x1234_5678, "{name}: native prefix");
        assert_eq!(at_frontier.rdi, before.rdi, "{name}: following MOV");
        assert_eq!(at_frontier.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(at_frontier.xmm[0], before.xmm[0], "{name}: XMM0");
        assert_eq!(
            at_frontier.ymm_high[0], before.ymm_high[0],
            "{name}: YMM0 high"
        );
        assert_eq!(
            at_frontier.zmm_high[0], before.zmm_high[0],
            "{name}: ZMM0 high"
        );
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
        assert_eq!(after.xmm[0], at_frontier.xmm[0], "{name}: XMM0");
        assert_eq!(
            after.ymm_high[0], at_frontier.ymm_high[0],
            "{name}: YMM0 high"
        );
        assert_eq!(
            after.zmm_high[0], at_frontier.zmm_high[0],
            "{name}: ZMM0 high"
        );
    }
}
