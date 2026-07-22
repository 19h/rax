//! Intel APX PUSH2/POP2 strict-lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_apx_push2_uses_llvm_encoding_and_preserves_source_order() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `push2 %rax, %rbx` as EVEX MAP4 FF /6. The ModRM B
    // operand (RAX) occupies the lower final stack slot.
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x64, 0x18, 0xFF, 0xF0], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_eq!(result.ops.len(), 5);

    let tmp1 = match result.ops[0].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert!(dst.is_virtual());
            assert_eq!(src, x86_gpr(0));
            dst
        }
        ref other => panic!("expected source capture for PUSH2 operand 1, got {other:?}"),
    };
    let tmp2 = match result.ops[1].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert!(dst.is_virtual());
            assert_eq!(src, x86_gpr(3));
            dst
        }
        ref other => panic!("expected source capture for PUSH2 operand 2, got {other:?}"),
    };
    match result.ops[2].kind {
        OpKind::Sub {
            dst,
            src1,
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(dst, x86_gpr(4));
            assert_eq!(src1, x86_gpr(4));
        }
        ref other => panic!("expected PUSH2 stack decrement, got {other:?}"),
    }
    match &result.ops[3].kind {
        OpKind::Store {
            src,
            addr: Address::Direct(base),
            width: MemWidth::B8,
        } => {
            assert_eq!(*src, tmp1);
            assert_eq!(*base, x86_gpr(4));
        }
        other => panic!("expected first PUSH2 store, got {other:?}"),
    }
    match &result.ops[4].kind {
        OpKind::Store {
            src,
            addr,
            width: MemWidth::B8,
        } => {
            assert_eq!(*src, tmp2);
            assert_eq!(*addr, Address::base_off(x86_gpr(4), 8));
        }
        other => panic!("expected second PUSH2 store, got {other:?}"),
    }
}
#[test]
fn lift_apx_pop2_uses_llvm_encoding_and_writes_after_rsp_increment() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `pop2 %rax, %rbx` as EVEX MAP4 8F. Intel's V operand
    // (RBX) receives [RSP], while the ModRM B operand (RAX) receives
    // [RSP+8].
    let result = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x64, 0x18, 0x8F, 0xC0], &mut ctx)
        .unwrap();
    assert_eq!(result.bytes_consumed, 6);
    assert_eq!(result.ops.len(), 5);

    let tmp1 = match &result.ops[0].kind {
        OpKind::Load {
            dst,
            addr: Address::Direct(base),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(4));
            *dst
        }
        other => panic!("expected first POP2 load, got {other:?}"),
    };
    let tmp2 = match &result.ops[1].kind {
        OpKind::Load {
            dst,
            addr,
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*addr, Address::base_off(x86_gpr(4), 8));
            *dst
        }
        other => panic!("expected second POP2 load, got {other:?}"),
    };
    match result.ops[2].kind {
        OpKind::Add {
            dst,
            src1,
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } => {
            assert_eq!(dst, x86_gpr(4));
            assert_eq!(src1, x86_gpr(4));
        }
        ref other => panic!("expected POP2 stack increment, got {other:?}"),
    }
    match result.ops[3].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(dst, x86_gpr(3));
            assert_eq!(src, tmp1);
        }
        ref other => panic!("expected POP2 first destination write, got {other:?}"),
    }
    match result.ops[4].kind {
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W64,
        } => {
            assert_eq!(dst, x86_gpr(0));
            assert_eq!(src, tmp2);
        }
        ref other => panic!("expected POP2 second destination write, got {other:?}"),
    }
}
#[test]
fn lift_apx_push2_pop2_decode_egprs_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `push2 %r16, %rcx`.
    let push = lifter
        .lift_insn(0x1000, &[0x62, 0xFC, 0x74, 0x18, 0xFF, 0xF0], &mut ctx)
        .unwrap();
    match push.ops[0].kind {
        OpKind::Mov {
            src: SrcOperand::Reg(src),
            ..
        } => assert_eq!(src, x86_gpr(16)),
        ref other => panic!("expected PUSH2 first EGPR operand, got {other:?}"),
    }
    match push.ops[1].kind {
        OpKind::Mov {
            src: SrcOperand::Reg(src),
            ..
        } => assert_eq!(src, x86_gpr(1)),
        ref other => panic!("expected PUSH2 second operand, got {other:?}"),
    }

    // LLVM 23: `pop2 %r20, %rbp`. V (RBP) receives the low qword and B
    // (R20) receives the high qword.
    let pop = lifter
        .lift_insn(0x2000, &[0x62, 0xFC, 0x54, 0x18, 0x8F, 0xC4], &mut ctx)
        .unwrap();
    match pop.ops[3].kind {
        OpKind::Mov { dst, .. } => assert_eq!(dst, x86_gpr(5)),
        ref other => panic!("expected POP2 low-slot destination, got {other:?}"),
    }
    match pop.ops[4].kind {
        OpKind::Mov { dst, .. } => assert_eq!(dst, x86_gpr(20)),
        ref other => panic!("expected POP2 high-slot destination, got {other:?}"),
    }
}
#[test]
fn lift_apx_push2_pop2_reject_invalid_forms_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // Intel APX revision 5.0: memory forms, reserved ModR/M groups, either RSP
    // operand, duplicate POP2 destinations, and reserved EVEX payload fields
    // are #UD encodings. Each one must terminate strict lifting at the complete
    // prefix/opcode/ModR/M frontier without entering direct fallback.
    for bytes in [
        &[0x62, 0xF4, 0x6C, 0x18, 0xFF, 0x30][..],
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0x00][..],
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC8][..],
        &[0x62, 0xF4, 0x5C, 0x18, 0xFF, 0xF0][..],
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC4][..],
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0xC0][..],
        &[0x62, 0xF4, 0x64, 0x08, 0xFF, 0xF0][..],
        &[0x62, 0xF4, 0x64, 0x1C, 0xFF, 0xF0][..],
        &[0x62, 0xF4, 0x64, 0x98, 0xFF, 0xF0][..],
        &[0x62, 0xF4, 0x64, 0x38, 0xFF, 0xF0][..],
        &[0x62, 0xF4, 0x64, 0x19, 0xFF, 0xF0][..],
        &[0x62, 0xF4, 0x65, 0x18, 0xFF, 0xF0][..],
        &[0x62, 0xF4, 0x60, 0x18, 0xFF, 0xF0][..],
    ] {
        let result = lifter
            .lift_insn(0x1000, bytes, &mut ctx)
            .expect("reserved PUSH2/POP2 form must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, 6);
    }
}

#[test]
fn lift_apx_reserved_group45_cells_are_exact_ud_frontiers() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    for mode in 0..=3 {
        for group in 1..=7 {
            let modrm = (mode << 6) | (group << 3) | 3;
            let result = lifter
                .lift_insn(0x1000, &[0x62, 0xF4, 0x7C, 0x18, 0x8F, modrm], &mut ctx)
                .expect("reserved MAP4 8F group must strictly lift to #UD");
            assert_invalid_opcode_trap(&result, 6);
        }

        for group in 2..=7 {
            let modrm = (mode << 6) | (group << 3) | 3;
            let result = lifter
                .lift_insn(0x1000, &[0x62, 0xF4, 0x64, 0x18, 0xFE, modrm], &mut ctx)
                .expect("reserved MAP4 FE group must strictly lift to #UD");
            assert_invalid_opcode_trap(&result, 6);
        }

        for group in [2, 3, 4, 5, 7] {
            let modrm = (mode << 6) | (group << 3) | 3;
            let result = lifter
                .lift_insn(0x1000, &[0x62, 0xF4, 0x64, 0x18, 0xFF, modrm], &mut ctx)
                .expect("reserved MAP4 FF group must strictly lift to #UD");
            assert_invalid_opcode_trap(&result, 6);
        }
    }
}

#[test]
fn lift_apx_push2_pop2_memory_ud_does_not_decode_an_address() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // Each ModR/M byte would require a SIB and/or displacement if it described
    // a memory operand. PP2 rejects ModRM.Mod != 3 before those bytes exist.
    for bytes in [
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0x04][..],
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F, 0x45][..],
        &[0x62, 0xF4, 0x64, 0x18, 0xFF, 0x34][..],
        &[0x62, 0xF4, 0x64, 0x18, 0xFF, 0xB5][..],
    ] {
        let result = lifter
            .lift_insn(0x1000, bytes, &mut ctx)
            .expect("PP2 ModR/M is sufficient to determine #UD");
        assert_invalid_opcode_trap(&result, 6);
    }

    for bytes in [
        &[0x62, 0xF4, 0x7C, 0x18, 0x8F][..],
        &[0x62, 0xF4, 0x64, 0x18, 0xFF][..],
    ] {
        assert!(matches!(
            lifter.lift_insn(0x1000, bytes, &mut ctx),
            Err(LiftError::Incomplete {
                have: 5,
                need: 6,
                ..
            })
        ));
    }
}
