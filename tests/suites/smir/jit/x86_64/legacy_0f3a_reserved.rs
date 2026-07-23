//! Native handoff coverage for reserved legacy `0F 3A` map cells.

use super::*;

#[test]
fn jit_commits_supported_prefix_before_legacy_0f3a_ud() {
    for (name, invalid) in [
        ("unassigned map cell", &[0x0F, 0x3A, 0x00][..]),
        (
            "legacy spelling of VEX-only RORX",
            &[0xF2, 0x0F, 0x3A, 0xF0][..],
        ),
        (
            "legacy spelling of EVEX-only VPTERNLOGD",
            &[0x66, 0x0F, 0x3A, 0x25][..],
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
