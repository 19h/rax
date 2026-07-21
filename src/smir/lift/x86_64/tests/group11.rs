//! Legacy Group 11 (`C6`/`C7`) encoding and strict-frontier tests.

use super::*;

fn assert_ud(bytes: &[u8], expected_len: usize) {
    let result = lift_single(bytes).expect("reserved Group 11 encoding must strictly lift");
    assert_eq!(result.bytes_consumed, expected_len, "{bytes:02X?}");
    assert!(result.ops.is_empty(), "{bytes:02X?}");
    assert!(result.branch_targets.is_empty(), "{bytes:02X?}");
    assert!(matches!(
        result.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode,
        }
    ));
}

#[test]
fn reserved_group11_selectors_strictly_lift_to_ud_before_operand_decode() {
    let mut cases = 0;
    for opcode in [0xC6, 0xC7] {
        for group in 1..=7 {
            for mod_bits in 0..=3 {
                for rm in 0..=7 {
                    let raw_modrm = (mod_bits << 6) | (group << 3) | rm;
                    if raw_modrm == 0xF8 {
                        continue;
                    }

                    // Apparent SIB, displacement, and immediate fields are
                    // intentionally absent. The reserved opcode extension is
                    // sufficient to determine #UD without operand decoding.
                    assert_ud(&[opcode, raw_modrm], 2);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 446);
}

#[test]
fn reserved_group11_selector_is_never_downgraded_to_a_nop() {
    let mut lifter = X86_64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, &[0xC7, 0xC8], &mut ctx)
        .expect("non-strict lifting must preserve architecturally required #UD");
    assert!(result.ops.is_empty());
    assert!(matches!(
        result.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode,
        }
    ));
}

#[test]
fn reserved_group11_selectors_ignore_non_lock_legacy_and_register_extension_prefixes() {
    for bytes in [
        &[0x66, 0xC6, 0xC8][..],
        &[0x67, 0xC7, 0xD0],
        &[0xF2, 0xC6, 0xD8],
        &[0xF3, 0xC7, 0xE0],
        &[0x64, 0xC6, 0xE8],
        &[0x4C, 0xC7, 0xF0],
        &[0xD5, 0x7F, 0xC6, 0xF1],
    ] {
        assert_ud(bytes, bytes.len());
    }
}

#[test]
fn group11_valid_mov_and_fixed_rtm_aliases_remain_distinct() {
    let mov8 = lift_single(&[0xC6, 0xC0, 0x7F]).expect("C6 /0 MOV");
    assert_eq!(mov8.bytes_consumed, 3);
    assert!(matches!(
        mov8.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm(0x7F),
                width: OpWidth::W8,
            },
            ..
        }]
    ));

    let mov32 = lift_single(&[0xC7, 0xC0, 0x78, 0x56, 0x34, 0x12]).expect("C7 /0 MOV");
    assert_eq!(mov32.bytes_consumed, 6);
    assert!(matches!(
        mov32.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm(0x1234_5678),
                width: OpWidth::W32,
            },
            ..
        }]
    ));

    let xabort = lift_single(&[0xC6, 0xF8, 0x42]).expect("C6 F8 XABORT");
    assert_eq!(xabort.bytes_consumed, 3);
    assert!(xabort.ops.is_empty());
    assert!(matches!(xabort.control_flow, ControlFlow::Fallthrough));

    let xbegin = lift_single(&[0xC7, 0xF8, 0, 0, 0, 0]).expect("C7 F8 XBEGIN");
    assert_eq!(xbegin.bytes_consumed, 6);
    assert!(matches!(
        xbegin.control_flow,
        ControlFlow::Branch { target: 0x1006 }
    ));
}

#[test]
fn group11_valid_forms_still_require_operands_and_reject_lock() {
    for bytes in [&[0xC6, 0x04][..], &[0xC7, 0x05]] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "valid /0 must decode its address and immediate: {bytes:02X?}"
        );
    }

    for bytes in [
        &[0xF0, 0xC6, 0xC0, 0x01][..],
        &[0xF0, 0xC7, 0xC0, 0, 0, 0, 0],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "LOCK Group 11 must be invalid: {bytes:02X?}"
        );
    }
}

#[test]
fn reserved_group11_selector_terminates_a_strict_block_as_ud() {
    let mem = TestMemory::new(
        0x1000,
        vec![
            0xB8, 0x78, 0x56, 0x34, 0x12, // MOV EAX,0x12345678
            0xC6, 0xC8, // reserved Group 11 /1
        ],
    );
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    let block = lifter
        .lift_block(0x1000, &mem, &mut ctx)
        .expect("reserved Group 11 selector must not create an unsupported frontier");

    assert!(matches!(
        block.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                src: SrcOperand::Imm(0x1234_5678),
                width: OpWidth::W32,
            },
            ..
        }]
    ));
    assert!(matches!(
        block.terminator,
        Terminator::Trap {
            kind: TrapKind::InvalidOpcode,
        }
    ));
}
