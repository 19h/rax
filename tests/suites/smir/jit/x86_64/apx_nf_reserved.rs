//! Native-JIT handoff tests for APX NF-reserved operations.

use super::*;

#[test]
fn jit_commits_supported_prefix_before_apx_nf_ud_frontiers() {
    for (name, invalid) in [
        ("ADC opcode frontier", &[0x62, 0xF4, 0xBC, 0x1C, 0x11][..]),
        ("SBB opcode frontier", &[0x62, 0xF4, 0xBC, 0x1C, 0x19][..]),
        (
            "ADC immediate ModR/M before missing SIB and immediate",
            &[0x62, 0xF4, 0xBC, 0x1C, 0x83, 0x14][..],
        ),
        (
            "SBB immediate ModR/M before missing displacement and immediate",
            &[0x62, 0xF4, 0xBC, 0x1C, 0x81, 0x1D][..],
        ),
        (
            "RCL ModR/M before missing SIB and immediate",
            &[0x62, 0xF4, 0xBC, 0x1C, 0xC1, 0x14][..],
        ),
        (
            "RCR ModR/M before missing displacement",
            &[0x62, 0xF4, 0xBC, 0x1C, 0xD3, 0x1D][..],
        ),
        (
            "NOT ModR/M before missing SIB",
            &[0x62, 0xF4, 0xBC, 0x1C, 0xF7, 0x14][..],
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
            "{name}: reserved APX NF frontier must retain the native prefix"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rsi, 0x1234_5678, "{name}: native prefix result");
        assert_eq!(after.rdi, before.rdi, "{name}: following instruction");
        assert_eq!(after.rflags, before.rflags, "{name}: flags");
        assert_eq!(after.rip, LOAD_ADDR + 5, "{name}: exact handoff PC");
    }
}
