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
mod evex_fp16_scalar_replay;
#[cfg(test)]
mod evex_permute_replay;
#[cfg(test)]
mod far_call;
#[cfg(test)]
mod far_jump;
#[cfg(test)]
mod far_return;
#[cfg(test)]
mod fast_system_transfer;
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
mod lmsw;
#[cfg(test)]
mod maskmovdqu;
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
