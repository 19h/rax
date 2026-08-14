//! tests::memory tests

use super::*;
use crate::smir::lower::aarch64::*;

#[test]
fn lowers_mov_x_same_reg_as_noop() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Reg(x(0)),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_mov_w_same_reg_as_self_mov_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Mov {
            dst: x(0),
            src: SrcOperand::Reg(x(0)),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_smulh_zero_source_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::MulS {
            dst_lo: VReg::virt(0),
            dst_hi: Some(x(0)),
            src1: x(1),
            src2: SrcOperand::Reg(VReg::Imm(0)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_umulh_two_imms_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::MulU {
            dst_lo: VReg::virt(0),
            dst_hi: Some(x(0)),
            src1: VReg::Imm(-1),
            src2: SrcOperand::Imm64(2),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 1, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cwd_w8_imm_sign_set_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cwd {
            dst: x(0),
            src: VReg::Imm(0x80),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xff, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_cwd_x_imm_sign_set_as_movn_zero() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Cwd {
            dst: x(0),
            src: VReg::Imm(-1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_xchg_same_x_as_noop() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Xchg {
            reg1: x(0),
            reg2: x(0),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_xchg_same_w_as_self_mov_zero_ext() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Xchg {
            reg1: x(0),
            reg2: x(0),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_xchg_same_w16_as_uxth() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Xchg {
            reg1: x(0),
            reg2: x(0),
            width: OpWidth::W16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 15, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_xchg_same_w8_as_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Xchg {
            reg1: x(0),
            reg2: x(0),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_lea_base_index_scale_disp() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Lea {
            dst: x(0),
            addr: Address::BaseIndexScale {
                base: Some(x(1)),
                index: x(2),
                scale: 4,
                disp: -0x20,
                disp_size: DispSize::Auto,
            },
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 0, 2, 0, 1, 2).to_le_bytes());
    expected.extend_from_slice(&enc_addsub_imm_regs(1, 1, 0, 0, 0x20, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_lea_index_scale_without_base() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Lea {
            dst: x(0),
            addr: Address::BaseIndexScale {
                base: None,
                index: x(2),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 0, 3, 0, 31, 2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
// Regression for issue #13: a base-less PC-relative LEA (ADR-style) resolves to
// the CURRENT guest PC + offset, matching the interpreter. The previous lowering
// used 0 as the base, so it computed an offset from zero. (The SP-base
// BaseIndexScale form is covered by `lowers_lea_sp_base_index_scale_runtime`.)
#[test]
fn issue_13_pcrel_baseless_lea_uses_guest_pc() {
    let guest_pc = 0x4000u64;
    let offset = 0x120i64;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        guest_pc,
        OpKind::Lea {
            dst: x(0),
            addr: Address::PcRel {
                offset,
                disp_size: DispSize::Auto,
                base: None,
            },
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();
    let (out, _, _) = run_aarch64_code(&code, &[], 0);
    assert_eq!(
        out[0],
        guest_pc.wrapping_add(offset as u64),
        "base-less PC-relative LEA must resolve to guest_pc + offset, not 0 + offset"
    );
}
#[test]
fn lowers_lea_sp_base_index_scale_runtime() {
    let index = 5;
    let disp = 0x20;
    let code = lower_single_op(OpKind::Lea {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::Arm(ArmReg::Sp))),
            index: x(2),
            scale: 4,
            disp,
            disp_size: DispSize::Auto,
        },
    });

    let old_nzcv = 0b1010;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(2, index)], old_nzcv);

    assert_eq!(out[0], 0x8000 + index * 4 + disp as u64);
    assert_eq!(out[2], index);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_lea_sp_base_index_scale_aliases_index_runtime() {
    let index = 7;
    let disp = -0x20;
    let code = lower_single_op(OpKind::Lea {
        dst: x(2),
        addr: Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::Arm(ArmReg::Sp))),
            index: x(2),
            scale: 8,
            disp,
            disp_size: DispSize::Auto,
        },
    });

    let old_nzcv = 0b0101;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &[(2, index)], old_nzcv);
    let expected = (0x8000_i64 + (index as i64) * 8 + i64::from(disp)) as u64;

    assert_eq!(out[2], expected);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_lea_large_base_offset_runtime() {
    let base = 0x1000;
    let offset = 0x12345;
    let code = lower_single_op(OpKind::Lea {
        dst: x(1),
        addr: Address::BaseOffset {
            base: x(1),
            offset,
            disp_size: DispSize::Auto,
        },
    });

    let regs = [(1, base), (16, 0x1616_1616_1616_1616)];
    let old_nzcv = 0b1100;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);

    assert_eq!(out[1], base + offset as u64);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_lea_large_base_index_disp_runtime() {
    let base = 0x200000;
    let index = 3;
    let disp = -0x12345;
    let code = lower_single_op(OpKind::Lea {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: Some(x(1)),
            index: x(2),
            scale: 2,
            disp,
            disp_size: DispSize::Auto,
        },
    });

    let regs = [(1, base), (2, index), (16, 0x1616_1616_1616_1616)];
    let old_nzcv = 0b0011;
    let (out, out_nzcv, sp) = run_aarch64_code(&code, &regs, old_nzcv);
    let expected = (base as i64 + (index as i64) * 2 + i64::from(disp)) as u64;

    assert_eq!(out[0], expected);
    assert_eq!(out[1], base);
    assert_eq!(out[2], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn fuses_scalar_pre_index_load_sequence() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Load {
            dst: x(0),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_simm(3, 0b01, 0b11, 8).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_scalar_post_index_store_sequence() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Store {
            src: x(0),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(-8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_simm(3, 0b00, 0b01, -8).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_pred_load_with_tbz_guard() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::PredLoad {
            dst: x(0),
            cond: x(2),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
            signed: SignExtend::Zero,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_test_branch(2, 0, false, 8).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_uimm(3, 0b01, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_pred_store_with_tbz_guard() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::PredStore {
            src: SrcOperand::Reg(x(0)),
            cond: x(2),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_test_branch(2, 0, false, 8).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_uimm(3, 0b00, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_pred_store_imm_with_tbz_guard() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::PredStore {
            src: SrcOperand::Imm(0x1234),
            cond: x(2),
            addr: Address::Direct(x(1)),
            width: MemWidth::B4,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_test_branch(2, 0, false, 32).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&enc_addsub_imm_regs(1, 0, 0, 0, 0, 16, 1).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 17, 31).to_le_bytes());
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x1234, 17).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_uimm_regs(2, 0b00, 0, 17, 16).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 17, 31).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_store_immediate_runtime() {
    let mem_addr = 0x9000;
    let value = 0x1122_3344_5566_7788;
    let code = lower_single_op(OpKind::Store {
        src: VReg::Imm(value as i64),
        addr: Address::Direct(x(1)),
        width: MemWidth::B8,
    });

    let old_nzcv = 0b0111;
    let regs = [
        (1, mem_addr),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, 0, MemWidth::B8);

    assert_eq!(mem, value);
    assert_eq!(out[1], mem_addr);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_store_immediate_sp_base_runtime() {
    let mem_addr = 0x8020;
    let value = 0x3456_789a;
    let code = lower_single_op(OpKind::Store {
        src: VReg::Imm(value),
        addr: Address::BaseOffset {
            base: VReg::Arch(ArchReg::Arm(ArmReg::Sp)),
            offset: 0x20,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B4,
    });

    let old_nzcv = 0b1001;
    let regs = [(16, 0x1616_1616_1616_1616), (17, 0x1717_1717_1717_1717)];
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, 0, MemWidth::B4);

    assert_eq!(mem, value as u64);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn lowers_pred_store_immediate_runtime() {
    let mem_addr = 0x9000;
    let value = 0x5566_7788;
    let code = lower_single_op(OpKind::PredStore {
        src: SrcOperand::Imm(value),
        cond: x(2),
        addr: Address::Direct(x(1)),
        width: MemWidth::B4,
    });

    let old_nzcv = 0b0011;
    let regs_true = [
        (1, mem_addr),
        (2, 1),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs_true, old_nzcv, mem_addr, 0, MemWidth::B4);

    assert_eq!(mem, value as u64);
    assert_eq!(out[1], mem_addr);
    assert_eq!(out[2], 1);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);

    let regs_false = [
        (1, mem_addr),
        (2, 0),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let (out, out_nzcv, sp, mem) = run_aarch64_code_with_memory(
        &code,
        &regs_false,
        old_nzcv,
        mem_addr,
        0xaabb_ccdd,
        MemWidth::B4,
    );

    assert_eq!(mem, 0xaabb_ccdd);
    assert_eq!(out[1], mem_addr);
    assert_eq!(out[2], 0);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
}
#[test]
fn rejects_rep_movs_in_native_lowerer() {
    for (label, dst, src, count, width) in [
        ("arm", x(0), x(1), x(2), MemWidth::B2),
        (
            "apx egpr",
            x86(X86Reg::R16),
            x86(X86Reg::R17),
            x86(X86Reg::R18),
            MemWidth::B4,
        ),
        ("unsupported width", x(0), x(1), x(2), MemWidth::B16),
    ] {
        let err = try_lower_single_op(OpKind::RepMovs {
            dst,
            src,
            count,
            width,
        })
        .unwrap_err();
        assert!(
            matches!(err, LowerError::UnsupportedOp { .. }),
            "{label}: {err:?}"
        );
    }
}
#[test]
fn fuses_signed_load_w_zero_extend_sequence() {
    let tmp = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Load {
            dst: tmp,
            addr: Address::Direct(x(1)),
            width: MemWidth::B1,
            sign: SignExtend::Sign,
        },
    );
    builder.push_op(
        0,
        OpKind::ZeroExtend {
            dst: x(0),
            src: tmp,
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_uimm(0, 0b11, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_signed_load_w_post_index_sequence() {
    let tmp = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Load {
            dst: tmp,
            addr: Address::Direct(x(1)),
            width: MemWidth::B2,
            sign: SignExtend::Sign,
        },
    );
    builder.push_op(
        0,
        OpKind::ZeroExtend {
            dst: x(0),
            src: tmp,
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_simm(1, 0b11, 0b01, 8).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_signed_load_w_reg_offset_sequence() {
    let ext = VReg::virt(0);
    let addr = VReg::virt(1);
    let load_tmp = VReg::virt(2);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::SignExtend {
            dst: ext,
            src: x(2),
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: addr,
            src1: x(1),
            src2: SrcOperand::Reg(ext),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Load {
            dst: load_tmp,
            addr: Address::Direct(addr),
            width: MemWidth::B1,
            sign: SignExtend::Sign,
        },
    );
    builder.push_op(
        0,
        OpKind::ZeroExtend {
            dst: x(0),
            src: load_tmp,
            from_width: OpWidth::W32,
            to_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_reg(0, 0b11, 2, 0b110, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_load_base_index_scale_as_scaled_reg_offset() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Load {
            dst: x(0),
            addr: Address::BaseIndexScale {
                base: Some(x(1)),
                index: x(2),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_reg(3, 0b01, 2, 0b011, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_store_base_index_scale_as_unscaled_reg_offset() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Store {
            src: x(0),
            addr: Address::BaseIndexScale {
                base: Some(x(1)),
                index: x(2),
                scale: 1,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B4,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_reg(2, 0b00, 2, 0b011, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_load_base_index_scale_mismatch_with_scratch_runtime() {
    let mem_addr = 0x9000;
    let index = 3;
    let base = mem_addr - index * 4;
    let mem_value = 0x1122_3344_5566_7788;
    let code = lower_single_op(OpKind::Load {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: Some(x(1)),
            index: x(2),
            scale: 4,
            disp: 0,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B8,
        sign: SignExtend::Zero,
    });

    let regs = [
        (1, base),
        (2, index),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b1011;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, mem_value, MemWidth::B8);

    assert_eq!(out[0], mem_value);
    assert_eq!(out[1], base);
    assert_eq!(out[2], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, mem_value);
}
#[test]
fn lowers_store_base_index_scale_disp_with_scratch_runtime() {
    let mem_addr = 0x9000;
    let index = 5;
    let disp = 0x20;
    let base = mem_addr - index * 8 - disp;
    let src_value = 0xaabb_ccdd;
    let code = lower_single_op(OpKind::Store {
        src: x(0),
        addr: Address::BaseIndexScale {
            base: Some(x(1)),
            index: x(2),
            scale: 8,
            disp: disp as i32,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B4,
    });

    let regs = [
        (0, src_value),
        (1, base),
        (2, index),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b0110;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, 0x1122_3344, MemWidth::B4);

    assert_eq!(out[0], src_value);
    assert_eq!(out[1], base);
    assert_eq!(out[2], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, src_value);
}
#[test]
fn lowers_load_index_scale_disp_without_base_runtime() {
    let mem_addr = 0x9000;
    let index = 0x10;
    let disp = 0x8fe0;
    let code = lower_single_op(OpKind::Load {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: None,
            index: x(2),
            scale: 2,
            disp,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B1,
        sign: SignExtend::Zero,
    });

    let regs = [
        (2, index),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b0101;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, 0xab, MemWidth::B1);

    assert_eq!(out[0], 0xab);
    assert_eq!(out[2], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, 0xab);
}
#[test]
fn lowers_load_sp_base_index_scale_disp_runtime() {
    let mem_addr = 0x9000;
    let index = 0x20;
    let disp = 0xf80;
    let code = lower_single_op(OpKind::Load {
        dst: x(0),
        addr: Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::Arm(ArmReg::Sp))),
            index: x(2),
            scale: 4,
            disp,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B2,
        sign: SignExtend::Zero,
    });

    let regs = [
        (2, index),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b1100;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, 0x7abc, MemWidth::B2);

    assert_eq!(out[0], 0x7abc);
    assert_eq!(out[2], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, 0x7abc);
}
#[test]
fn lowers_load_large_base_offset_runtime() {
    let mem_addr = 0x9000_u64;
    let offset = 0x12345_i64;
    let base = mem_addr.wrapping_sub(offset as u64);
    let mem_value = 0x1122_3344_5566_7788;
    let code = lower_single_op(OpKind::Load {
        dst: x(0),
        addr: Address::BaseOffset {
            base: x(1),
            offset,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B8,
        sign: SignExtend::Zero,
    });

    let regs = [
        (1, base),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b0011;
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
fn lowers_store_sp_base_large_offset_runtime() {
    let offset = -0x1234;
    let mem_addr = (0x8000_i64 + i64::from(offset)) as u64;
    let src_value = 0x7abc;
    let code = lower_single_op(OpKind::Store {
        src: x(0),
        addr: Address::BaseOffset {
            base: VReg::Arch(ArchReg::Arm(ArmReg::Sp)),
            offset: i64::from(offset),
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B2,
    });

    let regs = [
        (0, src_value),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b1100;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, 0x1234, MemWidth::B2);

    assert_eq!(out[0], src_value);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, src_value);
}
#[test]
fn lowers_load_exclusive_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::LoadExclusive {
            dst: x(0),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldxr(3).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_store_exclusive_direct() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::StoreExclusive {
            status: x(2),
            src: x(3),
            addr: Address::Direct(x(1)),
            width: MemWidth::B4,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_stxr(2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
// Regression for issue #10: STXR/STLXR are CONSTRAINED UNPREDICTABLE when the
// status register Rs aliases the data register Rt or the base register Rn.
// Emitting such an encoding can SIGILL on the host, so these forms must bail to
// the interpreter instead of lowering natively. Non-overlapping forms still
// lower.
#[test]
fn issue_10_rejects_store_exclusive_status_register_overlap() {
    // Rs == Rt (status aliases the stored data register).
    let err = try_lower_single_op(OpKind::StoreExclusive {
        status: x(2),
        src: x(2),
        addr: Address::Direct(x(1)),
        width: MemWidth::B4,
    })
    .unwrap_err();
    assert!(
        matches!(err, LowerError::UnsupportedOp { .. }),
        "Rs==Rt must be rejected: {err:?}"
    );

    // Rs == Rn (status aliases the address base register).
    let err = try_lower_single_op(OpKind::StoreExclusive {
        status: x(1),
        src: x(3),
        addr: Address::Direct(x(1)),
        width: MemWidth::B4,
    })
    .unwrap_err();
    assert!(
        matches!(err, LowerError::UnsupportedOp { .. }),
        "Rs==Rn must be rejected: {err:?}"
    );

    // No overlap (Rs, Rt, Rn all distinct): must still lower natively.
    assert!(
        try_lower_single_op(OpKind::StoreExclusive {
            status: x(2),
            src: x(3),
            addr: Address::Direct(x(1)),
            width: MemWidth::B4,
        })
        .is_ok(),
        "non-overlapping STXR must lower"
    );
}
#[test]
fn fuses_lifted_cls_x_imm_src_as_movz() {
    let sign_mask = VReg::virt(0);
    let normalized = VReg::virt(1);
    let leading = VReg::virt(2);
    let src = VReg::Imm(-16);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Sar {
            dst: sign_mask,
            src,
            amount: SrcOperand::Imm(63),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Xor {
            dst: normalized,
            src1: src,
            src2: SrcOperand::Reg(sign_mask),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::Clz {
            dst: leading,
            src: normalized,
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0,
        OpKind::Sub {
            dst: x(0),
            src1: leading,
            src2: SrcOperand::Imm(1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 59, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_ctz_w8_imm_zero_as_movz_width() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Ctz {
            dst: x(0),
            src: VReg::Imm(0x1_0000_0000),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 8, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_bfx_x_imm_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Bfx {
            dst: x(0),
            src: VReg::Imm(0x1234_5678_9abc_def0),
            lsb: 16,
            width_bits: 16,
            sign_extend: false,
            op_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x9abc, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_bfx_w_imm_sign_extend_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Bfx {
            dst: x(0),
            src: VReg::Imm(0xf0),
            lsb: 4,
            width_bits: 4,
            sign_extend: true,
            op_width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_bfx_x_imm_sign_extend_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Bfx {
            dst: x(0),
            src: VReg::Imm(0xf0),
            lsb: 4,
            width_bits: 4,
            sign_extend: true,
            op_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_logical_reg_identity_w_same_reg_as_self_mov_zero_ext() {
    let cases = [
        OpKind::And {
            dst: x(0),
            src1: x(0),
            src2: SrcOperand::Reg(x(0)),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
        OpKind::Or {
            dst: x(0),
            src1: x(0),
            src2: SrcOperand::Reg(x(0)),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    ];

    for kind in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(0, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
}
#[test]
fn lowers_logical_zero_base_w_same_dst_as_self_mov_zero_ext() {
    let cases = [
        OpKind::Or {
            dst: x(0),
            src1: VReg::Imm(0),
            src2: SrcOperand::Reg(x(0)),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
        OpKind::Xor {
            dst: x(0),
            src1: VReg::Imm(0),
            src2: SrcOperand::Reg(x(0)),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    ];

    for kind in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        builder.push_op(0, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.lower_function(&func).unwrap();
        let code = lowerer.finalize().unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&enc_mov_reg(0, 0, 0).to_le_bytes());
        expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
        assert_eq!(code, expected);
    }
}
#[test]
fn lowers_zero_extend_w8_imm_src_to_x_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::ZeroExtend {
            dst: x(0),
            src: VReg::Imm(0x12ab),
            from_width: OpWidth::W8,
            to_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0xab, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_zero_extend_w8_imm_src_to_w16_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::ZeroExtend {
            dst: x(0),
            src: VReg::Imm(0x12ab),
            from_width: OpWidth::W8,
            to_width: OpWidth::W16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0xab, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_sign_extend_w8_imm_src_positive_to_x_as_movz() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::SignExtend {
            dst: x(0),
            src: VReg::Imm(0x7f),
            from_width: OpWidth::W8,
            to_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x7f, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_constant_select_true_imm_as_mov_imm() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: VReg::Imm(1),
            src_true: VReg::Imm(0x2468),
            src_false: x(1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x2468, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_constant_select_true_w8_imm_as_mov_imm_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: VReg::Imm(1),
            src_true: VReg::Imm(0x1234),
            src_false: x(1),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(0, 0b10, 0, 0x1234, 0).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_register_select_identical_arms_as_mov() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: x(3),
            src_true: x(1),
            src_false: x(1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_register_select_w8_identical_arms_as_mov_uxtb() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: x(3),
            src_true: x(1),
            src_false: x(1),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(0, 0, 1).to_le_bytes());
    expected.extend_from_slice(&enc_bitfield_regs(0, 0b10, 0, 7, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_register_select_true_arm_dst_as_cbnz_over_false_mov() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: x(3),
            src_true: x(0),
            src_false: x(1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_cbnz(3, 2).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_register_select_false_arm_dst_as_cbz_over_true_mov() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Select {
            dst: x(0),
            cond: x(3),
            src_true: x(1),
            src_false: x(0),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_cbz(3, 2).to_le_bytes());
    expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rbit_w8_as_mov_reg() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rbit {
            dst: x(0),
            src: x(1),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rbit_w8_imm_as_movz_full_imm() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rbit {
            dst: x(0),
            src: VReg::Imm(0x1234),
            width: OpWidth::W8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b10, 0, 0x1234, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rbit_w16_as_mov_reg() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rbit {
            dst: x(0),
            src: x(1),
            width: OpWidth::W16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_reg(1, 0, 1).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rbit_w32_imm_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rbit {
            dst: x(0),
            src: VReg::Imm(-1),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(0, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_rbit_x_imm_all_ones_as_movn() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Rbit {
            dst: x(0),
            src: VReg::Imm(-1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_lifted_full_width_bfxil_imm_source_as_movn() {
    let extracted = VReg::virt(0);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Bfx {
            dst: extracted,
            src: VReg::Imm(-1),
            lsb: 0,
            width_bits: 64,
            sign_extend: false,
            op_width: OpWidth::W64,
        },
    );
    builder.push_op(
        0,
        OpKind::Bfi {
            dst: x(0),
            dst_in: x(1),
            src: extracted,
            lsb: 0,
            width_bits: 64,
            op_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_mov_wide(1, 0b00, 0, 0, 0).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_load_pair_large_base_offset_via_scratch() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::LoadPair {
            dst1: x(0),
            dst2: x(2),
            addr: Address::BaseOffset {
                base: x(1),
                offset: 0x400,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
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
    expected.extend_from_slice(&enc_addsub_imm_regs(1, 0, 0, 0, 0x400, 16, 16).to_le_bytes());
    expected.extend_from_slice(&enc_ldp_regs(0b10, 0b10, true, 0, 0, 2, 16).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_load_pair_base_index_scale_via_scratch() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::LoadPair {
            dst1: x(0),
            dst2: x(2),
            addr: Address::BaseIndexScale {
                base: Some(x(1)),
                index: x(3),
                scale: 8,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            width: MemWidth::B8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b00, 0b11, -16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&enc_addsub_shift_regs(1, 0, 0, 0, 3, 16, 1, 3).to_le_bytes());
    expected.extend_from_slice(&enc_ldp_regs(0b10, 0b10, true, 0, 0, 2, 16).to_le_bytes());
    expected.extend_from_slice(&enc_ldst_simm_regs(3, 0b01, 0b01, 16, 16, 31).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn lowers_store_pair_base_index_scale_disp_runtime() {
    let mem_addr = 0x9000_u64;
    let index = 6_u64;
    let disp = 0x20_i32;
    let base = mem_addr - index * 4 - disp as u64;
    let src1 = 0xaaaa_bbbb_1122_3344;
    let src2 = 0xcccc_dddd_5566_7788;
    let expected_mem = ((src2 & 0xffff_ffff) << 32) | (src1 & 0xffff_ffff);
    let code = lower_single_op(OpKind::StorePair {
        src1: x(0),
        src2: x(2),
        addr: Address::BaseIndexScale {
            base: Some(x(1)),
            index: x(3),
            scale: 4,
            disp,
            disp_size: DispSize::Auto,
        },
        width: MemWidth::B4,
    });

    let regs = [
        (0, src1),
        (1, base),
        (2, src2),
        (3, index),
        (16, 0x1616_1616_1616_1616),
        (17, 0x1717_1717_1717_1717),
    ];
    let old_nzcv = 0b1001;
    let (out, out_nzcv, sp, mem) =
        run_aarch64_code_with_memory(&code, &regs, old_nzcv, mem_addr, 0, MemWidth::B8);

    assert_eq!(out[0], src1);
    assert_eq!(out[1], base);
    assert_eq!(out[2], src2);
    assert_eq!(out[3], index);
    assert_eq!(out[16], 0x1616_1616_1616_1616);
    assert_eq!(out[17], 0x1717_1717_1717_1717);
    assert_eq!(out_nzcv, old_nzcv);
    assert_eq!(sp, 0x8000);
    assert_eq!(mem, expected_mem);
}
#[test]
fn fuses_pair_pre_index_load_sequence() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(16),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0,
        OpKind::LoadPair {
            dst1: x(0),
            dst2: x(2),
            addr: Address::Direct(x(1)),
            width: MemWidth::B8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldp(0b10, 0b11, true, 2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn fuses_pair_post_index_store_sequence() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::StorePair {
            src1: x(0),
            src2: x(2),
            addr: Address::Direct(x(1)),
            width: MemWidth::B4,
        },
    );
    builder.push_op(
        0,
        OpKind::Add {
            dst: x(1),
            src1: x(1),
            src2: SrcOperand::Imm(-8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.lower_function(&func).unwrap();
    let code = lowerer.finalize().unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(&enc_ldp(0b00, 0b01, false, -2).to_le_bytes());
    expected.extend_from_slice(&0xd65f_03c0u32.to_le_bytes());
    assert_eq!(code, expected);
}
#[test]
fn rejects_lea_unsupported_scale() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    builder.push_op(
        0,
        OpKind::Lea {
            dst: x(0),
            addr: Address::BaseIndexScale {
                base: Some(x(1)),
                index: x(2),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let err = lowerer.lower_function(&func).unwrap_err();
    assert!(matches!(err, LowerError::UnsupportedOp { .. }));
}
