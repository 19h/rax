//! tests::constant tests

use super::*;
use crate::smir::optimize::*;

    #[test]
    fn test_constant_propagation() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let v2 = VReg::virt(2);

        // mov v0, 10
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Imm(10),
                width: OpWidth::W64,
            },
        ));

        // mov v1, v0 (should propagate to mov v1, 10)
        block.push_op(make_op(
            1,
            OpKind::Mov {
                dst: v1,
                src: SrcOperand::Reg(v0),
                width: OpWidth::W64,
            },
        ));

        // add v2, v1, v0 (v0 should be replaced with 10)
        block.push_op(make_op(
            2,
            OpKind::Add {
                dst: v2,
                src1: v1,
                src2: SrcOperand::Reg(v0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v2] });

        let propagated = constant_propagation(&mut block);

        assert!(propagated >= 2);

        // Check that v0 in add was replaced with immediate
        if let OpKind::Add { src2, .. } = &block.ops[2].kind {
            assert!(matches!(src2, SrcOperand::Imm(10)));
        }
    }
    #[test]
    fn constant_propagation_folds_crc32c_and_propagates_zero_extended_result() {
        let crc = VReg::virt(0);
        let data = VReg::virt(1);
        let result = VReg::virt(2);
        let copy = VReg::virt(3);
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: crc,
                src: SrcOperand::Imm(0x89AB_CDEF_u32 as i64),
                width: OpWidth::W32,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::Mov {
                dst: data,
                src: SrcOperand::Imm(0x0123_4567),
                width: OpWidth::W32,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::Crc32C {
                dst: result,
                crc,
                data,
                data_width: OpWidth::W32,
            },
        ));
        block.push_op(make_op(
            3,
            OpKind::Mov {
                dst: copy,
                src: SrcOperand::Reg(result),
                width: OpWidth::W64,
            },
        ));

        assert_eq!(constant_propagation(&mut block), 2);
        assert!(matches!(
            block.ops[2].kind,
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0x796A_B9A9),
                width: OpWidth::W64,
            } if dst == result
        ));
        assert!(matches!(
            block.ops[3].kind,
            OpKind::Mov {
                dst,
                src: SrcOperand::Imm(0x796A_B9A9),
                width: OpWidth::W64,
            } if dst == copy
        ));
        assert!(op_fully_defines(&OpKind::Crc32C {
            dst: result,
            crc,
            data,
            data_width: OpWidth::W64,
        }));
    }
    #[test]
    fn aarch32_selective_flags_and_shift_32_survive_and_propagate_exactly() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let src = VReg::virt(0);
        let movs = VReg::virt(1);
        let lsr = VReg::virt(2);
        let asr = VReg::virt(3);
        let lsr_copy = VReg::virt(4);
        let asr_copy = VReg::virt(5);
        let nz = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF));
        let nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));

        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: src,
                src: SrcOperand::Imm(0x8000_0001_u32 as i64),
                width: OpWidth::W32,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::And {
                dst: movs,
                src1: src,
                src2: SrcOperand::Imm(-1),
                width: OpWidth::W32,
                flags: nz,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::Shr {
                dst: lsr,
                src,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W32,
                flags: nzc,
            },
        ));
        block.push_op(make_op(
            3,
            OpKind::Sar {
                dst: asr,
                src,
                amount: SrcOperand::Imm(32),
                width: OpWidth::W32,
                flags: nzc,
            },
        ));
        block.push_op(make_op(
            4,
            OpKind::Mov {
                dst: lsr_copy,
                src: SrcOperand::Reg(lsr),
                width: OpWidth::W32,
            },
        ));
        block.push_op(make_op(
            5,
            OpKind::Mov {
                dst: asr_copy,
                src: SrcOperand::Reg(asr),
                width: OpWidth::W32,
            },
        ));

        assert!(constant_propagation(&mut block) >= 2);
        assert_eq!(constant_folding(&mut block), 0);
        assert!(matches!(
            block.ops[1].kind,
            OpKind::And { flags, .. } if flags == nz
        ));
        assert!(matches!(
            block.ops[2].kind,
            OpKind::Shr { flags, .. } if flags == nzc
        ));
        assert!(matches!(
            block.ops[3].kind,
            OpKind::Sar { flags, .. } if flags == nzc
        ));
        assert!(matches!(
            block.ops[4].kind,
            OpKind::Mov {
                src: SrcOperand::Imm(0),
                ..
            }
        ));
        assert!(matches!(
            block.ops[5].kind,
            OpKind::Mov {
                src: SrcOperand::Imm(0xffff_ffff),
                ..
            }
        ));
    }
    #[test]
    fn aarch32_register_shift_metadata_and_low_byte_constants_are_exact() {
        let nzc = FlagUpdate::Specific(FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF));
        let data = VReg::virt(0);
        let count = VReg::virt(1);
        let copy = VReg::virt(2);

        let metadata = OpKind::ArmRegShift {
            dst: data,
            src: data,
            amount: SrcOperand::Reg(count),
            shift: ShiftOp::Lsl,
            width: OpWidth::W32,
            flags: nzc,
        };
        assert_eq!(metadata.dests(), vec![data]);
        assert_eq!(metadata.source_vregs(), vec![data, count]);
        assert_eq!(metadata.flags_written(), nzc.as_set());
        assert_eq!(metadata.flags_must_write(), nzc.as_set());
        assert_eq!(metadata.flags_read(), FlagSet::CF);

        let flagless_metadata = OpKind::ArmRegShift {
            dst: copy,
            src: data,
            amount: SrcOperand::Reg(count),
            shift: ShiftOp::Ror,
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        };
        assert_eq!(flagless_metadata.flags_read(), FlagSet::EMPTY);

        for (shift, expected) in [
            (ShiftOp::Lsl, 0_i64),
            (ShiftOp::Lsr, 0),
            (ShiftOp::Asr, i64::from(u32::MAX)),
            (ShiftOp::Ror, i64::from(0x8000_0001_u32)),
        ] {
            let mut block = SmirBlock::new(BlockId(0), 0x1000);
            block.push_op(make_op(
                0,
                OpKind::Mov {
                    dst: data,
                    src: SrcOperand::Imm(i64::from(0x8000_0001_u32)),
                    width: OpWidth::W32,
                },
            ));
            block.push_op(make_op(
                1,
                OpKind::Mov {
                    dst: count,
                    src: SrcOperand::Imm(0x120),
                    width: OpWidth::W32,
                },
            ));
            block.push_op(make_op(
                2,
                OpKind::ArmRegShift {
                    dst: data,
                    src: data,
                    amount: SrcOperand::Reg(count),
                    shift,
                    width: OpWidth::W32,
                    flags: nzc,
                },
            ));
            block.push_op(make_op(
                3,
                OpKind::Mov {
                    dst: copy,
                    src: SrcOperand::Reg(data),
                    width: OpWidth::W32,
                },
            ));
            block.set_terminator(Terminator::Return { values: vec![copy] });

            assert!(constant_propagation(&mut block) >= 2);
            assert!(matches!(
                block.ops[2].kind,
                OpKind::ArmRegShift {
                    amount: SrcOperand::Imm(0x120),
                    flags,
                    ..
                } if flags == nzc
            ));
            assert!(matches!(
                block.ops[3].kind,
                OpKind::Mov {
                    src: SrcOperand::Imm(value),
                    ..
                } if value == expected
            ));
        }

        let mut live_flags = SmirBlock::new(BlockId(0), 0x1000);
        live_flags.push_op(make_op(0, metadata));
        live_flags.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(dead_code_elimination(&mut live_flags), 0);
        assert_eq!(live_flags.ops.len(), 1);

        let mut dead_flags = SmirBlock::new(BlockId(0), 0x1000);
        dead_flags.push_op(make_op(
            0,
            OpKind::ArmRegShift {
                dst: data,
                src: data,
                amount: SrcOperand::Reg(count),
                shift: ShiftOp::Lsr,
                width: OpWidth::W32,
                flags: nzc,
            },
        ));
        dead_flags.set_terminator(Terminator::Return { values: vec![data] });
        assert_eq!(dead_flag_elimination(&mut dead_flags), 1);
        assert!(matches!(
            dead_flags.ops[0].kind,
            OpKind::ArmRegShift {
                flags: FlagUpdate::None,
                ..
            }
        ));

        let mut flagless_consumer = SmirBlock::new(BlockId(0), 0x1000);
        flagless_consumer.push_op(make_op(
            0,
            OpKind::And {
                dst: data,
                src1: VReg::Imm(0x8000_0001),
                src2: SrcOperand::Imm(-1),
                width: OpWidth::W32,
                flags: nzc,
            },
        ));
        flagless_consumer.push_op(make_op(1, flagless_metadata));
        flagless_consumer.set_terminator(Terminator::Return { values: vec![copy] });
        assert_eq!(dead_flag_elimination(&mut flagless_consumer), 1);
        assert!(matches!(
            flagless_consumer.ops[0].kind,
            OpKind::And {
                flags: FlagUpdate::None,
                ..
            }
        ));
    }
    #[test]
    fn test_constant_folding() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);

        // and v0, v1, 0 -> mov v0, 0
        block.push_op(make_op(
            0,
            OpKind::And {
                dst: v0,
                src1: v1,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v0] });

        let folded = constant_folding(&mut block);

        assert_eq!(folded, 1);

        // Check it was folded to a mov
        if let OpKind::Mov { src, .. } = &block.ops[0].kind {
            assert!(matches!(src, SrcOperand::Imm(0)));
        } else {
            panic!("Expected Mov operation");
        }
    }
    #[test]
    fn folds_evex_ternary_projections_and_zero_reduced_immediates() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let dst = VReg::virt(0);
        let src1 = VReg::virt(1);
        let src2 = VReg::virt(2);
        let src3 = VReg::virt(3);
        block.push_op(make_op(
            0,
            OpKind::X86TernaryLogic {
                dst,
                src1,
                src2,
                src3,
                mask: None,
                imm: 0xAA,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                zeroing: false,
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::X86PackedRotate {
                dst,
                src: src1,
                count: None,
                mask: None,
                amount: 64,
                width: VecWidth::V512,
                elem: VecElementType::I32,
                left: true,
                zeroing: false,
            },
        ));
        block.push_op(make_op(
            2,
            OpKind::X86PackedFunnelShift {
                dst,
                src: src2,
                fill: src3,
                count: None,
                mask: None,
                amount: 128,
                width: VecWidth::V512,
                elem: VecElementType::I64,
                left: false,
                zeroing: false,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });

        assert_eq!(constant_folding(&mut block), 3);
        assert!(matches!(
            block.ops[0].kind,
            OpKind::VMov { src, width: VecWidth::V512, .. } if src == src3
        ));
        assert!(matches!(
            block.ops[1].kind,
            OpKind::VMov { src, width: VecWidth::V512, .. } if src == src1
        ));
        assert!(matches!(
            block.ops[2].kind,
            OpKind::VMov { src, width: VecWidth::V512, .. } if src == src2
        ));
    }
    #[test]
    fn test_xor_same_register_fold() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);

        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);

        // xor v0, v1, v1 -> mov v0, 0
        block.push_op(make_op(
            0,
            OpKind::Xor {
                dst: v0,
                src1: v1,
                src2: SrcOperand::Reg(v1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));

        block.set_terminator(Terminator::Return { values: vec![v0] });

        let folded = constant_folding(&mut block);

        assert_eq!(folded, 1);

        if let OpKind::Mov { src, .. } = &block.ops[0].kind {
            assert!(matches!(src, SrcOperand::Imm(0)));
        } else {
            panic!("Expected Mov operation");
        }
    }
    #[test]
    fn optimize_function_preserves_faulting_load_after_mul_zero_fold() {
        use crate::smir::ir::FunctionBuilder;

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let load_tmp = builder.alloc_vreg();
        let dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));

        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: load_tmp,
                addr: Address::Absolute(0x2000),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1003,
            OpKind::MulS {
                dst_lo: dst,
                dst_hi: None,
                src1: load_tmp,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![dst] });

        let mut func = builder.finish();
        optimize_function(&mut func, OptLevel::O2);
        let block = &func.blocks[0];

        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load { dst, .. } if dst == load_tmp
        )));
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Mov {
                dst: mov_dst,
                src: SrcOperand::Imm(0),
                ..
            } if mov_dst == dst
        )));
        assert!(
            !block
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::MulS { .. }))
        );
    }
    #[test]
    fn test_copy_propagation() {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));

        // mov v0, rbx     (W64 copy)
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Reg(rbx),
                width: OpWidth::W64,
            },
        ));
        // add v1, rax, v0  -> v0 rewritten to rbx
        block.push_op(make_op(
            1,
            OpKind::Add {
                dst: v1,
                src1: rax,
                src2: SrcOperand::Reg(v0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![v1] });

        let n = copy_propagation(&mut block);
        assert_eq!(n, 1);
        if let OpKind::Add { src2, .. } = &block.ops[1].kind {
            assert!(matches!(src2, SrcOperand::Reg(r) if *r == rbx));
        } else {
            panic!("expected Add");
        }
    }
    #[test]
    fn test_copy_propagation_w32_not_recorded() {
        // A 32-bit copy must NOT be propagated into a 64-bit-equality use.
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        let v0 = VReg::virt(0);
        let v1 = VReg::virt(1);
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
        block.push_op(make_op(
            0,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::Reg(rbx),
                width: OpWidth::W32, // zero-extends; v0 != rbx in 64 bits
            },
        ));
        block.push_op(make_op(
            1,
            OpKind::Add {
                dst: v1,
                src1: rax,
                src2: SrcOperand::Reg(v0),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![v1] });
        let n = copy_propagation(&mut block);
        assert_eq!(n, 0); // not propagated
    }
    #[test]
    fn test_branch_folding_same_target_and_unreachable() {
        use crate::smir::ir::types::FunctionId;
        let b0 = BlockId(0);
        let b1 = BlockId(1);
        let b2 = BlockId(2);
        let mut func = SmirFunction::new(FunctionId(0), b0, 0x1000);

        // b0: cond-branch to b1 either way (same target) -> folds to Branch b1.
        let mut blk0 = SmirBlock::new(b0, 0x1000);
        blk0.set_terminator(Terminator::CondBranch {
            cond: VReg::virt(0),
            true_target: b1,
            false_target: b1,
        });
        func.add_block(blk0);

        // b1: reachable, returns.
        let mut blk1 = SmirBlock::new(b1, 0x1010);
        blk1.set_terminator(Terminator::Return { values: vec![] });
        func.add_block(blk1);

        // b2: unreachable -> removed.
        let mut blk2 = SmirBlock::new(b2, 0x1020);
        blk2.set_terminator(Terminator::Return { values: vec![] });
        func.add_block(blk2);

        let n = branch_folding(&mut func);
        assert!(n >= 2); // 1 fold + 1 unreachable removed
        assert!(matches!(func.blocks[0].terminator, Terminator::Branch { target } if target == b1));
        assert!(func.blocks.iter().all(|b| b.id != b2));
    }
