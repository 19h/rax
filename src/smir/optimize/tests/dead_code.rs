//! tests::dead_code tests

use super::*;
use crate::smir::optimize::*;

#[test]
fn x86_count_metadata_tracks_results_sources_and_dead_flags() {
    let dst = VReg::virt(0);
    let src = VReg::virt(1);
    let popcnt = OpKind::X86Count {
        dst,
        src,
        width: OpWidth::W32,
        kind: X86CountKind::Popcnt,
        flags: FlagUpdate::All,
    };
    assert_eq!(popcnt.dests(), vec![dst]);
    assert_eq!(popcnt.source_vregs(), vec![src]);
    assert_eq!(popcnt.flags_written(), FlagSet::ALL_X86);
    assert_eq!(popcnt.flags_must_write(), FlagSet::ALL_X86);
    assert!(op_fully_defines(&popcnt));

    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(0, popcnt));
    block.set_terminator(Terminator::Return { values: vec![] });
    assert_eq!(dead_flag_elimination(&mut block), 1);
    assert!(matches!(
        block.ops[0].kind,
        OpKind::X86Count {
            flags: FlagUpdate::None,
            ..
        }
    ));

    let defined = FlagSet::CF.union(FlagSet::ZF);
    let arch_dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let arch_src = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let lzcnt = OpKind::X86Count {
        dst: arch_dst,
        src: arch_src,
        width: OpWidth::W16,
        kind: X86CountKind::Lzcnt,
        flags: FlagUpdate::Specific(defined),
    };
    assert_eq!(lzcnt.flags_written(), defined);
    assert_eq!(lzcnt.flags_must_write(), defined);
    assert!(!op_fully_defines(&lzcnt));
}
#[test]
fn x86_adx_metadata_tracks_sources_carry_chain_and_dead_output() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let src1 = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let src2 = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    for (kind, output) in [
        (X86AdxKind::Adcx, FlagSet::CF),
        (X86AdxKind::Adox, FlagSet::OF),
    ] {
        let adx = OpKind::X86Adx {
            dst,
            src1,
            src2,
            width: OpWidth::W64,
            kind,
            flags: FlagUpdate::Specific(output),
        };
        assert_eq!(adx.dests(), vec![dst]);
        assert_eq!(adx.source_vregs(), vec![src1, src2]);
        assert_eq!(adx.flags_read(), output);
        assert_eq!(adx.flags_written(), output);
        assert_eq!(adx.flags_must_write(), output);
        assert_eq!(op_out_width(&adx), Some(OpWidth::W64));

        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(0, adx));
        block.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(dead_flag_elimination(&mut block), 1);
        assert!(matches!(
            block.ops[0].kind,
            OpKind::X86Adx {
                flags: FlagUpdate::None,
                ..
            }
        ));
        assert_eq!(block.ops[0].kind.flags_read(), output);
    }
}
#[test]
fn dead_flag_elimination_removes_pure_fixed_flag_definitions() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let pure = [
        OpKind::Cmp {
            src1: rax,
            src2: SrcOperand::Reg(rcx),
            width: OpWidth::W64,
        },
        OpKind::Test {
            src1: rax,
            src2: SrcOperand::Reg(rcx),
            width: OpWidth::W64,
        },
        OpKind::Bt {
            src: rax,
            index: SrcOperand::Reg(rcx),
            width: OpWidth::W64,
        },
        OpKind::SetCF { value: true },
        OpKind::CmcCF,
    ];

    for kind in pure {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(0, kind));
        assert_eq!(dead_flag_elimination_with(&mut block, FlagSet::EMPTY), 1);
        assert!(matches!(block.ops[0].kind, OpKind::Nop));
    }

    let mut live_bt = SmirBlock::new(BlockId(0), 0x1000);
    live_bt.push_op(make_op(
        0,
        OpKind::Bt {
            src: rax,
            index: SrcOperand::Imm(7),
            width: OpWidth::W64,
        },
    ));
    assert_eq!(dead_flag_elimination_with(&mut live_bt, FlagSet::CF), 0);
    assert!(matches!(live_bt.ops[0].kind, OpKind::Bt { .. }));

    // Update forms still produce a GPR value even when CF is dead.
    let mut update = SmirBlock::new(BlockId(0), 0x1000);
    update.push_op(make_op(
        0,
        OpKind::Bts {
            dst: rax,
            src: rax,
            index: SrcOperand::Imm(7),
            width: OpWidth::W64,
        },
    ));
    assert_eq!(dead_flag_elimination_with(&mut update, FlagSet::EMPTY), 0);
    assert!(matches!(update.ops[0].kind, OpKind::Bts { .. }));
}
#[test]
fn dead_code_elimination_preserves_volatile_x86_timestamp_read() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::X86ReadTsc {
            dst_lo: VReg::virt(0),
            dst_hi: VReg::virt(1),
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![] });

    assert_eq!(dead_code_elimination(&mut block), 0);
    assert!(matches!(block.ops[0].kind, OpKind::X86ReadTsc { .. }));
}
#[test]
fn test_dead_flag_elimination() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);

    let v0 = VReg::virt(0);
    let v1 = VReg::virt(1);
    let v2 = VReg::virt(2);

    // Add with flags that are never used
    block.push_op(make_op(
        0,
        OpKind::Add {
            dst: v0,
            src1: v1,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    ));

    // Another add with flags
    block.push_op(make_op(
        1,
        OpKind::Add {
            dst: v2,
            src1: v0,
            src2: SrcOperand::Imm(2),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    ));

    block.set_terminator(Terminator::Return { values: vec![v2] });

    let eliminated = dead_flag_elimination(&mut block);

    // Both flag updates should be eliminated since no flags are read
    assert_eq!(eliminated, 2);

    // Check flags are now None
    for op in &block.ops {
        if let OpKind::Add { flags, .. } = &op.kind {
            assert_eq!(*flags, FlagUpdate::None);
        }
    }
}
// Regression for issue #108: OpKind::SatN ORs the Hexagon USR:OVF sticky bit
// as a side effect, but that write is invisible to dests(). DCE must therefore
// keep a SatN that can set OVF (set_ovf == true) even when its data result is
// dead — yet may still drop one that cannot (set_ovf == false). The SatN
// writes a virtual temp that is never read (so its data result is dead and not
// kept alive by the frontier), isolating the decision to the side effect.
#[test]
fn issue_108_dce_keeps_satn_with_ovf_side_effect() {
    use crate::smir::ir::FunctionBuilder;

    fn satn_count_after_opt(set_ovf: bool) -> usize {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        let tmp = builder.alloc_vreg();
        let dead = builder.alloc_vreg();
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: tmp,
                src: SrcOperand::Imm(0x8000),
                width: OpWidth::W64,
            },
        );
        builder.push_op(
            0x1004,
            OpKind::SatN {
                dst: dead,
                src: SrcOperand::Reg(tmp),
                sat_bits: 16,
                signed: true,
                set_ovf,
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        });
        let mut func = builder.finish();
        optimize_function(&mut func, OptLevel::O2);
        func.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::SatN { .. }))
            .count()
    }

    assert_eq!(
        satn_count_after_opt(true),
        1,
        "a SatN that can set USR:OVF must survive DCE even with a dead data result",
    );
    assert_eq!(
        satn_count_after_opt(false),
        0,
        "a SatN with set_ovf=false and a dead data result has no side effect and is removable",
    );
}
// Regression for issue #112: PredStore writes memory (writes_memory() == true),
// so redundant-load elimination must invalidate its cached loads across one.
// A `Load X; PredStore X; Load X` sequence must keep BOTH loads — forwarding the
// second from the first would read stale memory if the PredStore committed.
#[test]
fn issue_112_redundant_load_elim_invalidates_on_pred_store() {
    use crate::smir::ir::FunctionBuilder;
    use crate::smir::ir::types::SignExtend;

    let r0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(0)));
    let r1 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(1)));
    let r2 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(2)));
    let r3 = VReg::Arch(ArchReg::Hexagon(HexagonReg::R(3)));
    let p0 = VReg::Arch(ArchReg::Hexagon(HexagonReg::P(0)));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Load {
            dst: r1,
            addr: Address::Direct(r0),
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x1004,
        OpKind::PredStore {
            src: SrcOperand::Reg(r2),
            cond: p0,
            addr: Address::Direct(r0),
            width: MemWidth::B4,
        },
    );
    builder.push_op(
        0x1008,
        OpKind::Load {
            dst: r3,
            addr: Address::Direct(r0),
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        },
    );
    builder.set_terminator(Terminator::Trap {
        kind: crate::smir::ir::TrapKind::Halt,
    });
    let mut func = builder.finish();
    func.attrs.allow_redundant_load_elimination = true;

    let eliminated = redundant_load_elimination(&mut func);
    assert_eq!(
        eliminated, 0,
        "a PredStore must prevent the following load from being forwarded",
    );
    let load_count = func.blocks[0]
        .ops
        .iter()
        .filter(|op| matches!(op.kind, OpKind::Load { .. }))
        .count();
    assert_eq!(
        load_count, 2,
        "both loads must survive across a PredStore (none rewritten to a Mov)",
    );
}
#[test]
fn a32_data_processing_register_shift_metadata_and_dead_flags_are_exact() {
    let dst = VReg::virt(0);
    let rn = VReg::virt(1);
    let rm = VReg::virt(2);
    let rs = VReg::virt(3);
    let nzc = FlagSet::SF.union(FlagSet::ZF).union(FlagSet::CF);

    for opcode in 0_u8..16 {
        let kind = ArmDpRegShiftKind::from_opcode(opcode).unwrap();
        let flags = FlagUpdate::Specific(if kind.is_logical() {
            nzc
        } else {
            FlagSet::NZCV
        });
        let op = OpKind::ArmDpRegShift {
            kind,
            dst: kind.writes_result().then_some(dst),
            rn: kind.uses_rn().then_some(rn),
            rm,
            rs,
            shift: ShiftOp::Ror,
            flags,
        };
        assert_eq!(
            op.dests(),
            if kind.writes_result() {
                vec![dst]
            } else {
                vec![]
            }
        );
        let mut expected_sources = Vec::new();
        if kind.uses_rn() {
            expected_sources.push(rn);
        }
        expected_sources.extend([rm, rs]);
        assert_eq!(op.source_vregs(), expected_sources);
        assert_eq!(op.flags_written(), flags.as_set());
        assert_eq!(op.flags_must_write(), flags.as_set());
        assert_eq!(
            op.flags_read(),
            if kind.reads_carry() || kind.is_logical() {
                FlagSet::CF
            } else {
                FlagSet::EMPTY
            }
        );
    }

    for (kind, still_reads_carry) in [
        (ArmDpRegShiftKind::And, false),
        (ArmDpRegShiftKind::Adc, true),
    ] {
        let flags = FlagUpdate::Specific(if kind.is_logical() {
            nzc
        } else {
            FlagSet::NZCV
        });
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(
            0,
            OpKind::ArmDpRegShift {
                kind,
                dst: Some(dst),
                rn: Some(rn),
                rm,
                rs,
                shift: ShiftOp::Lsl,
                flags,
            },
        ));
        block.set_terminator(Terminator::Return { values: vec![dst] });
        assert_eq!(dead_flag_elimination(&mut block), 1);
        assert!(matches!(
            block.ops[0].kind,
            OpKind::ArmDpRegShift {
                flags: FlagUpdate::None,
                ..
            }
        ));
        assert_eq!(
            block.ops[0].kind.flags_read(),
            if still_reads_carry {
                FlagSet::CF
            } else {
                FlagSet::EMPTY
            }
        );
    }
}
#[test]
fn test_dead_code_elimination() {
    let mut block = SmirBlock::new(BlockId(0), 0x1000);

    let v0 = VReg::virt(0);
    let v1 = VReg::virt(1);
    let v2 = VReg::virt(2);

    // mov v0, 10 (unused)
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: v0,
            src: SrcOperand::Imm(10),
            width: OpWidth::W64,
        },
    ));

    // mov v1, 20 (used)
    block.push_op(make_op(
        1,
        OpKind::Mov {
            dst: v1,
            src: SrcOperand::Imm(20),
            width: OpWidth::W64,
        },
    ));

    // mov v2, 30 (unused)
    block.push_op(make_op(
        2,
        OpKind::Mov {
            dst: v2,
            src: SrcOperand::Imm(30),
            width: OpWidth::W64,
        },
    ));

    block.set_terminator(Terminator::Return { values: vec![v1] });

    let eliminated = dead_code_elimination(&mut block);

    assert_eq!(eliminated, 2);
    assert_eq!(block.ops.len(), 1);

    // Only v1 should remain
    if let OpKind::Mov { dst, .. } = &block.ops[0].kind {
        assert_eq!(*dst, v1);
    }
}
#[test]
fn vfma_accumulator_definition_survives_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let accumulator = VReg::virt(1);
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(i64::from(2.0f32.to_bits())),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: accumulator,
            scalar,
            elem: VecElementType::F32,
            lanes: 8,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::VFma {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            acc: accumulator,
            elem: VecElementType::F32,
            lanes: 8,
            negate_product: false,
            negate_acc: false,
        },
    ));
    block.set_terminator(Terminator::Return {
        values: vec![VReg::Arch(ArchReg::X86(X86Reg::Ymm(2)))],
    });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 3, "VFma accumulator producer was removed");
    assert!(matches!(
        block.ops[1].kind,
        OpKind::VBroadcast { dst, .. } if dst == accumulator
    ));
}
#[test]
fn vpermute_table_and_index_definitions_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let table1 = VReg::virt(1);
    let table2 = VReg::virt(2);
    let indices = VReg::virt(3);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, table1), (2, table2), (3, indices)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I8,
                lanes: 16,
            },
        ));
    }
    block.push_op(make_op(
        4,
        OpKind::VPermute {
            dst,
            src1: table1,
            src2: Some(table2),
            indices,
            elem: VecElementType::I8,
            width: VecWidth::V128,
            overwrite_table: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 5, "VPermute source producer was removed");
    for source in [table1, table2, indices] {
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast { dst, .. } if dst == source
        )));
    }
}
#[test]
fn x86_permute_bytes_words_inputs_and_merge_destination_survive_dce() {
    let scalar = VReg::virt(0);
    let dst = VReg::virt(1);
    let table1 = VReg::virt(2);
    let table2 = VReg::virt(3);
    let mask = VReg::virt(4);
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, dst), (2, table1), (3, table2)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I8,
                lanes: 16,
            },
        ));
    }
    block.push_op(make_op(
        4,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(0x55),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        5,
        OpKind::X86PermuteBytesWords {
            dst,
            table1,
            table2: Some(table2),
            indices: dst,
            mask: Some(mask),
            elem: VecElementType::I8,
            width: VecWidth::V128,
            overwrite_table: false,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 6);
    for source in [dst, table1, table2] {
        assert!(block.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VBroadcast { dst, .. } if dst == source
        )));
    }
    assert!(block.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Mov { dst, .. } if dst == mask
    )));
}
#[test]
fn vpopcnt_source_definition_survives_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let source = VReg::virt(1);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(0x55),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: source,
            scalar,
            elem: VecElementType::I8,
            lanes: 16,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::VPopcnt {
            dst,
            src: source,
            mask: None,
            elem: VecElementType::I8,
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 3, "VPopcnt source producer was removed");
    assert!(block.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast { dst, .. } if dst == source
    )));
}
#[test]
fn x86_mov_mask_source_definition_survives_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let source = VReg::virt(1);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(0x80),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: source,
            scalar,
            elem: VecElementType::I8,
            lanes: 16,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::X86MovMask {
            dst,
            src: source,
            elem: VecElementType::I8,
            lanes: 16,
            dst_width: OpWidth::W32,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 3, "MOVMSK source producer was removed");
    assert!(block.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast { dst, .. } if dst == source
    )));
}
#[test]
fn x86_movd_q_source_definition_survives_dead_code_elimination() {
    let source = VReg::virt(0);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: source,
            src: SrcOperand::Imm(0x1234_5678),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::X86MovdQ {
            dst,
            src: source,
            width: OpWidth::W32,
            zero_upper: true,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 2, "MOVD source producer was removed");
    assert!(matches!(block.ops[0].kind, OpKind::Mov { dst: actual, .. } if actual == source));
}
#[test]
fn vconflict_source_definition_survives_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let source = VReg::virt(1);
    let mask = VReg::virt(2);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: source,
            scalar,
            elem: VecElementType::I32,
            lanes: 4,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        3,
        OpKind::VConflict {
            dst,
            src: source,
            mask: Some(mask),
            elem: VecElementType::I32,
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 4, "VConflict input producer was removed");
}
#[test]
fn vleadingzeros_source_definition_survives_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let source = VReg::virt(1);
    let mask = VReg::virt(2);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: source,
            scalar,
            elem: VecElementType::I32,
            lanes: 4,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        3,
        OpKind::VLeadingZeros {
            dst,
            src: source,
            mask: Some(mask),
            elem: VecElementType::I32,
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(
        block.ops.len(),
        4,
        "VLeadingZeros input producer was removed"
    );
}
#[test]
fn vmultiplyadd52_input_definitions_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let acc = VReg::virt(1);
    let src1 = VReg::virt(2);
    let src2 = VReg::virt(3);
    let mask = VReg::virt(4);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, acc), (2, src1), (3, src2)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I64,
                lanes: 2,
            },
        ));
    }
    block.push_op(make_op(
        4,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        5,
        OpKind::VMultiplyAdd52 {
            dst,
            acc,
            src1,
            src2,
            mask: Some(mask),
            width: VecWidth::V128,
            high: false,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(
        block.ops.len(),
        6,
        "VMultiplyAdd52 input producer was removed"
    );
}
#[test]
fn vdotproductext_input_definitions_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let acc = VReg::virt(1);
    let src1 = VReg::virt(2);
    let src2 = VReg::virt(3);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, acc), (2, src1), (3, src2)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
    }
    block.push_op(make_op(
        4,
        OpKind::VDotProductExt {
            dst,
            acc,
            src1,
            src2,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I32,
            width: VecWidth::V128,
            src1_signed: true,
            src2_signed: false,
            saturate: true,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(
        block.ops.len(),
        5,
        "VDotProductExt input producer was removed"
    );
}
#[test]
fn bf16_input_definitions_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let acc = VReg::virt(1);
    let src1 = VReg::virt(2);
    let src2 = VReg::virt(3);
    let dot = VReg::virt(4);
    let mask = VReg::virt(5);
    let convert_mask = VReg::virt(6);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, acc), (2, src1), (3, src2)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I32,
                lanes: 4,
            },
        ));
    }
    block.push_op(make_op(
        4,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        5,
        OpKind::VDotProductBF16 {
            dst: dot,
            acc,
            src1,
            src2,
            mask: Some(mask),
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.push_op(make_op(
        6,
        OpKind::Mov {
            dst: convert_mask,
            src: SrcOperand::Imm(0x55),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        7,
        OpKind::VCvtFP32ToBF16 {
            dst,
            src1: dot,
            src2: Some(src2),
            mask: Some(convert_mask),
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 8, "BF16 input producer was removed");
}
#[test]
fn fp16_mask_and_input_definitions_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let src1 = VReg::virt(1);
    let src2 = VReg::virt(2);
    let mask = VReg::virt(3);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(0x3c00),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, src1), (2, src2)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I16,
                lanes: 8,
            },
        ));
    }
    block.push_op(make_op(
        3,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(0x55),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        4,
        OpKind::VFP16Arith {
            dst,
            src1,
            src2,
            mask: Some(mask),
            op: Avx10FP16Op::Add,
            round: FpRoundMode::Dynamic,
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 5, "FP16 input producer was removed");
}
#[test]
fn vmpsadbw_mask_and_merge_destination_definitions_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let src1 = VReg::virt(1);
    let src2 = VReg::virt(2);
    let mask = VReg::virt(3);
    let dst = VReg::virt(4);
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(0x55),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, src1), (2, src2), (3, dst)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I8,
                lanes: 16,
            },
        ));
    }
    block.push_op(make_op(
        4,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(0x55),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        5,
        OpKind::VMpsadbw {
            dst,
            src1,
            src2,
            mask: Some(mask),
            width: VecWidth::V128,
            imm: 0x07,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(
        block.ops.len(),
        6,
        "VMPSADBW input, mask, or merge-destination producer was removed"
    );
}
#[test]
fn vshufflebitqm_input_definitions_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let src = VReg::virt(1);
    let indices = VReg::virt(2);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::K(1)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    for (id, vector) in [(1, src), (2, indices)] {
        block.push_op(make_op(
            id,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem: VecElementType::I64,
                lanes: 2,
            },
        ));
    }
    block.push_op(make_op(
        3,
        OpKind::VShuffleBitQM {
            dst,
            src,
            indices,
            mask: None,
            width: VecWidth::V128,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(
        block.ops.len(),
        4,
        "VShuffleBitQM input producer was removed"
    );
}
#[test]
fn vcompress_vexpand_inputs_and_merge_destinations_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let src = VReg::virt(1);
    let packed = VReg::virt(2);
    let mask = VReg::virt(3);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: src,
            scalar,
            elem: VecElementType::I32,
            lanes: 4,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::VBroadcast {
            dst: packed,
            scalar,
            elem: VecElementType::I32,
            lanes: 4,
        },
    ));
    block.push_op(make_op(
        3,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(5),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        4,
        OpKind::VCompress {
            dst: packed,
            src,
            mask: Some(mask),
            elem: VecElementType::I32,
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.push_op(make_op(
        5,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I32,
            lanes: 4,
        },
    ));
    block.push_op(make_op(
        6,
        OpKind::VExpand {
            dst,
            src: packed,
            mask: Some(mask),
            elem: VecElementType::I32,
            width: VecWidth::V128,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(
        block.ops.len(),
        7,
        "compress/expand input producer was removed"
    );
}
#[test]
fn x86_narrow_inputs_and_merge_destination_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let src = VReg::virt(1);
    let mask = VReg::virt(2);
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(3),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: src,
            scalar,
            elem: VecElementType::I32,
            lanes: 4,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(5),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        3,
        OpKind::VBroadcast {
            dst,
            scalar,
            elem: VecElementType::I8,
            lanes: 4,
        },
    ));
    block.push_op(make_op(
        4,
        OpKind::X86NarrowInt {
            dst,
            src,
            mask: Some(mask),
            src_elem: VecElementType::I32,
            dst_elem: VecElementType::I8,
            width: VecWidth::V128,
            mode: X86NarrowMode::SignedSaturate,
            zeroing: false,
        },
    ));
    block.set_terminator(Terminator::Return { values: vec![dst] });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 5, "narrowing input producer was removed");
}
#[test]
fn x86_aes_sources_survive_dead_code_elimination() {
    let scalar = VReg::virt(0);
    let state = VReg::virt(1);
    let key = VReg::virt(2);
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.push_op(make_op(
        0,
        OpKind::Mov {
            dst: scalar,
            src: SrcOperand::Imm(0x5A),
            width: OpWidth::W64,
        },
    ));
    block.push_op(make_op(
        1,
        OpKind::VBroadcast {
            dst: state,
            scalar,
            elem: VecElementType::I64,
            lanes: 2,
        },
    ));
    block.push_op(make_op(
        2,
        OpKind::VBroadcast {
            dst: key,
            scalar,
            elem: VecElementType::I64,
            lanes: 2,
        },
    ));
    block.push_op(make_op(
        3,
        OpKind::X86Aes {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            src1: state,
            src2: Some(key),
            width: VecWidth::V128,
            op: X86AesOp::Enc,
            imm: 0,
        },
    ));
    block.set_terminator(Terminator::Return {
        values: vec![VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))],
    });

    dead_code_elimination(&mut block);
    assert_eq!(block.ops.len(), 4, "AES source producer was removed");
    assert!(block.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast { dst, .. } if dst == state
    )));
    assert!(block.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast { dst, .. } if dst == key
    )));
}
