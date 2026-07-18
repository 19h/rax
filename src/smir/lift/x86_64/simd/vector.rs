//! Shared packed-vector lifting helpers (append_*, lift_vec_*)

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
use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl X86_64Lifter {

    /// Merge a computed legacy packed-XMM result into the architectural
    /// destination without changing the shared YMM/ZMM state above bit 127.
    pub(crate) fn append_legacy_packed_result(
        &self,
        dst: VReg,
        result: VReg,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = VecWidth::V128.lanes(elem) as u8;
        let mut scalars = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: result,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            scalars.push((lane, scalar));
        }
        for (lane, scalar) in scalars {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
    }


    pub(crate) fn append_zero_vector(
        &self,
        width: VecWidth,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        let vector = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: vector,
                scalar: zero,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        vector
    }


    pub(crate) fn append_vector_splat_imm(
        &self,
        value: u64,
        width: VecWidth,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(value as i64),
                width: OpWidth::W64,
            },
        ));
        let vector = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        vector
    }


    pub(crate) fn append_vector_and(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }


    /// Append the vector operation `!src1 & src2`.
    pub(crate) fn append_vector_and_not(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }


    pub(crate) fn append_vector_or(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }


    pub(crate) fn append_vector_xor(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }


    pub(crate) fn append_vector_sub(
        &self,
        src1: VReg,
        src2: VReg,
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
            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes,
            },
        ));
        dst
    }


    pub(crate) fn append_vector_shift(
        &self,
        src: VReg,
        amount: u8,
        shift: ShiftOp,
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
            OpKind::VShift {
                dst,
                src,
                amount: SrcOperand::Imm(i64::from(amount)),
                shift,
                elem,
                lanes,
            },
        ));
        dst
    }


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


    pub(crate) fn append_broadcast_memory_source(
        &self,
        addr: Address,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Load {
                dst: scalar,
                addr,
                width: match elem.bytes() {
                    1 => MemWidth::B1,
                    2 => MemWidth::B2,
                    4 => MemWidth::B4,
                    8 => MemWidth::B8,
                    _ => unreachable!(),
                },
                sign: SignExtend::Zero,
            },
        ));
        let vector = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        vector
    }


    /// Materialize an EVEX scalar broadcast whose memory access is suppressed
    /// when every applicable opmask bit is clear. The architectural memory
    /// operand is scalar, so aggregate the lane predicates and issue at most
    /// one read before broadcasting it to the active vector width.
    pub(crate) fn append_masked_broadcast_memory_source(
        &self,
        addr: Address,
        elem: VecElementType,
        width: VecWidth,
        mask: VReg,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let lanes = width.lanes(elem) as u8;
        let lane_mask = if lanes == 64 {
            u64::MAX
        } else {
            (1u64 << lanes) - 1
        };
        let active = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::And {
                dst: active,
                src1: mask,
                src2: SrcOperand::Imm(lane_mask as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::PredLoad {
                dst: scalar,
                cond: active,
                addr,
                width: match elem.bytes() {
                    1 => MemWidth::B1,
                    2 => MemWidth::B2,
                    4 => MemWidth::B4,
                    8 => MemWidth::B8,
                    _ => unreachable!(),
                },
                signed: SignExtend::Zero,
            },
        ));
        let vector = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem,
                lanes,
            },
        ));
        vector
    }


    pub(crate) fn append_permute_immediate_indices(
        &self,
        width: VecWidth,
        elem: VecElementType,
        imm: u8,
        table_domain_lanes: u8,
        control_repeat_lanes: u8,
        control_bits: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        let lanes = width.lanes(elem) as u8;
        let selector_mask = (1u8 << control_bits) - 1;
        for lane in 0..lanes {
            let domain_base = lane / table_domain_lanes * table_domain_lanes;
            let shift = (lane % control_repeat_lanes) * control_bits;
            let selector = (imm >> shift) & selector_mask;
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(domain_base + selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        indices
    }


    pub(crate) fn append_permil_variable_indices(
        &self,
        controls: VReg,
        width: VecWidth,
        elem: VecElementType,
        domain_lanes: u8,
        control_shift: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        let lanes = width.lanes(elem) as u8;
        for lane in 0..lanes {
            let control = ctx.alloc_vreg();
            let shifted = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
            let absolute = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: control,
                    vec: controls,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            if control_shift == 0 {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: shifted,
                        src: SrcOperand::Reg(control),
                        width: OpWidth::W64,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: shifted,
                        src: control,
                        amount: SrcOperand::Imm(i64::from(control_shift)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: selected,
                    src1: shifted,
                    src2: SrcOperand::Imm(i64::from(domain_lanes - 1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: absolute,
                    src1: selected,
                    src2: SrcOperand::Imm(i64::from(lane / domain_lanes * domain_lanes)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar: absolute,
                    lane,
                    elem,
                },
            ));
        }
        indices
    }


    pub(crate) fn append_masked_permute_memory_result(
        &self,
        addr: Address,
        indices: VReg,
        width: VecWidth,
        elem: VecElementType,
        broadcast: bool,
        mask: VReg,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let result = self.append_zero_vector(width, elem, pc, ctx, ops);
        let lanes = width.lanes(elem) as u8;
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = match elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
            VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
            _ => unreachable!(),
        };
        for lane in 0..lanes {
            let shifted_mask = ctx.alloc_vreg();
            let active = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
            let bounded = ctx.alloc_vreg();
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: shifted_mask,
                    src: mask,
                    amount: SrcOperand::Imm(i64::from(lane)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: active,
                    src1: shifted_mask,
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: selected,
                    vec: indices,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: bounded,
                    src1: selected,
                    src2: SrcOperand::Imm(i64::from(lanes - 1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::PredLoad {
                    dst: scalar,
                    cond: active,
                    addr: if broadcast {
                        Address::Direct(base)
                    } else {
                        Address::BaseIndexScale {
                            base: Some(base),
                            index: bounded,
                            scale: elem.bytes() as u8,
                            disp: 0,
                            disp_size: DispSize::Auto,
                        }
                    },
                    width: mem_width,
                    signed: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: result,
                    vec: result,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        result
    }


    pub(crate) fn append_two_table_permute_memory_result(
        &self,
        table1: VReg,
        addr: Address,
        indices: VReg,
        width: VecWidth,
        elem: VecElementType,
        broadcast: bool,
        mask: Option<VReg>,
        overwrite_table: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let lanes = width.lanes(elem) as u8;
        let zero_table = self.append_zero_vector(width, elem, pc, ctx, ops);
        let result = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPermute {
                dst: result,
                src1: table1,
                src2: Some(zero_table),
                indices,
                elem,
                width,
                overwrite_table,
            },
        ));
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = match elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
            VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
            _ => unreachable!(),
        };
        let table_bit = lanes.ilog2();
        for lane in 0..lanes {
            let index = ctx.alloc_vreg();
            let table_shifted = ctx.alloc_vreg();
            let selects_memory = ctx.alloc_vreg();
            let bounded = ctx.alloc_vreg();
            let load_cond = if let Some(mask) = mask {
                let mask_shifted = ctx.alloc_vreg();
                let active = ctx.alloc_vreg();
                let cond = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: mask_shifted,
                        src: mask,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: active,
                        src1: mask_shifted,
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                Some((active, cond))
            } else {
                None
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: index,
                    vec: indices,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: table_shifted,
                    src: index,
                    amount: SrcOperand::Imm(i64::from(table_bit)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: selects_memory,
                    src1: table_shifted,
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            let cond = if let Some((active, cond)) = load_cond {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: cond,
                        src1: active,
                        src2: SrcOperand::Reg(selects_memory),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                cond
            } else {
                selects_memory
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: bounded,
                    src1: index,
                    src2: SrcOperand::Imm(i64::from(lanes - 1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: loaded,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::PredLoad {
                    dst: loaded,
                    cond,
                    addr: if broadcast {
                        Address::Direct(base)
                    } else {
                        Address::BaseIndexScale {
                            base: Some(base),
                            index: bounded,
                            scale: elem.bytes() as u8,
                            disp: 0,
                            disp_size: DispSize::Auto,
                        }
                    },
                    width: mem_width,
                    signed: SignExtend::Zero,
                },
            ));
            let table_value = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: table_value,
                    vec: result,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: selected,
                    cond: selects_memory,
                    src_true: loaded,
                    src_false: table_value,
                    width: match elem {
                        VecElementType::I8 => OpWidth::W8,
                        VecElementType::I16 => OpWidth::W16,
                        VecElementType::I32 | VecElementType::F32 => OpWidth::W32,
                        VecElementType::I64 | VecElementType::F64 => OpWidth::W64,
                        _ => unreachable!(),
                    },
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: result,
                    vec: result,
                    scalar: selected,
                    lane,
                    elem,
                },
            ));
        }
        result
    }


    pub(crate) fn append_packed_shuffle_imm(
        &self,
        dst: VReg,
        src: VReg,
        width: VecWidth,
        elem: VecElementType,
        imm: u8,
        high_words: Option<bool>,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = if elem == VecElementType::I32 { 4 } else { 8 };
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        for lane in 0..lanes {
            let within = lane % block_lanes;
            let block = lane - within;
            let shuffled = match high_words {
                None => true,
                Some(true) => within >= 4,
                Some(false) => within < 4,
            };
            let selector = if shuffled {
                let output = within % 4;
                block + if high_words == Some(true) { 4 } else { 0 } + ((imm >> (output * 2)) & 3)
            } else {
                lane
            };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1: src,
                src2: None,
                indices,
                elem,
                lanes,
            },
        ));
    }


    pub(crate) fn append_two_source_shuffle_imm(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        elem: VecElementType,
        imm: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = if elem == VecElementType::F32 { 4 } else { 2 };
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        for lane in 0..lanes {
            let within = lane % block_lanes;
            let block = lane - within;
            let (from_second, control) = if elem == VecElementType::F32 {
                (within >= 2, (imm >> (within * 2)) & 3)
            } else {
                (within == 1, (imm >> lane) & 1)
            };
            let selector = block + control + if from_second { lanes } else { 0 };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1,
                src2: Some(src2),
                indices,
                elem,
                lanes,
            },
        ));
    }


    pub(crate) fn append_duplicate_shuffle(
        &self,
        dst: VReg,
        src: VReg,
        width: VecWidth,
        elem: VecElementType,
        high: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        for lane in 0..lanes {
            let selector = lane / 2 * 2 + u8::from(high);
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1: src,
                src2: None,
                indices,
                elem,
                lanes,
            },
        ));
    }


    /// Append the exact per-lane semantics of PSIGNB/W/D using generic SMIR:
    /// negative control -> wrapping negation, zero control -> zero, positive
    /// control -> original value.
    pub(crate) fn append_packed_sign(
        &self,
        dst: VReg,
        value: VReg,
        control: VReg,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let negated = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VUnary {
                dst: negated,
                src: value,
                elem,
                lanes,
                op: VecUnaryOp::Neg,
            },
        ));
        let negative = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: negative,
                src1: control,
                src2: zero,
                cond: VecCmpCond::Lt,
                elem,
                lanes,
            },
        ));
        let is_zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: is_zero,
                src1: control,
                src2: zero,
                cond: VecCmpCond::Eq,
                elem,
                lanes,
            },
        ));
        let signed = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst: signed,
                mask: negative,
                src_true: negated,
                src_false: value,
                width,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VAndNot {
                dst,
                src1: is_zero,
                src2: signed,
                width,
            },
        ));
    }


    pub(crate) fn packed_minmax_shape(opcode: u8, qword: bool) -> (VecElementType, bool, bool) {
        let elem = match opcode {
            0xDA | 0xDE => VecElementType::I8,
            0xEA | 0xEE => VecElementType::I16,
            0x38 | 0x3C => VecElementType::I8,
            0x3A | 0x3E => VecElementType::I16,
            0x39 | 0x3B | 0x3D | 0x3F if qword => VecElementType::I64,
            0x39 | 0x3B | 0x3D | 0x3F => VecElementType::I32,
            _ => unreachable!(),
        };
        let min = matches!(opcode, 0x38..=0x3B | 0xDA | 0xEA);
        let signed = matches!(opcode, 0x38 | 0x39 | 0x3C | 0x3D | 0xEA | 0xEE);
        (elem, min, signed)
    }


    /// Select the elementwise signed/unsigned packed integer minimum or maximum.
    /// `VCmp` produces an all-ones lane mask, which makes the subsequent
    /// bit-select exact for every integer element width, including EVEX qwords.
    pub(crate) fn append_packed_minmax(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        min: bool,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let cond = match (min, signed) {
            (true, true) => VecCmpCond::Lt,
            (true, false) => VecCmpCond::Ltu,
            (false, true) => VecCmpCond::Gt,
            (false, false) => VecCmpCond::Gtu,
        };
        let select_src1 = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: select_src1,
                src1,
                src2,
                cond,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst,
                mask: select_src1,
                src_true: src1,
                src_false: src2,
                width,
            },
        ));
    }


    /// Find the unsigned minimum of eight packed words and return the minimum
    /// in bits 15:0 and its first (lowest) lane index in bits 18:16.  Scalar
    /// comparisons are used so the strict-less-than tie rule is explicit.  The
    /// temporary flag changes are bracketed by ReadFlags/WriteFlags because
    /// PHMINPOSUW does not architecturally modify flags.
    pub(crate) fn append_phminposuw(
        &self,
        dst: VReg,
        src: VReg,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let saved_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::ReadFlags { dst: saved_flags },
        ));

        let mut minimum = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: minimum,
                vec: src,
                lane: 0,
                elem: VecElementType::I16,
                sign: SignExtend::Zero,
            },
        ));
        let mut index = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: index,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));

        for lane in 1..8u8 {
            let candidate = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: candidate,
                    vec: src,
                    lane,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Cmp {
                    src1: candidate,
                    src2: SrcOperand::Reg(minimum),
                    width: OpWidth::W16,
                },
            ));
            let replace = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::SetCC {
                    dst: replace,
                    cond: Condition::Ult,
                    width: OpWidth::W64,
                },
            ));
            let next_minimum = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: next_minimum,
                    cond: replace,
                    src_true: candidate,
                    src_false: minimum,
                    width: OpWidth::W64,
                },
            ));
            let lane_index = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: lane_index,
                    src: SrcOperand::Imm(i64::from(lane)),
                    width: OpWidth::W64,
                },
            ));
            let next_index = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: next_index,
                    cond: replace,
                    src_true: lane_index,
                    src_false: index,
                    width: OpWidth::W64,
                },
            ));
            minimum = next_minimum;
            index = next_index;
        }

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::WriteFlags { src: saved_flags },
        ));
        let shifted_index = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Shl {
                dst: shifted_index,
                src: index,
                amount: SrcOperand::Imm(16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let packed = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Or {
                dst: packed,
                src1: minimum,
                src2: SrcOperand::Reg(shifted_index),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let zero = self.append_zero_vector(VecWidth::V128, VecElementType::I64, pc, ctx, ops);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInsertLane {
                dst,
                vec: zero,
                scalar: packed,
                lane: 0,
                elem: VecElementType::I64,
            },
        ));
    }


    /// Blend raw element bits according to the sign bit of each mask element.
    /// Floating-point blend forms deliberately use I32/I64 comparisons so NaN
    /// payloads and non-canonical bit patterns are copied without FP semantics.
    pub(crate) fn append_variable_blend(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        mask: VReg,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let select_src2 = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: select_src2,
                src1: mask,
                src2: zero,
                cond: VecCmpCond::Lt,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst,
                mask: select_src2,
                src_true: src2,
                src_false: src1,
                width,
            },
        ));
    }


    /// Blend raw integer lane bits under an immediate mask. Floating-point
    /// forms deliberately use I32/I64 lanes so NaN payloads are copied without
    /// invoking FP semantics. PBLENDW repeats imm8 for each 128-bit block.
    pub(crate) fn append_immediate_blend(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        imm: u8,
        repeat_128: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = (16 / elem.bytes()) as u8;
        let mut selected = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let bit = if repeat_128 { lane % block_lanes } else { lane };
            let source = if (imm >> bit) & 1 != 0 { src2 } else { src1 };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: source,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            selected.push(scalar);
        }
        let output = self.append_zero_vector(width, elem, pc, ctx, ops);
        for (lane, scalar) in selected.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar,
                    lane: lane as u8,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width,
            },
        ));
    }


    pub(crate) fn append_insert_scalar_lane(
        &self,
        dst: VReg,
        merge: VReg,
        scalar: VReg,
        elem: VecElementType,
        lane: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = VecWidth::V128.lanes(elem) as u8;
        let mut values = Vec::with_capacity(lanes as usize);
        for current in 0..lanes {
            if current == lane {
                values.push(scalar);
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: value,
                        vec: merge,
                        lane: current,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                values.push(value);
            }
        }
        let output = self.append_zero_vector(VecWidth::V128, elem, pc, ctx, ops);
        for (current, value) in values.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar: value,
                    lane: current as u8,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width: VecWidth::V128,
            },
        ));
    }


    pub(crate) fn append_insertps(
        &self,
        dst: VReg,
        merge: VReg,
        inserted: VReg,
        destination_lane: u8,
        zero_mask: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        let mut values = Vec::with_capacity(4);
        for lane in 0..4u8 {
            if (zero_mask >> lane) & 1 != 0 {
                values.push(zero);
            } else if lane == destination_lane {
                values.push(inserted);
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: value,
                        vec: merge,
                        lane,
                        elem: VecElementType::I32,
                        sign: SignExtend::Zero,
                    },
                ));
                values.push(value);
            }
        }
        let output = self.append_zero_vector(VecWidth::V128, VecElementType::I32, pc, ctx, ops);
        for (lane, value) in values.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar: value,
                    lane: lane as u8,
                    elem: VecElementType::I32,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width: VecWidth::V128,
            },
        ));
    }


    /// Multiply the even dword lanes into full qword products.
    /// Every input is extracted before the output vector is initialized, which
    /// makes all architectural source/destination alias combinations safe.
    pub(crate) fn append_pmuldq(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(VecElementType::I64) as u8;
        let mut products = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let a = ctx.alloc_vreg();
            let b = ctx.alloc_vreg();
            let product = ctx.alloc_vreg();
            let source_lane = lane * 2;
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: a,
                    vec: src1,
                    lane: source_lane,
                    elem: VecElementType::I32,
                    sign: if signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: b,
                    vec: src2,
                    lane: source_lane,
                    elem: VecElementType::I32,
                    sign: if signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                if signed {
                    OpKind::MulS {
                        dst_lo: product,
                        dst_hi: None,
                        src1: a,
                        src2: SrcOperand::Reg(b),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    }
                } else {
                    OpKind::MulU {
                        dst_lo: product,
                        dst_hi: None,
                        src1: a,
                        src2: SrcOperand::Reg(b),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    }
                },
            ));
            products.push(product);
        }
        let output = self.append_zero_vector(width, VecElementType::I64, pc, ctx, ops);
        for (lane, product) in products.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar: product,
                    lane: lane as u8,
                    elem: VecElementType::I64,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width,
            },
        ));
    }


    pub(crate) fn pmul_high_word_kind(
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        signed: bool,
    ) -> OpKind {
        OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem: VecElementType::I16,
            lanes: width.lanes(VecElementType::I16) as u8,
            signed1: signed,
            signed2: signed,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        }
    }


    pub(crate) fn packed_unsigned_average_kind(
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        elem: VecElementType,
    ) -> OpKind {
        OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes: width.lanes(elem) as u8,
            op: VLaneOp::AvgRnd,
            signed: false,
            set_ovf: false,
        }
    }


    pub(crate) fn pmaddwd_kind(dst: VReg, src1: VReg, src2: VReg, width: VecWidth) -> OpKind {
        // The zero accumulator selects the non-accumulating PMADDWD semantic
        // subset of VDotProduct. Narrowing to I32 performs the instruction's
        // unique 0x8000*0x8000 + 0x8000*0x8000 wrap to 0x8000_0000.
        OpKind::VDotProduct {
            dst,
            acc: VReg::Imm(0),
            src1,
            src2,
            mask: None,
            src_elem: VecElementType::I16,
            acc_elem: VecElementType::I32,
            width,
            src1_unsigned: false,
            saturate: false,
            zeroing: false,
        }
    }


    /// Carry-less multiply one selected qword pair in every independent
    /// 128-bit block. imm8[0] selects src1's qword and imm8[4] selects src2's;
    /// all other immediate bits are architecturally ignored.
    pub(crate) fn append_pclmulqdq(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        imm: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let blocks = (width.bytes() / 16) as u8;
        let select1 = imm & 1;
        let select2 = (imm >> 4) & 1;
        let mut products = Vec::with_capacity(blocks as usize);
        for block in 0..blocks {
            let lhs = ctx.alloc_vreg();
            let rhs = ctx.alloc_vreg();
            let lo = ctx.alloc_vreg();
            let hi = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: lhs,
                    vec: src1,
                    lane: block * 2 + select1,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: rhs,
                    vec: src2,
                    lane: block * 2 + select2,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::ClMul {
                    dst: lo,
                    dst_hi: Some(hi),
                    src1: SrcOperand::Reg(lhs),
                    src2: SrcOperand::Reg(rhs),
                    elem_bits: 64,
                    lanes: 1,
                    acc: false,
                },
            ));
            products.push((lo, hi));
        }

        let output = self.append_zero_vector(width, VecElementType::I64, pc, ctx, ops);
        for (block, (lo, hi)) in products.into_iter().enumerate() {
            for (offset, scalar) in [(0u8, lo), (1, hi)] {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: output,
                        vec: output,
                        scalar,
                        lane: block as u8 * 2 + offset,
                        elem: VecElementType::I64,
                    },
                ));
            }
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width,
            },
        ));
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


    /// Build PALIGNR's `(high || low)` concatenations and extract one result
    /// from each architectural block (8 bytes for MMX, 16 bytes otherwise).
    pub(crate) fn append_align_right(
        &self,
        dst: VReg,
        high: VReg,
        low: VReg,
        width: VecWidth,
        offset: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(VecElementType::I8) as u8;
        let block_lanes = if width == VecWidth::V64 { 8 } else { 16 };
        let indices = self.append_zero_vector(width, VecElementType::I8, pc, ctx, ops);
        for lane in 0..lanes {
            let block_base = lane / block_lanes * block_lanes;
            let in_block = lane % block_lanes;
            let concatenated = u16::from(offset) + u16::from(in_block);
            let block_lanes = u16::from(block_lanes);
            let selector = if concatenated < block_lanes {
                u16::from(block_base) + concatenated
            } else if concatenated < block_lanes * 2 {
                u16::from(lanes) + u16::from(block_base) + concatenated - block_lanes
            } else {
                u16::from(lanes) * 2
            };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem: VecElementType::I8,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1: low,
                src2: Some(high),
                indices,
                elem: VecElementType::I8,
                lanes,
            },
        ));
    }


    pub(crate) fn packed_extend_shape(opcode: u8) -> (VecElementType, VecElementType, bool) {
        let signed = opcode < 0x30;
        let (src, dst) = match opcode & 0x0F {
            0x00 => (VecElementType::I8, VecElementType::I16),
            0x01 => (VecElementType::I8, VecElementType::I32),
            0x02 => (VecElementType::I8, VecElementType::I64),
            0x03 => (VecElementType::I16, VecElementType::I32),
            0x04 => (VecElementType::I16, VecElementType::I64),
            0x05 => (VecElementType::I32, VecElementType::I64),
            _ => unreachable!(),
        };
        (src, dst, signed)
    }


    pub(crate) fn packed_extend_source_width(source_bytes: u32) -> VecWidth {
        match source_bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => unreachable!(),
        }
    }


    pub(crate) fn append_packed_extend(
        &self,
        dst: VReg,
        src: VReg,
        src_elem: VecElementType,
        dst_elem: VecElementType,
        lanes: u8,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        // Extract every source lane before initializing `dst`, preserving exact
        // behavior when the architectural source and destination alias.
        let mut scalars = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: src,
                    lane,
                    elem: src_elem,
                    sign: if signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                },
            ));
            scalars.push((lane, scalar));
        }
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst,
                scalar: zero,
                elem: dst_elem,
                lanes,
            },
        ));
        for (lane, scalar) in scalars {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem: dst_elem,
                },
            ));
        }
    }


    pub(crate) fn append_packed_extend_memory_source(
        &self,
        addr: Address,
        src_elem: VecElementType,
        lanes: u8,
        mask: Option<VReg>,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let source_bytes = u32::from(lanes) * src_elem.bytes();
        let source_width = Self::packed_extend_source_width(source_bytes);
        let source = self.append_zero_vector(source_width, src_elem, pc, ctx, ops);
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = match src_elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 => MemWidth::B4,
            _ => unreachable!(),
        };
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            let lane_addr = Address::base_off(base, i64::from(lane) * i64::from(src_elem.bytes()));
            if let Some(mask) = mask {
                let shifted = ctx.alloc_vreg();
                let active = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: shifted,
                        src: mask,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: active,
                        src1: shifted,
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: scalar,
                        cond: active,
                        addr: lane_addr,
                        width: mem_width,
                        signed: SignExtend::Zero,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr: lane_addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: source,
                    vec: source,
                    scalar,
                    lane,
                    elem: src_elem,
                },
            ));
        }
        source
    }


    /// Construct a VEX scalar XMM result: lane 0 comes from `low_scalar`, the
    /// remaining 128-bit lanes come from `upper_src`, and all state above bit
    /// 127 is cleared. Extract upper lanes before clearing `dst` so aliases are
    /// exact (for example, `vaddss xmm0,xmm0,xmm1`).
    pub(crate) fn append_vex_scalar_result(
        &self,
        dst: VReg,
        upper_src: VReg,
        low_scalar: VReg,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let xmm_lanes = VecWidth::V128.lanes(elem) as u8;
        let mut upper = Vec::with_capacity((xmm_lanes - 1) as usize);
        for lane in 1..xmm_lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: upper_src,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            upper.push((lane, scalar));
        }

        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst,
                scalar: zero,
                elem,
                lanes: 1,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInsertLane {
                dst,
                vec: dst,
                scalar: low_scalar,
                lane: 0,
                elem,
            },
        ));
        for (lane, scalar) in upper {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
    }


    /// Construct an XMM result whose low 32- or 64-bit lane is `scalar` and
    /// whose remaining bits through bit 127 are zero. Legacy encodings retain
    /// the shared architectural backing state above bit 127; VEX/EVEX
    /// encodings clear it.
    pub(crate) fn append_scalar_zeroed_xmm_result(
        &self,
        dst: VReg,
        scalar: VReg,
        elem: VecElementType,
        zero_upper: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));

        let result = if zero_upper { dst } else { ctx.alloc_vreg() };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: result,
                scalar: zero,
                elem,
                lanes: 1,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInsertLane {
                dst: result,
                vec: result,
                scalar,
                lane: 0,
                elem,
            },
        ));

        if !zero_upper {
            self.append_legacy_packed_result(dst, result, elem, pc, ctx, ops);
        }
    }


    /// Emit the exact integer sign-bit mask used by the (V)MOVMSK families.
    pub(crate) fn append_sse_movmask(
        &self,
        dst: VReg,
        src: VReg,
        elem: VecElementType,
        lanes: u8,
        dst_width: OpWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let accumulated = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: accumulated,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            let sign = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: src,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: sign,
                    src: scalar,
                    amount: SrcOperand::Imm(i64::from(elem.bytes() * 8 - 1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            let positioned = if lane == 0 {
                sign
            } else {
                let shifted = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shl {
                        dst: shifted,
                        src: sign,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                shifted
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: accumulated,
                    src1: accumulated,
                    src2: SrcOperand::Reg(positioned),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(accumulated),
                width: dst_width,
            },
        ));
    }


    pub(crate) fn lift_vec_pmovmskb(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        _ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let ops = vec![SmirOp::with_hint(
            OpId(0),
            pc,
            OpKind::X86MovMask {
                dst: self.gpr(modrm.reg),
                src: self.vec_reg(modrm.rm, prefix.width),
                elem: VecElementType::I8,
                lanes: prefix.width.lanes(VecElementType::I8) as u8,
                dst_width: OpWidth::W32,
            },
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0xD7,
                width: prefix.width,
                w: prefix.w,
            },
        )];
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn append_unpack_shuffle(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        high: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = (16 / elem.bytes()) as u8;
        let half = block_lanes / 2;
        let zero = ctx.alloc_vreg();
        let indices = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: indices,
                scalar: zero,
                elem,
                lanes,
            },
        ));
        for output in 0..lanes {
            let within_block = output % block_lanes;
            let block_base = output - within_block;
            let source_lane = block_base + if high { half } else { 0 } + within_block / 2;
            let selector = if within_block & 1 == 0 {
                source_lane
            } else {
                lanes + source_lane
            };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane: output,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1,
                src2: Some(src2),
                indices,
                elem,
                lanes,
            },
        ));
    }


    pub(crate) fn append_integer_interleave(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        high: bool,
        hint: X86OpHint,
        pc: u64,
        ops: &mut Vec<SmirOp>,
    ) {
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInterleave {
                dst,
                src1,
                src2,
                elem,
                lanes: width.lanes(elem) as u8,
                block_lanes: (if width == VecWidth::V64 { 8 } else { 16 } / elem.bytes()) as u8,
                high,
            },
            hint,
        ));
    }


    pub(crate) fn append_fp_estimate_result(
        &self,
        dst: VReg,
        merge: VReg,
        src: VReg,
        opcode: u8,
        scalar: bool,
        width: VecWidth,
        legacy: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let raw = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VUnary {
                dst: raw,
                src,
                elem: VecElementType::F32,
                lanes: if scalar {
                    1
                } else {
                    width.lanes(VecElementType::F32) as u8
                },
                op: if opcode == 0x53 {
                    VecUnaryOp::FRecipEstimate
                } else {
                    VecUnaryOp::FRsqrtEstimate
                },
            },
        ));
        if scalar {
            let low = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: low,
                    vec: raw,
                    lane: 0,
                    elem: VecElementType::F32,
                    sign: SignExtend::Zero,
                },
            ));
            if legacy {
                let result = ctx.alloc_vreg();
                self.append_vex_scalar_result(
                    result,
                    merge,
                    low,
                    VecElementType::F32,
                    pc,
                    ctx,
                    ops,
                );
                self.append_legacy_packed_result(dst, result, VecElementType::F32, pc, ctx, ops);
            } else {
                self.append_vex_scalar_result(dst, merge, low, VecElementType::F32, pc, ctx, ops);
            }
        } else if legacy {
            self.append_legacy_packed_result(dst, raw, VecElementType::F32, pc, ctx, ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: raw,
                    width,
                },
            ));
        }
    }


    pub(crate) fn lift_vec_fp_estimate(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let scalar = prefix.pp == X86SsePrefix::Rep;
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::Rep)
            || (!scalar && prefix.vvvv != 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let width = if scalar { VecWidth::V128 } else { prefix.width };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            if scalar {
                let value = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: value,
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar: value,
                        elem: VecElementType::F32,
                        lanes: 1,
                    },
                ));
                vector
            } else {
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: vector,
                        addr,
                        width,
                    },
                ));
                vector
            }
        } else {
            self.vec_reg(modrm.rm, width)
        };
        self.append_fp_estimate_result(
            self.vec_reg(modrm.reg, width),
            self.xmm(prefix.vvvv),
            src,
            opcode,
            scalar,
            width,
            false,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn append_fp_addsub_horizontal(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        opcode: u8,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let per_128 = 16 / elem.bytes() as u8;
        let result = self.append_zero_vector(width, elem, pc, ctx, ops);
        for lane in 0..lanes {
            let (source, left_lane, right_lane, subtract) = if opcode == 0xD0 {
                (src1, lane, lane, lane & 1 == 0)
            } else {
                let group = lane / per_128;
                let position = lane % per_128;
                let pairs = per_128 / 2;
                let (source, pair) = if position < pairs {
                    (src1, position)
                } else {
                    (src2, position - pairs)
                };
                let left = group * per_128 + pair * 2;
                (source, left, left + 1, opcode == 0x7D)
            };
            let left = ctx.alloc_vreg();
            let right = ctx.alloc_vreg();
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: left,
                    vec: source,
                    lane: left_lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: right,
                    vec: if opcode == 0xD0 { src2 } else { source },
                    lane: right_lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                if subtract {
                    OpKind::FSub {
                        dst: scalar,
                        src1: left,
                        src2: right,
                        precision: if elem == VecElementType::F32 {
                            FpPrecision::F32
                        } else {
                            FpPrecision::F64
                        },
                    }
                } else {
                    OpKind::FAdd {
                        dst: scalar,
                        src1: left,
                        src2: right,
                        precision: if elem == VecElementType::F32 {
                            FpPrecision::F32
                        } else {
                            FpPrecision::F64
                        },
                    }
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: result,
                    vec: result,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: result,
                width,
            },
        ));
    }


    pub(crate) fn lift_vec_addsub_horizontal(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.map != X86VecMap::Map0F
            || !matches!(prefix.pp, X86SsePrefix::OpSize | X86SsePrefix::Repne)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if prefix.pp == X86SsePrefix::Repne {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
            ));
            loaded
        } else {
            self.vec_reg(modrm.rm, prefix.width)
        };
        self.append_fp_addsub_horizontal(
            self.vec_reg(modrm.reg, prefix.width),
            self.vec_reg(prefix.vvvv, prefix.width),
            src2,
            opcode,
            elem,
            prefix.width,
            pc,
            ctx,
            &mut ops,
        );
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_half_move(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.l_bits != 0
            || !matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize)
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.w != (prefix.pp == X86SsePrefix::OpSize)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let store = matches!(opcode, 0x13 | 0x17);
        if (store && !modrm.is_memory)
            || (!store && prefix.pp == X86SsePrefix::OpSize && !modrm.is_memory)
            || (store && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let lane = if matches!(opcode, 0x16 | 0x17) { 1 } else { 0 };
        let reg_ext = if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
            16
        } else {
            0
        };
        let mut ops = Vec::new();
        if store {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 8, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: self.xmm(modrm.reg + reg_ext),
                    lane,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: scalar,
                    addr,
                    width: MemWidth::B8,
                },
            ));
        } else {
            let dst = self.xmm(modrm.reg + reg_ext);
            let merge = self.xmm(
                prefix.vvvv
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                        16
                    } else {
                        0
                    },
            );
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                    self.vec_disp8_addr_to_smir(
                        prefix,
                        modrm.addr.as_ref().unwrap(),
                        next_pc,
                        8,
                        ctx,
                    )
                } else {
                    self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
                };
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B8,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                let source = self.xmm(
                    modrm.rm
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                            16
                        } else {
                            0
                        },
                );
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: if opcode == 0x12 { 1 } else { 0 },
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: merge,
                    width: VecWidth::V128,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem: VecElementType::I64,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_movnt(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let valid_shape = match opcode {
            0x2B => matches!(prefix.pp, X86SsePrefix::None | X86SsePrefix::OpSize),
            0xE7 => prefix.pp == X86SsePrefix::OpSize,
            _ => false,
        };
        let wrong_evex_w = prefix.encoding == VecEncodingKind::Evex
            && match opcode {
                0x2B => prefix.w != (prefix.pp == X86SsePrefix::OpSize),
                0xE7 => prefix.w,
                _ => false,
            };
        if !valid_shape
            || prefix.l_bits == 3
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || wrong_evex_w
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = if prefix.encoding == VecEncodingKind::Evex {
            self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
        } else {
            self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86CheckAlignment {
                addr: addr.clone(),
                alignment: prefix.width.bytes() as u8,
            },
        ));
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VStore {
                src: self.vec_reg(
                    modrm.reg
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                            16
                        } else {
                            0
                        },
                    prefix.width,
                ),
                addr,
                width: prefix.width,
            },
            X86OpHint::VecAlign(X86VecAlign::Aligned),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_vzero(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::None
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let mut ops = Vec::new();
        let zero = self.append_zero_vector(VecWidth::V512, VecElementType::I64, pc, ctx, &mut ops);
        for index in 0u8..16 {
            let dst = self.xmm(index);
            if prefix.width == VecWidth::V128 {
                let low0 = ctx.alloc_vreg();
                let low1 = ctx.alloc_vreg();
                let result = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: low0,
                        vec: dst,
                        lane: 0,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: low1,
                        vec: dst,
                        lane: 1,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: result,
                        vec: zero,
                        scalar: low0,
                        lane: 0,
                        elem: VecElementType::I64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: result,
                        vec: result,
                        scalar: low1,
                        lane: 1,
                        elem: VecElementType::I64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst,
                        src: result,
                        width: VecWidth::V512,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst,
                        src: zero,
                        width: VecWidth::V512,
                    },
                ));
            }
        }
        Ok(LiftResult::fallthrough(ops, prefix.bytes + 1))
    }


    pub(crate) fn lift_vec_permute_variable(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || (prefix.zeroing && prefix.aaa == 0)
            || prefix.l_bits == 3
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let permil = matches!(opcode, 0x0C | 0x0D);
        let elem = match opcode {
            0x0C => VecElementType::F32,
            0x0D => VecElementType::F64,
            0x16 if prefix.w => VecElementType::F64,
            0x16 => VecElementType::F32,
            0x36 if prefix.w => VecElementType::I64,
            0x36 => VecElementType::I32,
            0x8D if prefix.w => VecElementType::I16,
            0x8D => VecElementType::I8,
            _ => unreachable!(),
        };
        let valid = match (prefix.encoding, opcode) {
            (VecEncodingKind::Vex, 0x0C | 0x0D) => !prefix.w,
            (VecEncodingKind::Evex, 0x0C) => !prefix.w,
            (VecEncodingKind::Evex, 0x0D) => prefix.w,
            (VecEncodingKind::Vex, 0x16 | 0x36) => !prefix.w && prefix.width == VecWidth::V256,
            (VecEncodingKind::Evex, 0x16 | 0x36) => prefix.width != VecWidth::V128,
            (VecEncodingKind::Evex, 0x8D) => true,
            _ => false,
        };
        if !valid {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.b && (!modrm.is_memory || opcode == 0x8D) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b;
        let mut ops = Vec::new();
        let memory_addr = if modrm.is_memory {
            let scale = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                scale,
                ctx,
            );
            ops.extend(pre_ops);
            Some(addr)
        } else {
            None
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let vvvv = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let rm_reg = (!modrm.is_memory).then(|| {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        });
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));

        let controls = if permil {
            if let Some(addr) = memory_addr.clone() {
                if let Some(mask) = mask {
                    self.append_evex_masked_vector_source(
                        addr,
                        elem,
                        prefix.width,
                        broadcast,
                        mask,
                        pc,
                        ctx,
                        &mut ops,
                    )
                } else if broadcast {
                    self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
                } else {
                    let loaded = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: loaded,
                            addr,
                            width: prefix.width,
                        },
                    ));
                    loaded
                }
            } else {
                rm_reg.unwrap()
            }
        } else {
            vvvv
        };
        let indices = if permil {
            self.append_permil_variable_indices(
                controls,
                prefix.width,
                elem,
                if elem == VecElementType::F32 { 4 } else { 2 },
                if elem == VecElementType::F64 { 1 } else { 0 },
                pc,
                ctx,
                &mut ops,
            )
        } else {
            controls
        };

        let table = if permil {
            Some(vvvv)
        } else if let Some(addr) = memory_addr {
            if let Some(mask) = mask {
                let raw = self.append_masked_permute_memory_result(
                    addr,
                    indices,
                    prefix.width,
                    elem,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                );
                self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
                return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
            }
            if broadcast {
                Some(self.append_broadcast_memory_source(
                    addr,
                    elem,
                    prefix.width,
                    pc,
                    ctx,
                    &mut ops,
                ))
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                Some(loaded)
            }
        } else {
            rm_reg
        }
        .unwrap();

        let direct_vbmi =
            prefix.encoding == VecEncodingKind::Evex && opcode == 0x8D && !modrm.is_memory;
        let raw = if prefix.encoding == VecEncodingKind::Evex && !direct_vbmi {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            if direct_vbmi {
                OpKind::X86PermuteBytesWords {
                    dst: raw,
                    table1: table,
                    table2: None,
                    indices,
                    mask,
                    elem,
                    width: prefix.width,
                    overwrite_table: false,
                    zeroing: prefix.zeroing,
                }
            } else {
                OpKind::VPermute {
                    dst: raw,
                    src1: table,
                    src2: None,
                    indices,
                    elem,
                    width: prefix.width,
                    overwrite_table: false,
                }
            },
        ));
        if prefix.encoding == VecEncodingKind::Evex && !direct_vbmi {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_permute_immediate(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || prefix.v_high
            || (prefix.zeroing && prefix.aaa == 0)
            || prefix.l_bits == 3
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x04 => VecElementType::F32,
            0x05 | 0x01 => VecElementType::F64,
            0x00 => VecElementType::I64,
            _ => unreachable!(),
        };
        let permil = matches!(opcode, 0x04 | 0x05);
        let valid = if permil {
            match prefix.encoding {
                VecEncodingKind::Vex => !prefix.w,
                VecEncodingKind::Evex => prefix.w == (elem == VecElementType::F64),
            }
        } else {
            prefix.w && prefix.width != VecWidth::V128
        };
        if !valid {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.b && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&imm) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };
        let next_pc = pc + imm_offset as u64 + 1;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b;
        let mut ops = Vec::new();
        let indices = self.append_permute_immediate_indices(
            prefix.width,
            elem,
            imm,
            if elem == VecElementType::F32 {
                4
            } else if permil {
                2
            } else {
                4
            },
            if elem == VecElementType::F64 && permil {
                8
            } else {
                4
            },
            if elem == VecElementType::F64 && permil {
                1
            } else {
                2
            },
            pc,
            ctx,
            &mut ops,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let table = if modrm.is_memory {
            let scale = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                scale,
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                let raw = self.append_masked_permute_memory_result(
                    addr,
                    indices,
                    prefix.width,
                    elem,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                );
                self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
                return Ok(LiftResult::fallthrough(ops, imm_offset + 1));
            }
            if broadcast {
                self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPermute {
                dst: raw,
                src1: table,
                src2: None,
                indices,
                elem,
                width: prefix.width,
                overwrite_table: false,
            },
        ));
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_vpmadd52(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || !prefix.w
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.b && (prefix.encoding != VecEncodingKind::Evex || !modrm.is_memory) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b { 8 } else { prefix.width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I64,
                    prefix.width,
                    prefix.b,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if prefix.b {
                self.append_broadcast_memory_source(
                    addr,
                    VecElementType::I64,
                    prefix.width,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMultiplyAdd52 {
                dst,
                acc: dst,
                src1,
                src2,
                mask,
                width: prefix.width,
                high: opcode == 0xB5,
                zeroing: prefix.encoding == VecEncodingKind::Evex && prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_vnni_dot(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.b && (prefix.encoding != VecEncodingKind::Evex || !modrm.is_memory) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if prefix.b { 4 } else { prefix.width.bytes() },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I32,
                    prefix.width,
                    prefix.b,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if prefix.b {
                self.append_broadcast_memory_source(
                    addr,
                    VecElementType::I32,
                    prefix.width,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VDotProduct {
                dst,
                acc: dst,
                src1,
                src2,
                mask,
                src_elem: if opcode < 0x52 {
                    VecElementType::I8
                } else {
                    VecElementType::I16
                },
                acc_elem: VecElementType::I32,
                width: prefix.width,
                src1_unsigned: opcode < 0x52,
                saturate: opcode & 1 != 0,
                zeroing: prefix.encoding == VecEncodingKind::Evex && prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_fp_unpack(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match prefix.pp {
            X86SsePrefix::None => VecElementType::F32,
            X86SsePrefix::OpSize => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex
                && ((elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: elem == VecElementType::F64,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let lanes = prefix.width.lanes(elem) as u8;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let scale = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    scale,
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::F32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let raw = ctx.alloc_vreg();
        self.append_unpack_shuffle(
            raw,
            self.vec_reg(
                prefix.vvvv
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            ),
            src2,
            elem,
            prefix.width,
            opcode == 0x15,
            pc,
            ctx,
            &mut ops,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: raw,
                    width: prefix.width,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_fp_compare(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let fp16 = prefix.map == X86VecMap::Map0F3A;
        let (elem, scalar) = match (fp16, prefix.pp) {
            (true, X86SsePrefix::None) => (VecElementType::F16, false),
            (true, X86SsePrefix::Rep) => (VecElementType::F16, true),
            (true, _) => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            (false, X86SsePrefix::None) => (VecElementType::F32, false),
            (false, X86SsePrefix::OpSize) => (VecElementType::F64, false),
            (false, X86SsePrefix::Rep) => (VecElementType::F32, true),
            (false, X86SsePrefix::Repne) => (VecElementType::F64, true),
        };
        if (fp16 && (prefix.encoding != VecEncodingKind::Evex || prefix.w))
            || (!scalar && prefix.l_bits == 3)
            || (prefix.encoding == VecEncodingKind::Evex
                && ((elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: prefix.pp == X86SsePrefix::OpSize,
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let predicate = bytes[imm_offset];
        if predicate & !0x1F != 0
            || (prefix.encoding == VecEncodingKind::Evex && (modrm.reg >= 8 || prefix.reg_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..=imm_offset].to_vec(),
            });
        }
        let packed_sae =
            prefix.encoding == VecEncodingKind::Evex && !scalar && prefix.b && !modrm.is_memory;
        if prefix.encoding == VecEncodingKind::Evex
            && !scalar
            && prefix.b
            && !modrm.is_memory
            && prefix.l_bits != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..=imm_offset].to_vec(),
            });
        }
        if prefix.encoding == VecEncodingKind::Evex && scalar && prefix.b && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..=imm_offset].to_vec(),
            });
        }
        let width = if packed_sae {
            VecWidth::V512
        } else if scalar {
            VecWidth::V128
        } else {
            prefix.width
        };
        let lanes = if scalar { 1 } else { width.lanes(elem) as u8 };
        let next_pc = pc + imm_offset as u64 + 1;
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast =
            prefix.encoding == VecEncodingKind::Evex && !scalar && prefix.b && modrm.is_memory;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let scale = if scalar || broadcast {
                elem.bytes()
            } else {
                width.bytes()
            };
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    scale,
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if scalar {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: value,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                if let Some(mask_reg) = mask {
                    let active = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: active,
                            src1: mask_reg,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: value,
                            cond: active,
                            addr,
                            width: match elem {
                                VecElementType::F16 => MemWidth::B2,
                                VecElementType::F32 => MemWidth::B4,
                                _ => MemWidth::B8,
                            },
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: value,
                            addr,
                            width: match elem {
                                VecElementType::F16 => MemWidth::B2,
                                VecElementType::F32 => MemWidth::B4,
                                _ => MemWidth::B8,
                            },
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar: value,
                        elem,
                        lanes: 1,
                    },
                ));
                loaded
            } else if let Some(mask_reg) = mask {
                self.append_evex_masked_vector_source(
                    addr, elem, width, broadcast, mask_reg, pc, ctx, &mut ops,
                )
            } else if broadcast {
                let value = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: value,
                        addr,
                        width: match elem {
                            VecElementType::F16 => MemWidth::B2,
                            VecElementType::F32 => MemWidth::B4,
                            _ => MemWidth::B8,
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar: value,
                        elem,
                        lanes,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            width,
        );
        let mask_destination = prefix.encoding == VecEncodingKind::Evex;
        let dst = if mask_destination {
            VReg::Arch(ArchReg::X86(X86Reg::K(modrm.reg)))
        } else {
            self.vec_reg(modrm.reg, width)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86VectorFpCompare {
                dst,
                src1,
                src2,
                mask,
                elem,
                width,
                lanes,
                predicate,
                scalar,
                mask_destination,
                zero_upper: !mask_destination,
                suppress_exceptions: prefix.encoding == VecEncodingKind::Evex
                    && (scalar && prefix.b || packed_sae),
            },
            self.vec_hint(prefix, 0xC2),
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_gather(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.b
            || prefix.zeroing
            || prefix.l_bits == 3
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if prefix.encoding == VecEncodingKind::Evex && (prefix.aaa == 0 || prefix.vvvv != 0) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if !modrm.is_memory || modrm.byte & 7 != 4 || bytes.len() <= cursor + 1 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let sib = bytes[cursor + 1];
        let index_number = ((sib >> 3) & 7)
            | modrm_prefix.rex_x()
            | if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                16
            } else {
                0
            };
        let dst_number = modrm.reg
            + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                16
            } else {
                0
            };
        if dst_number == index_number
            || (prefix.encoding == VecEncodingKind::Vex
                && (prefix.vvvv == dst_number || prefix.vvvv == index_number))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let data_elem = if prefix.w {
            VecElementType::I64
        } else {
            VecElementType::I32
        };
        let index_elem = if opcode & 1 == 0 {
            VecElementType::I32
        } else {
            VecElementType::I64
        };
        let lanes = prefix
            .width
            .lanes(data_elem)
            .min(prefix.width.lanes(index_elem)) as u8;
        let result_bits = usize::from(lanes) * data_elem.bytes() as usize * 8;
        let result_width = match result_bits {
            64 => VecWidth::V64,
            128 => VecWidth::V128,
            256 => VecWidth::V256,
            512 => VecWidth::V512,
            _ => unreachable!("invalid gather result width"),
        };
        let index_bits = usize::from(lanes) * index_elem.bytes() as usize * 8;
        let index_width = match index_bits {
            64 => VecWidth::V64,
            128 => VecWidth::V128,
            256 => VecWidth::V256,
            512 => VecWidth::V512,
            _ => unreachable!("invalid gather index width"),
        };
        let dst = self.vec_reg(dst_number, result_width);
        let index = self.vec_reg(index_number, index_width);
        let old_dst = ctx.alloc_vreg();
        let mut ops = vec![SmirOp::new(
            OpId(0),
            pc,
            OpKind::VMov {
                dst: old_dst,
                src: dst,
                width: result_width,
            },
        )];

        // Normalize destination width before the first potentially faulting
        // access. Intel permits unused high portions to be cleared even when
        // the instruction suspends before gathering its first element.
        let initial_dst = self.append_zero_vector(result_width, data_elem, pc, ctx, &mut ops);
        for lane in 0..lanes {
            let old = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: old,
                    vec: old_dst,
                    lane,
                    elem: data_elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: initial_dst,
                    vec: initial_dst,
                    scalar: old,
                    lane,
                    elem: data_elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: initial_dst,
                width: result_width,
            },
        ));

        let scalar_zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: scalar_zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        let mut conditions = Vec::with_capacity(lanes as usize);
        let vector_mask = if prefix.encoding == VecEncodingKind::Vex {
            let mask = self.vec_reg(prefix.vvvv, result_width);
            let old_mask = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst: old_mask,
                    src: mask,
                    width: result_width,
                },
            ));
            let normalized = self.append_zero_vector(result_width, data_elem, pc, ctx, &mut ops);
            for lane in 0..lanes {
                let raw = ctx.alloc_vreg();
                let cond = ctx.alloc_vreg();
                let sign_mask = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: raw,
                        vec: old_mask,
                        lane,
                        elem: data_elem,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: cond,
                        src: raw,
                        amount: SrcOperand::Imm(i64::from(data_elem.bytes() * 8 - 1)),
                        width: if data_elem == VecElementType::I32 {
                            OpWidth::W32
                        } else {
                            OpWidth::W64
                        },
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Sub {
                        dst: sign_mask,
                        src1: scalar_zero,
                        src2: SrcOperand::Reg(cond),
                        width: if data_elem == VecElementType::I32 {
                            OpWidth::W32
                        } else {
                            OpWidth::W64
                        },
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: normalized,
                        vec: normalized,
                        scalar: sign_mask,
                        lane,
                        elem: data_elem,
                    },
                ));
                conditions.push(cond);
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst: mask,
                    src: normalized,
                    width: result_width,
                },
            ));
            Some(mask)
        } else {
            let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
            let snapshot = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: snapshot,
                    src: SrcOperand::Reg(mask),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: mask,
                    src1: mask,
                    src2: SrcOperand::Imm(((1u64 << lanes) - 1) as i64),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            for lane in 0..lanes {
                let shifted = ctx.alloc_vreg();
                let cond = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: shifted,
                        src: snapshot,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: cond,
                        src1: shifted,
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                conditions.push(cond);
            }
            None
        };

        let mut x86_addr = modrm.addr.unwrap();
        x86_addr.index = None;
        if prefix.encoding == VecEncodingKind::Evex && x86_addr.disp_size == DispSize::Disp8 {
            x86_addr.disp *= i64::from(data_elem.bytes());
        }
        let mem_width = if data_elem == VecElementType::I32 {
            MemWidth::B4
        } else {
            MemWidth::B8
        };
        for (lane, cond) in conditions.into_iter().enumerate() {
            let lane = lane as u8;
            let value = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: value,
                    vec: dst,
                    lane,
                    elem: data_elem,
                    sign: SignExtend::Zero,
                },
            ));
            let addr = self
                .append_vsib_lane_address(&x86_addr, index, lane, index_elem, pc, ctx, &mut ops);
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::PredLoad {
                    dst: value,
                    cond,
                    addr,
                    width: mem_width,
                    signed: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar: value,
                    lane,
                    elem: data_elem,
                },
            ));
            if let Some(mask) = vector_mask {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: mask,
                        vec: mask,
                        scalar: scalar_zero,
                        lane,
                        elem: data_elem,
                    },
                ));
            } else {
                let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: mask,
                        src1: mask,
                        src2: SrcOperand::Imm(!(1i64 << lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
        }

        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_fma3(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let fp16 = prefix.encoding == VecEncodingKind::Evex
            && prefix.map == X86VecMap::Map6
            && prefix.pp == X86SsePrefix::OpSize
            && !prefix.w;
        if !fp16 && (prefix.map != X86VecMap::Map0F38 || prefix.pp != X86SsePrefix::OpSize) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let low = opcode & 0x0F;
        let alternating = matches!(low, 0x06 | 0x07);
        let scalar = matches!(low, 0x09 | 0x0B | 0x0D | 0x0F);
        if prefix.encoding == VecEncodingKind::Evex && (prefix.zeroing && prefix.aaa == 0) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if fp16 {
            VecElementType::F16
        } else if prefix.w {
            VecElementType::F64
        } else {
            VecElementType::F32
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let embedded_rounding =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        if (fp16 && scalar && prefix.b && modrm.is_memory)
            || (!scalar && !embedded_rounding && prefix.l_bits == 3)
            || (!fp16 && embedded_rounding && !scalar && prefix.width != VecWidth::V512)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let operation_width = if scalar {
            VecWidth::V128
        } else if fp16 && embedded_rounding {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = if scalar {
            1
        } else {
            operation_width.lanes(elem) as u8
        };
        let round = if fp16 && embedded_rounding {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            operation_width,
        );
        let old_dst = dst;
        let vex_src = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            operation_width,
        );
        let mask_cond = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);
        let rm_src = if modrm.is_memory {
            let (addr, pre_ops) = if scalar {
                self.vec_scalar_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    elem,
                    ctx,
                )
            } else if prefix.encoding == VecEncodingKind::Evex && prefix.b {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    elem.bytes(),
                    ctx,
                )
            } else {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if scalar {
                let scalar_value = ctx.alloc_vreg();
                let mem_width = match elem {
                    VecElementType::F16 => MemWidth::B2,
                    VecElementType::F32 => MemWidth::B4,
                    VecElementType::F64 => MemWidth::B8,
                    _ => unreachable!(),
                };
                if let Some(cond) = mask_cond {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar_value,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar_value,
                            cond,
                            addr,
                            width: mem_width,
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar_value,
                            addr,
                            width: mem_width,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar: scalar_value,
                        elem,
                        lanes: 1,
                    },
                ));
                vector
            } else if prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0 {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    operation_width,
                    prefix.b,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if prefix.encoding == VecEncodingKind::Evex && prefix.b {
                let scalar_value = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar_value,
                        addr,
                        width: match elem {
                            VecElementType::F16 => MemWidth::B2,
                            VecElementType::F32 => MemWidth::B4,
                            VecElementType::F64 => MemWidth::B8,
                            _ => unreachable!(),
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar: scalar_value,
                        elem,
                        lanes,
                    },
                ));
                vector
            } else {
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: vector,
                        addr,
                        width: operation_width,
                    },
                ));
                vector
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                operation_width,
            )
        };

        let order = opcode >> 4;
        let (mul1, mul2, acc) = match order {
            0x09 => (old_dst, rm_src, vex_src),
            0x0A => (vex_src, old_dst, rm_src),
            0x0B => (vex_src, rm_src, old_dst),
            _ => unreachable!(),
        };
        let negate_product = matches!(low, 0x0C | 0x0D | 0x0E | 0x0F);
        let negate_acc = matches!(low, 0x0A | 0x0B | 0x0E | 0x0F);
        let raw = ctx.alloc_vreg();
        if fp16 {
            let kind = match low {
                0x06 => X86FmaKind::AddSub,
                0x07 => X86FmaKind::SubAdd,
                0x08 | 0x09 => X86FmaKind::Add,
                0x0A | 0x0B => X86FmaKind::Sub,
                0x0C | 0x0D => X86FmaKind::NegativeMultiplyAdd,
                0x0E | 0x0F => X86FmaKind::NegativeMultiplySub,
                _ => unreachable!(),
            };
            let order = match order {
                0x09 => X86FmaOrder::Order132,
                0x0A => X86FmaOrder::Order213,
                0x0B => X86FmaOrder::Order231,
                _ => unreachable!(),
            };
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86FP16Fma {
                    dst: raw,
                    src1: old_dst,
                    src2: vex_src,
                    src3: rm_src,
                    mask: (prefix.aaa != 0)
                        .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                    kind,
                    order,
                    round,
                    lanes,
                },
                self.vec_hint(prefix, opcode),
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VFma {
                    dst: raw,
                    src1: mul1,
                    src2: mul2,
                    acc,
                    elem,
                    lanes,
                    negate_product,
                    negate_acc: if alternating { false } else { negate_acc },
                },
            ));
        }

        let result = if alternating && !fp16 {
            let sub = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VFma {
                    dst: sub,
                    src1: mul1,
                    src2: mul2,
                    acc,
                    elem,
                    lanes,
                    negate_product: false,
                    negate_acc: true,
                },
            ));
            let selected = self.append_zero_vector(operation_width, elem, pc, ctx, &mut ops);
            let subtract_even = low == 0x06;
            for lane in 0..lanes {
                let scalar_value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar_value,
                        vec: if (lane & 1 == 0) == subtract_even {
                            sub
                        } else {
                            raw
                        },
                        lane,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: selected,
                        vec: selected,
                        scalar: scalar_value,
                        lane,
                        elem,
                    },
                ));
            }
            selected
        } else {
            raw
        };

        if scalar {
            let low_result = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: low_result,
                    vec: result,
                    lane: 0,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            let low_result = self.append_evex_scalar_select(
                prefix, mask_cond, dst, low_result, elem, pc, ctx, &mut ops,
            );
            self.append_vex_scalar_result(dst, old_dst, low_result, elem, pc, ctx, &mut ops);
        } else {
            if prefix.encoding == VecEncodingKind::Evex {
                self.append_evex_vector_mask_result_width(
                    prefix,
                    dst,
                    result,
                    elem,
                    operation_width,
                    pc,
                    ctx,
                    &mut ops,
                );
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst,
                        src: result,
                        width: operation_width,
                    },
                ));
            }
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_extend(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex && prefix.v_high)
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
            || (prefix.encoding == VecEncodingKind::Evex
                && matches!(opcode, 0x25 | 0x35)
                && prefix.w)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (src_elem, dst_elem, signed) = Self::packed_extend_shape(opcode);
        let lanes = prefix.width.lanes(dst_elem) as u8;
        let source_bytes = u32::from(lanes) * src_elem.bytes();
        let source_width = Self::packed_extend_source_width(source_bytes);
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = if prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0 {
            Some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))))
        } else {
            None
        };
        let src = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    source_bytes,
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            self.append_packed_extend_memory_source(addr, src_elem, lanes, mask, pc, ctx, &mut ops)
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                source_width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_packed_extend(
                raw, src, src_elem, dst_elem, lanes, signed, pc, ctx, &mut ops,
            );
            self.append_evex_vector_mask_result(prefix, dst, raw, dst_elem, pc, ctx, &mut ops);
        } else {
            self.append_packed_extend(
                dst, src, src_elem, dst_elem, lanes, signed, pc, ctx, &mut ops,
            );
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_minmax(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let qword = prefix.encoding == VecEncodingKind::Evex
            && prefix.w
            && matches!(opcode, 0x39 | 0x3B | 0x3D | 0x3F);
        let (elem, min, signed) = Self::packed_minmax_shape(opcode, qword);
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && modrm.is_memory
            && matches!(elem, VecElementType::I32 | VecElementType::I64);
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast {
                        elem.bytes()
                    } else {
                        prefix.width.bytes()
                    },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::I32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && (prefix.encoding == VecEncodingKind::Vex || prefix.aaa == 0) {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1,
                    src2,
                    elem,
                    lanes: prefix.width.lanes(elem) as u8,
                    op: if min { VLaneOp::Min } else { VLaneOp::Max },
                    signed,
                    set_ovf: false,
                },
                self.vec_hint(prefix, opcode),
            ));
        } else if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_packed_minmax(
                raw,
                src1,
                src2,
                elem,
                prefix.width,
                min,
                signed,
                pc,
                ctx,
                &mut ops,
            );
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            self.append_packed_minmax(
                dst,
                src1,
                src2,
                elem,
                prefix.width,
                min,
                signed,
                pc,
                ctx,
                &mut ops,
            );
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_pclmulqdq(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || prefix.aaa != 0
            || prefix.zeroing
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        self.append_pclmulqdq(dst, src1, src2, prefix.width, imm, pc, ctx, &mut ops);
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_mpsadbw(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let vex = prefix.encoding == VecEncodingKind::Vex
            && prefix.pp == X86SsePrefix::OpSize
            && matches!(prefix.width, VecWidth::V128 | VecWidth::V256);
        let evex_mpsadbw = prefix.encoding == VecEncodingKind::Evex
            && prefix.pp == X86SsePrefix::Rep
            && !prefix.w
            && prefix.l_bits != 3
            && matches!(
                prefix.width,
                VecWidth::V128 | VecWidth::V256 | VecWidth::V512
            )
            && !prefix.b
            && (!prefix.zeroing || prefix.aaa != 0);
        let vdbpsadbw = prefix.encoding == VecEncodingKind::Evex
            && prefix.pp == X86SsePrefix::OpSize
            && !prefix.w
            && prefix.l_bits != 3
            && matches!(
                prefix.width,
                VecWidth::V128 | VecWidth::V256 | VecWidth::V512
            )
            && !prefix.b
            && (!prefix.zeroing || prefix.aaa != 0);
        if !vex && !evex_mpsadbw && !vdbpsadbw {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let evex = evex_mpsadbw || vdbpsadbw;
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    prefix.width.bytes(),
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm + if evex && prefix.rm_high { 16 } else { 0 },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg + if evex && prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        if vdbpsadbw {
            let raw = self.append_vdbpsadbw(
                src1,
                src2,
                prefix.width,
                bytes[imm_offset],
                pc,
                ctx,
                &mut ops,
            );
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I16,
                pc,
                ctx,
                &mut ops,
            );
            return Ok(LiftResult::fallthrough(ops, imm_offset + 1));
        }

        let kind = OpKind::VMpsadbw {
            dst,
            src1,
            src2,
            mask: (evex && prefix.aaa != 0)
                .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
            width: prefix.width,
            imm: bytes[imm_offset],
            zeroing: evex && prefix.zeroing,
        };
        if modrm.is_memory {
            // The register-only native admission deliberately leaves memory
            // loads in the established fault-atomic fallback path.
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        } else {
            let hint = if evex {
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::Rep,
                    opcode: 0x42,
                    width: prefix.width,
                    w: false,
                }
            } else {
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: prefix.width,
                    w: prefix.w,
                }
            };
            ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_psadbw(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let kind = OpKind::VSadBytes {
            dst,
            src1,
            src2,
            width: prefix.width,
        };
        if modrm.is_memory {
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                self.vec_hint(prefix, 0xF6),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_aes_round(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let unary = opcode == 0xDB;
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (unary
                && (prefix.encoding != VecEncodingKind::Vex
                    || prefix.width != VecWidth::V128
                    || prefix.vvvv != 0))
            || (!unary
                && prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) =
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let op = match opcode {
            0xDB => X86AesOp::InvMixColumns,
            0xDC => X86AesOp::Enc,
            0xDD => X86AesOp::EncLast,
            0xDE => X86AesOp::Dec,
            0xDF => X86AesOp::DecLast,
            _ => unreachable!(),
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Aes {
                dst,
                src1: if unary {
                    src2
                } else {
                    self.vec_reg(
                        prefix.vvvv
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                                16
                            } else {
                                0
                            },
                        prefix.width,
                    )
                },
                src2: (!unary).then_some(src2),
                width: prefix.width,
                op,
                imm: 0,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_aes_keygen(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.width != VecWidth::V128
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86Aes {
                dst: self.xmm(modrm.reg),
                src1: src,
                src2: None,
                width: VecWidth::V128,
                op: X86AesOp::KeygenAssist,
                imm: bytes[imm_offset],
            },
        ));
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn packed_shift_count_spec(
        opcode: u8,
        evex: bool,
        w: bool,
    ) -> Option<(VecElementType, ShiftOp, &'static str)> {
        match opcode {
            0xD1 => Some((VecElementType::I16, ShiftOp::Lsr, "PSRLW")),
            0xD2 if !evex || !w => Some((VecElementType::I32, ShiftOp::Lsr, "PSRLD")),
            0xD3 if !evex || w => Some((VecElementType::I64, ShiftOp::Lsr, "PSRLQ")),
            0xE1 => Some((VecElementType::I16, ShiftOp::Asr, "PSRAW")),
            0xE2 if evex && w => Some((VecElementType::I64, ShiftOp::Asr, "PSRAQ")),
            0xE2 => Some((VecElementType::I32, ShiftOp::Asr, "PSRAD")),
            0xF1 => Some((VecElementType::I16, ShiftOp::Lsl, "PSLLW")),
            0xF2 if !evex || !w => Some((VecElementType::I32, ShiftOp::Lsl, "PSLLD")),
            0xF3 if !evex || w => Some((VecElementType::I64, ShiftOp::Lsl, "PSLLQ")),
            _ => None,
        }
    }


    pub(crate) fn lift_vec_packed_shift_count(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let evex = prefix.encoding == VecEncodingKind::Evex;
        let Some((elem, shift, _)) = Self::packed_shift_count_spec(opcode, evex, prefix.w) else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let count_vec = if modrm.is_memory {
            let (addr, pre_ops) = if evex {
                self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 16, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm + if evex && prefix.rm_high { 16 } else { 0 })
        };
        let count = if modrm.is_memory {
            let count = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: count,
                    vec: count_vec,
                    lane: 0,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            count
        } else {
            count_vec
        };
        let src = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg + if evex && prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = if evex && prefix.aaa != 0 {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedShift {
                dst: raw,
                src,
                count,
                width: prefix.width,
                elem,
                shift,
            },
        ));
        if evex && prefix.aaa != 0 {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_shift_variable(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let evex = prefix.encoding == VecEncodingKind::Evex;
        if prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, shift) = match (opcode, prefix.w, evex) {
            (0x10, true, true) => (VecElementType::I16, ShiftOp::Lsr),
            (0x11, true, true) => (VecElementType::I16, ShiftOp::Asr),
            (0x12, true, true) => (VecElementType::I16, ShiftOp::Lsl),
            (0x45, false, _) => (VecElementType::I32, ShiftOp::Lsr),
            (0x45, true, _) => (VecElementType::I64, ShiftOp::Lsr),
            (0x46, false, _) => (VecElementType::I32, ShiftOp::Asr),
            (0x46, true, true) => (VecElementType::I64, ShiftOp::Asr),
            (0x47, false, _) => (VecElementType::I32, ShiftOp::Lsl),
            (0x47, true, _) => (VecElementType::I64, ShiftOp::Lsl),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = evex
            && prefix.b
            && modrm.is_memory
            && matches!(elem, VecElementType::I32 | VecElementType::I64);
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let count = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    elem.bytes()
                } else {
                    prefix.width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            if evex && prefix.aaa != 0 {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    broadcast,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm + if evex && prefix.rm_high { 16 } else { 0 },
                prefix.width,
            )
        };
        let src = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg + if evex && prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedShiftVariable {
                dst,
                src,
                count,
                mask: (evex && prefix.aaa != 0)
                    .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                width: prefix.width,
                elem,
                shift,
                zeroing: evex && prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_shift_imm(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let evex = prefix.encoding == VecEncodingKind::Evex;
        if prefix.pp != X86SsePrefix::OpSize
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (evex && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let group = (modrm.byte >> 3) & 7;
        let (elem, shift, byte_lane, rotate_left) = match (opcode, group, evex && prefix.w) {
            (0x71, 2, _) => (VecElementType::I16, ShiftOp::Lsr, false, None),
            (0x71, 4, _) => (VecElementType::I16, ShiftOp::Asr, false, None),
            (0x71, 6, _) => (VecElementType::I16, ShiftOp::Lsl, false, None),
            (0x72, 0, w) if evex => (
                if w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                ShiftOp::Lsr,
                false,
                Some(false),
            ),
            (0x72, 1, w) if evex => (
                if w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                ShiftOp::Lsl,
                false,
                Some(true),
            ),
            (0x72, 2, false) => (VecElementType::I32, ShiftOp::Lsr, false, None),
            (0x72, 4, false) => (VecElementType::I32, ShiftOp::Asr, false, None),
            (0x72, 4, true) if evex => (VecElementType::I64, ShiftOp::Asr, false, None),
            (0x72, 6, false) => (VecElementType::I32, ShiftOp::Lsl, false, None),
            (0x73, 2, true) if evex => (VecElementType::I64, ShiftOp::Lsr, false, None),
            (0x73, 2, false) if !evex => (VecElementType::I64, ShiftOp::Lsr, false, None),
            (0x73, 3, _) => (VecElementType::I8, ShiftOp::Lsr, true, None),
            (0x73, 6, true) if evex => (VecElementType::I64, ShiftOp::Lsl, false, None),
            (0x73, 6, false) if !evex => (VecElementType::I64, ShiftOp::Lsl, false, None),
            (0x73, 7, _) => (VecElementType::I8, ShiftOp::Lsl, true, None),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if evex
            && ((byte_lane && (prefix.aaa != 0 || prefix.zeroing || prefix.b))
                || (!byte_lane && prefix.zeroing && prefix.aaa == 0)
                || (prefix.b && !modrm.is_memory)
                || (elem == VecElementType::I16 && prefix.b)
                || (rotate_left.is_some() && prefix.reg_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let lanes = prefix.width.lanes(elem) as u8;
        let mask = if evex && !byte_lane && prefix.aaa != 0 {
            Some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))))
        } else {
            None
        };
        let src = if modrm.is_memory {
            if !evex {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let broadcast = prefix.b;
            let e4nf = byte_lane || elem == VecElementType::I16;
            let tuple_bytes = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                tuple_bytes,
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            if e4nf {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: prefix.width,
                    },
                ));
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: scalar,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                let mem_width = if elem == VecElementType::I32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                };
                if let Some(mask_reg) = mask {
                    let active = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: active,
                            src1: mask_reg,
                            src2: SrcOperand::Imm((1i64 << lanes) - 1),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond: active,
                            addr,
                            width: mem_width,
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar,
                            addr,
                            width: mem_width,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: value,
                        scalar,
                        elem,
                        lanes,
                    },
                ));
            } else if let Some(mask_reg) = mask {
                let zero = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: zero,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: value,
                        scalar: zero,
                        elem,
                        lanes,
                    },
                ));
                let base = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Lea { dst: base, addr },
                ));
                for lane in 0..lanes {
                    let shifted = ctx.alloc_vreg();
                    let active = ctx.alloc_vreg();
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Shr {
                            dst: shifted,
                            src: mask_reg,
                            amount: SrcOperand::Imm(i64::from(lane)),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: active,
                            src1: shifted,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond: active,
                            addr: Address::base_off(
                                base,
                                i64::from(lane) * i64::from(elem.bytes()),
                            ),
                            width: if elem == VecElementType::I32 {
                                MemWidth::B4
                            } else {
                                MemWidth::B8
                            },
                            signed: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst: value,
                            vec: value,
                            scalar,
                            lane,
                            elem,
                        },
                    ));
                }
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: prefix.width,
                    },
                ));
            }
            value
        } else {
            self.vec_reg(
                modrm.rm + if evex && prefix.rm_high { 16 } else { 0 },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = if rotate_left.is_some() {
            dst
        } else if evex && !byte_lane && mask.is_some() {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            if let Some(left) = rotate_left {
                OpKind::X86PackedRotate {
                    dst: raw,
                    src,
                    count: None,
                    mask,
                    amount: bytes[imm_offset],
                    width: prefix.width,
                    elem,
                    left,
                    zeroing: prefix.zeroing,
                }
            } else {
                OpKind::X86PackedShiftImm {
                    dst: raw,
                    src,
                    width: prefix.width,
                    elem,
                    shift,
                    amount: bytes[imm_offset],
                    byte_lane,
                }
            },
        ));
        if rotate_left.is_none() && evex && !byte_lane && mask.is_some() {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_packed_shuffle_imm(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let dword = prefix.pp == X86SsePrefix::OpSize;
        if !matches!(
            prefix.encoding,
            VecEncodingKind::Vex | VecEncodingKind::Evex
        ) || !matches!(
            prefix.pp,
            X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
        ) || prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.v_high
                    || prefix.l_bits == 3
                    || (prefix.zeroing && prefix.aaa == 0)
                    || (dword && prefix.w)
                    || (!dword && prefix.b)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast =
            prefix.encoding == VecEncodingKind::Evex && dword && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast { 4 } else { prefix.width.bytes() },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if broadcast {
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar,
                        elem: VecElementType::I32,
                        lanes: prefix.width.lanes(VecElementType::I32) as u8,
                    },
                ));
                vector
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let (elem, high_words) = match prefix.pp {
            X86SsePrefix::OpSize => (VecElementType::I32, None),
            X86SsePrefix::Rep => (VecElementType::I16, Some(true)),
            X86SsePrefix::Repne => (VecElementType::I16, Some(false)),
            X86SsePrefix::None => unreachable!(),
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        self.append_packed_shuffle_imm(
            raw,
            src,
            prefix.width,
            elem,
            bytes[imm_offset],
            high_words,
            pc,
            ctx,
            &mut ops,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_duplicate_move(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let (elem, high) = match (opcode, prefix.pp) {
            (0x12, X86SsePrefix::Rep) => (VecElementType::F32, false),
            (0x16, X86SsePrefix::Rep) => (VecElementType::F32, true),
            (0x12, X86SsePrefix::Repne) => (VecElementType::F64, false),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.v_high
                    || prefix.l_bits == 3
                    || prefix.b
                    || (prefix.zeroing && prefix.aaa == 0)
                    || (elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let scalar_dd = elem == VecElementType::F64 && prefix.width == VecWidth::V128;
        let src = if modrm.is_memory && scalar_dd {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 8, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            let vector = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: scalar,
                    addr,
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem,
                    lanes: 2,
                },
            ));
            vector
        } else if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    prefix.width.bytes(),
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        self.append_duplicate_shuffle(raw, src, prefix.width, elem, high, pc, ctx, &mut ops);
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_two_source_shuffle_imm(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match prefix.pp {
            X86SsePrefix::None => VecElementType::F32,
            X86SsePrefix::OpSize => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if (prefix.encoding == VecEncodingKind::Vex
            && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.l_bits == 3
                    || (prefix.zeroing && prefix.aaa == 0)
                    || (elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast {
                        elem.bytes()
                    } else {
                        prefix.width.bytes()
                    },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if broadcast {
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::F32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                vector
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        self.append_two_source_shuffle_imm(
            raw,
            src1,
            src2,
            prefix.width,
            elem,
            bytes[imm_offset],
            pc,
            ctx,
            &mut ops,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_gfni(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let multiply = prefix.map == X86VecMap::Map0F38 && opcode == 0xCF;
        let affine = prefix.map == X86VecMap::Map0F3A && matches!(opcode, 0xCE | 0xCF);
        let evex = prefix.encoding == VecEncodingKind::Evex;
        if !matches!(
            prefix.encoding,
            VecEncodingKind::Vex | VecEncodingKind::Evex
        ) || prefix.pp != X86SsePrefix::OpSize
            || (!multiply && !affine)
            || prefix.w != affine
            || (evex && prefix.l_bits == 3)
            || (evex && prefix.zeroing && prefix.aaa == 0)
            || (evex && multiply && prefix.b)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if evex && prefix.b && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if affine && bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let bytes_consumed = imm_offset + usize::from(affine);
        let next_pc = pc + bytes_consumed as u64;
        let elem = VecElementType::I8;
        let mask =
            (evex && prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let broadcast = evex && affine && prefix.b;
            let (addr, pre_ops) = if evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast { 8 } else { prefix.width.bytes() },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if evex && multiply && mask.is_some() {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    false,
                    mask.unwrap(),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                self.append_broadcast_memory_source(
                    addr,
                    VecElementType::I64,
                    prefix.width,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let src1 = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = if multiply {
            self.append_gf2p8_mul_vector(src1, src2, prefix.width, pc, ctx, &mut ops)
        } else {
            self.append_gf2p8_affine_vector(
                src1,
                src2,
                prefix.width,
                bytes[imm_offset],
                opcode == 0xCF,
                pc,
                ctx,
                &mut ops,
            )
        };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        if evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: raw,
                    width: prefix.width,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }


    pub(crate) fn lift_vec_extract_0f3a(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, lane_mask, mem_width, op_width) = match opcode {
            0x14 => (VecElementType::I8, 0x0F, MemWidth::B1, OpWidth::W32),
            0x15 => (VecElementType::I16, 0x07, MemWidth::B2, OpWidth::W32),
            0x16 if prefix.w => (VecElementType::I64, 0x01, MemWidth::B8, OpWidth::W64),
            0x16 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            0x17 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            _ => unreachable!(),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.encoding == VecEncodingKind::Evex && !modrm.is_memory && prefix.rm_high {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let addr = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                mem_width.bytes(),
                ctx,
            );
            ops.extend(pre_ops);
            Some(addr)
        } else {
            None
        };
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: scalar,
                vec: self.xmm(
                    modrm.reg
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                            16
                        } else {
                            0
                        },
                ),
                lane: bytes[imm_offset] & lane_mask,
                elem,
                sign: SignExtend::Zero,
            },
        ));
        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: scalar,
                    addr,
                    width: mem_width,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.rm),
                    src: SrcOperand::Reg(scalar),
                    width: op_width,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_pinsrw_pextrw(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (opcode == 0xC5 && (prefix.vvvv != 0 || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if (opcode == 0xC5 && modrm.is_memory)
            || (prefix.encoding == VecEncodingKind::Evex
                && ((opcode == 0xC5 && prefix.reg_high)
                    || (opcode == 0xC4 && !modrm.is_memory && prefix.rm_high)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let lane = bytes[imm_offset] & 0x07;
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();

        if opcode == 0xC5 {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: self.xmm(
                        modrm.rm
                            + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                16
                            } else {
                                0
                            },
                    ),
                    lane,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.reg),
                    src: SrcOperand::Reg(scalar),
                    width: OpWidth::W32,
                },
            ));
        } else {
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    2,
                    ctx,
                );
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B2,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                self.gpr(modrm.rm)
            };
            self.append_insert_scalar_lane(
                self.xmm(
                    modrm.reg
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                            16
                        } else {
                            0
                        },
                ),
                self.xmm(
                    prefix.vvvv
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                            16
                        } else {
                            0
                        },
                ),
                scalar,
                VecElementType::I16,
                lane,
                pc,
                ctx,
                &mut ops,
            );
        }

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_insert_0f3a(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && opcode == 0x21 && prefix.w)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.encoding == VecEncodingKind::Evex
            && !modrm.is_memory
            && prefix.rm_high
            && opcode != 0x21
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let dst = self.xmm(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
        );
        let merge = self.xmm(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
        );

        if opcode == 0x21 {
            let inserted = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    4,
                    ctx,
                );
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: self.xmm(
                            modrm.rm
                                + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                    16
                                } else {
                                    0
                                },
                        ),
                        lane: (imm >> 6) & 0x03,
                        elem: VecElementType::I32,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            };
            self.append_insertps(
                dst,
                merge,
                inserted,
                (imm >> 4) & 0x03,
                imm & 0x0F,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            let (elem, lane_mask, mem_width) = match opcode {
                0x20 => (VecElementType::I8, 0x0F, MemWidth::B1),
                0x22 if prefix.w => (VecElementType::I64, 0x01, MemWidth::B8),
                0x22 => (VecElementType::I32, 0x03, MemWidth::B4),
                _ => unreachable!(),
            };
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    mem_width.bytes(),
                    ctx,
                );
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                self.gpr(modrm.rm)
            };
            self.append_insert_scalar_lane(
                dst,
                merge,
                scalar,
                elem,
                imm & lane_mask,
                pc,
                ctx,
                &mut ops,
            );
        }

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }


    pub(crate) fn lift_vec_pmuldq(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && !prefix.w)
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast { 8 } else { prefix.width.bytes() },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I64,
                    prefix.width,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B8,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar,
                        elem: VecElementType::I64,
                        lanes: prefix.width.lanes(VecElementType::I64) as u8,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_pmuldq(raw, src1, src2, prefix.width, signed, pc, ctx, &mut ops);
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I64,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            self.append_pmuldq(dst, src1, src2, prefix.width, signed, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_pmul_low(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0xD5 => VecElementType::I16,
            0x40 if prefix.encoding == VecEncodingKind::Evex && prefix.w => VecElementType::I64,
            0x40 => VecElementType::I32,
            _ => unreachable!(),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = opcode == 0x40
            && prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast {
                        elem.bytes()
                    } else {
                        prefix.width.bytes()
                    },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::I32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let masked = prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0;
        let raw = if masked { ctx.alloc_vreg() } else { dst };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMul {
                dst: raw,
                src1,
                src2,
                elem,
                lanes: prefix.width.lanes(elem) as u8,
            },
            self.vec_hint(prefix, opcode),
        ));
        if masked {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_pmul_high_word(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    VecElementType::I16,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && mask.is_none() {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(dst, src1, src2, prefix.width, opcode == 0xE5),
                self.vec_hint(prefix, opcode),
            ));
        } else {
            let raw = if prefix.encoding == VecEncodingKind::Evex {
                ctx.alloc_vreg()
            } else {
                dst
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmul_high_word_kind(raw, src1, src2, prefix.width, opcode == 0xE5),
            ));
            if prefix.encoding == VecEncodingKind::Evex {
                self.append_evex_vector_mask_result(
                    prefix,
                    dst,
                    raw,
                    VecElementType::I16,
                    pc,
                    ctx,
                    &mut ops,
                );
            }
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_average(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if opcode == 0xE0 {
            VecElementType::I8
        } else {
            VecElementType::I16
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && (prefix.encoding == VecEncodingKind::Vex || prefix.aaa == 0) {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(dst, src1, src2, prefix.width, elem),
                self.vec_hint(prefix, opcode),
            ));
        } else if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(raw, src1, src2, prefix.width, elem),
            ));
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(dst, src1, src2, prefix.width, elem),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_pmaddwd(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && (prefix.encoding == VecEncodingKind::Vex || prefix.aaa == 0) {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(dst, src1, src2, prefix.width),
                self.vec_hint(prefix, 0xF5),
            ));
        } else if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(raw, src1, src2, prefix.width),
            ));
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I32,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::pmaddwd_kind(dst, src1, src2, prefix.width),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_movntdqa(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || prefix.l_bits == 3
            || prefix.b
            || prefix.aaa != 0
            || prefix.zeroing
            || (prefix.encoding == VecEncodingKind::Evex && (prefix.w || prefix.v_high))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = if prefix.encoding == VecEncodingKind::Evex {
            self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
        } else {
            self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86CheckAlignment {
                addr: addr.clone(),
                alignment: prefix.width.bytes() as u8,
            },
        ));
        let loaded = ctx.alloc_vreg();
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VLoad {
                dst: loaded,
                addr,
                width: prefix.width,
            },
            X86OpHint::VecAlign(X86VecAlign::Aligned),
        ));
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: loaded,
                width: prefix.width,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_palignr(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + imm_offset as u64 + 1;
        let lanes = prefix.width.lanes(VecElementType::I8) as u8;
        let mut ops = Vec::new();
        let low = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            if prefix.encoding != VecEncodingKind::Evex || prefix.aaa == 0 {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
            } else {
                let zero = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: zero,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar: zero,
                        elem: VecElementType::I8,
                        lanes,
                    },
                ));
                if imm < 16 {
                    let base = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Lea { dst: base, addr },
                    ));
                    let mask = VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)));
                    for output_lane in 0..lanes {
                        let selected = u16::from(imm) + u16::from(output_lane % 16);
                        if selected >= 16 {
                            continue;
                        }
                        let source_lane = output_lane / 16 * 16 + selected as u8;
                        let shifted = ctx.alloc_vreg();
                        let active = ctx.alloc_vreg();
                        let scalar = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Shr {
                                dst: shifted,
                                src: mask,
                                amount: SrcOperand::Imm(i64::from(output_lane)),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::And {
                                dst: active,
                                src1: shifted,
                                src2: SrcOperand::Imm(1),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Mov {
                                dst: scalar,
                                src: SrcOperand::Imm(0),
                                width: OpWidth::W64,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::PredLoad {
                                dst: scalar,
                                cond: active,
                                addr: Address::base_off(base, i64::from(source_lane)),
                                width: MemWidth::B1,
                                signed: SignExtend::Zero,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::VInsertLane {
                                dst: loaded,
                                vec: loaded,
                                scalar,
                                lane: source_lane,
                                elem: VecElementType::I8,
                            },
                        ));
                    }
                }
            }
            loaded
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let high = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_align_right(raw, high, low, prefix.width, imm, pc, ctx, &mut ops);
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I8,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            self.append_align_right(dst, high, low, prefix.width, imm, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(
            ops,
            cursor + modrm.bytes_consumed + 1,
        ))
    }


    /// Lift VEX/EVEX load-and-broadcast instructions from vector, memory, or
    /// (for EVEX opcodes 7A..7C) GPR sources.  Tuple memory forms use one
    /// predicate shared by every tuple-element load: Type E6 suppresses the
    /// complete source access only when every architecturally relevant mask
    /// bit is zero.
    pub(crate) fn lift_vec_load_broadcast(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let (elem, source_lanes, memory_only, gpr_source, valid_width) =
            match (prefix.encoding, opcode, prefix.w) {
                (VecEncodingKind::Vex, 0x18, false) => (
                    VecElementType::F32,
                    1,
                    false,
                    false,
                    matches!(prefix.width, VecWidth::V128 | VecWidth::V256),
                ),
                (VecEncodingKind::Vex, 0x19, false) => (
                    VecElementType::F64,
                    1,
                    false,
                    false,
                    prefix.width == VecWidth::V256,
                ),
                (VecEncodingKind::Vex, 0x1A, false) => (
                    VecElementType::F32,
                    4,
                    true,
                    false,
                    prefix.width == VecWidth::V256,
                ),
                (VecEncodingKind::Vex, 0x58, false) => (
                    VecElementType::I32,
                    1,
                    false,
                    false,
                    matches!(prefix.width, VecWidth::V128 | VecWidth::V256),
                ),
                (VecEncodingKind::Vex, 0x59, false) => (
                    VecElementType::I64,
                    1,
                    false,
                    false,
                    matches!(prefix.width, VecWidth::V128 | VecWidth::V256),
                ),
                (VecEncodingKind::Vex, 0x5A, false) => (
                    VecElementType::I32,
                    4,
                    true,
                    false,
                    prefix.width == VecWidth::V256,
                ),
                (VecEncodingKind::Vex, 0x78, false) => (
                    VecElementType::I8,
                    1,
                    false,
                    false,
                    matches!(prefix.width, VecWidth::V128 | VecWidth::V256),
                ),
                (VecEncodingKind::Vex, 0x79, false) => (
                    VecElementType::I16,
                    1,
                    false,
                    false,
                    matches!(prefix.width, VecWidth::V128 | VecWidth::V256),
                ),

                (VecEncodingKind::Evex, 0x18, false) => {
                    (VecElementType::F32, 1, false, false, true)
                }
                (VecEncodingKind::Evex, 0x19, false) => (
                    VecElementType::F32,
                    2,
                    false,
                    false,
                    matches!(prefix.width, VecWidth::V256 | VecWidth::V512),
                ),
                (VecEncodingKind::Evex, 0x19, true) => (
                    VecElementType::F64,
                    1,
                    false,
                    false,
                    matches!(prefix.width, VecWidth::V256 | VecWidth::V512),
                ),
                (VecEncodingKind::Evex, 0x1A, false) => (
                    VecElementType::F32,
                    4,
                    true,
                    false,
                    matches!(prefix.width, VecWidth::V256 | VecWidth::V512),
                ),
                (VecEncodingKind::Evex, 0x1A, true) => (
                    VecElementType::F64,
                    2,
                    true,
                    false,
                    matches!(prefix.width, VecWidth::V256 | VecWidth::V512),
                ),
                (VecEncodingKind::Evex, 0x1B, false) => (
                    VecElementType::F32,
                    8,
                    true,
                    false,
                    prefix.width == VecWidth::V512,
                ),
                (VecEncodingKind::Evex, 0x1B, true) => (
                    VecElementType::F64,
                    4,
                    true,
                    false,
                    prefix.width == VecWidth::V512,
                ),
                (VecEncodingKind::Evex, 0x58, false) => {
                    (VecElementType::I32, 1, false, false, true)
                }
                (VecEncodingKind::Evex, 0x59, false) => {
                    (VecElementType::I32, 2, false, false, true)
                }
                (VecEncodingKind::Evex, 0x59, true) => (VecElementType::I64, 1, false, false, true),
                (VecEncodingKind::Evex, 0x5A, false) => (
                    VecElementType::I32,
                    4,
                    true,
                    false,
                    matches!(prefix.width, VecWidth::V256 | VecWidth::V512),
                ),
                (VecEncodingKind::Evex, 0x5A, true) => (
                    VecElementType::I64,
                    2,
                    true,
                    false,
                    matches!(prefix.width, VecWidth::V256 | VecWidth::V512),
                ),
                (VecEncodingKind::Evex, 0x5B, false) => (
                    VecElementType::I32,
                    8,
                    true,
                    false,
                    prefix.width == VecWidth::V512,
                ),
                (VecEncodingKind::Evex, 0x5B, true) => (
                    VecElementType::I64,
                    4,
                    true,
                    false,
                    prefix.width == VecWidth::V512,
                ),
                (VecEncodingKind::Evex, 0x78, false) => (VecElementType::I8, 1, false, false, true),
                (VecEncodingKind::Evex, 0x79, false) => {
                    (VecElementType::I16, 1, false, false, true)
                }
                (VecEncodingKind::Evex, 0x7A, false) => (VecElementType::I8, 1, false, true, true),
                (VecEncodingKind::Evex, 0x7B, false) => (VecElementType::I16, 1, false, true, true),
                (VecEncodingKind::Evex, 0x7C, false) => (VecElementType::I32, 1, false, true, true),
                (VecEncodingKind::Evex, 0x7C, true) => (VecElementType::I64, 1, false, true, true),
                _ => {
                    return Err(LiftError::InvalidEncoding {
                        addr: pc,
                        bytes: bytes.to_vec(),
                    });
                }
            };
        if !valid_width {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if (memory_only && !modrm.is_memory)
            || (gpr_source && modrm.is_memory)
            || (gpr_source && prefix.rm_high)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let destination_lanes = prefix.width.lanes(elem) as u8;
        let mut ops = Vec::new();

        let memory_condition =
            if prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0 && modrm.is_memory {
                let cond = ctx.alloc_vreg();
                let lane_mask = if destination_lanes == 64 {
                    u64::MAX
                } else {
                    (1u64 << destination_lanes) - 1
                };
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: cond,
                        src1: VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                        src2: SrcOperand::Imm(lane_mask as i64),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                Some(cond)
            } else {
                None
            };

        let source = if gpr_source {
            self.gpr(modrm.rm)
        } else if modrm.is_memory {
            let tuple_bytes = u32::from(source_lanes) * elem.bytes();
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                tuple_bytes,
                ctx,
            );
            ops.extend(pre_ops);
            let base = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Lea { dst: base, addr },
            ));
            let vector = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
            let mem_width = match elem.bytes() {
                1 => MemWidth::B1,
                2 => MemWidth::B2,
                4 => MemWidth::B4,
                8 => MemWidth::B8,
                _ => unreachable!(),
            };
            for lane in 0..source_lanes {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: scalar,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                let lane_addr = Address::base_off(base, i64::from(lane) * i64::from(elem.bytes()));
                if let Some(cond) = memory_condition {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond,
                            addr: lane_addr,
                            width: mem_width,
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar,
                            addr: lane_addr,
                            width: mem_width,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: vector,
                        vec: vector,
                        scalar,
                        lane,
                        elem,
                    },
                ));
            }
            vector
        } else {
            self.xmm(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
            )
        };

        let raw = ctx.alloc_vreg();
        if source_lanes == 1 {
            let scalar = if gpr_source {
                source
            } else {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: 0,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            };
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: raw,
                    scalar,
                    elem,
                    lanes: destination_lanes,
                },
            ));
        } else {
            let zeroed = self.append_zero_vector(prefix.width, elem, pc, ctx, &mut ops);
            for lane in 0..destination_lanes {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: source,
                        lane: lane % source_lanes,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: raw,
                        vec: if lane == 0 { zeroed } else { raw },
                        scalar,
                        lane,
                        elem,
                    },
                ));
            }
        }

        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VMov {
                    dst,
                    src: raw,
                    width: prefix.width,
                },
            ));
        }

        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_scalar_fp_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        from: VecElementType,
        to: VecElementType,
    ) -> Result<LiftResult, LiftError> {
        let expected_w = matches!((from, to), (VecElementType::F64, _));
        if !from.is_float()
            || !to.is_float()
            || from == to
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(
                    (from, to),
                    (VecElementType::F32, VecElementType::F64)
                        | (VecElementType::F64, VecElementType::F32)
                ))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.w != expected_w || (prefix.zeroing && prefix.aaa == 0)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let opcode = bytes[prefix.bytes];
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let mut ops = Vec::new();
        let mask_condition = self.append_evex_mask_condition(prefix, pc, ctx, &mut ops);
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_scalar_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                from,
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            let width = match from {
                VecElementType::F16 => MemWidth::B2,
                VecElementType::F32 => MemWidth::B4,
                VecElementType::F64 => MemWidth::B8,
                _ => unreachable!(),
            };
            if let Some(cond) = mask_condition {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: value,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: value,
                        cond,
                        addr,
                        width,
                        signed: SignExtend::Zero,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: value,
                        addr,
                        width,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            value
        } else {
            self.xmm(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
            )
        };
        let dst = self.xmm(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
        );
        let merge = self.xmm(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
        );
        let embedded_rounding =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && from.bytes() > to.bytes();
        let round = if embedded_rounding {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86FpConvert {
                dst,
                merge,
                src,
                mask,
                from,
                to,
                mask_zeroing: prefix.zeroing,
                round,
                suppress_exceptions: prefix.encoding == VecEncodingKind::Evex && prefix.b,
                zero_upper: true,
            },
            self.vec_hint(prefix, opcode),
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_fp16_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        from: VecElementType,
        to: VecElementType,
    ) -> Result<LiftResult, LiftError> {
        let expected_w = from == VecElementType::F64;
        if !matches!(
            from,
            VecElementType::F16 | VecElementType::F32 | VecElementType::F64
        ) || !matches!(
            to,
            VecElementType::F16 | VecElementType::F32 | VecElementType::F64
        ) || from == to
            || (prefix.encoding == VecEncodingKind::Vex
                && (from != VecElementType::F16 || to != VecElementType::F32 || prefix.w))
            || prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.v_high || prefix.w != expected_w || (prefix.zeroing && prefix.aaa == 0)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let opcode = bytes[prefix.bytes];
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let register_sae_or_er =
            prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        if prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && modrm.is_memory
            && prefix.map == X86VecMap::Map0F38
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if prefix.encoding == VecEncodingKind::Evex && !register_sae_or_er && prefix.l_bits == 3 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        if register_sae_or_er && from.bytes() < to.bytes() && prefix.l_bits != 2 {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let instruction_width = if register_sae_or_er {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = (instruction_width.bytes() / from.bytes().max(to.bytes())) as u8;
        let src_bytes = u32::from(lanes) * from.bytes();
        let dst_bytes = u32::from(lanes) * to.bytes();
        let container_width = |bytes: u32| match bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let src_width = container_width(src_bytes);
        let dst_width = container_width(dst_bytes);
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast { from.bytes() } else { src_bytes },
                ctx,
            );
            ops.extend(pre_ops);
            let value = ctx.alloc_vreg();
            let zero = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: zero,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: value,
                    scalar: zero,
                    elem: from,
                    lanes,
                },
            ));
            let mem_width = match from {
                VecElementType::F16 => MemWidth::B2,
                VecElementType::F32 => MemWidth::B4,
                VecElementType::F64 => MemWidth::B8,
                _ => unreachable!(),
            };
            if broadcast {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: scalar,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                if let Some(mask_reg) = mask {
                    let cond = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: cond,
                            src1: mask_reg,
                            src2: SrcOperand::Imm(((1u64 << lanes) - 1) as i64),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond,
                            addr,
                            width: mem_width,
                            signed: SignExtend::Zero,
                        },
                    ));
                } else {
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Load {
                            dst: scalar,
                            addr,
                            width: mem_width,
                            sign: SignExtend::Zero,
                        },
                    ));
                }
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: value,
                        scalar,
                        elem: from,
                        lanes,
                    },
                ));
            } else if mask.is_some() || src_bytes < 8 {
                let base = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Lea { dst: base, addr },
                ));
                for lane in 0..lanes {
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        },
                    ));
                    if let Some(mask_reg) = mask {
                        let shifted = ctx.alloc_vreg();
                        let cond = ctx.alloc_vreg();
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Shr {
                                dst: shifted,
                                src: mask_reg,
                                amount: SrcOperand::Imm(i64::from(lane)),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::And {
                                dst: cond,
                                src1: shifted,
                                src2: SrcOperand::Imm(1),
                                width: OpWidth::W64,
                                flags: FlagUpdate::None,
                            },
                        ));
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::PredLoad {
                                dst: scalar,
                                cond,
                                addr: Address::base_off(
                                    base,
                                    i64::from(lane) * i64::from(from.bytes()),
                                ),
                                width: mem_width,
                                signed: SignExtend::Zero,
                            },
                        ));
                    } else {
                        ops.push(SmirOp::new(
                            OpId(ops.len() as u16),
                            pc,
                            OpKind::Load {
                                dst: scalar,
                                addr: Address::base_off(
                                    base,
                                    i64::from(lane) * i64::from(from.bytes()),
                                ),
                                width: mem_width,
                                sign: SignExtend::Zero,
                            },
                        ));
                    }
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst: value,
                            vec: value,
                            scalar,
                            lane,
                            elem: from,
                        },
                    ));
                }
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: src_width,
                    },
                ));
            }
            value
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                src_width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            dst_width,
        );
        let round = if register_sae_or_er && from.bytes() > to.bytes() {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        let hint = if register_sae_or_er {
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: instruction_width,
                w: prefix.w,
            }
        } else {
            self.vec_hint(prefix, opcode)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedFpConvert {
                dst,
                src,
                mask,
                from,
                to,
                lanes,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: register_sae_or_er,
                report_fp16_denormal: from == VecElementType::F16
                    && (to == VecElementType::F64 || broadcast),
            },
            hint,
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_int_fp_convert(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let opcode = bytes[prefix.bytes];
        let conversion = if prefix.encoding == VecEncodingKind::Vex {
            match (opcode, prefix.pp) {
                (0x5B, X86SsePrefix::None) => {
                    Some((true, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::OpSize) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::Rep) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, true))
                }
                (0xE6, X86SsePrefix::Rep) => {
                    Some((true, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::Repne) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::OpSize) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, true))
                }
                _ => None,
            }
        } else {
            match (opcode, prefix.pp, prefix.w) {
                (0x5B, X86SsePrefix::None, false) => {
                    Some((true, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::None, true) => {
                    Some((true, VecElementType::I64, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, false))
                }
                (0x5B, X86SsePrefix::Rep, false) => {
                    Some((false, VecElementType::I32, VecElementType::F32, true, true))
                }
                (0xE6, X86SsePrefix::Rep, false) => {
                    Some((true, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::Rep, true) => {
                    Some((true, VecElementType::I64, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::Repne, true) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, false))
                }
                (0xE6, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I32, VecElementType::F64, true, true))
                }
                (0x7A, X86SsePrefix::Rep, false) => {
                    Some((true, VecElementType::I32, VecElementType::F64, false, false))
                }
                (0x7A, X86SsePrefix::Rep, true) => {
                    Some((true, VecElementType::I64, VecElementType::F64, false, false))
                }
                (0x7A, X86SsePrefix::Repne, false) => {
                    Some((true, VecElementType::I32, VecElementType::F32, false, false))
                }
                (0x7A, X86SsePrefix::Repne, true) => {
                    Some((true, VecElementType::I64, VecElementType::F32, false, false))
                }
                (0x7B, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I64, VecElementType::F32, true, false))
                }
                (0x7B, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I64, VecElementType::F64, true, false))
                }
                (0x7A, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I64, VecElementType::F32, true, true))
                }
                (0x7A, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I64, VecElementType::F64, true, true))
                }
                (0x79, X86SsePrefix::None, false) => Some((
                    false,
                    VecElementType::I32,
                    VecElementType::F32,
                    false,
                    false,
                )),
                (0x79, X86SsePrefix::None, true) => Some((
                    false,
                    VecElementType::I32,
                    VecElementType::F64,
                    false,
                    false,
                )),
                (0x78, X86SsePrefix::None, false) => {
                    Some((false, VecElementType::I32, VecElementType::F32, false, true))
                }
                (0x78, X86SsePrefix::None, true) => {
                    Some((false, VecElementType::I32, VecElementType::F64, false, true))
                }
                (0x79, X86SsePrefix::OpSize, false) => Some((
                    false,
                    VecElementType::I64,
                    VecElementType::F32,
                    false,
                    false,
                )),
                (0x79, X86SsePrefix::OpSize, true) => Some((
                    false,
                    VecElementType::I64,
                    VecElementType::F64,
                    false,
                    false,
                )),
                (0x78, X86SsePrefix::OpSize, false) => {
                    Some((false, VecElementType::I64, VecElementType::F32, false, true))
                }
                (0x78, X86SsePrefix::OpSize, true) => {
                    Some((false, VecElementType::I64, VecElementType::F64, false, true))
                }
                _ => None,
            }
        };
        let Some((int_to_fp, int_elem, fp_elem, signed, truncate)) = conversion else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        if prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex && prefix.v_high)
            || (prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: matches!(prefix.pp, X86SsePrefix::OpSize),
            rep_prefix: match prefix.pp {
                X86SsePrefix::Rep => Some(0xF3),
                X86SsePrefix::Repne => Some(0xF2),
                _ => None,
            },
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let can_embed_rounding =
            !int_to_fp || !(int_elem == VecElementType::I32 && fp_elem == VecElementType::F64);
        let embedded_control = prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && !modrm.is_memory
            && can_embed_rounding;
        if (prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && !modrm.is_memory
            && !can_embed_rounding)
            || (!embedded_control && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let operation_width = if embedded_control {
            VecWidth::V512
        } else {
            prefix.width
        };
        let src_elem = if int_to_fp { int_elem } else { fp_elem };
        let dst_elem = if int_to_fp { fp_elem } else { int_elem };
        let (lanes, src_bytes, dst_bytes) = if dst_elem.bytes() >= src_elem.bytes() {
            let lanes = operation_width.bytes() / dst_elem.bytes();
            (lanes, lanes * src_elem.bytes(), operation_width.bytes())
        } else {
            let lanes = operation_width.bytes() / src_elem.bytes();
            (lanes, operation_width.bytes(), lanes * dst_elem.bytes())
        };
        let exact_width = |bytes: u32| match bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let register_width = |bytes: u32| match bytes {
            0..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => VecWidth::V512,
        };
        let src_width = exact_width(src_bytes);
        let dst_width = register_width(dst_bytes);
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    src_elem.bytes()
                } else {
                    src_bytes
                },
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask_reg) = mask {
                self.append_evex_masked_vector_source(
                    addr, src_elem, src_width, broadcast, mask_reg, pc, ctx, &mut ops,
                )
            } else if broadcast {
                self.append_broadcast_memory_source(addr, src_elem, src_width, pc, ctx, &mut ops)
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: src_width,
                    },
                ));
                value
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                src_width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            dst_width,
        );
        let round = if !int_to_fp && truncate {
            FpRoundMode::RoundTowardZero
        } else if embedded_control {
            match prefix.l_bits {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        } else {
            FpRoundMode::Dynamic
        };
        let hint = if embedded_control {
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: operation_width,
                w: prefix.w,
            }
        } else {
            self.vec_hint(prefix, opcode)
        };
        let kind = if int_to_fp {
            OpKind::X86PackedIntToFp {
                dst,
                src,
                mask,
                int_elem,
                fp_elem,
                signed,
                lanes: lanes as u8,
                src_width,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: embedded_control,
            }
        } else {
            OpKind::X86PackedFpToInt {
                dst,
                src,
                mask,
                fp_elem,
                int_elem,
                signed,
                truncate,
                lanes: lanes as u8,
                src_width,
                dst_width,
                mask_zeroing: prefix.zeroing,
                zero_upper: true,
                round,
                suppress_exceptions: embedded_control,
            }
        };
        ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }


    pub(crate) fn lift_vec_packed_f32_to_f16_store(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.map != X86VecMap::Map0F3A
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.w
            || prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.v_high || (prefix.zeroing && prefix.aaa == 0)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let opcode = bytes[prefix.bytes];
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let register_sae = prefix.encoding == VecEncodingKind::Evex && prefix.b && !modrm.is_memory;
        if (prefix.encoding == VecEncodingKind::Evex
            && ((prefix.b && modrm.is_memory) || (prefix.zeroing && modrm.is_memory)))
            || (!register_sae && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let imm_offset = cursor + modrm.bytes_consumed;
        let imm = *bytes.get(imm_offset).ok_or(LiftError::Incomplete {
            addr: pc,
            have: bytes.len(),
            need: imm_offset + 1,
        })?;
        let instruction_width = if register_sae {
            VecWidth::V512
        } else {
            prefix.width
        };
        let lanes = instruction_width.lanes(VecElementType::F32) as u8;
        let dst_width = match lanes {
            4 => VecWidth::V64,
            8 => VecWidth::V128,
            16 => VecWidth::V256,
            _ => unreachable!(),
        };
        let round = if imm & 4 != 0 {
            FpRoundMode::Dynamic
        } else {
            match imm & 3 {
                0 => FpRoundMode::RoundNearest,
                1 => FpRoundMode::RoundDown,
                2 => FpRoundMode::RoundUp,
                _ => FpRoundMode::RoundTowardZero,
            }
        };
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            instruction_width,
        );
        let next_pc = pc + imm_offset as u64 + 1;
        let hint = if register_sae {
            X86OpHint::EvexOp {
                map: prefix.map,
                pp: prefix.pp,
                opcode,
                width: instruction_width,
                w: false,
            }
        } else {
            self.vec_hint(prefix, opcode)
        };
        let mut ops = Vec::new();
        if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                u32::from(lanes) * 2,
                ctx,
            );
            ops.extend(pre_ops);
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86PackedFpConvertStore {
                    addr,
                    src,
                    mask,
                    lanes,
                    round,
                },
                hint,
            ));
        } else {
            let dst = self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                dst_width,
            );
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86PackedFpConvert {
                    dst,
                    src,
                    mask,
                    from: VecElementType::F32,
                    to: VecElementType::F16,
                    lanes,
                    dst_width,
                    mask_zeroing: prefix.zeroing,
                    zero_upper: true,
                    round,
                    suppress_exceptions: register_sae,
                    report_fp16_denormal: false,
                },
                hint,
            ));
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }
}
