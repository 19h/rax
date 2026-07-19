//! gate::memory tests

use super::*;
use crate::smir::lower::runtime::jit_gate_tests::*;
use crate::smir::lower::runtime::*;

#[test]
fn x86_immediate_memory_bit_update_gate_accepts_exact_rmw_and_fails_closed() {
    let old = VReg::Virtual(VirtualId(20));
    let mask = VReg::Virtual(VirtualId(21));
    let result = VReg::Virtual(VirtualId(22));
    let address = || Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R16),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let build = |action: u8, mem_width: MemWidth, bit: i64| {
        let width = mem_width.to_op_width().unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address(),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: mask,
                src: SrcOperand::Imm(1),
                width,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Shl {
                dst: mask,
                src: mask,
                amount: SrcOperand::Imm(bit),
                width,
                flags: FlagUpdate::None,
            },
        );
        if action == 1 {
            builder.push_op(
                0x1000,
                OpKind::Not {
                    dst: mask,
                    src: mask,
                    width,
                },
            );
        }
        let compute = match action {
            0 => OpKind::Or {
                dst: result,
                src1: old,
                src2: SrcOperand::Reg(mask),
                width,
                flags: FlagUpdate::None,
            },
            1 => OpKind::And {
                dst: result,
                src1: old,
                src2: SrcOperand::Reg(mask),
                width,
                flags: FlagUpdate::None,
            },
            2 => OpKind::Xor {
                dst: result,
                src1: old,
                src2: SrcOperand::Reg(mask),
                width,
                flags: FlagUpdate::None,
            },
            _ => unreachable!(),
        };
        builder.push_op(0x1000, compute);
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: result,
                addr: address(),
                width: mem_width,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Bt {
                src: old,
                index: SrcOperand::Imm(bit),
                width,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };
    let build_folded = |action: u8, bit: i64, mask: i64, flags: FlagUpdate| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address(),
                width: MemWidth::B8,
                sign: SignExtend::Zero,
            },
        );
        let compute = match action {
            0 => OpKind::Or {
                dst: result,
                src1: old,
                src2: SrcOperand::Imm(mask),
                width: OpWidth::W64,
                flags,
            },
            1 => OpKind::And {
                dst: result,
                src1: old,
                src2: SrcOperand::Imm(mask),
                width: OpWidth::W64,
                flags,
            },
            2 => OpKind::Xor {
                dst: result,
                src1: old,
                src2: SrcOperand::Imm(mask),
                width: OpWidth::W64,
                flags,
            },
            _ => unreachable!(),
        };
        builder.push_op(0x1000, compute);
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: result,
                addr: address(),
                width: MemWidth::B8,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Bt {
                src: old,
                index: SrcOperand::Imm(bit),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };

    for (action, mem_width, bit) in [
        (0, MemWidth::B2, 15),
        (1, MemWidth::B4, 31),
        (2, MemWidth::B8, 63),
    ] {
        let function = build(action, mem_width, bit);
        assert!(is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            false,
        ));
    }

    for function in [
        build_folded(0, 5, 1 << 5, FlagUpdate::None),
        build_folded(1, 5, !(1 << 5), FlagUpdate::None),
        build_folded(2, 63, i64::MIN, FlagUpdate::None),
    ] {
        assert!(is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            false,
        ));
    }

    let folded_wrong_mask = build_folded(2, 63, 1, FlagUpdate::None);
    let mut folded_wrong_replay = build_folded(0, 5, 1 << 5, FlagUpdate::None);
    let OpKind::Bt { index, .. } = &mut folded_wrong_replay.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *index = SrcOperand::Imm(6);
    let folded_flagged = build_folded(1, 5, !(1 << 5), FlagUpdate::All);
    for function in [folded_wrong_mask, folded_wrong_replay, folded_flagged] {
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    let mut signed = build(0, MemWidth::B8, 5);
    let OpKind::Load { sign, .. } = &mut signed.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *sign = SignExtend::Sign;

    let mut register_index = build(0, MemWidth::B8, 5);
    let OpKind::Shl { amount, .. } = &mut register_index.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *amount = SrcOperand::Reg(x86(X86Reg::Rcx));

    let mut wrong_store = build(0, MemWidth::B8, 5);
    let OpKind::Store { addr, .. } = &mut wrong_store.blocks[0].ops[4].kind else {
        unreachable!()
    };
    *addr = Address::Absolute(0x2000);

    let mut wrong_replay = build(2, MemWidth::B8, 5);
    let OpKind::Bt { index, .. } = &mut wrong_replay.blocks[0].ops[5].kind else {
        unreachable!()
    };
    *index = SrcOperand::Imm(6);

    let mut wrong_reset = build(1, MemWidth::B8, 5);
    let OpKind::Not { width, .. } = &mut wrong_reset.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *width = OpWidth::W32;

    let mut flagged_compute = build(0, MemWidth::B8, 5);
    let OpKind::Or { flags, .. } = &mut flagged_compute.blocks[0].ops[3].kind else {
        unreachable!()
    };
    *flags = FlagUpdate::All;

    let mut wrong_pc = build(0, MemWidth::B8, 5);
    wrong_pc.blocks[0].ops[5].guest_pc = 0x1001;

    for function in [
        signed,
        register_index,
        wrong_store,
        wrong_replay,
        wrong_reset,
        flagged_compute,
        wrong_pc,
    ] {
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
    }
}
#[test]
fn x86_scalar_memory_destination_alu_gate_accepts_exact_replay_and_fails_closed() {
    let old = VReg::Virtual(VirtualId(30));
    let result = VReg::Virtual(VirtualId(31));
    let flags_result = VReg::Virtual(VirtualId(32));
    let address = || Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R16),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };
    let alu =
        |tag: u8, dst: VReg, src1: VReg, src2: SrcOperand, width: OpWidth, flags: FlagUpdate| {
            match tag {
                0 => OpKind::Add {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                1 => OpKind::Or {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                2 => OpKind::Adc {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                3 => OpKind::Sbb {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                4 => OpKind::And {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                5 => OpKind::Sub {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                6 => OpKind::Xor {
                    dst,
                    src1,
                    src2,
                    width,
                    flags,
                },
                _ => unreachable!(),
            }
        };
    let build = |tag: u8,
                 source: SrcOperand,
                 mem_width: MemWidth,
                 compute_flags: FlagUpdate,
                 replay_flags: FlagUpdate,
                 replay_tag: u8,
                 store_addr: Address,
                 replay_pc: u64,
                 extra_old_use: bool| {
        let width = mem_width.to_op_width().unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address(),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            alu(tag, result, old, source.clone(), width, compute_flags),
        );
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: result,
                addr: store_addr,
                width: mem_width,
            },
        );
        builder.push_op(
            replay_pc,
            alu(replay_tag, flags_result, old, source, width, replay_flags),
        );
        if extra_old_use {
            builder.push_op(
                0x1001,
                OpKind::Mov {
                    dst: x86(X86Reg::R11),
                    src: SrcOperand::Reg(old),
                    width,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };

    for (tag, source, mem_width) in [
        (0, SrcOperand::Reg(x86(X86Reg::Rax)), MemWidth::B1),
        (1, SrcOperand::Reg(x86(X86Reg::Rsp)), MemWidth::B2),
        (2, SrcOperand::Reg(x86(X86Reg::Rbp)), MemWidth::B4),
        (3, SrcOperand::Reg(x86(X86Reg::R16)), MemWidth::B8),
        (4, SrcOperand::Imm(0x7F), MemWidth::B1),
        (5, SrcOperand::Imm(-0x1234), MemWidth::B8),
        (6, SrcOperand::Reg(x86(X86Reg::R15)), MemWidth::B4),
    ] {
        let function = build(
            tag,
            source.clone(),
            mem_width,
            FlagUpdate::None,
            FlagUpdate::All,
            tag,
            address(),
            0x1000,
            false,
        );
        assert!(
            is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "tag={tag} source={source:?} width={mem_width:?}: {function:?}"
        );
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            false,
        ));
    }

    let invalid = [
        build(
            0,
            SrcOperand::Reg(x86(X86Reg::Rax)),
            MemWidth::B8,
            FlagUpdate::All,
            FlagUpdate::All,
            0,
            address(),
            0x1000,
            false,
        ),
        build(
            0,
            SrcOperand::Reg(x86(X86Reg::Rax)),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::None,
            0,
            address(),
            0x1000,
            false,
        ),
        build(
            0,
            SrcOperand::Reg(x86(X86Reg::Rax)),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            1,
            address(),
            0x1000,
            false,
        ),
        build(
            0,
            SrcOperand::Reg(x86(X86Reg::Rax)),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            0,
            Address::Absolute(0x2000),
            0x1000,
            false,
        ),
        build(
            0,
            SrcOperand::Reg(x86(X86Reg::Rax)),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            0,
            address(),
            0x1001,
            false,
        ),
        build(
            0,
            SrcOperand::Reg(x86(X86Reg::Rax)),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            0,
            address(),
            0x1000,
            true,
        ),
    ];
    for function in invalid {
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    let mut signed = build(
        0,
        SrcOperand::Imm(1),
        MemWidth::B8,
        FlagUpdate::None,
        FlagUpdate::All,
        0,
        address(),
        0x1000,
        false,
    );
    let OpKind::Load { sign, .. } = &mut signed.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *sign = SignExtend::Sign;
    assert!(!is_native_clobber_safe_excluding(
        &signed,
        &std::collections::HashMap::new(),
        true,
    ));
}
#[test]
fn x86_scalar_memory_destination_unary_gate_accepts_exact_replay_and_fails_closed() {
    let old = VReg::Virtual(VirtualId(40));
    let result = VReg::Virtual(VirtualId(41));
    let flags_result = VReg::Virtual(VirtualId(42));
    let address = || Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rbp)),
        index: x86(X86Reg::R17),
        scale: 4,
        disp: 16,
        disp_size: DispSize::Disp8,
    };
    let unary = |tag: u8, dst: VReg, src: VReg, width: OpWidth, flags: FlagUpdate| match tag {
        0 => OpKind::Neg {
            dst,
            src,
            width,
            flags,
        },
        1 => OpKind::Inc {
            dst,
            src,
            width,
            flags,
        },
        2 => OpKind::Dec {
            dst,
            src,
            width,
            flags,
        },
        _ => unreachable!(),
    };
    let build_flagged = |tag: u8,
                         replay_tag: u8,
                         mem_width: MemWidth,
                         compute_flags: FlagUpdate,
                         replay_flags: FlagUpdate,
                         store_addr: Address,
                         replay_pc: u64,
                         extra_old_use: bool| {
        let width = mem_width.to_op_width().unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address(),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(0x1000, unary(tag, result, old, width, compute_flags));
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: result,
                addr: store_addr,
                width: mem_width,
            },
        );
        builder.push_op(
            replay_pc,
            unary(replay_tag, flags_result, old, width, replay_flags),
        );
        if extra_old_use {
            builder.push_op(
                0x1001,
                OpKind::Mov {
                    dst: x86(X86Reg::R11),
                    src: SrcOperand::Reg(old),
                    width,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };

    for (tag, mem_width) in [(0, MemWidth::B1), (1, MemWidth::B4), (2, MemWidth::B8)] {
        let function = build_flagged(
            tag,
            tag,
            mem_width,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        );
        assert!(is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            false,
        ));
    }

    let mut not_builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    not_builder.push_op(
        0x1000,
        OpKind::Load {
            dst: old,
            addr: address(),
            width: MemWidth::B2,
            sign: SignExtend::Zero,
        },
    );
    not_builder.push_op(
        0x1000,
        OpKind::Not {
            dst: old,
            src: old,
            width: OpWidth::W16,
        },
    );
    not_builder.push_op(
        0x1000,
        OpKind::Store {
            src: old,
            addr: address(),
            width: MemWidth::B2,
        },
    );
    not_builder.set_terminator(Terminator::Return { values: vec![] });
    let not_function = not_builder.finish();
    assert!(is_native_clobber_safe_excluding(
        &not_function,
        &std::collections::HashMap::new(),
        true,
    ));

    for function in [
        build_flagged(
            0,
            0,
            MemWidth::B8,
            FlagUpdate::All,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build_flagged(
            1,
            1,
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::None,
            address(),
            0x1000,
            false,
        ),
        build_flagged(
            2,
            0,
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build_flagged(
            0,
            0,
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            Address::Absolute(0x2000),
            0x1000,
            false,
        ),
        build_flagged(
            0,
            0,
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1001,
            false,
        ),
        build_flagged(
            0,
            0,
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            true,
        ),
    ] {
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    let mut malformed_not = not_function;
    let OpKind::Store { addr, .. } = &mut malformed_not.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *addr = Address::Absolute(0x2000);
    assert!(!is_native_clobber_safe_excluding(
        &malformed_not,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut signed = build_flagged(
        0,
        0,
        MemWidth::B8,
        FlagUpdate::None,
        FlagUpdate::All,
        address(),
        0x1000,
        false,
    );
    let OpKind::Load { sign, .. } = &mut signed.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *sign = SignExtend::Sign;
    assert!(!is_native_clobber_safe_excluding(
        &signed,
        &std::collections::HashMap::new(),
        true,
    ));
}
#[test]
fn x86_scalar_memory_destination_shift_gate_accepts_exact_replay_and_fails_closed() {
    let old = VReg::Virtual(VirtualId(50));
    let result = VReg::Virtual(VirtualId(51));
    let flags_result = VReg::Virtual(VirtualId(52));
    let address = || Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rbx)),
        index: x86(X86Reg::R18),
        scale: 8,
        disp: -32,
        disp_size: DispSize::Disp8,
    };
    let shift =
        |tag: u8, dst: VReg, src: VReg, amount: SrcOperand, width: OpWidth, flags: FlagUpdate| {
            match tag {
                0 => OpKind::Rol {
                    dst,
                    src,
                    amount,
                    width,
                    flags,
                },
                1 => OpKind::Ror {
                    dst,
                    src,
                    amount,
                    width,
                    flags,
                },
                2 => OpKind::Rcl {
                    dst,
                    src,
                    amount,
                    width,
                    flags,
                },
                3 => OpKind::Rcr {
                    dst,
                    src,
                    amount,
                    width,
                    flags,
                },
                4 => OpKind::Shl {
                    dst,
                    src,
                    amount,
                    width,
                    flags,
                },
                5 => OpKind::Shr {
                    dst,
                    src,
                    amount,
                    width,
                    flags,
                },
                7 => OpKind::Sar {
                    dst,
                    src,
                    amount,
                    width,
                    flags,
                },
                _ => unreachable!(),
            }
        };
    let rotate_flags = FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF));
    let build = |tag: u8,
                 replay_tag: u8,
                 amount: SrcOperand,
                 replay_amount: SrcOperand,
                 mem_width: MemWidth,
                 compute_flags: FlagUpdate,
                 replay_flags: FlagUpdate,
                 store_addr: Address,
                 replay_pc: u64,
                 extra_old_use: bool| {
        let width = mem_width.to_op_width().unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: old,
                addr: address(),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            shift(tag, result, old, amount, width, compute_flags),
        );
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: result,
                addr: store_addr,
                width: mem_width,
            },
        );
        builder.push_op(
            replay_pc,
            shift(
                replay_tag,
                flags_result,
                old,
                replay_amount,
                width,
                replay_flags,
            ),
        );
        if extra_old_use {
            builder.push_op(
                0x1001,
                OpKind::Mov {
                    dst: x86(X86Reg::R11),
                    src: SrcOperand::Reg(old),
                    width,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };

    for (tag, amount, mem_width) in [
        (0, SrcOperand::Imm(7), MemWidth::B1),
        (1, SrcOperand::Imm(7), MemWidth::B2),
        (0, SrcOperand::Imm(31), MemWidth::B1),
        (1, SrcOperand::Imm(9), MemWidth::B1),
        (2, SrcOperand::Imm(1), MemWidth::B4),
        (3, SrcOperand::Imm(1), MemWidth::B8),
        (2, SrcOperand::Imm(32), MemWidth::B4),
        (3, SrcOperand::Imm(64), MemWidth::B8),
        (2, SrcOperand::Imm(7), MemWidth::B4),
        (3, SrcOperand::Imm(7), MemWidth::B8),
        (4, SrcOperand::Imm(0), MemWidth::B1),
        (5, SrcOperand::Imm(1), MemWidth::B4),
        (7, SrcOperand::Imm(255), MemWidth::B8),
        (0, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B1),
        (1, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B1),
        (0, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
        (1, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B8),
        (2, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
        (3, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B8),
        (4, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
        (5, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B8),
        (7, SrcOperand::Reg(x86(X86Reg::Rcx)), MemWidth::B4),
    ] {
        let replay_flags = if tag <= 3 {
            rotate_flags
        } else {
            FlagUpdate::All
        };
        let function = build(
            tag,
            tag,
            amount.clone(),
            amount,
            mem_width,
            FlagUpdate::None,
            replay_flags,
            address(),
            0x1000,
            false,
        );
        assert!(is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            false,
        ));
    }

    for tag in 0..=3 {
        for mem_width in [MemWidth::B1, MemWidth::B2] {
            for amount in [
                SrcOperand::Imm(0),
                SrcOperand::Imm(1),
                SrcOperand::Imm(9),
                SrcOperand::Imm(17),
                SrcOperand::Imm(31),
                SrcOperand::Reg(x86(X86Reg::Rcx)),
            ] {
                let function = build(
                    tag,
                    tag,
                    amount.clone(),
                    amount,
                    mem_width,
                    FlagUpdate::None,
                    rotate_flags,
                    address(),
                    0x1000,
                    false,
                );
                assert!(
                    is_native_clobber_safe_excluding(
                        &function,
                        &std::collections::HashMap::new(),
                        true,
                    ),
                    "subword rotate tag {tag} with {mem_width:?} must JIT"
                );
            }
        }
    }

    for mem_width in [MemWidth::B1, MemWidth::B2] {
        for amount in [
            SrcOperand::Imm(0),
            SrcOperand::Imm(1),
            SrcOperand::Imm(8),
            SrcOperand::Imm(16),
            SrcOperand::Imm(31),
            SrcOperand::Reg(x86(X86Reg::Rcx)),
        ] {
            let function = build(
                7,
                7,
                amount.clone(),
                amount,
                mem_width,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            );
            assert!(
                is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "subword SAR with {mem_width:?} must JIT"
            );
        }
    }

    for tag in [4, 5] {
        for (mem_width, boundary) in [(MemWidth::B1, 8), (MemWidth::B2, 16)] {
            for amount in [
                SrcOperand::Imm(0),
                SrcOperand::Imm(1),
                SrcOperand::Imm(boundary - 1),
                SrcOperand::Imm(boundary),
                SrcOperand::Imm(boundary + 1),
                SrcOperand::Imm(31),
                SrcOperand::Reg(x86(X86Reg::Rcx)),
            ] {
                let function = build(
                    tag,
                    tag,
                    amount.clone(),
                    amount,
                    mem_width,
                    FlagUpdate::None,
                    FlagUpdate::All,
                    address(),
                    0x1000,
                    false,
                );
                assert!(
                    is_native_clobber_safe_excluding(
                        &function,
                        &std::collections::HashMap::new(),
                        true,
                    ),
                    "subword shift tag {tag} with {mem_width:?} must JIT"
                );
            }
        }
    }

    for function in [
        {
            let mut function = build(
                4,
                4,
                SrcOperand::Imm(1),
                SrcOperand::Imm(1),
                MemWidth::B8,
                FlagUpdate::None,
                FlagUpdate::All,
                address(),
                0x1000,
                false,
            );
            function.blocks[0].ops[1].x86_hint = Some(X86OpHint::ShiftGroup6);
            function.blocks[0].ops[3].x86_hint = Some(X86OpHint::ShiftGroup6);
            function
        },
        build(
            0,
            0,
            SrcOperand::Imm(1),
            SrcOperand::Imm(1),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build(
            4,
            4,
            SrcOperand::Imm(1),
            SrcOperand::Imm(1),
            MemWidth::B8,
            FlagUpdate::All,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build(
            5,
            7,
            SrcOperand::Imm(1),
            SrcOperand::Imm(1),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build(
            7,
            7,
            SrcOperand::Reg(x86(X86Reg::Rdx)),
            SrcOperand::Reg(x86(X86Reg::Rdx)),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build(
            4,
            4,
            SrcOperand::Imm(256),
            SrcOperand::Imm(256),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build(
            4,
            4,
            SrcOperand::Imm(1),
            SrcOperand::Imm(2),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            false,
        ),
        build(
            4,
            4,
            SrcOperand::Imm(1),
            SrcOperand::Imm(1),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            Address::Absolute(0x2000),
            0x1000,
            false,
        ),
        build(
            4,
            4,
            SrcOperand::Imm(1),
            SrcOperand::Imm(1),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1001,
            false,
        ),
        build(
            4,
            4,
            SrcOperand::Imm(1),
            SrcOperand::Imm(1),
            MemWidth::B8,
            FlagUpdate::None,
            FlagUpdate::All,
            address(),
            0x1000,
            true,
        ),
    ] {
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
    }
}
#[test]
fn x86_scalar_memory_source_gate_accepts_exact_ssa_pairs_and_fails_closed() {
    let temporary = VReg::Virtual(VirtualId(17));
    let build = |consumer: OpKind,
                 mem_width: MemWidth,
                 sign: SignExtend,
                 consumer_pc: u64,
                 extra_use: bool,
                 addr: Address| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr,
                width: mem_width,
                sign,
            },
        );
        builder.push_op(consumer_pc, consumer);
        if extra_use {
            builder.push_op(
                0x1001,
                OpKind::Mov {
                    dst: x86(X86Reg::R11),
                    src: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };
    let address = || Address::BaseIndexScale {
        base: Some(x86(X86Reg::Rsp)),
        index: x86(X86Reg::R16),
        scale: 2,
        disp: -8,
        disp_size: DispSize::Disp8,
    };

    let valid = [
        (
            OpKind::Add {
                dst: x86(X86Reg::R8),
                src1: x86(X86Reg::R9),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
            MemWidth::B1,
        ),
        (
            OpKind::Sub {
                dst: x86(X86Reg::R10),
                src1: temporary,
                src2: SrcOperand::Reg(x86(X86Reg::R10)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            MemWidth::B2,
        ),
        (
            OpKind::Or {
                dst: x86(X86Reg::Rax),
                src1: temporary,
                src2: SrcOperand::Imm(0x1234),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            MemWidth::B4,
        ),
        (
            OpKind::Cmp {
                src1: x86(X86Reg::Rbx),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
            },
            MemWidth::B8,
        ),
        (
            OpKind::Cmp {
                src1: temporary,
                src2: SrcOperand::Reg(x86(X86Reg::Rdi)),
                width: OpWidth::W32,
            },
            MemWidth::B4,
        ),
        (
            OpKind::Cmp {
                src1: temporary,
                src2: SrcOperand::Imm(-1),
                width: OpWidth::W64,
            },
            MemWidth::B8,
        ),
        (
            OpKind::Test {
                src1: temporary,
                src2: SrcOperand::Reg(x86(X86Reg::Rsi)),
                width: OpWidth::W16,
            },
            MemWidth::B2,
        ),
        (
            OpKind::Test {
                src1: temporary,
                src2: SrcOperand::Imm(0x7F),
                width: OpWidth::W8,
            },
            MemWidth::B1,
        ),
        (
            OpKind::MulS {
                dst_lo: x86(X86Reg::Rax),
                dst_hi: None,
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
            MemWidth::B2,
        ),
        (
            OpKind::MulS {
                dst_lo: x86(X86Reg::R8),
                dst_hi: None,
                src1: x86(X86Reg::R8),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
            MemWidth::B4,
        ),
        (
            OpKind::MulS {
                dst_lo: x86(X86Reg::R15),
                dst_hi: None,
                src1: x86(X86Reg::R15),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            MemWidth::B8,
        ),
        (
            OpKind::X86Count {
                dst: x86(X86Reg::R8),
                src: temporary,
                width: OpWidth::W16,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::All,
            },
            MemWidth::B2,
        ),
        (
            OpKind::X86Count {
                dst: x86(X86Reg::R9),
                src: temporary,
                width: OpWidth::W32,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
            },
            MemWidth::B4,
        ),
        (
            OpKind::X86Count {
                dst: x86(X86Reg::R15),
                src: temporary,
                width: OpWidth::W64,
                kind: X86CountKind::Lzcnt,
                flags: FlagUpdate::None,
            },
            MemWidth::B8,
        ),
        (
            OpKind::Bsf {
                dst: x86(X86Reg::R8),
                src: temporary,
                width: OpWidth::W16,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
            MemWidth::B2,
        ),
        (
            OpKind::Bsr {
                dst: x86(X86Reg::R15),
                src: temporary,
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
            MemWidth::B8,
        ),
        (
            OpKind::Bt {
                src: temporary,
                index: SrcOperand::Imm(15),
                width: OpWidth::W16,
            },
            MemWidth::B2,
        ),
    ];
    for (consumer, mem_width) in valid {
        let function = build(
            consumer,
            mem_width,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        );
        assert!(is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            true,
        ));
        assert!(!is_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
            false,
        ));
    }

    let binary = |dst, src1, flags| OpKind::Add {
        dst,
        src1,
        src2: SrcOperand::Reg(temporary),
        width: OpWidth::W64,
        flags,
    };
    let base = binary(x86(X86Reg::R8), x86(X86Reg::R9), FlagUpdate::All);
    let invalid = [
        build(
            base.clone(),
            MemWidth::B8,
            SignExtend::Sign,
            0x1000,
            false,
            address(),
        ),
        build(
            base.clone(),
            MemWidth::B8,
            SignExtend::Zero,
            0x1001,
            false,
            address(),
        ),
        build(
            base.clone(),
            MemWidth::B4,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            base.clone(),
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            true,
            address(),
        ),
        build(
            binary(x86(X86Reg::R16), x86(X86Reg::R9), FlagUpdate::All),
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            binary(x86(X86Reg::R8), x86(X86Reg::Rsp), FlagUpdate::All),
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            binary(
                x86(X86Reg::R8),
                x86(X86Reg::R9),
                FlagUpdate::Specific(FlagSet::ZF),
            ),
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            base,
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            Address::Direct(VReg::Virtual(VirtualId(99))),
        ),
        build(
            OpKind::MulS {
                dst_lo: x86(X86Reg::R8),
                dst_hi: None,
                src1: x86(X86Reg::R9),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::MulS {
                dst_lo: x86(X86Reg::R8),
                dst_hi: None,
                src1: x86(X86Reg::R8),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
            MemWidth::B1,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::MulS {
                dst_lo: x86(X86Reg::Rsp),
                dst_hi: None,
                src1: x86(X86Reg::Rsp),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::MulS {
                dst_lo: x86(X86Reg::R8),
                dst_hi: None,
                src1: x86(X86Reg::R8),
                src2: SrcOperand::Reg(temporary),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF)),
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::X86Count {
                dst: x86(X86Reg::R8),
                src: temporary,
                width: OpWidth::W64,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::All,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: temporary,
                width: OpWidth::W64,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::All,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::X86Count {
                dst: x86(X86Reg::R8),
                src: temporary,
                width: OpWidth::W32,
                kind: X86CountKind::Lzcnt,
                flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::Bsf {
                dst: x86(X86Reg::R8),
                src: temporary,
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::Bsr {
                dst: x86(X86Reg::R16),
                src: temporary,
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::Bsf {
                dst: x86(X86Reg::R8),
                src: temporary,
                width: OpWidth::W32,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::Bt {
                src: temporary,
                index: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W64,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::Bt {
                src: temporary,
                index: SrcOperand::Imm(7),
                width: OpWidth::W32,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
        build(
            OpKind::Bt {
                src: temporary,
                index: SrcOperand::Imm(64),
                width: OpWidth::W64,
            },
            MemWidth::B8,
            SignExtend::Zero,
            0x1000,
            false,
            address(),
        ),
    ];
    for (case, function) in invalid.into_iter().enumerate() {
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "invalid scalar memory-source case {case}: {function:?}"
        );
    }

    let mut hinted_imul = build(
        OpKind::MulS {
            dst_lo: x86(X86Reg::R8),
            dst_hi: None,
            src1: x86(X86Reg::R8),
            src2: SrcOperand::Reg(temporary),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        MemWidth::B8,
        SignExtend::Zero,
        0x1000,
        false,
        address(),
    );
    hinted_imul.blocks[0].ops[1].x86_hint = Some(X86OpHint::ImulImm8);
    assert!(!is_native_clobber_safe_excluding(
        &hinted_imul,
        &std::collections::HashMap::new(),
        true,
    ));
}
#[test]
fn x86_immediate_memory_imul_gate_enforces_width_hint_and_signed_range() {
    let temporary = VReg::Virtual(VirtualId(18));
    let build = |dst_lo: VReg,
                 dst_hi: Option<VReg>,
                 src1: VReg,
                 value: i64,
                 width: OpWidth,
                 mem_width: MemWidth,
                 flags: FlagUpdate,
                 hint: Option<X86OpHint>| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(x86(X86Reg::Rbx)),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2: SrcOperand::Imm(value),
                width,
                flags,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = hint;
        function
    };

    for (name, dst, value, width, mem_width, flags, hint) in [
        (
            "W16 imm8 lower bound",
            x86(X86Reg::Rax),
            i64::from(i8::MIN),
            OpWidth::W16,
            MemWidth::B2,
            FlagUpdate::All,
            X86OpHint::ImulImm8,
        ),
        (
            "W16 imm16",
            x86(X86Reg::R8),
            0x1234,
            OpWidth::W16,
            MemWidth::B2,
            FlagUpdate::All,
            X86OpHint::ImulImm32,
        ),
        (
            "W32 NF imm8 upper bound",
            x86(X86Reg::R9),
            i64::from(i8::MAX),
            OpWidth::W32,
            MemWidth::B4,
            FlagUpdate::None,
            X86OpHint::ImulImm8,
        ),
        (
            "W32 imm32 lower bound",
            x86(X86Reg::R10),
            i64::from(i32::MIN),
            OpWidth::W32,
            MemWidth::B4,
            FlagUpdate::All,
            X86OpHint::ImulImm32,
        ),
        (
            "W64 NF imm32 upper bound",
            x86(X86Reg::R15),
            i64::from(i32::MAX),
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::None,
            X86OpHint::ImulImm32,
        ),
    ] {
        let function = build(
            dst,
            None,
            temporary,
            value,
            width,
            mem_width,
            flags,
            Some(hint),
        );
        assert!(
            is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}"
        );
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), false,),
            "{name} must still require memory helpers"
        );
    }

    for (name, dst_lo, dst_hi, src1, value, width, mem_width, flags, hint) in [
        (
            "W8 has no two/three-operand IMUL",
            x86(X86Reg::R8),
            None,
            temporary,
            1,
            OpWidth::W8,
            MemWidth::B1,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm8),
        ),
        (
            "imm8 overflow",
            x86(X86Reg::R8),
            None,
            temporary,
            i64::from(i8::MAX) + 1,
            OpWidth::W16,
            MemWidth::B2,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm8),
        ),
        (
            "imm16 overflow",
            x86(X86Reg::R8),
            None,
            temporary,
            i64::from(i16::MAX) + 1,
            OpWidth::W16,
            MemWidth::B2,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm32),
        ),
        (
            "W32 imm32 overflow",
            x86(X86Reg::R8),
            None,
            temporary,
            i64::from(i32::MAX) + 1,
            OpWidth::W32,
            MemWidth::B4,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm32),
        ),
        (
            "W64 imm32 underflow",
            x86(X86Reg::R8),
            None,
            temporary,
            i64::from(i32::MIN) - 1,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm32),
        ),
        (
            "unhinted",
            x86(X86Reg::R8),
            None,
            temporary,
            7,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
            None,
        ),
        (
            "wrong hint family",
            x86(X86Reg::R8),
            None,
            temporary,
            7,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
            Some(X86OpHint::Mulx),
        ),
        (
            "stack destination",
            x86(X86Reg::Rsp),
            None,
            temporary,
            7,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm8),
        ),
        (
            "extended destination",
            x86(X86Reg::R16),
            None,
            temporary,
            7,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm8),
        ),
        (
            "widening destination",
            x86(X86Reg::Rax),
            Some(x86(X86Reg::Rdx)),
            temporary,
            7,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm8),
        ),
        (
            "architectural first source",
            x86(X86Reg::R8),
            None,
            x86(X86Reg::Rax),
            7,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm8),
        ),
        (
            "partial flags",
            x86(X86Reg::R8),
            None,
            temporary,
            7,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF)),
            Some(X86OpHint::ImulImm8),
        ),
        (
            "memory width mismatch",
            x86(X86Reg::R8),
            None,
            temporary,
            7,
            OpWidth::W64,
            MemWidth::B4,
            FlagUpdate::All,
            Some(X86OpHint::ImulImm8),
        ),
    ] {
        let function = build(dst_lo, dst_hi, src1, value, width, mem_width, flags, hint);
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: {function:?}"
        );
    }
}
#[test]
fn x86_memory_cmove_gate_requires_exact_ssa_pair() {
    let temporary = VReg::Virtual(VirtualId(19));
    let build = |dst: VReg,
                 mem_width: MemWidth,
                 consumer_width: OpWidth,
                 sign: SignExtend,
                 consumer_pc: u64,
                 extra_use: bool| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rbx)),
                    index: x86(X86Reg::R16),
                    scale: 4,
                    disp: -9,
                    disp_size: DispSize::Disp8,
                },
                width: mem_width,
                sign,
            },
        );
        builder.push_op(
            consumer_pc,
            OpKind::CMove {
                dst,
                src: temporary,
                cond: Condition::Ne,
                width: consumer_width,
            },
        );
        if extra_use {
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: x86(X86Reg::R8),
                    src: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };

    for (name, dst, mem_width, width) in [
        (
            "CMOVNE AX,m16",
            x86(X86Reg::Rax),
            MemWidth::B2,
            OpWidth::W16,
        ),
        (
            "CMOVNE ESP,m32",
            x86(X86Reg::Rsp),
            MemWidth::B4,
            OpWidth::W32,
        ),
        (
            "CMOVNE RBP,m64",
            x86(X86Reg::Rbp),
            MemWidth::B8,
            OpWidth::W64,
        ),
        (
            "CMOVNE R16,m64",
            x86(X86Reg::R16),
            MemWidth::B8,
            OpWidth::W64,
        ),
    ] {
        let function = build(dst, mem_width, width, SignExtend::Zero, 0x1000, false);
        assert!(
            is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}"
        );
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), false,),
            "{name} must require memory helpers"
        );
    }

    for (name, dst, mem_width, width, sign, pc, extra_use) in [
        (
            "byte CMOV",
            x86(X86Reg::Rax),
            MemWidth::B1,
            OpWidth::W8,
            SignExtend::Zero,
            0x1000,
            false,
        ),
        (
            "signed load",
            x86(X86Reg::Rax),
            MemWidth::B8,
            OpWidth::W64,
            SignExtend::Sign,
            0x1000,
            false,
        ),
        (
            "width mismatch",
            x86(X86Reg::Rax),
            MemWidth::B4,
            OpWidth::W64,
            SignExtend::Zero,
            0x1000,
            false,
        ),
        (
            "non-GPR destination",
            x86(X86Reg::Xmm(0)),
            MemWidth::B8,
            OpWidth::W64,
            SignExtend::Zero,
            0x1000,
            false,
        ),
        (
            "different guest PC",
            x86(X86Reg::Rax),
            MemWidth::B8,
            OpWidth::W64,
            SignExtend::Zero,
            0x1001,
            false,
        ),
        (
            "extra temporary use",
            x86(X86Reg::Rax),
            MemWidth::B8,
            OpWidth::W64,
            SignExtend::Zero,
            0x1000,
            true,
        ),
    ] {
        let function = build(dst, mem_width, width, sign, pc, extra_use);
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: {function:?}"
        );
    }

    let mut hinted = build(
        x86(X86Reg::Rax),
        MemWidth::B8,
        OpWidth::W64,
        SignExtend::Zero,
        0x1000,
        false,
    );
    hinted.blocks[0].ops[1].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut hinted_load = build(
        x86(X86Reg::Rax),
        MemWidth::B8,
        OpWidth::W64,
        SignExtend::Zero,
        0x1000,
        false,
    );
    hinted_load.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe_excluding(
        &hinted_load,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut duplicate_definition = build(
        x86(X86Reg::Rax),
        MemWidth::B8,
        OpWidth::W64,
        SignExtend::Zero,
        0x1000,
        false,
    );
    let duplicate_load = duplicate_definition.blocks[0].ops[0].clone();
    duplicate_definition.blocks[0].ops.insert(0, duplicate_load);
    assert!(!is_native_clobber_safe_excluding(
        &duplicate_definition,
        &std::collections::HashMap::new(),
        true,
    ));
}
#[test]
fn x86_memory_extension_gate_requires_exact_ssa_pair() {
    let temporary = VReg::Virtual(VirtualId(18));
    let build = |signed: bool,
                 dst: VReg,
                 mem_width: MemWidth,
                 from_width: OpWidth,
                 to_width: OpWidth,
                 load_sign: SignExtend,
                 consumer_pc: u64,
                 extra_use: bool| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rbx)),
                    index: x86(X86Reg::R16),
                    scale: 2,
                    disp: -7,
                    disp_size: crate::smir::ir::types::DispSize::Disp8,
                },
                width: mem_width,
                sign: load_sign,
            },
        );
        builder.push_op(
            consumer_pc,
            if signed {
                OpKind::SignExtend {
                    dst,
                    src: temporary,
                    from_width,
                    to_width,
                }
            } else {
                OpKind::ZeroExtend {
                    dst,
                    src: temporary,
                    from_width,
                    to_width,
                }
            },
        );
        if extra_use {
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: x86(X86Reg::R8),
                    src: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };

    for (name, signed, dst, mem_width, from_width, to_width, load_sign) in [
        (
            "MOVZX SP,byte memory",
            false,
            x86(X86Reg::Rsp),
            MemWidth::B1,
            OpWidth::W8,
            OpWidth::W16,
            SignExtend::Zero,
        ),
        (
            "MOVSX EBP,byte memory",
            true,
            x86(X86Reg::Rbp),
            MemWidth::B1,
            OpWidth::W8,
            OpWidth::W32,
            SignExtend::Sign,
        ),
        (
            "MOVZX R16,word memory",
            false,
            x86(X86Reg::R16),
            MemWidth::B2,
            OpWidth::W16,
            OpWidth::W64,
            SignExtend::Zero,
        ),
        (
            "MOVSXD R15,dword memory",
            true,
            x86(X86Reg::R15),
            MemWidth::B4,
            OpWidth::W32,
            OpWidth::W64,
            SignExtend::Sign,
        ),
    ] {
        let function = build(
            signed, dst, mem_width, from_width, to_width, load_sign, 0x1000, false,
        );
        assert!(
            is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}"
        );
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), false,),
            "{name} must require memory helpers"
        );
    }

    for (name, signed, dst, mem_width, from_width, to_width, load_sign, pc, extra_use) in [
        (
            "load sign mismatch",
            true,
            x86(X86Reg::Rax),
            MemWidth::B1,
            OpWidth::W8,
            OpWidth::W64,
            SignExtend::Zero,
            0x1000,
            false,
        ),
        (
            "source width mismatch",
            false,
            x86(X86Reg::Rax),
            MemWidth::B2,
            OpWidth::W8,
            OpWidth::W64,
            SignExtend::Zero,
            0x1000,
            false,
        ),
        (
            "not widening",
            false,
            x86(X86Reg::Rax),
            MemWidth::B2,
            OpWidth::W16,
            OpWidth::W16,
            SignExtend::Zero,
            0x1000,
            false,
        ),
        (
            "non-GPR destination",
            true,
            x86(X86Reg::Xmm(0)),
            MemWidth::B1,
            OpWidth::W8,
            OpWidth::W64,
            SignExtend::Sign,
            0x1000,
            false,
        ),
        (
            "different guest PC",
            true,
            x86(X86Reg::Rax),
            MemWidth::B1,
            OpWidth::W8,
            OpWidth::W64,
            SignExtend::Sign,
            0x1001,
            false,
        ),
        (
            "extra temporary use",
            false,
            x86(X86Reg::Rax),
            MemWidth::B1,
            OpWidth::W8,
            OpWidth::W64,
            SignExtend::Zero,
            0x1000,
            true,
        ),
    ] {
        let function = build(
            signed, dst, mem_width, from_width, to_width, load_sign, pc, extra_use,
        );
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: {function:?}"
        );
    }

    let mut hinted = build(
        false,
        x86(X86Reg::Rax),
        MemWidth::B1,
        OpWidth::W8,
        OpWidth::W64,
        SignExtend::Zero,
        0x1000,
        false,
    );
    hinted.blocks[0].ops[1].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe_excluding(
        &hinted,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut hinted_load = build(
        true,
        x86(X86Reg::Rax),
        MemWidth::B2,
        OpWidth::W16,
        OpWidth::W64,
        SignExtend::Sign,
        0x1000,
        false,
    );
    hinted_load.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!is_native_clobber_safe_excluding(
        &hinted_load,
        &std::collections::HashMap::new(),
        true,
    ));

    let mut duplicate_definition = build(
        false,
        x86(X86Reg::Rax),
        MemWidth::B1,
        OpWidth::W8,
        OpWidth::W64,
        SignExtend::Zero,
        0x1000,
        false,
    );
    let duplicate_load = duplicate_definition.blocks[0].ops[0].clone();
    duplicate_definition.blocks[0].ops.insert(0, duplicate_load);
    assert!(!is_native_clobber_safe_excluding(
        &duplicate_definition,
        &std::collections::HashMap::new(),
        true,
    ));
}
#[test]
fn x86_widening_memory_multiply_gate_requires_exact_implicit_shape() {
    let temporary = VReg::Virtual(VirtualId(19));
    let build = |consumer: OpKind,
                 mem_width: MemWidth,
                 sign: SignExtend,
                 extra_use: bool,
                 hint: Option<X86OpHint>| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::Direct(x86(X86Reg::Rbx)),
                width: mem_width,
                sign,
            },
        );
        builder.push_op(0x1000, consumer);
        if extra_use {
            builder.push_op(
                0x1000,
                OpKind::Mov {
                    dst: x86(X86Reg::R8),
                    src: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = hint;
        function
    };
    let multiply = |signed: bool,
                    dst_lo: VReg,
                    dst_hi: Option<VReg>,
                    src1: VReg,
                    src2: SrcOperand,
                    width: OpWidth,
                    flags: FlagUpdate| {
        if signed {
            OpKind::MulS {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            }
        } else {
            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1,
                src2,
                width,
                flags,
            }
        }
    };
    let rax = x86(X86Reg::Rax);
    let rdx = x86(X86Reg::Rdx);
    let exact = |signed, width, flags| {
        multiply(
            signed,
            rax,
            (width != OpWidth::W8).then_some(rdx),
            rax,
            SrcOperand::Reg(temporary),
            width,
            flags,
        )
    };

    for (name, signed, width, mem_width, flags) in [
        (
            "MUL byte",
            false,
            OpWidth::W8,
            MemWidth::B1,
            FlagUpdate::All,
        ),
        (
            "IMUL word NF",
            true,
            OpWidth::W16,
            MemWidth::B2,
            FlagUpdate::None,
        ),
        (
            "MUL dword",
            false,
            OpWidth::W32,
            MemWidth::B4,
            FlagUpdate::All,
        ),
        (
            "IMUL qword",
            true,
            OpWidth::W64,
            MemWidth::B8,
            FlagUpdate::All,
        ),
    ] {
        let function = build(
            exact(signed, width, flags),
            mem_width,
            SignExtend::Zero,
            false,
            None,
        );
        assert!(
            is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}"
        );
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), false,),
            "{name} must require memory helpers"
        );
    }

    for (name, consumer, mem_width) in [
        (
            "non-RAX low destination",
            multiply(
                false,
                x86(X86Reg::R8),
                Some(rdx),
                rax,
                SrcOperand::Reg(temporary),
                OpWidth::W64,
                FlagUpdate::All,
            ),
            MemWidth::B8,
        ),
        (
            "non-RDX high destination",
            multiply(
                false,
                rax,
                Some(x86(X86Reg::Rcx)),
                rax,
                SrcOperand::Reg(temporary),
                OpWidth::W64,
                FlagUpdate::All,
            ),
            MemWidth::B8,
        ),
        (
            "non-RAX multiplicand",
            multiply(
                true,
                rax,
                Some(rdx),
                x86(X86Reg::Rcx),
                SrcOperand::Reg(temporary),
                OpWidth::W64,
                FlagUpdate::All,
            ),
            MemWidth::B8,
        ),
        (
            "non-memory source",
            multiply(
                false,
                rax,
                Some(rdx),
                rax,
                SrcOperand::Reg(x86(X86Reg::Rcx)),
                OpWidth::W64,
                FlagUpdate::All,
            ),
            MemWidth::B8,
        ),
        (
            "width mismatch",
            exact(true, OpWidth::W32, FlagUpdate::All),
            MemWidth::B8,
        ),
        (
            "partial flags",
            exact(
                false,
                OpWidth::W64,
                FlagUpdate::Specific(FlagSet::CF.union(FlagSet::OF)),
            ),
            MemWidth::B8,
        ),
    ] {
        let function = build(consumer, mem_width, SignExtend::Zero, false, None);
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: {function:?}"
        );
    }

    for (name, sign, extra_use, hint) in [
        ("signed load", SignExtend::Sign, false, None),
        ("extra temporary use", SignExtend::Zero, true, None),
        (
            "unexpected hint",
            SignExtend::Zero,
            false,
            Some(X86OpHint::Mulx),
        ),
    ] {
        let function = build(
            exact(true, OpWidth::W64, FlagUpdate::All),
            MemWidth::B8,
            sign,
            extra_use,
            hint,
        );
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: {function:?}"
        );
    }
}
#[test]
fn x86_crc32_gate_covers_register_and_single_use_memory_shapes() {
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let op = OpKind::Crc32C {
            dst: x86(X86Reg::R8),
            crc: x86(X86Reg::R8),
            data: x86(X86Reg::R9),
            data_width: width,
        };
        assert!(op.is_jit_safe(), "CRC32 must be class-whitelisted");
        assert!(x86_gate(op), "{width:?} register CRC32 must JIT");
    }

    for (dst, data, width) in [
        (X86Reg::Rsp, X86Reg::Rbp, OpWidth::W8),
        (X86Reg::R8, X86Reg::Rbp, OpWidth::W16),
        (X86Reg::Rbp, X86Reg::Rsp, OpWidth::W64),
        (X86Reg::R31, X86Reg::R16, OpWidth::W32),
    ] {
        let dst = x86(dst);
        let op = OpKind::Crc32C {
            dst,
            crc: dst,
            data: x86(data),
            data_width: width,
        };
        assert!(x86_gate(op), "state-backed {width:?} CRC32 must JIT");
    }

    for (name, op) in [
        (
            "non-destructive destination",
            OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R9),
                data: x86(X86Reg::R10),
                data_width: OpWidth::W64,
            },
        ),
        (
            "state-backed non-destructive accumulator",
            OpKind::Crc32C {
                dst: x86(X86Reg::Rsp),
                crc: x86(X86Reg::Rbp),
                data: x86(X86Reg::R10),
                data_width: OpWidth::W32,
            },
        ),
        (
            "virtual source",
            OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R8),
                data: VReg::Virtual(VirtualId(1)),
                data_width: OpWidth::W16,
            },
        ),
        (
            "invalid width",
            OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R8),
                data: x86(X86Reg::R9),
                data_width: OpWidth::W128,
            },
        ),
    ] {
        assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
        assert!(!x86_gate(op), "malformed {name} CRC32 must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Crc32C {
            dst: x86(X86Reg::Rbp),
            crc: x86(X86Reg::Rbp),
            data: x86(X86Reg::Rsp),
            data_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed CRC32 must fail closed"
    );

    let memory_crc = |extra_use: bool, signed: SignExtend, crc_width: OpWidth| {
        let temporary = VReg::Virtual(VirtualId(7));
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr: Address::BaseIndexScale {
                    base: Some(x86(X86Reg::Rsp)),
                    index: x86(X86Reg::R16),
                    scale: 2,
                    disp: 8,
                    disp_size: DispSize::Disp8,
                },
                width: MemWidth::B4,
                sign: signed,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Crc32C {
                dst: x86(X86Reg::R10),
                crc: x86(X86Reg::R10),
                data: temporary,
                data_width: crc_width,
            },
        );
        if extra_use {
            builder.push_op(
                0x1001,
                OpKind::Mov {
                    dst: x86(X86Reg::R11),
                    src: SrcOperand::Reg(temporary),
                    width: OpWidth::W64,
                },
            );
        }
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };

    let valid = memory_crc(false, SignExtend::Zero, OpWidth::W32);
    assert!(is_native_clobber_safe_excluding(
        &valid,
        &std::collections::HashMap::new(),
        true
    ));
    assert!(
        !is_native_clobber_safe_excluding(&valid, &std::collections::HashMap::new(), false),
        "memory CRC32 requires MMU-helper mode"
    );
    for invalid in [
        memory_crc(true, SignExtend::Zero, OpWidth::W32),
        memory_crc(false, SignExtend::Sign, OpWidth::W32),
        memory_crc(false, SignExtend::Zero, OpWidth::W64),
    ] {
        assert!(!is_native_clobber_safe_excluding(
            &invalid,
            &std::collections::HashMap::new(),
            true
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Crc32C {
            dst: x86(X86Reg::R8),
            crc: x86(X86Reg::R8),
            data: x86(X86Reg::R9),
            data_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let function = builder.finish();
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_scalar_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("sse4.2")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_scalar_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}
#[test]
fn x86_push_gate_requires_fault_precise_helper_fusion_and_exact_rsp_snapshot() {
    let rsp = x86(X86Reg::Rsp);
    let mut ordinary = FunctionBuilder::new(FunctionId(0), 0x1000);
    ordinary.push_op(
        0x1000,
        OpKind::Sub {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    ordinary.push_op(
        0x1000,
        OpKind::Store {
            src: x86(X86Reg::Rax),
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
        },
    );
    ordinary.set_terminator(Terminator::Return { values: vec![] });
    let ordinary = ordinary.finish();
    assert!(!is_native_clobber_safe_excluding(
        &ordinary,
        &std::collections::HashMap::new(),
        false
    ));
    assert!(is_native_clobber_safe_excluding(
        &ordinary,
        &std::collections::HashMap::new(),
        true
    ));

    let temporary = VReg::Virtual(VirtualId(7));
    let mut push_rsp = FunctionBuilder::new(FunctionId(0), 0x2000);
    push_rsp.push_op(
        0x2000,
        OpKind::Mov {
            dst: temporary,
            src: SrcOperand::Reg(rsp),
            width: OpWidth::W64,
        },
    );
    push_rsp.push_op(
        0x2000,
        OpKind::Sub {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    push_rsp.push_op(
        0x2000,
        OpKind::Store {
            src: temporary,
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
        },
    );
    push_rsp.set_terminator(Terminator::Return { values: vec![] });
    assert!(is_native_clobber_safe_excluding(
        &push_rsp.finish(),
        &std::collections::HashMap::new(),
        true
    ));

    let mut malformed = FunctionBuilder::new(FunctionId(0), 0x3000);
    malformed.push_op(
        0x3000,
        OpKind::Sub {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    malformed.push_op(
        0x3000,
        OpKind::Store {
            src: rsp,
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
        },
    );
    malformed.set_terminator(Terminator::Return { values: vec![] });
    assert!(!is_native_clobber_safe_excluding(
        &malformed.finish(),
        &std::collections::HashMap::new(),
        true
    ));
}
#[test]
fn x86_pop_gate_requires_exact_fault_precise_alias_shapes() {
    let rsp = x86(X86Reg::Rsp);
    let mut ordinary = FunctionBuilder::new(FunctionId(0), 0x1000);
    ordinary.push_op(
        0x1000,
        OpKind::Load {
            dst: x86(X86Reg::Rbp),
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    ordinary.push_op(
        0x1000,
        OpKind::Add {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    ordinary.set_terminator(Terminator::Return { values: vec![] });
    let ordinary = ordinary.finish();
    assert!(!is_native_clobber_safe_excluding(
        &ordinary,
        &std::collections::HashMap::new(),
        false
    ));
    assert!(is_native_clobber_safe_excluding(
        &ordinary,
        &std::collections::HashMap::new(),
        true
    ));

    let popped = VReg::Virtual(VirtualId(7));
    let mut pop_rsp = FunctionBuilder::new(FunctionId(1), 0x2000);
    pop_rsp.push_op(
        0x2000,
        OpKind::Load {
            dst: popped,
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    pop_rsp.push_op(
        0x2000,
        OpKind::Mov {
            dst: rsp,
            src: SrcOperand::Reg(popped),
            width: OpWidth::W64,
        },
    );
    pop_rsp.set_terminator(Terminator::Return { values: vec![] });
    assert!(is_native_clobber_safe_excluding(
        &pop_rsp.finish(),
        &std::collections::HashMap::new(),
        true
    ));

    let popped = VReg::Virtual(VirtualId(8));
    let incremented = VReg::Virtual(VirtualId(9));
    let mut pop_sp = FunctionBuilder::new(FunctionId(2), 0x3000);
    pop_sp.push_op(
        0x3000,
        OpKind::Load {
            dst: popped,
            addr: Address::Direct(rsp),
            width: MemWidth::B2,
            sign: SignExtend::Zero,
        },
    );
    pop_sp.push_op(
        0x3000,
        OpKind::Add {
            dst: incremented,
            src1: rsp,
            src2: SrcOperand::Imm(2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    pop_sp.push_op(
        0x3000,
        OpKind::Mov {
            dst: rsp,
            src: SrcOperand::Reg(incremented),
            width: OpWidth::W64,
        },
    );
    pop_sp.push_op(
        0x3000,
        OpKind::Mov {
            dst: rsp,
            src: SrcOperand::Reg(popped),
            width: OpWidth::W16,
        },
    );
    pop_sp.set_terminator(Terminator::Return { values: vec![] });
    assert!(is_native_clobber_safe_excluding(
        &pop_sp.finish(),
        &std::collections::HashMap::new(),
        true
    ));

    // A same-instruction POP-like pair with the wrong delta must fail
    // closed instead of being admitted as an independent Load and ADD.
    let mut malformed = FunctionBuilder::new(FunctionId(3), 0x4000);
    malformed.push_op(
        0x4000,
        OpKind::Load {
            dst: x86(X86Reg::Rax),
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    malformed.push_op(
        0x4000,
        OpKind::Add {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    malformed.set_terminator(Terminator::Return { values: vec![] });
    assert!(!is_native_clobber_safe_excluding(
        &malformed.finish(),
        &std::collections::HashMap::new(),
        true
    ));

    // The same two SMIR operations at distinct guest PCs are independent
    // instructions, not a malformed fused POP, and remain individually safe.
    let mut independent = FunctionBuilder::new(FunctionId(4), 0x5000);
    independent.push_op(
        0x5000,
        OpKind::Load {
            dst: x86(X86Reg::Rax),
            addr: Address::Direct(rsp),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    independent.push_op(
        0x5001,
        OpKind::Add {
            dst: rsp,
            src1: rsp,
            src2: SrcOperand::Imm(2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    independent.set_terminator(Terminator::Return { values: vec![] });
    assert!(is_native_clobber_safe_excluding(
        &independent.finish(),
        &std::collections::HashMap::new(),
        true
    ));
}
