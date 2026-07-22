//! Intel APX Revision 7.0 CCMP/CTEST direct-execution coverage.

use crate::common::*;

const CF: u64 = 1 << 0;
const PF: u64 = 1 << 2;
const AF: u64 = 1 << 4;
const ZF: u64 = 1 << 6;
const SF: u64 = 1 << 7;
const IF: u64 = 1 << 9;
const DF: u64 = 1 << 10;
const OF: u64 = 1 << 11;
const AC: u64 = 1 << 18;
const STATUS: u64 = CF | PF | AF | ZF | SF | OF;
const PRESERVED: u64 = IF | DF | AC;
const NONCANONICAL: u64 = 0x0000_8000_0000_0000;

fn conditional_prefix(dfv: u8, w: bool, pp: u8, scc: u8, u: bool) -> [u8; 4] {
    let p1 = (if w { 0x80 } else { 0 }) | ((dfv & 0x0F) << 3) | (if u { 0x04 } else { 0 }) | pp;
    [0x62, 0xF4, p1, scc & 0x0F]
}

fn instruction(dfv: u8, w: bool, pp: u8, scc: u8, u: bool, opcode_and_operands: &[u8]) -> Vec<u8> {
    let mut code = conditional_prefix(dfv, w, pp, scc, u).to_vec();
    code.extend_from_slice(opcode_and_operands);
    code.push(0xF4);
    code
}

fn default_status(dfv: u8) -> u64 {
    (if dfv & 0x1 != 0 { CF | PF } else { 0 })
        | (if dfv & 0x2 != 0 { ZF } else { 0 })
        | (if dfv & 0x4 != 0 { SF } else { 0 })
        | (if dfv & 0x8 != 0 { OF } else { 0 })
}

fn scc_holds(scc: u8, rflags: u64) -> bool {
    let cf = rflags & CF != 0;
    let zf = rflags & ZF != 0;
    let sf = rflags & SF != 0;
    let of = rflags & OF != 0;
    match scc {
        0x0 => of,
        0x1 => !of,
        0x2 => cf,
        0x3 => !cf,
        0x4 => zf,
        0x5 => !zf,
        0x6 => cf || zf,
        0x7 => !cf && !zf,
        0x8 => sf,
        0x9 => !sf,
        0xA => true,
        0xB => false,
        0xC => sf != of,
        0xD => sf == of,
        0xE => zf || sf != of,
        0xF => !zf && sf == of,
        _ => unreachable!(),
    }
}

fn execute(code: &[u8], mut regs: Registers) -> Registers {
    regs.rflags |= 0x2;
    let (mut vcpu, _) = setup_apx_vm(code, Some(regs));
    run_until_hlt(&mut vcpu).unwrap_or_else(|error| panic!("{code:02X?}: {error:#}"))
}

fn scalar_image(regs: &Registers) -> [u64; 34] {
    [
        regs.rax,
        regs.rbx,
        regs.rcx,
        regs.rdx,
        regs.rsi,
        regs.rdi,
        regs.rsp,
        regs.rbp,
        regs.r8,
        regs.r9,
        regs.r10,
        regs.r11,
        regs.r12,
        regs.r13,
        regs.r14,
        regs.r15,
        regs.r16,
        regs.r17,
        regs.r18,
        regs.r19,
        regs.r20,
        regs.r21,
        regs.r22,
        regs.r23,
        regs.r24,
        regs.r25,
        regs.r26,
        regs.r27,
        regs.r28,
        regs.r29,
        regs.r30,
        regs.r31,
        regs.rip,
        regs.rflags,
    ]
}

fn assert_precise_exception(code: &[u8], vector: u8, configure: impl FnOnce(&mut Registers)) {
    let mut regs = Registers {
        rax: NONCANONICAL,
        rbx: NONCANONICAL,
        rcx: 0x1111_2222_3333_4444,
        rdx: 0x5555_6666_7777_8888,
        rflags: 0x2 | STATUS | PRESERVED,
        ..Registers::default()
    };
    configure(&mut regs);
    let (mut vcpu, _) = setup_apx_vm_no_idt(code, Some(regs));
    let before = vcpu.get_regs().unwrap();
    let error = match vcpu.step() {
        Err(error) => format!("{error:#}"),
        Ok(exit) => panic!("expected exception {vector} for {code:02X?}, got {exit:?}"),
    };
    assert!(
        error.contains(&format!("IDT entry {vector} not present")),
        "{code:02X?}: expected vector {vector}, got {error}"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(scalar_image(&after), scalar_image(&before), "{code:02X?}");
}

fn assert_precise_memory_fault(code: &[u8]) {
    let regs = Registers {
        rax: 0x1111_2222_3333_4444,
        rbx: NONCANONICAL,
        rflags: 0x2 | STATUS | PRESERVED,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_apx_vm_no_idt(code, Some(regs));
    let before = vcpu.get_regs().unwrap();
    let error = match vcpu.step() {
        Err(error) => format!("{error:#}"),
        Ok(exit) => panic!("false-SCC memory operand did not fault for {code:02X?}: {exit:?}"),
    };
    assert!(
        error.contains("failed to read") || error.contains("IDT entry 13 not present"),
        "{code:02X?}: unexpected memory-fault diagnostic: {error}"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(scalar_image(&after), scalar_image(&before), "{code:02X?}");
}

#[test]
fn every_scc_selects_compare_or_default_flags_exactly() {
    let patterns = [0, STATUS, CF | ZF, SF];
    let dfv = 0x0D;
    for scc in 0..=0x0F {
        for status in patterns {
            let initial = 0x2 | PRESERVED | status;
            let regs = Registers {
                rax: 5,
                rbx: 5,
                rflags: initial,
                ..Registers::default()
            };
            let code = instruction(dfv, true, 0, scc, true, &[0x39, 0xD8]);
            let result = execute(&code, regs);
            let selected = if scc_holds(scc, initial) {
                PF | ZF // 5 - 5 = 0
            } else {
                default_status(dfv)
            };
            assert_eq!(
                result.rflags & (STATUS | PRESERVED | 0x2),
                0x2 | PRESERVED | selected,
                "SCC={scc:X} initial={initial:#x}"
            );
        }
    }
}

#[test]
fn every_dfv_value_sets_cf_pf_zf_sf_of_and_clears_af() {
    for dfv in 0..=0x0F {
        for tail in [
            &[0x39, 0xD8][..],
            &[0xF7, 0xC8, 0xFF, 0x00, 0x00, 0x00][..], // CTEST F7 /1
        ] {
            let regs = Registers {
                rax: 0xFFFF,
                rbx: 1,
                rflags: 0x2 | PRESERVED | STATUS,
                ..Registers::default()
            };
            let code = instruction(dfv, true, 0, 0x0B, true, tail);
            let result = execute(&code, regs);
            assert_eq!(
                result.rflags & (STATUS | PRESERVED | 0x2),
                0x2 | PRESERVED | default_status(dfv),
                "DFV={dfv:X} tail={tail:02X?}"
            );
        }
    }
}

#[test]
fn all_promoted_ccmp_forms_execute_at_each_operand_width() {
    struct Case {
        name: &'static str,
        w: bool,
        pp: u8,
        tail: &'static [u8],
    }
    let cases = [
        Case {
            name: "38 /r",
            w: false,
            pp: 0,
            tail: &[0x38, 0xD8],
        },
        Case {
            name: "39 /r W16",
            w: false,
            pp: 1,
            tail: &[0x39, 0xD8],
        },
        Case {
            name: "39 /r W32",
            w: false,
            pp: 0,
            tail: &[0x39, 0xD8],
        },
        Case {
            name: "39 /r W64",
            w: true,
            pp: 0,
            tail: &[0x39, 0xD8],
        },
        Case {
            name: "3A /r",
            w: true,
            pp: 0,
            tail: &[0x3A, 0xD8],
        },
        Case {
            name: "3B /r",
            w: true,
            pp: 0,
            tail: &[0x3B, 0xD8],
        },
        Case {
            name: "80 /7",
            w: true,
            pp: 0,
            tail: &[0x80, 0xF8, 5],
        },
        Case {
            name: "81 /7 W16",
            w: false,
            pp: 1,
            tail: &[0x81, 0xF8, 5, 0],
        },
        Case {
            name: "81 /7 W32",
            w: false,
            pp: 0,
            tail: &[0x81, 0xF8, 5, 0, 0, 0],
        },
        Case {
            name: "81 /7 W64",
            w: true,
            pp: 0,
            tail: &[0x81, 0xF8, 5, 0, 0, 0],
        },
        Case {
            name: "83 /7 W16",
            w: false,
            pp: 1,
            tail: &[0x83, 0xF8, 5],
        },
        Case {
            name: "83 /7 W32",
            w: false,
            pp: 0,
            tail: &[0x83, 0xF8, 5],
        },
        Case {
            name: "83 /7 W64",
            w: true,
            pp: 0,
            tail: &[0x83, 0xF8, 5],
        },
    ];
    for case in cases {
        let regs = Registers {
            rax: 5,
            rbx: 5,
            rflags: 0x2 | PRESERVED | STATUS,
            ..Registers::default()
        };
        let code = instruction(0, case.w, case.pp, 0x0A, true, case.tail);
        let result = execute(&code, regs);
        assert_eq!(result.rflags & STATUS, PF | ZF, "{}", case.name);
        assert_eq!(result.rflags & PRESERVED, PRESERVED, "{}", case.name);
    }
}

#[test]
fn ccmp_register_direction_is_opcode_exact() {
    let regs = Registers {
        rax: 1,
        rbx: 2,
        rflags: 0x2 | PRESERVED,
        ..Registers::default()
    };

    // 39 /r compares r/m64 - r64: RAX - RBX = -1.
    let rm_reg = instruction(0, true, 0, 0x0A, true, &[0x39, 0xD8]);
    let result = execute(&rm_reg, regs.clone());
    assert_eq!(result.rflags & STATUS, CF | PF | AF | SF);

    // 3B /r reverses the operands: RBX - RAX = 1.
    let reg_rm = instruction(0, true, 0, 0x0A, true, &[0x3B, 0xD8]);
    let result = execute(&reg_rm, regs);
    assert_eq!(result.rflags & STATUS, 0);
}

#[test]
fn all_promoted_ctest_forms_execute_including_group_one() {
    struct Case {
        name: &'static str,
        w: bool,
        pp: u8,
        tail: &'static [u8],
    }
    let cases = [
        Case {
            name: "84 /r",
            w: true,
            pp: 0,
            tail: &[0x84, 0xD8],
        },
        Case {
            name: "85 /r W16",
            w: false,
            pp: 1,
            tail: &[0x85, 0xD8],
        },
        Case {
            name: "85 /r W32",
            w: false,
            pp: 0,
            tail: &[0x85, 0xD8],
        },
        Case {
            name: "85 /r W64",
            w: true,
            pp: 0,
            tail: &[0x85, 0xD8],
        },
        Case {
            name: "F6 /0",
            w: true,
            pp: 0,
            tail: &[0xF6, 0xC0, 1],
        },
        Case {
            name: "F6 /1",
            w: false,
            pp: 0,
            tail: &[0xF6, 0xC8, 1],
        },
        Case {
            name: "F7 /0 W16",
            w: false,
            pp: 1,
            tail: &[0xF7, 0xC0, 1, 0],
        },
        Case {
            name: "F7 /1 W16",
            w: false,
            pp: 1,
            tail: &[0xF7, 0xC8, 1, 0],
        },
        Case {
            name: "F7 /0 W32",
            w: false,
            pp: 0,
            tail: &[0xF7, 0xC0, 1, 0, 0, 0],
        },
        Case {
            name: "F7 /1 W32",
            w: false,
            pp: 0,
            tail: &[0xF7, 0xC8, 1, 0, 0, 0],
        },
        Case {
            name: "F7 /0 W64",
            w: true,
            pp: 0,
            tail: &[0xF7, 0xC0, 1, 0, 0, 0],
        },
        Case {
            name: "F7 /1 W64",
            w: true,
            pp: 0,
            tail: &[0xF7, 0xC8, 1, 0, 0, 0],
        },
    ];
    for case in cases {
        let regs = Registers {
            rax: 0x10,
            rbx: 0x01,
            rflags: 0x2 | PRESERVED | STATUS,
            ..Registers::default()
        };
        let code = instruction(0, case.w, case.pp, 0x0A, true, case.tail);
        let result = execute(&code, regs);
        assert_eq!(result.rflags & STATUS, PF | ZF, "{}", case.name);
        assert_eq!(result.rflags & PRESERVED, PRESERVED, "{}", case.name);
    }
}

#[test]
fn false_scc_never_suppresses_memory_faults() {
    for tail in [
        &[0x39, 0x03][..],
        &[0x3B, 0x03][..],
        &[0x85, 0x03][..],
        &[0x83, 0x3B, 1][..],
        &[0xF7, 0x03, 1, 0, 0, 0][..],
        &[0xF7, 0x0B, 1, 0, 0, 0][..],
    ] {
        let code = instruction(0x0F, true, 0, 0x0B, true, tail);
        assert_precise_memory_fault(&code);
    }
}

#[test]
fn reserved_conditional_encodings_are_precise_ud() {
    let mut opcode82 = conditional_prefix(0, false, 0, 0, true).to_vec();
    opcode82.push(0x82);
    assert_precise_exception(&opcode82, 6, |_| {});

    for opcode in [0x38, 0x3A, 0x84] {
        for pp in 1..=3 {
            let code = instruction(0, true, pp, 0, true, &[opcode, 0x00]);
            assert_precise_exception(&code, 6, |_| {});
        }
    }
    for opcode in [0x39, 0x3B, 0x85] {
        for pp in 2..=3 {
            let code = instruction(0, true, pp, 0, true, &[opcode, 0x00]);
            assert_precise_exception(&code, 6, |_| {});
        }
    }

    for reserved_nibble in 1..=0x0F {
        for tail in [
            &[0x38, 0x00][..],
            &[0x39, 0x00][..],
            &[0x3A, 0x00][..],
            &[0x3B, 0x00][..],
            &[0x84, 0x00][..],
            &[0x85, 0x00][..],
        ] {
            let mut code = instruction(0, true, 0, 0, true, tail);
            code[3] |= reserved_nibble << 4;
            assert_precise_exception(&code, 6, |_| {});
        }
    }

    for opcode in [0x38, 0x39, 0x3A, 0x3B, 0x84, 0x85] {
        let code = instruction(0, opcode & 1 != 0, 0, 0, false, &[opcode, 0xC0]);
        assert_precise_exception(&code, 6, |_| {});
    }

    let grouped = [
        (0x80, 7),
        (0x81, 7),
        (0x83, 7),
        (0xF6, 0),
        (0xF6, 1),
        (0xF7, 0),
        (0xF7, 1),
    ];
    for reserved_nibble in 1..=0x0F {
        for (opcode, group) in grouped {
            let mut code = instruction(
                0,
                opcode != 0x80 && opcode != 0xF6,
                0,
                0,
                true,
                &[opcode, group << 3],
            );
            code[3] |= reserved_nibble << 4;
            assert_precise_exception(&code, 6, |_| {});
        }
    }

    for (opcode, group) in grouped {
        let code = instruction(
            0,
            opcode != 0x80 && opcode != 0xF6,
            0,
            0,
            false,
            &[opcode, 0xC0 | group << 3],
        );
        assert_precise_exception(&code, 6, |_| {});
    }

    for (opcode, group) in grouped {
        let first_invalid_pp = if matches!(opcode, 0x80 | 0xF6) { 1 } else { 2 };
        for invalid_pp in first_invalid_pp..=3 {
            let code = instruction(0, true, invalid_pp, 0, true, &[opcode, 0xC0 | group << 3]);
            assert_precise_exception(&code, 6, |_| {});
        }
    }

    for prefix in [0x66, 0xF2, 0xF3, 0xF0, 0x48] {
        let mut code = vec![prefix];
        code.extend_from_slice(&instruction(0, true, 0, 0x0A, true, &[0x39, 0xD8]));
        assert_precise_exception(&code, 6, |_| {});
    }
}

#[test]
fn conditional_memory_success_and_rip_relative_immediate_are_exact() {
    let mut regs = Registers {
        rax: 5,
        rbx: DATA_ADDR,
        rflags: 0x2 | PRESERVED,
        ..Registers::default()
    };
    let code = instruction(0, true, 0, 0x0A, true, &[0x3B, 0x03]);
    let (mut vcpu, memory) = setup_apx_vm(&code, Some(regs.clone()));
    write_mem_at_u64(&memory, DATA_ADDR, 5);
    let result = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(result.rflags & STATUS, PF | ZF);

    // F7 /1, [RIP+disp32], imm32. The operand address is based on the end of
    // the ten-byte instruction, not the start of its immediate.
    let data = CODE_ADDR + 10 + 0x20;
    let code = instruction(
        0,
        true,
        0,
        0x0A,
        true,
        &[0xF7, 0x0D, 0x20, 0, 0, 0, 1, 0, 0, 0],
    );
    regs.rflags = 0x2 | PRESERVED;
    let (mut vcpu, memory) = setup_apx_vm(&code, Some(regs));
    write_mem_at_u64(&memory, data, 0x10);
    let result = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(result.rflags & STATUS, PF | ZF);

    // U=0 is reserved only for ModR/M.Mod=3. For a SIB memory operand it is
    // logical X4=1, selecting the EGPR index R17 rather than RCX.
    let code = instruction(0, true, 0, 0x0A, false, &[0x85, 0x04, 0x0B]);
    let regs = Registers {
        rax: 1,
        rbx: DATA_ADDR,
        rcx: NONCANONICAL,
        r17: 0,
        rflags: 0x2 | PRESERVED,
        ..Registers::default()
    };
    let (mut vcpu, memory) = setup_apx_vm(&code, Some(regs));
    write_mem_at_u64(&memory, DATA_ADDR, 0);
    let result = run_until_hlt(&mut vcpu).unwrap();
    assert_eq!(result.rflags & STATUS, PF | ZF);
}

#[test]
fn egpr_operands_and_w64_sign_extended_immediate_are_exact() {
    let mut code = instruction(0, true, 0, 0x0A, true, &[0x39, 0xD1]);
    code[1] = 0xEC; // R4=1 and B4=1: operands are R17 and R18.
    let regs = Registers {
        r17: 5,
        r18: 5,
        rflags: 0x2 | PRESERVED | STATUS,
        ..Registers::default()
    };
    let result = execute(&code, regs);
    assert_eq!(result.rflags & STATUS, PF | ZF);
    assert_eq!(result.rflags & PRESERVED, PRESERVED);

    let code = instruction(
        0,
        true,
        0,
        0x0A,
        true,
        &[0x81, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF],
    );
    let regs = Registers {
        rax: u64::MAX,
        rflags: 0x2 | PRESERVED | STATUS,
        ..Registers::default()
    };
    let result = execute(&code, regs);
    assert_eq!(result.rflags & STATUS, PF | ZF);
    assert_eq!(result.rflags & PRESERVED, PRESERVED);
}
