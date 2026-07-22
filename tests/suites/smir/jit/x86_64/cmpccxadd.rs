//! CMPccXADD strict-frontier native-JIT tests.

use super::*;

#[test]
fn jit_executes_supported_prefix_before_reserved_cmpccxadd_frontiers() {
    for (name, invalid, apx) in [
        (
            "VEX reserved mandatory prefix",
            &[0xC4, 0xE2, 0x72, 0xE2][..],
            false,
        ),
        (
            "VEX reserved vector length",
            &[0xC4, 0xE2, 0x75, 0xE2][..],
            false,
        ),
        (
            "VEX reserved register ModR/M",
            &[0xC4, 0xE2, 0x71, 0xE2, 0xC0][..],
            false,
        ),
        (
            "APX reserved payload",
            &[0x62, 0xEA, 0x61, 0x04, 0xE2][..],
            true,
        ),
        (
            "APX reserved mandatory prefix",
            &[0x62, 0xEA, 0x62, 0x00, 0xE2][..],
            true,
        ),
        (
            "APX reserved register ModR/M",
            &[0x62, 0xEA, 0x61, 0x00, 0xE2, 0xC0][..],
            true,
        ),
    ] {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12]; // mov esi,0x12345678
        code.extend_from_slice(invalid);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        vcpu.set_apx_enabled(apx);
        let mut before = vcpu.get_regs().unwrap();
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: reserved CMPccXADD frontier must retain the native prefix"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rsi, 0x1234_5678, "{name}: native prefix result");
        assert_eq!(after.rdi, before.rdi, "{name}: following instruction");
        assert_eq!(after.rflags, before.rflags, "{name}: flags");
        assert_eq!(after.rip, LOAD_ADDR + 5, "{name}: exact handoff PC");
    }
}
