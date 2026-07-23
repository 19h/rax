//! Architectural #UD coverage for legacy prefixes preceding VEX.

use crate::common::*;
use rax::vm::vcpu::Registers;

const ALLOWED_PREFIXES: [u8; 7] = [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67];
const VEX_FORMS: [(&str, &[u8]); 2] = [
    ("VEX2 VPXOR", &[0xC5, 0xF9, 0xEF, 0xC0]),
    ("VEX3 VPXOR", &[0xC4, 0xE1, 0x79, 0xEF, 0xC0]),
];

fn assert_vex_ud_noncommitting(prefixes: &[u8], vex: &[u8], name: &str) {
    let mut code = prefixes.to_vec();
    code.extend_from_slice(vex);
    code.push(0xF4);

    let mut initial = Registers::default();
    initial.rax = 0x0123_4567_89AB_CDEF;
    initial.rflags = 0x2 | 0x8D5;
    initial.xmm[0] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
    initial.ymm_high[0] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
    initial.zmm_high[0] = [1, 2, 3, 4];

    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));
    for path in ["cold decode", "decode-cache hit"] {
        let before = vcpu.get_regs().unwrap();
        let error = match vcpu.step() {
            Err(error) => error,
            Ok(exit) => panic!("{name} ({path}): expected #UD, got {exit:?}"),
        };
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name} ({path}): expected #UD delivery failure, got {error}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, before.rip, "{name} ({path}): fault RIP");
        assert_eq!(after.rax, before.rax, "{name} ({path}): RAX");
        assert_eq!(after.rflags, before.rflags, "{name} ({path}): RFLAGS");
        assert_eq!(after.xmm[0], before.xmm[0], "{name} ({path}): XMM0");
        assert_eq!(
            after.ymm_high[0], before.ymm_high[0],
            "{name} ({path}): YMM0 high"
        );
        assert_eq!(
            after.zmm_high[0], before.zmm_high[0],
            "{name} ({path}): ZMM0 high"
        );
    }
}

#[test]
fn forbidden_legacy_prefixes_before_vex_raise_ud_without_committing() {
    for (encoding, vex) in VEX_FORMS {
        for forbidden in [0xF0_u8, 0x66, 0xF2, 0xF3] {
            let name = format!("{encoding}, prefix {forbidden:02X}");
            assert_vex_ud_noncommitting(&[forbidden], vex, &name);
        }

        for rex in 0x40_u8..=0x4F {
            let name = format!("{encoding}, REX {rex:02X}");
            assert_vex_ud_noncommitting(&[rex], vex, &name);
            for allowed in ALLOWED_PREFIXES {
                let name = format!("{encoding}, hidden REX {rex:02X}/{allowed:02X}");
                assert_vex_ud_noncommitting(&[rex, allowed], vex, &name);
                let name = format!("{encoding}, {allowed:02X}/REX {rex:02X}");
                assert_vex_ud_noncommitting(&[allowed, rex], vex, &name);
            }
        }
    }
}

#[test]
fn address_size_and_segment_prefixes_before_vex_remain_valid() {
    for prefixes in ALLOWED_PREFIXES.iter().map(|prefix| vec![*prefix]).chain(
        ALLOWED_PREFIXES
            .iter()
            .filter(|prefix| **prefix != 0x67)
            .map(|segment| vec![*segment, 0x67]),
    ) {
        for (encoding, vex) in VEX_FORMS {
            let mut code = prefixes.clone();
            code.extend_from_slice(vex);
            code.push(0xF4);

            let mut initial = Registers::default();
            initial.xmm[0] = [u64::MAX, u64::MAX];
            let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));
            let exit = vcpu
                .step()
                .unwrap_or_else(|error| panic!("{encoding}, {prefixes:02X?}: {error}"));
            assert!(exit.is_none(), "{encoding}, {prefixes:02X?}: {exit:?}");
            let after = vcpu.get_regs().unwrap();
            assert_eq!(after.rip, CODE_ADDR + (code.len() - 1) as u64);
            assert_eq!(after.xmm[0], [0, 0], "{encoding}, {prefixes:02X?}");
        }
    }
}
