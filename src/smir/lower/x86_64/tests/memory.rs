//! tests::memory tests

use super::*;
use crate::smir::lower::x86_64::*;

    #[test]
    fn lower_x86_minmax_emits_source2_selecting_native_opcodes() {
        let xmm0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        let xmm2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));

        for (name, kind, expected) in [
            (
                "MINSS",
                OpKind::VX86MinMax {
                    dst: xmm0,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::F32,
                    lanes: 1,
                    min: true,
                },
                &[0xF3, 0x0F, 0x5D, 0xC2][..],
            ),
            (
                "MAXPD",
                OpKind::VX86MinMax {
                    dst: xmm0,
                    src1: xmm1,
                    src2: xmm2,
                    elem: VecElementType::F64,
                    lanes: 2,
                    min: false,
                },
                &[0x66, 0x0F, 0x5F, 0xC2][..],
            ),
        ] {
            let code = lower_single_op(kind);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name}: missing native opcode in {code:02X?}"
            );
            assert!(
                code.windows(4)
                    .any(|window| window == [0xF3, 0x0F, 0x6F, 0xC1]),
                "{name}: source1 copy missing in {code:02X?}"
            );
        }
    }
    #[test]
    fn lower_x86_fp_compare_emits_comi_ucomi_native_opcodes() {
        let xmm0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        for (name, kind, expected) in [
            (
                "UCOMISS",
                OpKind::X86FpCompare {
                    src1: xmm0,
                    src2: xmm1,
                    elem: VecElementType::F32,
                    signaling: false,
                },
                &[0x0F, 0x2E, 0xC1][..],
            ),
            (
                "COMISD",
                OpKind::X86FpCompare {
                    src1: xmm0,
                    src2: xmm1,
                    elem: VecElementType::F64,
                    signaling: true,
                },
                &[0x66, 0x0F, 0x2F, 0xC1][..],
            ),
        ] {
            let code = lower_single_op(kind);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name}: missing native opcode in {code:02X?}"
            );
        }
    }
    #[test]
    fn lower_x86_fp_to_int_emits_width_and_rounding_native_opcodes() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        for (name, kind, expected) in [
            (
                "CVTSS2SI eax,xmm1",
                OpKind::X86FpToInt {
                    dst: rax,
                    src: xmm1,
                    elem: VecElementType::F32,
                    int_width: OpWidth::W32,
                    signed: true,
                    truncate: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                },
                &[0xF3, 0x0F, 0x2D, 0xC1][..],
            ),
            (
                "CVTTSD2SI rax,xmm1",
                OpKind::X86FpToInt {
                    dst: rax,
                    src: xmm1,
                    elem: VecElementType::F64,
                    int_width: OpWidth::W64,
                    signed: true,
                    truncate: true,
                    round: FpRoundMode::RoundTowardZero,
                    suppress_exceptions: false,
                },
                &[0xF2, 0x48, 0x0F, 0x2C, 0xC1][..],
            ),
        ] {
            let code = lower_single_op(kind);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name}: missing native opcode in {code:02X?}"
            );
        }

        let fp16_er = lower_single_op_err(OpKind::X86FpToInt {
            dst: rax,
            src: xmm1,
            elem: VecElementType::F16,
            int_width: OpWidth::W32,
            signed: true,
            truncate: false,
            round: FpRoundMode::RoundDown,
            suppress_exceptions: true,
        });
        assert!(matches!(fp16_er, LowerError::UnsupportedOp { .. }));

        let unsigned = lower_single_op_err(OpKind::X86FpToInt {
            dst: rax,
            src: xmm1,
            elem: VecElementType::F32,
            int_width: OpWidth::W32,
            signed: false,
            truncate: false,
            round: FpRoundMode::Dynamic,
            suppress_exceptions: false,
        });
        assert!(matches!(unsigned, LowerError::UnsupportedOp { .. }));
    }
    #[test]
    fn lower_x86_int_to_fp_emits_source_width_native_opcodes() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        for (name, kind, expected) in [
            (
                "CVTSI2SS xmm1,eax",
                OpKind::X86IntToFp {
                    dst: xmm1,
                    merge: xmm1,
                    src: rax,
                    elem: VecElementType::F32,
                    int_width: OpWidth::W32,
                    signed: true,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: false,
                },
                &[0xF3, 0x0F, 0x2A, 0xC8][..],
            ),
            (
                "CVTSI2SD xmm1,rax",
                OpKind::X86IntToFp {
                    dst: xmm1,
                    merge: xmm1,
                    src: rax,
                    elem: VecElementType::F64,
                    int_width: OpWidth::W64,
                    signed: true,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: false,
                },
                &[0xF2, 0x48, 0x0F, 0x2A, 0xC8][..],
            ),
        ] {
            let code = lower_single_op(kind);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name}: missing native opcode in {code:02X?}"
            );
        }

        for kind in [
            OpKind::X86IntToFp {
                dst: xmm1,
                merge: xmm1,
                src: rax,
                elem: VecElementType::F32,
                int_width: OpWidth::W64,
                signed: false,
                round: FpRoundMode::Dynamic,
                suppress_exceptions: false,
                zero_upper: false,
            },
            OpKind::X86IntToFp {
                dst: xmm1,
                merge: xmm1,
                src: rax,
                elem: VecElementType::F16,
                int_width: OpWidth::W64,
                signed: true,
                round: FpRoundMode::RoundDown,
                suppress_exceptions: true,
                zero_upper: false,
            },
        ] {
            assert!(matches!(
                lower_single_op_err(kind),
                LowerError::UnsupportedOp { .. }
            ));
        }
    }
    #[test]
    fn lower_x86_scalar_fp_convert_emits_native_opcodes() {
        let xmm0 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
        let xmm1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
        for (name, kind, expected) in [
            (
                "CVTSS2SD xmm0,xmm1",
                OpKind::X86FpConvert {
                    dst: xmm0,
                    merge: xmm0,
                    src: xmm1,
                    mask: None,
                    from: VecElementType::F32,
                    to: VecElementType::F64,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: false,
                },
                &[0xF3, 0x0F, 0x5A, 0xC1][..],
            ),
            (
                "CVTSD2SS xmm0,xmm1",
                OpKind::X86FpConvert {
                    dst: xmm0,
                    merge: xmm0,
                    src: xmm1,
                    mask: None,
                    from: VecElementType::F64,
                    to: VecElementType::F32,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: false,
                },
                &[0xF2, 0x0F, 0x5A, 0xC1][..],
            ),
        ] {
            let code = lower_single_op(kind);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name}: missing native opcode in {code:02X?}"
            );
        }
    }
    #[test]
    fn test_emit_push_pop() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_push(PhysReg::Rbp);
            emit.emit_pop(PhysReg::Rbp);
        }
        // PUSH RBP = 55, POP RBP = 5D
        assert_eq!(buf.data(), &[0x55, 0x5D]);
    }
    #[test]
    fn emit_scalar_count_memory_encodes_stack_sources_and_partial_widths() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_x86_count_rm(
                X86CountKind::Popcnt,
                PhysReg::R8,
                PhysReg::Rsp,
                0,
                OpWidth::W16,
            );
            emit.emit_x86_count_rm(
                X86CountKind::Lzcnt,
                PhysReg::R15,
                PhysReg::Rsp,
                8,
                OpWidth::W64,
            );
        }
        assert_eq!(
            buf.data(),
            &[
                0xF3, 0x66, 0x44, 0x0F, 0xB8, 0x04, 0x24, // popcnt r8w,[rsp]
                0xF3, 0x4C, 0x0F, 0xBD, 0x7C, 0x24, 0x08, // lzcnt r15,[rsp+8]
            ]
        );
    }
    #[test]
    fn emit_bit_scan_memory_encodes_stack_sources_and_partial_widths() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_bit_scan_rm(false, PhysReg::R8, PhysReg::Rsp, 0, OpWidth::W16);
            emit.emit_bit_scan_rm(true, PhysReg::R15, PhysReg::Rsp, 8, OpWidth::W64);
        }
        assert_eq!(
            buf.data(),
            &[
                0x66, 0x44, 0x0F, 0xBC, 0x04, 0x24, // bsf r8w,[rsp]
                0x4C, 0x0F, 0xBD, 0x7C, 0x24, 0x08, // bsr r15,[rsp+8]
            ]
        );
    }
    #[test]
    fn emit_bit_test_memory_immediate_encodes_all_actions_and_widths() {
        let mut buf = CodeBuffer::new();
        {
            let mut emit = X86Emitter::new(&mut buf);
            emit.emit_bit_test_mi_disp(BitTestRegOp::Test, PhysReg::Rsp, 0, 15, OpWidth::W16);
            emit.emit_bit_test_mi_disp(BitTestRegOp::Test, PhysReg::Rsp, 8, 63, OpWidth::W64);
            emit.emit_bit_test_mi_disp(BitTestRegOp::Set, PhysReg::Rsp, 0, 15, OpWidth::W16);
            emit.emit_bit_test_mi_disp(BitTestRegOp::Reset, PhysReg::Rsp, 8, 7, OpWidth::W32);
            emit.emit_bit_test_mi_disp(
                BitTestRegOp::Complement,
                PhysReg::Rsp,
                16,
                63,
                OpWidth::W64,
            );
        }
        assert_eq!(
            buf.data(),
            &[
                0x66, 0x0F, 0xBA, 0x24, 0x24, 0x0F, // bt word [rsp],15
                0x48, 0x0F, 0xBA, 0x64, 0x24, 0x08, 0x3F, // bt qword [rsp+8],63
                0x66, 0x0F, 0xBA, 0x2C, 0x24, 0x0F, // bts word [rsp],15
                0x0F, 0xBA, 0x74, 0x24, 0x08, 0x07, // btr dword [rsp+8],7
                0x48, 0x0F, 0xBA, 0x7C, 0x24, 0x10, 0x3F, // btc qword [rsp+16],63
            ]
        );
    }
    #[test]
    fn lower_nf_implicit_group3_rejects_native_stack_operands() {
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));

        for (name, kind) in [
            (
                "mul rsp",
                OpKind::MulU {
                    dst_lo: rax,
                    dst_hi: Some(rdx),
                    src1: rax,
                    src2: SrcOperand::Reg(rsp),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
            (
                "div rsp",
                OpKind::DivU {
                    quot: rax,
                    rem: Some(rdx),
                    src1: rax,
                    src2: SrcOperand::Reg(rsp),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();
            let mut lowerer = X86_64Lowerer::new();
            assert!(lowerer.lower_function(&func).is_err(), "{name}");
        }
    }
    #[test]
    fn lower_mulx_hint_emits_native_bmi2_with_aliasing() {
        let gpr = |reg| VReg::Arch(ArchReg::X86(reg));

        for (name, dst_lo, dst_hi, src2, width, expected) in [
            (
                "64-bit",
                X86Reg::Rbx,
                X86Reg::Rcx,
                X86Reg::Rax,
                OpWidth::W64,
                &[0xC4, 0xE2, 0xE3, 0xF6, 0xC8][..],
            ),
            (
                "32-bit extended",
                X86Reg::R8,
                X86Reg::R9,
                X86Reg::R10,
                OpWidth::W32,
                &[0xC4, 0x42, 0x3B, 0xF6, 0xCA][..],
            ),
            (
                "same destination keeps upper half",
                X86Reg::Rcx,
                X86Reg::Rcx,
                X86Reg::Rax,
                OpWidth::W64,
                &[0xC4, 0xE2, 0xF3, 0xF6, 0xC8][..],
            ),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::MulU {
                    dst_lo: gpr(dst_lo),
                    dst_hi: Some(gpr(dst_hi)),
                    src1: gpr(X86Reg::Rdx),
                    src2: SrcOperand::Reg(gpr(src2)),
                    width,
                    flags: FlagUpdate::None,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut func = builder.finish();
            func.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);

            let mut lowerer = X86_64Lowerer::new();
            let result = lowerer.lower_function(&func).expect(name);
            assert!(result.relocations.is_empty(), "{name}");
            let code = lowerer.finalize().expect(name);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{name}: missing MULX {expected:02X?} in {code:02X?}"
            );
        }
    }
    #[test]
    fn lower_bswap_covers_native_widths_and_rejects_silent_noops() {
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));

        let word = lower_single_op(OpKind::Bswap {
            dst: r8,
            src: r8,
            width: OpWidth::W16,
        });
        assert!(word.contains(&0x9C), "word swap must preserve RFLAGS");
        assert!(word.contains(&0x9D), "word swap must restore RFLAGS");
        assert!(
            word.windows(5)
                .any(|bytes| bytes == [0x66, 0x41, 0xC1, 0xC0, 0x08]),
            "word swap must lower as ROL r8w,8: {word:02X?}"
        );

        for (width, expected) in [
            (OpWidth::W32, &[0x41, 0x0F, 0xC8][..]),
            (OpWidth::W64, &[0x49, 0x0F, 0xC8][..]),
        ] {
            let code = lower_single_op(OpKind::Bswap {
                dst: r8,
                src: r8,
                width,
            });
            assert!(
                code.windows(expected.len()).any(|bytes| bytes == expected),
                "{width:?} Bswap encoding: {code:02X?}"
            );
        }

        assert!(matches!(
            lower_single_op_err(OpKind::Bswap {
                dst: r8,
                src: r8,
                width: OpWidth::W8,
            }),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[test]
    fn lower_x86_bls_emits_native_alias_safe_encodings_and_exact_flags() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);

        let flagful = lower_single_op(OpKind::X86Bls {
            dst: rax,
            src: rbx,
            width: OpWidth::W64,
            kind: X86BlsKind::Blsr,
            flags: FlagUpdate::Specific(defined),
        });
        assert!(
            flagful
                .windows(5)
                .any(|window| window == [0xC4, 0xE2, 0xF8, 0xF3, 0xCB]),
            "BLSR encoding: {flagful:02X?}"
        );
        assert!(
            flagful.iter().filter(|byte| **byte == 0x9C).count() >= 2 && flagful.contains(&0x9D),
            "flagful BLSR must merge only defined status flags"
        );

        let nf_alias = lower_single_op(OpKind::X86Bls {
            dst: r8,
            src: r8,
            width: OpWidth::W32,
            kind: X86BlsKind::Blsi,
            flags: FlagUpdate::None,
        });
        assert!(
            nf_alias
                .windows(5)
                .any(|window| window == [0xC4, 0xC2, 0x38, 0xF3, 0xD8]),
            "aliased APX NF BLSI lowering: {nf_alias:02X?}"
        );
        assert!(
            nf_alias.contains(&0x9C) && nf_alias.contains(&0x9D),
            "APX NF BLSI must preserve every incoming flag"
        );

        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
        let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
        let r31 = VReg::Arch(ArchReg::X86(X86Reg::R31));
        for (name, op, expected) in [
            (
                "state-backed BLSR qword",
                OpKind::X86Bls {
                    dst: rsp,
                    src: rbp,
                    width: OpWidth::W64,
                    kind: X86BlsKind::Blsr,
                    flags: FlagUpdate::Specific(defined),
                },
                &[0xC4, 0xE2, 0xE8, 0xF3, 0xCF][..],
            ),
            (
                "state-backed BLSMSK dword",
                OpKind::X86Bls {
                    dst: r31,
                    src: rsp,
                    width: OpWidth::W32,
                    kind: X86BlsKind::Blsmsk,
                    flags: FlagUpdate::None,
                },
                &[0xC4, 0xE2, 0x68, 0xF3, 0xD7][..],
            ),
            (
                "state-backed BLSI all operands alias",
                OpKind::X86Bls {
                    dst: r16,
                    src: r16,
                    width: OpWidth::W64,
                    kind: X86BlsKind::Blsi,
                    flags: FlagUpdate::None,
                },
                &[0xC4, 0xE2, 0xE8, 0xF3, 0xDF][..],
            ),
        ] {
            let code = lower_single_op(op);
            assert!(
                code.windows(expected.len()).any(|bytes| bytes == expected),
                "{name}: missing scratch BMI1 {expected:02X?} in {code:02X?}"
            );
            assert!(
                code.contains(&0x9C) && code.contains(&0x9D),
                "{name}: flags must be saved and restored or merged"
            );
        }

        for malformed in [
            OpKind::X86Bls {
                dst: rax,
                src: rbx,
                width: OpWidth::W16,
                kind: X86BlsKind::Blsmsk,
                flags: FlagUpdate::Specific(defined),
            },
            OpKind::X86Bls {
                dst: rax,
                src: rbx,
                width: OpWidth::W64,
                kind: X86BlsKind::Blsr,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
            OpKind::X86Bls {
                dst: rax,
                src: rbx,
                width: OpWidth::W64,
                kind: X86BlsKind::Blsi,
                flags: FlagUpdate::All,
            },
            OpKind::X86Bls {
                dst: r16,
                src: rsp,
                width: OpWidth::W16,
                kind: X86BlsKind::Blsr,
                flags: FlagUpdate::None,
            },
            OpKind::X86Bls {
                dst: r31,
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(7)),
                width: OpWidth::W64,
                kind: X86BlsKind::Blsi,
                flags: FlagUpdate::None,
            },
            OpKind::X86Bls {
                dst: r31,
                src: rbp,
                width: OpWidth::W64,
                kind: X86BlsKind::Blsmsk,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ] {
            assert!(matches!(
                lower_single_op_err(malformed),
                LowerError::InvalidOperand { .. }
            ));
        }
        assert!(matches!(
            lower_single_hinted_op_err(
                OpKind::X86Bls {
                    dst: r16,
                    src: rsp,
                    width: OpWidth::W64,
                    kind: X86BlsKind::Blsr,
                    flags: FlagUpdate::None,
                },
                X86OpHint::Mulx,
            ),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[test]
    fn lower_flag_stack_ops_reject_native_stack_operands() {
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));

        for (name, kind) in [
            ("readflags rsp", OpKind::ReadFlags { dst: rsp }),
            ("writeflags rbp", OpKind::WriteFlags { src: rbp }),
            (
                "select rsp",
                OpKind::Select {
                    dst: rax,
                    cond: rsp,
                    src_true: rax,
                    src_false: rbp,
                    width: OpWidth::W64,
                },
            ),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();
            let mut lowerer = X86_64Lowerer::new();
            assert!(lowerer.lower_function(&func).is_err(), "{name}");
        }
    }
    #[test]
    fn lower_count_ops_reject_native_stack_operands() {
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));

        for (name, kind) in [
            (
                "popcnt dst rsp",
                OpKind::Popcnt {
                    dst: rsp,
                    src: rax,
                    width: OpWidth::W64,
                },
            ),
            (
                "ctz src rsp",
                OpKind::Ctz {
                    dst: rax,
                    src: rsp,
                    width: OpWidth::W64,
                },
            ),
            (
                "clz dst rbp",
                OpKind::Clz {
                    dst: rbp,
                    src: rax,
                    width: OpWidth::W64,
                },
            ),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let func = builder.finish();

            let mut lowerer = X86_64Lowerer::new();
            assert!(lowerer.lower_function(&func).is_err(), "{name}");
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_register_bit_tests_preserve_undefined_flags_and_partial_writes() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        const STATUS: u64 = 0x8D5; // CF, PF, AF, ZF, SF, OF
        let cases = [
            (
                "bt r64,r64",
                OpKind::Bt {
                    src: r8,
                    index: SrcOperand::Reg(r9),
                    width: OpWidth::W64,
                },
                1u64 << 63,
                63,
                1u64 << 63,
                true,
            ),
            (
                "bts r16,imm8",
                OpKind::Bts {
                    dst: r8,
                    src: r8,
                    index: SrcOperand::Imm(15),
                    width: OpWidth::W16,
                },
                0xA5A5_A5A5_A5A5_0000,
                0,
                0xA5A5_A5A5_A5A5_8000,
                false,
            ),
            (
                "btr r32,imm8",
                OpKind::Btr {
                    dst: r8,
                    src: r8,
                    index: SrcOperand::Imm64(31),
                    width: OpWidth::W32,
                },
                u64::MAX,
                0,
                0x7FFF_FFFF,
                true,
            ),
            (
                "btc r64,r64",
                OpKind::Btc {
                    dst: r8,
                    src: r8,
                    index: SrcOperand::Reg(r9),
                    width: OpWidth::W64,
                },
                0,
                63,
                1u64 << 63,
                false,
            ),
        ];

        for (name, kind, source, index, expected, expected_cf) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("lower {name}: {error:?}"));
            let code = lowerer.finalize().expect("finalize register bit test");
            let exec = ExecMem::new(&code).expect("map register bit test");
            let mut regs = GuestRegs::default();
            regs.gpr[8] = source;
            regs.gpr[9] = index;
            regs.rflags = 0x2 | STATUS;
            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr[8], expected, "{name}: result");
            let expected_status = (STATUS & !1) | u64::from(expected_cf);
            assert_eq!(
                regs.rflags & STATUS,
                expected_status,
                "{name}: only CF may change"
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_x86_alignment_check_snapshots_prior_native_register_writes() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let r11 = VReg::Arch(ArchReg::X86(X86Reg::R11));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x2000);
        builder.push_op(
            0x2000,
            OpKind::Mov {
                dst: rax,
                src: SrcOperand::Imm64(0x4000),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x2001,
            OpKind::X86CheckAlignment {
                addr: Address::Direct(rax),
                alignment: 64,
            },
        );
        builder.push_op(
            0x2002,
            OpKind::Mov {
                dst: r11,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower prior-write alignment check");
        let exec = ExecMem::new(
            &lowerer
                .finalize()
                .expect("finalize prior-write alignment check"),
        )
        .expect("map prior-write alignment check");
        let mut regs = GuestRegs::default();
        regs.gpr[0] = 0x4001; // stale entry value is intentionally misaligned
        regs.exit_pc = 0xDEAD;
        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(regs.gpr[0], 0x4000);
        assert_eq!(regs.gpr[11], 1, "aligned continuation must execute");
        assert_eq!(regs.exit_pc, 0xDEAD, "must not deopt from stale state");
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_x86_random_preserves_width_semantics_and_defines_exact_status_flags() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);
        const NON_CF_STATUS: u64 = STATUS & !(1 << 0);
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        for seed in [false, true] {
            if (seed && !std::is_x86_feature_detected!("rdseed"))
                || (!seed && !std::is_x86_feature_detected!("rdrand"))
            {
                continue;
            }
            for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
                builder.push_op(
                    0x1000,
                    OpKind::X86Random {
                        dst: r9,
                        width,
                        seed,
                    },
                );
                builder.set_terminator(Terminator::Return { values: vec![] });
                let mut lowerer = X86_64Lowerer::new();
                let lowered = lowerer
                    .lower_function(&builder.finish())
                    .expect("lower native X86Random");
                let exec = ExecMem::new(&lowerer.finalize().expect("finalize native X86Random"))
                    .expect("map native X86Random");
                let input = 0xA5A5_5A5A_C3C3_3C3C;
                let mut regs = GuestRegs::default();
                regs.gpr[9] = input;
                regs.rflags = 0x2 | STATUS;
                exec.run(lowered.entry_offset, &mut regs);

                let success = regs.rflags & 1 != 0;
                assert_eq!(regs.rflags & NON_CF_STATUS, 0, "seed={seed} {width:?}");
                assert_ne!(regs.rflags & 0x2, 0, "reserved RFLAGS bit must survive");
                match width {
                    OpWidth::W16 => {
                        assert_eq!(regs.gpr[9] >> 16, input >> 16);
                        if !success {
                            assert_eq!(regs.gpr[9] & 0xFFFF, 0);
                        }
                    }
                    OpWidth::W32 => {
                        assert_eq!(regs.gpr[9] >> 32, 0);
                        if !success {
                            assert_eq!(regs.gpr[9], 0);
                        }
                    }
                    OpWidth::W64 => {
                        if !success {
                            assert_eq!(regs.gpr[9], 0);
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_rdpid_returns_emulated_tsc_aux_and_preserves_flags() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, OpKind::X86ReadPid { dst: r9 });
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower state-backed RDPID");
        let code = lowerer.finalize().expect("finalize state-backed RDPID");
        let exec = ExecMem::new(&code).expect("map state-backed RDPID");
        let mut regs = GuestRegs::default();
        regs.gpr[9] = u64::MAX;
        regs.rflags = 0x2 | 0x8D5;
        regs.tsc_aux = 0xA5C3_7E91;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr[9], 0xA5C3_7E91, "RDPID zero-extends TSC_AUX");
        assert_eq!(regs.rflags & 0x8D5, 0x8D5, "RDPID preserves RFLAGS");

        let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
        let mut builder = FunctionBuilder::new(FunctionId(1), 0x2000);
        builder.push_op(0x2000, OpKind::X86ReadPid { dst: r16 });
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower state-backed APX RDPID");
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize APX RDPID"))
            .expect("map state-backed APX RDPID");
        let mut regs = GuestRegs::default();
        regs.gpr[0] = 0x1111_2222_3333_4444;
        regs.gpr[1] = 0x5555_6666_7777_8888;
        regs.gpr[16] = u64::MAX;
        regs.rflags = 0x2 | 0x8D5;
        regs.tsc_aux = 0xA5C3_7E91;
        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(regs.gpr[16], 0xA5C3_7E91);
        assert_eq!(regs.gpr[0], 0x1111_2222_3333_4444);
        assert_eq!(regs.gpr[1], 0x5555_6666_7777_8888);
        assert_eq!(regs.rflags & 0x8D5, 0x8D5);
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_crc32c_matches_castagnoli_recurrence_and_preserves_flags() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        if !std::is_x86_feature_detected!("sse4.2") {
            return;
        }
        fn crc32c(mut crc: u32, data: u64, width: OpWidth) -> u32 {
            const POLY: u32 = 0x82F6_3B78;
            for byte in 0..(width.bits() / 8) {
                crc ^= ((data >> (byte * 8)) & 0xff) as u32;
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (POLY & 0u32.wrapping_sub(crc & 1));
                }
            }
            crc
        }

        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let r9 = VReg::Arch(ArchReg::X86(X86Reg::R9));
        const FLAGS: u64 = 0x8D5;
        for (name, width, accumulator, data, alias) in [
            ("byte", OpWidth::W8, 0x1234_5678, 0xA5, false),
            ("word", OpWidth::W16, 0x89AB_CDEF, 0xBEEF, false),
            ("dword", OpWidth::W32, 0x1020_3040, 0xDEAD_BEEF, false),
            (
                "qword",
                OpWidth::W64,
                0x7654_3210,
                0x0123_4567_89AB_CDEF,
                false,
            ),
            (
                "aliased dword",
                OpWidth::W32,
                0xA5A5_5A5A,
                0xA5A5_5A5A,
                true,
            ),
        ] {
            let data_reg = if alias { r8 } else { r9 };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Crc32C {
                    dst: r8,
                    crc: r8,
                    data: data_reg,
                    data_width: width,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("lower {name}: {error:?}"));
            let code = lowerer.finalize().expect("finalize CRC32");
            let exec = ExecMem::new(&code).expect("map CRC32");
            let mut regs = GuestRegs::default();
            regs.gpr[8] = accumulator;
            regs.gpr[9] = data;
            regs.rflags = 0x2 | FLAGS;
            exec.run(lowered.entry_offset, &mut regs);

            let source = if alias { accumulator } else { data };
            assert_eq!(
                regs.gpr[8],
                u64::from(crc32c(accumulator as u32, source, width)),
                "{name}: result"
            );
            assert_eq!(regs.rflags & FLAGS, FLAGS, "{name}: flags");
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_bit_scans_preserve_every_non_zf_status_flag() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let zf_only = FlagUpdate::Specific(FlagSet::ZF);
        const STATUS: u64 = 0x8D5; // CF, PF, AF, ZF, SF, OF
        for (name, kind, source, expected_result, expected_zf) in [
            (
                "bsf nonzero",
                OpKind::Bsf {
                    dst: r8,
                    src: rax,
                    width: OpWidth::W64,
                    flags: zf_only,
                },
                0x100,
                Some(8),
                false,
            ),
            (
                "bsr nonzero",
                OpKind::Bsr {
                    dst: r8,
                    src: rax,
                    width: OpWidth::W32,
                    flags: zf_only,
                },
                0x8000_0000,
                Some(31),
                false,
            ),
            (
                "bsf zero",
                OpKind::Bsf {
                    dst: r8,
                    src: rax,
                    width: OpWidth::W16,
                    flags: zf_only,
                },
                0,
                None,
                true,
            ),
        ] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("lower {name}: {error:?}"));
            let code = lowerer.finalize().expect("finalize bit scan");
            let exec = ExecMem::new(&code).expect("map bit scan");
            let mut regs = GuestRegs::default();
            regs.gpr[0] = source;
            regs.gpr[8] = 0xA5A5_A5A5_A5A5_A5A5;
            regs.rflags = 0x2 | STATUS;
            exec.run(lowered.entry_offset, &mut regs);

            if let Some(expected) = expected_result {
                assert_eq!(regs.gpr[8], expected, "{name}: result");
            }
            let expected_status = if expected_zf {
                STATUS | (1 << 6)
            } else {
                STATUS & !(1 << 6)
            };
            assert_eq!(
                regs.rflags & STATUS,
                expected_status,
                "{name}: only ZF may change"
            );
        }
    }
