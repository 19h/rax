//! jit_gate_tests.rs

use super::*;

// ---- split submodules ----
#[cfg(test)]
mod aarch64;
#[cfg(test)]
mod ac;
#[cfg(test)]
mod addr32_memory;
#[cfg(test)]
mod apx_bmi2_shift;
#[cfg(test)]
mod atomic_rmw;
#[cfg(test)]
mod cli;
#[cfg(test)]
mod clts;
#[cfg(test)]
mod cpuid;
#[cfg(test)]
mod descriptor_table;
#[cfg(test)]
mod evex;
#[cfg(test)]
mod evex_bw_immediate_replay;
#[cfg(test)]
mod evex_bw_shuffle_madd_replay;
#[cfg(test)]
mod evex_chunk_extract_replay;
#[cfg(test)]
mod evex_chunk_insert_replay;
#[cfg(test)]
mod evex_chunk_shuffle_replay;
#[cfg(test)]
mod evex_fp16_flag_compare_replay;
#[cfg(test)]
mod evex_fp16_narrow_replay;
#[cfg(test)]
mod evex_fp16_scalar_replay;
#[cfg(test)]
mod evex_fp16_widen_replay;
#[cfg(test)]
mod evex_fp_class_replay;
#[cfg(test)]
mod evex_fp_compare_replay;
#[cfg(test)]
mod evex_fp_sqrt_replay;
#[cfg(test)]
mod evex_gfni_replay;
#[cfg(test)]
mod evex_gpr_broadcast_replay;
#[cfg(test)]
mod evex_high_low_move_replay;
#[cfg(test)]
mod evex_lane_shuffle_replay;
#[cfg(test)]
mod evex_mask_blend_replay;
#[cfg(test)]
mod evex_mask_broadcast_replay;
#[cfg(test)]
mod evex_mask_to_vector_replay;
#[cfg(test)]
mod evex_move_replay;
#[cfg(test)]
mod evex_packed_compare_replay;
#[cfg(test)]
mod evex_packed_extend_replay;
#[cfg(test)]
mod evex_permute_replay;
#[cfg(test)]
mod evex_scalar_fp_convert_replay;
#[cfg(test)]
mod evex_scalar_fp_to_int_replay;
#[cfg(test)]
mod evex_scalar_int_to_fp_replay;
#[cfg(test)]
mod evex_scalar_integer_move_replay;
#[cfg(test)]
mod evex_scalar_lane_transfer_replay;
#[cfg(test)]
mod evex_scalar_move_replay;
#[cfg(test)]
mod evex_vector_align_replay;
#[cfg(test)]
mod evex_vector_to_mask_replay;
#[cfg(test)]
mod evex_vp2intersect_replay;
#[cfg(test)]
mod evex_vpclmulqdq_replay;
#[cfg(test)]
mod far_call;
#[cfg(test)]
mod far_jump;
#[cfg(test)]
mod far_return;
#[cfg(test)]
mod fast_system_transfer;
#[cfg(test)]
mod fp_arithmetic_replay;
#[cfg(test)]
mod fp_binary;
#[cfg(test)]
mod fsgsbase;
#[cfg(test)]
mod gate;
#[cfg(test)]
mod invlpg;
#[cfg(test)]
mod invpcid;
#[cfg(test)]
mod legacy_vex_fp_compare_replay;
#[cfg(test)]
mod legacy_vex_fp_horizontal_addsub_replay;
#[cfg(test)]
mod legacy_vex_fp_shuffle_replay;
#[cfg(test)]
mod legacy_vex_fp_sqrt_replay;
#[cfg(test)]
mod legacy_vex_high_low_move_replay;
#[cfg(test)]
mod legacy_vex_scalar_move_replay;
#[cfg(test)]
mod lmsw;
#[cfg(test)]
mod maskmovdqu;
#[cfg(test)]
mod mem_rmw_flagless;
#[cfg(test)]
mod mmx;
#[cfg(test)]
mod mmx_maskmov;
#[cfg(test)]
mod mmx_memory;
#[cfg(test)]
mod mmx_memory_source;
#[cfg(test)]
mod monitor_mwait;
#[cfg(test)]
mod movbe;
#[cfg(test)]
mod msr;
#[cfg(test)]
mod opmask;
#[cfg(test)]
mod pkru;
#[cfg(test)]
mod pmc;
#[cfg(test)]
mod read_control;
#[cfg(test)]
mod read_debug;
#[cfg(test)]
mod require_apx;
#[cfg(test)]
mod selector;
#[cfg(test)]
mod selector_query;
#[cfg(test)]
mod selector_verify;
#[cfg(test)]
mod serialize;
#[cfg(test)]
mod smsw;
#[cfg(test)]
mod sqrt;
#[cfg(test)]
mod sse4a;
#[cfg(test)]
mod state_alu;
#[cfg(test)]
mod state_lea;
#[cfg(test)]
mod state_mem_load;
#[cfg(test)]
mod sti;
#[cfg(test)]
mod swapgs;
#[cfg(test)]
mod timing;
#[cfg(test)]
mod trap;
#[cfg(test)]
mod vector;
#[cfg(test)]
mod vex_alignr_replay;
#[cfg(test)]
mod vex_apx_mulx;
#[cfg(test)]
mod vex_apx_mulx_memory;
#[cfg(test)]
mod vex_bmi2_shift;
#[cfg(test)]
mod vex_bmi2_shift_memory;
#[cfg(test)]
mod vex_cross_lane_128_replay;
#[cfg(test)]
mod vex_fma3_replay;
#[cfg(test)]
mod vex_fma4_replay;
#[cfg(test)]
mod vex_fp_dot_product_replay;
#[cfg(test)]
mod vex_fp_logic_replay;
#[cfg(test)]
mod vex_gfni_replay;
#[cfg(test)]
mod vex_immediate_blend_replay;
#[cfg(test)]
mod vex_packed_string_replay;
#[cfg(test)]
mod vex_scalar_insert_replay;
#[cfg(test)]
mod vex_variable_blend_replay;
#[cfg(test)]
mod vex_variable_permute_replay;
#[cfg(test)]
mod vex_vpclmulqdq_replay;
#[cfg(test)]
mod vex_widening_dword_multiply_replay;
#[cfg(test)]
mod waitpkg;
#[cfg(test)]
mod write_control;
#[cfg(test)]
mod write_debug;
#[cfg(test)]
mod x87_transcendental;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    ArmDpRegShiftKind, OpKind, X86AdxKind, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86SsePrefix, X86ThreeDNowKind, X86VecMap, X86X87ControlKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, ArmReg, BlockId, Condition, DispSize, FenceKind, FpPrecision, FpRoundMode,
    FunctionId, LocalId, MemWidth, OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg,
    VecCmpCond, VecElementType, VecUnaryOp, VecWidth, VirtualId, X86AesOp, X86Reg,
};
use crate::smir::ir::{CallTarget, FunctionBuilder, LocalSlot, PhiNode, Terminator};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn arm_x(n: u8) -> VReg {
    VReg::Arch(ArchReg::Arm(ArmReg::X(n)))
}

fn arm_v(n: u8) -> VReg {
    VReg::Arch(ArchReg::Arm(ArmReg::V(n)))
}

fn x86_gate(op: OpKind) -> bool {
    let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
    b.push_op(0x1000, op);
    b.set_terminator(Terminator::Return { values: vec![] });
    is_native_clobber_safe(&b.finish())
}

fn aarch64_gate(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (i, op) in ops.into_iter().enumerate() {
        b.push_op(0x1000 + i as u64 * 4, op);
    }
    b.set_terminator(Terminator::Return { values: vec![] });
    is_aarch64_native_clobber_safe_excluding(
        &b.finish(),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

fn aarch32_gate_with_mem(ops: Vec<OpKind>, allow_mem: bool) -> bool {
    let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (i, op) in ops.into_iter().enumerate() {
        b.push_op(0x1000 + i as u64 * 4, op);
    }
    b.set_terminator(Terminator::Return { values: vec![] });
    is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(
        &b.finish(),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

fn aarch32_gate(ops: Vec<OpKind>) -> bool {
    aarch32_gate_with_mem(ops, false)
}

fn aarch32_cond_cfg(
    test_dst: VReg,
    branch_cond: VReg,
    condition: Condition,
    op_after_test: Option<OpKind>,
) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let true_target = builder.create_block(0x2000);
    let false_target = builder.create_block(0x1004);
    builder.push_op(
        0x1000,
        OpKind::TestCondition {
            dst: test_dst,
            cond: condition,
        },
    );
    if let Some(op) = op_after_test {
        builder.push_op(0x1000, op);
    }
    builder.set_terminator(Terminator::CondBranch {
        cond: branch_cond,
        true_target,
        false_target,
    });
    builder.switch_to_block(true_target);
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    builder.switch_to_block(false_target);
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    builder.finish()
}

fn aarch32_call_cfg(
    target: CallTarget,
    link_dst: VReg,
    link_pc: i64,
    link_width: OpWidth,
    args: Vec<VReg>,
    continuation_pc: u64,
) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let continuation = builder.create_block(continuation_pc);
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: link_dst,
            src: SrcOperand::Imm(link_pc),
            width: link_width,
        },
    );
    builder.set_terminator(Terminator::Call {
        target,
        args,
        continuation,
    });
    builder.switch_to_block(continuation);
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    builder.finish()
}

fn aarch32_indirect_cfg(
    target: VReg,
    possible_targets: Vec<BlockId>,
) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::IndirectBranch {
        target,
        possible_targets,
    });
    builder.finish()
}

fn aarch32_blx_lr_cfg(
    snapshot_dst: VReg,
    snapshot_src: VReg,
    call_target: VReg,
    link_pc: i64,
    args: Vec<VReg>,
) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let continuation = builder.create_block(0x1004);
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: snapshot_dst,
            src: SrcOperand::Reg(snapshot_src),
            width: OpWidth::W32,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: arm_x(14),
            src: SrcOperand::Imm(link_pc),
            width: OpWidth::W32,
        },
    );
    builder.set_terminator(Terminator::Call {
        target: CallTarget::IndirectInterworking(call_target),
        args,
        continuation,
    });
    builder.switch_to_block(continuation);
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    builder.finish()
}

fn x86_aarch64_gate(ops: Vec<OpKind>) -> bool {
    let mut b = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (i, op) in ops.into_iter().enumerate() {
        b.push_op(0x1000 + i as u64, op);
    }
    b.set_terminator(Terminator::Return { values: vec![] });
    is_x86_aarch64_native_clobber_safe_excluding(&b.finish(), &std::collections::HashMap::new())
}
