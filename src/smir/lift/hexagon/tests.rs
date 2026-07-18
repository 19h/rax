//! tests.rs

    use super::*;

    struct MockMemory {
        data: Vec<u8>,
        base: GuestAddr,
    }

    impl MemoryReader for MockMemory {
        fn read(
            &self,
            addr: GuestAddr,
            size: usize,
        ) -> Result<Vec<u8>, crate::smir::ir::memory::MemoryError> {
            let offset = (addr - self.base) as usize;
            if offset + size > self.data.len() {
                return Err(crate::smir::ir::memory::MemoryError::OutOfBounds { addr });
            }
            Ok(self.data[offset..offset + size].to_vec())
        }
    }

    #[test]
    fn test_hexagon_lifter_add() {
        let mut lifter = HexagonLifter::default_isa();
        let mut ctx = LiftContext::new(SourceArch::Hexagon);

        // R0 = add(R1, R2) - encoded as a test
        // This is a simplified test - actual encoding would need the real opcode
        let bytes = [0x00u8, 0x00, 0x00, 0x00]; // Placeholder

        // We can't easily test without the actual decoder, but we can test the lifter structure
        assert_eq!(lifter.source_arch(), SourceArch::Hexagon);
    }

    #[test]
    fn test_lift_context_hexagon() {
        let mut ctx = LiftContext::new(SourceArch::Hexagon);

        // Test extended immediate
        ctx.set_extended_imm(0x12345);
        let extended = ctx.extend_imm(0x20);
        assert_eq!(extended, (0x12345i32 << 6) | 0x20);

        // Extension should be consumed
        let not_extended = ctx.extend_imm(0x30);
        assert_eq!(not_extended, 0x30);
    }

    // Regression for issue #106: S2_mask's width (#u5) and offset (#U5) are
    // UNEXTENDED fields. A guest A4_ext before S2_mask leaves a pending extender;
    // the lift must ignore it for these fields (matching the interpreter, which
    // reads them without immext) rather than folding it in via fimm_u — which would
    // drive the shift amount well past 63 and panic the host on `1u64 << width` in
    // overflow-checked builds.
    #[test]
    fn issue_106_s2_mask_ignores_immext_and_does_not_panic() {
        // `lift_unknown_op` has a very large stack frame in debug builds, so run the
        // lift on a thread with a generous stack: a plain 2 MiB test thread would
        // overflow on the frame itself, which is unrelated to the bug under test.
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                fn mask_imm(ops: &[SmirOp]) -> Option<i64> {
                    ops.iter().find_map(|op| match &op.kind {
                        OpKind::Mov {
                            src: SrcOperand::Imm(m),
                            ..
                        } => Some(*m),
                        _ => None,
                    })
                }

                // S2_mask: R1 = mask(#u5=4, #U5=0) -> ((1<<4)-1)<<0 = 0xF.
                // Encoding 0x8d00e401 = base 0x8d002000 | parse[15:14]=11 (single
                // insn, end-of-packet) | width(=4 at bits[12:8]) | Rd(=1 at [4:0]).
                let word = 0x8d00_e401u32.to_le_bytes();

                // Baseline (no pending extender): mask = 0xF.
                let mut lifter = HexagonLifter::default_isa();
                let mut ctx = LiftContext::new(SourceArch::Hexagon);
                let base = lifter.lift_insn(0x1000, &word, &mut ctx).unwrap();
                assert_eq!(
                    mask_imm(&base.ops),
                    Some(0xF),
                    "baseline S2_mask mask value"
                );

                // With a pending maximum-width extender: the result must be
                // UNCHANGED and, critically, the lift must not panic on an
                // out-of-range shift (the pre-fix `fimm_u` made width ~
                // 0x3ff_ffff << 6, panicking `1u64 << width` in checked builds).
                let mut lifter = HexagonLifter::default_isa();
                let mut ctx = LiftContext::new(SourceArch::Hexagon);
                ctx.set_extended_imm(0x03ff_ffff);
                let ext = lifter.lift_insn(0x1000, &word, &mut ctx).unwrap();
                assert_eq!(
                    mask_imm(&ext.ops),
                    Some(0xF),
                    "a pending immext must not change S2_mask width/offset (no panic)",
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    fn assert_unsupported(
        result: Result<(Vec<SmirOp>, ControlFlow), LiftError>,
        expected_mnemonic: &str,
    ) {
        match result {
            Err(LiftError::Unsupported { mnemonic, .. }) => {
                assert_eq!(mnemonic, expected_mnemonic);
            }
            other => panic!("expected Unsupported({expected_mnemonic}), got {other:?}"),
        }
    }

    fn hex_r(reg: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::R(reg)))
    }

    fn hex_m(reg: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::M(reg)))
    }

    fn hex_v(reg: u8) -> VReg {
        VReg::Arch(ArchReg::Hexagon(HexagonReg::V(reg)))
    }

    fn set_decoded_field(word: &mut u32, dop: &DecodedOp, letter: u8, value: u32) {
        let field = dop
            .fields
            .iter()
            .find(|field| field.letter == letter)
            .unwrap_or_else(|| panic!("field {} not found in {dop:?}", letter as char));
        assert!((value as u64) < (1u64 << field.bits.len()));
        for (idx, &bit) in field.bits.iter().enumerate() {
            let shift = field.bits.len() - 1 - idx;
            if ((value >> shift) & 1) != 0 {
                *word |= 1u32 << bit;
            } else {
                *word &= !(1u32 << bit);
            }
        }
    }

    fn lift_decoded(insn: DecodedInsn) -> Vec<SmirOp> {
        let mut lifter = HexagonLifter::default_isa();
        let mut ctx = LiftContext::new(SourceArch::Hexagon);
        let (ops, flow) = lifter.lift_insn_inner(&insn, 0x1000, &mut ctx).unwrap();
        assert!(matches!(flow, ControlFlow::Fallthrough));
        ops
    }

    #[track_caller]
    fn op_index<F>(ops: &[SmirOp], mut pred: F) -> usize
    where
        F: FnMut(&OpKind) -> bool,
    {
        ops.iter()
            .position(|op| pred(&op.kind))
            .unwrap_or_else(|| panic!("operation not found in {ops:#?}"))
    }

    #[track_caller]
    fn op_index_after<F>(ops: &[SmirOp], start: usize, mut pred: F) -> usize
    where
        F: FnMut(&OpKind) -> bool,
    {
        start
            + 1
            + ops[start + 1..]
                .iter()
                .position(|op| pred(&op.kind))
                .unwrap_or_else(|| panic!("operation after {start} not found in {ops:#?}"))
    }

    #[track_caller]
    fn materialized_ea_from(ops: &[SmirOp], base: VReg) -> (usize, VReg) {
        ops.iter()
            .enumerate()
            .find_map(|(idx, op)| match &op.kind {
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(src),
                    width: OpWidth::W32,
                } if *src == base && dst.is_virtual() => Some((idx, *dst)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("EA materialization from {base:?} not found in {ops:#?}"))
    }

    #[track_caller]
    fn assert_vaddb_lane(kind: &OpKind, reg: u8) {
        match kind {
            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op,
                signed,
                set_ovf,
            } => {
                assert_eq!(*dst, hex_v(reg));
                assert_eq!(*src1, hex_v(reg));
                assert_eq!(*src2, hex_v(reg));
                assert_eq!(*elem, VecElementType::I8);
                assert_eq!(*lanes, 128);
                assert_eq!(*op, VLaneOp::Add);
                assert!(!signed);
                assert!(!set_ovf);
            }
            other => panic!("expected VLane vaddb for V{reg}, got {other:?}"),
        }
    }

    #[test]
    fn hvx_vlane_dv_normalizes_raw_odd_pair_fields() {
        // `lift_unknown_op` has a very large stack frame in debug builds.
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let mut word = 0x1c60_0080u32;
                let base = decode_word(word).expect("V6_vaddb_dv base word decodes");
                assert_eq!(base.opcode, Opcode::V6_vaddb_dv);

                for letter in [b'd', b'u', b'v'] {
                    set_decoded_field(&mut word, &base, letter, 31);
                }

                let dop = decode_word(word).expect("mutated V6_vaddb_dv word decodes");
                assert_eq!(dop.opcode, Opcode::V6_vaddb_dv);
                assert_eq!(dop.field(b'd').unwrap().value, 31);
                assert_eq!(dop.field(b'u').unwrap().value, 31);
                assert_eq!(dop.field(b'v').unwrap().value, 31);

                let mut lifter = HexagonLifter::default_isa();
                let mut ctx = LiftContext::new(SourceArch::Hexagon);
                let (ops, flow) = lifter
                    .lift_insn_inner(&DecodedInsn::Unknown(word), 0x1000, &mut ctx)
                    .unwrap();

                assert!(matches!(flow, ControlFlow::Fallthrough));
                assert_eq!(ops.len(), 2);
                assert_vaddb_lane(&ops[0].kind, 30);
                assert_vaddb_lane(&ops[1].kind, 31);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn faulting_postinc_imm_load_does_not_commit_base_update() {
        let ops = lift_decoded(DecodedInsn::Load {
            dst: 0,
            addr: AddrMode::PostIncImm { base: 0, offset: 4 },
            width: HexMemWidth::Word,
            sign: MemSign::Unsigned,
            pred: None,
        });
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops,
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        let mut ctx = crate::smir::ir::context::SmirContext::new_hexagon();
        ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(0)), 0x2000);
        let mut memory = crate::smir::FlatMemory::new(0x1000);
        let interp = crate::smir::interpret::SmirInterpreter::new();

        let exit = interp.execute_block(&mut ctx, &mut memory, &block);

        match exit {
            crate::smir::interpret::BlockResult::Exit(
                crate::smir::ir::context::ExitReason::MemoryFault { addr, write },
            ) => {
                assert_eq!(addr, 0x2000);
                assert!(!write);
            }
            other => panic!("expected memory fault, got {other:?}"),
        }
        assert_eq!(
            ctx.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(0))),
            0x2000
        );
    }

    #[test]
    fn postinc_imm_load_alias_commits_writeback_after_successful_load() {
        let ops = lift_decoded(DecodedInsn::Load {
            dst: 0,
            addr: AddrMode::PostIncImm { base: 0, offset: 4 },
            width: HexMemWidth::Word,
            sign: MemSign::Unsigned,
            pred: None,
        });
        let r0 = hex_r(0);
        let (ea_idx, ea) = materialized_ea_from(&ops, r0);
        let staged_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(4),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                } if dst.is_virtual() && *src1 == r0
            )
        });
        let load_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::Load {
                    dst,
                    addr: Address::Direct(addr),
                    width: MemWidth::B4,
                    sign: SignExtend::Zero,
                } if dst.is_virtual() && *addr == ea
            )
        });
        let commit_idx = op_index_after(&ops, load_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });
        let final_idx = op_index_after(&ops, commit_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });

        assert!(
            ea_idx < staged_idx
                && staged_idx < load_idx
                && load_idx < commit_idx
                && commit_idx < final_idx,
            "expected staged update, load, committed update, then aliased destination: {ops:#?}"
        );
    }

    #[test]
    fn predicated_postinc_imm_load_alias_commits_after_predload() {
        let ops = lift_decoded(DecodedInsn::Load {
            dst: 0,
            addr: AddrMode::PostIncImm { base: 0, offset: 4 },
            width: HexMemWidth::Word,
            sign: MemSign::Unsigned,
            pred: Some(crate::isa::hexagon::decode::PredCond {
                pred: 0,
                sense: true,
                pred_new: false,
            }),
        });
        let r0 = hex_r(0);
        let (ea_idx, ea) = materialized_ea_from(&ops, r0);
        let load_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::PredLoad {
                    dst,
                    addr: Address::Direct(addr),
                    width: MemWidth::B4,
                    signed: SignExtend::Zero,
                    ..
                } if dst.is_virtual() && *addr == ea
            )
        });
        let update_idx = op_index_after(&ops, load_idx, |kind| {
            matches!(
                kind,
                OpKind::Select {
                    dst,
                    src_false,
                    width: OpWidth::W32,
                    ..
                } if *dst == r0 && *src_false == r0
            )
        });
        let final_idx = op_index_after(&ops, update_idx, |kind| {
            matches!(
                kind,
                OpKind::Select {
                    dst,
                    src_false,
                    width: OpWidth::W32,
                    ..
                } if *dst == r0 && *src_false == r0
            )
        });

        assert!(
            ea_idx < load_idx && load_idx < update_idx && update_idx < final_idx,
            "expected predload, gated base update, then gated aliased destination: {ops:#?}"
        );
    }

    #[test]
    fn postinc_reg_loadpair_alias_commits_writeback_after_loadpair() {
        let ops = lift_decoded(DecodedInsn::Load {
            dst: 0,
            addr: AddrMode::PostIncReg { base: 0, modsel: 0 },
            width: HexMemWidth::Double,
            sign: MemSign::Unsigned,
            pred: None,
        });
        let r0 = hex_r(0);
        let (ea_idx, ea) = materialized_ea_from(&ops, r0);
        let staged_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                } if dst.is_virtual() && *src1 == r0 && *src2 == hex_m(0)
            )
        });
        let load_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::LoadPair {
                    dst1,
                    dst2,
                    addr: Address::Direct(addr),
                    width: MemWidth::B4,
                } if dst1.is_virtual() && dst2.is_virtual() && *addr == ea
            )
        });
        let commit_idx = op_index_after(&ops, load_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });
        let final_idx = op_index_after(&ops, commit_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });

        assert!(
            ea_idx < staged_idx
                && staged_idx < load_idx
                && load_idx < commit_idx
                && commit_idx < final_idx,
            "expected staged M0 writeback, load pair, committed update, then aliased pair write: {ops:#?}"
        );
    }

    #[test]
    fn loadalign_postinc_alias_uses_staged_update_before_pair_write() {
        let ops = lift_decoded(DecodedInsn::LoadAlign {
            dst_pair: 0,
            addr: AddrMode::PostIncImm { base: 0, offset: 1 },
            width: HexMemWidth::Byte,
            pred: None,
        });
        let r0 = hex_r(0);
        let (ea_idx, ea) = materialized_ea_from(&ops, r0);
        let (staged_idx, staged_update) = ops
            .iter()
            .enumerate()
            .find_map(|(idx, op)| match &op.kind {
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                } if dst.is_virtual() && *src1 == r0 => Some((idx, *dst)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("staged loadalign update not found in {ops:#?}"));
        let load_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::Load {
                    addr: Address::Direct(addr),
                    width: MemWidth::B1,
                    sign: SignExtend::Zero,
                    ..
                } if *addr == ea
            )
        });
        let old_pair_read_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::Or {
                    src1,
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                    ..
                } if *src1 == staged_update
            )
        });
        let commit_idx = op_index_after(&ops, load_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });
        let final_idx = op_index_after(&ops, commit_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });

        assert!(
            ea_idx < staged_idx
                && staged_idx < old_pair_read_idx
                && load_idx < commit_idx
                && commit_idx < final_idx,
            "expected loadalign to use staged base for alias, then commit after load: {ops:#?}"
        );
    }

    #[test]
    fn loadunpack_postinc_alias_commits_writeback_after_byte_loads() {
        let ops = lift_decoded(DecodedInsn::LoadUnpack {
            dst: 0,
            addr: AddrMode::PostIncReg { base: 0, modsel: 0 },
            count: 2,
            sign: MemSign::Unsigned,
            pred: None,
        });
        let r0 = hex_r(0);
        let (ea_idx, ea) = materialized_ea_from(&ops, r0);
        let staged_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::Add {
                    dst,
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                } if dst.is_virtual() && *src1 == r0 && *src2 == hex_m(0)
            )
        });
        let first_load_idx = op_index(&ops, |kind| {
            matches!(
                kind,
                OpKind::Load {
                    addr: Address::Direct(addr),
                    width: MemWidth::B1,
                    sign: SignExtend::Zero,
                    ..
                } if *addr == ea
            )
        });
        let commit_idx = op_index_after(&ops, first_load_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });
        let final_write_idx = op_index_after(&ops, commit_idx, |kind| {
            matches!(
                kind,
                OpKind::Mov {
                    dst,
                    src: SrcOperand::Reg(_),
                    width: OpWidth::W32,
                } if *dst == r0
            )
        });

        assert!(
            ea_idx < staged_idx
                && staged_idx < first_load_idx
                && first_load_idx < commit_idx
                && commit_idx < final_write_idx,
            "expected unpack byte load before committed update and aliased write: {ops:#?}"
        );
    }

    #[test]
    fn test_hexagon_control_pair_rejects_odd_creg_base() {
        let mut lifter = HexagonLifter::default_isa();

        let mut ctx = LiftContext::new(SourceArch::Hexagon);
        assert_unsupported(
            lifter.lift_insn_inner(
                &DecodedInsn::TfrCrRPair { dst: 0, src: 11 },
                0x1000,
                &mut ctx,
            ),
            "tfrcpp",
        );

        let mut ctx = LiftContext::new(SourceArch::Hexagon);
        assert_unsupported(
            lifter.lift_insn_inner(
                &DecodedInsn::TfrRrCrPair { dst: 11, src: 0 },
                0x1000,
                &mut ctx,
            ),
            "tfrpcp",
        );
    }

    #[test]
    fn test_hexagon_creg_value_write_masks_gp() {
        let lifter = HexagonLifter::default_isa();
        let mut ops = Vec::new();
        let mut op_id = 0;
        let mut ctx = LiftContext::new(SourceArch::Hexagon);
        let gp = VReg::Arch(ArchReg::Hexagon(HexagonReg::Gp));
        let r3 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(3)));

        lifter.emit_creg_value_write(&mut ops, &mut op_id, 0x1000, &mut ctx, gp, r3);

        assert_eq!(ops.len(), 2);
        let masked = match &ops[0].kind {
            OpKind::And {
                dst,
                src1,
                src2,
                width,
                flags,
            } => {
                assert_eq!(*src1, r3);
                assert_eq!(*src2, SrcOperand::Imm(0xffff_ffc0u32 as i64));
                assert_eq!(*width, OpWidth::W32);
                assert_eq!(*flags, FlagUpdate::None);
                *dst
            }
            other => panic!("expected GP mask And, got {other:?}"),
        };

        match &ops[1].kind {
            OpKind::Mov { dst, src, width } => {
                assert_eq!(*dst, gp);
                assert_eq!(*src, SrcOperand::Reg(masked));
                assert_eq!(*width, OpWidth::W32);
            }
            other => panic!("expected GP Mov, got {other:?}"),
        }
    }

    #[test]
    fn store_release_lifts_as_unconditional_store() {
        let mut lifter = HexagonLifter::default_isa();
        let mut ctx = LiftContext::new(SourceArch::Hexagon);
        let insn = DecodedInsn::StoreCond {
            src: 3,
            base: 4,
            width: HexMemWidth::Word,
            success_pred: None,
        };

        let (ops, flow) = lifter.lift_insn_inner(&insn, 0x1000, &mut ctx).unwrap();

        assert!(matches!(flow, ControlFlow::Fallthrough));
        assert_eq!(ops.len(), 1);
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::StoreExclusive { .. }))
        );
        match &ops[0].kind {
            OpKind::Store { src, addr, width } => {
                assert_eq!(*src, lifter.hex_reg(3));
                assert!(matches!(addr, Address::Direct(reg) if *reg == lifter.hex_reg(4)));
                assert_eq!(*width, MemWidth::B4);
            }
            other => panic!("expected release store to lift as Store, got {other:?}"),
        }
    }

    #[test]
    fn store_conditional_still_lifts_as_exclusive_store() {
        let mut lifter = HexagonLifter::default_isa();
        let mut ctx = LiftContext::new(SourceArch::Hexagon);
        let insn = DecodedInsn::StoreCond {
            src: 3,
            base: 4,
            width: HexMemWidth::Word,
            success_pred: Some(2),
        };

        let (ops, flow) = lifter.lift_insn_inner(&insn, 0x1000, &mut ctx).unwrap();

        assert!(matches!(flow, ControlFlow::Fallthrough));
        assert!(ops.iter().any(|op| matches!(
            &op.kind,
            OpKind::StoreExclusive {
                src,
                addr,
                width,
                ..
            } if *src == lifter.hex_reg(3)
                && matches!(addr, Address::Direct(reg) if *reg == lifter.hex_reg(4))
                && *width == MemWidth::B4
        )));
        assert!(!ops.iter().any(|op| matches!(op.kind, OpKind::Store { .. })));
        assert!(ops.iter().any(|op| matches!(
            &op.kind,
            OpKind::And {
                dst,
                src2: SrcOperand::Imm(0xff),
                width: OpWidth::W32,
                ..
            } if *dst == lifter.hex_pred(2)
        )));
    }
