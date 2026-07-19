//! compare.rs

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};

impl X86_64Lifter {
    pub(crate) fn append_vector_compare(
        &self,
        src1: VReg,
        src2: VReg,
        cond: VecCmpCond,
        elem: VecElementType,
        lanes: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
            },
        ));
        dst
    }

    /// Compute PTEST/VPTEST's two whole-vector reductions and commit exactly
    /// CF/ZF while clearing OF/SF/AF/PF. Memory operands are materialized by the
    /// caller before this helper, so no architectural flag write can precede a
    /// possible source fault.
    pub(crate) fn append_ptest_flags(
        &self,
        first: VReg,
        second: VReg,
        width: VecWidth,
        tested_bits: Option<u64>,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let and_acc = ctx.alloc_vreg();
        let andnot_acc = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: and_acc,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: andnot_acc,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        for lane in 0..width.lanes(VecElementType::I64) as u8 {
            let raw_a = ctx.alloc_vreg();
            let raw_b = ctx.alloc_vreg();
            let intersection = ctx.alloc_vreg();
            let outside = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: raw_a,
                    vec: first,
                    lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: raw_b,
                    vec: second,
                    lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            let (a, b) = if let Some(mask) = tested_bits {
                let a = ctx.alloc_vreg();
                let b = ctx.alloc_vreg();
                for (dst, src) in [(a, raw_a), (b, raw_b)] {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst,
                            src1: src,
                            src2: SrcOperand::Imm(mask as i64),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                }
                (a, b)
            } else {
                (raw_a, raw_b)
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: intersection,
                    src1: a,
                    src2: SrcOperand::Reg(b),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: and_acc,
                    src1: and_acc,
                    src2: SrcOperand::Reg(intersection),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::AndNot {
                    dst: outside,
                    src1: b,
                    src2: SrcOperand::Reg(a),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: andnot_acc,
                    src1: andnot_acc,
                    src2: SrcOperand::Reg(outside),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
        }

        let old_flags = ctx.alloc_vreg();
        let zf = ctx.alloc_vreg();
        let cf = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::ReadFlags { dst: old_flags },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Cmp {
                src1: and_acc,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::SetCC {
                dst: zf,
                cond: Condition::Eq,
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Cmp {
                src1: andnot_acc,
                src2: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::SetCC {
                dst: cf,
                cond: Condition::Eq,
                width: OpWidth::W64,
            },
        ));
        let shifted_zf = ctx.alloc_vreg();
        let cleared = ctx.alloc_vreg();
        let with_cf = ctx.alloc_vreg();
        let new_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Shl {
                dst: shifted_zf,
                src: zf,
                amount: SrcOperand::Imm(6),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        // CF|PF|AF|ZF|SF|OF are the complete PTEST-defined status set.
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::And {
                dst: cleared,
                src1: old_flags,
                src2: SrcOperand::Imm(!0x8D5),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Or {
                dst: with_cf,
                src1: cleared,
                src2: SrcOperand::Reg(cf),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Or {
                dst: new_flags,
                src1: with_cf,
                src2: SrcOperand::Reg(shifted_zf),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::WriteFlags { src: new_flags },
        ));
    }
}
