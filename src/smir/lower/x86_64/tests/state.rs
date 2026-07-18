//! tests::state tests

use super::*;
use crate::smir::lower::x86_64::*;

    #[test]
    fn rejects_non_state_backed_guest_stack_frame_address_destinations() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));

        let err = lower_single_op_err(OpKind::Lea {
            dst: rbp,
            addr: Address::Direct(rax),
        });
        assert!(
            matches!(err, LowerError::InvalidRegister(ref reg) if reg.contains("Rbp")),
            "LEA RBP,[RAX] must remain outside the state-backed path, got {err:?}"
        );
    }
    #[test]
    fn lower_state_backed_gpr_extensions_emit_state_commits_and_reject_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let rsp = lower_single_op(OpKind::ZeroExtend {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbx),
            from_width: OpWidth::W8,
            to_width: OpWidth::W16,
        });
        assert!(
            rsp.windows(5)
                .any(|bytes| bytes == [0x66, 0x0F, 0xB6, 0x50, 0x18]),
            "MOVZX SP,BL must read the RBX snapshot: {rsp:02X?}"
        );
        assert!(
            rsp.windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x50, 0x20]),
            "MOVZX SP,BL must partially commit GuestRegs.gpr[4]: {rsp:02X?}"
        );

        let r16 = lower_single_op(OpKind::SignExtend {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rbp),
            from_width: OpWidth::W16,
            to_width: OpWidth::W64,
        });
        assert!(
            r16.windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "MOVSX R16,BP must commit GuestRegs.gpr[16]: {r16:02X?}"
        );

        for malformed in [
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rsp),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
            OpKind::SignExtend {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed extension must fail lowering"
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_cmov_emits_snapshot_cmov_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let rsp = lower_single_op(OpKind::CMove {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbx),
            cond: Condition::Ne,
            width: OpWidth::W16,
        });
        for (name, expected) in [
            (
                "complete RSP destination seed",
                &[0x48, 0x8B, 0x50, 0x20][..],
            ),
            ("RBX source snapshot", &[0x66, 0x8B, 0x78, 0x18][..]),
            ("native CMOVNE DX,DI", &[0x66, 0x0F, 0x45, 0xD7][..]),
            ("partial RSP slot commit", &[0x66, 0x89, 0x50, 0x20][..]),
        ] {
            assert!(
                rsp.windows(expected.len()).any(|window| window == expected),
                "CMOVNE SP,BX missing {name} {expected:02X?}: {rsp:02X?}"
            );
        }

        let r16 = lower_single_op(OpKind::CMove {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rbp),
            cond: Condition::Negative,
            width: OpWidth::W64,
        });
        assert!(
            r16.windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "CMOVS R16,RBP must commit GuestRegs.gpr[16]: {r16:02X?}"
        );

        for malformed in [
            OpKind::CMove {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbx),
                cond: Condition::Ne,
                width: OpWidth::W8,
            },
            OpKind::CMove {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::R16),
                cond: Condition::Always,
                width: OpWidth::W64,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed CMOVcc must fail lowering"
            );
        }

        let hinted = OpKind::CMove {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::R16),
            cond: Condition::Ne,
            width: OpWidth::W64,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[test]
    fn lower_state_backed_gpr_setcc_emits_state_commits_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let rsp = lower_single_op(OpKind::SetCC {
            dst: x86(X86Reg::Rsp),
            cond: Condition::Ne,
            width: OpWidth::W8,
        });
        assert!(
            rsp.windows(3).any(|bytes| bytes == [0x0F, 0x95, 0xC2]),
            "SETNE SPL must evaluate into DL: {rsp:02X?}"
        );
        assert!(
            rsp.windows(3).any(|bytes| bytes == [0x88, 0x50, 0x20]),
            "SETNE SPL must partially commit GuestRegs.gpr[4]: {rsp:02X?}"
        );

        let r16 = lower_single_op(OpKind::SetCC {
            dst: x86(X86Reg::R16),
            cond: Condition::Overflow,
            width: OpWidth::W64,
        });
        assert!(
            r16.windows(4)
                .any(|bytes| bytes == [0x48, 0x0F, 0xB6, 0xD2]),
            "SETZUO R16 must zero-extend the predicate to 64 bits: {r16:02X?}"
        );
        assert!(
            r16.windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "SETZUO R16 must commit GuestRegs.gpr[16]: {r16:02X?}"
        );

        for malformed in [
            OpKind::SetCC {
                dst: x86(X86Reg::R16),
                cond: Condition::Ne,
                width: OpWidth::W16,
            },
            OpKind::SetCC {
                dst: x86(X86Reg::R16),
                cond: Condition::Always,
                width: OpWidth::W64,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed SETcc must fail lowering"
            );
        }

        let hinted = OpKind::SetCC {
            dst: x86(X86Reg::R16),
            cond: Condition::Overflow,
            width: OpWidth::W64,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_rsp_rbp_moves_preserve_host_stack_and_guest_widths() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
        let rsi = VReg::Arch(ArchReg::X86(X86Reg::Rsi));
        let rdi = VReg::Arch(ArchReg::X86(X86Reg::Rdi));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: rax,
                src: SrcOperand::Reg(rsp),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1001,
            OpKind::Mov {
                dst: rsp,
                src: SrcOperand::Imm64(0x1234_5678_9ABC_DEF0u64 as i64),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1002,
            OpKind::Mov {
                dst: rbp,
                src: SrcOperand::Reg(rsp),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1003,
            OpKind::Mov {
                dst: rcx,
                src: SrcOperand::Reg(rbp),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1004,
            OpKind::Mov {
                dst: rsp,
                src: SrcOperand::Reg(rsi),
                width: OpWidth::W32,
            },
        );
        builder.push_op(
            0x1005,
            OpKind::Mov {
                dst: rsp,
                src: SrcOperand::Reg(rdx),
                width: OpWidth::W16,
            },
        );
        builder.push_op(
            0x1006,
            OpKind::Mov {
                dst: rbp,
                src: SrcOperand::Reg(rdi),
                width: OpWidth::W8,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower state-backed stack-register moves");
        assert!(lowered.relocations.is_empty());
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize")).expect("exec memory");

        let initial_rsp = 0x0FED_CBA9_8765_4321;
        let mut regs = GuestRegs::default();
        regs.gpr[2] = 0xA55A;
        regs.gpr[4] = initial_rsp;
        regs.gpr[5] = 0xDEAD_BEEF_CAFE_BABE;
        regs.gpr[6] = 0x8765_4321;
        regs.gpr[7] = 0x7B;
        regs.rflags = 0x8D5;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr[0], initial_rsp, "MOV RAX,RSP source snapshot");
        assert_eq!(regs.gpr[1], 0x1234_5678_9ABC_DEF0, "MOV RCX,RBP chain");
        assert_eq!(regs.gpr[4], 0x8765_A55A, "MOV ESP,ESI then MOV SP,DX");
        assert_eq!(regs.gpr[5], 0x1234_5678_9ABC_DE7B, "MOV BPL,DIL merge");
        const STATUS: u64 = 0x8D5;
        assert_eq!(
            regs.rflags & STATUS,
            STATUS,
            "MOV sequence must preserve status flags"
        );
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_extensions_preserve_widths_flags_and_aliases() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for kind in [
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            OpKind::SignExtend {
                dst: x86(X86Reg::Rbp),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W16,
                to_width: OpWidth::W32,
            },
            OpKind::ZeroExtend {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rcx),
                from_width: OpWidth::W32,
                to_width: OpWidth::W64,
            },
            OpKind::SignExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::R17),
                from_width: OpWidth::W32,
                to_width: OpWidth::W64,
            },
            OpKind::ZeroExtend {
                dst: x86(X86Reg::R18),
                src: x86(X86Reg::R18),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rdx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
        ] {
            builder.push_op(0x1000, kind);
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[5].x86_hint = Some(X86OpHint::LegacyHighByteReg);

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&function)
            .expect("lower state-backed register extensions");
        let code = lowerer.finalize().expect("finalize register extensions");
        assert!(
            code.windows(4)
                .any(|bytes| bytes == [0x66, 0x0F, 0xB6, 0x50]),
            "W16 MOVZX must extend through the state snapshot: {code:02X?}"
        );
        assert!(
            code.windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "R16 destination must commit GuestRegs.gpr[16]: {code:02X?}"
        );

        let exec = ExecMem::new(&code).expect("map register extensions");
        let mut regs = GuestRegs::default();
        regs.gpr[1] = 0x0123_4567_89AB_CDEF;
        regs.gpr[2] = 0xA5A5_5A5A_1357_CD00;
        regs.gpr[3] = 0xFEDC_BA98_7654_80A7;
        regs.gpr[4] = 0x1234_5678_9ABC_80F2;
        regs.gpr[5] = 0x0FED_CBA9_8765_8001;
        regs.gpr[17] = 0xB1B2_B3B4_8000_0001;
        regs.gpr[18] = 0xC1C2_C3C4_C5C6_80F1;
        regs.rflags = 0x2 | 0x8D5;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr[0], 0xFFFF_FFFF_8000_0001, "MOVSXD RAX,R17D");
        assert_eq!(regs.gpr[1], 0x0123_4567_89AB_CDEF, "RCX source");
        assert_eq!(regs.gpr[2], 0xA5A5_5A5A_1357_CD00, "RDX source");
        assert_eq!(regs.gpr[3], 0xFEDC_BA98_7654_80A7, "RBX source");
        assert_eq!(regs.gpr[4], 0x1234_5678_9ABC_00CD, "MOVZX SP,DH");
        assert_eq!(regs.gpr[5], 0x0000_0000_FFFF_80A7, "MOVSX EBP,BX");
        assert_eq!(regs.gpr[16], 0x89AB_CDEF, "MOVZX R16,ECX");
        assert_eq!(regs.gpr[18], 0xC1C2_C3C4_C5C6_00F1, "MOVZX R18W,R18B alias");
        assert_eq!(
            regs.rflags & 0x8D5,
            0x8D5,
            "MOVZX/MOVSX/MOVSXD preserve status flags"
        );
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_cmov_preserves_widths_flags_and_aliases() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS: u64 = 0x8D5;
        const ZF_SET: u64 = 0x2 | STATUS;
        const ZF_CLEAR: u64 = 0x2 | (STATUS & !(1 << 6));

        struct Case {
            name: &'static str,
            kind: OpKind,
            rflags: u64,
            destination_index: usize,
            expected_destination: u64,
        }

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let cases = [
            Case {
                name: "CMOVNE SP,BX true partial destination",
                kind: OpKind::CMove {
                    dst: x86(X86Reg::Rsp),
                    src: x86(X86Reg::Rbx),
                    cond: Condition::Ne,
                    width: OpWidth::W16,
                },
                rflags: ZF_CLEAR,
                destination_index: 4,
                expected_destination: 0x1234_5678_9ABC_80A7,
            },
            Case {
                name: "CMOVNE SP,BX false preserves complete destination",
                kind: OpKind::CMove {
                    dst: x86(X86Reg::Rsp),
                    src: x86(X86Reg::Rbx),
                    cond: Condition::Ne,
                    width: OpWidth::W16,
                },
                rflags: ZF_SET,
                destination_index: 4,
                expected_destination: 0x1234_5678_9ABC_80F2,
            },
            Case {
                name: "CMOVNE EBP,ESP false zeroes upper dword",
                kind: OpKind::CMove {
                    dst: x86(X86Reg::Rbp),
                    src: x86(X86Reg::Rsp),
                    cond: Condition::Ne,
                    width: OpWidth::W32,
                },
                rflags: ZF_SET,
                destination_index: 5,
                expected_destination: 0x8765_8001,
            },
            Case {
                name: "CMOVE EBP,ESP true zeroes upper dword",
                kind: OpKind::CMove {
                    dst: x86(X86Reg::Rbp),
                    src: x86(X86Reg::Rsp),
                    cond: Condition::Eq,
                    width: OpWidth::W32,
                },
                rflags: ZF_SET,
                destination_index: 5,
                expected_destination: 0x9ABC_80F2,
            },
            Case {
                name: "CMOVS R16,RBP state-backed destination",
                kind: OpKind::CMove {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rbp),
                    cond: Condition::Negative,
                    width: OpWidth::W64,
                },
                rflags: ZF_SET,
                destination_index: 16,
                expected_destination: 0x0FED_CBA9_8765_8001,
            },
            Case {
                name: "CMOVP RAX,R16 state-backed source",
                kind: OpKind::CMove {
                    dst: x86(X86Reg::Rax),
                    src: x86(X86Reg::R16),
                    cond: Condition::Parity,
                    width: OpWidth::W64,
                },
                rflags: ZF_SET,
                destination_index: 0,
                expected_destination: 0xA1A2_A3A4_A5A6_80F1,
            },
            Case {
                name: "CMOVNE SP,SP alias",
                kind: OpKind::CMove {
                    dst: x86(X86Reg::Rsp),
                    src: x86(X86Reg::Rsp),
                    cond: Condition::Ne,
                    width: OpWidth::W16,
                },
                rflags: ZF_CLEAR,
                destination_index: 4,
                expected_destination: 0x1234_5678_9ABC_80F2,
            },
        ];

        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, case.kind);
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            regs.gpr[0] = 0x0123_4567_89AB_CDEF;
            regs.gpr[1] = 0x1111_2222_3333_4444;
            regs.gpr[2] = 0xA5A5_5A5A_1357_2468;
            regs.gpr[3] = 0xFEDC_BA98_7654_80A7;
            regs.gpr[4] = 0x1234_5678_9ABC_80F2;
            regs.gpr[5] = 0x0FED_CBA9_8765_8001;
            regs.gpr[6] = 0x99AA_BBCC_DDEE_FF00;
            regs.gpr[7] = 0x0F1E_2D3C_4B5A_6978;
            regs.gpr[16] = 0xA1A2_A3A4_A5A6_80F1;
            regs.gpr[31] = 0xF1F2_F3F4_F5F6_F7F8;
            regs.rflags = case.rflags;
            let mut expected = regs;
            expected.gpr[case.destination_index] = case.expected_destination;

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS,
                case.rflags & STATUS,
                "{} status flags",
                case.name
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_setcc_preserves_widths_flags_and_host_stack() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS: u64 = 0x8D5;
        const ALL_SET: u64 = 0x2 | STATUS;
        const ZF_CLEAR: u64 = ALL_SET & !(1 << 6);
        const OF_CLEAR: u64 = ALL_SET & !(1 << 11);

        struct Case {
            name: &'static str,
            kind: OpKind,
            rflags: u64,
            destination_index: usize,
            expected_destination: u64,
        }

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let cases = [
            Case {
                name: "SETNE SPL true partial destination",
                kind: OpKind::SetCC {
                    dst: x86(X86Reg::Rsp),
                    cond: Condition::Ne,
                    width: OpWidth::W8,
                },
                rflags: ZF_CLEAR,
                destination_index: 4,
                expected_destination: 0x1234_5678_9ABC_8001,
            },
            Case {
                name: "SETNE SPL false partial destination",
                kind: OpKind::SetCC {
                    dst: x86(X86Reg::Rsp),
                    cond: Condition::Ne,
                    width: OpWidth::W8,
                },
                rflags: ALL_SET,
                destination_index: 4,
                expected_destination: 0x1234_5678_9ABC_8000,
            },
            Case {
                name: "SETE BPL true partial destination",
                kind: OpKind::SetCC {
                    dst: x86(X86Reg::Rbp),
                    cond: Condition::Eq,
                    width: OpWidth::W8,
                },
                rflags: ALL_SET,
                destination_index: 5,
                expected_destination: 0x0FED_CBA9_8765_8001,
            },
            Case {
                name: "SETNE R16B true state-backed EGPR destination",
                kind: OpKind::SetCC {
                    dst: x86(X86Reg::R16),
                    cond: Condition::Ne,
                    width: OpWidth::W8,
                },
                rflags: ZF_CLEAR,
                destination_index: 16,
                expected_destination: 0xA1A2_A3A4_A5A6_8001,
            },
            Case {
                name: "SETZUO R16 true full destination",
                kind: OpKind::SetCC {
                    dst: x86(X86Reg::R16),
                    cond: Condition::Overflow,
                    width: OpWidth::W64,
                },
                rflags: ALL_SET,
                destination_index: 16,
                expected_destination: 1,
            },
            Case {
                name: "SETZUO RSP false full destination",
                kind: OpKind::SetCC {
                    dst: x86(X86Reg::Rsp),
                    cond: Condition::Overflow,
                    width: OpWidth::W64,
                },
                rflags: OF_CLEAR,
                destination_index: 4,
                expected_destination: 0,
            },
            Case {
                name: "SETZUNE RBP true full destination",
                kind: OpKind::SetCC {
                    dst: x86(X86Reg::Rbp),
                    cond: Condition::Ne,
                    width: OpWidth::W64,
                },
                rflags: ZF_CLEAR,
                destination_index: 5,
                expected_destination: 1,
            },
        ];

        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, case.kind);
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            regs.gpr[0] = 0x0123_4567_89AB_CDEF;
            regs.gpr[1] = 0x1111_2222_3333_4444;
            regs.gpr[2] = 0xA5A5_5A5A_1357_2468;
            regs.gpr[3] = 0xFEDC_BA98_7654_80A7;
            regs.gpr[4] = 0x1234_5678_9ABC_80F2;
            regs.gpr[5] = 0x0FED_CBA9_8765_8001;
            regs.gpr[6] = 0x99AA_BBCC_DDEE_FF00;
            regs.gpr[7] = 0x0F1E_2D3C_4B5A_6978;
            regs.gpr[16] = 0xA1A2_A3A4_A5A6_80F1;
            regs.gpr[31] = 0xF1F2_F3F4_F5F6_F7F8;
            regs.rflags = case.rflags;
            let mut expected = regs;
            expected.gpr[case.destination_index] = case.expected_destination;

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS,
                case.rflags & STATUS,
                "{} status flags",
                case.name
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_stack_add_sub_preserve_host_stack_widths_and_flags() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
        let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
        let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Add {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Reg(rax),
                width: OpWidth::W8,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x1001,
            OpKind::Sub {
                dst: rbp,
                src1: rbp,
                src2: SrcOperand::Reg(rdx),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x1002,
            OpKind::Add {
                dst: r16,
                src1: rsp,
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x1003,
            OpKind::Sub {
                dst: rbp,
                src1: rsp,
                src2: SrcOperand::Imm(0x10),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower state-backed stack arithmetic");
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize")).expect("exec memory");

        let mut regs = GuestRegs::default();
        regs.gpr[0] = 0x20;
        regs.gpr[2] = 0x20;
        regs.gpr[4] = 0x1111_2222_3333_44F0;
        regs.gpr[5] = 0xAAAA_BBBB_CCCC_DD10;
        regs.rflags = 0x8D5;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr[4], 0x1111_2222_3333_4410, "ADD SPL,AL");
        assert_eq!(regs.gpr[16], 0x2100, "ADD R16D,ESP/EBP snapshot");
        assert_eq!(regs.gpr[5], 0x1111_2222_3333_4400, "SUB RBP,RSP,16");
        const STATUS: u64 = 0x8D5;
        assert_eq!(
            regs.rflags & STATUS,
            STATUS,
            "FlagUpdate::None arithmetic must preserve status flags"
        );
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn return_shape_does_not_elide_state_backed_guest_rsp_updates() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1004,
            OpKind::Add {
                dst: rsp,
                src1: rsp,
                src2: SrcOperand::Imm(8),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&func)
            .expect("state-backed return-shape RSP update");
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize")).expect("exec memory");
        let mut regs = GuestRegs::default();
        regs.gpr[4] = 0x1234_5678_9ABC_DEF0;
        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(
            regs.gpr[4], 0x1234_5678_9ABC_DEF8,
            "guest RSP update must execute rather than becoming a host RET immediate"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_immediate_bit_test_consumes_staged_stack_source() {
        // bt qword ptr [rbx],5; hlt
        let (lowered, entry) =
            lower_rex2_block_with_mem_helpers(&[0x48, 0x0F, 0xBA, 0x23, 0x05, 0xF4], true);
        assert!(entry < lowered.len());
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBA, 0x64, 0x24, 0x08, 0x05]),
            "helper-backed BT must consume the caller-owned stack word: {lowered:02X?}"
        );
        let mut helper_call = vec![0xFF, 0x90];
        helper_call.extend_from_slice(&(X86_GUEST_LOAD_FN_OFFSET as u32).to_le_bytes());
        assert_eq!(
            lowered
                .windows(helper_call.len())
                .filter(|bytes| *bytes == helper_call)
                .count(),
            1,
            "memory-source BT must issue exactly one load helper call"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_immediate_bit_update_stages_rmw_and_replays_cf() {
        // btr qword ptr [rbx],5; hlt
        let (lowered, entry) =
            lower_rex2_block_with_mem_helpers(&[0x48, 0x0F, 0xBA, 0x33, 0x05, 0xF4], true);
        assert!(entry < lowered.len());
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBA, 0x74, 0x24, 0x10, 0x05]),
            "helper-backed BTR must modify the staged store word: {lowered:02X?}"
        );
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBA, 0x64, 0x24, 0x08, 0x05]),
            "helper-backed BTR must replay CF from the original word: {lowered:02X?}"
        );
        for (offset, name) in [
            (X86_GUEST_LOAD_FN_OFFSET, "load"),
            (X86_GUEST_STORE_FN_OFFSET, "store"),
        ] {
            let mut helper_call = vec![0xFF, 0x90];
            helper_call.extend_from_slice(&(offset as u32).to_le_bytes());
            assert_eq!(
                lowered
                    .windows(helper_call.len())
                    .filter(|bytes| *bytes == helper_call)
                    .count(),
                1,
                "memory-destination BTR must issue exactly one {name} helper call"
            );
        }

        // btc qword ptr [rbx],63; hlt
        let (lowered, entry) =
            lower_rex2_block_with_mem_helpers(&[0x48, 0x0F, 0xBA, 0x3B, 0x3F, 0xF4], true);
        assert!(entry < lowered.len());
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBA, 0x7C, 0x24, 0x10, 0x3F]),
            "helper-backed BTC must modify the staged store word: {lowered:02X?}"
        );
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBA, 0x64, 0x24, 0x08, 0x3F]),
            "helper-backed BTC must replay CF from the original word: {lowered:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_two_operand_imul_uses_staged_memory_source() {
        for (name, instruction, expected) in [
            (
                "W16 RAX",
                &[0x66, 0x0F, 0xAF, 0x03, 0xF4][..],
                &[0x66, 0x0F, 0xAF, 0x04, 0x24][..],
            ),
            (
                "W32 R8",
                &[0x44, 0x0F, 0xAF, 0x03, 0xF4][..],
                &[0x44, 0x0F, 0xAF, 0x04, 0x24][..],
            ),
            (
                "W64 R9",
                &[0x4C, 0x0F, 0xAF, 0x0B, 0xF4][..],
                &[0x4C, 0x0F, 0xAF, 0x0C, 0x24][..],
            ),
        ] {
            let (lowered, entry) = lower_rex2_block_with_mem_helpers(instruction, true);
            assert!(entry < lowered.len(), "{name}");
            assert!(
                lowered
                    .windows(expected.len())
                    .any(|bytes| bytes == expected),
                "helper-backed {name} IMUL must consume the staged stack source: {lowered:02X?}"
            );
        }

        let temporary = VReg::Virtual(crate::smir::ir::types::VirtualId(90));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let mut builder = FunctionBuilder::new(FunctionId(90), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::MulS {
                dst_lo: r8,
                dst_hi: None,
                src1: r8,
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer
            .lower_function(&builder.finish())
            .expect("lower flag-preserving helper-backed IMUL");
        let lowered = lowerer.finalize().expect("finalize flag-preserving IMUL");
        assert!(
            lowered
                .windows(8)
                .any(|bytes| bytes == [0x9C, 0x4C, 0x0F, 0xAF, 0x44, 0x24, 0x08, 0x9D]),
            "NF IMUL must preserve flags around the staged stack source: {lowered:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_immediate_imul_uses_hint_and_staged_memory_source() {
        for (name, instruction, expected) in [
            (
                "W16 RAX imm16",
                &[0x66, 0x69, 0x03, 0x34, 0x12, 0xF4][..],
                &[0x66, 0x69, 0x04, 0x24, 0x34, 0x12][..],
            ),
            (
                "W32 R8 imm8",
                &[0x44, 0x6B, 0x03, 0xFD, 0xF4][..],
                &[0x44, 0x6B, 0x04, 0x24, 0xFD][..],
            ),
            (
                "W64 R9 imm32",
                &[0x4C, 0x69, 0x0B, 0x78, 0x56, 0x34, 0x12, 0xF4][..],
                &[0x4C, 0x69, 0x0C, 0x24, 0x78, 0x56, 0x34, 0x12][..],
            ),
        ] {
            let (lowered, entry) = lower_rex2_block_with_mem_helpers(instruction, true);
            assert!(entry < lowered.len(), "{name}");
            assert!(
                lowered
                    .windows(expected.len())
                    .any(|bytes| bytes == expected),
                "helper-backed {name} must preserve the opcode hint and consume the staged source: {lowered:02X?}"
            );
        }

        let temporary = VReg::Virtual(crate::smir::ir::types::VirtualId(91));
        let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
        let mut builder = FunctionBuilder::new(FunctionId(91), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
                width: MemWidth::B2,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::MulS {
                dst_lo: r8,
                dst_hi: None,
                src1: temporary,
                src2: SrcOperand::Imm(0x1234),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::ImulImm32);
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer
            .lower_function(&function)
            .expect("lower flag-preserving helper-backed immediate IMUL");
        let lowered = lowerer
            .finalize()
            .expect("finalize flag-preserving immediate IMUL");
        assert!(
            lowered.windows(10).any(|bytes| {
                bytes == [0x9C, 0x66, 0x44, 0x69, 0x44, 0x24, 0x08, 0x34, 0x12, 0x9D]
            }),
            "NF immediate IMUL must preserve flags around the shifted staged source: {lowered:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_memory_cmov_uses_staged_and_state_destinations() {
        for (name, instruction, expected) in [
            (
                "CMOVNE AX,word [RBX]",
                &[0x66, 0x0F, 0x45, 0x03, 0xF4][..],
                &[0x66, 0x0F, 0x45, 0x04, 0x24][..],
            ),
            (
                "CMOVE ECX,dword [RBX]",
                &[0x0F, 0x44, 0x0B, 0xF4][..],
                &[0x0F, 0x44, 0x0C, 0x24][..],
            ),
            (
                "CMOVNE R9,qword [RBX]",
                &[0x4C, 0x0F, 0x45, 0x0B, 0xF4][..],
                &[0x4C, 0x0F, 0x45, 0x0C, 0x24][..],
            ),
        ] {
            let (lowered, entry) = lower_rex2_block_with_mem_helpers(instruction, true);
            assert!(entry < lowered.len(), "{name}");
            assert!(
                lowered
                    .windows(expected.len())
                    .any(|bytes| bytes == expected),
                "{name} must conditionally read the helper-staged source: {lowered:02X?}"
            );
            assert!(
                lowered
                    .windows(5)
                    .any(|bytes| bytes == [0x48, 0x89, 0x44, 0x24, 0x10]),
                "{name} helper must stage above saved flags and RAX: {lowered:02X?}"
            );
        }

        let (rsp, _) = lower_rex2_block_with_mem_helpers(&[0x66, 0x0F, 0x45, 0x23, 0xF4], true);
        assert!(
            rsp.windows(6)
                .any(|bytes| bytes == [0x66, 0x0F, 0x45, 0x54, 0x24, 0x10]),
            "CMOVNE SP must conditionally update a state-commit scratch: {rsp:02X?}"
        );
        assert!(
            rsp.windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x50, 0x20]),
            "CMOVNE SP must partially commit GuestRegs.gpr[4]: {rsp:02X?}"
        );

        let (rbp, _) = lower_rex2_block_with_mem_helpers(&[0x0F, 0x44, 0x2B, 0xF4], true);
        assert!(
            rbp.windows(4)
                .any(|bytes| bytes == [0x48, 0x89, 0x50, 0x28]),
            "CMOVE EBP must commit the complete conditional result: {rbp:02X?}"
        );
        assert!(
            rbp.windows(4)
                .any(|bytes| bytes == [0x48, 0x89, 0x55, 0x00]),
            "CMOVE EBP must synchronize the prologue's saved guest RBP: {rbp:02X?}"
        );

        let (r16, _) = lower_rex2_block_with_mem_helpers(&[0xD5, 0xC8, 0x45, 0x03, 0xF4], true);
        assert!(
            r16.windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "CMOVNE R16 must commit canonical GuestRegs.gpr[16]: {r16:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_memory_extensions_use_staged_and_state_destinations() {
        for (name, instruction, expected) in [
            (
                "MOVZX EAX,byte [RBX]",
                &[0x0F, 0xB6, 0x03, 0xF4][..],
                &[0x0F, 0xB6, 0x04, 0x24][..],
            ),
            (
                "MOVZX R9,word [RBX]",
                &[0x4C, 0x0F, 0xB7, 0x0B, 0xF4][..],
                &[0x4C, 0x0F, 0xB7, 0x0C, 0x24][..],
            ),
            (
                "MOVSX AX,byte [RBX]",
                &[0x66, 0x0F, 0xBE, 0x03, 0xF4][..],
                &[0x66, 0x0F, 0xBE, 0x04, 0x24][..],
            ),
            (
                "MOVSX R10,word [RBX]",
                &[0x4C, 0x0F, 0xBF, 0x13, 0xF4][..],
                &[0x4C, 0x0F, 0xBF, 0x14, 0x24][..],
            ),
            (
                "MOVSXD R12,dword [RBX]",
                &[0x4C, 0x63, 0x23, 0xF4][..],
                &[0x4C, 0x63, 0x24, 0x24][..],
            ),
        ] {
            let (lowered, entry) = lower_rex2_block_with_mem_helpers(instruction, true);
            assert!(entry < lowered.len(), "{name}");
            assert!(
                lowered
                    .windows(expected.len())
                    .any(|bytes| bytes == expected),
                "{name} must extend the helper-staged stack source: {lowered:02X?}"
            );
            assert!(
                lowered
                    .windows(5)
                    .any(|bytes| bytes == [0x48, 0x89, 0x44, 0x24, 0x10]),
                "{name} helper must stage above saved flags and RAX: {lowered:02X?}"
            );
        }

        let (rsp, _) = lower_rex2_block_with_mem_helpers(&[0x66, 0x0F, 0xB6, 0x23, 0xF4], true);
        assert!(
            rsp.windows(6)
                .any(|bytes| bytes == [0x66, 0x0F, 0xB6, 0x54, 0x24, 0x10]),
            "MOVZX SP must extend into a state-commit scratch: {rsp:02X?}"
        );
        assert!(
            rsp.windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x50, 0x20]),
            "MOVZX SP must partially commit GuestRegs.gpr[4]: {rsp:02X?}"
        );

        let (rbp, _) = lower_rex2_block_with_mem_helpers(&[0x0F, 0xBF, 0x2B, 0xF4], true);
        assert!(
            rbp.windows(4)
                .any(|bytes| bytes == [0x48, 0x89, 0x50, 0x28]),
            "MOVSX EBP must commit GuestRegs.gpr[5]: {rbp:02X?}"
        );
        assert!(
            rbp.windows(4)
                .any(|bytes| bytes == [0x48, 0x89, 0x55, 0x00]),
            "MOVSX EBP must synchronize the prologue's saved guest RBP: {rbp:02X?}"
        );

        let temporary = VReg::Virtual(crate::smir::ir::types::VirtualId(93));
        let mut builder = FunctionBuilder::new(FunctionId(93), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
                width: MemWidth::B2,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::ZeroExtend {
                dst: VReg::Arch(ArchReg::X86(X86Reg::R16)),
                src: temporary,
                from_width: OpWidth::W16,
                to_width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer
            .lower_function(&builder.finish())
            .expect("lower helper-backed R16 MOVZX");
        let r16 = lowerer.finalize().expect("finalize R16 MOVZX");
        assert!(
            r16.windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "MOVZX R16 must commit canonical GuestRegs.gpr[16]: {r16:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_widening_multiply_uses_staged_memory_source() {
        for (name, instruction, expected) in [
            ("MUL byte", &[0xF6, 0x23, 0xF4][..], &[0xF6, 0x24, 0x24][..]),
            (
                "IMUL word",
                &[0x66, 0xF7, 0x2B, 0xF4][..],
                &[0x66, 0xF7, 0x2C, 0x24][..],
            ),
            (
                "MUL dword",
                &[0xF7, 0x23, 0xF4][..],
                &[0xF7, 0x24, 0x24][..],
            ),
            (
                "IMUL qword",
                &[0x48, 0xF7, 0x2B, 0xF4][..],
                &[0x48, 0xF7, 0x2C, 0x24][..],
            ),
        ] {
            let (lowered, entry) = lower_rex2_block_with_mem_helpers(instruction, true);
            assert!(entry < lowered.len(), "{name}");
            assert!(
                lowered
                    .windows(expected.len())
                    .any(|bytes| bytes == expected),
                "helper-backed {name} must consume the staged stack source: {lowered:02X?}"
            );
        }

        let temporary = VReg::Virtual(crate::smir::ir::types::VirtualId(92));
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let mut builder = FunctionBuilder::new(FunctionId(92), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::MulS {
                dst_lo: rax,
                dst_hi: Some(rdx),
                src1: rax,
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer
            .lower_function(&builder.finish())
            .expect("lower flag-preserving helper-backed widening IMUL");
        let lowered = lowerer
            .finalize()
            .expect("finalize flag-preserving widening IMUL");
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x9C, 0x48, 0xF7, 0x6C, 0x24, 0x08, 0x9D]),
            "NF widening IMUL must preserve flags around the shifted staged source: {lowered:02X?}"
        );
        assert!(
            lowered
                .windows(5)
                .any(|bytes| bytes == [0x48, 0x89, 0x44, 0x24, 0x10]),
            "load helper must stage the source above its saved flags and RAX: {lowered:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_optimized_helper_backed_btc_accepts_folded_w64_mask() {
        // btc qword ptr [rbx],63; hlt
        let bytes = [0x48, 0x0F, 0xBA, 0x3B, 0x3F, 0xF4];
        let reader = TestReader {
            base: 0x1000,
            bytes: bytes.to_vec(),
        };
        let mut lifter = X86_64Lifter::strict();
        let mut lctx = LiftContext::new(SourceArch::X86_64);
        let mut block = lifter
            .lift_block(0x1000, &reader, &mut lctx)
            .expect("lift BTC block");
        block.set_terminator(Terminator::Return { values: vec![] });
        let block_id = block.id;
        let mut function = SmirFunction::new(FunctionId(0), block_id, 0x1000);
        function.add_block(block);
        crate::smir::optimize::optimize_function(
            &mut function,
            crate::smir::optimize::OptLevel::O2,
        );
        assert!(matches!(
            function.blocks[0].ops[1].kind,
            OpKind::Xor {
                src2: SrcOperand::Imm(i64::MIN),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
                ..
            }
        ));

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        let result = lowerer
            .lower_function(&function)
            .expect("lower optimized BTC block");
        assert!(result.relocations.is_empty());
        let lowered = lowerer.finalize().expect("finalize optimized BTC");
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBA, 0x7C, 0x24, 0x10, 0x3F]),
            "optimized helper-backed BTC must modify the staged store word: {lowered:02X?}"
        );
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBA, 0x64, 0x24, 0x08, 0x3F]),
            "optimized helper-backed BTC must replay CF from the original word: {lowered:02X?}"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_bit_scan_consumes_staged_stack_source() {
        // bsf r8w, word ptr [rbx]; hlt
        let (lowered, entry) =
            lower_rex2_block_with_mem_helpers(&[0x66, 0x44, 0x0F, 0xBC, 0x03, 0xF4], true);
        assert!(entry < lowered.len());
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0x66, 0x44, 0x0F, 0xBC, 0x44, 0x24, 0x08]),
            "helper-backed BSF must consume the caller-owned stack word: {lowered:02X?}"
        );
        assert!(
            lowered
                .windows(5)
                .any(|bytes| bytes == [0x4C, 0x8B, 0x44, 0x24, 0x10]),
            "zero-source BSF must restore the saved full destination: {lowered:02X?}"
        );
        let mut helper_call = vec![0xFF, 0x90];
        helper_call.extend_from_slice(&(X86_GUEST_LOAD_FN_OFFSET as u32).to_le_bytes());
        assert_eq!(
            lowered
                .windows(helper_call.len())
                .filter(|bytes| *bytes == helper_call)
                .count(),
            1,
            "memory-source bit scan must issue exactly one load helper call"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn lower_helper_backed_scalar_count_consumes_staged_stack_source() {
        // popcnt r8w, word ptr [rbx]; hlt
        let (lowered, entry) =
            lower_rex2_block_with_mem_helpers(&[0xF3, 0x66, 0x44, 0x0F, 0xB8, 0x03, 0xF4], true);
        assert!(entry < lowered.len());
        assert!(
            lowered
                .windows(7)
                .any(|bytes| bytes == [0xF3, 0x66, 0x44, 0x0F, 0xB8, 0x04, 0x24]),
            "helper-backed POPCNT must consume the caller-owned stack word: {lowered:02X?}"
        );
        let mut helper_call = vec![0xFF, 0x90];
        helper_call.extend_from_slice(&(X86_GUEST_LOAD_FN_OFFSET as u32).to_le_bytes());
        assert_eq!(
            lowered
                .windows(helper_call.len())
                .filter(|bytes| *bytes == helper_call)
                .count(),
            1,
            "memory-source count must issue exactly one load helper call"
        );
    }
    #[test]
    fn lower_guest_rbp_mov_updates_state_and_saved_epilogue_value() {
        // `mov rbp, 0x1234` (48 C7 C5 34 12 00 00) must write GuestRegs.gpr[5]
        // and the prologue's saved guest-RBP word. Hardware RBP remains the
        // trusted frame pointer until the epilogue POP consumes that saved word.
        let (lowered, _) = lower_rex2_block(&[0x48, 0xC7, 0xC5, 0x34, 0x12, 0x00, 0x00, 0xF4]);
        assert!(
            lowered
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x89, 0x50, 0x28]),
            "state-backed guest RBP store missing: {lowered:02X?}"
        );
        assert!(
            lowered
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x89, 0x55, 0x00]),
            "saved guest RBP update missing: {lowered:02X?}"
        );
    }
    #[test]
    fn lower_state_backed_gpr_rotate_emits_count_flag_contracts_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let rotate_flags = FlagSet::CF.union(FlagSet::OF);

        let one = lower_single_op(OpKind::Rol {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(rotate_flags),
        });
        assert!(
            one.windows(3).any(|bytes| bytes == [0x48, 0xD1, 0xC2]),
            "state-backed ROL must rotate RDX by its immediate: {one:02X?}"
        );
        assert_eq!(
            one.iter().filter(|byte| **byte == 0x9C).count(),
            2,
            "flagful ROL must save incoming and native RFLAGS: {one:02X?}"
        );
        assert_eq!(one.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(
            one.windows(9)
                .any(|bytes| bytes == [0x48, 0x81, 0x64, 0x24, 0x10, 0xFE, 0xF7, 0xFF, 0xFF]),
            "count-one ROL must replace exactly CF and OF: {one:02X?}"
        );

        let dynamic = lower_single_op(OpKind::Ror {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        });
        assert!(
            dynamic.windows(2).any(|bytes| bytes == [0xD2, 0xCA]),
            "state-backed ROR must use staged CL and DL: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x83, 0xE7, 0x1F]),
            "byte ROR must classify the 5-bit masked count: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(2)
                .filter(|bytes| *bytes == [0x0F, 0x84])
                .count()
                >= 2,
            "dynamic ROR must branch on zero and one counts: {dynamic:02X?}"
        );

        let suppressed = lower_single_op(OpKind::Rol {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R31),
            amount: SrcOperand::Imm(9),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        });
        assert!(
            suppressed
                .windows(4)
                .any(|bytes| bytes == [0x66, 0xC1, 0xC2, 0x09]),
            "state-backed NF ROL must use staged DX: {suppressed:02X?}"
        );
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9C).count(), 1);
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(
            suppressed
                .windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
            "word ROL must partially synchronize guest RBP: {suppressed:02X?}"
        );

        for malformed in [
            OpKind::Rol {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W128,
                flags: FlagUpdate::Specific(rotate_flags),
            },
            OpKind::Ror {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
            OpKind::Rol {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
            OpKind::Ror {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
                ),
                "malformed state-backed rotate must fail lowering"
            );
        }
        assert!(matches!(
            lower_single_hinted_op_err(
                OpKind::Rol {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rsp),
                    amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                X86OpHint::Mulx,
            ),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_rotate_preserves_width_alias_count_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;
        let rotate_flags = FlagSet::CF.union(FlagSet::OF);

        struct Case {
            name: &'static str,
            right: bool,
            dst: X86Reg,
            src: X86Reg,
            count_reg: Option<X86Reg>,
            immediate: i64,
            width: OpWidth,
            flags: FlagUpdate,
            source: u64,
            count: u64,
            status: u64,
        }

        let cases = [
            Case {
                name: "ROL RSP,RBP,0 preserves every flag",
                right: false,
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                count_reg: None,
                immediate: 0,
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0x8123_4567_89AB_CDEF,
                count: 0,
                status: 0x8D5,
            },
            Case {
                name: "ROL BPL,SPL,1 partial count-one flags",
                right: false,
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                count_reg: None,
                immediate: 1,
                width: OpWidth::W8,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0x2233_4455_6677_5681,
                count: 1,
                status: 0x0D4,
            },
            Case {
                name: "ROR R16B,R31B,9 preserves multi-bit OF",
                right: true,
                dst: X86Reg::R16,
                src: X86Reg::R31,
                count_reg: None,
                immediate: 9,
                width: OpWidth::W8,
                flags: FlagUpdate::All,
                source: 0xFFEE_DDCC_BBAA_1302,
                count: 9,
                status: 0x8D4,
            },
            Case {
                name: "ROL R31W,R16W,SP effective-zero updates CF",
                right: false,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::Rsp),
                immediate: 0,
                width: OpWidth::W16,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0xAABB_CCDD_EEFF_8000,
                count: 16,
                status: 0x8D4,
            },
            Case {
                name: "ROR R16D,R16D,R16 all aliases",
                right: true,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::R16),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0xAABB_CCDD_8000_0011,
                count: 0x8000_0011,
                status: 0x0D5,
            },
            Case {
                name: "NF ROR RSP,R31D,BP zero-extends and preserves flags",
                right: true,
                dst: X86Reg::Rsp,
                src: X86Reg::R31,
                count_reg: Some(X86Reg::Rbp),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
                source: 0xFFEE_DDCC_8000_0001,
                count: 1,
                status: 0x8D5,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let amount = case
                .count_reg
                .map_or(SrcOperand::Imm(case.immediate), |reg| {
                    SrcOperand::Reg(x86(reg))
                });
            let kind = if case.right {
                OpKind::Ror {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                }
            } else {
                OpKind::Rol {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                }
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x1357_0000_2468_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            if let Some(count_reg) = case.count_reg {
                let count_idx = count_reg.gpr_index().unwrap() as usize;
                if count_idx != src_idx {
                    regs.gpr[count_idx] = case.count;
                }
            }
            regs.rflags = 0x2 | case.status;

            let mut expected = regs;
            let bits = u64::from(case.width.bits());
            let count_mask = if bits == 64 { 0x3f } else { 0x1f };
            let raw_count = case.count_reg.map_or(case.immediate as u64, |reg| {
                regs.gpr[reg.gpr_index().unwrap() as usize]
            });
            let masked = raw_count & count_mask;
            let amount = masked % bits;
            let source = regs.gpr[src_idx] & case.width.mask();
            let result = if amount == 0 {
                source
            } else if case.right {
                ((source >> amount) | (source << (bits - amount))) & case.width.mask()
            } else {
                ((source << amount) | (source >> (bits - amount))) & case.width.mask()
            };
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W8 | OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W128 => unreachable!(),
            };
            if case.flags.updates_any() && masked != 0 {
                let sign_bit = case.width.sign_bit();
                let cf = if case.right {
                    u64::from(result & sign_bit != 0)
                } else {
                    result & 1
                };
                expected.rflags = (expected.rflags & !1) | cf;
                if masked == 1 {
                    let of = if case.right {
                        u64::from((result & sign_bit != 0) != (result & (sign_bit >> 1) != 0))
                    } else {
                        u64::from((result & sign_bit != 0) != (cf != 0))
                    };
                    expected.rflags = (expected.rflags & !(1 << 11)) | (of << 11);
                }
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_shift_emits_count_flag_contracts_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));

        let one = lower_single_op(OpKind::Shl {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        });
        assert!(
            one.windows(3).any(|bytes| bytes == [0x48, 0xD1, 0xE2]),
            "state-backed SHL must shift RDX by its immediate: {one:02X?}"
        );
        assert_eq!(
            one.iter().filter(|byte| **byte == 0x9C).count(),
            2,
            "flagful SHL must save incoming and native RFLAGS: {one:02X?}"
        );
        assert_eq!(one.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(
            one.windows(9)
                .any(|bytes| bytes == [0x48, 0x81, 0x64, 0x24, 0x18, 0x3A, 0xF7, 0xFF, 0xFF]),
            "count-one SHL must replace CF/PF/ZF/SF/OF while retaining AF: {one:02X?}"
        );

        let dynamic = lower_single_op(OpKind::Shr {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        });
        assert!(
            dynamic.windows(2).any(|bytes| bytes == [0xD2, 0xEA]),
            "state-backed SHR must use staged CL and DL: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x83, 0xE7, 0x1F]),
            "byte SHR must classify the 5-bit masked count: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x83, 0xFF, 0x08]),
            "byte SHR must classify operand-width boundary counts: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(2)
                .filter(|bytes| matches!(*bytes, [0x0F, 0x84] | [0x0F, 0x87]))
                .count()
                >= 4,
            "dynamic subword SHR must branch on zero/one/boundary/oversized counts: {dynamic:02X?}"
        );

        let suppressed = lower_single_op(OpKind::Sar {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R31),
            amount: SrcOperand::Imm(9),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        });
        assert!(
            suppressed
                .windows(4)
                .any(|bytes| bytes == [0x66, 0xC1, 0xFA, 0x09]),
            "state-backed NF SAR must use staged DX: {suppressed:02X?}"
        );
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9C).count(), 1);
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(
            suppressed
                .windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
            "word SAR must partially synchronize guest RBP: {suppressed:02X?}"
        );

        for malformed in [
            OpKind::Shl {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W128,
                flags: FlagUpdate::All,
            },
            OpKind::Shr {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Sar {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Shl {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
                ),
                "malformed state-backed shift must fail lowering"
            );
        }
        assert!(matches!(
            lower_single_hinted_op_err(
                OpKind::Shr {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rsp),
                    amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                X86OpHint::Mulx,
            ),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_shift_preserves_width_alias_count_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            kind: u8,
            dst: X86Reg,
            src: X86Reg,
            count_reg: Option<X86Reg>,
            immediate: i64,
            width: OpWidth,
            flags: FlagUpdate,
            source: u64,
            count: u64,
            status: u64,
        }

        let cases = [
            Case {
                name: "SHL RSP,RBP,0 preserves every flag",
                kind: 0,
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                count_reg: None,
                immediate: 0,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
                source: 0x8123_4567_89AB_CDEF,
                count: 0,
                status: 0x8D5,
            },
            Case {
                name: "SHR BPL,SPL,1 partial count-one flags",
                kind: 1,
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                count_reg: None,
                immediate: 1,
                width: OpWidth::W8,
                flags: FlagUpdate::All,
                source: 0x2233_4455_6677_5681,
                count: 1,
                status: 0x0D4,
            },
            Case {
                name: "SHL R16B,R31B,8 reconstructs boundary CF",
                kind: 0,
                dst: X86Reg::R16,
                src: X86Reg::R31,
                count_reg: None,
                immediate: 8,
                width: OpWidth::W8,
                flags: FlagUpdate::All,
                source: 0xFFEE_DDCC_BBAA_1381,
                count: 8,
                status: 0x8D4,
            },
            Case {
                name: "SHR R31W,R16W,17 clears oversized CF and OF",
                kind: 1,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                count_reg: None,
                immediate: 17,
                width: OpWidth::W16,
                flags: FlagUpdate::All,
                source: 0xAABB_CCDD_EEFF_8001,
                count: 17,
                status: 0x8D5,
            },
            Case {
                name: "SAR R16B,R31B,9 reconstructs oversized sign CF",
                kind: 2,
                dst: X86Reg::R16,
                src: X86Reg::R31,
                count_reg: None,
                immediate: 9,
                width: OpWidth::W8,
                flags: FlagUpdate::All,
                source: 0xFFEE_DDCC_BBAA_1381,
                count: 9,
                status: 0x0D4,
            },
            Case {
                name: "SAR R31W,R16W,SP dynamic boundary",
                kind: 2,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::Rsp),
                immediate: 0,
                width: OpWidth::W16,
                flags: FlagUpdate::All,
                source: 0xAABB_CCDD_EEFF_8001,
                count: 16,
                status: 0x8D4,
            },
            Case {
                name: "SHL R16D,R16D,R16 all aliases",
                kind: 0,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::R16),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::All,
                source: 0xAABB_CCDD_8000_0001,
                count: 0x8000_0001,
                status: 0x0D5,
            },
            Case {
                name: "NF SAR RSP,R31D,BP zero-extends and preserves flags",
                kind: 2,
                dst: X86Reg::Rsp,
                src: X86Reg::R31,
                count_reg: Some(X86Reg::Rbp),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
                source: 0xFFEE_DDCC_8000_0001,
                count: 1,
                status: 0x8D5,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let amount = case
                .count_reg
                .map_or(SrcOperand::Imm(case.immediate), |reg| {
                    SrcOperand::Reg(x86(reg))
                });
            let kind = match case.kind {
                0 => OpKind::Shl {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                },
                1 => OpKind::Shr {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                },
                2 => OpKind::Sar {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                },
                _ => unreachable!(),
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x1357_0000_2468_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            if let Some(count_reg) = case.count_reg {
                let count_idx = count_reg.gpr_index().unwrap() as usize;
                if count_idx != src_idx {
                    regs.gpr[count_idx] = case.count;
                }
            }
            regs.rflags = 0x2 | case.status;

            let mut expected = regs;
            let bits = u64::from(case.width.bits());
            let count_mask = if bits == 64 { 0x3f } else { 0x1f };
            let raw_count = case.count_reg.map_or(case.immediate as u64, |reg| {
                regs.gpr[reg.gpr_index().unwrap() as usize]
            });
            let count = raw_count & count_mask;
            let source = regs.gpr[src_idx] & case.width.mask();
            let signed_source = if source & case.width.sign_bit() != 0 {
                source | !case.width.mask()
            } else {
                source
            };
            let result = if count >= bits {
                if case.kind == 2 && (signed_source as i64) < 0 {
                    case.width.mask()
                } else {
                    0
                }
            } else {
                match case.kind {
                    0 => (source << count) & case.width.mask(),
                    1 => source >> count,
                    2 => ((signed_source as i64 >> count) as u64) & case.width.mask(),
                    _ => unreachable!(),
                }
            };
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W8 | OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W128 => unreachable!(),
            };
            if case.flags.updates_any() && count != 0 {
                let cf = match case.kind {
                    0 if count <= bits => (source >> (bits - count)) & 1,
                    0 => 0,
                    1 => (source >> (count - 1)) & 1,
                    2 => (signed_source >> (count - 1)) & 1,
                    _ => unreachable!(),
                };
                expected.rflags = (expected.rflags & !1) | cf;
                let pf = u64::from((result as u8).count_ones().is_multiple_of(2));
                expected.rflags = (expected.rflags & !(1 << 2)) | (pf << 2);
                let zf = u64::from(result == 0);
                expected.rflags = (expected.rflags & !(1 << 6)) | (zf << 6);
                let sf = u64::from(result & case.width.sign_bit() != 0);
                expected.rflags = (expected.rflags & !(1 << 7)) | (sf << 7);
                let of = if count == 1 {
                    match case.kind {
                        0 => u64::from((cf != 0) != (sf != 0)),
                        1 => u64::from(source & case.width.sign_bit() != 0),
                        2 => 0,
                        _ => unreachable!(),
                    }
                } else {
                    0
                };
                expected.rflags = (expected.rflags & !(1 << 11)) | (of << 11);
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_carry_rotate_emits_count_flag_contracts_and_rejects_malformed_shapes()
    {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let rotate_flags = FlagSet::CF.union(FlagSet::OF);

        let one = lower_single_op(OpKind::Rcl {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(rotate_flags),
        });
        assert!(
            one.windows(3).any(|bytes| bytes == [0x48, 0xD1, 0xD2]),
            "state-backed RCL must rotate RDX through incoming CF: {one:02X?}"
        );
        assert_eq!(
            one.iter().filter(|byte| **byte == 0x9C).count(),
            2,
            "flagful RCL must save incoming and native RFLAGS: {one:02X?}"
        );
        assert_eq!(one.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(
            one.windows(9)
                .any(|bytes| bytes == [0x48, 0x81, 0x64, 0x24, 0x10, 0xFE, 0xF7, 0xFF, 0xFF]),
            "count-one RCL must replace exactly CF and OF: {one:02X?}"
        );

        let dynamic = lower_single_op(OpKind::Rcr {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        });
        assert!(
            dynamic.windows(2).any(|bytes| bytes == [0xD2, 0xDA]),
            "state-backed RCR must use staged CL and DL: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x83, 0xE7, 0x1F]),
            "byte RCR must classify the 5-bit masked count: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(2)
                .filter(|bytes| *bytes == [0x0F, 0x84])
                .count()
                >= 2,
            "dynamic RCR must branch on zero and one masked counts: {dynamic:02X?}"
        );

        let suppressed = lower_single_op(OpKind::Rcl {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R31),
            amount: SrcOperand::Imm(9),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        });
        assert!(
            suppressed
                .windows(4)
                .any(|bytes| bytes == [0x66, 0xC1, 0xD2, 0x09]),
            "state-backed suppressed-output RCL must use staged DX: {suppressed:02X?}"
        );
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9C).count(), 1);
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(
            suppressed
                .windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
            "word RCL must partially synchronize guest RBP: {suppressed:02X?}"
        );

        for malformed in [
            OpKind::Rcl {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W128,
                flags: FlagUpdate::Specific(rotate_flags),
            },
            OpKind::Rcr {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
            OpKind::Rcl {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
            OpKind::Rcr {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
                ),
                "malformed state-backed carry rotate must fail lowering"
            );
        }
        assert!(matches!(
            lower_single_hinted_op_err(
                OpKind::Rcl {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rsp),
                    amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                X86OpHint::Mulx,
            ),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_carry_rotate_preserves_alias_count_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;
        let rotate_flags = FlagSet::CF.union(FlagSet::OF);

        struct Case {
            name: &'static str,
            right: bool,
            dst: X86Reg,
            src: X86Reg,
            count_reg: Option<X86Reg>,
            immediate: i64,
            width: OpWidth,
            flags: FlagUpdate,
            source: u64,
            count: u64,
            status: u64,
        }

        let cases = [
            Case {
                name: "RCL RSP,RBP,0 preserves every flag",
                right: false,
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                count_reg: None,
                immediate: 0,
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0x8123_4567_89AB_CDEF,
                count: 0,
                status: 0x8D5,
            },
            Case {
                name: "RCL BPL,SPL,1 consumes incoming CF",
                right: false,
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                count_reg: None,
                immediate: 1,
                width: OpWidth::W8,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0x2233_4455_6677_5642,
                count: 1,
                status: 0x0D5,
            },
            Case {
                name: "RCR R16B,R31B,10 effective one preserves raw-multi OF",
                right: true,
                dst: X86Reg::R16,
                src: X86Reg::R31,
                count_reg: None,
                immediate: 10,
                width: OpWidth::W8,
                flags: FlagUpdate::All,
                source: 0xFFEE_DDCC_BBAA_1301,
                count: 10,
                status: 0x8D4,
            },
            Case {
                name: "RCL R31B,R16B,SP full through-carry period",
                right: false,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::Rsp),
                immediate: 0,
                width: OpWidth::W8,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0xAABB_CCDD_EEFF_13A5,
                count: 9,
                status: 0x8D5,
            },
            Case {
                name: "RCL R31W,R16W,SP effective one raw multi",
                right: false,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::Rsp),
                immediate: 0,
                width: OpWidth::W16,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0xAABB_CCDD_EEFF_4000,
                count: 18,
                status: 0x8D4,
            },
            Case {
                name: "RCR R16D,R16D,R16 all aliases",
                right: true,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::R16),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::Specific(rotate_flags),
                source: 0xAABB_CCDD_8000_0001,
                count: 0x8000_0001,
                status: 0x0D5,
            },
            Case {
                name: "suppressed RCR RSP,R31D,BP consumes CF and zero-extends",
                right: true,
                dst: X86Reg::Rsp,
                src: X86Reg::R31,
                count_reg: Some(X86Reg::Rbp),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
                source: 0xFFEE_DDCC_8000_0001,
                count: 1,
                status: 0x8D5,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let amount = case
                .count_reg
                .map_or(SrcOperand::Imm(case.immediate), |reg| {
                    SrcOperand::Reg(x86(reg))
                });
            let kind = if case.right {
                OpKind::Rcr {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                }
            } else {
                OpKind::Rcl {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                }
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x1357_0000_2468_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            if let Some(count_reg) = case.count_reg {
                let count_idx = count_reg.gpr_index().unwrap() as usize;
                if count_idx != src_idx {
                    regs.gpr[count_idx] = case.count;
                }
            }
            regs.rflags = 0x2 | case.status;

            let mut expected = regs;
            let bits = u64::from(case.width.bits());
            let count_mask = if bits == 64 { 0x3f } else { 0x1f };
            let raw_count = case.count_reg.map_or(case.immediate as u64, |reg| {
                regs.gpr[reg.gpr_index().unwrap() as usize]
            });
            let masked = raw_count & count_mask;
            let effective = masked % (bits + 1);
            let source = regs.gpr[src_idx] & case.width.mask();
            let mut result = source;
            let mut carry = expected.rflags & 1 != 0;
            for _ in 0..effective {
                if case.right {
                    let next = result & 1 != 0;
                    result = (result >> 1) | (u64::from(carry) << (bits - 1));
                    carry = next;
                } else {
                    let next = result & case.width.sign_bit() != 0;
                    result = ((result << 1) | u64::from(carry)) & case.width.mask();
                    carry = next;
                }
            }
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W8 | OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W128 => unreachable!(),
            };
            if case.flags.updates_any() && effective != 0 {
                expected.rflags = (expected.rflags & !1) | u64::from(carry);
                if masked == 1 {
                    let msb = result & case.width.sign_bit() != 0;
                    let of = if case.right {
                        let second = result & (case.width.sign_bit() >> 1) != 0;
                        msb != second
                    } else {
                        msb != carry
                    };
                    expected.rflags = (expected.rflags & !(1 << 11)) | (u64::from(of) << 11);
                }
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_double_shift_emits_guarded_flag_contracts_and_rejects_malformed_shapes()
     {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));

        let one = lower_single_op(OpKind::Shld {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            amount: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        });
        assert!(
            one.windows(5)
                .any(|bytes| bytes == [0x48, 0x0F, 0xA4, 0xF2, 0x01]),
            "state-backed SHLD must shift staged RDX with RSI: {one:02X?}"
        );
        assert_eq!(one.iter().filter(|byte| **byte == 0x9C).count(), 2);
        assert_eq!(one.iter().filter(|byte| **byte == 0x9D).count(), 1);

        let dynamic = lower_single_op(OpKind::Shrd {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        });
        assert!(
            dynamic
                .windows(4)
                .any(|bytes| bytes == [0x66, 0x0F, 0xAD, 0xF2]),
            "state-backed SHRD must use staged DX, SI, and CL: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x83, 0xFF, 0x10]),
            "word SHRD must guard counts above the defined width: {dynamic:02X?}"
        );
        assert!(
            dynamic.windows(2).any(|bytes| bytes == [0x0F, 0x87]),
            "word SHRD must branch around undefined host counts: {dynamic:02X?}"
        );
        assert!(
            dynamic
                .windows(9)
                .any(|bytes| bytes == [0x48, 0x81, 0x64, 0x24, 0x18, 0xFF, 0xF7, 0xFF, 0xFF]),
            "multi-bit SHRD must clear deterministic OF: {dynamic:02X?}"
        );

        let suppressed = lower_single_op(OpKind::Shld {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R31),
            amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        });
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9C).count(), 1);
        assert_eq!(suppressed.iter().filter(|byte| **byte == 0x9D).count(), 2);
        assert!(
            suppressed
                .windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
            "word SHLD must partially synchronize guest RBP: {suppressed:02X?}"
        );

        let ndd = lower_single_op(OpKind::X86NddDoubleShift {
            dst: x86(X86Reg::R16),
            base: x86(X86Reg::Rsp),
            fill: x86(X86Reg::R31),
            amount: SrcOperand::Imm(4),
            width: OpWidth::W64,
            left: true,
            flags: FlagUpdate::All,
        });
        assert!(
            ndd.windows(5)
                .any(|bytes| bytes == [0x48, 0x0F, 0xA4, 0xF2, 0x04]),
            "state-backed NDD SHLD must shift staged base RDX with fill RSI: {ndd:02X?}"
        );
        assert!(
            ndd.windows(4)
                .any(|bytes| bytes == [0x48, 0x8B, 0x50, 0x20]),
            "state-backed NDD SHLD must load guest RSP as its independent base: {ndd:02X?}"
        );

        let guarded_ndd = lower_single_op(OpKind::X86NddDoubleShift {
            dst: x86(X86Reg::Rdx),
            base: x86(X86Reg::Rax),
            fill: x86(X86Reg::Rbx),
            amount: SrcOperand::Imm(17),
            width: OpWidth::W16,
            left: true,
            flags: FlagUpdate::All,
        });
        assert!(
            !guarded_ndd
                .windows(5)
                .any(|bytes| bytes == [0x66, 0x0F, 0xA4, 0xF2, 0x11]),
            "W16 NDD count above the width must not execute the host instruction: {guarded_ndd:02X?}"
        );
        assert_eq!(guarded_ndd.iter().filter(|byte| **byte == 0x9C).count(), 1);
        assert_eq!(guarded_ndd.iter().filter(|byte| **byte == 0x9D).count(), 1);

        let guarded_legacy = lower_single_op(OpKind::Shld {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rbx),
            amount: SrcOperand::Imm(17),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        });
        assert!(
            !guarded_legacy
                .windows(5)
                .any(|bytes| bytes == [0x66, 0x0F, 0xA4, 0xF2, 0x11]),
            "W16 legacy count above the width must not execute the host instruction: {guarded_legacy:02X?}"
        );

        let dynamic_legacy = lower_single_op(OpKind::Shrd {
            dst: x86(X86Reg::Rax),
            src: x86(X86Reg::Rbx),
            amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        });
        assert!(
            dynamic_legacy
                .windows(4)
                .any(|bytes| bytes == [0x66, 0x0F, 0xAD, 0xF2]),
            "dynamic W16 legacy SHRD must use the staged register form: {dynamic_legacy:02X?}"
        );
        assert!(
            dynamic_legacy
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x83, 0xFF, 0x10]),
            "dynamic W16 legacy SHRD must guard counts above the width: {dynamic_legacy:02X?}"
        );

        for malformed in [
            OpKind::Shld {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
            OpKind::Shrd {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Shld {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Shrd {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::R16),
                base: x86(X86Reg::Rsp),
                fill: x86(X86Reg::R31),
                amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::R16),
                base: x86(X86Reg::Rsp),
                fill: x86(X86Reg::R31),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                left: false,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
                ),
                "malformed state-backed double shift must fail lowering"
            );
        }
        assert!(matches!(
            lower_single_hinted_op_err(
                OpKind::Shld {
                    dst: x86(X86Reg::R16),
                    src: x86(X86Reg::Rsp),
                    amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
                X86OpHint::Mulx,
            ),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_double_shift_preserves_alias_count_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            left: bool,
            dst: X86Reg,
            src: X86Reg,
            count_reg: Option<X86Reg>,
            immediate: i64,
            width: OpWidth,
            flags: FlagUpdate,
            base: u64,
            fill: u64,
            count: u64,
            status: u64,
        }

        let cases = [
            Case {
                name: "SHLD RSP,RBP,0 preserves every flag",
                left: true,
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                count_reg: None,
                immediate: 0,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
                base: 0x8123_4567_89AB_CDEF,
                fill: 0x1020_3040_5060_7080,
                count: 0,
                status: 0x8D5,
            },
            Case {
                name: "SHLD BP,SP,1 partial count-one flags",
                left: true,
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                count_reg: None,
                immediate: 1,
                width: OpWidth::W16,
                flags: FlagUpdate::All,
                base: 0x3344_5566_8765_4000,
                fill: 0x2233_4455_6677_8001,
                count: 1,
                status: 0x8D5,
            },
            Case {
                name: "SHRD R16W,R31W,17 immediate undefined no-op",
                left: false,
                dst: X86Reg::R16,
                src: X86Reg::R31,
                count_reg: None,
                immediate: 17,
                width: OpWidth::W16,
                flags: FlagUpdate::All,
                base: 0xAABB_CCDD_EEFF_1357,
                fill: 0xFFEE_DDCC_BBAA_2468,
                count: 17,
                status: 0x0D5,
            },
            Case {
                name: "SHLD R31W,R16W,SP dynamic undefined no-op",
                left: true,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::Rsp),
                immediate: 0,
                width: OpWidth::W16,
                flags: FlagUpdate::All,
                base: 0xFFEE_DDCC_BBAA_1357,
                fill: 0xAABB_CCDD_EEFF_2468,
                count: 17,
                status: 0x8D5,
            },
            Case {
                name: "SHRD R16D all operands alias",
                left: false,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                count_reg: Some(X86Reg::R16),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::All,
                base: 0xAABB_CCDD_8000_0001,
                fill: 0,
                count: 0,
                status: 0x0D5,
            },
            Case {
                name: "NF SHRD RSP,R31D,BP preserves flags and zero-extends",
                left: false,
                dst: X86Reg::Rsp,
                src: X86Reg::R31,
                count_reg: Some(X86Reg::Rbp),
                immediate: 0,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
                base: 0x2233_4455_8000_0001,
                fill: 0xFFEE_DDCC_2468_1357,
                count: 4,
                status: 0x8D5,
            },
            Case {
                name: "SHRD RAX,RDX,BP stages only the count",
                left: false,
                dst: X86Reg::Rax,
                src: X86Reg::Rdx,
                count_reg: Some(X86Reg::Rbp),
                immediate: 0,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
                base: 0x8123_4567_89AB_CDEF,
                fill: 0x1020_3040_5060_7080,
                count: 9,
                status: 0x8D5,
            },
            Case {
                name: "SHLD RBX,R31,7 stages only the fill",
                left: true,
                dst: X86Reg::Rbx,
                src: X86Reg::R31,
                count_reg: None,
                immediate: 7,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
                base: 0x0123_4567_89AB_CDEF,
                fill: 0xFEDC_BA98_7654_3210,
                count: 7,
                status: 0x0D5,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let amount = case
                .count_reg
                .map_or(SrcOperand::Imm(case.immediate), |reg| {
                    SrcOperand::Reg(x86(reg))
                });
            let kind = if case.left {
                OpKind::Shld {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                }
            } else {
                OpKind::Shrd {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    amount,
                    width: case.width,
                    flags: case.flags,
                }
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x1357_0000_2468_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            regs.gpr[dst_idx] = case.base;
            if src_idx != dst_idx {
                regs.gpr[src_idx] = case.fill;
            }
            if let Some(count_reg) = case.count_reg {
                let count_idx = count_reg.gpr_index().unwrap() as usize;
                if count_idx != dst_idx && count_idx != src_idx {
                    regs.gpr[count_idx] = case.count;
                }
            }
            regs.rflags = 0x2 | case.status;

            let mut expected = regs;
            let bits = u64::from(case.width.bits());
            let count_mask = if bits == 64 { 0x3f } else { 0x1f };
            let raw_count = case.count_reg.map_or(case.immediate as u64, |reg| {
                regs.gpr[reg.gpr_index().unwrap() as usize]
            });
            let masked = raw_count & count_mask;
            let base = regs.gpr[dst_idx] & case.width.mask();
            let fill = regs.gpr[src_idx] & case.width.mask();
            let defined = masked != 0 && masked <= bits;
            let result = if !defined {
                base
            } else if case.left {
                ((base << masked) | (fill >> (bits - masked))) & case.width.mask()
            } else {
                ((base >> masked) | (fill << (bits - masked))) & case.width.mask()
            };
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W8 | OpWidth::W128 => unreachable!(),
            };
            if case.flags.updates_any() && defined {
                let cf = if case.left {
                    (base >> (bits - masked)) & 1
                } else {
                    (base >> (masked - 1)) & 1
                };
                expected.rflags = (expected.rflags & !1) | cf;
                let pf = u64::from((result as u8).count_ones().is_multiple_of(2));
                expected.rflags = (expected.rflags & !(1 << 2)) | (pf << 2);
                expected.rflags = (expected.rflags & !(1 << 6)) | (u64::from(result == 0) << 6);
                expected.rflags = (expected.rflags & !(1 << 7))
                    | (u64::from(result & case.width.sign_bit() != 0) << 7);
                let of = u64::from(masked == 1 && ((result ^ base) & case.width.sign_bit()) != 0);
                expected.rflags = (expected.rflags & !(1 << 11)) | (of << 11);
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_count_emits_flag_contracts_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let flagless = lower_single_op(OpKind::X86Count {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W32,
            kind: X86CountKind::Lzcnt,
            flags: FlagUpdate::None,
        });
        assert!(
            flagless.contains(&0x9C) && flagless.contains(&0x9D),
            "APX NF LZCNT must preserve RFLAGS: {flagless:02X?}"
        );
        assert!(
            flagless
                .windows(4)
                .any(|bytes| bytes == [0xF3, 0x0F, 0xBD, 0xD2]),
            "dword LZCNT must count EDX into EDX: {flagless:02X?}"
        );
        assert!(
            flagless
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0xF8, 0x00, 0x00, 0x00]),
            "dword LZCNT must fully commit GuestRegs.gpr[31]: {flagless:02X?}"
        );

        let popcnt_all = lower_single_op(OpKind::X86Count {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W16,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        });
        assert!(
            !popcnt_all.contains(&0x9C) && !popcnt_all.contains(&0x9D),
            "flag-setting POPCNT must leave native flags live: {popcnt_all:02X?}"
        );
        assert!(
            popcnt_all
                .windows(5)
                .any(|bytes| bytes == [0xF3, 0x66, 0x0F, 0xB8, 0xD2]),
            "word POPCNT must count DX into DX: {popcnt_all:02X?}"
        );
        assert!(
            popcnt_all
                .windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
            "word POPCNT must partially synchronize guest RBP: {popcnt_all:02X?}"
        );

        let tzcnt_flags = lower_single_op(OpKind::X86Count {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            kind: X86CountKind::Tzcnt,
            flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
        });
        assert_eq!(
            tzcnt_flags.iter().filter(|byte| **byte == 0x9C).count(),
            2,
            "state-backed TZCNT must save old and new RFLAGS: {tzcnt_flags:02X?}"
        );
        assert_eq!(tzcnt_flags.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(tzcnt_flags.contains(&0x41), "TZCNT must merge CF and ZF");

        for malformed in [
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W8,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::All,
            },
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                kind: X86CountKind::Lzcnt,
                flags: FlagUpdate::All,
            },
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                width: OpWidth::W64,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::None,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed count must fail lowering"
            );
        }

        let hinted = OpKind::X86Count {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_count_preserves_width_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            dst: X86Reg,
            src: X86Reg,
            source: u64,
            width: OpWidth,
            kind: X86CountKind,
            flags: FlagUpdate,
        }

        let cases = [
            Case {
                name: "POPCNT BP,SP partial flag-setting destination",
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                source: 0x2233_4455_6677_5678,
                width: OpWidth::W16,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::All,
            },
            Case {
                name: "TZCNT RSP,RBP full flag-merge destination",
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                source: 0,
                width: OpWidth::W64,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
            },
            Case {
                name: "NF LZCNT R31D,R16D zero-extending destination",
                dst: X86Reg::R31,
                src: X86Reg::R16,
                source: 0xAABB_CCDD_8000_0000,
                width: OpWidth::W32,
                kind: X86CountKind::Lzcnt,
                flags: FlagUpdate::None,
            },
            Case {
                name: "POPCNT R16D in-place selective ZF destination",
                dst: X86Reg::R16,
                src: X86Reg::R16,
                source: 0,
                width: OpWidth::W32,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
            Case {
                name: "NF TZCNT R16W,SP partial destination",
                dst: X86Reg::R16,
                src: X86Reg::Rsp,
                source: 0x2233_4455_6677_0080,
                width: OpWidth::W16,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::None,
            },
        ];

        let count_result = |source: u64, width: OpWidth, kind: X86CountKind| {
            let value = source & width.mask();
            match kind {
                X86CountKind::Popcnt => u64::from(value.count_ones()),
                X86CountKind::Tzcnt => u64::from(if value == 0 {
                    width.bits()
                } else {
                    value.trailing_zeros()
                }),
                X86CountKind::Lzcnt => u64::from(if value == 0 {
                    width.bits()
                } else {
                    value.leading_zeros() - (64 - width.bits())
                }),
            }
        };
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::X86Count {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                    kind: case.kind,
                    flags: case.flags,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            regs.gpr[4] = 0x2233_4455_6677_5678;
            regs.gpr[5] = 0x3344_5566_8765_9ABC;
            regs.gpr[16] = 0xAABB_CCDD_EEFF_7788;
            regs.gpr[31] = 0xFFEE_DDCC_BBAA_1357;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            regs.rflags = STATUS_MASK;

            let mut expected = regs;
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let source = regs.gpr[src_idx];
            let result = count_result(source, case.width, case.kind);
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W8 | OpWidth::W128 => unreachable!(),
            };
            let requested = case.flags.as_set();
            if !requested.is_empty() {
                let new_status = match case.kind {
                    X86CountKind::Popcnt => u64::from(source & case.width.mask() == 0) << 6,
                    X86CountKind::Tzcnt | X86CountKind::Lzcnt => {
                        u64::from(source & case.width.mask() == 0) | (u64::from(result == 0) << 6)
                    }
                };
                let requested_mask = X86_64Lowerer::x86_status_rflags_mask(requested) as u64;
                expected.rflags =
                    (expected.rflags & !requested_mask) | (new_status & requested_mask);
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_bit_scan_restores_zero_destination_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let zf_only = FlagUpdate::Specific(FlagSet::ZF);

        let flagful = lower_single_op(OpKind::Bsf {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            flags: zf_only,
        });
        assert!(
            flagful
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x0F, 0xBC, 0xD2]),
            "state-backed BSF must scan RDX in place: {flagful:02X?}"
        );
        assert!(
            flagful.windows(2).any(|bytes| bytes == [0x0F, 0x85]),
            "state-backed BSF must branch around zero-source restoration: {flagful:02X?}"
        );
        assert_eq!(
            flagful
                .windows(4)
                .filter(|bytes| *bytes == [0x48, 0x8B, 0x50, 0x28])
                .count(),
            1,
            "BSF must load RBP source once: {flagful:02X?}"
        );
        assert_eq!(
            flagful
                .windows(4)
                .filter(|bytes| *bytes == [0x48, 0x8B, 0x50, 0x20])
                .count(),
            1,
            "zero-source BSF must restore the retained RSP destination: {flagful:02X?}"
        );
        assert_eq!(
            flagful.iter().filter(|byte| **byte == 0x9C).count(),
            2,
            "ZF-only BSF must save old and new RFLAGS: {flagful:02X?}"
        );
        assert_eq!(flagful.iter().filter(|byte| **byte == 0x9D).count(), 1);

        let flagless = lower_single_op(OpKind::Bsr {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        });
        assert!(
            flagless.windows(3).any(|bytes| bytes == [0x0F, 0xBD, 0xD2]),
            "state-backed BSR must scan EDX in place: {flagless:02X?}"
        );
        assert_eq!(
            flagless.iter().filter(|byte| **byte == 0x9C).count(),
            1,
            "flag-suppressed BSR must save RFLAGS once: {flagless:02X?}"
        );
        assert_eq!(flagless.iter().filter(|byte| **byte == 0x9D).count(), 1);
        assert!(
            flagless
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0xF8, 0x00, 0x00, 0x00]),
            "dword BSR must fully commit GuestRegs.gpr[31]: {flagless:02X?}"
        );

        for malformed in [
            OpKind::Bsf {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W8,
                flags: zf_only,
            },
            OpKind::Bsr {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Bsf {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                width: OpWidth::W64,
                flags: zf_only,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed bit scan must fail lowering"
            );
        }

        let hinted = OpKind::Bsr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            flags: zf_only,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_bit_scan_preserves_width_zero_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            dst: X86Reg,
            src: X86Reg,
            source: u64,
            width: OpWidth,
            reverse: bool,
            flags: FlagUpdate,
        }

        let zf_only = FlagUpdate::Specific(FlagSet::ZF);
        let cases = [
            Case {
                name: "BSF BP,SP partial destination",
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                source: 0x2233_4455_6677_8000,
                width: OpWidth::W16,
                reverse: false,
                flags: zf_only,
            },
            Case {
                name: "BSR RSP,RBP full destination",
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                source: 1u64 << 63,
                width: OpWidth::W64,
                reverse: true,
                flags: zf_only,
            },
            Case {
                name: "BSF R31,R16 extended destination",
                dst: X86Reg::R31,
                src: X86Reg::R16,
                source: 0x100,
                width: OpWidth::W64,
                reverse: false,
                flags: zf_only,
            },
            Case {
                name: "flag-suppressed zero BSR R16D,R16D alias",
                dst: X86Reg::R16,
                src: X86Reg::R16,
                source: 0,
                width: OpWidth::W32,
                reverse: true,
                flags: FlagUpdate::None,
            },
            Case {
                name: "zero BSF R16W,SP partial destination",
                dst: X86Reg::R16,
                src: X86Reg::Rsp,
                source: 0,
                width: OpWidth::W16,
                reverse: false,
                flags: zf_only,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let kind = if case.reverse {
                OpKind::Bsr {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                    flags: case.flags,
                }
            } else {
                OpKind::Bsf {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                    flags: case.flags,
                }
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            regs.gpr[4] = 0x2233_4455_6677_5678;
            regs.gpr[5] = 0x3344_5566_8765_9ABC;
            regs.gpr[16] = 0xAABB_CCDD_EEFF_7788;
            regs.gpr[31] = 0xFFEE_DDCC_BBAA_1357;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            regs.rflags = STATUS_MASK;

            let mut expected = regs;
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let value = case.source & case.width.mask();
            let result = if value == 0 {
                None
            } else if case.reverse {
                Some(u64::from(case.width.bits() - 1 - value.leading_zeros()))
            } else {
                Some(u64::from(value.trailing_zeros()))
            };
            if let Some(result) = result {
                expected.gpr[dst_idx] = match case.width {
                    OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                    OpWidth::W32 | OpWidth::W64 => result,
                    OpWidth::W8 | OpWidth::W128 => unreachable!(),
                };
            }
            if case.flags.updates_any() {
                let zf = u64::from(value == 0) << 6;
                expected.rflags = (expected.rflags & !(1 << 6)) | zf;
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_neg_emits_flag_contracts_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let flagless = lower_single_op(OpKind::Neg {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R16),
            width: OpWidth::W8,
            flags: FlagUpdate::None,
        });
        assert!(
            flagless.contains(&0x9C) && flagless.contains(&0x9D),
            "APX NF Neg must preserve RFLAGS: {flagless:02X?}"
        );
        assert!(
            flagless.windows(2).any(|bytes| bytes == [0xF6, 0xDA]),
            "byte Neg must negate DL: {flagless:02X?}"
        );
        assert!(
            flagless.windows(3).any(|bytes| bytes == [0x88, 0x55, 0x00]),
            "byte Neg must partially synchronize guest RBP: {flagless:02X?}"
        );

        let flagful = lower_single_op(OpKind::Neg {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        });
        assert!(
            !flagful.contains(&0x9C) && !flagful.contains(&0x9D),
            "flag-setting Neg must leave native flags live: {flagful:02X?}"
        );
        assert!(
            flagful.windows(2).any(|bytes| bytes == [0xF7, 0xDA]),
            "dword Neg must negate EDX: {flagful:02X?}"
        );
        assert!(
            flagful
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "dword Neg must fully commit GuestRegs.gpr[16]: {flagful:02X?}"
        );

        for malformed in [
            OpKind::Neg {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W128,
                flags: FlagUpdate::All,
            },
            OpKind::Neg {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
            OpKind::Neg {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed Neg must fail lowering"
            );
        }

        let hinted = OpKind::Neg {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_neg_preserves_width_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            dst: X86Reg,
            src: X86Reg,
            width: OpWidth,
            flags: FlagUpdate,
        }

        let cases = [
            Case {
                name: "NEG BPL,R16B partial flag-setting destination",
                dst: X86Reg::Rbp,
                src: X86Reg::R16,
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
            Case {
                name: "NF NEG R16W,SP partial destination",
                dst: X86Reg::R16,
                src: X86Reg::Rsp,
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            Case {
                name: "NEG RSP in-place full destination",
                dst: X86Reg::Rsp,
                src: X86Reg::Rsp,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            Case {
                name: "NEG R31D,EBP zero-extending destination",
                dst: X86Reg::R31,
                src: X86Reg::Rbp,
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            Case {
                name: "NF NEG R16D in-place zero-extending destination",
                dst: X86Reg::R16,
                src: X86Reg::R16,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ];

        let neg_status = |source: u64, width: OpWidth| {
            let mask = width.mask();
            let source = source & mask;
            let result = source.wrapping_neg() & mask;
            let sign_bit = width.sign_bit();
            u64::from(source != 0)
                | (u64::from((result as u8).count_ones().is_multiple_of(2)) << 2)
                | (u64::from(source & 0xF != 0) << 4)
                | (u64::from(result == 0) << 6)
                | (u64::from(result & sign_bit != 0) << 7)
                | (u64::from(source == sign_bit) << 11)
        };
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Neg {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                    flags: case.flags,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            regs.gpr[4] = 0x2233_4455_6677_5678;
            regs.gpr[5] = 0x3344_5566_8765_9ABC;
            regs.gpr[16] = 0xAABB_CCDD_EEFF_7788;
            regs.gpr[31] = 0xFFEE_DDCC_BBAA_1357;
            regs.rflags = STATUS_MASK;
            let mut expected = regs;
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            let source = regs.gpr[src_idx];
            let result = source.wrapping_neg() & case.width.mask();
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W8 | OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W128 => unreachable!(),
            };
            if case.flags.updates_any() {
                expected.rflags = (expected.rflags & !STATUS_MASK) | neg_status(source, case.width);
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_inc_dec_emits_flag_contracts_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let flagless_inc = lower_single_op(OpKind::Inc {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R16),
            width: OpWidth::W8,
            flags: FlagUpdate::None,
        });
        assert!(
            flagless_inc.contains(&0x9C) && flagless_inc.contains(&0x9D),
            "APX NF Inc must preserve RFLAGS: {flagless_inc:02X?}"
        );
        assert!(
            flagless_inc.windows(2).any(|bytes| bytes == [0xFE, 0xC2]),
            "byte Inc must increment DL: {flagless_inc:02X?}"
        );
        assert!(
            flagless_inc
                .windows(3)
                .any(|bytes| bytes == [0x88, 0x55, 0x00]),
            "byte Inc must partially synchronize guest RBP: {flagless_inc:02X?}"
        );

        let flagful_dec = lower_single_op(OpKind::Dec {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        });
        assert!(
            !flagful_dec.contains(&0x9C) && !flagful_dec.contains(&0x9D),
            "flag-setting Dec must leave native flags live: {flagful_dec:02X?}"
        );
        assert!(
            flagful_dec.windows(2).any(|bytes| bytes == [0xFF, 0xCA]),
            "dword Dec must decrement EDX: {flagful_dec:02X?}"
        );
        assert!(
            flagful_dec
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "dword Dec must fully commit GuestRegs.gpr[16]: {flagful_dec:02X?}"
        );

        for malformed in [
            OpKind::Inc {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W128,
                flags: FlagUpdate::All,
            },
            OpKind::Dec {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
            OpKind::Inc {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed Inc/Dec must fail lowering"
            );
        }

        let hinted = OpKind::Dec {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_inc_dec_preserve_width_and_flag_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            decrement: bool,
            dst: X86Reg,
            src: X86Reg,
            width: OpWidth,
            flags: FlagUpdate,
        }

        let cases = [
            Case {
                name: "INC BPL,R16B partial flag-setting destination",
                decrement: false,
                dst: X86Reg::Rbp,
                src: X86Reg::R16,
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
            Case {
                name: "NF DEC R16W,SP partial destination",
                decrement: true,
                dst: X86Reg::R16,
                src: X86Reg::Rsp,
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            Case {
                name: "DEC RSP in-place full destination",
                decrement: true,
                dst: X86Reg::Rsp,
                src: X86Reg::Rsp,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            Case {
                name: "INC R31D,EBP zero-extending destination",
                decrement: false,
                dst: X86Reg::R31,
                src: X86Reg::Rbp,
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            Case {
                name: "NF INC R16D in-place zero-extending destination",
                decrement: false,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ];

        let inc_dec_status = |source: u64, width: OpWidth, decrement: bool, incoming: u64| {
            let mask = width.mask();
            let source = source & mask;
            let result = if decrement {
                source.wrapping_sub(1) & mask
            } else {
                source.wrapping_add(1) & mask
            };
            let sign_bit = width.sign_bit();
            (incoming & 1)
                | (u64::from((result as u8).count_ones().is_multiple_of(2)) << 2)
                | (u64::from(if decrement {
                    source & 0xF == 0
                } else {
                    source & 0xF == 0xF
                }) << 4)
                | (u64::from(result == 0) << 6)
                | (u64::from(result & sign_bit != 0) << 7)
                | (u64::from(if decrement {
                    source == sign_bit
                } else {
                    source == sign_bit - 1
                }) << 11)
        };
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let kind = if case.decrement {
                OpKind::Dec {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                    flags: case.flags,
                }
            } else {
                OpKind::Inc {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                    flags: case.flags,
                }
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            regs.gpr[4] = 0x2233_4455_6677_5678;
            regs.gpr[5] = 0x3344_5566_8765_9ABD;
            regs.gpr[16] = 0xAABB_CCDD_EEFF_778A;
            regs.gpr[31] = 0xFFEE_DDCC_BBAA_1357;
            regs.rflags = STATUS_MASK;
            let mut expected = regs;
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            let source = regs.gpr[src_idx];
            let result = if case.decrement {
                source.wrapping_sub(1) & case.width.mask()
            } else {
                source.wrapping_add(1) & case.width.mask()
            };
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W8 | OpWidth::W16 => (regs.gpr[dst_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W128 => unreachable!(),
            };
            if case.flags.updates_any() {
                expected.rflags = (expected.rflags & !STATUS_MASK)
                    | inc_dec_status(source, case.width, case.decrement, expected.rflags);
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_not_emits_slot_commits_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let byte = lower_single_op(OpKind::Not {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R16),
            width: OpWidth::W8,
        });
        assert!(
            byte.windows(2).any(|bytes| bytes == [0xF6, 0xD2]),
            "byte Not must complement DL: {byte:02X?}"
        );
        assert!(
            byte.windows(3).any(|bytes| bytes == [0x88, 0x55, 0x00]),
            "byte Not must partially synchronize guest RBP: {byte:02X?}"
        );

        let dword = lower_single_op(OpKind::Not {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W32,
        });
        assert!(
            dword.windows(2).any(|bytes| bytes == [0xF7, 0xD2]),
            "dword Not must complement EDX: {dword:02X?}"
        );
        assert!(
            dword
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "dword Not must fully commit GuestRegs.gpr[16]: {dword:02X?}"
        );

        for malformed in [
            OpKind::Not {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W128,
            },
            OpKind::Not {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                width: OpWidth::W64,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed Not must fail lowering"
            );
        }

        let hinted = OpKind::Not {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_not_preserves_widths_flags_and_host_stack() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            dst: X86Reg,
            src: X86Reg,
            width: OpWidth,
        }

        let cases = [
            Case {
                name: "NOT BPL,R16B partial destination",
                dst: X86Reg::Rbp,
                src: X86Reg::R16,
                width: OpWidth::W8,
            },
            Case {
                name: "NOT R16W,SP partial destination",
                dst: X86Reg::R16,
                src: X86Reg::Rsp,
                width: OpWidth::W16,
            },
            Case {
                name: "NOT RSP in-place full destination",
                dst: X86Reg::Rsp,
                src: X86Reg::Rsp,
                width: OpWidth::W64,
            },
            Case {
                name: "NOT R31D,EBP zero-extending destination",
                dst: X86Reg::R31,
                src: X86Reg::Rbp,
                width: OpWidth::W32,
            },
            Case {
                name: "NOT R16D in-place zero-extending destination",
                dst: X86Reg::R16,
                src: X86Reg::R16,
                width: OpWidth::W32,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Not {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            regs.rflags = STATUS;
            let mut expected = regs;
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            let source = regs.gpr[src_idx];
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W8 => (regs.gpr[dst_idx] & !0xFF) | ((!source) & 0xFF),
                OpWidth::W16 => (regs.gpr[dst_idx] & !0xFFFF) | ((!source) & 0xFFFF),
                OpWidth::W32 => u64::from(!(source as u32)),
                OpWidth::W64 => !source,
                _ => unreachable!(),
            };

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(regs.rflags & STATUS, STATUS, "{} status flags", case.name);
        }
    }
    #[test]
    fn lower_state_backed_gpr_bswap_emits_slot_commits_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let word = lower_single_op(OpKind::Bswap {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R16),
            width: OpWidth::W16,
        });
        assert!(word.contains(&0x9C), "word Bswap must save RFLAGS");
        assert!(word.contains(&0x9D), "word Bswap must restore RFLAGS");
        assert!(
            word.windows(4)
                .any(|bytes| bytes == [0x66, 0xC1, 0xC2, 0x08]),
            "word Bswap must rotate DX by one byte: {word:02X?}"
        );
        assert!(
            word.windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
            "word Bswap must partially synchronize guest RBP: {word:02X?}"
        );

        let dword = lower_single_op(OpKind::Bswap {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W32,
        });
        assert!(
            dword.windows(2).any(|bytes| bytes == [0x0F, 0xCA]),
            "dword Bswap must operate on EDX: {dword:02X?}"
        );
        assert!(
            dword
                .windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "dword Bswap must fully commit GuestRegs.gpr[16]: {dword:02X?}"
        );

        for malformed in [
            OpKind::Bswap {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W8,
            },
            OpKind::Bswap {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                width: OpWidth::W64,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed Bswap must fail lowering"
            );
        }

        let hinted = OpKind::Bswap {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_bswap_preserves_widths_flags_and_host_stack() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            dst: X86Reg,
            src: X86Reg,
            width: OpWidth,
        }

        let cases = [
            Case {
                name: "MOVBE BP,R16W partial destination",
                dst: X86Reg::Rbp,
                src: X86Reg::R16,
                width: OpWidth::W16,
            },
            Case {
                name: "MOVBE R16D,ESP zero-extending destination",
                dst: X86Reg::R16,
                src: X86Reg::Rsp,
                width: OpWidth::W32,
            },
            Case {
                name: "BSWAP RSP full in-place destination",
                dst: X86Reg::Rsp,
                src: X86Reg::Rsp,
                width: OpWidth::W64,
            },
            Case {
                name: "MOVBE R31,RBP full state-to-state copy",
                dst: X86Reg::R31,
                src: X86Reg::Rbp,
                width: OpWidth::W64,
            },
            Case {
                name: "BSWAP R16D zero-extending in-place destination",
                dst: X86Reg::R16,
                src: X86Reg::R16,
                width: OpWidth::W32,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Bswap {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            regs.rflags = STATUS;
            let mut expected = regs;
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            let source = regs.gpr[src_idx];
            expected.gpr[dst_idx] = match case.width {
                OpWidth::W16 => {
                    (regs.gpr[dst_idx] & !0xFFFF) | u64::from((source as u16).swap_bytes())
                }
                OpWidth::W32 => u64::from((source as u32).swap_bytes()),
                OpWidth::W64 => source.swap_bytes(),
                _ => unreachable!(),
            };

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(regs.rflags & STATUS, STATUS, "{} status flags", case.name);
        }
    }
    #[test]
    fn lower_state_backed_gpr_xchg_emits_slot_commits_and_rejects_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let word = lower_single_op(OpKind::Xchg {
            reg1: x86(X86Reg::Rsp),
            reg2: x86(X86Reg::R16),
            width: OpWidth::W16,
        });
        assert!(
            word.windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x50, 0x20]),
            "word Xchg must partially commit GuestRegs.gpr[4]: {word:02X?}"
        );
        assert!(
            word.windows(7)
                .any(|bytes| bytes == [0x66, 0x89, 0xB8, 0x80, 0x00, 0x00, 0x00]),
            "word Xchg must partially commit GuestRegs.gpr[16]: {word:02X?}"
        );

        let dword = lower_single_op(OpKind::Xchg {
            reg1: x86(X86Reg::Rbp),
            reg2: x86(X86Reg::R17),
            width: OpWidth::W32,
        });
        assert!(
            dword
                .windows(4)
                .any(|bytes| bytes == [0x48, 0x89, 0x55, 0x00]),
            "dword Xchg must synchronize the zero-extended guest RBP: {dword:02X?}"
        );

        for malformed in [
            OpKind::Xchg {
                reg1: x86(X86Reg::R16),
                reg2: x86(X86Reg::Rax),
                width: OpWidth::W8,
            },
            OpKind::Xchg {
                reg1: x86(X86Reg::R16),
                reg2: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
                width: OpWidth::W64,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed Xchg must fail lowering"
            );
        }

        let hinted = OpKind::Xchg {
            reg1: x86(X86Reg::R16),
            reg2: x86(X86Reg::Rax),
            width: OpWidth::W64,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_xchg_preserves_widths_flags_and_host_stack() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            reg1: X86Reg,
            reg2: X86Reg,
            width: OpWidth,
        }

        let cases = [
            Case {
                name: "XCHG AX,R16W partial exchange",
                reg1: X86Reg::Rax,
                reg2: X86Reg::R16,
                width: OpWidth::W16,
            },
            Case {
                name: "XCHG EBP,R17D zero-extending exchange",
                reg1: X86Reg::Rbp,
                reg2: X86Reg::R17,
                width: OpWidth::W32,
            },
            Case {
                name: "XCHG RSP,R31 full exchange",
                reg1: X86Reg::Rsp,
                reg2: X86Reg::R31,
                width: OpWidth::W64,
            },
            Case {
                name: "XCHG SP,BP partial state-to-state exchange",
                reg1: X86Reg::Rsp,
                reg2: X86Reg::Rbp,
                width: OpWidth::W16,
            },
            Case {
                name: "XCHG R16D,R16D zero-extending self exchange",
                reg1: X86Reg::R16,
                reg2: X86Reg::R16,
                width: OpWidth::W32,
            },
        ];

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Xchg {
                    reg1: x86(case.reg1),
                    reg2: x86(case.reg2),
                    width: case.width,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            regs.rflags = STATUS;
            let mut expected = regs;
            let reg1_idx = case.reg1.gpr_index().unwrap() as usize;
            let reg2_idx = case.reg2.gpr_index().unwrap() as usize;
            let old_reg1 = regs.gpr[reg1_idx];
            let old_reg2 = regs.gpr[reg2_idx];
            match case.width {
                OpWidth::W16 => {
                    expected.gpr[reg1_idx] = (old_reg1 & !0xFFFF) | (old_reg2 & 0xFFFF);
                    expected.gpr[reg2_idx] = (old_reg2 & !0xFFFF) | (old_reg1 & 0xFFFF);
                }
                OpWidth::W32 => {
                    expected.gpr[reg1_idx] = old_reg2 & 0xFFFF_FFFF;
                    expected.gpr[reg2_idx] = old_reg1 & 0xFFFF_FFFF;
                }
                OpWidth::W64 => {
                    expected.gpr[reg1_idx] = old_reg2;
                    expected.gpr[reg2_idx] = old_reg1;
                }
                _ => unreachable!(),
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(regs.rflags & STATUS, STATUS, "{} status flags", case.name);
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_and_not_preserves_gprs_flags_and_aliases() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        struct Case {
            name: &'static str,
            flagful: bool,
            dst: X86Reg,
            src1: X86Reg,
            src2: X86Reg,
            width: OpWidth,
            src1_value: u64,
            src2_value: u64,
        }
        let cases = [
            Case {
                name: "ANDN RSP,RSP,RBP destination-first-source alias",
                flagful: true,
                dst: X86Reg::Rsp,
                src1: X86Reg::Rsp,
                src2: X86Reg::Rbp,
                width: OpWidth::W64,
                src1_value: 0xF0F0_00FF_AA55_5A5A,
                src2_value: 0x70F0_F000_AA00_0A0A,
            },
            Case {
                name: "ANDN EBP,ESP,EBP destination-second-source alias",
                flagful: true,
                dst: X86Reg::Rbp,
                src1: X86Reg::Rsp,
                src2: X86Reg::Rbp,
                width: OpWidth::W32,
                src1_value: 0,
                src2_value: u64::MAX,
            },
            Case {
                name: "ANDN R16D,R31D,R31D source alias",
                flagful: true,
                dst: X86Reg::R16,
                src1: X86Reg::R31,
                src2: X86Reg::R31,
                width: OpWidth::W32,
                src1_value: 0xAABB_CCDD_8000_0018,
                src2_value: 0xAABB_CCDD_8000_0018,
            },
            Case {
                name: "NF ANDN R31,R31,R31 all operands alias",
                flagful: false,
                dst: X86Reg::R31,
                src1: X86Reg::R31,
                src2: X86Reg::R31,
                width: OpWidth::W64,
                src1_value: 0xDEAD_BEEF_1357_2418,
                src2_value: 0xDEAD_BEEF_1357_2418,
            },
        ];
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        const STATUS: u64 = 0x8D5;

        for case in cases {
            let flags = if case.flagful {
                FlagUpdate::Specific(defined)
            } else {
                FlagUpdate::None
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::AndNot {
                    dst: x86(case.dst),
                    src1: x86(case.src1),
                    src2: SrcOperand::Reg(x86(case.src2)),
                    width: case.width,
                    flags,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x2468_0000_1357_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src1_idx = case.src1.gpr_index().unwrap() as usize;
            let src2_idx = case.src2.gpr_index().unwrap() as usize;
            regs.gpr[src1_idx] = case.src1_value;
            regs.gpr[src2_idx] = case.src2_value;
            regs.rflags = 0x2 | STATUS;

            let mut expected = regs;
            let src1 = regs.gpr[src1_idx] & case.width.mask();
            let src2 = regs.gpr[src2_idx] & case.width.mask();
            let result = (src1 & !src2) & case.width.mask();
            expected.gpr[dst_idx] = result;
            if case.flagful {
                expected.rflags &= !0x8C1;
                expected.rflags |= u64::from(result == 0) << 6;
                expected.rflags |= ((result >> (case.width.bits() - 1)) & 1) << 7;
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            if case.width == OpWidth::W32 {
                assert_eq!(regs.gpr[dst_idx] >> 32, 0, "{} zero extension", case.name);
            }
            assert_eq!(
                regs.rflags & STATUS,
                expected.rflags & STATUS,
                "{} status flags",
                case.name
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_bls_preserves_gprs_flags_and_aliases() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        if !std::is_x86_feature_detected!("bmi1") {
            return;
        }
        struct Case {
            name: &'static str,
            kind: X86BlsKind,
            flagful: bool,
            dst: X86Reg,
            src: X86Reg,
            width: OpWidth,
            source: u64,
        }
        let cases = [
            Case {
                name: "BLSR RSP,RBP zero source",
                kind: X86BlsKind::Blsr,
                flagful: true,
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                width: OpWidth::W64,
                source: 0,
            },
            Case {
                name: "BLSMSK EBP,ESP zero source",
                kind: X86BlsKind::Blsmsk,
                flagful: true,
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                width: OpWidth::W32,
                source: 0,
            },
            Case {
                name: "BLSI R31D,R16D",
                kind: X86BlsKind::Blsi,
                flagful: true,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                width: OpWidth::W32,
                source: 0xAABB_CCDD_8000_0018,
            },
            Case {
                name: "NF BLSR R16,R16 source-destination alias",
                kind: X86BlsKind::Blsr,
                flagful: false,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                width: OpWidth::W64,
                source: 0xDEAD_BEEF_1357_2418,
            },
        ];
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let defined = FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF);
        const STATUS: u64 = 0x8D5;

        for case in cases {
            let flags = if case.flagful {
                FlagUpdate::Specific(defined)
            } else {
                FlagUpdate::None
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::X86Bls {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    width: case.width,
                    kind: case.kind,
                    flags,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x2468_0000_1357_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            regs.rflags = 0x2 | STATUS;

            let mut expected = regs;
            let source = regs.gpr[src_idx] & case.width.mask();
            let result = match case.kind {
                X86BlsKind::Blsr => source & source.wrapping_sub(1),
                X86BlsKind::Blsmsk => source ^ source.wrapping_sub(1),
                X86BlsKind::Blsi => source.wrapping_neg() & source,
            } & case.width.mask();
            expected.gpr[dst_idx] = result;
            if case.flagful {
                expected.rflags &= !0x8C1;
                let carry = match case.kind {
                    X86BlsKind::Blsr | X86BlsKind::Blsmsk => source == 0,
                    X86BlsKind::Blsi => source != 0,
                };
                let zero = case.kind != X86BlsKind::Blsmsk && result == 0;
                expected.rflags |= u64::from(carry);
                expected.rflags |= u64::from(zero) << 6;
                expected.rflags |= ((result >> (case.width.bits() - 1)) & 1) << 7;
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            if case.width == OpWidth::W32 {
                assert_eq!(regs.gpr[dst_idx] >> 32, 0, "{} zero extension", case.name);
            }
            assert_eq!(
                regs.rflags & STATUS,
                expected.rflags & STATUS,
                "{} status flags",
                case.name
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_bextr_bzhi_preserve_gprs_flags_and_aliases() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        struct Case {
            name: &'static str,
            bzhi: bool,
            flagful: bool,
            dst: X86Reg,
            src: X86Reg,
            control: X86Reg,
            width: OpWidth,
            source: u64,
            control_value: u64,
        }
        let cases = [
            Case {
                name: "BEXTR RSP,RBP,R16",
                bzhi: false,
                flagful: true,
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                control: X86Reg::R16,
                width: OpWidth::W64,
                source: 0xFEDC_BA98_7654_3210,
                control_value: (20 << 8) | 12,
            },
            Case {
                name: "BZHI EBP,ESP,ECX",
                bzhi: true,
                flagful: true,
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                control: X86Reg::Rcx,
                width: OpWidth::W32,
                source: 0xAABB_CCDD_DEAD_BEEF,
                control_value: 40,
            },
            Case {
                name: "NF BEXTR R31D,R16D,R31D destination-control alias",
                bzhi: false,
                flagful: false,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                control: X86Reg::R31,
                width: OpWidth::W32,
                source: 0x0123_4567_89AB_CDEF,
                control_value: (12 << 8) | 7,
            },
            Case {
                name: "NF BZHI R16,R16,R16 all operands alias",
                bzhi: true,
                flagful: false,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                control: X86Reg::R16,
                width: OpWidth::W64,
                source: 0,
                control_value: 0xDEAD_BEEF_1357_2420,
            },
        ];
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        const STATUS: u64 = 0x8D5;

        for case in cases {
            if (case.bzhi && !std::is_x86_feature_detected!("bmi2"))
                || (!case.bzhi && !std::is_x86_feature_detected!("bmi1"))
            {
                continue;
            }
            let flags = if case.flagful {
                if case.bzhi {
                    FlagUpdate::Specific(
                        FlagSet::CF
                            .union(FlagSet::ZF)
                            .union(FlagSet::SF)
                            .union(FlagSet::OF),
                    )
                } else {
                    FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF))
                }
            } else {
                FlagUpdate::None
            };
            let kind = if case.bzhi {
                OpKind::Bzhi {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    index: x86(case.control),
                    width: case.width,
                    flags,
                }
            } else {
                OpKind::Bextr {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    control: x86(case.control),
                    width: case.width,
                    flags,
                }
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x2468_0000_1357_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            let control_idx = case.control.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            regs.gpr[control_idx] = case.control_value;
            regs.rflags = 0x2 | STATUS;

            let mut expected = regs;
            let bits = case.width.bits();
            let operand_mask = case.width.mask();
            let source = regs.gpr[src_idx] & operand_mask;
            let control = regs.gpr[control_idx] & operand_mask;
            let result = if case.bzhi {
                let index = (control & 0xFF) as u32;
                if index >= bits {
                    source
                } else {
                    source & ((1u64 << index) - 1)
                }
            } else {
                let start = (control & 0xFF) as u32;
                let length = ((control >> 8) & 0xFF) as u32;
                if start >= bits || length == 0 {
                    0
                } else {
                    let field_bits = length.min(bits - start);
                    let shifted = source >> start;
                    if field_bits == 64 {
                        shifted
                    } else {
                        shifted & ((1u64 << field_bits) - 1)
                    }
                }
            };
            expected.gpr[dst_idx] = result;
            if case.flagful {
                if case.bzhi {
                    let index = (control & 0xFF) as u32;
                    expected.rflags &= !0x8C1;
                    expected.rflags |= u64::from(index >= bits);
                    expected.rflags |= u64::from(result == 0) << 6;
                    expected.rflags |= ((result >> (bits - 1)) & 1) << 7;
                } else {
                    expected.rflags &= !0x841;
                    expected.rflags |= u64::from(result == 0) << 6;
                }
            }

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            if case.width == OpWidth::W32 {
                assert_eq!(regs.gpr[dst_idx] >> 32, 0, "{} zero extension", case.name);
            }
            assert_eq!(
                regs.rflags & STATUS,
                expected.rflags & STATUS,
                "{} status flags",
                case.name
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_pdep_pext_preserve_gprs_flags_and_aliases() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        if !std::is_x86_feature_detected!("bmi2") {
            return;
        }
        fn pdep(mut src: u64, mut mask: u64) -> u64 {
            let mut result = 0u64;
            while mask != 0 {
                let bit = mask & mask.wrapping_neg();
                if src & 1 != 0 {
                    result |= bit;
                }
                src >>= 1;
                mask &= mask - 1;
            }
            result
        }
        fn pext(src: u64, mut mask: u64) -> u64 {
            let mut result = 0u64;
            let mut output_bit = 1u64;
            while mask != 0 {
                let bit = mask & mask.wrapping_neg();
                if src & bit != 0 {
                    result |= output_bit;
                }
                output_bit <<= 1;
                mask &= mask - 1;
            }
            result
        }

        struct Case {
            name: &'static str,
            extract: bool,
            dst: X86Reg,
            src: X86Reg,
            mask: X86Reg,
            width: OpWidth,
            source: u64,
            mask_value: u64,
        }
        let cases = [
            Case {
                name: "PDEP RSP,RBP,R16",
                extract: false,
                dst: X86Reg::Rsp,
                src: X86Reg::Rbp,
                mask: X86Reg::R16,
                width: OpWidth::W64,
                source: 0x0123_4567_89AB_CDEF,
                mask_value: 0xF0F0_00FF_AA55_5A5A,
            },
            Case {
                name: "PEXT EBP,ESP,ECX",
                extract: true,
                dst: X86Reg::Rbp,
                src: X86Reg::Rsp,
                mask: X86Reg::Rcx,
                width: OpWidth::W32,
                source: 0xAABB_CCDD_DEAD_BEEF,
                mask_value: 0xFFFF_0000_F0F0_55AA,
            },
            Case {
                name: "PDEP R31D,R16D,R31D destination-mask alias",
                extract: false,
                dst: X86Reg::R31,
                src: X86Reg::R16,
                mask: X86Reg::R31,
                width: OpWidth::W32,
                source: 0xFEDC_BA98_7654_3210,
                mask_value: 0xAAAA_5555,
            },
            Case {
                name: "PEXT R16,R16,R16 all operands alias",
                extract: true,
                dst: X86Reg::R16,
                src: X86Reg::R16,
                mask: X86Reg::R16,
                width: OpWidth::W64,
                source: 0xDEAD_BEEF_1357_2468,
                mask_value: 0xA5A5_5A5A_C3C3_3C3C,
            },
        ];
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        const FLAGS: u64 = 0x8D5;

        for case in cases {
            let kind = if case.extract {
                OpKind::Pext {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    mask: x86(case.mask),
                    width: case.width,
                }
            } else {
                OpKind::Pdep {
                    dst: x86(case.dst),
                    src: x86(case.src),
                    mask: x86(case.mask),
                    width: case.width,
                }
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x2468_0000_1357_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let src_idx = case.src.gpr_index().unwrap() as usize;
            let mask_idx = case.mask.gpr_index().unwrap() as usize;
            regs.gpr[src_idx] = case.source;
            regs.gpr[mask_idx] = case.mask_value;
            regs.rflags = 0x2 | FLAGS;

            let mut expected = regs;
            let operand_mask = case.width.mask();
            let source = regs.gpr[src_idx] & operand_mask;
            let mask = regs.gpr[mask_idx] & operand_mask;
            expected.gpr[dst_idx] = if case.extract {
                pext(source, mask)
            } else {
                pdep(source, mask)
            };

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & FLAGS,
                expected.rflags & FLAGS,
                "{} flags",
                case.name
            );
        }
    }
    #[test]
    fn lower_state_backed_gpr_bit_tests_emit_cf_merge_and_reject_malformed_shapes() {
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));

        let bt = lower_single_op(OpKind::Bt {
            src: x86(X86Reg::Rsp),
            index: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W64,
        });
        assert!(
            bt.windows(4).any(|bytes| bytes == [0x48, 0x0F, 0xA3, 0xFA]),
            "state-backed BT must test RDX by RDI: {bt:02X?}"
        );
        assert_eq!(
            bt.iter().filter(|byte| **byte == 0x9C).count(),
            2,
            "state-backed BT must save old and new RFLAGS: {bt:02X?}"
        );
        assert_eq!(bt.iter().filter(|byte| **byte == 0x9D).count(), 1);

        let bts = lower_single_op(OpKind::Bts {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::Rbp),
            index: SrcOperand::Imm(15),
            width: OpWidth::W16,
        });
        assert!(
            bts.windows(5)
                .any(|bytes| bytes == [0x66, 0x0F, 0xBA, 0xEA, 0x0F]),
            "state-backed BTS must update DX by immediate: {bts:02X?}"
        );
        assert!(
            bts.windows(4)
                .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
            "word BTS must partially synchronize guest RBP: {bts:02X?}"
        );

        let btr = lower_single_op(OpKind::Btr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R16),
            index: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W32,
        });
        assert!(
            btr.windows(3).any(|bytes| bytes == [0x0F, 0xB3, 0xFA]),
            "state-backed BTR must update EDX by EDI: {btr:02X?}"
        );
        assert!(
            btr.windows(7)
                .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
            "dword BTR must fully commit GuestRegs.gpr[16]: {btr:02X?}"
        );

        for malformed in [
            OpKind::Bt {
                src: x86(X86Reg::Rsp),
                index: SrcOperand::Imm(0),
                width: OpWidth::W8,
            },
            OpKind::Btc {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                index: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
            OpKind::Bts {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::R16),
                index: SrcOperand::Reg(VReg::Virtual(crate::smir::ir::types::VirtualId(0))),
                width: OpWidth::W64,
            },
        ] {
            assert!(
                matches!(
                    lower_single_op_err(malformed),
                    LowerError::InvalidOperand { .. }
                ),
                "malformed state-backed bit test must fail lowering"
            );
        }

        let hinted = OpKind::Btr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R16),
            index: SrcOperand::Imm(7),
            width: OpWidth::W64,
        };
        assert!(matches!(
            lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
            LowerError::InvalidOperand { .. }
        ));
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_gpr_bit_tests_preserve_width_and_cf_contracts() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        const STATUS_MASK: u64 = 0x8D5;

        struct Case {
            name: &'static str,
            kind: BitTestRegOp,
            operand: X86Reg,
            index: SrcOperand,
            source: u64,
            index_value: u64,
            width: OpWidth,
        }

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let cases = [
            Case {
                name: "BT RSP,RBP register index",
                kind: BitTestRegOp::Test,
                operand: X86Reg::Rsp,
                index: SrcOperand::Reg(x86(X86Reg::Rbp)),
                source: 1u64 << 63,
                index_value: 63,
                width: OpWidth::W64,
            },
            Case {
                name: "BTS BP,15 partial destination",
                kind: BitTestRegOp::Set,
                operand: X86Reg::Rbp,
                index: SrcOperand::Imm(15),
                source: 0x3344_5566_8765_0000,
                index_value: 15,
                width: OpWidth::W16,
            },
            Case {
                name: "BTR R16D,R31D zero-extending destination",
                kind: BitTestRegOp::Reset,
                operand: X86Reg::R16,
                index: SrcOperand::Reg(x86(X86Reg::R31)),
                source: u64::MAX,
                index_value: 31,
                width: OpWidth::W32,
            },
            Case {
                name: "BTC R31,R16 extended destination and index",
                kind: BitTestRegOp::Complement,
                operand: X86Reg::R31,
                index: SrcOperand::Reg(x86(X86Reg::R16)),
                source: 0,
                index_value: 63,
                width: OpWidth::W64,
            },
            Case {
                name: "BT R16W,SP masked register index",
                kind: BitTestRegOp::Test,
                operand: X86Reg::R16,
                index: SrcOperand::Reg(x86(X86Reg::Rsp)),
                source: 1,
                index_value: 16,
                width: OpWidth::W16,
            },
            Case {
                name: "BTR RSP,63 full destination",
                kind: BitTestRegOp::Reset,
                operand: X86Reg::Rsp,
                index: SrcOperand::Imm64(63),
                source: 1u64 << 63,
                index_value: 63,
                width: OpWidth::W64,
            },
        ];

        for case in cases {
            let operand = x86(case.operand);
            let kind = match case.kind {
                BitTestRegOp::Test => OpKind::Bt {
                    src: operand,
                    index: case.index.clone(),
                    width: case.width,
                },
                BitTestRegOp::Set => OpKind::Bts {
                    dst: operand,
                    src: operand,
                    index: case.index.clone(),
                    width: case.width,
                },
                BitTestRegOp::Reset => OpKind::Btr {
                    dst: operand,
                    src: operand,
                    index: case.index.clone(),
                    width: case.width,
                },
                BitTestRegOp::Complement => OpKind::Btc {
                    dst: operand,
                    src: operand,
                    index: case.index.clone(),
                    width: case.width,
                },
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind);
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0xA1A2_0000_0000_8000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let operand_idx = case.operand.gpr_index().unwrap() as usize;
            regs.gpr[operand_idx] = case.source;
            if let SrcOperand::Reg(index) = case.index {
                let index_idx = X86_64Lowerer::x86_gpr_index(index).unwrap() as usize;
                regs.gpr[index_idx] = case.index_value;
            }
            regs.rflags = STATUS_MASK;

            let mut expected = regs;
            let bit = case.index_value & (u64::from(case.width.bits()) - 1);
            let value = case.source & case.width.mask();
            let cf = (value >> bit) & 1;
            let result = match case.kind {
                BitTestRegOp::Test => None,
                BitTestRegOp::Set => Some(value | (1u64 << bit)),
                BitTestRegOp::Reset => Some(value & !(1u64 << bit)),
                BitTestRegOp::Complement => Some(value ^ (1u64 << bit)),
            };
            if let Some(result) = result {
                expected.gpr[operand_idx] = match case.width {
                    OpWidth::W16 => (regs.gpr[operand_idx] & !case.width.mask()) | result,
                    OpWidth::W32 | OpWidth::W64 => result,
                    OpWidth::W8 | OpWidth::W128 => unreachable!(),
                };
            }
            expected.rflags = (expected.rflags & !1) | cf;

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & STATUS_MASK,
                expected.rflags & STATUS_MASK,
                "{} status flags",
                case.name
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_x86_alignment_check_covers_addresses_preserves_state_and_deopts_precisely() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        let marker = 0xD1CE_BA5E_F00D_CAFEu64;
        let sentinel_pc = 0xABCD_EF01_2345_6789u64;
        let status = 0x8D5u64; // CF, PF, AF, ZF, SF, OF

        let mut cases = Vec::new();

        let mut direct = GuestRegs::default();
        direct.gpr[0] = 0x1000;
        cases.push((
            "direct/aligned16",
            Address::Direct(x86(X86Reg::Rax)),
            16,
            direct,
            false,
        ));

        let mut stack = GuestRegs::default();
        stack.gpr[4] = 0x2011;
        cases.push((
            "rsp+disp/misaligned32",
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: -16,
                disp_size: DispSize::Disp8,
            },
            32,
            stack,
            true,
        ));

        let mut sib = GuestRegs::default();
        sib.gpr[5] = 0x3000;
        sib.gpr[9] = 8;
        cases.push((
            "rbp+r9*8-64/aligned64",
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::R9),
                scale: 8,
                disp: -64,
                disp_size: DispSize::Disp8,
            },
            64,
            sib,
            false,
        ));

        cases.push((
            "pcrel/misaligned64",
            Address::PcRel {
                offset: -1,
                disp_size: DispSize::Disp8,
                base: Some(0x4000),
            },
            64,
            GuestRegs::default(),
            true,
        ));
        cases.push((
            "absolute/aligned32",
            Address::Absolute(0x5000),
            32,
            GuestRegs::default(),
            false,
        ));

        let mut segmented = GuestRegs::default();
        segmented.fs_base = 0x5F00;
        segmented.gpr[4] = 0x80;
        segmented.gpr[16] = 0x20;
        cases.push((
            "fs+rsp+r16*4/aligned64",
            Address::SegmentRel {
                segment: x86(X86Reg::FsBase),
                base: Some(x86(X86Reg::Rsp)),
                index: Some(x86(X86Reg::R16)),
                scale: 4,
                disp: 0,
            },
            64,
            segmented,
            false,
        ));

        let mut wide_disp = GuestRegs::default();
        wide_disp.gs_base = 0x8000_0000_0000_6001;
        cases.push((
            "gs+wide-disp/aligned16",
            Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: None,
                index: None,
                scale: 1,
                disp: i64::MIN,
            },
            16,
            wide_disp,
            true,
        ));

        for (name, addr, alignment, mut regs, should_fault) in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, OpKind::X86CheckAlignment { addr, alignment });
            builder.push_op(
                0x1001,
                OpKind::Mov {
                    dst: x86(X86Reg::R11),
                    src: SrcOperand::Imm64(marker as i64),
                    width: OpWidth::W64,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("lower {name}: {error:?}"));
            let exec = ExecMem::new(&lowerer.finalize().expect("finalize alignment check"))
                .expect("map alignment check");

            regs.gpr[11] = 0x1111_2222_3333_4444;
            regs.rflags = 0x2 | status;
            regs.exit_pc = sentinel_pc;
            let before = regs;
            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.rflags & status, status, "{name}: status flags");
            for index in 0..32 {
                let expected = if index == 11 && !should_fault {
                    marker
                } else {
                    before.gpr[index]
                };
                assert_eq!(regs.gpr[index], expected, "{name}: gpr[{index}]");
            }
            assert_eq!(
                regs.exit_pc,
                if should_fault { 0x1000 } else { sentinel_pc },
                "{name}: precise resume PC"
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_xgetbv_selects_guest_state_and_deopts_faults_precisely() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1234_5678);
        builder.push_op(
            0x1234_5678,
            OpKind::X86XGetBv {
                dst_low: rax,
                dst_high: rdx,
                selector: rcx,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower state-backed XGETBV");
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize state-backed XGETBV"))
            .expect("map state-backed XGETBV");

        let flags = 0x2 | 0x8D5;
        let mut regs = GuestRegs::default();
        regs.cr4 = 1 << 18;
        regs.xcr0 = 0xFEDC_BA98_7654_3217;
        regs.xgetbv1 = 0x00F0_00F0_FFFF_00F5;
        regs.gpr[0] = u64::MAX;
        regs.gpr[1] = 0;
        regs.gpr[2] = u64::MAX;
        regs.rflags = flags;
        regs.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(regs.gpr[0], 0x7654_3217);
        assert_eq!(regs.gpr[2], 0xFEDC_BA98);
        assert_eq!(regs.gpr[1], 0, "XGETBV preserves RCX");
        assert_eq!(regs.rflags & 0x8D5, flags & 0x8D5);
        assert_eq!(regs.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

        regs.gpr[0] = u64::MAX;
        regs.gpr[1] = 1;
        regs.gpr[2] = u64::MAX;
        regs.rflags = flags;
        exec.run(lowered.entry_offset, &mut regs);
        let xinuse = regs.xgetbv1 & regs.xcr0;
        assert_eq!(regs.gpr[0], xinuse as u32 as u64);
        assert_eq!(regs.gpr[2], (xinuse >> 32) as u32 as u64);
        assert_eq!(regs.gpr[1], 1);
        assert_eq!(regs.rflags & 0x8D5, flags & 0x8D5);

        for (name, cr4, selector) in [
            ("OSXSAVE clear", 0, 0),
            ("invalid selector", 1 << 18, 2),
            ("large low-32 selector", 1 << 18, 0xFFFF_FFFF),
        ] {
            let mut fault = GuestRegs::default();
            fault.cr4 = cr4;
            fault.xcr0 = regs.xcr0;
            fault.xgetbv1 = regs.xgetbv1;
            fault.gpr[0] = 0x1111_2222_3333_4444;
            fault.gpr[1] = selector;
            fault.gpr[2] = 0x5555_6666_7777_8888;
            fault.rflags = flags;
            fault.exit_pc = 0;
            exec.run(lowered.entry_offset, &mut fault);
            assert_eq!(fault.exit_pc, 0x1234_5678, "{name}: precise restart PC");
            assert_eq!(fault.gpr[0], 0x1111_2222_3333_4444, "{name}: RAX");
            assert_eq!(fault.gpr[1], selector, "{name}: RCX");
            assert_eq!(fault.gpr[2], 0x5555_6666_7777_8888, "{name}: RDX");
            assert_eq!(fault.rflags & 0x8D5, flags & 0x8D5, "{name}: flags");
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_xsetbv_validates_state_commits_and_hands_off_precisely() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1234_5000);
        builder.push_op(
            0x1234_5000,
            OpKind::X86XSetBv {
                selector: rcx,
                src_low: rax,
                src_high: rdx,
            },
        );
        builder.push_op(0x1234_5003, OpKind::Nop);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower state-backed XSETBV");
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize state-backed XSETBV"))
            .expect("map state-backed XSETBV");

        let flags = 0x2 | 0x8D5;
        let run = |value: u64, cr4: u64, cr0: u64, cpl: u64, apx_enabled: bool, selector: u32| {
            let mut regs = GuestRegs::default();
            regs.cr4 = cr4;
            regs.cr0 = cr0;
            regs.cpl = cpl;
            regs.apx_enabled = u64::from(apx_enabled);
            regs.xcr0 = 3;
            regs.gpr[0] = 0xA5A5_A5A5_0000_0000 | (value as u32 as u64);
            regs.gpr[1] = 0x5A5A_5A5A_0000_0000 | u64::from(selector);
            regs.gpr[2] = 0xC3C3_C3C3_0000_0000 | ((value >> 32) as u32 as u64);
            regs.rflags = flags;
            regs.exit_pc = 0;
            let inputs = (regs.gpr[0], regs.gpr[1], regs.gpr[2]);
            exec.run(lowered.entry_offset, &mut regs);
            assert_eq!(
                (regs.gpr[0], regs.gpr[1], regs.gpr[2]),
                inputs,
                "XSETBV must preserve EDX:EAX and ECX"
            );
            assert_eq!(regs.rflags & 0x8D5, flags & 0x8D5);
            regs
        };

        for (name, value, apx_enabled) in [
            ("x87", 1, false),
            ("x87+sse", 3, false),
            ("avx", 7, false),
            ("avx512", 0xE7, false),
            ("apx", 0x0008_00E7, true),
        ] {
            let regs = run(value, 1 << 18, 1, 0, apx_enabled, 0);
            assert_eq!(regs.xcr0, value, "{name}: committed XCR0");
            assert_eq!(regs.exit_pc, 0x1234_5003, "{name}: next PC handoff");
        }

        // CPL is ignored outside protected mode.
        let real_mode = run(7, 1 << 18, 0, 3, false, 0);
        assert_eq!(real_mode.xcr0, 7);
        assert_eq!(real_mode.exit_pc, 0x1234_5003);

        for (name, value, cr4, cr0, cpl, apx_enabled, selector) in [
            ("OSXSAVE clear", 7, 0, 1, 0, false, 0),
            ("protected CPL3", 7, 1 << 18, 1, 3, false, 0),
            ("selector one", 7, 1 << 18, 1, 0, false, 1),
            ("x87 disabled", 0, 1 << 18, 1, 0, false, 0),
            ("unsupported bit", 9, 1 << 18, 1, 0, false, 0),
            ("AVX without SSE", 5, 1 << 18, 1, 0, false, 0),
            ("partial AVX512", 0x27, 1 << 18, 1, 0, false, 0),
            ("AVX512 without AVX", 0xE3, 1 << 18, 1, 0, false, 0),
            ("APX disabled", 0x0008_0001, 1 << 18, 1, 0, false, 0),
            ("high unsupported", (1u64 << 63) | 1, 1 << 18, 1, 0, true, 0),
        ] {
            let regs = run(value, cr4, cr0, cpl, apx_enabled, selector);
            assert_eq!(regs.xcr0, 3, "{name}: XCR0 must not commit");
            assert_eq!(regs.exit_pc, 0x1234_5000, "{name}: fault restart PC");
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_helper_backed_immediate_memory_bit_update_commits_cf_only_after_store() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        #[repr(C)]
        struct LoadResult {
            value: u64,
            ok: u64,
        }

        #[derive(Default)]
        struct MemoryContext {
            load_value: u64,
            store_value: u64,
            committed_value: u64,
            loads: u64,
            stores: u64,
            store_ok: u64,
        }

        extern "C" fn load(
            context: *mut MemoryContext,
            _addr: u64,
            _size: u64,
            _signed: u64,
        ) -> LoadResult {
            let context = unsafe { &mut *context };
            context.loads += 1;
            LoadResult {
                value: context.load_value,
                ok: 1,
            }
        }

        extern "C" fn store(
            context: *mut MemoryContext,
            _addr: u64,
            value: u64,
            _size: u64,
        ) -> u64 {
            let context = unsafe { &mut *context };
            context.stores += 1;
            context.store_value = value;
            if context.store_ok != 0 {
                context.committed_value = value;
            }
            context.store_ok
        }

        let old = VReg::Virtual(crate::smir::ir::types::VirtualId(35));
        let mask = VReg::Virtual(crate::smir::ir::types::VirtualId(36));
        let result = VReg::Virtual(crate::smir::ir::types::VirtualId(37));
        let address = Address::Absolute(0x4000);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address.clone(),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(1),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Shl {
                dst: mask,
                src: mask,
                amount: SrcOperand::Imm(5),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Not {
                dst: mask,
                src: mask,
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::And {
                dst: result,
                src1: old,
                src2: SrcOperand::Reg(mask),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: result,
                addr: address,
                width: MemWidth::B8,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Bt {
                src: old,
                index: SrcOperand::Imm(5),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower helper-backed memory BTR");
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize memory BTR"))
            .expect("map helper-backed memory BTR");

        const STATUS_MASK: u64 = 0x8D5;
        const INCOMING_FLAGS: u64 = 0x2 | 0x8D4; // CF clear
        let initial_gprs = {
            let mut gprs = [0u64; 32];
            for (index, value) in gprs.iter_mut().enumerate() {
                *value = 0xA500_0000_0000_0000 | index as u64;
            }
            gprs
        };

        let mut success_context = MemoryContext {
            load_value: 1 << 5,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 1,
            ..MemoryContext::default()
        };
        let mut success = GuestRegs::default();
        success.gpr = initial_gprs;
        success.rflags = INCOMING_FLAGS;
        success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
        success.ctx = (&mut success_context as *mut MemoryContext) as u64;
        success.load_fn = load as usize as u64;
        success.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut success);

        assert_eq!(success_context.loads, 1);
        assert_eq!(success_context.stores, 1);
        assert_eq!(success_context.store_value, 0, "BTR store value");
        assert_eq!(success_context.committed_value, 0);
        assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
        assert_eq!(
            success.rflags & STATUS_MASK,
            (INCOMING_FLAGS & STATUS_MASK) | 1,
            "successful BTR must replace only CF"
        );
        assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

        let mut fault_context = MemoryContext {
            load_value: 1 << 5,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 0,
            ..MemoryContext::default()
        };
        let mut fault = GuestRegs::default();
        fault.gpr = initial_gprs;
        fault.rflags = INCOMING_FLAGS;
        fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
        fault.load_fn = load as usize as u64;
        fault.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut fault);

        assert_eq!(fault_context.loads, 1);
        assert_eq!(fault_context.stores, 1);
        assert_eq!(fault_context.store_value, 0, "updated value reaches store");
        assert_eq!(
            fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
            "failed store must not commit memory"
        );
        assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
        assert_eq!(
            fault.rflags & STATUS_MASK,
            INCOMING_FLAGS & STATUS_MASK,
            "fault must preserve every arithmetic status flag"
        );
        assert_eq!(fault.exit_pc, 0x1000, "fault must restart current PC");
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_helper_backed_memory_destination_alu_commits_flags_only_after_store() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        #[repr(C)]
        struct LoadResult {
            value: u64,
            ok: u64,
        }

        #[derive(Default)]
        struct MemoryContext {
            load_value: u64,
            store_value: u64,
            committed_value: u64,
            last_addr: u64,
            last_size: u64,
            loads: u64,
            stores: u64,
            store_ok: u64,
        }

        extern "C" fn load(
            context: *mut MemoryContext,
            addr: u64,
            size: u64,
            _signed: u64,
        ) -> LoadResult {
            let context = unsafe { &mut *context };
            context.loads += 1;
            context.last_addr = addr;
            context.last_size = size;
            LoadResult {
                value: context.load_value,
                ok: 1,
            }
        }

        extern "C" fn store(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
            let context = unsafe { &mut *context };
            context.stores += 1;
            context.last_addr = addr;
            context.last_size = size;
            context.store_value = value;
            if context.store_ok != 0 {
                context.committed_value = value;
            }
            context.store_ok
        }

        let old = VReg::Virtual(crate::smir::ir::types::VirtualId(40));
        let result = VReg::Virtual(crate::smir::ir::types::VirtualId(41));
        let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(42));
        let source = VReg::Arch(ArchReg::X86(X86Reg::R16));
        let address = Address::Absolute(0x4000);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address.clone(),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Add {
                dst: result,
                src1: old,
                src2: SrcOperand::Reg(source),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: result,
                addr: address,
                width: MemWidth::B8,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Add {
                dst: flags_result,
                src1: old,
                src2: SrcOperand::Reg(source),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower helper-backed scalar RMW");
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize scalar RMW"))
            .expect("map scalar RMW");

        const INCOMING_FLAGS: u64 = 0x2 | 0x8D5;
        let initial_gprs = {
            let mut gprs = [0u64; 32];
            for (index, value) in gprs.iter_mut().enumerate() {
                *value = 0xA500_0000_0000_0000 | index as u64;
            }
            gprs[16] = 1;
            gprs
        };

        let mut success_context = MemoryContext {
            load_value: u64::MAX,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 1,
            ..MemoryContext::default()
        };
        let mut success = GuestRegs::default();
        success.gpr = initial_gprs;
        success.rflags = INCOMING_FLAGS;
        success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
        success.ctx = (&mut success_context as *mut MemoryContext) as u64;
        success.load_fn = load as usize as u64;
        success.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut success);

        assert_eq!(success_context.loads, 1);
        assert_eq!(success_context.stores, 1);
        assert_eq!(success_context.last_addr, 0x4000);
        assert_eq!(success_context.last_size, 8);
        assert_eq!(success_context.store_value, 0);
        assert_eq!(success_context.committed_value, 0);
        assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
        assert_eq!(success.rflags & 0x8D5, 0x55, "ADD result flags");
        assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

        let mut fault_context = MemoryContext {
            load_value: u64::MAX,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 0,
            ..MemoryContext::default()
        };
        let mut fault = GuestRegs::default();
        fault.gpr = initial_gprs;
        fault.rflags = INCOMING_FLAGS;
        fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
        fault.load_fn = load as usize as u64;
        fault.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut fault);

        assert_eq!(fault_context.loads, 1);
        assert_eq!(fault_context.stores, 1);
        assert_eq!(fault_context.store_value, 0, "computed value reaches store");
        assert_eq!(
            fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
            "failed store must not commit memory"
        );
        assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
        assert_eq!(
            fault.rflags & 0x8D5,
            INCOMING_FLAGS & 0x8D5,
            "fault must preserve every arithmetic status flag"
        );
        assert_eq!(fault.exit_pc, 0x1000, "fault must restart current PC");
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_helper_backed_memory_destination_unary_commits_flags_only_after_store() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        #[repr(C)]
        struct LoadResult {
            value: u64,
            ok: u64,
        }

        #[derive(Default)]
        struct MemoryContext {
            load_value: u64,
            store_value: u64,
            committed_value: u64,
            loads: u64,
            stores: u64,
            store_ok: u64,
        }

        extern "C" fn load(
            context: *mut MemoryContext,
            _addr: u64,
            _size: u64,
            _signed: u64,
        ) -> LoadResult {
            let context = unsafe { &mut *context };
            context.loads += 1;
            LoadResult {
                value: context.load_value,
                ok: 1,
            }
        }

        extern "C" fn store(
            context: *mut MemoryContext,
            _addr: u64,
            value: u64,
            _size: u64,
        ) -> u64 {
            let context = unsafe { &mut *context };
            context.stores += 1;
            context.store_value = value;
            if context.store_ok != 0 {
                context.committed_value = value;
            }
            context.store_ok
        }

        let old = VReg::Virtual(crate::smir::ir::types::VirtualId(50));
        let result = VReg::Virtual(crate::smir::ir::types::VirtualId(51));
        let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(52));
        let address = Address::Absolute(0x5000);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x2000);
        builder.push_op(
            0x2000,
            OpKind::Load {
                dst: old,
                addr: address.clone(),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x2000,
            OpKind::Inc {
                dst: result,
                src: old,
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x2000,
            OpKind::Store {
                src: result,
                addr: address,
                width: MemWidth::B8,
            },
        );
        builder.push_op(
            0x2000,
            OpKind::Inc {
                dst: flags_result,
                src: old,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower helper-backed scalar unary RMW");
        let exec = ExecMem::new(
            &lowerer
                .finalize()
                .expect("finalize helper-backed scalar unary RMW"),
        )
        .expect("map helper-backed scalar unary RMW");

        const INCOMING_FLAGS: u64 = 0x2 | 0x8D5;
        const INITIAL_VALUE: u64 = 0x7FFF_FFFF_FFFF_FFFF;
        const RESULT_VALUE: u64 = 0x8000_0000_0000_0000;
        let initial_gprs = {
            let mut gprs = [0u64; 32];
            for (index, value) in gprs.iter_mut().enumerate() {
                *value = 0xB600_0000_0000_0000 | index as u64;
            }
            gprs
        };

        let mut success_context = MemoryContext {
            load_value: INITIAL_VALUE,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 1,
            ..MemoryContext::default()
        };
        let mut success = GuestRegs::default();
        success.gpr = initial_gprs;
        success.rflags = INCOMING_FLAGS;
        success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
        success.ctx = (&mut success_context as *mut MemoryContext) as u64;
        success.load_fn = load as usize as u64;
        success.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut success);

        assert_eq!(success_context.loads, 1);
        assert_eq!(success_context.stores, 1);
        assert_eq!(success_context.store_value, RESULT_VALUE);
        assert_eq!(success_context.committed_value, RESULT_VALUE);
        assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
        assert_eq!(
            success.rflags & 0x8D5,
            0x895,
            "INC result flags must preserve incoming CF"
        );
        assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

        let mut fault_context = MemoryContext {
            load_value: INITIAL_VALUE,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 0,
            ..MemoryContext::default()
        };
        let mut fault = GuestRegs::default();
        fault.gpr = initial_gprs;
        fault.rflags = INCOMING_FLAGS;
        fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
        fault.load_fn = load as usize as u64;
        fault.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut fault);

        assert_eq!(fault_context.loads, 1);
        assert_eq!(fault_context.stores, 1);
        assert_eq!(fault_context.store_value, RESULT_VALUE);
        assert_eq!(
            fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
            "failed store must not commit memory"
        );
        assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
        assert_eq!(
            fault.rflags & 0x8D5,
            INCOMING_FLAGS & 0x8D5,
            "fault must preserve every arithmetic status flag"
        );
        assert_eq!(fault.exit_pc, 0x2000, "fault must restart current PC");
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn helper_backed_memory_destination_cl_shift_emits_both_native_replays() {
        let old = VReg::Virtual(crate::smir::ir::types::VirtualId(63));
        let result = VReg::Virtual(crate::smir::ir::types::VirtualId(64));
        let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(65));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let address = Address::Absolute(0x7000);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x4000);
        builder.push_op(
            0x4000,
            OpKind::Load {
                dst: old,
                addr: address.clone(),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x4000,
            OpKind::Shl {
                dst: result,
                src: old,
                amount: SrcOperand::Reg(rcx),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x4000,
            OpKind::Store {
                src: result,
                addr: address,
                width: MemWidth::B4,
            },
        );
        builder.push_op(
            0x4000,
            OpKind::Shl {
                dst: flags_result,
                src: old,
                amount: SrcOperand::Reg(rcx),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer
            .lower_function(&builder.finish())
            .expect("lower helper-backed CL shift RMW");
        let code = lowerer
            .finalize()
            .expect("finalize helper-backed CL shift RMW");
        assert_eq!(
            code.windows(2)
                .filter(|bytes| *bytes == [0xD3, 0xE0])
                .count(),
            2,
            "SHL EAX,CL must execute once speculatively and once after store"
        );
    }
    #[cfg(feature = "smir-jit")]
    #[test]
    fn helper_backed_subword_cl_shifts_emit_original_operand_boundary_cf_merge() {
        let lower = |tag: u8, mem_width: MemWidth| {
            let old = VReg::Virtual(crate::smir::ir::types::VirtualId(66));
            let result = VReg::Virtual(crate::smir::ir::types::VirtualId(67));
            let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(68));
            let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
            let address = Address::Absolute(0x7100);
            let width = mem_width.to_op_width().unwrap();
            let shift = |dst, src, flags| match tag {
                4 => OpKind::Shl {
                    dst,
                    src,
                    amount: SrcOperand::Reg(rcx),
                    width,
                    flags,
                },
                5 => OpKind::Shr {
                    dst,
                    src,
                    amount: SrcOperand::Reg(rcx),
                    width,
                    flags,
                },
                _ => unreachable!(),
            };
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x4100);
            builder.push_op(
                0x4100,
                OpKind::Load {
                    dst: old,
                    addr: address.clone(),
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            );
            builder.push_op(0x4100, shift(result, old, FlagUpdate::None));
            builder.push_op(
                0x4100,
                OpKind::Store {
                    src: result,
                    addr: address,
                    width: mem_width,
                },
            );
            builder.push_op(0x4100, shift(flags_result, old, FlagUpdate::All));
            builder.set_terminator(Terminator::Return { values: vec![] });

            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_mem_helpers(true);
            lowerer
                .lower_function(&builder.finish())
                .expect("lower helper-backed subword CL shift RMW");
            lowerer
                .finalize()
                .expect("finalize helper-backed subword CL shift RMW")
        };

        for (name, code, replay, boundary_merge) in [
            (
                "SHL r/m8,CL",
                lower(4, MemWidth::B1),
                &[0xD2, 0xE0][..],
                &[
                    0x48, 0x8B, 0x44, 0x24, 0x08, // mov rax,[rsp+8]
                    0x48, 0x83, 0xE0, 0x01, // and rax,1
                    0x48, 0x09, 0x04, 0x24, // or [rsp],rax
                ][..],
            ),
            (
                "SHR r/m16,CL",
                lower(5, MemWidth::B2),
                &[0x66, 0xD3, 0xE8][..],
                &[
                    0x48, 0x8B, 0x44, 0x24, 0x08, // mov rax,[rsp+8]
                    0x48, 0xC1, 0xE8, 0x0F, // shr rax,15
                    0x48, 0x83, 0xE0, 0x01, // and rax,1
                    0x48, 0x09, 0x04, 0x24, // or [rsp],rax
                ][..],
            ),
        ] {
            assert_eq!(
                code.windows(replay.len())
                    .filter(|bytes| *bytes == replay)
                    .count(),
                2,
                "{name} must execute once speculatively and once after store"
            );
            assert!(
                code.windows(boundary_merge.len())
                    .any(|bytes| bytes == boundary_merge),
                "{name} must reconstruct boundary CF from the original operand: {code:02X?}"
            );
        }
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_helper_backed_memory_destination_shift_commits_flags_only_after_store() {
        use crate::smir::lower::runtime::{ExecMem, GuestRegs};

        #[repr(C)]
        struct LoadResult {
            value: u64,
            ok: u64,
        }

        #[derive(Default)]
        struct MemoryContext {
            load_value: u64,
            store_value: u64,
            committed_value: u64,
            loads: u64,
            stores: u64,
            store_ok: u64,
        }

        extern "C" fn load(
            context: *mut MemoryContext,
            _addr: u64,
            _size: u64,
            _signed: u64,
        ) -> LoadResult {
            let context = unsafe { &mut *context };
            context.loads += 1;
            LoadResult {
                value: context.load_value,
                ok: 1,
            }
        }

        extern "C" fn store(context: *mut MemoryContext, _addr: u64, value: u64, size: u64) -> u64 {
            let context = unsafe { &mut *context };
            let value = match size {
                1 => value & u64::from(u8::MAX),
                2 => value & u64::from(u16::MAX),
                4 => value & u64::from(u32::MAX),
                8 => value,
                _ => return 0,
            };
            context.stores += 1;
            context.store_value = value;
            if context.store_ok != 0 {
                context.committed_value = value;
            }
            context.store_ok
        }

        let old = VReg::Virtual(crate::smir::ir::types::VirtualId(60));
        let result = VReg::Virtual(crate::smir::ir::types::VirtualId(61));
        let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(62));
        let address = Address::Absolute(0x6000);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x3000);
        builder.push_op(
            0x3000,
            OpKind::Load {
                dst: old,
                addr: address.clone(),
                width: MemWidth::B2,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x3000,
            OpKind::Shl {
                dst: result,
                src: old,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x3000,
            OpKind::Store {
                src: result,
                addr: address,
                width: MemWidth::B2,
            },
        );
        builder.push_op(
            0x3000,
            OpKind::Shl {
                dst: flags_result,
                src: old,
                amount: SrcOperand::Imm(4),
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        let lowered = lowerer
            .lower_function(&builder.finish())
            .expect("lower helper-backed scalar shift RMW");
        let exec = ExecMem::new(
            &lowerer
                .finalize()
                .expect("finalize helper-backed scalar shift RMW"),
        )
        .expect("map helper-backed scalar shift RMW");

        const INCOMING_FLAGS: u64 = 0x2 | 0x8D5;
        const INITIAL_VALUE: u64 = 0xF123;
        const RESULT_VALUE: u64 = 0x1230;
        const RESULT_STATUS: u64 = 0x15; // CF | PF | preserved AF
        let initial_gprs = {
            let mut gprs = [0u64; 32];
            for (index, value) in gprs.iter_mut().enumerate() {
                *value = 0xC700_0000_0000_0000 | index as u64;
            }
            gprs
        };

        let mut success_context = MemoryContext {
            load_value: INITIAL_VALUE,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 1,
            ..MemoryContext::default()
        };
        let mut success = GuestRegs::default();
        success.gpr = initial_gprs;
        success.rflags = INCOMING_FLAGS;
        success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
        success.ctx = (&mut success_context as *mut MemoryContext) as u64;
        success.load_fn = load as usize as u64;
        success.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut success);

        assert_eq!(success_context.loads, 1);
        assert_eq!(success_context.stores, 1);
        assert_eq!(success_context.store_value, RESULT_VALUE);
        assert_eq!(success_context.committed_value, RESULT_VALUE);
        assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
        assert_eq!(success.rflags & 0x8D5, RESULT_STATUS, "SHL result flags");
        assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

        let mut fault_context = MemoryContext {
            load_value: INITIAL_VALUE,
            committed_value: 0xDEAD_BEEF_DEAD_BEEF,
            store_ok: 0,
            ..MemoryContext::default()
        };
        let mut fault = GuestRegs::default();
        fault.gpr = initial_gprs;
        fault.rflags = INCOMING_FLAGS;
        fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
        fault.load_fn = load as usize as u64;
        fault.store_fn = store as usize as u64;
        exec.run(lowered.entry_offset, &mut fault);

        assert_eq!(fault_context.loads, 1);
        assert_eq!(fault_context.stores, 1);
        assert_eq!(fault_context.store_value, RESULT_VALUE);
        assert_eq!(
            fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
            "failed store must not commit memory"
        );
        assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
        assert_eq!(
            fault.rflags & 0x8D5,
            INCOMING_FLAGS & 0x8D5,
            "fault must preserve every arithmetic status flag"
        );
        assert_eq!(fault.exit_pc, 0x3000, "fault must restart current PC");
    }
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    #[test]
    fn native_state_backed_crc32c_preserves_gprs_and_flags() {
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

        struct Case {
            name: &'static str,
            dst: X86Reg,
            data: X86Reg,
            width: OpWidth,
            accumulator: u64,
            source: u64,
        }
        let cases = [
            Case {
                name: "CRC32 EBP,BPL alias",
                dst: X86Reg::Rbp,
                data: X86Reg::Rbp,
                width: OpWidth::W8,
                accumulator: 0x1234_56A5,
                source: 0x1234_56A5,
            },
            Case {
                name: "CRC32 ESP,BP",
                dst: X86Reg::Rsp,
                data: X86Reg::Rbp,
                width: OpWidth::W16,
                accumulator: 0x89AB_CDEF,
                source: 0x0123_4567_89AB_BEEF,
            },
            Case {
                name: "CRC32 EBP,ESP",
                dst: X86Reg::Rbp,
                data: X86Reg::Rsp,
                width: OpWidth::W32,
                accumulator: 0x1020_3040,
                source: 0xAABB_CCDD_DEAD_BEEF,
            },
            Case {
                name: "CRC32 RSP,RBP",
                dst: X86Reg::Rsp,
                data: X86Reg::Rbp,
                width: OpWidth::W64,
                accumulator: 0x7654_3210,
                source: 0x0123_4567_89AB_CDEF,
            },
            Case {
                name: "state-backed CRC32 R31,R16",
                dst: X86Reg::R31,
                data: X86Reg::R16,
                width: OpWidth::W64,
                accumulator: 0xA5A5_5A5A,
                source: 0xFEDC_BA98_7654_3210,
            },
        ];
        let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
        const FLAGS: u64 = 0x8D5;

        for case in cases {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Crc32C {
                    dst: x86(case.dst),
                    crc: x86(case.dst),
                    data: x86(case.data),
                    data_width: case.width,
                },
            );
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&builder.finish())
                .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
            let exec = ExecMem::new(&code)
                .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

            let mut regs = GuestRegs::default();
            for (index, value) in regs.gpr.iter_mut().enumerate() {
                *value = 0x1357_0000_2468_0000u64
                    .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
            }
            let dst_idx = case.dst.gpr_index().unwrap() as usize;
            let data_idx = case.data.gpr_index().unwrap() as usize;
            regs.gpr[dst_idx] = case.accumulator;
            if data_idx != dst_idx {
                regs.gpr[data_idx] = case.source;
            }
            regs.rflags = 0x2 | FLAGS;

            let mut expected = regs;
            expected.gpr[dst_idx] = u64::from(crc32c(
                regs.gpr[dst_idx] as u32,
                regs.gpr[data_idx],
                case.width,
            ));

            exec.run(lowered.entry_offset, &mut regs);

            assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
            assert_eq!(
                regs.rflags & FLAGS,
                expected.rflags & FLAGS,
                "{} flags",
                case.name
            );
        }
    }
