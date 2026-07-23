//! Native-prefix handoff coverage for profile-disabled Intel APX CET stores.

use super::*;

#[test]
fn jit_executes_supported_prefix_before_profile_disabled_apx_cet_traps() {
    for (name, instruction) in [
        ("WRSSD", &[0x62, 0xF4, 0x7C, 0x08, 0x66, 0x03][..]),
        ("WRSSQ", &[0x62, 0xF4, 0xFC, 0x08, 0x66, 0x03][..]),
        ("WRUSSD", &[0x62, 0xF4, 0x7D, 0x08, 0x65, 0x03][..]),
        ("WRUSSQ", &[0x62, 0xF4, 0xFD, 0x08, 0x65, 0x03][..]),
    ] {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12]; // MOV ESI,0x12345678
        code.extend_from_slice(instruction);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        vcpu.set_apx_enabled(true);
        let mut before = vcpu.get_regs().unwrap();
        before.rax = MEM_SIZE + 0x1000;
        before.rbx = MEM_SIZE + 0x2000;
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
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
