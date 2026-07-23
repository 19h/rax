//! Native-prefix handoff coverage for unassigned Intel APX MAP4 opcodes.

use super::*;

#[test]
fn jit_commits_supported_prefix_before_unassigned_apx_map4_ud() {
    // Intel APX Architecture Specification revision 7.0 section 3.1.5 does
    // not promote legacy MOV (89) or LEA (8D) into EVEX MAP4. Both remain
    // available through REX2, but these EVEX opcode bytes are unassigned.
    for (name, opcode) in [("unassigned MOV", 0x89), ("unassigned LEA", 0x8D)] {
        let code = [
            0xBE, 0x78, 0x56, 0x34, 0x12, // MOV ESI,0x12345678
            0x62, 0xF4, 0x7C, 0x08, opcode, // unassigned EVEX MAP4 opcode
            0xBF, 0x01, 0x00, 0x00, 0x00, // following MOV EDI,1
            0xF4,
        ];

        let mut vcpu = make_vcpu_code(&code);
        vcpu.set_apx_enabled(true);
        let mut before = vcpu.get_regs().unwrap();
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

#[test]
fn jit_commits_supported_prefix_before_profile_disabled_f8_ud() {
    for (name, disabled) in [
        ("legacy ENQCMD", &[0xF2, 0x0F, 0x38, 0xF8][..]),
        ("legacy ENQCMDS", &[0xF3, 0x0F, 0x38, 0xF8][..]),
        ("APX F2 F8", &[0x62, 0xF4, 0x7F, 0x08, 0xF8][..]),
        ("APX F3 F8", &[0x62, 0xF4, 0x7E, 0x08, 0xF8][..]),
    ] {
        let mut code = vec![
            0xBE, 0x78, 0x56, 0x34, 0x12, // MOV ESI,0x12345678
        ];
        code.extend_from_slice(disabled);
        code.extend_from_slice(&[
            0xBF, 0x01, 0x00, 0x00, 0x00, // following MOV EDI,1
            0xF4,
        ]);

        let mut vcpu = make_vcpu_code(&code);
        vcpu.set_apx_enabled(true);
        let mut before = vcpu.get_regs().unwrap();
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
