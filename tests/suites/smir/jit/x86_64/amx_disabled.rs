//! Native-prefix handoff coverage for RAX's profile-disabled Intel AMX cells.

use super::*;

#[test]
fn jit_commits_supported_prefix_before_every_profile_disabled_amx_cell() {
    let mut cases: Vec<(&str, Vec<u8>)> = vec![
        ("TMMULTF32PS", vec![0xC4, 0xE2, 0x69, 0x48, 0xC1]),
        ("LDTILECFG", vec![0xC4, 0xE2, 0x78, 0x49, 0x80, 0, 4, 0, 0]),
        ("STTILECFG", vec![0xC4, 0xE2, 0x79, 0x49, 0x40, 0x20]),
        ("TILEZERO", vec![0xC4, 0xE2, 0x7B, 0x49, 0xC0]),
        (
            "TILELOADDRS",
            vec![0xC4, 0xE2, 0x7B, 0x4A, 0x44, 0x88, 0x20],
        ),
        (
            "TILELOADDRST1",
            vec![0xC4, 0xE2, 0x79, 0x4A, 0x44, 0x88, 0x20],
        ),
        ("TILELOADD", vec![0xC4, 0xE2, 0x7B, 0x4B, 0x44, 0x88, 0x20]),
        (
            "TILELOADDT1",
            vec![0xC4, 0xE2, 0x79, 0x4B, 0x44, 0x88, 0x20],
        ),
        ("TILESTORED", vec![0xC4, 0xE2, 0x7A, 0x4B, 0x44, 0x88, 0x20]),
        ("TDPBF16PS", vec![0xC4, 0xE2, 0x6A, 0x5C, 0xC1]),
        ("TDPFP16PS", vec![0xC4, 0xE2, 0x6B, 0x5C, 0xC1]),
        ("TDPBUUD", vec![0xC4, 0xE2, 0x68, 0x5E, 0xC1]),
        ("TDPBUSD", vec![0xC4, 0xE2, 0x69, 0x5E, 0xC1]),
        ("TDPBSUD", vec![0xC4, 0xE2, 0x6A, 0x5E, 0xC1]),
        ("TDPBSSD", vec![0xC4, 0xE2, 0x6B, 0x5E, 0xC1]),
        ("TCMMRLFP16PS", vec![0xC4, 0xE2, 0x68, 0x6C, 0xC1]),
        ("TCMMIMFP16PS", vec![0xC4, 0xE2, 0x69, 0x6C, 0xC1]),
    ];

    for (opcode, pp_values) in [(0x4A, &[1_u8, 2_u8][..]), (0x6D, &[0, 1, 2, 3][..])] {
        for &pp in pp_values {
            cases.push((
                "AMX-AVX512 register-selected row",
                vec![0x62, 0xF2, 0x6C | pp, 0x48, opcode, 0xC1],
            ));
        }
    }
    for (opcode, pp_values) in [(0x07, &[0_u8, 1, 2, 3][..]), (0x77, &[2, 3][..])] {
        for &pp in pp_values {
            cases.push((
                "AMX-AVX512 immediate-selected row",
                vec![0x62, 0xF3, 0x7C | pp, 0x48, opcode, 0xC1, 0xA5],
            ));
        }
    }

    assert_eq!(cases.len(), 29);
    for (name, disabled) in cases {
        let mut code = vec![0xBE, 0x78, 0x56, 0x34, 0x12];
        code.extend_from_slice(&disabled);
        code.extend_from_slice(&[0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4]);

        let mut vcpu = make_vcpu_code(&code);
        let mut before = vcpu.get_regs().unwrap();
        before.rax = u64::MAX;
        before.rdi = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name} {disabled:02X?}: {error}")),
            "{name} {disabled:02X?}: terminal #UD must preserve the native prefix"
        );

        let at_frontier = vcpu.get_regs().unwrap();
        assert_eq!(at_frontier.rsi, 0x1234_5678, "{name}: native prefix");
        assert_eq!(at_frontier.rax, before.rax, "{name}: operand state");
        assert_eq!(at_frontier.rdi, before.rdi, "{name}: following MOV");
        assert_eq!(at_frontier.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(at_frontier.rip, LOAD_ADDR + 5, "{name}: frontier RIP");

        let error = match vcpu.step() {
            Err(error) => error,
            Ok(exit) => panic!("{name}: disabled AMX unexpectedly retired with {exit:?}"),
        };
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
    }
}
