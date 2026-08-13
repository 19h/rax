//! tests::alu tests

use super::*;
use crate::smir::lower::x86_64::*;

#[test]
fn test_emit_setcc() {
    let mut buf = CodeBuffer::new();
    {
        let mut emit = X86Emitter::new(&mut buf);
        emit.emit_setcc(X86Cond::E, PhysReg::Rax);
    }
    // SETE AL = 0F 94 C0
    assert_eq!(buf.data(), &[0x0F, 0x94, 0xC0]);
}

#[test]
fn test_immediate_encoding_tracks_operand_width() {
    for (width, expected) in [
        (OpWidth::W8, &[0x40, 0xF6, 0xC6, 0x7F][..]),
        (OpWidth::W16, &[0x66, 0xF7, 0xC6, 0xFF, 0x0F][..]),
        (OpWidth::W32, &[0xF7, 0xC6, 0xFF, 0x0F, 0x00, 0x00][..]),
        (
            OpWidth::W64,
            &[0x48, 0xF7, 0xC6, 0xFF, 0x0F, 0x00, 0x00][..],
        ),
    ] {
        let mut buffer = CodeBuffer::new();
        X86Emitter::new(&mut buffer).emit_test_ri(
            PhysReg::Rsi,
            if width == OpWidth::W8 { 0x7F } else { 0x0FFF },
            width,
        );
        assert_eq!(buffer.data(), expected, "{width:?}");
    }
}
#[test]
fn lower_mulx_hint_rejects_malformed_shapes() {
    let gpr = |reg| VReg::Arch(ArchReg::X86(reg));
    for (name, kind) in [
        (
            "non-RDX implicit source",
            OpKind::MulU {
                dst_lo: gpr(X86Reg::Rbx),
                dst_hi: Some(gpr(X86Reg::Rcx)),
                src1: gpr(X86Reg::Rax),
                src2: SrcOperand::Reg(gpr(X86Reg::Rsi)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        (
            "missing upper destination",
            OpKind::MulU {
                dst_lo: gpr(X86Reg::Rbx),
                dst_hi: None,
                src1: gpr(X86Reg::Rdx),
                src2: SrcOperand::Reg(gpr(X86Reg::Rsi)),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        (
            "immediate source",
            OpKind::MulU {
                dst_lo: gpr(X86Reg::Rbx),
                dst_hi: Some(gpr(X86Reg::Rcx)),
                src1: gpr(X86Reg::Rdx),
                src2: SrcOperand::Imm(7),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        (
            "16-bit width",
            OpKind::MulU {
                dst_lo: gpr(X86Reg::Rbx),
                dst_hi: Some(gpr(X86Reg::Rcx)),
                src1: gpr(X86Reg::Rdx),
                src2: SrcOperand::Reg(gpr(X86Reg::Rsi)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
        (
            "flag-writing form",
            OpKind::MulU {
                dst_lo: gpr(X86Reg::Rbx),
                dst_hi: Some(gpr(X86Reg::Rcx)),
                src1: gpr(X86Reg::Rdx),
                src2: SrcOperand::Reg(gpr(X86Reg::Rsi)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut func = builder.finish();
        func.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
        let mut lowerer = X86_64Lowerer::new();
        assert!(lowerer.lower_function(&func).is_err(), "{name}");
    }
}

#[test]
fn lower_byte_implicit_imul_uses_the_one_operand_ax_form() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));

    for flags in [FlagUpdate::All, FlagUpdate::None] {
        let code = lower_single_op(OpKind::MulS {
            dst_lo: rax,
            dst_hi: None,
            src1: rax,
            src2: SrcOperand::Reg(rcx),
            width: OpWidth::W8,
            flags,
        });
        assert!(
            code.windows(2).any(|bytes| bytes == [0xF6, 0xE9]),
            "implicit IMUL CL must produce AX with F6 /5: {code:02X?}"
        );
        assert!(
            !code.windows(2).any(|bytes| bytes == [0x0F, 0xAF]),
            "the nonexistent two-operand byte IMUL form must not be emitted: {code:02X?}"
        );
        assert_eq!(code.contains(&0x9C), flags == FlagUpdate::None);
        assert_eq!(code.contains(&0x9D), flags == FlagUpdate::None);
    }

    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::MulS {
            dst_lo: rbx,
            dst_hi: None,
            src1: rbx,
            src2: SrcOperand::Reg(rcx),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    assert!(
        matches!(
            lowerer.lower_function(&builder.finish()),
            Err(LowerError::InvalidOperand { .. })
        ),
        "non-implicit W8 MulS must fail closed"
    );
}

#[test]
fn lower_x86_count_honors_flag_contracts_and_rejects_malformed_ir() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));

    let popcnt_all = lower_single_op(OpKind::X86Count {
        dst: rax,
        src: rbx,
        width: OpWidth::W64,
        kind: X86CountKind::Popcnt,
        flags: FlagUpdate::All,
    });
    assert!(
        popcnt_all
            .windows(5)
            .any(|bytes| bytes == [0xF3, 0x48, 0x0F, 0xB8, 0xC3]),
        "full legacy POPCNT must lower directly: {popcnt_all:02X?}"
    );
    assert!(!popcnt_all.contains(&0x9C));
    assert!(!popcnt_all.contains(&0x9D));

    let popcnt_nf = lower_single_op(OpKind::X86Count {
        dst: rax,
        src: rbx,
        width: OpWidth::W64,
        kind: X86CountKind::Popcnt,
        flags: FlagUpdate::None,
    });
    assert!(
        popcnt_nf
            .windows(7)
            .any(|bytes| { bytes == [0x9C, 0xF3, 0x48, 0x0F, 0xB8, 0xC3, 0x9D] })
    );

    let tzcnt_flags = lower_single_op(OpKind::X86Count {
        dst: rax,
        src: rbx,
        width: OpWidth::W64,
        kind: X86CountKind::Tzcnt,
        flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
    });
    assert!(
        tzcnt_flags
            .windows(5)
            .any(|bytes| bytes == [0xF3, 0x48, 0x0F, 0xBC, 0xC3])
    );
    assert_eq!(
        tzcnt_flags.iter().filter(|byte| **byte == 0x9C).count(),
        2,
        "merged TZCNT must save old and new RFLAGS: {tzcnt_flags:02X?}"
    );
    assert_eq!(tzcnt_flags.iter().filter(|byte| **byte == 0x9D).count(), 1);
    assert!(
        tzcnt_flags.contains(&0x41),
        "merge mask must select exactly CF and ZF"
    );

    for malformed in [
        OpKind::X86Count {
            dst: rax,
            src: rbx,
            width: OpWidth::W64,
            kind: X86CountKind::Tzcnt,
            flags: FlagUpdate::All,
        },
        OpKind::X86Count {
            dst: rax,
            src: rbx,
            width: OpWidth::W8,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        },
    ] {
        assert!(matches!(
            lower_single_op_err(malformed),
            LowerError::InvalidOperand { .. }
        ));
    }
}
#[test]
fn lower_xchg_covers_partial_full_and_eax_self_exchange_encodings() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));

    for (width, expected) in [
        (OpWidth::W8, &[0x41, 0x86, 0xC0][..]),
        (OpWidth::W16, &[0x66, 0x41, 0x90][..]),
        (OpWidth::W32, &[0x41, 0x90][..]),
        (OpWidth::W64, &[0x49, 0x90][..]),
    ] {
        let code = lower_single_op(OpKind::Xchg {
            reg1: rax,
            reg2: r8,
            width,
        });
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{width:?} accumulator Xchg encoding: {code:02X?}"
        );
    }

    let eax_self = lower_single_op(OpKind::Xchg {
        reg1: rax,
        reg2: rax,
        width: OpWidth::W32,
    });
    assert!(
        eax_self.windows(2).any(|bytes| bytes == [0x87, 0xC0]),
        "EAX self-exchange must retain its zero-extending write: {eax_self:02X?}"
    );

    let al_self = lower_single_op(OpKind::Xchg {
        reg1: rax,
        reg2: rax,
        width: OpWidth::W8,
    });
    assert!(
        al_self.windows(2).any(|bytes| bytes == [0x86, 0xC0]),
        "AL self-exchange must retain its partial byte write: {al_self:02X?}"
    );

    assert!(matches!(
        lower_single_op_err(OpKind::Xchg {
            reg1: rax,
            reg2: r8,
            width: OpWidth::W128,
        }),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_bextr_bzhi_preserves_undefined_or_all_flags_by_update_mode() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));

    let flagful_bextr = lower_single_op(OpKind::Bextr {
        dst: rax,
        src: rdx,
        control: rcx,
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
    });
    assert!(
        flagful_bextr
            .windows(5)
            .any(|window| window == &[0xC4, 0xE2, 0xF0, 0xF7, 0xC2]),
        "flagful BEXTR should lower to native VEX BMI"
    );
    assert!(
        flagful_bextr.iter().filter(|byte| **byte == 0x9C).count() >= 2
            && flagful_bextr.contains(&0x9D),
        "flagful BEXTR must merge defined native flags with saved undefined flags"
    );

    let flagful_bzhi = lower_single_op(OpKind::Bzhi {
        dst: rax,
        src: rdx,
        index: rcx,
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(
            FlagSet::CF
                .union(FlagSet::ZF)
                .union(FlagSet::SF)
                .union(FlagSet::OF),
        ),
    });
    assert!(
        flagful_bzhi
            .windows(5)
            .any(|window| window == &[0xC4, 0xE2, 0xF0, 0xF5, 0xC2]),
        "flagful BZHI should lower to native VEX BMI"
    );
    assert!(
        flagful_bzhi.iter().filter(|byte| **byte == 0x9C).count() >= 2
            && flagful_bzhi.contains(&0x9D),
        "flagful BZHI must merge defined native flags with saved undefined flags"
    );

    let flagless_bextr = lower_single_op(OpKind::Bextr {
        dst: rax,
        src: rdx,
        control: rcx,
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    });
    assert!(
        flagless_bextr
            .windows(5)
            .any(|window| window == &[0xC4, 0xE2, 0xF0, 0xF7, 0xC2]),
        "flagless BEXTR should still lower to native VEX BMI"
    );
    assert!(
        flagless_bextr.contains(&0x9C) && flagless_bextr.contains(&0x9D),
        "flagless BEXTR must preserve flags around the native instruction"
    );

    let flagless_bzhi = lower_single_op(OpKind::Bzhi {
        dst: rax,
        src: rdx,
        index: rcx,
        width: OpWidth::W64,
        flags: FlagUpdate::None,
    });
    assert!(
        flagless_bzhi
            .windows(5)
            .any(|window| window == &[0xC4, 0xE2, 0xF0, 0xF5, 0xC2]),
        "flagless BZHI should still lower to native VEX BMI"
    );
    assert!(
        flagless_bzhi.contains(&0x9C) && flagless_bzhi.contains(&0x9D),
        "flagless BZHI must preserve flags around the native instruction"
    );

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
    let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
    let r31 = VReg::Arch(ArchReg::X86(X86Reg::R31));
    for (name, op, expected) in [
        (
            "state-backed BEXTR qword",
            OpKind::Bextr {
                dst: rsp,
                src: rbp,
                control: r16,
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
            },
            &[0xC4, 0xE2, 0xB8, 0xF7, 0xD7][..],
        ),
        (
            "state-backed BZHI dword",
            OpKind::Bzhi {
                dst: r31,
                src: rsp,
                index: rbp,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
            &[0xC4, 0xE2, 0x38, 0xF5, 0xD7][..],
        ),
        (
            "state-backed BEXTR all operands alias",
            OpKind::Bextr {
                dst: r16,
                src: r16,
                control: r16,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            &[0xC4, 0xE2, 0xB8, 0xF7, 0xD7][..],
        ),
    ] {
        let code = lower_single_op(op);
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name}: missing scratch BMI encoding {expected:02X?} in {code:02X?}"
        );
        assert!(
            code.contains(&0x9C) && code.contains(&0x9D),
            "{name}: flags must be saved and restored or merged"
        );
    }

    for (name, op) in [
        (
            "BEXTR unsupported width",
            OpKind::Bextr {
                dst: rax,
                src: rdx,
                control: rcx,
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
        (
            "BEXTR undefined flag request",
            OpKind::Bextr {
                dst: rax,
                src: rdx,
                control: rcx,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "BZHI incomplete flag request",
            OpKind::Bzhi {
                dst: rax,
                src: rdx,
                index: rcx,
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ),
        (
            "PDEP unsupported width",
            OpKind::Pdep {
                dst: rax,
                src: rdx,
                mask: rcx,
                width: OpWidth::W16,
            },
        ),
        (
            "state-backed BEXTR unsupported width",
            OpKind::Bextr {
                dst: r16,
                src: rsp,
                control: rbp,
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
        (
            "state-backed BZHI virtual source",
            OpKind::Bzhi {
                dst: r31,
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(7)),
                index: rbp,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
        (
            "state-backed BZHI incomplete flag request",
            OpKind::Bzhi {
                dst: r31,
                src: rsp,
                index: rbp,
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ),
    ] {
        assert!(
            matches!(
                lower_single_op_err(op),
                LowerError::UnsupportedOp { .. } | LowerError::InvalidOperand { .. }
            ),
            "{name}"
        );
    }
    assert!(matches!(
        lower_single_hinted_op_err(
            OpKind::Bextr {
                dst: r16,
                src: rsp,
                control: rbp,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_pdep_pext_preserves_flags_and_register_aliases() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));

    for (name, op, expected) in [
        (
            "PDEP distinct",
            OpKind::Pdep {
                dst: rax,
                src: rcx,
                mask: rdx,
                width: OpWidth::W64,
            },
            &[0xC4, 0xE2, 0xF3, 0xF5, 0xC2][..],
        ),
        (
            "PEXT distinct",
            OpKind::Pext {
                dst: rax,
                src: rcx,
                mask: rdx,
                width: OpWidth::W64,
            },
            &[0xC4, 0xE2, 0xF2, 0xF5, 0xC2][..],
        ),
        (
            "PDEP destination aliases source",
            OpKind::Pdep {
                dst: rcx,
                src: rcx,
                mask: rdx,
                width: OpWidth::W64,
            },
            &[0xC4, 0xE2, 0xF3, 0xF5, 0xCA][..],
        ),
        (
            "PEXT destination aliases mask",
            OpKind::Pext {
                dst: rdx,
                src: rcx,
                mask: rdx,
                width: OpWidth::W64,
            },
            &[0xC4, 0xE2, 0xF2, 0xF5, 0xD2][..],
        ),
    ] {
        let code = lower_single_op(op);
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name}: {code:02X?}"
        );
        assert!(
            code.contains(&0x9C) && code.contains(&0x9D),
            "{name} must preserve all incoming flags"
        );
    }

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
    let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
    let r31 = VReg::Arch(ArchReg::X86(X86Reg::R31));
    for (name, op, expected) in [
        (
            "state-backed PDEP qword",
            OpKind::Pdep {
                dst: rsp,
                src: rbp,
                mask: r16,
                width: OpWidth::W64,
            },
            &[0xC4, 0xC2, 0xC3, 0xF5, 0xD0][..],
        ),
        (
            "state-backed PEXT dword",
            OpKind::Pext {
                dst: r31,
                src: rsp,
                mask: rbp,
                width: OpWidth::W32,
            },
            &[0xC4, 0xC2, 0x42, 0xF5, 0xD0][..],
        ),
        (
            "state-backed PEXT all operands alias",
            OpKind::Pext {
                dst: rbp,
                src: rbp,
                mask: rbp,
                width: OpWidth::W64,
            },
            &[0xC4, 0xC2, 0xC2, 0xF5, 0xD0][..],
        ),
    ] {
        let code = lower_single_op(op);
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name}: missing scratch BMI2 {expected:02X?} in {code:02X?}"
        );
    }

    for malformed in [
        OpKind::Pdep {
            dst: r16,
            src: rsp,
            mask: rbp,
            width: OpWidth::W16,
        },
        OpKind::Pext {
            dst: r31,
            src: VReg::Virtual(crate::smir::ir::types::VirtualId(7)),
            mask: rbp,
            width: OpWidth::W64,
        },
    ] {
        assert!(matches!(
            lower_single_op_err(malformed),
            LowerError::InvalidOperand { .. }
        ));
    }
    assert!(matches!(
        lower_single_hinted_op_err(
            OpKind::Pdep {
                dst: r16,
                src: rsp,
                mask: rbp,
                width: OpWidth::W64,
            },
            X86OpHint::Mulx,
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_register_bit_tests_emit_all_widths_and_reject_malformed_shapes() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
    let r10 = VReg::Arch(ArchReg::X86(X86Reg::R10));

    let bt = lower_single_op(OpKind::Bt {
        src: rax,
        index: SrcOperand::Reg(rcx),
        width: OpWidth::W32,
    });
    assert!(
        bt.windows(4).any(|bytes| bytes == [0x9C, 0x0F, 0xA3, 0xC8]),
        "missing BT r32,r32 encoding: {bt:02X?}"
    );

    let btr = lower_single_op(OpKind::Btr {
        dst: r8,
        src: r8,
        index: SrcOperand::Imm(31),
        width: OpWidth::W32,
    });
    assert!(
        btr.windows(6)
            .any(|bytes| bytes == [0x9C, 0x41, 0x0F, 0xBA, 0xF0, 0x1F]),
        "missing BTR r32,imm8 encoding: {btr:02X?}"
    );

    let btc = lower_single_op(OpKind::Btc {
        dst: r9,
        src: r9,
        index: SrcOperand::Reg(r10),
        width: OpWidth::W16,
    });
    assert!(
        btc.windows(6)
            .any(|bytes| bytes == [0x9C, 0x66, 0x45, 0x0F, 0xBB, 0xD1]),
        "missing BTC r16,r16 encoding: {btc:02X?}"
    );

    for malformed in [
        OpKind::Bts {
            dst: rax,
            src: rcx,
            index: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
        OpKind::Bt {
            src: rax,
            index: SrcOperand::Imm(1),
            width: OpWidth::W8,
        },
        OpKind::Bt {
            src: VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            index: SrcOperand::Reg(VReg::Virtual(crate::smir::ir::types::VirtualId(0))),
            width: OpWidth::W64,
        },
    ] {
        assert!(
            matches!(
                lower_single_op_err(malformed),
                LowerError::InvalidOperand { .. }
                    | LowerError::InvalidRegister(_)
                    | LowerError::RegisterAllocationFailed { .. }
            ),
            "malformed register bit test must fail lowering"
        );
    }
}
#[test]
fn lower_crc32c_emits_every_source_width_and_rejects_malformed_shapes() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rsi = VReg::Arch(ArchReg::X86(X86Reg::Rsi));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
    let r10 = VReg::Arch(ArchReg::X86(X86Reg::R10));
    let r11 = VReg::Arch(ArchReg::X86(X86Reg::R11));
    let r12 = VReg::Arch(ArchReg::X86(X86Reg::R12));
    let r13 = VReg::Arch(ArchReg::X86(X86Reg::R13));

    for (name, op, expected) in [
        (
            "crc32 r32,r8",
            OpKind::Crc32C {
                dst: rax,
                crc: rax,
                data: rsi,
                data_width: OpWidth::W8,
            },
            &[0xF2, 0x40, 0x0F, 0x38, 0xF0, 0xC6][..],
        ),
        (
            "crc32 r32,r16",
            OpKind::Crc32C {
                dst: r8,
                crc: r8,
                data: r9,
                data_width: OpWidth::W16,
            },
            &[0xF2, 0x66, 0x45, 0x0F, 0x38, 0xF1, 0xC1][..],
        ),
        (
            "crc32 r32,r32",
            OpKind::Crc32C {
                dst: r10,
                crc: r10,
                data: r11,
                data_width: OpWidth::W32,
            },
            &[0xF2, 0x45, 0x0F, 0x38, 0xF1, 0xD3][..],
        ),
        (
            "crc32 r64,r64",
            OpKind::Crc32C {
                dst: r12,
                crc: r12,
                data: r13,
                data_width: OpWidth::W64,
            },
            &[0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xE5][..],
        ),
    ] {
        let code = lower_single_op(op);
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name}: missing {expected:02X?} in {code:02X?}"
        );
    }

    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
    for (name, op, expected) in [
        (
            "state-backed crc32 r32,r8",
            OpKind::Crc32C {
                dst: rbp,
                crc: rbp,
                data: rbp,
                data_width: OpWidth::W8,
            },
            &[0xF2, 0x40, 0x0F, 0x38, 0xF0, 0xD7][..],
        ),
        (
            "state-backed crc32 r32,r16",
            OpKind::Crc32C {
                dst: rsp,
                crc: rsp,
                data: rbp,
                data_width: OpWidth::W16,
            },
            &[0xF2, 0x66, 0x0F, 0x38, 0xF1, 0xD7][..],
        ),
        (
            "state-backed crc32 r32,r32",
            OpKind::Crc32C {
                dst: rbp,
                crc: rbp,
                data: rsp,
                data_width: OpWidth::W32,
            },
            &[0xF2, 0x0F, 0x38, 0xF1, 0xD7][..],
        ),
        (
            "state-backed crc32 r64,r64",
            OpKind::Crc32C {
                dst: rsp,
                crc: rsp,
                data: rbp,
                data_width: OpWidth::W64,
            },
            &[0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xD7][..],
        ),
    ] {
        let code = lower_single_op(op);
        assert!(
            code.windows(expected.len()).any(|bytes| bytes == expected),
            "{name}: missing scratch CRC32 {expected:02X?} in {code:02X?}"
        );
    }

    for malformed in [
        OpKind::Crc32C {
            dst: r8,
            crc: r9,
            data: r10,
            data_width: OpWidth::W64,
        },
        OpKind::Crc32C {
            dst: r8,
            crc: r8,
            data: r9,
            data_width: OpWidth::W128,
        },
        OpKind::Crc32C {
            dst: rsp,
            crc: rbp,
            data: r9,
            data_width: OpWidth::W32,
        },
        OpKind::Crc32C {
            dst: r8,
            crc: r8,
            data: VReg::Virtual(crate::smir::ir::types::VirtualId(99)),
            data_width: OpWidth::W16,
        },
    ] {
        let error = lower_single_op_err(malformed);
        assert!(
            matches!(
                error,
                LowerError::InvalidRegister(_)
                    | LowerError::RegisterAllocationFailed { .. }
                    | LowerError::InvalidOperand { .. }
                    | LowerError::UnsupportedOp { .. }
            ),
            "malformed CRC32 must fail lowering"
        );
    }

    assert!(matches!(
        lower_single_hinted_op_err(
            OpKind::Crc32C {
                dst: VReg::Arch(ArchReg::X86(X86Reg::R16)),
                crc: VReg::Arch(ArchReg::X86(X86Reg::R16)),
                data: rsp,
                data_width: OpWidth::W64,
            },
            X86OpHint::Mulx,
        ),
        LowerError::InvalidOperand { .. }
    ));
}
#[test]
fn lower_bit_scans_emit_zf_merge_and_validate_flag_contracts() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let r14 = VReg::Arch(ArchReg::X86(X86Reg::R14));
    let r15 = VReg::Arch(ArchReg::X86(X86Reg::R15));
    let zf_only = FlagUpdate::Specific(FlagSet::ZF);

    let bsf = lower_single_op(OpKind::Bsf {
        dst: r8,
        src: rax,
        width: OpWidth::W64,
        flags: zf_only,
    });
    let expected_bsf = [
        0x9C, 0x4C, 0x0F, 0xBC, 0xC0, 0x41, 0x50, 0x9C, 0x48, 0x83, 0x24, 0x24, 0x40, 0x41, 0x58,
        0x48, 0x83, 0x64, 0x24, 0x08, 0xBF, 0x4C, 0x09, 0x44, 0x24, 0x08, 0x41, 0x58, 0x9D,
    ];
    assert!(
        bsf.windows(expected_bsf.len())
            .any(|bytes| bytes == expected_bsf),
        "missing ZF-only BSF merge {expected_bsf:02X?}: {bsf:02X?}"
    );

    let bsr = lower_single_op(OpKind::Bsr {
        dst: r15,
        src: r14,
        width: OpWidth::W16,
        flags: zf_only,
    });
    assert!(
        bsr.windows(5)
            .any(|bytes| bytes == [0x66, 0x45, 0x0F, 0xBD, 0xFE]),
        "missing W16 extended-register BSR encoding: {bsr:02X?}"
    );

    let preserving = lower_single_op(OpKind::Bsf {
        dst: r8,
        src: rax,
        width: OpWidth::W32,
        flags: FlagUpdate::None,
    });
    assert!(
        preserving
            .windows(6)
            .any(|bytes| bytes == [0x9C, 0x44, 0x0F, 0xBC, 0xC0, 0x9D]),
        "flag-suppressed BSF must be wrapped by PUSHFQ/POPFQ: {preserving:02X?}"
    );

    let err = lower_single_op_err(OpKind::Bsr {
        dst: r8,
        src: rax,
        width: OpWidth::W64,
        flags: FlagUpdate::Specific(FlagSet::CF),
    });
    assert!(matches!(err, LowerError::InvalidOperand { .. }), "{err:?}");
}
