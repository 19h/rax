//! tests::atomic tests

use super::*;
use crate::smir::lower::aarch64::*;

#[test]
fn lowers_atomic_load_acquire_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicLoad {
            dst: x(0),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            order: MemoryOrder::Acquire,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldar(3).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_load_acquire_base_offset_runtime() {
    let mem_addr = 0x9000_u64;
    let offset = 0x38_i64;
    let base = mem_addr - offset as u64;
    let mem_value = 0x1122_3344_5566_7788;
    let code = lower_single_op(OpKind::AtomicLoad {
        dst: x(0),
        addr: Address::BaseOffset {
            base: x(1),
            offset,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B8,
        order: MemoryOrder::Acquire,
    });

    let regs = [
        (1, base),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[0], mem_value);
    assert_eq!(out[1], base);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn lowers_atomic_load_acquire_base_index_scale_runtime() {
    let mem_addr = 0x9000_u64;
    let disp = 0x40_i32;
    let index = (mem_addr - disp as u64) / 8;
    let mem_value = 0x8877_6655_4433_2211;
    let code = lower_single_op(OpKind::AtomicLoad {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: None,
            index: x(3),
            scale: 8,
            disp,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B8,
        order: MemoryOrder::Acquire,
    });

    let regs = [
        (3, index),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b1000;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[0], mem_value);
    assert_eq!(out[3], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn lowers_atomic_load_relaxed_as_plain_load() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicLoad {
            dst: x(0),
            addr: Address::BaseOffset {
                base: x(1),
                offset: 2,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B2,
            order: MemoryOrder::Relaxed,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_uimm(1, 0b01, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_load_apx_egpr_value_operands() {
    let acquire = lower_single_op(OpKind::AtomicLoad {
        dst: x86(X86Reg::R16),
        addr: Address::Direct(x(1)),
        width: MemWidth::B8,
        order: MemoryOrder::Acquire,
    });
    let words = code_words(&acquire);
    assert_eq!(words[0], enc_ldar_regs(3, 16, 1));

    let relaxed = lower_single_op(OpKind::AtomicLoad {
        dst: x86(X86Reg::R17),
        addr: Address::BaseOffset {
            base: x(1),
            offset: 16,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B8,
        order: MemoryOrder::Relaxed,
    });
    let words = code_words(&relaxed);
    assert_eq!(words[0], enc_ldst_uimm_regs(3, 0b01, 2, 17, 1));
}
#[test]
fn rejects_atomic_load_apx_r31_value_mapping() {
    for order in [MemoryOrder::Acquire, MemoryOrder::Relaxed] {
        let err = try_lower_single_op(OpKind::AtomicLoad {
            dst: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            order,
        })
        .unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn rejects_atomic_load_release_order() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicLoad {
            dst: x(0),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            order: MemoryOrder::Release,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    assert!(lowerer.lower_function(&func).is_err());
}
#[test]
fn lowers_atomic_store_release_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicStore {
            src: x(3),
            addr: Address::Direct(x(1)),
            width: MemWidth::B4,
            order: MemoryOrder::Release,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_stlr(2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_store_release_base_offset() {
    let offset = -0x20_i64;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicStore {
            src: x(0),
            addr: Address::BaseOffset {
                base: x(1),
                offset,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B4,
            order: MemoryOrder::Release,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&enc_addsub_imm_regs(1, 0, 0, 0, 0, 16, 1).to_le_bytes());
    expected.extend_from_slice(&enc_addsub_imm_regs(1, 1, 0, 0, 0x20, 16, 16).to_le_bytes());
    expected.extend_from_slice(&enc_stlr_regs(2, 0, 16).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_store_release_base_index_scale() {
    let disp = 0x20_i32;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicStore {
            src: x(0),
            addr: Address::BaseIndexScale {
                base: Some(x(1)),
                index: x(3),
                scale: 4,
                disp,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B4,
            order: MemoryOrder::Release,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 0, 2, 16, 1, 3).to_le_bytes());
    expected.extend_from_slice(&enc_addsub_imm_regs(1, 0, 0, 0, 0x20, 16, 16).to_le_bytes());
    expected.extend_from_slice(&enc_stlr_regs(2, 0, 16).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_store_relaxed_as_plain_store() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicStore {
            src: x(0),
            addr: Address::BaseOffset {
                base: x(1),
                offset: 16,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
            order: MemoryOrder::Relaxed,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_uimm(3, 0b00, 2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_store_apx_egpr_value_operands() {
    let release = lower_single_op(OpKind::AtomicStore {
        src: x86(X86Reg::R18),
        addr: Address::Direct(x(1)),
        width: MemWidth::B4,
        order: MemoryOrder::Release,
    });
    let words = code_words(&release);
    assert_eq!(words[0], enc_stlr_regs(2, 18, 1));

    let relaxed = lower_single_op(OpKind::AtomicStore {
        src: x86(X86Reg::R19),
        addr: Address::BaseOffset {
            base: x(1),
            offset: 16,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B8,
        order: MemoryOrder::Relaxed,
    });
    let words = code_words(&relaxed);
    assert_eq!(words[0], enc_ldst_uimm_regs(3, 0b00, 2, 19, 1));
}
#[test]
fn rejects_atomic_store_apx_r31_value_mapping() {
    for order in [MemoryOrder::Release, MemoryOrder::Relaxed] {
        let err = try_lower_single_op(OpKind::AtomicStore {
            src: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            order,
        })
        .unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn rejects_atomic_store_acquire_order() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicStore {
            src: x(0),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            order: MemoryOrder::Acquire,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    assert!(lowerer.lower_function(&func).is_err());
}
#[test]
fn lowers_atomic_rmw_swap_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicRmw {
            dst: x(0),
            addr: Address::Direct(x(1)),
            src: x(2),
            op: AtomicOp::Swap,
            width: MemWidth::B8,
            order: MemoryOrder::Relaxed,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_atomic_rmw(3, 0, 0, 1, 0b000).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_rmw_and_zero_as_swap_zero() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicRmw {
            dst: x(0),
            addr: Address::Direct(x(1)),
            src: VReg::Imm(0),
            op: AtomicOp::And,
            width: MemWidth::B8,
            order: MemoryOrder::Relaxed,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_atomic_rmw_regs(3, 0, 0, 1, 0b000, 31, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_rmw_sub_zero_as_ldadd_zero() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicRmw {
            dst: x(0),
            addr: Address::Direct(x(1)),
            src: VReg::Imm(0),
            op: AtomicOp::Sub,
            width: MemWidth::B8,
            order: MemoryOrder::Relaxed,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_atomic_rmw_regs(3, 0, 0, 0, 0b000, 31, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_rmw_and_all_ones_as_ldclr_zero() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicRmw {
            dst: x(0),
            addr: Address::Direct(x(1)),
            src: VReg::Imm(-1),
            op: AtomicOp::And,
            width: MemWidth::B8,
            order: MemoryOrder::Relaxed,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_atomic_rmw_regs(3, 0, 0, 0, 0b001, 31, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_rmw_add_acqrel_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicRmw {
            dst: x(0),
            addr: Address::Direct(x(1)),
            src: x(2),
            op: AtomicOp::Add,
            width: MemWidth::B4,
            order: MemoryOrder::AcqRel,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_atomic_rmw(2, 1, 1, 0, 0b000).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_atomic_rmw_apx_egpr_lse_value_operands() {
    let code = lower_single_op(OpKind::AtomicRmw {
        dst: x86(X86Reg::R16),
        addr: Address::Direct(x(1)),
        src: x86(X86Reg::R17),
        op: AtomicOp::Add,
        width: MemWidth::B8,
        order: MemoryOrder::AcqRel,
    });
    let words = code_words(&code);
    assert_eq!(words[0], enc_atomic_rmw_regs(3, 1, 1, 0, 0b000, 17, 1, 16));
}
#[test]
fn rejects_atomic_rmw_apx_r31_value_mapping() {
    for kind in [
        OpKind::AtomicRmw {
            dst: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            src: x86(X86Reg::R16),
            op: AtomicOp::Add,
            width: MemWidth::B8,
            order: MemoryOrder::Relaxed,
        },
        OpKind::AtomicRmw {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            src: x86(X86Reg::R31),
            op: AtomicOp::Add,
            width: MemWidth::B8,
            order: MemoryOrder::Relaxed,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_atomic_rmw_lse_base_offset_runtime() {
    let mem_addr = 0x9000_u64;
    let offset = 0x50_i64;
    let base = mem_addr - offset as u64;
    let src_value = 5;
    let mem_value = 0x1234;
    let code = lower_single_op(OpKind::AtomicRmw {
        dst: x(0),
        addr: Address::BaseOffset {
            base: x(1),
            offset,
            disp_size: DispSize::Auto,
        },
        src: x(2),
        op: AtomicOp::Add,
        width: MemWidth::B8,
        order: MemoryOrder::Release,
    });
    let (expected_old, expected_mem) =
        ref_atomic_rmw(mem_value, src_value, MemWidth::B8, AtomicOp::Add);

    let regs = [
        (1, base),
        (2, src_value),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b0011;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[0], expected_old);
    assert_eq!(out[1], base);
    assert_eq!(out[2], src_value);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn lowers_atomic_rmw_lse_base_index_scale_runtime() {
    let mem_addr = 0x9000_u64;
    let index = 7_u64;
    let disp = 0x18_i32;
    let base = mem_addr - index * 8 - disp as u64;
    let src_value = 9;
    let mem_value = 0x4567;
    let code = lower_single_op(OpKind::AtomicRmw {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: Some(x(1)),
            index: x(3),
            scale: 8,
            disp,
            disp_size: DispSize::Auto,
        },
        src: x(2),
        op: AtomicOp::Add,
        width: MemWidth::B8,
        order: MemoryOrder::Release,
    });
    let (expected_old, expected_mem) =
        ref_atomic_rmw(mem_value, src_value, MemWidth::B8, AtomicOp::Add);

    let regs = [
        (1, base),
        (2, src_value),
        (3, index),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b0100;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[0], expected_old);
    assert_eq!(out[1], base);
    assert_eq!(out[2], src_value);
    assert_eq!(out[3], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn lowers_atomic_rmw_lse_with_immediate_source() {
    assert_atomic_rmw_lowering(
        "or_imm",
        AtomicOp::Or,
        0,
        1,
        VReg::Imm(0x55),
        None,
        0x55,
        MemWidth::B8,
        MemoryOrder::Release,
        0x100,
    );
}
#[test]
fn lowers_unfused_atomic_rmw_with_exclusive_loop() {
    assert_atomic_rmw_lowering(
        "and_reg",
        AtomicOp::And,
        0,
        1,
        x(2),
        Some(2),
        0x0ff0,
        MemWidth::B8,
        MemoryOrder::Relaxed,
        0xf0f0,
    );
    assert_atomic_rmw_lowering(
        "sub_dst_aliases_src",
        AtomicOp::Sub,
        0,
        1,
        x(0),
        Some(0),
        3,
        MemWidth::B8,
        MemoryOrder::Acquire,
        10,
    );
    assert_atomic_rmw_lowering(
        "and_dst_aliases_base",
        AtomicOp::And,
        1,
        1,
        x(2),
        Some(2),
        0xffff,
        MemWidth::B8,
        MemoryOrder::AcqRel,
        0x1234_5678,
    );
    assert_atomic_rmw_lowering(
        "nand_b1_imm",
        AtomicOp::Nand,
        0,
        1,
        VReg::Imm(-1),
        None,
        u64::MAX,
        MemWidth::B1,
        MemoryOrder::SeqCst,
        0x3c,
    );
    assert_atomic_rmw_lowering(
        "neg_b4_ignores_operand",
        AtomicOp::Neg,
        0,
        1,
        VReg::Imm(0),
        None,
        0,
        MemWidth::B4,
        MemoryOrder::SeqCst,
        1,
    );
}
#[test]
fn lowers_atomic_rmw_apx_egpr_exclusive_loop_runtime() {
    let mem_addr = 0x9000_u64;
    let src_value = 3;
    let mem_value = 10;
    let code = lower_single_op(OpKind::AtomicRmw {
        dst: x86(X86Reg::R16),
        addr: Address::Direct(x(1)),
        src: x86(X86Reg::R17),
        op: AtomicOp::Sub,
        width: MemWidth::B8,
        order: MemoryOrder::AcqRel,
    });
    let (expected_old, expected_mem) =
        ref_atomic_rmw(mem_value, src_value, MemWidth::B8, AtomicOp::Sub);

    let regs = [
        (1, mem_addr),
        (15, 0x1515_1515_1515_1515),
        (16, 0x1616_1616_1616_1616),
        (17, src_value),
        (18, 0x1818_1818_1818_1818),
    ];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[1], mem_addr);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], expected_old);
    assert_eq!(out[17], src_value);
    assert_eq!(out[18], 0x1818_1818_1818_1818);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn lowers_atomic_rmw_apx_egpr_address_operands_runtime() {
    let mem_addr = 0x9000_u64;
    let index = 4_u64;
    let disp = -0x18_i32;
    let sib_base = (mem_addr as i64 - (index as i64) * 8 - i64::from(disp)) as u64;
    let src_value = 7;
    let mem_value = 11;
    let code = lower_single_op(OpKind::AtomicRmw {
        dst: x86(X86Reg::R16),
        addr: Address::BaseIndexScale {
            base: Some(x86(X86Reg::R17)),
            index: x86(X86Reg::R18),
            scale: 8,
            disp,
            disp_size: DispSize::Auto,
        },
        src: x86(X86Reg::R19),
        op: AtomicOp::Add,
        width: MemWidth::B8,
        order: MemoryOrder::AcqRel,
    });
    let (expected_old, expected_mem) =
        ref_atomic_rmw(mem_value, src_value, MemWidth::B8, AtomicOp::Add);

    let regs = [
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
        (17, sib_base),
        (18, index),
        (19, src_value),
    ];
    let old_nzcv = 0b1100;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], expected_old);
    assert_eq!(out[17], sib_base);
    assert_eq!(out[18], index);
    assert_eq!(out[19], src_value);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn rejects_atomic_rmw_apx_r31_address_mapping() {
    for kind in [
        OpKind::AtomicRmw {
            dst: x86(X86Reg::R16),
            addr: Address::Direct(x86(X86Reg::R31)),
            src: x86(X86Reg::R17),
            op: AtomicOp::Add,
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        },
        OpKind::AtomicRmw {
            dst: x86(X86Reg::R16),
            addr: Address::BaseIndexScale {
                base: Some(x86(X86Reg::R17)),
                index: x86(X86Reg::R31),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            src: x86(X86Reg::R18),
            op: AtomicOp::Add,
            width: MemWidth::B8,
            order: MemoryOrder::AcqRel,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn lowers_atomic_rmw_exclusive_loop_base_offset_runtime() {
    let mem_addr = 0x9000_u64;
    let offset = 0x60_i64;
    let base = mem_addr - offset as u64;
    let src_value = 3;
    let mem_value = 10;
    let code = lower_single_op(OpKind::AtomicRmw {
        dst: x(0),
        addr: Address::BaseOffset {
            base: x(1),
            offset,
            disp_size: DispSize::Auto,
        },
        src: x(2),
        op: AtomicOp::Sub,
        width: MemWidth::B8,
        order: MemoryOrder::AcqRel,
    });
    let (expected_old, expected_mem) =
        ref_atomic_rmw(mem_value, src_value, MemWidth::B8, AtomicOp::Sub);

    let regs = [
        (1, base),
        (2, src_value),
        (15, 0x1515_1515_1515_1515),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b0101;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[0], expected_old);
    assert_eq!(out[1], base);
    assert_eq!(out[2], src_value);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn lowers_atomic_rmw_exclusive_loop_base_index_scale_runtime() {
    let mem_addr = 0x9000_u64;
    let index = 9_u64;
    let disp = 0x30_i32;
    let base = mem_addr - index * 8 - disp as u64;
    let src_value = 4;
    let mem_value = 20;
    let code = lower_single_op(OpKind::AtomicRmw {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: Some(x(1)),
            index: x(3),
            scale: 8,
            disp,
            disp_size: DispSize::Auto,
        },
        src: x(2),
        op: AtomicOp::Sub,
        width: MemWidth::B8,
        order: MemoryOrder::AcqRel,
    });
    let (expected_old, expected_mem) =
        ref_atomic_rmw(mem_value, src_value, MemWidth::B8, AtomicOp::Sub);

    let regs = [
        (1, base),
        (2, src_value),
        (3, index),
        (15, 0x1515_1515_1515_1515),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[0], expected_old);
    assert_eq!(out[1], base);
    assert_eq!(out[2], src_value);
    assert_eq!(out[3], index);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn lowers_atomic_cmpxadd_runtime() {
    assert_atomic_cmpxadd_lowering(
        "cmpxadd_b4_true",
        0,
        1,
        2,
        3,
        Condition::Ule,
        MemWidth::B4,
        5,
        7,
        3,
    );
    assert_atomic_cmpxadd_lowering(
        "cmpxadd_cmp_alias_false",
        2,
        1,
        2,
        3,
        Condition::Ugt,
        MemWidth::B4,
        1,
        2,
        99,
    );
    assert_atomic_cmpxadd_lowering(
        "cmpxadd_add_alias_true",
        2,
        1,
        3,
        2,
        Condition::Ugt,
        MemWidth::B8,
        5,
        1,
        3,
    );
    assert_atomic_cmpxadd_lowering(
        "cmpxadd_base_alias_true",
        1,
        1,
        2,
        3,
        Condition::Eq,
        MemWidth::B8,
        4,
        4,
        1,
    );
    assert_atomic_cmpxadd_lowering(
        "cmpxadd_b1_signed_condition",
        0,
        1,
        2,
        3,
        Condition::Slt,
        MemWidth::B1,
        0x80,
        1,
        2,
    );
}
#[test]
fn lowers_atomic_cmpxadd_sp_zero_offset_uses_original_sp_runtime() {
    let sp_reg = VReg::Arch(ArchReg::Arm(ArmReg::Sp));
    for (label, addr) in [
        ("direct_sp", Address::Direct(sp_reg)),
        ("base_offset_sp_zero", Address::base_off(sp_reg, 0)),
    ] {
        let mem_addr = 0x8000;
        let mem_value = 0x1111_2222_3333_4444;
        let cmp_value = mem_value;
        let add_value = 0x0101_0101_0101_0101;
        let code = lower_single_op(OpKind::AtomicCmpXadd {
            dst_old: x(0),
            addr,
            cmp: x(1),
            add: x(2),
            cond: Condition::Eq,
            width: MemWidth::B8,
            order: MemoryOrder::SeqCst,
        });
        let (expected_old, expected_mem, expected_nzcv) =
            ref_atomic_cmpxadd(mem_value, cmp_value, add_value, Condition::Eq, MemWidth::B8);

        let regs = [
            (1, cmp_value),
            (2, add_value),
            (14, 0x1414_1414_1414_1414),
            (15, 0x1515_1515_1515_1515),
            (16, 0x1616_1616_1616_1616),
            (17, 0x1717_1717_1717_1717),
        ];
        let old_nzcv = 0b0110;
        let (out, out_nzcv, sp, mem) =
            run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

        assert_eq!(out[0], expected_old, "{label}: old value");
        assert_eq!(out[1], cmp_value, "{label}: cmp preserved");
        assert_eq!(out[2], add_value, "{label}: add preserved");
        assert_eq!(out[14], 0x1414_1414_1414_1414, "{label}: x14 restored");
        assert_eq!(out[15], 0x1515_1515_1515_1515, "{label}: x15 restored");
        assert_eq!(out[16], 0x1616_1616_1616_1616, "{label}: x16 restored");
        assert_eq!(out[17], 0x1717_1717_1717_1717, "{label}: x17 restored");
        assert_eq!(out_nzcv, expected_nzcv, "{label}: NZCV");
        assert_eq!(sp, 0x8000, "{label}: stack restored");
        assert_eq!(mem, expected_mem, "{label}: memory");
    }
}
#[test]
fn lowers_atomic_cmpxadd_apx_egpr_value_operands_runtime() {
    let mem_addr = 0x9000_u64;
    let mem_value = 5;
    let cmp_value = 5;
    let add_value = 3;
    let code = lower_single_op(OpKind::AtomicCmpXadd {
        dst_old: x86(X86Reg::R16),
        addr: Address::Direct(x(1)),
        cmp: x86(X86Reg::R17),
        add: x86(X86Reg::R18),
        cond: Condition::Eq,
        width: MemWidth::B8,
        order: MemoryOrder::SeqCst,
    });
    let (expected_old, expected_mem, expected_nzcv) =
        ref_atomic_cmpxadd(mem_value, cmp_value, add_value, Condition::Eq, MemWidth::B8);

    let regs = [
        (1, mem_addr),
        (14, 0x1414_1414_1414_1414),
        (15, 0x1515_1515_1515_1515),
        (16, 0x1616_1616_1616_1616),
        (17, cmp_value),
        (18, add_value),
    ];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[1], mem_addr);
    assert_eq!(out[14], 0x1414_1414_1414_1414);
    assert_eq!(out[15], 0x1515_1515_1515_1515);
    assert_eq!(out[16], expected_old);
    assert_eq!(out[17], cmp_value);
    assert_eq!(out[18], add_value);
    assert_eq!(out_nzcv, expected_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn rejects_atomic_cmpxadd_apx_r31_value_mapping() {
    for kind in [
        OpKind::AtomicCmpXadd {
            dst_old: x86(X86Reg::R31),
            addr: Address::Direct(x(1)),
            cmp: x86(X86Reg::R17),
            add: x86(X86Reg::R18),
            cond: Condition::Eq,
            width: MemWidth::B8,
            order: MemoryOrder::SeqCst,
        },
        OpKind::AtomicCmpXadd {
            dst_old: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            cmp: x86(X86Reg::R31),
            add: x86(X86Reg::R18),
            cond: Condition::Eq,
            width: MemWidth::B8,
            order: MemoryOrder::SeqCst,
        },
        OpKind::AtomicCmpXadd {
            dst_old: x86(X86Reg::R16),
            addr: Address::Direct(x(1)),
            cmp: x86(X86Reg::R17),
            add: x86(X86Reg::R31),
            cond: Condition::Eq,
            width: MemWidth::B8,
            order: MemoryOrder::SeqCst,
        },
    ] {
        let err = try_lower_single_op(kind).unwrap_err();
        assert!(matches!(err, LowerError::InvalidRegister(_)));
    }
}
#[test]
fn rejects_atomic_cmpxadd_unsupported_forms() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicCmpXadd {
            dst_old: x(0),
            addr: Address::Direct(x(1)),
            cmp: x(2),
            add: x(3),
            cond: Condition::Eq,
            width: MemWidth::B16,
            order: MemoryOrder::SeqCst,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    assert!(matches!(
        lowerer.lower_function(&func),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::AtomicCmpXadd {
            dst_old: x(0),
            addr: Address::Direct(x(1)),
            cmp: x(2),
            add: x(3),
            cond: Condition::Parity,
            width: MemWidth::B8,
            order: MemoryOrder::SeqCst,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    assert!(matches!(
        lowerer.lower_function(&func),
        Err(LowerError::UnsupportedOp { .. })
    ));
}
